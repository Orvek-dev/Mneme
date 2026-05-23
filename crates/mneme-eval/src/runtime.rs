use crate::report::{CheckReport, ScenarioReport};
use crate::scenario::{AuditExpected, ClaimExpected, ContextPackExpected, Expected, Scenario};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(crate) enum FaultMode {
    None,
    SkipClaims,
    LeakSecrets,
    DropCitations,
}

impl FaultMode {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "skip-claims" => Some(Self::SkipClaims),
            "leak-secrets" => Some(Self::LeakSecrets),
            "drop-citations" => Some(Self::DropCitations),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReplayOptions {
    pub(crate) fault_mode: FaultMode,
}

impl Default for ReplayOptions {
    fn default() -> Self {
        Self {
            fault_mode: FaultMode::None,
        }
    }
}

#[derive(Debug, Clone)]
struct RecordedEvent {
    id: String,
    speaker_id: String,
    actor_agent_id: Option<String>,
    text: String,
    scope: String,
    trust_level: String,
}

#[derive(Debug, Clone)]
struct Claim {
    id: String,
    subject: String,
    predicate: String,
    object: String,
    status: String,
    scope: String,
    source_event_ids: Vec<String>,
}

impl Claim {
    fn text(&self) -> String {
        format!("{} {} {}", self.subject, self.predicate, self.object)
    }
}

#[derive(Debug, Clone)]
struct ContextPack {
    items: Vec<ContextItem>,
    omitted: Vec<OmittedItem>,
}

#[derive(Debug, Clone)]
struct ContextItem {
    claim_id: String,
    claim_text: String,
    source_event_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct OmittedItem {
    claim_id: String,
    reason: String,
}

#[derive(Debug, Clone, Default)]
struct BudgetActual {
    spent_tokens: u32,
    hard_cap_violations: u32,
}

#[derive(Debug, Clone)]
struct AuditEvent {
    kind: String,
    target_id: String,
}

#[derive(Debug, Clone, Default)]
struct ActualState {
    events: Vec<RecordedEvent>,
    claims: Vec<Claim>,
    context_pack: Option<ContextPack>,
    budget: BudgetActual,
    audit: Vec<AuditEvent>,
}

pub(crate) fn replay_scenario(scenario: &Scenario, options: ReplayOptions) -> ScenarioReport {
    let actual = run_fake_runtime(scenario, options);
    let checks = check_expected(scenario, &actual);
    ScenarioReport::new(scenario.id.clone(), scenario.tags.clone(), checks)
}

fn run_fake_runtime(scenario: &Scenario, options: ReplayOptions) -> ActualState {
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
    options: ReplayOptions,
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

fn check_expected(scenario: &Scenario, actual: &ActualState) -> Vec<CheckReport> {
    let mut checks = Vec::new();
    if let Some(expected) = &scenario.expected.event_append {
        let actual_count = actual.events.len();
        if actual_count == expected.count {
            checks.push(CheckReport::pass(
                "event_append.count",
                expected.count.to_string(),
                actual_count.to_string(),
            ));
        } else {
            checks.push(CheckReport::fail(
                "event_append.count",
                expected.count.to_string(),
                actual_count.to_string(),
                "actual.events",
            ));
        }
    }
    checks.extend(check_claims(&scenario.expected, actual));
    if let Some(expected) = &scenario.expected.context_pack {
        checks.extend(check_context_pack(expected, actual));
    }
    if let Some(expected) = &scenario.expected.budget {
        if actual.budget.hard_cap_violations == expected.hard_cap_violations {
            checks.push(CheckReport::pass(
                "budget.hard_cap_violations",
                expected.hard_cap_violations.to_string(),
                actual.budget.hard_cap_violations.to_string(),
            ));
        } else {
            checks.push(CheckReport::fail(
                "budget.hard_cap_violations",
                expected.hard_cap_violations.to_string(),
                actual.budget.hard_cap_violations.to_string(),
                "actual.budget",
            ));
        }
    }
    if let Some(expected) = &scenario.expected.audit {
        checks.push(check_audit(expected, actual));
    }
    checks
}

fn check_claims(expected: &Expected, actual: &ActualState) -> Vec<CheckReport> {
    expected
        .claims
        .iter()
        .map(|expected_claim| check_claim(expected_claim, actual))
        .collect()
}

fn check_claim(expected: &ClaimExpected, actual: &ActualState) -> CheckReport {
    let found = actual.claims.iter().any(|claim| {
        claim.subject == expected.subject
            && claim.predicate == expected.predicate
            && claim.object == expected.object
            && option_matches(expected.status.as_ref(), &claim.status)
            && option_matches(expected.scope.as_ref(), &claim.scope)
    });
    let name = format!(
        "claim.{}.{}.{}",
        expected.subject, expected.predicate, expected.object
    );
    if expected.must_not_exist {
        if found {
            CheckReport::fail(name, "absent", "present", "actual.claims")
        } else {
            CheckReport::pass(name, "absent", "absent")
        }
    } else if found {
        CheckReport::pass(name, "present", "present")
    } else {
        CheckReport::fail(name, "present", "absent", "actual.claims")
    }
}

fn check_context_pack(expected: &ContextPackExpected, actual: &ActualState) -> Vec<CheckReport> {
    let context = actual.context_pack.clone().unwrap_or(ContextPack {
        items: Vec::new(),
        omitted: Vec::new(),
    });
    let omitted_summary = context
        .omitted
        .iter()
        .map(|item| format!("{}:{}", item.claim_id, item.reason))
        .collect::<Vec<_>>()
        .join(",");
    let joined = context
        .items
        .iter()
        .map(|item| item.claim_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let joined_lower = joined.to_ascii_lowercase();
    let mut checks = Vec::new();

    for value in &expected.must_include {
        let value_lower = value.to_ascii_lowercase();
        if joined_lower.contains(&value_lower) {
            checks.push(CheckReport::pass(
                format!("context_pack.must_include.{value}"),
                "included",
                "included",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("context_pack.must_include.{value}"),
                "included",
                "missing",
                format!("actual.context_pack.items omitted={omitted_summary}"),
            ));
        }
    }
    for value in &expected.must_not_include {
        let value_lower = value.to_ascii_lowercase();
        if joined_lower.contains(&value_lower) {
            checks.push(CheckReport::fail(
                format!("context_pack.must_not_include.{value}"),
                "absent",
                "included",
                "actual.context_pack.items",
            ));
        } else {
            checks.push(CheckReport::pass(
                format!("context_pack.must_not_include.{value}"),
                "absent",
                "absent",
            ));
        }
    }
    if expected.citation_required {
        let missing = context
            .items
            .iter()
            .filter(|item| item.source_event_ids.is_empty())
            .map(|item| item.claim_id.clone())
            .collect::<Vec<_>>();
        if missing.is_empty() {
            checks.push(CheckReport::pass(
                "context_pack.citation_required",
                "all items cited",
                "all items cited",
            ));
        } else {
            checks.push(CheckReport::fail(
                "context_pack.citation_required",
                "all items cited",
                format!("missing citations for {}", missing.join(",")),
                "actual.context_pack.items",
            ));
        }
    }
    checks
}

fn check_audit(expected: &AuditExpected, actual: &ActualState) -> CheckReport {
    if !expected.read_write_events_required {
        return CheckReport::pass(
            "audit.read_write_events_required",
            "not required",
            "not required",
        );
    }
    let has_append = actual
        .audit
        .iter()
        .any(|event| event.kind == "event.append");
    let has_write = actual.audit.iter().any(|event| event.kind == "claim.write");
    let has_read = actual
        .audit
        .iter()
        .any(|event| event.kind == "context.read");
    let targets_nonempty = actual.audit.iter().all(|event| !event.target_id.is_empty());
    if has_append && has_write && has_read && targets_nonempty {
        CheckReport::pass(
            "audit.read_write_events_required",
            "append/write/read",
            "append/write/read",
        )
    } else {
        CheckReport::fail(
            "audit.read_write_events_required",
            "append/write/read",
            format!(
                "append={has_append} write={has_write} read={has_read} targets_nonempty={targets_nonempty}"
            ),
            "actual.audit",
        )
    }
}

fn option_matches(expected: Option<&String>, actual: &str) -> bool {
    match expected {
        Some(expected) => expected == actual,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::{
        Budget, BudgetExpected, ContextPackExpected, EventAppendExpected, Expected, InputEvent,
        Scenario,
    };

    #[test]
    fn deterministic_replay_passes_same_turn_memory() {
        let scenario = Scenario {
            id: "test".to_owned(),
            tags: Vec::new(),
            budget: Budget {
                daily_cloud_tokens: 100,
            },
            events: vec![InputEvent {
                speaker_id: "user".to_owned(),
                actor_agent_id: Some("codex".to_owned()),
                text: "remember: user prefers local-first tools".to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            }],
            expected: Expected {
                event_append: Some(EventAppendExpected { count: 1 }),
                claims: vec![ClaimExpected {
                    subject: "user".to_owned(),
                    predicate: "prefers".to_owned(),
                    object: "local-first tools".to_owned(),
                    status: Some("active".to_owned()),
                    scope: Some("private".to_owned()),
                    must_not_exist: false,
                }],
                context_pack: Some(ContextPackExpected {
                    query: "preferences".to_owned(),
                    must_include: vec!["local-first".to_owned()],
                    must_not_include: Vec::new(),
                    citation_required: true,
                }),
                budget: Some(BudgetExpected {
                    hard_cap_violations: 0,
                }),
                audit: Some(AuditExpected {
                    read_write_events_required: true,
                }),
            },
        };
        let first = replay_scenario(&scenario, ReplayOptions::default());
        let second = replay_scenario(&scenario, ReplayOptions::default());
        assert!(first.ok);
        assert_eq!(first.checks.len(), second.checks.len());
        assert_eq!(first.ok, second.ok);
    }

    #[test]
    fn seeded_fault_is_detected() {
        let scenario = Scenario {
            id: "test".to_owned(),
            tags: Vec::new(),
            budget: Budget {
                daily_cloud_tokens: 100,
            },
            events: vec![InputEvent {
                speaker_id: "user".to_owned(),
                actor_agent_id: None,
                text: "remember: user prefers local-first tools".to_owned(),
                scope: "private".to_owned(),
                trust_level: "trusted_user".to_owned(),
            }],
            expected: Expected {
                event_append: Some(EventAppendExpected { count: 1 }),
                claims: vec![ClaimExpected {
                    subject: "user".to_owned(),
                    predicate: "prefers".to_owned(),
                    object: "local-first tools".to_owned(),
                    status: Some("active".to_owned()),
                    scope: Some("private".to_owned()),
                    must_not_exist: false,
                }],
                context_pack: None,
                budget: None,
                audit: None,
            },
        };
        let report = replay_scenario(
            &scenario,
            ReplayOptions {
                fault_mode: FaultMode::SkipClaims,
            },
        );
        assert!(!report.ok);
    }
}
