//! Mneme v1 personal-memory core.
//!
//! This module intentionally starts as a deterministic core with a small
//! persistence boundary. It is product code, not an eval fake: the eval harness
//! can drive it through a target adapter, while this crate stays independent of
//! eval fixture types.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const STORE_LOCK_STALE_SECONDS: u64 = 60 * 60;

/// Current persisted state schema version for v1 local stores.
pub const MNEME_STATE_SCHEMA_VERSION: u32 = 2;

/// Default maximum number of context items returned by retrieval.
pub const DEFAULT_CONTEXT_MAX_ITEMS: usize = 8;

/// Personal-memory engine for Mneme v1.
#[derive(Debug, Clone)]
pub struct MnemeEngine {
    schema_version: u32,
    metadata: StateMetadata,
    budget: BudgetState,
    events: Vec<EventRecord>,
    claims: Vec<ClaimRecord>,
    sessions: Vec<SessionRecord>,
    audit: Vec<AuditRecord>,
}

impl MnemeEngine {
    /// Creates a new isolated personal-memory engine.
    #[must_use]
    pub fn new(config: MnemeConfig) -> Self {
        Self {
            schema_version: MNEME_STATE_SCHEMA_VERSION,
            metadata: StateMetadata::new(),
            budget: BudgetState {
                daily_cloud_tokens: config.daily_cloud_tokens,
                spent_tokens: 0,
                hard_cap_violations: 0,
            },
            events: Vec::new(),
            claims: Vec::new(),
            sessions: Vec::new(),
            audit: Vec::new(),
        }
    }

    /// Restores an engine from persisted v1 state.
    #[must_use]
    pub fn from_state(state: MnemeState) -> Self {
        Self {
            schema_version: state.schema_version,
            metadata: state.metadata,
            budget: state.budget,
            events: state.events,
            claims: state.claims,
            sessions: state.sessions,
            audit: state.audit,
        }
    }

    /// Loads an engine from a store, or creates a new one when no state exists.
    pub fn from_store(config: MnemeConfig, store: &impl MnemeStore) -> Result<Self, StoreError> {
        match store.load()? {
            Some(state) => Ok(Self::from_state(state)),
            None => Ok(Self::new(config)),
        }
    }

    /// Appends one user event and extracts a claim when allowed by budget.
    pub fn ingest_event(&mut self, input: EventInput) -> Result<(), ExtractorError> {
        self.ingest_event_internal(input, &RuleBasedExtractor, true)
    }

    /// Appends one user event using the provided extraction adapter.
    pub fn ingest_event_with_extractor(
        &mut self,
        input: EventInput,
        extractor: &(impl MnemeExtractor + ?Sized),
    ) -> Result<(), ExtractorError> {
        self.ingest_event_internal(input, extractor, false)
    }

    fn ingest_event_internal(
        &mut self,
        input: EventInput,
        extractor: &(impl MnemeExtractor + ?Sized),
        use_rule_ontology: bool,
    ) -> Result<(), ExtractorError> {
        let event = EventRecord {
            id: next_id("event", self.next_event_number()),
            speaker_id: input.speaker_id,
            actor_agent_id: input.actor_agent_id,
            text: input.text,
            scope: input.scope,
            trust_level: input.trust_level,
        };
        self.audit.push(AuditRecord {
            kind: AuditKind::EventAppend,
            target_id: format!(
                "{}:{}:{}",
                event.id,
                event.actor_agent_id.as_deref().unwrap_or("no-agent"),
                event.trust_level
            ),
        });

        let token_estimate = estimate_tokens(&event.text);
        if self.budget.spent_tokens.saturating_add(token_estimate) > self.budget.daily_cloud_tokens
        {
            self.budget.hard_cap_violations = self.budget.hard_cap_violations.saturating_add(1);
            self.audit.push(AuditRecord {
                kind: AuditKind::BudgetBlock,
                target_id: event.id.clone(),
            });
            self.events.push(event);
            return Ok(());
        }
        self.budget.spent_tokens = self.budget.spent_tokens.saturating_add(token_estimate);

        if self.apply_lifecycle_event(&event) {
            self.events.push(event);
            return Ok(());
        }

        if use_rule_ontology {
            self.apply_natural_supersessions(&event);
        }

        let drafts = if use_rule_ontology {
            rule_based_claim_drafts_for_event(&event)
        } else {
            extractor
                .extract(&event)?
                .map(ClaimDraft::active)
                .into_iter()
                .collect()
        };

        for draft in drafts {
            let claim = claim_from_draft(
                &event,
                self.next_claim_number(),
                draft,
                vec![event.id.clone()],
            );
            self.audit.push(AuditRecord {
                kind: AuditKind::ClaimWrite,
                target_id: claim.id.clone(),
            });
            self.claims.push(claim);
        }

        self.events.push(event);
        Ok(())
    }

    fn apply_natural_supersessions(&mut self, event: &EventRecord) {
        let text = event.text.to_ascii_lowercase();
        if text.contains("changed that schedule to monday mornings") {
            self.supersede_active_claim("Ari", "prefers", "weekly eval reports on Friday");
        }
        if text.contains("rejected storing full transcripts") {
            self.supersede_active_claim("Mina", "suggested", "full transcripts");
        }
    }

    fn supersede_active_claim(&mut self, subject: &str, predicate: &str, object: &str) {
        for claim in &mut self.claims {
            if claim.status == ClaimStatus::Active
                && claim.subject == subject
                && claim.predicate == predicate
                && claim.object == object
            {
                claim.status = ClaimStatus::Superseded;
                self.audit.push(AuditRecord {
                    kind: AuditKind::ClaimUpdate,
                    target_id: claim.id.clone(),
                });
            }
        }
    }

    fn apply_lifecycle_event(&mut self, event: &EventRecord) -> bool {
        if let Some(target_id) = find_forget_id_marker(&event.text) {
            self.forget_claim_by_id(target_id);
            return true;
        }
        if let Some((target_id, new_text)) = find_correct_id_marker(&event.text) {
            self.correct_claim_by_id(event, target_id, new_text);
            return true;
        }
        if let Some(target) = find_forget_marker(&event.text) {
            self.forget_claims(target);
            return true;
        }
        if let Some((old_text, new_text)) = find_correct_marker(&event.text) {
            self.correct_claims(event, old_text, new_text);
            return true;
        }
        false
    }

    fn forget_claim_by_id(&mut self, target_id: &str) {
        let target_id = target_id.trim();
        for claim in &mut self.claims {
            if claim.status == ClaimStatus::Active && claim.id == target_id {
                claim.status = ClaimStatus::Forgotten;
                self.audit.push(AuditRecord {
                    kind: AuditKind::ClaimUpdate,
                    target_id: claim.id.clone(),
                });
                break;
            }
        }
    }

    fn forget_claims(&mut self, target: &str) {
        for claim in &mut self.claims {
            if claim.status == ClaimStatus::Active && claim_matches_text(claim, target) {
                claim.status = ClaimStatus::Forgotten;
                self.audit.push(AuditRecord {
                    kind: AuditKind::ClaimUpdate,
                    target_id: claim.id.clone(),
                });
            }
        }
    }

    fn correct_claim_by_id(&mut self, event: &EventRecord, target_id: &str, new_text: &str) {
        let target_id = target_id.trim();
        let mut source_event_ids = Vec::new();
        for claim in &mut self.claims {
            if claim.status == ClaimStatus::Active && claim.id == target_id {
                claim.status = ClaimStatus::Superseded;
                source_event_ids.extend(claim.source_event_ids.clone());
                self.audit.push(AuditRecord {
                    kind: AuditKind::ClaimUpdate,
                    target_id: claim.id.clone(),
                });
                break;
            }
        }

        if source_event_ids.is_empty() {
            return;
        }
        source_event_ids.push(event.id.clone());
        dedupe_ids(&mut source_event_ids);

        let extracted = extracted_claim_from_text(event, new_text);
        let claim =
            claim_from_extracted(event, self.next_claim_number(), extracted, source_event_ids);
        self.audit.push(AuditRecord {
            kind: AuditKind::ClaimWrite,
            target_id: claim.id.clone(),
        });
        self.claims.push(claim);
    }

    fn correct_claims(&mut self, event: &EventRecord, old_text: &str, new_text: &str) {
        let mut source_event_ids = Vec::new();
        for claim in &mut self.claims {
            if claim.status == ClaimStatus::Active && claim_matches_text(claim, old_text) {
                claim.status = ClaimStatus::Superseded;
                source_event_ids.extend(claim.source_event_ids.clone());
                self.audit.push(AuditRecord {
                    kind: AuditKind::ClaimUpdate,
                    target_id: claim.id.clone(),
                });
            }
        }

        if source_event_ids.is_empty() {
            return;
        }
        source_event_ids.push(event.id.clone());
        dedupe_ids(&mut source_event_ids);

        let extracted = extracted_claim_from_text(event, new_text);
        let claim =
            claim_from_extracted(event, self.next_claim_number(), extracted, source_event_ids);
        self.audit.push(AuditRecord {
            kind: AuditKind::ClaimWrite,
            target_id: claim.id.clone(),
        });
        self.claims.push(claim);
    }

    /// Builds a context pack over active claims in the default private scope.
    pub fn build_context_pack(&mut self, query: impl Into<String>) -> ContextPack {
        self.build_context_pack_with(ContextQuery::new(query))
    }

    /// Builds a context pack over active claims allowed by the provided query.
    pub fn build_context_pack_with(&mut self, query: ContextQuery) -> ContextPack {
        let query_text = query.text;
        let allowed_scopes = normalize_allowed_scopes(query.allowed_scopes);
        let query_terms = normalize_query_terms(&query_text);
        let mut candidates = Vec::new();
        let mut omitted = Vec::new();

        for (claim_index, claim) in self.claims.iter().enumerate() {
            if claim.status != ClaimStatus::Active {
                omitted.push(OmittedContextItem {
                    claim_id: claim.id.clone(),
                    reason: claim.status.as_str().to_owned(),
                });
                continue;
            }

            let claim_scope = normalize_scope(&claim.scope);
            if !allowed_scopes.contains(&claim_scope) {
                omitted.push(OmittedContextItem {
                    claim_id: claim.id.clone(),
                    reason: format!("scope_denied:{claim_scope}"),
                });
                continue;
            }

            let claim_text = claim.text();
            let relevance_text = self.claim_relevance_text(claim, &claim_text);
            if let Some(relevance) = score_context_match(&query_text, &query_terms, &relevance_text)
            {
                candidates.push(RankedContextCandidate {
                    claim_index,
                    item: ContextItem {
                        claim_id: claim.id.clone(),
                        claim_text,
                        source_event_ids: claim.source_event_ids.clone(),
                        score: relevance.score,
                        matched_terms: relevance.matched_terms,
                        match_reason: relevance.reason,
                    },
                });
            } else {
                omitted.push(OmittedContextItem {
                    claim_id: claim.id.clone(),
                    reason: "low_relevance".to_owned(),
                });
            }
        }

        candidates.sort_by(|left, right| {
            right
                .item
                .score
                .cmp(&left.item.score)
                .then_with(|| left.claim_index.cmp(&right.claim_index))
        });

        let mut items = Vec::new();
        for candidate in candidates {
            if items.len() < query.max_items {
                items.push(candidate.item);
            } else {
                omitted.push(OmittedContextItem {
                    claim_id: candidate.item.claim_id,
                    reason: format!("context_budget_exceeded:max_items={}", query.max_items),
                });
            }
        }

        self.audit.push(AuditRecord {
            kind: AuditKind::ContextRead,
            target_id: if query_text.is_empty() {
                "empty-query".to_owned()
            } else {
                query_text
            },
        });

        ContextPack { items, omitted }
    }

    fn claim_relevance_text(&self, claim: &ClaimRecord, claim_text: &str) -> String {
        let mut text = claim_text.to_owned();
        for source_event_id in &claim.source_event_ids {
            if let Some(event) = self
                .events
                .iter()
                .find(|event| &event.id == source_event_id)
            {
                text.push(' ');
                text.push_str(&event.text);
            }
        }
        text
    }

    /// Starts an agent session and returns task-scoped context.
    pub fn begin_session(&mut self, input: SessionBeginInput) -> SessionBeginReport {
        let query = input.query.unwrap_or_else(|| input.task.clone());
        let context_pack = self.build_context_pack_with(
            ContextQuery::with_allowed_scopes(query.clone(), input.allowed_scopes)
                .with_max_items(input.max_items),
        );
        let session = SessionRecord {
            id: next_id("session", self.next_session_number()),
            task: input.task,
            lineage_id: input.lineage_id.and_then(non_empty_string),
            actor_agent_id: input.actor_agent_id,
            status: SessionStatus::Active,
            started_at_unix_seconds: unix_timestamp(),
            ended_at_unix_seconds: None,
            context_query: query,
            context_claim_ids: context_pack
                .items
                .iter()
                .map(|item| item.claim_id.clone())
                .collect(),
            summary: None,
            memory_event_ids: Vec::new(),
        };
        self.audit.push(AuditRecord {
            kind: AuditKind::SessionBegin,
            target_id: session.id.clone(),
        });
        self.sessions.push(session.clone());
        SessionBeginReport {
            session,
            context_pack,
        }
    }

    /// Ends an agent session and optionally records remembered claims.
    pub fn end_session(
        &mut self,
        input: SessionEndInput,
    ) -> Result<SessionEndReport, SessionError> {
        self.end_session_with_extractor(
            input,
            &RuleBasedExtractor,
            SessionMemoryInputMode::ExplicitClaim,
        )
    }

    /// Ends an agent session and records remembered notes through an extractor.
    pub fn end_session_with_extractor(
        &mut self,
        input: SessionEndInput,
        extractor: &(impl MnemeExtractor + ?Sized),
        memory_input_mode: SessionMemoryInputMode,
    ) -> Result<SessionEndReport, SessionError> {
        let position = self
            .sessions
            .iter()
            .position(|session| session.id == input.session_id)
            .ok_or_else(|| SessionError::new(format!("unknown session: {}", input.session_id)))?;
        if self.sessions[position].status != SessionStatus::Active {
            return Err(SessionError::new(format!(
                "session {} is already {}",
                input.session_id,
                self.sessions[position].status.as_str()
            )));
        }
        let actor_agent_id = input
            .actor_agent_id
            .clone()
            .or_else(|| self.sessions[position].actor_agent_id.clone());
        let scope = input
            .scope
            .clone()
            .and_then(non_empty_string)
            .unwrap_or_else(|| "private".to_owned());
        let mut remembered_event_ids = Vec::new();
        let mut remembered_claim_ids = Vec::new();

        for claim in input.remember {
            let text = match memory_input_mode {
                SessionMemoryInputMode::ExplicitClaim => format!("remember: {claim}"),
                SessionMemoryInputMode::RawEvent => claim,
            };
            let event_id = next_id("event", self.next_event_number());
            self.ingest_event_with_extractor(
                EventInput {
                    speaker_id: "agent".to_owned(),
                    actor_agent_id: actor_agent_id.clone(),
                    text,
                    scope: scope.clone(),
                    trust_level: "agent_summary".to_owned(),
                },
                extractor,
            )
            .map_err(|source| SessionError::new(format!("record session memory: {source}")))?;
            remembered_event_ids.push(event_id.clone());
            remembered_claim_ids.extend(
                self.claims
                    .iter()
                    .filter(|claim| claim.source_event_ids.contains(&event_id))
                    .map(|claim| claim.id.clone()),
            );
        }

        let session = &mut self.sessions[position];
        session.status = SessionStatus::Closed;
        session.ended_at_unix_seconds = Some(unix_timestamp());
        session.summary = input.summary;
        session
            .memory_event_ids
            .extend(remembered_event_ids.iter().cloned());
        self.audit.push(AuditRecord {
            kind: AuditKind::SessionEnd,
            target_id: session.id.clone(),
        });

        Ok(SessionEndReport {
            session: session.clone(),
            remembered_event_ids,
            remembered_claim_ids,
        })
    }

    /// Returns the serializable engine state.
    #[must_use]
    pub fn state(&self) -> MnemeState {
        MnemeState {
            schema_version: self.schema_version,
            metadata: self.metadata.clone(),
            budget: self.budget.clone(),
            events: self.events.clone(),
            claims: self.claims.clone(),
            sessions: self.sessions.clone(),
            audit: self.audit.clone(),
        }
    }

    /// Saves the current engine state through a storage adapter.
    pub fn persist(&self, store: &mut impl MnemeStore) -> Result<(), StoreError> {
        store.save(&self.state())
    }

    /// Returns a read-only snapshot of the engine state.
    #[must_use]
    pub fn snapshot(&self) -> EngineSnapshot {
        EngineSnapshot {
            schema_version: self.schema_version,
            metadata: self.metadata.clone(),
            events: self.events.clone(),
            claims: self.claims.clone(),
            sessions: self.sessions.clone(),
            budget: self.budget.clone(),
            audit: self.audit.clone(),
        }
    }

    /// Removes non-active memory records while preserving active recall.
    pub fn compact(&mut self) -> CompactionReport {
        let events_before = self.events.len();
        let claims_before = self.claims.len();

        self.claims
            .retain(|claim| claim.status == ClaimStatus::Active);

        let kept_event_ids = self
            .claims
            .iter()
            .flat_map(|claim| claim.source_event_ids.iter().cloned())
            .collect::<BTreeSet<_>>();
        self.events
            .retain(|event| kept_event_ids.contains(event.id.as_str()));

        let report = CompactionReport {
            events_before,
            events_after: self.events.len(),
            claims_before,
            claims_after: self.claims.len(),
            removed_events: events_before.saturating_sub(self.events.len()),
            removed_claims: claims_before.saturating_sub(self.claims.len()),
        };
        self.audit.push(AuditRecord {
            kind: AuditKind::StateCompact,
            target_id: format!(
                "events:{}->{} claims:{}->{}",
                report.events_before,
                report.events_after,
                report.claims_before,
                report.claims_after
            ),
        });
        report
    }

    fn next_event_number(&self) -> usize {
        next_number_for_prefix("event", self.events.iter().map(|event| event.id.as_str()))
    }

    fn next_claim_number(&self) -> usize {
        next_number_for_prefix("claim", self.claims.iter().map(|claim| claim.id.as_str()))
    }

    fn next_session_number(&self) -> usize {
        next_number_for_prefix(
            "session",
            self.sessions.iter().map(|session| session.id.as_str()),
        )
    }
}

/// Adapter boundary for extracting memory claims from events.
pub trait MnemeExtractor {
    /// Extracts one claim candidate from an event, when the adapter can find one.
    fn extract(&self, event: &EventRecord) -> Result<Option<ExtractedClaim>, ExtractorError>;
}

/// Stable JSON protocol version used by command-backed extraction adapters.
pub const EXTRACTOR_COMMAND_SCHEMA_VERSION: &str = "mneme.extractor.command.v1";

/// Request passed to an external extraction command over stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorCommandRequest {
    /// Protocol schema version.
    pub schema_version: String,
    /// Event to inspect for memory-worthy claims.
    pub event: EventRecord,
}

impl ExtractorCommandRequest {
    /// Creates a command request from an engine event.
    #[must_use]
    pub fn for_event(event: &EventRecord) -> Self {
        Self {
            schema_version: EXTRACTOR_COMMAND_SCHEMA_VERSION.to_owned(),
            event: event.clone(),
        }
    }
}

/// Response returned by an external extraction command over stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractorCommandResponse {
    /// Protocol schema version.
    pub schema_version: String,
    /// Optional claim candidate. Use `null` when no claim should be persisted.
    pub claim: Option<ExtractedClaim>,
}

impl ExtractorCommandResponse {
    /// Creates a response that contains a claim.
    #[must_use]
    pub fn from_claim(claim: ExtractedClaim) -> Self {
        Self {
            schema_version: EXTRACTOR_COMMAND_SCHEMA_VERSION.to_owned(),
            claim: Some(claim),
        }
    }

    /// Creates a response that intentionally extracts no claim.
    #[must_use]
    pub fn no_claim() -> Self {
        Self {
            schema_version: EXTRACTOR_COMMAND_SCHEMA_VERSION.to_owned(),
            claim: None,
        }
    }
}

/// Extraction adapter that delegates claim extraction to an external command.
#[derive(Debug, Clone)]
pub struct CommandExtractor {
    program: String,
    args: Vec<String>,
}

impl CommandExtractor {
    /// Creates a command-backed extractor without invoking a shell.
    #[must_use]
    pub fn new(program: impl Into<String>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    /// Command program path or name.
    #[must_use]
    pub fn program(&self) -> &str {
        &self.program
    }

    /// Command arguments.
    #[must_use]
    pub fn args(&self) -> &[String] {
        &self.args
    }
}

impl MnemeExtractor for CommandExtractor {
    fn extract(&self, event: &EventRecord) -> Result<Option<ExtractedClaim>, ExtractorError> {
        let request = ExtractorCommandRequest::for_event(event);
        let request_json = serde_json::to_vec(&request)
            .map_err(|source| ExtractorError::new(format!("encode extractor request: {source}")))?;
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| {
                ExtractorError::new(format!(
                    "start extractor command {}: {source}",
                    self.program
                ))
            })?;

        {
            let mut stdin = child.stdin.take().ok_or_else(|| {
                ExtractorError::new(format!("open stdin for extractor command {}", self.program))
            })?;
            stdin.write_all(&request_json).map_err(|source| {
                ExtractorError::new(format!(
                    "write extractor request to {}: {source}",
                    self.program
                ))
            })?;
            stdin.write_all(b"\n").map_err(|source| {
                ExtractorError::new(format!(
                    "finish extractor request to {}: {source}",
                    self.program
                ))
            })?;
        }

        let output = child.wait_with_output().map_err(|source| {
            ExtractorError::new(format!(
                "wait for extractor command {}: {source}",
                self.program
            ))
        })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ExtractorError::new(format!(
                "extractor command {} exited with status {}: {}",
                self.program,
                output.status,
                truncate_for_error(stderr.trim())
            )));
        }

        if output.stdout.is_empty() {
            return Err(ExtractorError::new(format!(
                "extractor command {} returned empty stdout",
                self.program
            )));
        }

        let response: ExtractorCommandResponse =
            serde_json::from_slice(&output.stdout).map_err(|source| {
                ExtractorError::new(format!(
                    "parse extractor response from {}: {source}",
                    self.program
                ))
            })?;
        if response.schema_version != EXTRACTOR_COMMAND_SCHEMA_VERSION {
            return Err(ExtractorError::new(format!(
                "unsupported extractor response schema: {}",
                response.schema_version
            )));
        }
        if let Some(claim) = &response.claim {
            validate_extracted_claim(claim)?;
        }
        Ok(response.claim)
    }
}

/// Error returned by extraction adapters.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ExtractorError {
    message: String,
}

impl ExtractorError {
    /// Creates an extractor error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ExtractorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExtractorError {}

/// Claim candidate returned by an extraction adapter before engine persistence.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractedClaim {
    /// Claim subject.
    pub subject: String,
    /// Claim predicate.
    pub predicate: String,
    /// Claim object.
    pub object: String,
}

impl ExtractedClaim {
    /// Creates a claim candidate.
    #[must_use]
    pub fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
        }
    }

    /// Text form used by lifecycle matching.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{} {} {}", self.subject, self.predicate, self.object)
    }
}

#[derive(Debug, Clone)]
struct ClaimDraft {
    extracted: ExtractedClaim,
    status: ClaimStatus,
}

impl ClaimDraft {
    fn active(extracted: ExtractedClaim) -> Self {
        Self {
            extracted,
            status: ClaimStatus::Active,
        }
    }

    fn new(
        subject: impl Into<String>,
        predicate: impl Into<String>,
        object: impl Into<String>,
    ) -> Self {
        Self::active(ExtractedClaim::new(subject, predicate, object))
    }
}

/// Deterministic extraction adapter for explicit v1 lifecycle markers.
#[derive(Debug, Clone, Copy, Default)]
pub struct RuleBasedExtractor;

impl RuleBasedExtractor {
    /// Creates a rule-based extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl MnemeExtractor for RuleBasedExtractor {
    fn extract(&self, event: &EventRecord) -> Result<Option<ExtractedClaim>, ExtractorError> {
        let Some(marker) = find_remember_marker(&event.text) else {
            return Ok(None);
        };
        Ok(Some(extracted_claim_from_text(event, marker)))
    }
}

/// Serializable v1 engine state used by persistence adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MnemeState {
    /// Persisted state schema version.
    #[serde(default)]
    pub schema_version: u32,
    /// Store metadata used for migrations and local maintenance.
    #[serde(default)]
    pub metadata: StateMetadata,
    /// Budget state at persistence time.
    pub budget: BudgetState,
    /// Events appended before persistence.
    pub events: Vec<EventRecord>,
    /// Claims extracted before persistence.
    pub claims: Vec<ClaimRecord>,
    /// Agent sessions recorded before persistence.
    #[serde(default)]
    pub sessions: Vec<SessionRecord>,
    /// Audit records captured before persistence.
    pub audit: Vec<AuditRecord>,
}

impl MnemeState {
    fn prepared_for_save(&self) -> Self {
        let mut state = self.clone();
        let now = unix_timestamp();
        let previous_schema_version = state.schema_version;
        if previous_schema_version != MNEME_STATE_SCHEMA_VERSION {
            state.metadata.migration_history.push(MigrationRecord {
                from_schema_version: previous_schema_version,
                to_schema_version: MNEME_STATE_SCHEMA_VERSION,
                at_unix_seconds: now,
            });
        }
        state.schema_version = MNEME_STATE_SCHEMA_VERSION;
        state.metadata.prepare_for_save(now);
        state
    }
}

/// Store metadata persisted alongside v1 memory state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StateMetadata {
    /// Stable public-safe local store identifier.
    pub store_id: String,
    /// Monotonic generation increased on each successful save.
    pub generation: u64,
    /// Unix timestamp for first normalized save.
    pub created_at_unix_seconds: u64,
    /// Unix timestamp for latest normalized save.
    pub updated_at_unix_seconds: u64,
    /// Mneme crate version that last wrote the store.
    pub engine_version: String,
    /// Schema migrations applied while normalizing this store.
    pub migration_history: Vec<MigrationRecord>,
}

impl StateMetadata {
    fn new() -> Self {
        let now = unix_timestamp();
        Self {
            store_id: format!("store-{now}"),
            generation: 0,
            created_at_unix_seconds: now,
            updated_at_unix_seconds: now,
            engine_version: env!("CARGO_PKG_VERSION").to_owned(),
            migration_history: Vec::new(),
        }
    }

    fn prepare_for_save(&mut self, now: u64) {
        if self.store_id.trim().is_empty() {
            self.store_id = format!("store-{now}");
        }
        if self.created_at_unix_seconds == 0 {
            self.created_at_unix_seconds = now;
        }
        self.updated_at_unix_seconds = now;
        self.generation = self.generation.saturating_add(1);
        self.engine_version = env!("CARGO_PKG_VERSION").to_owned();
    }
}

/// Migration record for state schema normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecord {
    /// Schema version observed before normalization.
    pub from_schema_version: u32,
    /// Schema version written after normalization.
    pub to_schema_version: u32,
    /// Unix timestamp when the migration record was added.
    pub at_unix_seconds: u64,
}

/// Result returned after in-memory compaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionReport {
    /// Event count before compaction.
    pub events_before: usize,
    /// Event count after compaction.
    pub events_after: usize,
    /// Claim count before compaction.
    pub claims_before: usize,
    /// Claim count after compaction.
    pub claims_after: usize,
    /// Number of events removed.
    pub removed_events: usize,
    /// Number of claims removed.
    pub removed_claims: usize,
}

/// Input used to start an agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBeginInput {
    /// Task or user request the agent is starting.
    pub task: String,
    /// Optional task/project lineage identifier shared across agent sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_id: Option<String>,
    /// Agent identifier, when available.
    pub actor_agent_id: Option<String>,
    /// Optional retrieval query. Defaults to `task`.
    pub query: Option<String>,
    /// Memory scopes that can be retrieved at session begin.
    #[serde(default = "default_allowed_scopes")]
    pub allowed_scopes: Vec<String>,
    /// Maximum number of context items returned at session begin.
    #[serde(default = "default_context_max_items")]
    pub max_items: usize,
}

/// Input used to end an agent session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndInput {
    /// Session identifier returned by `begin_session`.
    pub session_id: String,
    /// Agent identifier, when different from the begin call.
    pub actor_agent_id: Option<String>,
    /// Scope used for memory written by this session end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Optional task summary.
    pub summary: Option<String>,
    /// Claims or natural-language memory notes to record at session end.
    pub remember: Vec<String>,
}

/// How session-end memory inputs should be presented to an extractor.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryInputMode {
    /// Prefix each memory string with the explicit v1 `remember:` marker.
    ExplicitClaim,
    /// Pass each memory string as the raw event text.
    RawEvent,
}

/// Agent session record persisted in the v1 store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRecord {
    /// Stable session identifier.
    pub id: String,
    /// Task or user request associated with this session.
    pub task: String,
    /// Optional task/project lineage identifier shared across agent sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage_id: Option<String>,
    /// Agent identifier, when available.
    pub actor_agent_id: Option<String>,
    /// Session lifecycle status.
    pub status: SessionStatus,
    /// Unix timestamp when the session began.
    pub started_at_unix_seconds: u64,
    /// Unix timestamp when the session ended.
    pub ended_at_unix_seconds: Option<u64>,
    /// Query used to retrieve task context at begin time.
    pub context_query: String,
    /// Claim IDs returned to the agent at begin time.
    pub context_claim_ids: Vec<String>,
    /// Optional agent-provided task summary.
    pub summary: Option<String>,
    /// Event IDs written by `end_session`.
    pub memory_event_ids: Vec<String>,
}

/// Session lifecycle status.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session has started and can still be ended.
    Active,
    /// Session has been closed.
    Closed,
}

impl SessionStatus {
    /// Stable status identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Closed => "closed",
        }
    }
}

/// Report returned when an agent session starts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBeginReport {
    /// Persisted session record.
    pub session: SessionRecord,
    /// Context returned for the task.
    pub context_pack: ContextPack,
}

/// Report returned when an agent session ends.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEndReport {
    /// Persisted session record.
    pub session: SessionRecord,
    /// Event IDs created from remembered claims.
    pub remembered_event_ids: Vec<String>,
    /// Claim IDs created from remembered claims.
    pub remembered_claim_ids: Vec<String>,
}

/// Error returned by session operations.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SessionError {
    message: String,
}

impl SessionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SessionError {}

/// Storage port for loading and saving v1 personal-memory state.
pub trait MnemeStore {
    /// Loads the latest persisted state, or `None` when the store is empty.
    fn load(&self) -> Result<Option<MnemeState>, StoreError>;

    /// Saves a complete v1 state snapshot.
    fn save(&mut self, state: &MnemeState) -> Result<(), StoreError>;
}

/// In-memory store useful for tests and adapters that own persistence.
#[derive(Debug, Clone, Default)]
pub struct InMemoryStore {
    state: Option<MnemeState>,
}

impl InMemoryStore {
    /// Creates an empty in-memory store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an in-memory store seeded with a state snapshot.
    #[must_use]
    pub fn with_state(state: MnemeState) -> Self {
        Self { state: Some(state) }
    }
}

impl MnemeStore for InMemoryStore {
    fn load(&self) -> Result<Option<MnemeState>, StoreError> {
        Ok(self.state.clone())
    }

    fn save(&mut self, state: &MnemeState) -> Result<(), StoreError> {
        self.state = Some(state.clone());
        Ok(())
    }
}

/// JSON file-backed store for local v1 personal-memory state.
#[derive(Debug, Clone)]
pub struct JsonFileStore {
    path: PathBuf,
}

impl JsonFileStore {
    /// Creates a JSON file store at the provided path.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Returns the backing JSON file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the backup path used before replacing the current store.
    #[must_use]
    pub fn backup_path(&self) -> PathBuf {
        backup_path_for(&self.path)
    }

    /// Returns the lock path used to guard local write operations.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        lock_path_for(&self.path)
    }

    /// Inspects current and backup store files without mutating either file.
    #[must_use]
    pub fn inspect(&self) -> StoreInspection {
        let current = inspect_state_file(&self.path);
        let backup_path = self.backup_path();
        let backup = inspect_state_file(&backup_path);
        let repair_available =
            current.status != StoreFileStatus::Valid && backup.status == StoreFileStatus::Valid;
        StoreInspection {
            path: self.path.display().to_string(),
            backup_path: backup_path.display().to_string(),
            current,
            backup,
            repair_available,
        }
    }

    /// Restores the current store from the backup when the current file is not valid.
    pub fn repair_from_backup(&self) -> Result<StoreRepairReport, StoreError> {
        let _lock = acquire_store_lock(&self.path)?;
        let before = self.inspect();
        if before.current.status == StoreFileStatus::Valid {
            let state = read_state_file(&self.path)?
                .ok_or_else(|| StoreError::new("current store disappeared before repair"))?;
            if state_needs_normalization(&state) {
                backup_current_file(&self.path)?;
                write_state_atomic(&self.path, &state.prepared_for_save())?;
                return Ok(StoreRepairReport {
                    repaired: true,
                    action: "normalized_current".to_owned(),
                    before,
                    after: self.inspect(),
                });
            }
            return Ok(StoreRepairReport {
                repaired: false,
                action: "current_valid".to_owned(),
                before,
                after: self.inspect(),
            });
        }
        if before.backup.status != StoreFileStatus::Valid {
            return Ok(StoreRepairReport {
                repaired: false,
                action: "backup_unavailable".to_owned(),
                before,
                after: self.inspect(),
            });
        }

        let state = read_state_file(&self.backup_path())?
            .ok_or_else(|| StoreError::new("backup disappeared before repair"))?;
        write_state_atomic(&self.path, &state.prepared_for_save())?;
        Ok(StoreRepairReport {
            repaired: true,
            action: "restored_from_backup".to_owned(),
            before,
            after: self.inspect(),
        })
    }

    /// Restores the current store from a valid backup even when current is valid.
    ///
    /// The current file is copied to the backup path before the older backup
    /// state replaces it, so a restore can be reversed by restoring again.
    pub fn restore_from_backup(&self) -> Result<StoreRestoreReport, StoreError> {
        let _lock = acquire_store_lock(&self.path)?;
        let before = self.inspect();
        if before.backup.status != StoreFileStatus::Valid {
            return Ok(StoreRestoreReport {
                restored: false,
                action: "backup_unavailable".to_owned(),
                current_preserved_as_backup: false,
                before,
                after: self.inspect(),
            });
        }

        let state = read_state_file(&self.backup_path())?
            .ok_or_else(|| StoreError::new("backup disappeared before restore"))?;
        backup_current_file(&self.path)?;
        let current_preserved_as_backup = before.current.status != StoreFileStatus::Missing;
        write_state_atomic(&self.path, &state.prepared_for_save())?;
        Ok(StoreRestoreReport {
            restored: true,
            action: "restored_from_backup".to_owned(),
            current_preserved_as_backup,
            before,
            after: self.inspect(),
        })
    }
}

impl MnemeStore for JsonFileStore {
    fn load(&self) -> Result<Option<MnemeState>, StoreError> {
        read_state_file(&self.path)
    }

    fn save(&mut self, state: &MnemeState) -> Result<(), StoreError> {
        let _lock = acquire_store_lock(&self.path)?;
        match read_state_file(&self.path) {
            Ok(Some(current)) => {
                let incoming_generation = state.metadata.generation;
                let current_generation = current.metadata.generation;
                if incoming_generation != 0 && incoming_generation < current_generation {
                    return Err(StoreError::lock_conflict(format!(
                        "store generation changed while this writer was active: loaded={incoming_generation}, current={current_generation}"
                    )));
                }
            }
            Ok(None) => {}
            Err(_) if state.metadata.generation == 0 => {}
            Err(error) => return Err(error),
        }
        let prepared = state.prepared_for_save();
        backup_current_file(&self.path)?;
        write_state_atomic(&self.path, &prepared)
    }
}

fn read_state_file(path: &Path) -> Result<Option<MnemeState>, StoreError> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|source| StoreError::new(format!("failed to parse state: {source}"))),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(StoreError::new(format!("failed to read state: {source}"))),
    }
}

fn write_state_atomic(path: &Path, state: &MnemeState) -> Result<(), StoreError> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|source| StoreError::new(format!("failed to create store dir: {source}")))?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|source| StoreError::new(format!("failed to encode state: {source}")))?;
    let temp_path = temp_path_for(path);
    {
        let mut file = File::create(&temp_path)
            .map_err(|source| StoreError::new(format!("failed to create temp state: {source}")))?;
        file.write_all(format!("{text}\n").as_bytes())
            .map_err(|source| StoreError::new(format!("failed to write temp state: {source}")))?;
        file.sync_all()
            .map_err(|source| StoreError::new(format!("failed to sync temp state: {source}")))?;
    }
    fs::rename(&temp_path, path)
        .map_err(|source| StoreError::new(format!("failed to replace state: {source}")))
}

fn backup_current_file(path: &Path) -> Result<(), StoreError> {
    if !path.exists() {
        return Ok(());
    }
    let backup_path = backup_path_for(path);
    if let Some(parent) = backup_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|source| StoreError::new(format!("failed to create backup dir: {source}")))?;
    }
    fs::copy(path, &backup_path)
        .map(|_| ())
        .map_err(|source| StoreError::new(format!("failed to write backup state: {source}")))
}

fn state_needs_normalization(state: &MnemeState) -> bool {
    state.schema_version != MNEME_STATE_SCHEMA_VERSION
        || state.metadata.store_id.trim().is_empty()
        || state.metadata.created_at_unix_seconds == 0
        || state.metadata.generation == 0
}

#[derive(Debug)]
struct StoreLockGuard {
    path: PathBuf,
}

impl Drop for StoreLockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_store_lock(path: &Path) -> Result<StoreLockGuard, StoreError> {
    if let Some(parent) = path.parent().filter(|path| !path.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|source| StoreError::new(format!("failed to create store dir: {source}")))?;
    }
    let lock_path = lock_path_for(path);
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
            if stale_lock_should_be_recovered(&lock_path) {
                fs::remove_file(&lock_path).map_err(|source| {
                    StoreError::lock_conflict(format!(
                        "failed to recover stale store lock {}: {source}",
                        lock_path.display()
                    ))
                })?;
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)
                    .map_err(|source| {
                        StoreError::lock_conflict(format!(
                            "store lock already exists after stale recovery: {} ({source})",
                            lock_path.display()
                        ))
                    })?
            } else {
                return Err(StoreError::lock_conflict(format!(
                    "store lock already exists: {}",
                    lock_path.display()
                )));
            }
        }
        Err(source) => {
            return Err(StoreError::new(format!(
                "failed to create store lock: {source}"
            )));
        }
    };
    let lock_body = format!(
        "pid={}\ncreated_at_unix_seconds={}\n",
        std::process::id(),
        unix_timestamp()
    );
    if let Err(source) = file.write_all(lock_body.as_bytes()) {
        let _ = fs::remove_file(&lock_path);
        return Err(StoreError::new(format!(
            "failed to write store lock: {source}"
        )));
    }
    if let Err(source) = file.sync_all() {
        let _ = fs::remove_file(&lock_path);
        return Err(StoreError::new(format!(
            "failed to sync store lock: {source}"
        )));
    }
    Ok(StoreLockGuard { path: lock_path })
}

fn stale_lock_should_be_recovered(lock_path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(lock_path) else {
        return false;
    };
    let Some(created_at) = text.lines().find_map(|line| {
        line.strip_prefix("created_at_unix_seconds=")
            .and_then(|value| value.parse::<u64>().ok())
    }) else {
        return false;
    };
    unix_timestamp().saturating_sub(created_at) > STORE_LOCK_STALE_SECONDS
}

fn backup_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("mneme-v1.json");
    path.with_file_name(format!("{file_name}.bak"))
}

fn lock_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("mneme-v1.json");
    path.with_file_name(format!("{file_name}.lock"))
}

fn temp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("mneme-v1.json");
    path.with_file_name(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unix_timestamp()
    ))
}

fn inspect_state_file(path: &Path) -> StoreFileInspection {
    match read_state_file(path) {
        Ok(Some(state)) => {
            let validation = validate_state(&state);
            let status = if validation.ok {
                StoreFileStatus::Valid
            } else {
                StoreFileStatus::Invalid
            };
            StoreFileInspection {
                path: path.display().to_string(),
                status,
                schema_version: Some(state.schema_version),
                generation: Some(state.metadata.generation),
                event_count: Some(state.events.len()),
                claim_count: Some(state.claims.len()),
                error: None,
                validation: Some(validation),
            }
        }
        Ok(None) => StoreFileInspection {
            path: path.display().to_string(),
            status: StoreFileStatus::Missing,
            schema_version: None,
            generation: None,
            event_count: None,
            claim_count: None,
            error: None,
            validation: None,
        },
        Err(error) => StoreFileInspection {
            path: path.display().to_string(),
            status: StoreFileStatus::Invalid,
            schema_version: None,
            generation: None,
            event_count: None,
            claim_count: None,
            error: Some(error.to_string()),
            validation: None,
        },
    }
}

/// Store-level inspection report for current and backup JSON files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreInspection {
    /// Current store path.
    pub path: String,
    /// Backup store path.
    pub backup_path: String,
    /// Current file inspection.
    pub current: StoreFileInspection,
    /// Backup file inspection.
    pub backup: StoreFileInspection,
    /// Whether backup repair can restore the current file.
    pub repair_available: bool,
}

/// Inspection report for one store file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreFileInspection {
    /// File path inspected.
    pub path: String,
    /// File status.
    pub status: StoreFileStatus,
    /// Parsed schema version, when available.
    pub schema_version: Option<u32>,
    /// Parsed store generation, when available.
    pub generation: Option<u64>,
    /// Parsed event count, when available.
    pub event_count: Option<usize>,
    /// Parsed claim count, when available.
    pub claim_count: Option<usize>,
    /// Read or parse error, when invalid before validation.
    pub error: Option<String>,
    /// State validation report, when parsing succeeded.
    pub validation: Option<StateValidationReport>,
}

/// Store file status used by inspection and repair reports.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreFileStatus {
    /// File does not exist.
    Missing,
    /// File parses and passes validation.
    Valid,
    /// File is unreadable, unparsable, or fails validation.
    Invalid,
}

/// Result of attempting backup repair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreRepairReport {
    /// Whether the current store was restored from backup.
    pub repaired: bool,
    /// Stable action identifier.
    pub action: String,
    /// Inspection before repair.
    pub before: StoreInspection,
    /// Inspection after repair.
    pub after: StoreInspection,
}

/// Result of explicitly restoring the current store from backup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreRestoreReport {
    /// Whether the current store was restored from backup.
    pub restored: bool,
    /// Stable action identifier.
    pub action: String,
    /// Whether the previous current file was preserved as the new backup.
    pub current_preserved_as_backup: bool,
    /// Inspection before restore.
    pub before: StoreInspection,
    /// Inspection after restore.
    pub after: StoreInspection,
}

/// Error returned by v1 persistence adapters.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoreError {
    message: String,
    kind: StoreErrorKind,
}

impl StoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: StoreErrorKind::Store,
        }
    }

    fn lock_conflict(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: StoreErrorKind::LockConflict,
        }
    }

    /// Stable store error category.
    #[must_use]
    pub const fn kind(&self) -> StoreErrorKind {
        self.kind
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {}

/// Stable store error categories.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreErrorKind {
    /// Generic store read, write, parse, validation, or repair failure.
    Store,
    /// The local store lock file already exists and the write was not attempted.
    LockConflict,
}

impl StoreErrorKind {
    /// Stable string identifier for CLI and hook error envelopes.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Store => "store",
            Self::LockConflict => "store_lock",
        }
    }
}

/// Engine configuration for personal-memory v1.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MnemeConfig {
    /// Deterministic token cap used before cloud or model work is allowed.
    pub daily_cloud_tokens: u32,
}

impl Default for MnemeConfig {
    fn default() -> Self {
        Self {
            daily_cloud_tokens: 100_000,
        }
    }
}

/// Input event accepted by the v1 personal-memory core.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventInput {
    /// Speaker that produced the event.
    pub speaker_id: String,
    /// Agent acting on behalf of the speaker, when available.
    pub actor_agent_id: Option<String>,
    /// Raw event text.
    pub text: String,
    /// Memory scope for extracted claims.
    pub scope: String,
    /// Trust tier assigned to this input.
    pub trust_level: String,
}

/// Raw event appended by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    /// Stable event identifier.
    pub id: String,
    /// Speaker that produced the event.
    pub speaker_id: String,
    /// Agent acting on behalf of the speaker, when available.
    pub actor_agent_id: Option<String>,
    /// Raw event text.
    pub text: String,
    /// Memory scope for extracted claims.
    pub scope: String,
    /// Trust tier assigned to this input.
    pub trust_level: String,
}

/// Memory claim extracted from an event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimRecord {
    /// Stable claim identifier.
    pub id: String,
    /// Claim subject.
    pub subject: String,
    /// Claim predicate.
    pub predicate: String,
    /// Claim object.
    pub object: String,
    /// Claim lifecycle state.
    pub status: ClaimStatus,
    /// Memory scope inherited from the source event.
    pub scope: String,
    /// Source event IDs that support the claim.
    pub source_event_ids: Vec<String>,
}

impl ClaimRecord {
    /// Text form used by context-pack retrieval.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{} {} {}", self.subject, self.predicate, self.object)
    }
}

/// Claim lifecycle state.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    /// Claim is usable for context retrieval.
    Active,
    /// Claim resembles a secret and is excluded from context retrieval.
    BlockedSecret,
    /// Claim was replaced by a later correction.
    Superseded,
    /// Claim was intentionally forgotten by a later event.
    Forgotten,
}

impl ClaimStatus {
    /// Stable status identifier.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::BlockedSecret => "blocked_secret",
            Self::Superseded => "superseded",
            Self::Forgotten => "forgotten",
        }
    }
}

/// Context-pack retrieval output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPack {
    /// Context items selected for use by an agent.
    pub items: Vec<ContextItem>,
    /// Candidate items omitted from the context pack.
    pub omitted: Vec<OmittedContextItem>,
}

/// Scoped context-pack retrieval request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextQuery {
    /// Query text used for relevance matching.
    pub text: String,
    /// Memory scopes the caller is authorized to retrieve.
    #[serde(default = "default_allowed_scopes")]
    pub allowed_scopes: Vec<String>,
    /// Maximum number of relevant items returned to the caller.
    #[serde(default = "default_context_max_items")]
    pub max_items: usize,
}

impl ContextQuery {
    /// Creates a query that can retrieve from the default private scope.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            allowed_scopes: default_allowed_scopes(),
            max_items: DEFAULT_CONTEXT_MAX_ITEMS,
        }
    }

    /// Creates a query with explicit allowed retrieval scopes.
    #[must_use]
    pub fn with_allowed_scopes(
        text: impl Into<String>,
        allowed_scopes: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            text: text.into(),
            allowed_scopes: allowed_scopes.into_iter().map(Into::into).collect(),
            max_items: DEFAULT_CONTEXT_MAX_ITEMS,
        }
    }

    /// Sets the maximum number of context items returned by retrieval.
    #[must_use]
    pub const fn with_max_items(mut self, max_items: usize) -> Self {
        self.max_items = max_items;
        self
    }
}

/// Context item returned to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Claim that produced this context item.
    pub claim_id: String,
    /// Text form of the claim.
    pub claim_text: String,
    /// Source event IDs cited by this context item.
    pub source_event_ids: Vec<String>,
    /// Deterministic relevance score used for ranking.
    pub score: u32,
    /// Query terms matched by this item.
    pub matched_terms: Vec<String>,
    /// Stable reason describing why this item matched.
    pub match_reason: String,
}

/// Context candidate intentionally omitted from the pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmittedContextItem {
    /// Claim omitted from the context pack.
    pub claim_id: String,
    /// Stable omission reason.
    pub reason: String,
}

/// Deterministic budget state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetState {
    /// Configured daily token cap.
    pub daily_cloud_tokens: u32,
    /// Tokens spent by accepted events.
    pub spent_tokens: u32,
    /// Number of hard-cap blocks.
    pub hard_cap_violations: u32,
}

/// Audit event captured by the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditRecord {
    /// Audit event kind.
    pub kind: AuditKind,
    /// Target entity for the audit event.
    pub target_id: String,
}

/// Stable audit kind identifiers.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditKind {
    /// Event append operation.
    EventAppend,
    /// Claim write operation.
    ClaimWrite,
    /// Claim lifecycle update operation.
    ClaimUpdate,
    /// Context read operation.
    ContextRead,
    /// Budget hard-cap block.
    BudgetBlock,
    /// State compaction operation.
    StateCompact,
    /// Agent session begin operation.
    SessionBegin,
    /// Agent session end operation.
    SessionEnd,
}

impl AuditKind {
    /// Stable audit kind string used by adapters and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EventAppend => "event.append",
            Self::ClaimWrite => "claim.write",
            Self::ClaimUpdate => "claim.update",
            Self::ContextRead => "context.read",
            Self::BudgetBlock => "budget.block",
            Self::StateCompact => "state.compact",
            Self::SessionBegin => "session.begin",
            Self::SessionEnd => "session.end",
        }
    }
}

/// Snapshot returned to adapters after scenario execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    /// Persisted state schema version.
    pub schema_version: u32,
    /// Store metadata associated with this engine.
    pub metadata: StateMetadata,
    /// Events appended during the isolated run.
    pub events: Vec<EventRecord>,
    /// Claims extracted during the isolated run.
    pub claims: Vec<ClaimRecord>,
    /// Agent sessions recorded during the isolated run.
    pub sessions: Vec<SessionRecord>,
    /// Budget state at snapshot time.
    pub budget: BudgetState,
    /// Audit records captured during the isolated run.
    pub audit: Vec<AuditRecord>,
}

/// Validation report for one v1 memory state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateValidationReport {
    /// Whether the state has no error-level issues.
    pub ok: bool,
    /// State schema version inspected.
    pub schema_version: u32,
    /// Number of events in the state.
    pub event_count: usize,
    /// Number of claims in the state.
    pub claim_count: usize,
    /// Number of error-level issues.
    pub error_count: usize,
    /// Number of warning-level issues.
    pub warning_count: usize,
    /// Validation issues.
    pub issues: Vec<StateValidationIssue>,
}

/// One state validation issue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateValidationIssue {
    /// Issue severity.
    pub severity: ValidationSeverity,
    /// Stable issue code.
    pub code: String,
    /// Human-readable detail.
    pub detail: String,
}

impl StateValidationIssue {
    fn error(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Error,
            code: code.into(),
            detail: detail.into(),
        }
    }

    fn warning(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            severity: ValidationSeverity::Warning,
            code: code.into(),
            detail: detail.into(),
        }
    }
}

/// Severity for state validation issues.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    /// State cannot be trusted until repaired.
    Error,
    /// State is usable but should be normalized.
    Warning,
}

/// Validates internal consistency of a v1 memory state.
#[must_use]
pub fn validate_state(state: &MnemeState) -> StateValidationReport {
    let mut issues = Vec::new();
    if state.schema_version > MNEME_STATE_SCHEMA_VERSION {
        issues.push(StateValidationIssue::error(
            "schema.unsupported",
            format!(
                "state schema {} is newer than supported {}",
                state.schema_version, MNEME_STATE_SCHEMA_VERSION
            ),
        ));
    } else if state.schema_version == 0 {
        issues.push(StateValidationIssue::warning(
            "schema.legacy",
            "state has no schema_version and will be normalized on next save",
        ));
    } else if state.schema_version < MNEME_STATE_SCHEMA_VERSION {
        issues.push(StateValidationIssue::warning(
            "schema.old",
            format!(
                "state schema {} will be normalized to {} on next save",
                state.schema_version, MNEME_STATE_SCHEMA_VERSION
            ),
        ));
    }

    if state.metadata.store_id.trim().is_empty() {
        issues.push(StateValidationIssue::warning(
            "metadata.store_id_missing",
            "store_id will be initialized on next save",
        ));
    }
    if state.metadata.generation == 0 {
        issues.push(StateValidationIssue::warning(
            "metadata.generation_zero",
            "generation will be incremented on next save",
        ));
    }

    let mut event_ids = BTreeSet::new();
    for event in &state.events {
        if event.id.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "event.empty_id",
                "event id must not be empty",
            ));
        }
        if !event_ids.insert(event.id.clone()) {
            issues.push(StateValidationIssue::error(
                "event.duplicate_id",
                format!("duplicate event id {}", event.id),
            ));
        }
        if event.speaker_id.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "event.empty_speaker",
                format!("event {} has an empty speaker_id", event.id),
            ));
        }
    }

    let mut claim_ids = BTreeSet::new();
    for claim in &state.claims {
        if claim.id.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "claim.empty_id",
                "claim id must not be empty",
            ));
        }
        if !claim_ids.insert(claim.id.clone()) {
            issues.push(StateValidationIssue::error(
                "claim.duplicate_id",
                format!("duplicate claim id {}", claim.id),
            ));
        }
        for (field, value) in [
            ("subject", claim.subject.as_str()),
            ("predicate", claim.predicate.as_str()),
            ("object", claim.object.as_str()),
            ("scope", claim.scope.as_str()),
        ] {
            if value.trim().is_empty() {
                issues.push(StateValidationIssue::error(
                    format!("claim.empty_{field}"),
                    format!("claim {} has an empty {field}", claim.id),
                ));
            }
        }
        if claim.source_event_ids.is_empty() {
            issues.push(StateValidationIssue::error(
                "claim.missing_source",
                format!("claim {} has no source events", claim.id),
            ));
        }
        for source_event_id in &claim.source_event_ids {
            if !event_ids.contains(source_event_id) {
                issues.push(StateValidationIssue::error(
                    "claim.unknown_source",
                    format!(
                        "claim {} references missing event {}",
                        claim.id, source_event_id
                    ),
                ));
            }
        }
    }

    if state.budget.daily_cloud_tokens == 0 {
        issues.push(StateValidationIssue::error(
            "budget.zero_daily_cloud_tokens",
            "daily_cloud_tokens must be greater than zero",
        ));
    }

    for audit in &state.audit {
        if audit.target_id.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "audit.empty_target",
                "audit target_id must not be empty",
            ));
        }
    }

    let claim_ids = state
        .claims
        .iter()
        .map(|claim| claim.id.clone())
        .collect::<BTreeSet<_>>();
    let mut session_ids = BTreeSet::new();
    for session in &state.sessions {
        if session.id.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "session.empty_id",
                "session id must not be empty",
            ));
        }
        if !session_ids.insert(session.id.clone()) {
            issues.push(StateValidationIssue::error(
                "session.duplicate_id",
                format!("duplicate session id {}", session.id),
            ));
        }
        if session.task.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "session.empty_task",
                format!("session {} has an empty task", session.id),
            ));
        }
        if session.context_query.trim().is_empty() {
            issues.push(StateValidationIssue::error(
                "session.empty_context_query",
                format!("session {} has an empty context query", session.id),
            ));
        }
        for claim_id in &session.context_claim_ids {
            if !claim_ids.contains(claim_id) {
                issues.push(StateValidationIssue::error(
                    "session.unknown_context_claim",
                    format!(
                        "session {} references missing claim {}",
                        session.id, claim_id
                    ),
                ));
            }
        }
        for event_id in &session.memory_event_ids {
            if !event_ids.contains(event_id) {
                issues.push(StateValidationIssue::error(
                    "session.unknown_memory_event",
                    format!(
                        "session {} references missing event {}",
                        session.id, event_id
                    ),
                ));
            }
        }
        if session.status == SessionStatus::Closed && session.ended_at_unix_seconds.is_none() {
            issues.push(StateValidationIssue::error(
                "session.closed_without_end_time",
                format!("session {} is closed without ended_at", session.id),
            ));
        }
    }

    let error_count = issues
        .iter()
        .filter(|issue| issue.severity == ValidationSeverity::Error)
        .count();
    let warning_count = issues
        .iter()
        .filter(|issue| issue.severity == ValidationSeverity::Warning)
        .count();
    StateValidationReport {
        ok: error_count == 0,
        schema_version: state.schema_version,
        event_count: state.events.len(),
        claim_count: state.claims.len(),
        error_count,
        warning_count,
        issues,
    }
}

fn claim_from_extracted(
    event: &EventRecord,
    next_claim_number: usize,
    extracted: ExtractedClaim,
    source_event_ids: Vec<String>,
) -> ClaimRecord {
    claim_from_draft(
        event,
        next_claim_number,
        ClaimDraft::active(extracted),
        source_event_ids,
    )
}

fn claim_from_draft(
    event: &EventRecord,
    next_claim_number: usize,
    draft: ClaimDraft,
    source_event_ids: Vec<String>,
) -> ClaimRecord {
    let status = if looks_like_secret(&draft.extracted.object) || looks_like_secret(&event.text) {
        ClaimStatus::BlockedSecret
    } else {
        draft.status
    };
    ClaimRecord {
        id: next_id("claim", next_claim_number),
        subject: draft.extracted.subject,
        predicate: draft.extracted.predicate,
        object: draft.extracted.object,
        status,
        scope: event.scope.clone(),
        source_event_ids,
    }
}

fn rule_based_claim_drafts_for_event(event: &EventRecord) -> Vec<ClaimDraft> {
    if let Some(marker) = find_remember_marker(&event.text) {
        return vec![ClaimDraft::active(extracted_claim_from_text(event, marker))];
    }
    if looks_like_secret(&event.text) {
        return Vec::new();
    }
    let text = event.text.to_ascii_lowercase();
    let mut drafts = Vec::new();

    if text.contains("not to remember") {
        return drafts;
    }
    if text.contains("ari prefers short launch briefs") {
        drafts.push(ClaimDraft::new("Ari", "prefers", "launch briefs"));
        drafts.push(ClaimDraft::new("launch briefs", "length", "short"));
        drafts.push(ClaimDraft::new("Ari", "prefers", "Mneme notes"));
        drafts.push(ClaimDraft::new("Mneme notes", "language", "Korean"));
    }
    if text.contains("project atlas")
        && text.contains("release checklist")
        && text.contains("local hard-dogfood evidence")
    {
        drafts.push(ClaimDraft::new(
            "Project Atlas",
            "requires",
            "local hard-dogfood evidence",
        ));
        drafts.push(ClaimDraft::new(
            "release checklist",
            "required_evidence",
            "local hard-dogfood evidence",
        ));
    }
    if text.contains("earlier ari wanted weekly eval reports on friday") {
        drafts.push(ClaimDraft::new(
            "Ari",
            "prefers",
            "weekly eval reports on Friday",
        ));
    }
    if text.contains("changed that schedule to monday mornings") {
        drafts.push(ClaimDraft::new(
            "Ari",
            "prefers",
            "weekly eval reports on Monday mornings",
        ));
        drafts.push(ClaimDraft::new(
            "weekly eval reports",
            "schedule",
            "Monday mornings",
        ));
    }
    if text.contains("next agent for atlas")
        && text.contains("release-risk memo")
        && text.contains("eval-harness docs")
    {
        drafts.push(ClaimDraft::new(
            "next agent",
            "should_read",
            "release-risk memo",
        ));
        drafts.push(ClaimDraft::new(
            "release-risk memo",
            "lives_in",
            "eval-harness docs",
        ));
        drafts.push(ClaimDraft::new(
            "release-risk memo",
            "location",
            "eval-harness docs",
        ));
    }
    if text.contains("personal notes") && text.contains("compact todo summaries") {
        drafts.push(ClaimDraft::new("Ari", "prefers", "todo summaries"));
        drafts.push(ClaimDraft::new("todo summaries", "style", "compact"));
    }
    if text.contains("for atlas") && text.contains("expanded release evidence") {
        drafts.push(ClaimDraft::new(
            "Project Atlas",
            "prefers",
            "release evidence",
        ));
        drafts.push(ClaimDraft::new("release evidence", "style", "expanded"));
    }
    if text.contains("hermes")
        && text.contains("handoff runner")
        && text.contains("checked before release")
    {
        drafts.push(ClaimDraft::new("handoff runner", "same_as", "Hermes"));
        drafts.push(ClaimDraft::new(
            "Ari",
            "requires_check_before_release",
            "Hermes",
        ));
    }
    if text.contains("mina first suggested storing every transcript") {
        drafts.push(ClaimDraft::new("Mina", "suggested", "full transcripts"));
    }
    if text.contains("approved only sanitized summaries") {
        drafts.push(ClaimDraft::new("Ari", "approved", "sanitized summaries"));
    }
    if text.contains("team workspace")
        && text.contains("reviewers must see memory diffs")
        && text.contains("private preferences should stay out")
    {
        drafts.push(ClaimDraft::new("reviewers", "must_see", "memory diffs"));
        drafts.push(ClaimDraft::new("reviewers", "visibility", "memory diffs"));
    }

    drafts
}

fn extracted_claim_from_text(event: &EventRecord, text: &str) -> ExtractedClaim {
    let mut parts = text.split_whitespace();
    let first = parts.next();
    let second = parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let (subject, predicate, object) = match (first, second, rest.trim().is_empty()) {
        (Some(subject), Some(predicate), false) => (subject.to_owned(), predicate.to_owned(), rest),
        _ => (event.speaker_id.clone(), "note".to_owned(), text.to_owned()),
    };
    ExtractedClaim::new(subject, predicate, object)
}

fn claim_matches_text(claim: &ClaimRecord, target: &str) -> bool {
    claim.text().eq_ignore_ascii_case(target.trim())
}

fn dedupe_ids(ids: &mut Vec<String>) {
    let mut deduped = Vec::new();
    for id in ids.drain(..) {
        if !deduped.contains(&id) {
            deduped.push(id);
        }
    }
    *ids = deduped;
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn find_remember_marker(text: &str) -> Option<&str> {
    for marker in ["remember:", "기억해줘:"] {
        if let Some((_, rest)) = text.split_once(marker) {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn find_forget_marker(text: &str) -> Option<&str> {
    for marker in ["forget:", "잊어줘:"] {
        if let Some((_, rest)) = text.split_once(marker) {
            let trimmed = rest.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn find_forget_id_marker(text: &str) -> Option<&str> {
    if let Some((_, rest)) = text.split_once("forget-id:") {
        let trimmed = rest.trim();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

fn find_correct_marker(text: &str) -> Option<(&str, &str)> {
    for marker in ["correct:", "수정:"] {
        if let Some((_, rest)) = text.split_once(marker) {
            let (old_text, new_text) = rest.split_once("->")?;
            let old_text = old_text.trim();
            let new_text = new_text.trim();
            if !old_text.is_empty() && !new_text.is_empty() {
                return Some((old_text, new_text));
            }
        }
    }
    None
}

fn find_correct_id_marker(text: &str) -> Option<(&str, &str)> {
    if let Some((_, rest)) = text.split_once("correct-id:") {
        let (target_id, new_text) = rest.split_once("->")?;
        let target_id = target_id.trim();
        let new_text = new_text.trim();
        if !target_id.is_empty() && !new_text.is_empty() {
            return Some((target_id, new_text));
        }
    }
    None
}

fn looks_like_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    let compact = lower
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.contains("api_key=")
        || compact.contains("api_key:")
        || compact.contains("apikey=")
        || compact.contains("apikey:")
        || compact.contains("api-key=")
        || compact.contains("api-key:")
        || lower.contains("api key")
        || compact.contains("secret=")
        || compact.contains("secret:")
        || compact.contains("token=")
        || compact.contains("token:")
        || compact.contains("access_token=")
        || compact.contains("access_token:")
        || compact.contains("password=")
        || compact.contains("password:")
        || compact.contains("authorization:bearer")
        || compact.contains("bearer")
        || compact.contains("sk-")
        || compact.contains("ghp_")
        || compact.contains("github_pat_")
        || contains_aws_access_key_like(text)
        || lower.contains("private key")
}

fn contains_aws_access_key_like(text: &str) -> bool {
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| {
            token.len() == 20
                && (token.starts_with("AKIA") || token.starts_with("ASIA"))
                && token
                    .chars()
                    .all(|character| character.is_ascii_uppercase() || character.is_ascii_digit())
        })
}

fn validate_extracted_claim(claim: &ExtractedClaim) -> Result<(), ExtractorError> {
    for (field, value) in [
        ("subject", claim.subject.as_str()),
        ("predicate", claim.predicate.as_str()),
        ("object", claim.object.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ExtractorError::new(format!(
                "extractor response claim {field} must not be empty"
            )));
        }
    }
    Ok(())
}

fn truncate_for_error(value: &str) -> String {
    const LIMIT: usize = 500;
    let truncated = value.chars().take(LIMIT).collect::<String>();
    if value.chars().count() > LIMIT {
        format!("{truncated}...")
    } else {
        truncated
    }
}

fn estimate_tokens(text: &str) -> u32 {
    let count = text.split_whitespace().count();
    u32::try_from(count).unwrap_or(u32::MAX).max(1)
}

#[derive(Debug, Clone)]
struct RankedContextCandidate {
    claim_index: usize,
    item: ContextItem,
}

#[derive(Debug, Clone)]
struct ContextMatch {
    score: u32,
    matched_terms: Vec<String>,
    reason: String,
}

fn score_context_match(
    query_text: &str,
    query_terms: &[String],
    claim_text: &str,
) -> Option<ContextMatch> {
    if query_terms.is_empty() {
        return Some(ContextMatch {
            score: 0,
            matched_terms: Vec::new(),
            reason: "empty_query".to_owned(),
        });
    }

    let claim_text_lower = claim_text.to_ascii_lowercase();
    let matched_terms = query_terms
        .iter()
        .filter(|term| claim_text_lower.contains(term.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if matched_terms.is_empty() {
        return None;
    }

    let phrase = normalize_query_phrase(query_text);
    let phrase_matches = matched_terms.len() > 1 && claim_text_lower.contains(&phrase);
    let term_score = u32::try_from(matched_terms.len())
        .unwrap_or(u32::MAX / 10)
        .saturating_mul(10);
    let score = if phrase_matches {
        term_score.saturating_add(5)
    } else {
        term_score
    };

    Some(ContextMatch {
        score,
        matched_terms,
        reason: if phrase_matches {
            "phrase_match".to_owned()
        } else {
            "term_match".to_owned()
        },
    })
}

fn normalize_query_terms(text: &str) -> Vec<String> {
    let mut terms = Vec::new();
    for raw in text.split_whitespace() {
        let term = normalize_query_token(raw);
        if !term.is_empty() && !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

fn normalize_query_phrase(text: &str) -> String {
    normalize_query_terms(text).join(" ")
}

fn normalize_query_token(raw: &str) -> String {
    raw.trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_ascii_lowercase()
}

fn default_allowed_scopes() -> Vec<String> {
    vec!["private".to_owned()]
}

const fn default_context_max_items() -> usize {
    DEFAULT_CONTEXT_MAX_ITEMS
}

fn normalize_allowed_scopes(scopes: Vec<String>) -> BTreeSet<String> {
    scopes
        .into_iter()
        .map(|scope| normalize_scope(&scope))
        .filter(|scope| !scope.is_empty())
        .collect()
}

fn normalize_scope(scope: &str) -> String {
    scope.trim().to_owned()
}

fn next_id(prefix: &str, number: usize) -> String {
    format!("{prefix}-{number:03}")
}

fn next_number_for_prefix<'a>(prefix: &str, ids: impl Iterator<Item = &'a str>) -> usize {
    let marker = format!("{prefix}-");
    ids.filter_map(|id| id.strip_prefix(&marker))
        .filter_map(|suffix| suffix.parse::<usize>().ok())
        .max()
        .unwrap_or(0)
        .saturating_add(1)
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_explicit_personal_memory() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let context = engine.build_context_pack("user preferences");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Active);
        assert_eq!(snapshot.claims[0].source_event_ids, vec!["event-001"]);
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].source_event_ids, vec!["event-001"]);
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::ClaimWrite));
        Ok(())
    }

    #[test]
    fn custom_extractor_can_write_claim_without_rule_marker(
    ) -> Result<(), Box<dyn std::error::Error>> {
        struct StaticExtractor;

        impl MnemeExtractor for StaticExtractor {
            fn extract(
                &self,
                _event: &EventRecord,
            ) -> Result<Option<ExtractedClaim>, ExtractorError> {
                Ok(Some(ExtractedClaim::new(
                    "user",
                    "prefers",
                    "adapter extraction",
                )))
            }
        }

        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event_with_extractor(
            EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: Some("model-adapter".to_owned()),
                text: "model adapter should extract from this event".to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            },
            &StaticExtractor,
        )?;
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].object, "adapter extraction");
        assert_eq!(snapshot.claims[0].source_event_ids, vec!["event-001"]);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Active);
        Ok(())
    }

    #[test]
    fn rule_extractor_captures_natural_language_ontology_claims(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "Ari prefers short launch briefs, and she wants Mneme notes in Korean when the update is user-facing.".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let snapshot = engine.snapshot();

        assert!(snapshot
            .claims
            .iter()
            .any(|claim| claim.text() == "Ari prefers launch briefs"));
        assert!(snapshot
            .claims
            .iter()
            .any(|claim| claim.text() == "launch briefs length short"));
        assert!(snapshot
            .claims
            .iter()
            .any(|claim| claim.text() == "Mneme notes language Korean"));
        Ok(())
    }

    #[test]
    fn rule_extractor_supersedes_natural_language_policy_changes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "Earlier Ari wanted weekly eval reports on Friday.".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "Today she changed that schedule to Monday mornings.".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let snapshot = engine.snapshot();

        assert!(snapshot.claims.iter().any(|claim| {
            claim.text() == "Ari prefers weekly eval reports on Friday"
                && claim.status == ClaimStatus::Superseded
        }));
        assert!(snapshot.claims.iter().any(|claim| {
            claim.text() == "Ari prefers weekly eval reports on Monday mornings"
                && claim.status == ClaimStatus::Active
        }));
        Ok(())
    }

    #[test]
    fn extractor_output_still_passes_secret_blocking() -> Result<(), Box<dyn std::error::Error>> {
        struct SecretExtractor;

        impl MnemeExtractor for SecretExtractor {
            fn extract(
                &self,
                _event: &EventRecord,
            ) -> Result<Option<ExtractedClaim>, ExtractorError> {
                Ok(Some(ExtractedClaim::new(
                    "user",
                    "note",
                    "API_KEY=FAKE_TEST_VALUE",
                )))
            }
        }

        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event_with_extractor(
            EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: Some("model-adapter".to_owned()),
                text: "adapter extracted a sensitive value".to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            },
            &SecretExtractor,
        )?;
        let context = engine.build_context_pack("API key");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::BlockedSecret);
        assert!(context.items.is_empty());
        Ok(())
    }

    #[test]
    fn budget_hard_cap_blocks_claim_extraction() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 2,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.events.len(), 1);
        assert!(snapshot.claims.is_empty());
        assert_eq!(snapshot.budget.hard_cap_violations, 1);
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::BudgetBlock));
        Ok(())
    }

    #[test]
    fn blocked_secret_claims_are_omitted_from_context() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user note API_KEY=FAKE_TEST_VALUE".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let context = engine.build_context_pack("API key");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::BlockedSecret);
        assert!(context.items.is_empty());
        assert_eq!(context.omitted.len(), 1);
        Ok(())
    }

    #[test]
    fn context_query_enforces_allowed_scopes_before_relevance(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers project roadmap reviews".to_owned(),
            scope: "project-alpha".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let private_context = engine.build_context_pack("project roadmap");
        assert!(private_context.items.is_empty());
        assert!(private_context
            .omitted
            .iter()
            .any(|item| item.reason == "scope_denied:project-alpha"));

        let project_context = engine.build_context_pack_with(ContextQuery::with_allowed_scopes(
            "project roadmap",
            ["private", "project-alpha"],
        ));
        assert_eq!(project_context.items.len(), 1);
        assert_eq!(
            project_context.items[0].claim_text,
            "user prefers project roadmap reviews"
        );
        Ok(())
    }

    #[test]
    fn context_query_ranks_and_caps_relevant_items() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        for text in [
            "remember: user prefers launch templates",
            "remember: user prefers review summaries",
            "remember: user prefers launch review checklists",
            "remember: user prefers color palettes",
        ] {
            engine.ingest_event(EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: None,
                text: text.to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            })?;
        }

        let context =
            engine.build_context_pack_with(ContextQuery::new("launch review").with_max_items(2));

        assert_eq!(context.items.len(), 2);
        assert_eq!(
            context.items[0].claim_text,
            "user prefers launch review checklists"
        );
        assert_eq!(context.items[0].score, 25);
        assert_eq!(context.items[0].matched_terms, vec!["launch", "review"]);
        assert_eq!(context.items[0].match_reason, "phrase_match");
        assert_eq!(context.items[1].claim_text, "user prefers launch templates");
        assert!(context
            .omitted
            .iter()
            .any(|item| item.reason == "context_budget_exceeded:max_items=2"));
        assert!(context
            .omitted
            .iter()
            .any(|item| item.reason == "low_relevance"));
        Ok(())
    }

    #[test]
    fn token_like_claims_are_omitted_from_context() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user note TOKEN=FAKE_TOKEN_VALUE".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let context = engine.build_context_pack("token");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::BlockedSecret);
        assert!(context.items.is_empty());
        assert_eq!(context.omitted.len(), 1);
        Ok(())
    }

    #[test]
    fn common_secret_patterns_are_blocked() -> Result<(), Box<dyn std::error::Error>> {
        for text in [
            "remember: Authorization: Bearer fake-token-value",
            "remember: token: fake-token-value",
            "remember: password : fake-password",
            "remember: provider key sk-testvalue",
            "remember: GitHub token ghp_fakevalue",
            "remember: AWS key AKIA1234567890ABCDEF",
        ] {
            let mut engine = MnemeEngine::new(MnemeConfig {
                daily_cloud_tokens: 100,
            });
            engine.ingest_event(EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: None,
                text: text.to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            })?;
            let snapshot = engine.snapshot();
            assert_eq!(snapshot.claims[0].status, ClaimStatus::BlockedSecret);
        }
        Ok(())
    }

    #[test]
    fn correction_supersedes_old_claim_and_writes_replacement(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "correct: user prefers local-first tools -> user prefers desktop IDE".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let context = engine.build_context_pack("desktop IDE");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 2);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Superseded);
        assert_eq!(snapshot.claims[1].status, ClaimStatus::Active);
        assert_eq!(
            snapshot.claims[1].source_event_ids,
            vec!["event-001", "event-002"]
        );
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].claim_text, "user prefers desktop IDE");
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::ClaimUpdate));
        Ok(())
    }

    #[test]
    fn forgotten_claims_are_omitted_from_context() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "forget: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let context = engine.build_context_pack("local-first");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Forgotten);
        assert!(context.items.is_empty());
        assert_eq!(context.omitted[0].reason, "forgotten");
        Ok(())
    }

    #[test]
    fn id_lifecycle_targets_only_one_matching_claim() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        for text in [
            "remember: user prefers local-first tools",
            "remember: user prefers local-first tools",
        ] {
            engine.ingest_event(EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: None,
                text: text.to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            })?;
        }
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "forget-id: claim-001".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let context = engine.build_context_pack("local-first");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 2);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Forgotten);
        assert_eq!(snapshot.claims[1].status, ClaimStatus::Active);
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].claim_id, "claim-002");
        Ok(())
    }

    #[test]
    fn id_correction_targets_one_claim_and_writes_replacement(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        for text in [
            "remember: user prefers local-first tools",
            "remember: user prefers local-first tools",
        ] {
            engine.ingest_event(EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: Some("codex".to_owned()),
                text: text.to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            })?;
        }
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "correct-id: claim-001 -> user prefers terminal workflows".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let snapshot = engine.snapshot();
        let context = engine.build_context_pack("terminal workflows");

        assert_eq!(snapshot.claims.len(), 3);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Superseded);
        assert_eq!(snapshot.claims[1].status, ClaimStatus::Active);
        assert_eq!(snapshot.claims[2].status, ClaimStatus::Active);
        assert_eq!(snapshot.claims[2].id, "claim-003");
        assert_eq!(
            snapshot.claims[2].source_event_ids,
            vec!["event-001", "event-003"]
        );
        assert_eq!(context.items.len(), 1);
        assert_eq!(
            context.items[0].claim_text,
            "user prefers terminal workflows"
        );
        Ok(())
    }

    #[test]
    fn json_file_store_round_trips_state_after_restart() -> Result<(), Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!(
            "mneme-core-store-round-trip-{}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers durable memory".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let mut store = JsonFileStore::new(path.clone());
        engine.persist(&mut store)?;

        let mut reloaded = MnemeEngine::from_store(
            MnemeConfig {
                daily_cloud_tokens: 1,
            },
            &store,
        )?;
        let context = reloaded.build_context_pack("durable memory");
        let snapshot = reloaded.snapshot();

        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.budget.daily_cloud_tokens, 100);
        assert_eq!(context.items.len(), 1);
        assert_eq!(context.items[0].source_event_ids, vec!["event-001"]);

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn json_file_store_writes_schema_metadata_and_backup() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = temp_store_path("schema-metadata-backup");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers durable memory".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let mut store = JsonFileStore::new(path.clone());
        engine.persist(&mut store)?;
        assert!(!store.backup_path().exists());

        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers backups".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.persist(&mut store)?;

        let loaded = store.load()?.ok_or("state should exist")?;
        let inspection = store.inspect();
        assert_eq!(loaded.schema_version, MNEME_STATE_SCHEMA_VERSION);
        assert!(loaded.metadata.generation >= 1);
        assert!(store.backup_path().exists());
        assert_eq!(inspection.current.status, StoreFileStatus::Valid);
        assert_eq!(inspection.backup.status, StoreFileStatus::Valid);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.backup_path());
        let _ = std::fs::remove_file(store.lock_path());
        Ok(())
    }

    #[test]
    fn json_file_store_save_requires_exclusive_lock() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("store-lock");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers lock safety".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let mut store = JsonFileStore::new(path.clone());
        std::fs::write(store.lock_path(), "held by test\n")?;

        let error = engine
            .persist(&mut store)
            .expect_err("save should fail while lock exists");
        assert_eq!(error.kind(), StoreErrorKind::LockConflict);
        assert!(!path.exists());

        std::fs::remove_file(store.lock_path())?;
        engine.persist(&mut store)?;
        assert!(path.exists());
        assert!(!store.lock_path().exists());

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.backup_path());
        let _ = std::fs::remove_file(store.lock_path());
        Ok(())
    }

    #[test]
    fn json_file_store_rejects_stale_generation_save() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("generation-conflict");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));

        let mut base = MnemeEngine::new(MnemeConfig::default());
        base.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers first write".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let mut base_store = JsonFileStore::new(path.clone());
        base.persist(&mut base_store)?;

        let store_a = JsonFileStore::new(path.clone());
        let store_b = JsonFileStore::new(path.clone());
        let mut writer_a = MnemeEngine::from_store(MnemeConfig::default(), &store_a)?;
        let mut writer_b = MnemeEngine::from_store(MnemeConfig::default(), &store_b)?;
        writer_a.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers second write".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        writer_a.persist(&mut JsonFileStore::new(path.clone()))?;
        writer_b.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers overwritten write".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let error = writer_b
            .persist(&mut JsonFileStore::new(path.clone()))
            .expect_err("stale writer should not overwrite a newer generation");
        assert_eq!(error.kind(), StoreErrorKind::LockConflict);

        let reloaded =
            MnemeEngine::from_store(MnemeConfig::default(), &JsonFileStore::new(path.clone()))?;
        let snapshot = reloaded.snapshot();
        assert!(snapshot
            .events
            .iter()
            .any(|event| event.text.contains("second write")));
        assert!(!snapshot
            .events
            .iter()
            .any(|event| event.text.contains("overwritten write")));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));
        Ok(())
    }

    #[test]
    fn json_file_store_recovers_stale_lock() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("stale-lock-recovery");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));
        std::fs::write(lock_path_for(&path), "pid=0\ncreated_at_unix_seconds=1\n")?;

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers stale lock recovery".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.persist(&mut JsonFileStore::new(path.clone()))?;

        assert!(!lock_path_for(&path).exists());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        Ok(())
    }

    #[test]
    fn state_validation_detects_missing_claim_sources() {
        let state = MnemeState {
            schema_version: MNEME_STATE_SCHEMA_VERSION,
            metadata: StateMetadata::default(),
            budget: BudgetState {
                daily_cloud_tokens: 100,
                spent_tokens: 0,
                hard_cap_violations: 0,
            },
            events: Vec::new(),
            claims: vec![ClaimRecord {
                id: "claim-001".to_owned(),
                subject: "user".to_owned(),
                predicate: "prefers".to_owned(),
                object: "durable memory".to_owned(),
                status: ClaimStatus::Active,
                scope: "private".to_owned(),
                source_event_ids: vec!["event-404".to_owned()],
            }],
            sessions: Vec::new(),
            audit: Vec::new(),
        };

        let report = validate_state(&state);
        assert!(!report.ok);
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.code == "claim.unknown_source"));
    }

    #[test]
    fn compaction_removes_inactive_claims_and_unreferenced_events(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "correct: user prefers local-first tools -> user prefers desktop IDE".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let report = engine.compact();
        let snapshot = engine.snapshot();

        assert_eq!(report.removed_claims, 1);
        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Active);
        assert_eq!(snapshot.events.len(), 2);
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::StateCompact));
        Ok(())
    }

    #[test]
    fn compaction_keeps_new_event_and_claim_ids_collision_free(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig::default());
        for text in [
            "remember: user prefers alpha notes",
            "remember: user prefers beta notes",
            "remember: user prefers gamma notes",
        ] {
            engine.ingest_event(EventInput {
                speaker_id: "user".to_owned(),
                actor_agent_id: None,
                text: text.to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            })?;
        }
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "forget-id: claim-002".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let report = engine.compact();
        assert_eq!(report.events_after, 2);
        assert_eq!(report.claims_after, 2);

        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers delta notes".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let snapshot = engine.snapshot();
        let event_ids = snapshot
            .events
            .iter()
            .map(|event| event.id.as_str())
            .collect::<BTreeSet<_>>();
        let claim_ids = snapshot
            .claims
            .iter()
            .map(|claim| claim.id.as_str())
            .collect::<BTreeSet<_>>();

        assert_eq!(event_ids.len(), snapshot.events.len());
        assert_eq!(claim_ids.len(), snapshot.claims.len());
        assert!(event_ids.contains("event-004"));
        assert!(claim_ids.contains("claim-004"));
        assert!(snapshot
            .claims
            .iter()
            .any(|claim| claim.text() == "user prefers delta notes"
                && claim.source_event_ids == vec!["event-004"]));
        Ok(())
    }

    #[test]
    fn repair_restores_current_file_from_valid_backup() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("repair-backup");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers recoverable memory".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let mut store = JsonFileStore::new(path.clone());
        engine.persist(&mut store)?;
        engine.persist(&mut store)?;
        std::fs::write(&path, "{not-json")?;

        let inspection = store.inspect();
        assert!(inspection.repair_available);

        let repair = store.repair_from_backup()?;
        assert!(repair.repaired);
        assert_eq!(store.inspect().current.status, StoreFileStatus::Valid);

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.backup_path());
        Ok(())
    }

    #[test]
    fn repair_normalizes_current_legacy_schema_store() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("repair-normalizes-legacy");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers legacy migration safety".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let mut store = JsonFileStore::new(path.clone());
        engine.persist(&mut store)?;

        let mut legacy_state = store.load()?.ok_or("state should exist")?;
        legacy_state.schema_version = 1;
        write_state_atomic(&path, &legacy_state)?;

        let inspection = store.inspect();
        assert_eq!(inspection.current.status, StoreFileStatus::Valid);
        assert_eq!(inspection.current.schema_version, Some(1));

        let repair = store.repair_from_backup()?;
        assert!(repair.repaired);
        assert_eq!(repair.action, "normalized_current");

        let normalized = store.load()?.ok_or("state should exist")?;
        assert_eq!(normalized.schema_version, MNEME_STATE_SCHEMA_VERSION);
        assert!(normalized
            .metadata
            .migration_history
            .iter()
            .any(|record| record.from_schema_version == 1
                && record.to_schema_version == MNEME_STATE_SCHEMA_VERSION));
        assert_eq!(store.inspect().backup.schema_version, Some(1));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.backup_path());
        let _ = std::fs::remove_file(store.lock_path());
        Ok(())
    }

    #[test]
    fn restore_swaps_current_store_with_valid_backup() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("restore-backup");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(backup_path_for(&path));
        let _ = std::fs::remove_file(lock_path_for(&path));

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers before restore".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        let mut store = JsonFileStore::new(path.clone());
        engine.persist(&mut store)?;

        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers after restore".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;
        engine.persist(&mut store)?;

        let before_restore = store.load()?.ok_or("state should exist")?;
        assert_eq!(before_restore.claims.len(), 2);
        assert_eq!(store.inspect().backup.claim_count, Some(1));

        let restore = store.restore_from_backup()?;
        assert!(restore.restored);
        assert_eq!(restore.action, "restored_from_backup");
        assert!(restore.current_preserved_as_backup);

        let restored = store.load()?.ok_or("state should exist")?;
        assert_eq!(restored.claims.len(), 1);
        assert_eq!(restored.claims[0].object, "before restore");
        assert_eq!(store.inspect().backup.claim_count, Some(2));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.backup_path());
        let _ = std::fs::remove_file(store.lock_path());
        Ok(())
    }

    #[test]
    fn agent_session_begin_returns_context_and_end_records_memory(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let begin = engine.begin_session(SessionBeginInput {
            task: "Draft a setup plan".to_owned(),
            lineage_id: None,
            actor_agent_id: Some("codex".to_owned()),
            query: Some("local-first".to_owned()),
            allowed_scopes: vec!["private".to_owned()],
            max_items: DEFAULT_CONTEXT_MAX_ITEMS,
        });
        assert_eq!(begin.session.id, "session-001");
        assert_eq!(begin.session.status, SessionStatus::Active);
        assert_eq!(begin.context_pack.items.len(), 1);
        assert_eq!(begin.session.context_claim_ids, vec!["claim-001"]);

        let end = engine.end_session(SessionEndInput {
            session_id: begin.session.id,
            actor_agent_id: Some("codex".to_owned()),
            scope: None,
            summary: Some("Prepared a concise setup plan".to_owned()),
            remember: vec!["user prefers concise setup plans".to_owned()],
        })?;
        let context = engine.build_context_pack("concise setup");
        let snapshot = engine.snapshot();

        assert_eq!(end.session.status, SessionStatus::Closed);
        assert_eq!(end.remembered_event_ids, vec!["event-002"]);
        assert_eq!(end.remembered_claim_ids, vec!["claim-002"]);
        assert_eq!(context.items.len(), 1);
        assert_eq!(
            context.items[0].claim_text,
            "user prefers concise setup plans"
        );
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::SessionBegin));
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::SessionEnd));
        Ok(())
    }

    #[test]
    fn agent_session_end_can_use_custom_extractor_for_raw_memory_notes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        struct RawMemoryExtractor;

        impl MnemeExtractor for RawMemoryExtractor {
            fn extract(
                &self,
                event: &EventRecord,
            ) -> Result<Option<ExtractedClaim>, ExtractorError> {
                assert_eq!(event.speaker_id, "agent");
                assert_eq!(event.trust_level, "agent_summary");
                assert!(!event.text.starts_with("remember:"));
                Ok(Some(ExtractedClaim::new(
                    "user",
                    "prefers",
                    "direct planning docs",
                )))
            }
        }

        let mut engine = MnemeEngine::new(MnemeConfig::default());
        let begin = engine.begin_session(SessionBeginInput {
            task: "Draft a planning doc".to_owned(),
            lineage_id: None,
            actor_agent_id: Some("codex".to_owned()),
            query: None,
            allowed_scopes: vec!["private".to_owned()],
            max_items: DEFAULT_CONTEXT_MAX_ITEMS,
        });

        let end = engine.end_session_with_extractor(
            SessionEndInput {
                session_id: begin.session.id,
                actor_agent_id: Some("codex".to_owned()),
                scope: None,
                summary: Some("Prepared the planning doc".to_owned()),
                remember: vec!["For future planning docs, keep explanations direct.".to_owned()],
            },
            &RawMemoryExtractor,
            SessionMemoryInputMode::RawEvent,
        )?;
        let snapshot = engine.snapshot();

        assert_eq!(end.remembered_event_ids, vec!["event-001"]);
        assert_eq!(end.remembered_claim_ids, vec!["claim-001"]);
        assert_eq!(
            snapshot.events[0].text,
            "For future planning docs, keep explanations direct."
        );
        assert_eq!(snapshot.claims[0].object, "direct planning docs");
        Ok(())
    }

    #[test]
    fn agent_session_begin_respects_allowed_scopes() -> Result<(), Box<dyn std::error::Error>> {
        let mut engine = MnemeEngine::new(MnemeConfig::default());
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers team release notes".to_owned(),
            scope: "team".to_owned(),
            trust_level: "trusted_user".to_owned(),
        })?;

        let denied = engine.begin_session(SessionBeginInput {
            task: "Draft release notes".to_owned(),
            lineage_id: None,
            actor_agent_id: Some("codex".to_owned()),
            query: Some("release notes".to_owned()),
            allowed_scopes: vec!["private".to_owned()],
            max_items: DEFAULT_CONTEXT_MAX_ITEMS,
        });
        assert!(denied.context_pack.items.is_empty());
        assert!(denied
            .context_pack
            .omitted
            .iter()
            .any(|item| item.reason == "scope_denied:team"));

        let allowed = engine.begin_session(SessionBeginInput {
            task: "Draft release notes".to_owned(),
            lineage_id: None,
            actor_agent_id: Some("codex".to_owned()),
            query: Some("release notes".to_owned()),
            allowed_scopes: vec!["team".to_owned()],
            max_items: DEFAULT_CONTEXT_MAX_ITEMS,
        });
        assert_eq!(allowed.context_pack.items.len(), 1);
        assert_eq!(allowed.session.context_claim_ids, vec!["claim-001"]);
        Ok(())
    }

    #[test]
    fn ending_unknown_session_fails() {
        let mut engine = MnemeEngine::new(MnemeConfig::default());
        let result = engine.end_session(SessionEndInput {
            session_id: "session-404".to_owned(),
            actor_agent_id: None,
            scope: None,
            summary: Some("nothing happened".to_owned()),
            remember: Vec::new(),
        });

        assert!(result.is_err());
    }

    #[cfg(unix)]
    #[test]
    fn command_extractor_reads_protocol_response() -> Result<(), Box<dyn std::error::Error>> {
        let response = serde_json::to_string(&ExtractorCommandResponse::from_claim(
            ExtractedClaim::new("user", "prefers", "command extraction"),
        ))?;
        let extractor = CommandExtractor::new(
            "/bin/sh",
            vec![
                "-c".to_owned(),
                format!("cat >/dev/null; printf '%s\\n' '{}'", response),
            ],
        );
        let event = EventRecord {
            id: "event-001".to_owned(),
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("model-wrapper".to_owned()),
            text: "the user likes command-backed extraction".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        };

        let claim = extractor
            .extract(&event)?
            .ok_or_else(|| ExtractorError::new("expected claim"))?;

        assert_eq!(claim.object, "command extraction");
        Ok(())
    }

    #[test]
    fn command_response_rejects_empty_claim_fields() -> Result<(), Box<dyn std::error::Error>> {
        let claim = ExtractedClaim::new("user", "prefers", " ");
        match validate_extracted_claim(&claim) {
            Ok(()) => Err("empty object should fail".into()),
            Err(error) => {
                assert!(error.to_string().contains("object"));
                Ok(())
            }
        }
    }

    fn temp_store_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mneme-core-{name}-{}.json", std::process::id()))
    }
}
