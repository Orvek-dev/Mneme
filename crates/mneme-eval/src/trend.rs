use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::EvalError;
use crate::redaction;
use crate::report::{BaselineCategorySummary, BaselineFailedCheckSummary, BaselineReport};

const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct BaselineCompareConfig {
    pub(crate) before_path: PathBuf,
    pub(crate) after_path: PathBuf,
    pub(crate) max_pass_rate_drop: f64,
    pub(crate) max_category_drop: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BaselineCompareReport {
    pub(crate) report_schema_version: u32,
    pub(crate) command: &'static str,
    pub(crate) before: String,
    pub(crate) after: String,
    pub(crate) suite: String,
    pub(crate) target: String,
    pub(crate) comparable: bool,
    pub(crate) ok: bool,
    pub(crate) regression_detected: bool,
    pub(crate) aggregate: AggregateDelta,
    pub(crate) category_changes: Vec<RateDelta>,
    pub(crate) scenario_changes: Vec<RateDelta>,
    pub(crate) new_failed_scenarios: Vec<String>,
    pub(crate) resolved_scenarios: Vec<String>,
    pub(crate) regressed_scenarios: Vec<String>,
    pub(crate) improved_scenarios: Vec<String>,
    pub(crate) new_failed_checks: Vec<FailedCheckDelta>,
    pub(crate) redaction_findings: Vec<String>,
    pub(crate) notes: Vec<String>,
    pub(crate) recommended_next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AggregateDelta {
    pub(crate) before_pass_rate: f64,
    pub(crate) after_pass_rate: f64,
    pub(crate) pass_rate_delta: f64,
    pub(crate) before_failed_iterations: usize,
    pub(crate) after_failed_iterations: usize,
    pub(crate) failed_iterations_delta: i64,
    pub(crate) before_failed_scenario_runs: usize,
    pub(crate) after_failed_scenario_runs: usize,
    pub(crate) failed_scenario_runs_delta: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RateDelta {
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) before_pass_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) after_pass_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) pass_rate_delta: Option<f64>,
    pub(crate) before_failed: usize,
    pub(crate) after_failed: usize,
    pub(crate) failed_delta: i64,
    pub(crate) status: String,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FailedCheckDelta {
    pub(crate) check: String,
    pub(crate) before_count: usize,
    pub(crate) after_count: usize,
    pub(crate) delta: i64,
}

pub(crate) fn compare_baselines(
    config: &BaselineCompareConfig,
) -> Result<BaselineCompareReport, EvalError> {
    let before_raw = fs::read_to_string(&config.before_path)
        .map_err(|source| EvalError::io("read", &config.before_path, source))?;
    let after_raw = fs::read_to_string(&config.after_path)
        .map_err(|source| EvalError::io("read", &config.after_path, source))?;
    let before: BaselineReport = serde_json::from_str(&before_raw)
        .map_err(|source| EvalError::parse_json(&config.before_path, source))?;
    let after: BaselineReport = serde_json::from_str(&after_raw)
        .map_err(|source| EvalError::parse_json(&config.after_path, source))?;
    Ok(compare_baseline_reports(
        config,
        &before_raw,
        &after_raw,
        &before,
        &after,
    ))
}

fn compare_baseline_reports(
    config: &BaselineCompareConfig,
    before_raw: &str,
    after_raw: &str,
    before: &BaselineReport,
    after: &BaselineReport,
) -> BaselineCompareReport {
    let mut notes = Vec::new();
    let comparable = before.suite == after.suite && before.target == after.target;
    if !comparable {
        notes.push(format!(
            "baseline metadata mismatch: before suite={} target={}, after suite={} target={}",
            before.suite, before.target, after.suite, after.target
        ));
    }

    let mut redaction_findings = redaction::findings(before_raw);
    redaction_findings.extend(redaction::findings(after_raw));
    redaction_findings.sort();
    redaction_findings.dedup();

    let aggregate = AggregateDelta {
        before_pass_rate: before.pass_rate,
        after_pass_rate: after.pass_rate,
        pass_rate_delta: after.pass_rate - before.pass_rate,
        before_failed_iterations: before.failed_iterations,
        after_failed_iterations: after.failed_iterations,
        failed_iterations_delta: after.failed_iterations as i64 - before.failed_iterations as i64,
        before_failed_scenario_runs: before.failed_scenario_runs,
        after_failed_scenario_runs: after.failed_scenario_runs,
        failed_scenario_runs_delta: after.failed_scenario_runs as i64
            - before.failed_scenario_runs as i64,
    };

    let category_changes = compare_categories(
        &before.category_pass_rates,
        &after.category_pass_rates,
        config.max_category_drop,
    );
    let scenario_changes = compare_scenarios(before, after);
    let new_failed_scenarios = scenario_changes
        .iter()
        .filter(|change| change.before_failed == 0 && change.after_failed > 0)
        .map(|change| change.name.clone())
        .collect::<Vec<_>>();
    let resolved_scenarios = scenario_changes
        .iter()
        .filter(|change| change.before_failed > 0 && change.after_failed == 0)
        .map(|change| change.name.clone())
        .collect::<Vec<_>>();
    let regressed_scenarios = scenario_changes
        .iter()
        .filter(|change| change.status == "regressed")
        .map(|change| change.name.clone())
        .collect::<Vec<_>>();
    let improved_scenarios = scenario_changes
        .iter()
        .filter(|change| change.status == "improved")
        .map(|change| change.name.clone())
        .collect::<Vec<_>>();
    let new_failed_checks = compare_failed_checks(
        &before.failure_summary.failed_checks,
        &after.failure_summary.failed_checks,
    );

    let aggregate_regressed = aggregate.pass_rate_delta < -config.max_pass_rate_drop;
    if aggregate_regressed {
        notes.push(format!(
            "aggregate pass_rate dropped by {:.2} percentage point(s)",
            aggregate.pass_rate_delta.abs() * 100.0
        ));
    }
    let category_regressed = category_changes.iter().any(|change| {
        change
            .pass_rate_delta
            .is_some_and(|delta| delta < -config.max_category_drop)
    });
    let scenario_regressed = !new_failed_scenarios.is_empty() || !regressed_scenarios.is_empty();
    let check_regressed = !new_failed_checks.is_empty();
    let regression_detected =
        aggregate_regressed || category_regressed || scenario_regressed || check_regressed;

    if !redaction_findings.is_empty() {
        notes.push(format!(
            "redaction findings present: {}",
            redaction_findings.join(", ")
        ));
    }

    let ok = comparable && redaction_findings.is_empty() && !regression_detected;
    let recommended_next_actions = recommended_actions(
        regression_detected,
        &new_failed_scenarios,
        &regressed_scenarios,
        &resolved_scenarios,
        &new_failed_checks,
    );

    BaselineCompareReport {
        report_schema_version: REPORT_SCHEMA_VERSION,
        command: "baseline-compare",
        before: safe_display_path(&config.before_path),
        after: safe_display_path(&config.after_path),
        suite: after.suite.clone(),
        target: after.target.clone(),
        comparable,
        ok,
        regression_detected,
        aggregate,
        category_changes,
        scenario_changes,
        new_failed_scenarios,
        resolved_scenarios,
        regressed_scenarios,
        improved_scenarios,
        new_failed_checks,
        redaction_findings,
        notes,
        recommended_next_actions,
    }
}

fn compare_categories(
    before: &[BaselineCategorySummary],
    after: &[BaselineCategorySummary],
    max_category_drop: f64,
) -> Vec<RateDelta> {
    let before_map = before
        .iter()
        .map(|summary| (summary.category.as_str(), summary))
        .collect::<BTreeMap<_, _>>();
    let after_map = after
        .iter()
        .map(|summary| (summary.category.as_str(), summary))
        .collect::<BTreeMap<_, _>>();
    let names = before_map
        .keys()
        .chain(after_map.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    names
        .into_iter()
        .map(|name| {
            let before = before_map.get(name).copied();
            let after = after_map.get(name).copied();
            rate_delta(
                name,
                before.map(|summary| summary.pass_rate),
                after.map(|summary| summary.pass_rate),
                before.map_or(0, |summary| summary.failed),
                after.map_or(0, |summary| summary.failed),
                max_category_drop,
            )
        })
        .collect()
}

fn compare_scenarios(before: &BaselineReport, after: &BaselineReport) -> Vec<RateDelta> {
    let before_map = before
        .scenario_pass_rates
        .iter()
        .map(|summary| (summary.scenario_id.as_str(), summary))
        .collect::<BTreeMap<_, _>>();
    let after_map = after
        .scenario_pass_rates
        .iter()
        .map(|summary| (summary.scenario_id.as_str(), summary))
        .collect::<BTreeMap<_, _>>();
    let names = before_map
        .keys()
        .chain(after_map.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    names
        .into_iter()
        .map(|name| {
            let before = before_map.get(name).copied();
            let after = after_map.get(name).copied();
            rate_delta(
                name,
                before.map(|summary| summary.pass_rate),
                after.map(|summary| summary.pass_rate),
                before.map_or(0, |summary| summary.failed),
                after.map_or(0, |summary| summary.failed),
                0.0,
            )
        })
        .collect()
}

fn rate_delta(
    name: &str,
    before_pass_rate: Option<f64>,
    after_pass_rate: Option<f64>,
    before_failed: usize,
    after_failed: usize,
    tolerated_drop: f64,
) -> RateDelta {
    let pass_rate_delta = before_pass_rate
        .zip(after_pass_rate)
        .map(|(before, after)| after - before);
    let status = match (before_pass_rate, after_pass_rate, pass_rate_delta) {
        (None, Some(_), _) => "new",
        (Some(_), None, _) => "removed",
        (Some(_), Some(_), Some(delta)) if delta < -tolerated_drop => "regressed",
        (Some(_), Some(_), Some(delta)) if delta > 0.0 => "improved",
        _ => "unchanged",
    };
    RateDelta {
        name: name.to_owned(),
        before_pass_rate,
        after_pass_rate,
        pass_rate_delta,
        before_failed,
        after_failed,
        failed_delta: after_failed as i64 - before_failed as i64,
        status: status.to_owned(),
    }
}

fn compare_failed_checks(
    before: &[BaselineFailedCheckSummary],
    after: &[BaselineFailedCheckSummary],
) -> Vec<FailedCheckDelta> {
    let before_map = before
        .iter()
        .map(|check| (check.check.as_str(), check.count))
        .collect::<BTreeMap<_, _>>();
    let after_map = after
        .iter()
        .map(|check| (check.check.as_str(), check.count))
        .collect::<BTreeMap<_, _>>();
    after_map
        .iter()
        .filter_map(|(check, after_count)| {
            let before_count = before_map.get(check).copied().unwrap_or(0);
            (*after_count > before_count).then(|| FailedCheckDelta {
                check: (*check).to_owned(),
                before_count,
                after_count: *after_count,
                delta: *after_count as i64 - before_count as i64,
            })
        })
        .collect()
}

fn recommended_actions(
    regression_detected: bool,
    new_failed_scenarios: &[String],
    regressed_scenarios: &[String],
    resolved_scenarios: &[String],
    new_failed_checks: &[FailedCheckDelta],
) -> Vec<String> {
    let mut actions = Vec::new();
    if let Some(scenario) = new_failed_scenarios.first() {
        actions.push(format!(
            "inspect newly failing scenario `{scenario}` and create or promote a candidate if it is a real regression"
        ));
    }
    if let Some(scenario) = regressed_scenarios.first() {
        actions.push(format!(
            "replay regressed scenario `{scenario}` against the same target"
        ));
    }
    if let Some(check) = new_failed_checks.first() {
        actions.push(format!(
            "start with newly increased failed check `{}` (delta={})",
            check.check, check.delta
        ));
    }
    if !resolved_scenarios.is_empty() {
        actions.push(format!(
            "keep resolved scenarios in the suite to prevent regressions: {}",
            resolved_scenarios.join(", ")
        ));
    }
    if !regression_detected {
        actions.push(
            "no regression detected; keep the comparison report with release evidence".to_owned(),
        );
    }
    actions
}

fn safe_display_path(path: &Path) -> String {
    let display = match std::env::current_dir()
        .ok()
        .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
    {
        Some(relative) => relative.display().to_string(),
        None => path.display().to_string(),
    };
    redaction::sanitize_text(&display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        BaselineMetadata, BaselineRunReport, BaselineScenarioMetadata, CheckReport, ScenarioReport,
    };
    use crate::target::{EvalTargetMetadata, TargetKind};

    #[test]
    fn compare_baselines_detects_regression() -> Result<(), Box<dyn std::error::Error>> {
        let before = baseline_report(true);
        let after = baseline_report(false);
        let config = BaselineCompareConfig {
            before_path: PathBuf::from("before.json"),
            after_path: PathBuf::from("after.json"),
            max_pass_rate_drop: 0.0,
            max_category_drop: 0.0,
        };
        let report = compare_baseline_reports(
            &config,
            &serde_json::to_string(&before)?,
            &serde_json::to_string(&after)?,
            &before,
            &after,
        );
        assert!(!report.ok);
        assert!(report.regression_detected);
        assert_eq!(report.aggregate.pass_rate_delta, -1.0);
        assert_eq!(report.new_failed_scenarios, vec!["scenario-a"]);
        assert_eq!(report.new_failed_checks[0].check, "check");
        Ok(())
    }

    #[test]
    fn compare_baselines_detects_resolution() -> Result<(), Box<dyn std::error::Error>> {
        let before = baseline_report(false);
        let after = baseline_report(true);
        let config = BaselineCompareConfig {
            before_path: PathBuf::from("before.json"),
            after_path: PathBuf::from("after.json"),
            max_pass_rate_drop: 0.0,
            max_category_drop: 0.0,
        };
        let report = compare_baseline_reports(
            &config,
            &serde_json::to_string(&before)?,
            &serde_json::to_string(&after)?,
            &before,
            &after,
        );
        assert!(report.ok);
        assert!(!report.regression_detected);
        assert_eq!(report.resolved_scenarios, vec!["scenario-a"]);
        assert!(report.recommended_next_actions[0].contains("resolved"));
        Ok(())
    }

    fn baseline_report(passing: bool) -> BaselineReport {
        let check = if passing {
            CheckReport::pass("check", "expected", "expected")
        } else {
            CheckReport::fail("check", "expected", "actual", "artifact")
        };
        let eval = crate::report::EvalReport::from_results(
            TargetKind::MnemeV1Command.as_str(),
            EvalTargetMetadata::command(true),
            vec![ScenarioReport::new(
                "scenario-a".to_owned(),
                vec!["category-recall".to_owned()],
                vec![check],
            )],
        );
        BaselineReport::from_runs(
            "model",
            TargetKind::MnemeV1Command.as_str(),
            EvalTargetMetadata::command(true),
            BaselineMetadata::new(
                false,
                Some("openai".to_owned()),
                Some("dry-run".to_owned()),
                Some("test-run".to_owned()),
            ),
            vec![BaselineScenarioMetadata::new(
                "scenario-a",
                vec!["category-recall".to_owned()],
            )],
            vec![BaselineRunReport::from_eval_report(1, eval)],
        )
    }
}
