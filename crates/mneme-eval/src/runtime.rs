use crate::error::EvalError;
use crate::report::{CheckReport, ScenarioReport};
use crate::scenario::{
    AuditExpected, ClaimExpected, ContextPackExpected, CurationExpected, Expected, QualityExpected,
    Scenario, SessionExpected, StoreExpected, TeamExpected,
};
use crate::target::{ActualState, ContextPack, EvalTarget, QualityActual, TargetRunOptions};

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
    if let Some(expected) = &scenario.expected.store {
        checks.extend(check_store(expected, actual));
    }
    if let Some(expected) = &scenario.expected.session {
        checks.extend(check_session(expected, actual));
    }
    if let Some(expected) = &scenario.expected.quality {
        checks.extend(check_quality(expected, actual));
    }
    if let Some(expected) = &scenario.expected.curation {
        checks.extend(check_curation(expected, actual));
    }
    if let Some(expected) = &scenario.expected.team {
        checks.extend(check_team(expected, actual));
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
    let item_summary = context
        .items
        .iter()
        .map(|item| {
            format!(
                "{}:{}:{}:{}",
                item.claim_text,
                item.score,
                item.match_reason,
                item.matched_terms.join("|")
            )
        })
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

    if let Some(expected_count) = expected.item_count {
        if context.items.len() == expected_count {
            checks.push(CheckReport::pass(
                "context_pack.item_count",
                expected_count.to_string(),
                context.items.len().to_string(),
            ));
        } else {
            checks.push(CheckReport::fail(
                "context_pack.item_count",
                expected_count.to_string(),
                context.items.len().to_string(),
                format!("actual.context_pack.items={item_summary}"),
            ));
        }
    }

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
    if !expected.expected_order.is_empty() {
        checks.push(check_context_order(
            &expected.expected_order,
            &context,
            &omitted_summary,
            &item_summary,
        ));
    }
    for value in &expected.omitted_reason_contains {
        if omitted_summary.contains(value) {
            checks.push(CheckReport::pass(
                format!("context_pack.omitted_reason_contains.{value}"),
                "present",
                "present",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("context_pack.omitted_reason_contains.{value}"),
                "present",
                "missing",
                format!("actual.context_pack.omitted={omitted_summary}"),
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

fn check_context_order(
    expected_order: &[String],
    context: &ContextPack,
    omitted_summary: &str,
    item_summary: &str,
) -> CheckReport {
    let mut next_index = 0;
    let mut actual_order = Vec::new();
    for expected in expected_order {
        let expected_lower = expected.to_ascii_lowercase();
        let found = context
            .items
            .iter()
            .enumerate()
            .skip(next_index)
            .find(|(_, item)| {
                item.claim_text
                    .to_ascii_lowercase()
                    .contains(&expected_lower)
            });
        let Some((index, item)) = found else {
            return CheckReport::fail(
                "context_pack.expected_order",
                expected_order.join(" > "),
                actual_order.join(" > "),
                format!("actual.context_pack.items={item_summary} omitted={omitted_summary}"),
            );
        };
        next_index = index + 1;
        actual_order.push(item.claim_text.clone());
    }

    CheckReport::pass(
        "context_pack.expected_order",
        expected_order.join(" > "),
        actual_order.join(" > "),
    )
}

fn check_audit(expected: &AuditExpected, actual: &ActualState) -> Vec<CheckReport> {
    let mut checks = Vec::new();
    checks.push(check_read_write_audit(expected, actual));
    if expected.claim_update_required {
        checks.push(check_claim_update_audit(actual));
    }
    if expected.session_events_required {
        checks.push(check_session_audit(actual));
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

fn check_session_audit(actual: &ActualState) -> CheckReport {
    let has_begin = actual
        .audit
        .iter()
        .any(|event| event.kind == "session.begin");
    let has_end = actual.audit.iter().any(|event| event.kind == "session.end");
    if has_begin && has_end {
        CheckReport::pass("audit.session_events_required", "begin/end", "begin/end")
    } else {
        CheckReport::fail(
            "audit.session_events_required",
            "begin/end",
            format!("begin={has_begin} end={has_end}"),
            "actual.audit",
        )
    }
}

fn check_session(expected: &SessionExpected, actual: &ActualState) -> Vec<CheckReport> {
    let Some(session) = actual.sessions.first() else {
        return vec![CheckReport::fail(
            "session.present",
            "present",
            "missing",
            "actual.sessions",
        )];
    };
    let mut checks = Vec::new();
    if let Some(status) = &expected.status {
        if &session.status == status {
            checks.push(CheckReport::pass("session.status", status, &session.status));
        } else {
            checks.push(CheckReport::fail(
                "session.status",
                status,
                &session.status,
                "actual.sessions",
            ));
        }
    }
    if let Some(task) = &expected.task {
        if &session.task == task {
            checks.push(CheckReport::pass("session.task", task, &session.task));
        } else {
            checks.push(CheckReport::fail(
                "session.task",
                task,
                &session.task,
                "actual.sessions",
            ));
        }
    }
    if let Some(lineage_id) = &expected.lineage_id {
        if session.lineage_id.as_ref() == Some(lineage_id) {
            checks.push(CheckReport::pass(
                "session.lineage_id",
                lineage_id,
                lineage_id,
            ));
        } else {
            checks.push(CheckReport::fail(
                "session.lineage_id",
                lineage_id,
                format!("{:?}", session.lineage_id),
                "actual.sessions.lineage_id",
            ));
        }
    }
    if let Some(actor_agent_id) = &expected.actor_agent_id {
        if session.actor_agent_id.as_ref() == Some(actor_agent_id) {
            checks.push(CheckReport::pass(
                "session.actor_agent_id",
                actor_agent_id,
                actor_agent_id,
            ));
        } else {
            checks.push(CheckReport::fail(
                "session.actor_agent_id",
                actor_agent_id,
                format!("{:?}", session.actor_agent_id),
                "actual.sessions.actor_agent_id",
            ));
        }
    }
    for expected_text in &expected.context_must_include {
        let expected_lower = expected_text.to_ascii_lowercase();
        let context_text = actual
            .claims
            .iter()
            .filter(|claim| session.context_claim_ids.contains(&claim.id))
            .map(|claim| claim.text())
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase();
        if context_text.contains(&expected_lower) {
            checks.push(CheckReport::pass(
                format!("session.context_must_include.{expected_text}"),
                "included",
                "included",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("session.context_must_include.{expected_text}"),
                "included",
                "missing",
                "actual.sessions.context_claim_ids",
            ));
        }
    }
    if let Some(count) = expected.memory_event_count {
        if session.memory_event_ids.len() == count {
            checks.push(CheckReport::pass(
                "session.memory_event_count",
                count.to_string(),
                session.memory_event_ids.len().to_string(),
            ));
        } else {
            checks.push(CheckReport::fail(
                "session.memory_event_count",
                count.to_string(),
                session.memory_event_ids.len().to_string(),
                "actual.sessions.memory_event_ids",
            ));
        }
    }
    if let Some(summary) = &expected.summary_contains {
        let actual_summary = session.summary.clone().unwrap_or_default();
        if actual_summary
            .to_ascii_lowercase()
            .contains(&summary.to_ascii_lowercase())
        {
            checks.push(CheckReport::pass(
                "session.summary_contains",
                summary,
                summary,
            ));
        } else {
            checks.push(CheckReport::fail(
                "session.summary_contains",
                summary,
                actual_summary,
                "actual.sessions.summary",
            ));
        }
    }
    checks
}

fn check_store(expected: &StoreExpected, actual: &ActualState) -> Vec<CheckReport> {
    let mut checks = Vec::new();
    let Some(store) = &actual.store else {
        return vec![CheckReport::fail(
            "store.present",
            "present",
            "missing",
            "actual.store",
        )];
    };

    if expected.valid == store.valid {
        checks.push(CheckReport::pass(
            "store.valid",
            expected.valid.to_string(),
            store.valid.to_string(),
        ));
    } else {
        checks.push(CheckReport::fail(
            "store.valid",
            expected.valid.to_string(),
            store.valid.to_string(),
            format!("actual.store.error_count={}", store.error_count),
        ));
    }

    if let Some(schema_version) = expected.schema_version {
        if store.schema_version == Some(schema_version) {
            checks.push(CheckReport::pass(
                "store.schema_version",
                schema_version.to_string(),
                schema_version.to_string(),
            ));
        } else {
            checks.push(CheckReport::fail(
                "store.schema_version",
                schema_version.to_string(),
                format!("{:?}", store.schema_version),
                "actual.store.schema_version",
            ));
        }
    }

    checks.push(check_store_bool(
        "store.backup_required",
        expected.backup_required,
        store.backup_present,
    ));
    checks.push(check_store_bool(
        "store.repair_performed",
        expected.repair_performed,
        store.repair_performed,
    ));
    checks.push(check_store_bool(
        "store.restored",
        expected.restored,
        store.restored,
    ));
    checks.push(check_store_bool(
        "store.compacted",
        expected.compacted,
        store.compacted,
    ));
    checks.push(check_store_bool(
        "store.imported",
        expected.imported,
        store.imported,
    ));

    if store.generation.unwrap_or_default() > 0 {
        checks.push(CheckReport::pass(
            "store.generation",
            ">0",
            store.generation.unwrap_or_default().to_string(),
        ));
    } else {
        checks.push(CheckReport::fail(
            "store.generation",
            ">0",
            format!("{:?}", store.generation),
            "actual.store.generation",
        ));
    }

    checks
}

fn check_store_bool(name: &str, expected: bool, actual: bool) -> CheckReport {
    if expected == actual {
        CheckReport::pass(name, expected.to_string(), actual.to_string())
    } else {
        CheckReport::fail(
            name,
            expected.to_string(),
            actual.to_string(),
            "actual.store",
        )
    }
}

fn check_quality(expected: &QualityExpected, actual: &ActualState) -> Vec<CheckReport> {
    let Some(quality) = &actual.quality else {
        return vec![CheckReport::fail(
            "quality.present",
            "present",
            "missing",
            "actual.quality",
        )];
    };
    check_quality_actual("quality", expected, quality, "actual.quality")
}

fn check_quality_actual(
    prefix: &str,
    expected: &QualityExpected,
    quality: &QualityActual,
    artifact: &str,
) -> Vec<CheckReport> {
    let mut checks = Vec::new();
    if let Some(count) = expected.duplicate_active_groups {
        checks.push(check_quality_count(
            &format!("{prefix}.duplicate_active_groups"),
            count,
            quality.duplicate_active_groups,
            artifact,
        ));
    }
    if let Some(count) = expected.duplicate_active_claims {
        checks.push(check_quality_count(
            &format!("{prefix}.duplicate_active_claims"),
            count,
            quality.duplicate_active_claims,
            artifact,
        ));
    }
    if let Some(count) = expected.blocked_secret_count {
        checks.push(check_quality_count(
            &format!("{prefix}.blocked_secret_count"),
            count,
            quality.blocked_secret_count,
            artifact,
        ));
    }
    if let Some(count) = expected.inactive_claim_count {
        checks.push(check_quality_count(
            &format!("{prefix}.inactive_claim_count"),
            count,
            quality.inactive_claim_count,
            artifact,
        ));
    }
    if let Some(count) = expected.review_item_count {
        checks.push(check_quality_count(
            &format!("{prefix}.review_item_count"),
            count,
            quality.review_item_count,
            artifact,
        ));
    }
    for kind in &expected.finding_kinds {
        if quality.finding_kinds.contains(kind) {
            checks.push(CheckReport::pass(
                format!("{prefix}.finding_kind.{kind}"),
                "present",
                "present",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("{prefix}.finding_kind.{kind}"),
                "present",
                "missing",
                format!(
                    "{artifact}.finding_kinds={}",
                    quality.finding_kinds.join(",")
                ),
            ));
        }
    }
    checks
}

fn check_curation(expected: &CurationExpected, actual: &ActualState) -> Vec<CheckReport> {
    let mut checks = Vec::new();
    let Some(curation) = &actual.curation else {
        return vec![CheckReport::fail(
            "curation.present",
            "present",
            "missing",
            "actual.curation",
        )];
    };
    if let Some(count) = expected.duplicate_forget_count {
        checks.push(check_quality_count(
            "curation.duplicate_forget_count",
            count,
            curation.duplicate_forget_count,
            "actual.curation",
        ));
    }
    if let Some(count) = expected.blocked_secret_review_count {
        checks.push(check_quality_count(
            "curation.blocked_secret_review_count",
            count,
            curation.blocked_secret_review_count,
            "actual.curation",
        ));
    }
    if let Some(value) = expected.compact_recommended {
        checks.push(check_bool(
            "curation.compact_recommended",
            value,
            curation.compact_recommended,
            "actual.curation",
        ));
    }
    if let Some(value) = expected.compacted {
        checks.push(check_bool(
            "curation.compacted",
            value,
            curation.compacted,
            "actual.curation",
        ));
    }
    if let Some(value) = expected.changed {
        checks.push(check_bool(
            "curation.changed",
            value,
            curation.changed,
            "actual.curation",
        ));
    }
    if let Some(quality) = &expected.before_quality {
        checks.extend(check_quality_actual(
            "curation.before_quality",
            quality,
            &curation.before_quality,
            "actual.curation.before_quality",
        ));
    }
    if let Some(quality) = &expected.after_quality {
        checks.extend(check_quality_actual(
            "curation.after_quality",
            quality,
            &curation.after_quality,
            "actual.curation.after_quality",
        ));
    }
    checks
}

fn check_quality_count(name: &str, expected: usize, actual: usize, artifact: &str) -> CheckReport {
    if expected == actual {
        CheckReport::pass(name, expected.to_string(), actual.to_string())
    } else {
        CheckReport::fail(name, expected.to_string(), actual.to_string(), artifact)
    }
}

fn check_bool(name: &str, expected: bool, actual: bool, artifact: &str) -> CheckReport {
    if expected == actual {
        CheckReport::pass(name, expected.to_string(), actual.to_string())
    } else {
        CheckReport::fail(name, expected.to_string(), actual.to_string(), artifact)
    }
}

fn check_team(expected: &TeamExpected, actual: &ActualState) -> Vec<CheckReport> {
    let Some(team) = &actual.team else {
        return vec![CheckReport::fail(
            "team.present",
            "present",
            "missing",
            "actual.team",
        )];
    };
    let mut checks = Vec::new();
    if let Some(value) = expected.validation_ok {
        checks.push(check_bool(
            "team.validation_ok",
            value,
            team.validation_ok,
            "actual.team",
        ));
    }
    if let Some(count) = expected.memory_count {
        checks.push(check_quality_count(
            "team.memory_count",
            count,
            team.memory_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.active_memory_count {
        checks.push(check_quality_count(
            "team.active_memory_count",
            count,
            team.active_memory_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.blocked_secret_count {
        checks.push(check_quality_count(
            "team.blocked_secret_count",
            count,
            team.blocked_secret_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.quarantined_count {
        checks.push(check_quality_count(
            "team.quarantined_count",
            count,
            team.quarantined_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.promotion_count {
        checks.push(check_quality_count(
            "team.promotion_count",
            count,
            team.promotion_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.run_count {
        checks.push(check_quality_count(
            "team.run_count",
            count,
            team.run_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.open_run_count {
        checks.push(check_quality_count(
            "team.open_run_count",
            count,
            team.open_run_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.closed_run_count {
        checks.push(check_quality_count(
            "team.closed_run_count",
            count,
            team.closed_run_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.pending_promotion_count {
        checks.push(check_quality_count(
            "team.pending_promotion_count",
            count,
            team.pending_promotion_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.approved_promotion_count {
        checks.push(check_quality_count(
            "team.approved_promotion_count",
            count,
            team.approved_promotion_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.rejected_promotion_count {
        checks.push(check_quality_count(
            "team.rejected_promotion_count",
            count,
            team.rejected_promotion_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.denied_count {
        checks.push(check_quality_count(
            "team.denied_count",
            count,
            team.denied_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.scope_leak_count {
        checks.push(check_quality_count(
            "team.scope_leak_count",
            count,
            team.scope_leak_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.secret_leak_count {
        checks.push(check_quality_count(
            "team.secret_leak_count",
            count,
            team.secret_leak_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.sync_memory_count {
        checks.push(check_quality_count(
            "team.sync_memory_count",
            count,
            team.sync_memory_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.sync_omitted_count {
        checks.push(check_quality_count(
            "team.sync_omitted_count",
            count,
            team.sync_omitted_count,
            "actual.team",
        ));
    }
    if let Some(expected_value) = expected.sync_checksum_verified {
        checks.push(check_bool(
            "team.sync_checksum_verified",
            expected_value,
            team.sync_checksum_verified,
            "actual.team",
        ));
    }
    if let Some(count) = expected.handoff_context_item_count {
        checks.push(check_quality_count(
            "team.handoff_context_item_count",
            count,
            team.handoff_context_item_count,
            "actual.team",
        ));
    }
    if let Some(value) = expected.firewall_ok {
        checks.push(check_bool(
            "team.firewall_ok",
            value,
            team.firewall_ok,
            "actual.team",
        ));
    }
    if let Some(count) = expected.firewall_high_count {
        checks.push(check_quality_count(
            "team.firewall_high_count",
            count,
            team.firewall_high_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.ontology_entity_count {
        checks.push(check_quality_count(
            "team.ontology_entity_count",
            count,
            team.ontology_entity_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.ontology_relation_count {
        checks.push(check_quality_count(
            "team.ontology_relation_count",
            count,
            team.ontology_relation_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.ontology_attribute_count {
        checks.push(check_quality_count(
            "team.ontology_attribute_count",
            count,
            team.ontology_attribute_count,
            "actual.team",
        ));
    }
    if let Some(expected_value) = expected.quality_ok {
        checks.push(check_bool(
            "team.quality_ok",
            expected_value,
            team.quality_ok,
            "actual.team",
        ));
    }
    if let Some(count) = expected.quality_duplicate_group_count {
        checks.push(check_quality_count(
            "team.quality_duplicate_group_count",
            count,
            team.quality_duplicate_group_count,
            "actual.team",
        ));
    }
    if let Some(count) = expected.quality_conflict_group_count {
        checks.push(check_quality_count(
            "team.quality_conflict_group_count",
            count,
            team.quality_conflict_group_count,
            "actual.team",
        ));
    }

    let context_items = team
        .context_pack
        .as_ref()
        .map(|context| context.items.as_slice())
        .unwrap_or(&[]);
    let omitted_items = team
        .context_pack
        .as_ref()
        .map(|context| context.omitted.as_slice())
        .unwrap_or(&[]);
    let joined = context_items
        .iter()
        .map(|item| item.memory_text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let joined_lower = joined.to_ascii_lowercase();
    let full_output_lower = team.serialized_surface.to_ascii_lowercase();
    let item_summary = context_items
        .iter()
        .map(|item| format!("{}:{}:{}", item.memory_id, item.scope, item.score))
        .collect::<Vec<_>>()
        .join(",");
    let omitted_summary = omitted_items
        .iter()
        .map(|item| format!("{}:{}:{}", item.memory_id, item.memory_text, item.reason))
        .collect::<Vec<_>>()
        .join(",");

    if let Some(count) = expected.context_item_count {
        checks.push(check_quality_count(
            "team.context_item_count",
            count,
            context_items.len(),
            &format!("actual.team.context.items={item_summary}"),
        ));
    }
    for value in &expected.context_must_include {
        let value_lower = value.to_ascii_lowercase();
        if joined_lower.contains(&value_lower) {
            checks.push(CheckReport::pass(
                format!("team.context_must_include.{value}"),
                "included",
                "included",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("team.context_must_include.{value}"),
                "included",
                "missing",
                format!("actual.team.context.items={item_summary} omitted={omitted_summary}"),
            ));
        }
    }
    for value in &expected.context_must_not_include {
        let value_lower = value.to_ascii_lowercase();
        if joined_lower.contains(&value_lower) {
            checks.push(CheckReport::fail(
                format!("team.context_must_not_include.{value}"),
                "absent",
                "included",
                "actual.team.context.items",
            ));
        } else {
            checks.push(CheckReport::pass(
                format!("team.context_must_not_include.{value}"),
                "absent",
                "absent",
            ));
        }
    }
    for value in &expected.full_output_must_not_include {
        let value_lower = value.to_ascii_lowercase();
        if full_output_lower.contains(&value_lower) {
            checks.push(CheckReport::fail(
                format!("team.full_output_must_not_include.{value}"),
                "absent",
                "included",
                "actual.team.serialized_surface",
            ));
        } else {
            checks.push(CheckReport::pass(
                format!("team.full_output_must_not_include.{value}"),
                "absent",
                "absent",
            ));
        }
    }
    for value in &expected.omitted_reason_contains {
        if omitted_summary.contains(value) {
            checks.push(CheckReport::pass(
                format!("team.omitted_reason_contains.{value}"),
                "present",
                "present",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("team.omitted_reason_contains.{value}"),
                "present",
                "missing",
                format!("actual.team.context.omitted={omitted_summary}"),
            ));
        }
    }
    if expected.citation_required {
        let missing = context_items
            .iter()
            .filter(|item| item.source_event_ids.is_empty())
            .map(|item| item.memory_id.clone())
            .collect::<Vec<_>>();
        if missing.is_empty() {
            checks.push(CheckReport::pass(
                "team.citation_required",
                "all items cited",
                "all items cited",
            ));
        } else {
            checks.push(CheckReport::fail(
                "team.citation_required",
                "all items cited",
                format!("missing citations for {}", missing.join(",")),
                "actual.team.context.items",
            ));
        }
    }
    for kind in &expected.audit_kinds {
        if actual.audit.iter().any(|event| event.kind == *kind) {
            checks.push(CheckReport::pass(
                format!("team.audit_kind.{kind}"),
                "present",
                "present",
            ));
        } else {
            checks.push(CheckReport::fail(
                format!("team.audit_kind.{kind}"),
                "present",
                "missing",
                "actual.audit",
            ));
        }
    }
    checks
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
        Maintenance, Scenario,
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
            maintenance: Maintenance::default(),
            agent_flow: None,
            mcp_continuity_flow: None,
            team_flow: None,
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
                    query: "user preferences".to_owned(),
                    allowed_scopes: Vec::new(),
                    max_items: None,
                    item_count: None,
                    must_include: vec!["local-first".to_owned()],
                    must_not_include: Vec::new(),
                    expected_order: Vec::new(),
                    omitted_reason_contains: Vec::new(),
                    citation_required: true,
                }),
                budget: Some(BudgetExpected {
                    hard_cap_violations: 0,
                }),
                audit: Some(AuditExpected {
                    read_write_events_required: true,
                    claim_update_required: false,
                    session_events_required: false,
                }),
                store: None,
                session: None,
                quality: None,
                curation: None,
                team: None,
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
            maintenance: Maintenance::default(),
            agent_flow: None,
            mcp_continuity_flow: None,
            team_flow: None,
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
                store: None,
                session: None,
                quality: None,
                curation: None,
                team: None,
            },
        };
        let target = FakeEvalTarget;
        let report = replay_scenario(
            &scenario,
            &target,
            TargetRunOptions {
                fault_mode: FaultMode::SkipClaims,
                command_extractor: None,
            },
        )?;
        assert!(!report.ok);
        Ok(())
    }
}
