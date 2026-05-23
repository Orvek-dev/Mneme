use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::EvalError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Scenario {
    pub(crate) id: String,
    #[serde(default)]
    pub(crate) tags: Vec<String>,
    #[serde(default)]
    pub(crate) budget: Budget,
    pub(crate) events: Vec<InputEvent>,
    pub(crate) expected: Expected,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Budget {
    pub(crate) daily_cloud_tokens: u32,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            daily_cloud_tokens: 100_000,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InputEvent {
    pub(crate) speaker_id: String,
    #[serde(default)]
    pub(crate) actor_agent_id: Option<String>,
    pub(crate) text: String,
    #[serde(default = "default_scope")]
    pub(crate) scope: String,
    #[serde(default = "default_trust_level")]
    pub(crate) trust_level: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Expected {
    pub(crate) event_append: Option<EventAppendExpected>,
    pub(crate) claims: Vec<ClaimExpected>,
    pub(crate) context_pack: Option<ContextPackExpected>,
    pub(crate) budget: Option<BudgetExpected>,
    pub(crate) audit: Option<AuditExpected>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventAppendExpected {
    pub(crate) count: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ClaimExpected {
    pub(crate) subject: String,
    pub(crate) predicate: String,
    pub(crate) object: String,
    pub(crate) status: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) must_not_exist: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ContextPackExpected {
    pub(crate) query: String,
    pub(crate) must_include: Vec<String>,
    pub(crate) must_not_include: Vec<String>,
    pub(crate) citation_required: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BudgetExpected {
    pub(crate) hard_cap_violations: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AuditExpected {
    pub(crate) read_write_events_required: bool,
}

pub(crate) fn load_scenario(path: &Path) -> Result<Scenario, EvalError> {
    let text = fs::read_to_string(path).map_err(|source| EvalError::io("read", path, source))?;
    let scenario: Scenario =
        serde_yaml::from_str(&text).map_err(|source| EvalError::parse(path, source))?;
    validate_scenario(&scenario, path)?;
    Ok(scenario)
}

fn validate_scenario(scenario: &Scenario, path: &Path) -> Result<(), EvalError> {
    if scenario.id.trim().is_empty() {
        return Err(EvalError::scenario(format!(
            "scenario {} has an empty id",
            path.display()
        )));
    }
    if scenario.events.is_empty() {
        return Err(EvalError::scenario(format!(
            "scenario {} has no events",
            scenario.id
        )));
    }
    for (idx, event) in scenario.events.iter().enumerate() {
        if event.speaker_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {} event {} has an empty speaker_id",
                scenario.id,
                idx + 1
            )));
        }
        if event.text.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {} event {} has empty text",
                scenario.id,
                idx + 1
            )));
        }
    }
    Ok(())
}

fn default_scope() -> String {
    "private".to_owned()
}

fn default_trust_level() -> String {
    "trusted_user".to_owned()
}
