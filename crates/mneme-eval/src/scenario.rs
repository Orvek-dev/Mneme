use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::EvalError;

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[serde(default)]
    pub(crate) team_flow: Option<TeamFlow>,
    #[serde(default)]
    pub(crate) events: Vec<InputEvent>,
    pub(crate) expected: Expected,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Persistence {
    pub(crate) restart_after_event: usize,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Maintenance {
    pub(crate) export_import_roundtrip: bool,
    pub(crate) compact_after_events: bool,
    pub(crate) repair_from_backup: bool,
    pub(crate) curation: Option<CurationMaintenance>,
    pub(crate) restore_from_backup: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CurationMaintenance {
    pub(crate) apply: bool,
    pub(crate) compact: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AgentFlow {
    pub(crate) begin: AgentBegin,
    #[serde(default)]
    pub(crate) end: Option<AgentEnd>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AgentEnd {
    pub(crate) summary: Option<String>,
    pub(crate) remember: Vec<String>,
    pub(crate) extractor: AgentEndExtractor,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentEndExtractor {
    #[default]
    Rule,
    Command,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TeamFlow {
    pub(crate) workspace_id: Option<String>,
    pub(crate) users: Vec<TeamFlowUser>,
    pub(crate) agents: Vec<TeamFlowAgent>,
    pub(crate) projects: Vec<TeamFlowProject>,
    pub(crate) grants: Vec<TeamFlowGrant>,
    pub(crate) memories: Vec<TeamFlowMemory>,
    pub(crate) promotions: Vec<TeamFlowPromotion>,
    pub(crate) reviews: Vec<TeamFlowReview>,
    pub(crate) revoke_users: Vec<TeamFlowRevocation>,
    pub(crate) revoke_agents: Vec<TeamFlowRevocation>,
    pub(crate) contexts: Vec<TeamFlowContext>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowUser {
    pub(crate) id: String,
    pub(crate) role: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowAgent {
    pub(crate) id: String,
    pub(crate) owner_user_id: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TeamFlowProject {
    pub(crate) id: String,
    pub(crate) members: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowGrant {
    pub(crate) project_id: String,
    pub(crate) user_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowMemory {
    pub(crate) actor: TeamFlowActor,
    pub(crate) text: String,
    pub(crate) scope: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowPromotion {
    pub(crate) actor: TeamFlowActor,
    pub(crate) source_memory_id: String,
    #[serde(default)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowReview {
    pub(crate) actor: TeamFlowActor,
    pub(crate) promotion_id: String,
    pub(crate) approve: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowRevocation {
    pub(crate) actor: TeamFlowActor,
    pub(crate) target_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowContext {
    pub(crate) actor: TeamFlowActor,
    pub(crate) query: String,
    #[serde(default)]
    pub(crate) max_items: Option<usize>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TeamFlowActor {
    pub(crate) user_id: String,
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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
    pub(crate) team: Option<TeamExpected>,
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
            && self.team.is_none()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventAppendExpected {
    pub(crate) count: usize,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ClaimExpected {
    pub(crate) subject: String,
    pub(crate) predicate: String,
    pub(crate) object: String,
    pub(crate) status: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) must_not_exist: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BudgetExpected {
    pub(crate) hard_cap_violations: u32,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct AuditExpected {
    pub(crate) read_write_events_required: bool,
    pub(crate) claim_update_required: bool,
    pub(crate) session_events_required: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SessionExpected {
    pub(crate) status: Option<String>,
    pub(crate) task: Option<String>,
    pub(crate) actor_agent_id: Option<String>,
    pub(crate) context_must_include: Vec<String>,
    pub(crate) memory_event_count: Option<usize>,
    pub(crate) summary_contains: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct QualityExpected {
    pub(crate) duplicate_active_groups: Option<usize>,
    pub(crate) duplicate_active_claims: Option<usize>,
    pub(crate) blocked_secret_count: Option<usize>,
    pub(crate) inactive_claim_count: Option<usize>,
    pub(crate) review_item_count: Option<usize>,
    pub(crate) finding_kinds: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct TeamExpected {
    pub(crate) validation_ok: Option<bool>,
    pub(crate) memory_count: Option<usize>,
    pub(crate) active_memory_count: Option<usize>,
    pub(crate) blocked_secret_count: Option<usize>,
    pub(crate) quarantined_count: Option<usize>,
    pub(crate) promotion_count: Option<usize>,
    pub(crate) pending_promotion_count: Option<usize>,
    pub(crate) approved_promotion_count: Option<usize>,
    pub(crate) rejected_promotion_count: Option<usize>,
    pub(crate) denied_count: Option<usize>,
    pub(crate) scope_leak_count: Option<usize>,
    pub(crate) secret_leak_count: Option<usize>,
    pub(crate) sync_memory_count: Option<usize>,
    pub(crate) sync_omitted_count: Option<usize>,
    pub(crate) handoff_context_item_count: Option<usize>,
    pub(crate) firewall_ok: Option<bool>,
    pub(crate) firewall_high_count: Option<usize>,
    pub(crate) ontology_entity_count: Option<usize>,
    pub(crate) ontology_relation_count: Option<usize>,
    pub(crate) ontology_attribute_count: Option<usize>,
    pub(crate) context_item_count: Option<usize>,
    pub(crate) context_must_include: Vec<String>,
    pub(crate) context_must_not_include: Vec<String>,
    pub(crate) omitted_reason_contains: Vec<String>,
    pub(crate) citation_required: bool,
    pub(crate) audit_kinds: Vec<String>,
}

pub(crate) fn load_scenario(path: &Path) -> Result<Scenario, EvalError> {
    let text = fs::read_to_string(path).map_err(|source| EvalError::io("read", path, source))?;
    let scenario: Scenario =
        serde_yaml::from_str(&text).map_err(|source| EvalError::parse(path, source))?;
    validate_scenario(&scenario, path)?;
    Ok(scenario)
}

pub(crate) fn validate_scenario(scenario: &Scenario, path: &Path) -> Result<(), EvalError> {
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
    if scenario.events.is_empty() && scenario.team_flow.is_none() {
        return Err(EvalError::scenario(format!(
            "scenario {} has no events or team_flow",
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
    if let Some(team_flow) = &scenario.team_flow {
        validate_team_flow(team_flow, &scenario.id)?;
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
    if let Some(team) = &scenario.expected.team {
        validate_team_expected(team, &scenario.id)?;
    }
    Ok(())
}

fn validate_team_flow(team_flow: &TeamFlow, scenario_id: &str) -> Result<(), EvalError> {
    for (idx, user) in team_flow.users.iter().enumerate() {
        if user.id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow user {} has empty id",
                idx + 1
            )));
        }
        if !matches!(user.role.as_str(), "admin" | "maintainer" | "member") {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow user {} has unknown role {}",
                idx + 1,
                user.role
            )));
        }
    }
    for (idx, agent) in team_flow.agents.iter().enumerate() {
        if agent.id.trim().is_empty() || agent.owner_user_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow agent {} must include id and owner_user_id",
                idx + 1
            )));
        }
    }
    for (idx, project) in team_flow.projects.iter().enumerate() {
        if project.id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow project {} has empty id",
                idx + 1
            )));
        }
        if project
            .members
            .iter()
            .any(|member| member.trim().is_empty())
        {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow project {} has empty member",
                idx + 1
            )));
        }
    }
    for (idx, grant) in team_flow.grants.iter().enumerate() {
        if grant.project_id.trim().is_empty() || grant.user_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow grant {} must include project_id and user_id",
                idx + 1
            )));
        }
    }
    for (idx, memory) in team_flow.memories.iter().enumerate() {
        validate_team_actor(&memory.actor, scenario_id, "memory", idx + 1)?;
        if memory.text.trim().is_empty() || memory.scope.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow memory {} must include text and scope",
                idx + 1
            )));
        }
    }
    for (idx, promotion) in team_flow.promotions.iter().enumerate() {
        validate_team_actor(&promotion.actor, scenario_id, "promotion", idx + 1)?;
        if promotion.source_memory_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow promotion {} has empty source_memory_id",
                idx + 1
            )));
        }
    }
    for (idx, review) in team_flow.reviews.iter().enumerate() {
        validate_team_actor(&review.actor, scenario_id, "review", idx + 1)?;
        if review.promotion_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow review {} has empty promotion_id",
                idx + 1
            )));
        }
    }
    for (idx, revocation) in team_flow.revoke_users.iter().enumerate() {
        validate_team_actor(&revocation.actor, scenario_id, "revoke_user", idx + 1)?;
        if revocation.target_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow revoke_user {} has empty target_id",
                idx + 1
            )));
        }
    }
    for (idx, revocation) in team_flow.revoke_agents.iter().enumerate() {
        validate_team_actor(&revocation.actor, scenario_id, "revoke_agent", idx + 1)?;
        if revocation.target_id.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow revoke_agent {} has empty target_id",
                idx + 1
            )));
        }
    }
    for (idx, context) in team_flow.contexts.iter().enumerate() {
        validate_team_actor(&context.actor, scenario_id, "context", idx + 1)?;
        if context.query.trim().is_empty() {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow context {} has empty query",
                idx + 1
            )));
        }
        if context.max_items == Some(0) {
            return Err(EvalError::scenario(format!(
                "scenario {scenario_id} team_flow context {} has zero max_items",
                idx + 1
            )));
        }
    }
    Ok(())
}

fn validate_team_actor(
    actor: &TeamFlowActor,
    scenario_id: &str,
    label: &str,
    idx: usize,
) -> Result<(), EvalError> {
    if actor.user_id.trim().is_empty() {
        return Err(EvalError::scenario(format!(
            "scenario {scenario_id} team_flow {label} {idx} has empty actor user_id"
        )));
    }
    if actor
        .agent_id
        .as_ref()
        .is_some_and(|agent_id| agent_id.trim().is_empty())
    {
        return Err(EvalError::scenario(format!(
            "scenario {scenario_id} team_flow {label} {idx} has empty actor agent_id"
        )));
    }
    Ok(())
}

fn validate_team_expected(team: &TeamExpected, scenario_id: &str) -> Result<(), EvalError> {
    if team
        .context_must_include
        .iter()
        .chain(team.context_must_not_include.iter())
        .chain(team.omitted_reason_contains.iter())
        .chain(team.audit_kinds.iter())
        .any(|value| value.trim().is_empty())
    {
        return Err(EvalError::scenario(format!(
            "scenario {scenario_id} expected team entries must not be empty"
        )));
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
