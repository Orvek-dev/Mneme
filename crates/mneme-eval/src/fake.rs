use crate::error::EvalError;
use crate::scenario::{ContextPackExpected, Scenario};
use crate::target::{
    ActualState, AuditEvent, Claim, ContextItem, ContextPack, EvalTarget, FaultMode, OmittedItem,
    RecordedEvent, TargetRunOptions,
};

pub(crate) struct FakeEvalTarget;

impl EvalTarget for FakeEvalTarget {
    fn name(&self) -> &'static str {
        "fake"
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

fn extract_claim(event: &RecordedEvent, next_claim_number: usize) -> Option<Claim> {
    let marker = find_remember_marker(&event.text)?;
    let source_event_ids = vec![event.id.clone()];
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
    Some(Claim {
        id: format!("claim-{:03}", next_claim_number),
        subject,
        predicate,
        object,
        status: status.to_owned(),
        scope: event.scope.clone(),
        source_event_ids,
    })
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
