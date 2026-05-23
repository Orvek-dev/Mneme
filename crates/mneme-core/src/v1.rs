//! Mneme v1 personal-memory core.
//!
//! This module intentionally starts as a deterministic core with a small
//! persistence boundary. It is product code, not an eval fake: the eval harness
//! can drive it through a target adapter, while this crate stays independent of
//! eval fixture types.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Personal-memory engine for Mneme v1.
#[derive(Debug, Clone)]
pub struct MnemeEngine {
    budget: BudgetState,
    events: Vec<EventRecord>,
    claims: Vec<ClaimRecord>,
    audit: Vec<AuditRecord>,
}

impl MnemeEngine {
    /// Creates a new isolated personal-memory engine.
    #[must_use]
    pub fn new(config: MnemeConfig) -> Self {
        Self {
            budget: BudgetState {
                daily_cloud_tokens: config.daily_cloud_tokens,
                spent_tokens: 0,
                hard_cap_violations: 0,
            },
            events: Vec::new(),
            claims: Vec::new(),
            audit: Vec::new(),
        }
    }

    /// Restores an engine from persisted v1 state.
    #[must_use]
    pub fn from_state(state: MnemeState) -> Self {
        Self {
            budget: state.budget,
            events: state.events,
            claims: state.claims,
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
    pub fn ingest_event(&mut self, input: EventInput) {
        let event = EventRecord {
            id: next_id("event", self.events.len() + 1),
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
            return;
        }
        self.budget.spent_tokens = self.budget.spent_tokens.saturating_add(token_estimate);

        if self.apply_lifecycle_event(&event) {
            self.events.push(event);
            return;
        }

        if let Some(claim) = extract_claim(&event, self.claims.len() + 1) {
            self.audit.push(AuditRecord {
                kind: AuditKind::ClaimWrite,
                target_id: claim.id.clone(),
            });
            self.claims.push(claim);
        }

        self.events.push(event);
    }

    fn apply_lifecycle_event(&mut self, event: &EventRecord) -> bool {
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

        let claim = claim_from_marker(event, self.claims.len() + 1, new_text, source_event_ids);
        self.audit.push(AuditRecord {
            kind: AuditKind::ClaimWrite,
            target_id: claim.id.clone(),
        });
        self.claims.push(claim);
    }

    /// Builds a context pack over active claims and records a read audit event.
    pub fn build_context_pack(&mut self, query: impl Into<String>) -> ContextPack {
        let query = query.into();
        let query_terms = query
            .split_whitespace()
            .map(|term| term.to_ascii_lowercase())
            .collect::<Vec<_>>();
        let mut items = Vec::new();
        let mut omitted = Vec::new();

        for claim in &self.claims {
            if claim.status != ClaimStatus::Active {
                omitted.push(OmittedContextItem {
                    claim_id: claim.id.clone(),
                    reason: claim.status.as_str().to_owned(),
                });
                continue;
            }

            let claim_text = claim.text();
            let claim_text_lower = claim_text.to_ascii_lowercase();
            let matches_query = query_terms.is_empty()
                || query_terms
                    .iter()
                    .any(|term| claim_text_lower.contains(term));
            if matches_query {
                items.push(ContextItem {
                    claim_id: claim.id.clone(),
                    claim_text,
                    source_event_ids: claim.source_event_ids.clone(),
                });
            } else {
                omitted.push(OmittedContextItem {
                    claim_id: claim.id.clone(),
                    reason: "low_relevance".to_owned(),
                });
            }
        }

        self.audit.push(AuditRecord {
            kind: AuditKind::ContextRead,
            target_id: if query.is_empty() {
                "empty-query".to_owned()
            } else {
                query
            },
        });

        ContextPack { items, omitted }
    }

    /// Returns the serializable engine state.
    #[must_use]
    pub fn state(&self) -> MnemeState {
        MnemeState {
            budget: self.budget.clone(),
            events: self.events.clone(),
            claims: self.claims.clone(),
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
            events: self.events.clone(),
            claims: self.claims.clone(),
            budget: self.budget.clone(),
            audit: self.audit.clone(),
        }
    }
}

/// Serializable v1 engine state used by persistence adapters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MnemeState {
    /// Budget state at persistence time.
    pub budget: BudgetState,
    /// Events appended before persistence.
    pub events: Vec<EventRecord>,
    /// Claims extracted before persistence.
    pub claims: Vec<ClaimRecord>,
    /// Audit records captured before persistence.
    pub audit: Vec<AuditRecord>,
}

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
}

impl MnemeStore for JsonFileStore {
    fn load(&self) -> Result<Option<MnemeState>, StoreError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text)
                .map(Some)
                .map_err(|source| StoreError::new(format!("failed to parse state: {source}"))),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(StoreError::new(format!("failed to read state: {source}"))),
        }
    }

    fn save(&mut self, state: &MnemeState) -> Result<(), StoreError> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|source| {
                StoreError::new(format!("failed to create store dir: {source}"))
            })?;
        }
        let text = serde_json::to_string_pretty(state)
            .map_err(|source| StoreError::new(format!("failed to encode state: {source}")))?;
        fs::write(&self.path, format!("{text}\n"))
            .map_err(|source| StoreError::new(format!("failed to write state: {source}")))
    }
}

/// Error returned by v1 persistence adapters.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StoreError {
    message: String,
}

impl StoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StoreError {}

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

/// Context item returned to an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Claim that produced this context item.
    pub claim_id: String,
    /// Text form of the claim.
    pub claim_text: String,
    /// Source event IDs cited by this context item.
    pub source_event_ids: Vec<String>,
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
        }
    }
}

/// Snapshot returned to adapters after scenario execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineSnapshot {
    /// Events appended during the isolated run.
    pub events: Vec<EventRecord>,
    /// Claims extracted during the isolated run.
    pub claims: Vec<ClaimRecord>,
    /// Budget state at snapshot time.
    pub budget: BudgetState,
    /// Audit records captured during the isolated run.
    pub audit: Vec<AuditRecord>,
}

fn extract_claim(event: &EventRecord, next_claim_number: usize) -> Option<ClaimRecord> {
    let marker = find_remember_marker(&event.text)?;
    Some(claim_from_marker(
        event,
        next_claim_number,
        marker,
        vec![event.id.clone()],
    ))
}

fn claim_from_marker(
    event: &EventRecord,
    next_claim_number: usize,
    marker: &str,
    source_event_ids: Vec<String>,
) -> ClaimRecord {
    let mut parts = marker.split_whitespace();
    let first = parts.next();
    let second = parts.next();
    let rest = parts.collect::<Vec<_>>().join(" ");
    let (subject, predicate, object) = match (first, second, rest.trim().is_empty()) {
        (Some(subject), Some(predicate), false) => (subject.to_owned(), predicate.to_owned(), rest),
        _ => (
            event.speaker_id.clone(),
            "note".to_owned(),
            marker.to_owned(),
        ),
    };
    let status = if looks_like_secret(&object) || looks_like_secret(&event.text) {
        ClaimStatus::BlockedSecret
    } else {
        ClaimStatus::Active
    };
    ClaimRecord {
        id: next_id("claim", next_claim_number),
        subject,
        predicate,
        object,
        status,
        scope: event.scope.clone(),
        source_event_ids,
    }
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

fn looks_like_secret(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("api_key=")
        || lower.contains("api key")
        || lower.contains("secret=")
        || lower.contains("password=")
}

fn estimate_tokens(text: &str) -> u32 {
    let count = text.split_whitespace().count();
    u32::try_from(count).unwrap_or(u32::MAX).max(1)
}

fn next_id(prefix: &str, number: usize) -> String {
    format!("{prefix}-{number:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_explicit_personal_memory() {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
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
    }

    #[test]
    fn budget_hard_cap_blocks_claim_extraction() {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 2,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.events.len(), 1);
        assert!(snapshot.claims.is_empty());
        assert_eq!(snapshot.budget.hard_cap_violations, 1);
        assert!(snapshot
            .audit
            .iter()
            .any(|event| event.kind == AuditKind::BudgetBlock));
    }

    #[test]
    fn blocked_secret_claims_are_omitted_from_context() {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user note API_KEY=FAKE_TEST_VALUE".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
        let context = engine.build_context_pack("API key");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::BlockedSecret);
        assert!(context.items.is_empty());
        assert_eq!(context.omitted.len(), 1);
    }

    #[test]
    fn correction_supersedes_old_claim_and_writes_replacement() {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: Some("codex".to_owned()),
            text: "correct: user prefers local-first tools -> user prefers desktop IDE".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
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
    }

    #[test]
    fn forgotten_claims_are_omitted_from_context() {
        let mut engine = MnemeEngine::new(MnemeConfig {
            daily_cloud_tokens: 100,
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "remember: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
        engine.ingest_event(EventInput {
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            text: "forget: user prefers local-first tools".to_owned(),
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
        });
        let context = engine.build_context_pack("local-first");
        let snapshot = engine.snapshot();

        assert_eq!(snapshot.claims.len(), 1);
        assert_eq!(snapshot.claims[0].status, ClaimStatus::Forgotten);
        assert!(context.items.is_empty());
        assert_eq!(context.omitted[0].reason, "forgotten");
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
        });

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
}
