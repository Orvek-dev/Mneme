use crate::error::EvalError;
use crate::scenario::{AgentFlow, ContextPackExpected, Scenario};
use crate::target::{
    ActualState, AuditEvent, Claim, ContextItem, ContextPack, EvalTarget, EvalTargetMetadata,
    FaultMode, OmittedItem, RecordedEvent, SessionActual, StoreActual, TargetRunOptions,
};
use mneme_core::DEFAULT_CONTEXT_MAX_ITEMS;

pub(crate) struct FakeEvalTarget;

impl EvalTarget for FakeEvalTarget {
    fn name(&self) -> &'static str {
        "fake"
    }

    fn metadata(&self, _options: &TargetRunOptions) -> EvalTargetMetadata {
        EvalTargetMetadata::fake()
    }

    fn run(
        &self,
        scenario: &Scenario,
        options: TargetRunOptions,
    ) -> Result<ActualState, EvalError> {
        Ok(run_fake_runtime(scenario, options))
    }
}

fn run_fake_runtime(scenario: &Scenario, options: TargetRunOptions) -> ActualState {
    let mut actual = ActualState::default();

    for (idx, input) in scenario.events.iter().enumerate() {
        let event = RecordedEvent {
            id: format!("event-{:03}", idx + 1),
            speaker_id: input.speaker_id.clone(),
            actor_agent_id: input.actor_agent_id.clone(),
            text: input.text.clone(),
            scope: input.scope.clone(),
            trust_level: input.trust_level.clone(),
        };
        actual.audit.push(AuditEvent {
            kind: "event.append".to_owned(),
            target_id: format!(
                "{}:{}:{}",
                event.id,
                event.actor_agent_id.as_deref().unwrap_or("no-agent"),
                event.trust_level
            ),
        });
        let token_estimate = estimate_tokens(&event.text);
        if actual.budget.spent_tokens.saturating_add(token_estimate)
            > scenario.budget.daily_cloud_tokens
        {
            actual.budget.hard_cap_violations = actual.budget.hard_cap_violations.saturating_add(1);
            actual.audit.push(AuditEvent {
                kind: "budget.block".to_owned(),
                target_id: event.id.clone(),
            });
            actual.events.push(event);
            continue;
        }
        actual.budget.spent_tokens = actual.budget.spent_tokens.saturating_add(token_estimate);

        if apply_lifecycle_event(&mut actual, &event) {
            actual.events.push(event);
            continue;
        }

        if options.fault_mode != FaultMode::SkipClaims {
            if let Some(mut claim) = extract_claim(&event, actual.claims.len() + 1) {
                if options.fault_mode == FaultMode::LeakSecrets && claim.status == "blocked_secret"
                {
                    claim.status = "active".to_owned();
                }
                actual.audit.push(AuditEvent {
                    kind: "claim.write".to_owned(),
                    target_id: claim.id.clone(),
                });
                actual.claims.push(claim);
            }
        }
        actual.events.push(event);
    }

    let mut compacted = false;
    if scenario.maintenance.compact_after_events {
        compact_actual(&mut actual);
        compacted = true;
    }

    if let Some(agent_flow) = &scenario.agent_flow {
        apply_agent_flow(&mut actual, agent_flow, options.clone());
    }

    if let Some(context_expected) = &scenario.expected.context_pack {
        actual.context_pack = Some(build_context_pack(
            &actual.claims,
            context_expected,
            options,
        ));
        actual.audit.push(AuditEvent {
            kind: "context.read".to_owned(),
            target_id: scenario.id.clone(),
        });
    }

    if scenario.expected.store.is_some()
        || scenario.maintenance.export_import_roundtrip
        || scenario.maintenance.compact_after_events
        || scenario.maintenance.repair_from_backup
    {
        actual.store = Some(StoreActual {
            schema_version: Some(mneme_core::MNEME_STATE_SCHEMA_VERSION),
            valid: true,
            backup_present: scenario.maintenance.repair_from_backup
                || scenario.maintenance.export_import_roundtrip,
            repair_performed: scenario.maintenance.repair_from_backup,
            compacted,
            imported: scenario.maintenance.export_import_roundtrip,
            generation: Some(1),
            error_count: 0,
        });
    }

    actual
}

fn apply_agent_flow(actual: &mut ActualState, agent_flow: &AgentFlow, options: TargetRunOptions) {
    let query = agent_flow
        .begin
        .query
        .clone()
        .unwrap_or_else(|| agent_flow.begin.task.clone());
    let context = build_context_pack_for_query(
        &actual.claims,
        &query,
        &agent_flow.begin.allowed_scopes,
        options.clone(),
    );
    let mut session = SessionActual {
        id: format!("session-{:03}", actual.sessions.len() + 1),
        task: agent_flow.begin.task.clone(),
        actor_agent_id: agent_flow.begin.actor_agent_id.clone(),
        status: "active".to_owned(),
        context_claim_ids: context
            .items
            .iter()
            .map(|item| item.claim_id.clone())
            .collect(),
        summary: None,
        memory_event_ids: Vec::new(),
    };
    actual.audit.push(AuditEvent {
        kind: "context.read".to_owned(),
        target_id: query,
    });
    actual.audit.push(AuditEvent {
        kind: "session.begin".to_owned(),
        target_id: session.id.clone(),
    });

    if let Some(end) = &agent_flow.end {
        for remembered in &end.remember {
            let event = RecordedEvent {
                id: format!("event-{:03}", actual.events.len() + 1),
                speaker_id: "agent".to_owned(),
                actor_agent_id: agent_flow.begin.actor_agent_id.clone(),
                text: format!("remember: {remembered}"),
                scope: "private".to_owned(),
                trust_level: "agent_summary".to_owned(),
            };
            actual.audit.push(AuditEvent {
                kind: "event.append".to_owned(),
                target_id: format!(
                    "{}:{}:{}",
                    event.id,
                    event.actor_agent_id.as_deref().unwrap_or("no-agent"),
                    event.trust_level
                ),
            });
            if options.fault_mode != FaultMode::SkipClaims {
                if let Some(mut claim) = extract_claim(&event, actual.claims.len() + 1) {
                    if options.fault_mode == FaultMode::LeakSecrets
                        && claim.status == "blocked_secret"
                    {
                        claim.status = "active".to_owned();
                    }
                    actual.audit.push(AuditEvent {
                        kind: "claim.write".to_owned(),
                        target_id: claim.id.clone(),
                    });
                    actual.claims.push(claim);
                }
            }
            session.memory_event_ids.push(event.id.clone());
            actual.events.push(event);
        }
        session.status = "closed".to_owned();
        session.summary = end.summary.clone();
        actual.audit.push(AuditEvent {
            kind: "session.end".to_owned(),
            target_id: session.id.clone(),
        });
    }

    actual.sessions.push(session);
}

fn compact_actual(actual: &mut ActualState) {
    actual.claims.retain(|claim| claim.status == "active");
    let kept_event_ids = actual
        .claims
        .iter()
        .flat_map(|claim| claim.source_event_ids.iter().cloned())
        .collect::<std::collections::BTreeSet<_>>();
    actual
        .events
        .retain(|event| kept_event_ids.contains(event.id.as_str()));
    actual.audit.push(AuditEvent {
        kind: "state.compact".to_owned(),
        target_id: "fake".to_owned(),
    });
}

fn apply_lifecycle_event(actual: &mut ActualState, event: &RecordedEvent) -> bool {
    if let Some(target_id) = find_forget_id_marker(&event.text) {
        forget_claim_by_id(actual, target_id);
        return true;
    }
    if let Some((target_id, new_text)) = find_correct_id_marker(&event.text) {
        correct_claim_by_id(actual, event, target_id, new_text);
        return true;
    }
    if let Some(target) = find_forget_marker(&event.text) {
        forget_claims(actual, target);
        return true;
    }
    if let Some((old_text, new_text)) = find_correct_marker(&event.text) {
        correct_claims(actual, event, old_text, new_text);
        return true;
    }
    false
}

fn forget_claim_by_id(actual: &mut ActualState, target_id: &str) {
    let target_id = target_id.trim();
    for claim in &mut actual.claims {
        if claim.status == "active" && claim.id == target_id {
            claim.status = "forgotten".to_owned();
            actual.audit.push(AuditEvent {
                kind: "claim.update".to_owned(),
                target_id: claim.id.clone(),
            });
            break;
        }
    }
}

fn forget_claims(actual: &mut ActualState, target: &str) {
    for claim in &mut actual.claims {
        if claim.status == "active" && claim_matches_text(claim, target) {
            claim.status = "forgotten".to_owned();
            actual.audit.push(AuditEvent {
                kind: "claim.update".to_owned(),
                target_id: claim.id.clone(),
            });
        }
    }
}

fn correct_claim_by_id(
    actual: &mut ActualState,
    event: &RecordedEvent,
    target_id: &str,
    new_text: &str,
) {
    let target_id = target_id.trim();
    let mut source_event_ids = Vec::new();
    for claim in &mut actual.claims {
        if claim.status == "active" && claim.id == target_id {
            claim.status = "superseded".to_owned();
            source_event_ids.extend(claim.source_event_ids.clone());
            actual.audit.push(AuditEvent {
                kind: "claim.update".to_owned(),
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

    let claim = claim_from_marker(event, actual.claims.len() + 1, new_text, source_event_ids);
    actual.audit.push(AuditEvent {
        kind: "claim.write".to_owned(),
        target_id: claim.id.clone(),
    });
    actual.claims.push(claim);
}

fn correct_claims(actual: &mut ActualState, event: &RecordedEvent, old_text: &str, new_text: &str) {
    let mut source_event_ids = Vec::new();
    for claim in &mut actual.claims {
        if claim.status == "active" && claim_matches_text(claim, old_text) {
            claim.status = "superseded".to_owned();
            source_event_ids.extend(claim.source_event_ids.clone());
            actual.audit.push(AuditEvent {
                kind: "claim.update".to_owned(),
                target_id: claim.id.clone(),
            });
        }
    }

    if source_event_ids.is_empty() {
        return;
    }
    source_event_ids.push(event.id.clone());
    dedupe_ids(&mut source_event_ids);

    let claim = claim_from_marker(event, actual.claims.len() + 1, new_text, source_event_ids);
    actual.audit.push(AuditEvent {
        kind: "claim.write".to_owned(),
        target_id: claim.id.clone(),
    });
    actual.claims.push(claim);
}

fn extract_claim(event: &RecordedEvent, next_claim_number: usize) -> Option<Claim> {
    let marker = find_remember_marker(&event.text)?;
    Some(claim_from_marker(
        event,
        next_claim_number,
        marker,
        vec![event.id.clone()],
    ))
}

fn claim_from_marker(
    event: &RecordedEvent,
    next_claim_number: usize,
    marker: &str,
    source_event_ids: Vec<String>,
) -> Claim {
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
        "blocked_secret"
    } else {
        "active"
    };
    Claim {
        id: format!("claim-{:03}", next_claim_number),
        subject,
        predicate,
        object,
        status: status.to_owned(),
        scope: event.scope.clone(),
        source_event_ids,
    }
}

fn claim_matches_text(claim: &Claim, target: &str) -> bool {
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
    lower.contains("api_key=")
        || lower.contains("api key")
        || lower.contains("secret=")
        || lower.contains("token=")
        || lower.contains("access_token=")
        || lower.contains("password=")
}

fn estimate_tokens(text: &str) -> u32 {
    let count = text.split_whitespace().count();
    u32::try_from(count).unwrap_or(u32::MAX).max(1)
}

fn build_context_pack(
    claims: &[Claim],
    expected: &ContextPackExpected,
    options: TargetRunOptions,
) -> ContextPack {
    let query_terms = normalize_query_terms(&expected.query);
    let allowed_scopes = effective_allowed_scopes(&expected.allowed_scopes);
    let max_items = expected.max_items.unwrap_or(DEFAULT_CONTEXT_MAX_ITEMS);
    let mut candidates = Vec::new();
    let mut omitted = Vec::new();

    for (claim_index, claim) in claims.iter().enumerate() {
        if claim.status != "active" {
            omitted.push(OmittedItem {
                claim_id: claim.id.clone(),
                reason: claim.status.clone(),
            });
            continue;
        }
        if !allowed_scopes.contains(&claim.scope) {
            omitted.push(OmittedItem {
                claim_id: claim.id.clone(),
                reason: format!("scope_denied:{}", claim.scope),
            });
            continue;
        }
        let claim_text = claim.text();
        if let Some(relevance) = score_context_match(&expected.query, &query_terms, &claim_text) {
            let source_event_ids = if options.fault_mode == FaultMode::DropCitations {
                Vec::new()
            } else {
                claim.source_event_ids.clone()
            };
            candidates.push(RankedContextCandidate {
                claim_index,
                item: ContextItem {
                    claim_id: claim.id.clone(),
                    claim_text,
                    source_event_ids,
                    score: relevance.score,
                    matched_terms: relevance.matched_terms,
                    match_reason: relevance.reason,
                },
            });
        } else {
            omitted.push(OmittedItem {
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
        if items.len() < max_items {
            items.push(candidate.item);
        } else {
            omitted.push(OmittedItem {
                claim_id: candidate.item.claim_id,
                reason: format!("context_budget_exceeded:max_items={max_items}"),
            });
        }
    }

    ContextPack { items, omitted }
}

fn build_context_pack_for_query(
    claims: &[Claim],
    query: &str,
    allowed_scopes: &[String],
    options: TargetRunOptions,
) -> ContextPack {
    build_context_pack(
        claims,
        &ContextPackExpected {
            query: query.to_owned(),
            allowed_scopes: allowed_scopes.to_vec(),
            max_items: None,
            item_count: None,
            must_include: Vec::new(),
            must_not_include: Vec::new(),
            expected_order: Vec::new(),
            omitted_reason_contains: Vec::new(),
            citation_required: false,
        },
        options,
    )
}

fn effective_allowed_scopes(scopes: &[String]) -> Vec<String> {
    if scopes.is_empty() {
        vec!["private".to_owned()]
    } else {
        scopes.iter().map(|scope| scope.trim().to_owned()).collect()
    }
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
