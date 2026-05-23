use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct EvalReport {
    pub(crate) ok: bool,
    pub(crate) scenario_count: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) results: Vec<ScenarioReport>,
}

impl EvalReport {
    pub(crate) fn from_results(results: Vec<ScenarioReport>) -> Self {
        let scenario_count = results.len();
        let passed = results.iter().filter(|result| result.ok).count();
        let failed = scenario_count.saturating_sub(passed);
        Self {
            ok: failed == 0,
            scenario_count,
            passed,
            failed,
            results,
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
