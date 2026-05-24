use serde::Serialize;

use crate::target::EvalTargetMetadata;

const REPORT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvalReport {
    pub(crate) report_schema_version: u32,
    pub(crate) target: String,
    pub(crate) target_metadata: EvalTargetMetadata,
    pub(crate) ok: bool,
    pub(crate) scenario_count: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) results: Vec<ScenarioReport>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BaselineReport {
    pub(crate) report_schema_version: u32,
    pub(crate) suite: String,
    pub(crate) target: String,
    pub(crate) target_metadata: EvalTargetMetadata,
    pub(crate) ok: bool,
    pub(crate) iterations: usize,
    pub(crate) passed_iterations: usize,
    pub(crate) failed_iterations: usize,
    pub(crate) scenario_count: usize,
    pub(crate) total_scenario_runs: usize,
    pub(crate) passed_scenario_runs: usize,
    pub(crate) failed_scenario_runs: usize,
    pub(crate) pass_rate: f64,
    pub(crate) scenario_pass_rates: Vec<BaselineScenarioSummary>,
    pub(crate) runs: Vec<BaselineRunReport>,
}

impl BaselineReport {
    pub(crate) fn from_runs(
        suite: impl Into<String>,
        target: impl Into<String>,
        target_metadata: EvalTargetMetadata,
        scenario_ids: Vec<String>,
        runs: Vec<BaselineRunReport>,
    ) -> Self {
        let iterations = runs.len();
        let scenario_count = scenario_ids.len();
        let passed_iterations = runs.iter().filter(|run| run.ok).count();
        let failed_iterations = iterations.saturating_sub(passed_iterations);
        let total_scenario_runs = iterations.saturating_mul(scenario_count);
        let passed_scenario_runs = runs.iter().map(|run| run.passed).sum();
        let failed_scenario_runs = total_scenario_runs.saturating_sub(passed_scenario_runs);
        let pass_rate = rate(passed_scenario_runs, total_scenario_runs);
        let scenario_pass_rates = scenario_ids
            .iter()
            .map(|scenario_id| BaselineScenarioSummary::from_runs(scenario_id, &runs))
            .collect();
        Self {
            report_schema_version: REPORT_SCHEMA_VERSION,
            suite: suite.into(),
            target: target.into(),
            target_metadata,
            ok: failed_iterations == 0 && failed_scenario_runs == 0,
            iterations,
            passed_iterations,
            failed_iterations,
            scenario_count,
            total_scenario_runs,
            passed_scenario_runs,
            failed_scenario_runs,
            pass_rate,
            scenario_pass_rates,
            runs,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BaselineScenarioSummary {
    pub(crate) scenario_id: String,
    pub(crate) attempts: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) pass_rate: f64,
}

impl BaselineScenarioSummary {
    fn from_runs(scenario_id: &str, runs: &[BaselineRunReport]) -> Self {
        let attempts = runs.len();
        let passed = runs
            .iter()
            .filter(|run| {
                run.results
                    .iter()
                    .any(|result| result.scenario_id == scenario_id && result.ok)
            })
            .count();
        let failed = attempts.saturating_sub(passed);
        Self {
            scenario_id: scenario_id.to_owned(),
            attempts,
            passed,
            failed,
            pass_rate: rate(passed, attempts),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BaselineRunReport {
    pub(crate) iteration: usize,
    pub(crate) ok: bool,
    pub(crate) scenario_count: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    pub(crate) results: Vec<BaselineScenarioRunReport>,
}

impl BaselineRunReport {
    pub(crate) fn from_eval_report(iteration: usize, report: EvalReport) -> Self {
        let results = report
            .results
            .into_iter()
            .map(BaselineScenarioRunReport::from_scenario_report)
            .collect::<Vec<_>>();
        Self {
            iteration,
            ok: report.ok,
            scenario_count: report.scenario_count,
            passed: report.passed,
            failed: report.failed,
            error: None,
            results,
        }
    }

    pub(crate) fn from_error(
        iteration: usize,
        scenario_ids: &[String],
        error: impl Into<String>,
    ) -> Self {
        let results = scenario_ids
            .iter()
            .map(|scenario_id| BaselineScenarioRunReport::error(scenario_id.clone()))
            .collect::<Vec<_>>();
        Self {
            iteration,
            ok: false,
            scenario_count: scenario_ids.len(),
            passed: 0,
            failed: scenario_ids.len(),
            error: Some(error.into()),
            results,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BaselineScenarioRunReport {
    pub(crate) scenario_id: String,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) failed_checks: Vec<String>,
}

impl BaselineScenarioRunReport {
    fn from_scenario_report(report: ScenarioReport) -> Self {
        let failed_checks = report
            .checks
            .iter()
            .filter(|check| check.status == CheckStatus::Fail)
            .map(|check| check.name.clone())
            .collect();
        Self {
            scenario_id: report.scenario_id,
            ok: report.ok,
            failed_checks,
        }
    }

    fn error(scenario_id: String) -> Self {
        Self {
            scenario_id,
            ok: false,
            failed_checks: Vec::new(),
        }
    }
}

fn rate(passed: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        passed as f64 / total as f64
    }
}

impl EvalReport {
    pub(crate) fn from_results(
        target: impl Into<String>,
        target_metadata: EvalTargetMetadata,
        results: Vec<ScenarioReport>,
    ) -> Self {
        let scenario_count = results.len();
        let passed = results.iter().filter(|result| result.ok).count();
        let failed = scenario_count.saturating_sub(passed);
        Self {
            report_schema_version: REPORT_SCHEMA_VERSION,
            target: target.into(),
            target_metadata,
            ok: failed == 0,
            scenario_count,
            passed,
            failed,
            results,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ValidationReport {
    pub(crate) report_schema_version: u32,
    pub(crate) ok: bool,
    pub(crate) scenario_count: usize,
    pub(crate) valid: usize,
    pub(crate) invalid: usize,
    pub(crate) results: Vec<ScenarioValidationReport>,
}

impl ValidationReport {
    pub(crate) fn from_results(results: Vec<ScenarioValidationReport>) -> Self {
        let scenario_count = results.len();
        let valid = results.iter().filter(|result| result.ok).count();
        let invalid = scenario_count.saturating_sub(valid);
        Self {
            report_schema_version: REPORT_SCHEMA_VERSION,
            ok: invalid == 0,
            scenario_count,
            valid,
            invalid,
            results,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AcceptanceReport {
    pub(crate) report_schema_version: u32,
    pub(crate) target: String,
    pub(crate) ok: bool,
    pub(crate) gate_count: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) gates: Vec<AcceptanceGateReport>,
}

impl AcceptanceReport {
    pub(crate) fn from_gates(target: impl Into<String>, gates: Vec<AcceptanceGateReport>) -> Self {
        let gate_count = gates.len();
        let passed = gates
            .iter()
            .filter(|gate| gate.status == CheckStatus::Pass)
            .count();
        let failed = gate_count.saturating_sub(passed);
        Self {
            report_schema_version: REPORT_SCHEMA_VERSION,
            target: target.into(),
            ok: failed == 0,
            gate_count,
            passed,
            failed,
            gates,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct AcceptanceGateReport {
    pub(crate) name: String,
    pub(crate) status: CheckStatus,
    pub(crate) detail: String,
}

impl AcceptanceGateReport {
    pub(crate) fn pass(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }

    pub(crate) fn fail(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScenarioValidationReport {
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) scenario_id: Option<String>,
    pub(crate) tags: Vec<String>,
    pub(crate) ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

impl ScenarioValidationReport {
    pub(crate) fn pass(path: String, scenario_id: String, tags: Vec<String>) -> Self {
        Self {
            path,
            scenario_id: Some(scenario_id),
            tags,
            ok: true,
            error: None,
        }
    }

    pub(crate) fn fail(path: String, error: String) -> Self {
        Self {
            path,
            scenario_id: None,
            tags: Vec::new(),
            ok: false,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ScenarioReport {
    pub(crate) scenario_id: String,
    pub(crate) tags: Vec<String>,
    pub(crate) ok: bool,
    pub(crate) checks: Vec<CheckReport>,
}

impl ScenarioReport {
    pub(crate) fn new(scenario_id: String, tags: Vec<String>, checks: Vec<CheckReport>) -> Self {
        let ok = checks.iter().all(|check| check.status == CheckStatus::Pass);
        Self {
            scenario_id,
            tags,
            ok,
            checks,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CheckReport {
    pub(crate) name: String,
    pub(crate) status: CheckStatus,
    pub(crate) expected: String,
    pub(crate) actual: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) artifact: Option<String>,
}

impl CheckReport {
    pub(crate) fn pass(
        name: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Pass,
            expected: expected.into(),
            actual: actual.into(),
            artifact: None,
        }
    }

    pub(crate) fn fail(
        name: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
        artifact: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: CheckStatus::Fail,
            expected: expected.into(),
            actual: actual.into(),
            artifact: Some(artifact.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CheckStatus {
    Pass,
    Fail,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_report_json_preserves_schema_contract() -> Result<(), serde_json::Error> {
        let scenario = ScenarioReport::new(
            "example".to_owned(),
            Vec::new(),
            vec![CheckReport::pass("check", "expected", "expected")],
        );

        let report = EvalReport::from_results("fake", EvalTargetMetadata::fake(), vec![scenario]);
        let json = serde_json::to_value(&report)?;

        assert_eq!(json["report_schema_version"], REPORT_SCHEMA_VERSION);
        assert_eq!(json["target"], "fake");
        assert_eq!(json["target_metadata"]["extractor"], "fixture");
        assert_eq!(json["target_metadata"]["opt_in"], false);
        assert_eq!(json["ok"], true);
        assert_eq!(json["scenario_count"], 1);
        assert_eq!(json["passed"], 1);
        assert_eq!(json["failed"], 0);
        assert_eq!(json["results"][0]["scenario_id"], "example");
        assert_eq!(json["results"][0]["checks"][0]["status"], "pass");
        assert!(json["results"][0]["checks"][0].get("artifact").is_none());
        Ok(())
    }

    #[test]
    fn acceptance_report_json_preserves_schema_contract() -> Result<(), serde_json::Error> {
        let report =
            AcceptanceReport::from_gates("fake", vec![AcceptanceGateReport::pass("gate", "ok")]);
        let json = serde_json::to_value(&report)?;

        assert_eq!(json["report_schema_version"], REPORT_SCHEMA_VERSION);
        assert_eq!(json["target"], "fake");
        assert_eq!(json["ok"], true);
        assert_eq!(json["gate_count"], 1);
        assert_eq!(json["passed"], 1);
        assert_eq!(json["failed"], 0);
        assert_eq!(json["gates"][0]["name"], "gate");
        assert_eq!(json["gates"][0]["status"], "pass");
        assert_eq!(json["gates"][0]["detail"], "ok");
        Ok(())
    }

    #[test]
    fn baseline_report_json_preserves_schema_contract() -> Result<(), serde_json::Error> {
        let passed = EvalReport::from_results(
            "mneme-v1-command",
            EvalTargetMetadata::command(true),
            vec![ScenarioReport::new(
                "scenario-a".to_owned(),
                Vec::new(),
                vec![CheckReport::pass("check", "expected", "expected")],
            )],
        );
        let failed = EvalReport::from_results(
            "mneme-v1-command",
            EvalTargetMetadata::command(true),
            vec![ScenarioReport::new(
                "scenario-a".to_owned(),
                Vec::new(),
                vec![CheckReport::fail("check", "expected", "actual", "artifact")],
            )],
        );
        let report = BaselineReport::from_runs(
            "model",
            "mneme-v1-command",
            EvalTargetMetadata::command(true),
            vec!["scenario-a".to_owned()],
            vec![
                BaselineRunReport::from_eval_report(1, passed),
                BaselineRunReport::from_eval_report(2, failed),
            ],
        );
        let json = serde_json::to_value(&report)?;

        assert_eq!(json["report_schema_version"], REPORT_SCHEMA_VERSION);
        assert_eq!(json["suite"], "model");
        assert_eq!(json["target"], "mneme-v1-command");
        assert_eq!(json["target_metadata"]["command_configured"], true);
        assert_eq!(json["ok"], false);
        assert_eq!(json["iterations"], 2);
        assert_eq!(json["passed_iterations"], 1);
        assert_eq!(json["failed_iterations"], 1);
        assert_eq!(json["scenario_count"], 1);
        assert_eq!(json["total_scenario_runs"], 2);
        assert_eq!(json["passed_scenario_runs"], 1);
        assert_eq!(json["failed_scenario_runs"], 1);
        assert_eq!(json["pass_rate"], 0.5);
        assert_eq!(json["scenario_pass_rates"][0]["scenario_id"], "scenario-a");
        assert_eq!(json["runs"][1]["results"][0]["failed_checks"][0], "check");
        Ok(())
    }
}
