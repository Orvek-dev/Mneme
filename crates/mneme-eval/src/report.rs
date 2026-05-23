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
}
