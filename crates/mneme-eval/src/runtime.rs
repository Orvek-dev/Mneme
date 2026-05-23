use crate::error::EvalError;
use crate::report::{CheckReport, ScenarioReport};
use crate::scenario::{AuditExpected, ClaimExpected, ContextPackExpected, Expected, Scenario};
use crate::target::{ActualState, ContextPack, EvalTarget, TargetRunOptions};

pub(crate) fn replay_scenario(
    scenario: &Scenario,
    target: &dyn EvalTarget,
    options: TargetRunOptions,
) -> Result<ScenarioReport, EvalError> {
    let actual = target.run(scenario, options)?;
    let checks = check_expected(scenario, &actual);
    Ok(ScenarioReport::new(
        scenario.id.clone(),
        scenario.tags.clone(),
        checks,
    ))
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
        checks.extend(check_audit(expected, actual));
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

fn check_audit(expected: &AuditExpected, actual: &ActualState) -> Vec<CheckReport> {
    let mut checks = Vec::new();
    checks.push(check_read_write_audit(expected, actual));
    if expected.claim_update_required {
        checks.push(check_claim_update_audit(actual));
    }
    checks
}

fn check_read_write_audit(expected: &AuditExpected, actual: &ActualState) -> CheckReport {
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

fn check_claim_update_audit(actual: &ActualState) -> CheckReport {
    let has_update = actual
        .audit
        .iter()
        .any(|event| event.kind == "claim.update");
    let update_targets_nonempty = actual
        .audit
        .iter()
        .filter(|event| event.kind == "claim.update")
        .all(|event| !event.target_id.is_empty());
    if has_update && update_targets_nonempty {
        CheckReport::pass(
            "audit.claim_update_required",
            "claim.update",
            "claim.update",
        )
    } else {
        CheckReport::fail(
            "audit.claim_update_required",
            "claim.update",
            format!("has_update={has_update} targets_nonempty={update_targets_nonempty}"),
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
    use crate::fake::FakeEvalTarget;
    use crate::scenario::{
        Budget, BudgetExpected, ContextPackExpected, EventAppendExpected, Expected, InputEvent,
        Scenario,
    };
    use crate::target::FaultMode;

    #[test]
    fn deterministic_replay_passes_same_turn_memory() -> Result<(), EvalError> {
        let scenario = Scenario {
            id: "test".to_owned(),
            tags: Vec::new(),
            budget: Budget {
                daily_cloud_tokens: 100,
            },
            persistence: None,
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
                    claim_update_required: false,
                }),
            },
        };
        let target = FakeEvalTarget;
        let first = replay_scenario(&scenario, &target, TargetRunOptions::default())?;
        let second = replay_scenario(&scenario, &target, TargetRunOptions::default())?;
        assert!(first.ok);
        assert_eq!(first.checks.len(), second.checks.len());
        assert_eq!(first.ok, second.ok);
        Ok(())
    }

    #[test]
    fn seeded_fault_is_detected() -> Result<(), EvalError> {
        let scenario = Scenario {
            id: "test".to_owned(),
            tags: Vec::new(),
            budget: Budget {
                daily_cloud_tokens: 100,
            },
            persistence: None,
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
        let target = FakeEvalTarget;
        let report = replay_scenario(
            &scenario,
            &target,
            TargetRunOptions {
                fault_mode: FaultMode::SkipClaims,
            },
        )?;
        assert!(!report.ok);
        Ok(())
    }
}
