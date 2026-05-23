use crate::error::EvalError;
use crate::scenario::{ContextPackExpected, Scenario};
use crate::target::{
    ActualState, AuditEvent, Claim, ContextItem, ContextPack, EvalTarget, EvalTargetMetadata,
    FaultMode, OmittedItem, RecordedEvent, TargetRunOptions,
};

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

    actual
}

fn apply_lifecycle_event(actual: &mut ActualState, event: &RecordedEvent) -> bool {
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

fn build_context_pack(
    claims: &[Claim],
    expected: &ContextPackExpected,
    options: TargetRunOptions,
) -> ContextPack {
    let query_terms = expected
        .query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let mut items = Vec::new();
    let mut omitted = Vec::new();

    for claim in claims {
        if claim.status != "active" {
            omitted.push(OmittedItem {
                claim_id: claim.id.clone(),
                reason: claim.status.clone(),
            });
            continue;
        }
        let claim_text = claim.text();
        let claim_text_lower = claim_text.to_ascii_lowercase();
        let matches_query = query_terms.is_empty()
            || query_terms
                .iter()
                .any(|term| claim_text_lower.contains(term));
        if matches_query || !expected.must_include.is_empty() {
            let source_event_ids = if options.fault_mode == FaultMode::DropCitations {
                Vec::new()
            } else {
                claim.source_event_ids.clone()
            };
            items.push(ContextItem {
                claim_id: claim.id.clone(),
                claim_text,
                source_event_ids,
            });
        } else {
            omitted.push(OmittedItem {
                claim_id: claim.id.clone(),
                reason: "low_relevance".to_owned(),
            });
        }
    }

    ContextPack { items, omitted }
}
