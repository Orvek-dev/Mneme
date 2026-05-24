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
    #[serde(default)]
    pub(crate) maintenance: Maintenance,
    #[serde(default)]
    pub(crate) agent_flow: Option<AgentFlow>,
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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Maintenance {
    pub(crate) export_import_roundtrip: bool,
    pub(crate) compact_after_events: bool,
    pub(crate) repair_from_backup: bool,
    pub(crate) curation: Option<CurationMaintenance>,
    pub(crate) restore_from_backup: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CurationMaintenance {
    pub(crate) apply: bool,
    pub(crate) compact: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentFlow {
    pub(crate) begin: AgentBegin,
    #[serde(default)]
    pub(crate) end: Option<AgentEnd>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentBegin {
    pub(crate) task: String,
    #[serde(default)]
    pub(crate) actor_agent_id: Option<String>,
    #[serde(default)]
    pub(crate) query: Option<String>,
    #[serde(default)]
    pub(crate) allowed_scopes: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AgentEnd {
    pub(crate) summary: Option<String>,
    pub(crate) remember: Vec<String>,
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
    pub(crate) store: Option<StoreExpected>,
    pub(crate) session: Option<SessionExpected>,
    pub(crate) quality: Option<QualityExpected>,
    pub(crate) curation: Option<CurationExpected>,
}

impl Expected {
    fn is_empty(&self) -> bool {
        self.event_append.is_none()
            && self.claims.is_empty()
            && self.context_pack.is_none()
            && self.budget.is_none()
            && self.audit.is_none()
            && self.store.is_none()
            && self.session.is_none()
            && self.quality.is_none()
            && self.curation.is_none()
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
    pub(crate) allowed_scopes: Vec<String>,
    pub(crate) max_items: Option<usize>,
    pub(crate) item_count: Option<usize>,
    pub(crate) must_include: Vec<String>,
    pub(crate) must_not_include: Vec<String>,
    pub(crate) expected_order: Vec<String>,
    pub(crate) omitted_reason_contains: Vec<String>,
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
    pub(crate) claim_update_required: bool,
    pub(crate) session_events_required: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct StoreExpected {
    pub(crate) schema_version: Option<u32>,
    pub(crate) valid: bool,
    pub(crate) backup_required: bool,
    pub(crate) repair_performed: bool,
    pub(crate) restored: bool,
    pub(crate) compacted: bool,
    pub(crate) imported: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SessionExpected {
    pub(crate) status: Option<String>,
    pub(crate) task: Option<String>,
    pub(crate) actor_agent_id: Option<String>,
    pub(crate) context_must_include: Vec<String>,
    pub(crate) memory_event_count: Option<usize>,
    pub(crate) summary_contains: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct QualityExpected {
    pub(crate) duplicate_active_groups: Option<usize>,
    pub(crate) duplicate_active_claims: Option<usize>,
    pub(crate) blocked_secret_count: Option<usize>,
    pub(crate) inactive_claim_count: Option<usize>,
    pub(crate) review_item_count: Option<usize>,
    pub(crate) finding_kinds: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CurationExpected {
    pub(crate) duplicate_forget_count: Option<usize>,
    pub(crate) blocked_secret_review_count: Option<usize>,
    pub(crate) compact_recommended: Option<bool>,
    pub(crate) compacted: Option<bool>,
    pub(crate) changed: Option<bool>,
    pub(crate) before_quality: Option<QualityExpected>,
    pub(crate) after_quality: Option<QualityExpected>,
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
    if scenario
        .maintenance
        .curation
        .as_ref()
        .is_some_and(|curation| curation.compact && !curation.apply)
    {
        return Err(EvalError::scenario(format!(
            "scenario {} maintenance curation compact requires apply",
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
    if let Some(agent_flow) = &scenario.agent_flow {
        if agent_flow.begin.task.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {} agent_flow begin task must not be empty",
                scenario.id
            )));
        }
        if agent_flow
            .begin
            .query
            .as_ref()
            .is_some_and(|query| query.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} agent_flow begin query must not be empty",
                scenario.id
            )));
        }
        if agent_flow
            .begin
            .allowed_scopes
            .iter()
            .any(|scope| scope.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} agent_flow begin allowed_scopes entries must not be empty",
                scenario.id
            )));
        }
        if let Some(end) = &agent_flow.end {
            if end
                .summary
                .as_ref()
                .is_some_and(|summary| summary.trim().is_empty())
            {
                return Err(EvalError::scenario(format!(
                    "scenario {} agent_flow end summary must not be empty",
                    scenario.id
                )));
            }
            if end.remember.iter().any(|claim| claim.trim().is_empty()) {
                return Err(EvalError::scenario(format!(
                    "scenario {} agent_flow end remember entries must not be empty",
                    scenario.id
                )));
            }
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
        if context_pack
            .expected_order
            .iter()
            .any(|needle| needle.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} context_pack has an empty expected_order entry",
                scenario.id
            )));
        }
        if context_pack
            .allowed_scopes
            .iter()
            .any(|scope| scope.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} context_pack has an empty allowed_scopes entry",
                scenario.id
            )));
        }
        if context_pack
            .omitted_reason_contains
            .iter()
            .any(|reason| reason.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {} context_pack has an empty omitted_reason_contains entry",
                scenario.id
            )));
        }
    }
    if let Some(quality) = &scenario.expected.quality {
        validate_quality_expected(quality, &scenario.id, "quality")?;
    }
    if let Some(curation) = &scenario.expected.curation {
        if let Some(before_quality) = &curation.before_quality {
            validate_quality_expected(before_quality, &scenario.id, "curation.before_quality")?;
        }
        if let Some(after_quality) = &curation.after_quality {
            validate_quality_expected(after_quality, &scenario.id, "curation.after_quality")?;
        }
    }
    Ok(())
}

fn validate_quality_expected(
    quality: &QualityExpected,
    scenario_id: &str,
    field: &str,
) -> Result<(), EvalError> {
    if quality
        .finding_kinds
        .iter()
        .any(|kind| kind.trim().is_empty())
    {
        return Err(EvalError::scenario(format!(
            "scenario {scenario_id} {field} has an empty finding_kinds entry"
        )));
    }
    Ok(())
}

fn default_scope() -> String {
    "private".to_owned()
}

fn default_trust_level() -> String {
    "trusted_user".to_owned()
}
