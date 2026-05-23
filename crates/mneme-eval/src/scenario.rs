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
    #[serde(default)]
    pub(crate) persistence: Option<Persistence>,
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
pub(crate) struct Persistence {
    pub(crate) restart_after_event: usize,
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

impl Expected {
    fn is_empty(&self) -> bool {
        self.event_append.is_none()
            && self.claims.is_empty()
            && self.context_pack.is_none()
            && self.budget.is_none()
            && self.audit.is_none()
    }
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
    if scenario.budget.daily_cloud_tokens == 0 {
        return Err(EvalError::scenario(format!(
            "scenario {} has a zero daily_cloud_tokens budget",
            scenario.id
        )));
    }
    if scenario.events.is_empty() {
        return Err(EvalError::scenario(format!(
            "scenario {} has no events",
            scenario.id
        )));
    }
    if let Some(persistence) = &scenario.persistence {
        if persistence.restart_after_event == 0 {
            return Err(EvalError::scenario(format!(
                "scenario {} persistence restart_after_event must be greater than zero",
                scenario.id
            )));
        }
        if persistence.restart_after_event > scenario.events.len() {
            return Err(EvalError::scenario(format!(
                "scenario {} persistence restart_after_event exceeds event count",
                scenario.id
            )));
        }
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
    if scenario.expected.is_empty() {
        return Err(EvalError::scenario(format!(
            "scenario {} has no expected checks",
            scenario.id
        )));
    }
    for (idx, claim) in scenario.expected.claims.iter().enumerate() {
        if claim.subject.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {} expected claim {} has an empty subject",
                scenario.id,
                idx + 1
            )));
        }
        if claim.predicate.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {} expected claim {} has an empty predicate",
                scenario.id,
                idx + 1
            )));
        }
        if claim.object.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {} expected claim {} has an empty object",
                scenario.id,
                idx + 1
            )));
        }
        if claim
            .status
            .as_ref()
            .is_some_and(|status| status.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} expected claim {} has an empty status",
                scenario.id,
                idx + 1
            )));
        }
        if claim
            .scope
            .as_ref()
            .is_some_and(|scope| scope.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} expected claim {} has an empty scope",
                scenario.id,
                idx + 1
            )));
        }
    }
    if let Some(context_pack) = &scenario.expected.context_pack {
        if context_pack
            .must_include
            .iter()
            .any(|needle| needle.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} context_pack has an empty must_include entry",
                scenario.id
            )));
        }
        if context_pack
            .must_not_include
            .iter()
            .any(|needle| needle.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} context_pack has an empty must_not_include entry",
                scenario.id
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
