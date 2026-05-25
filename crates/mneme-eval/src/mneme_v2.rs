use mneme_core::{
    validate_team_state, TeamActor, TeamAgentInput, TeamContextPack, TeamContextQuery,
    TeamMemoryConfig, TeamMemoryEngine, TeamMemoryStatus, TeamProjectInput,
    TeamPromotionCreateInput, TeamPromotionReviewInput, TeamPromotionStatus, TeamRole,
    TeamSyncExportInput, TeamUserInput, DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
};

use crate::error::EvalError;
use crate::scenario::{Scenario, TeamFlowActor};
use crate::target::{
    ActualState, AuditEvent, BudgetActual, Claim, EvalTarget, EvalTargetMetadata, FaultMode,
    RecordedEvent, TargetRunOptions, TeamActual, TeamContextActual, TeamContextItemActual,
    TeamOmittedItemActual,
};

pub(crate) struct MnemeV2EvalTarget;

impl EvalTarget for MnemeV2EvalTarget {
    fn name(&self) -> &'static str {
        "mneme-v2"
    }

    fn metadata(&self, _options: &TargetRunOptions) -> EvalTargetMetadata {
        EvalTargetMetadata::rule_based()
    }

    fn run(
        &self,
        scenario: &Scenario,
        options: TargetRunOptions,
    ) -> Result<ActualState, EvalError> {
        run_team_flow(scenario, options)
    }
}

fn run_team_flow(scenario: &Scenario, options: TargetRunOptions) -> Result<ActualState, EvalError> {
    let Some(flow) = &scenario.team_flow else {
        return Err(EvalError::scenario(format!(
            "scenario {} requires team_flow for mneme-v2 target",
            scenario.id
        )));
    };
    let mut engine = TeamMemoryEngine::new(TeamMemoryConfig {
        workspace_id: flow
            .workspace_id
            .clone()
            .unwrap_or_else(|| "team".to_owned()),
    });
    let mut denied_count = 0usize;
    let mut last_context_actor = None;
    let mut last_context_query = None;
    let mut last_context_pack = None;

    for user in &flow.users {
        engine.upsert_user(TeamUserInput {
            user_id: user.id.clone(),
            role: parse_team_role(&user.role, &scenario.id)?,
        });
    }
    for agent in &flow.agents {
        engine
            .upsert_agent(TeamAgentInput {
                agent_id: agent.id.clone(),
                owner_user_id: agent.owner_user_id.clone(),
            })
            .map_err(|source| {
                EvalError::scenario(format!(
                    "scenario {} failed to upsert team agent {}: {source}",
                    scenario.id, agent.id
                ))
            })?;
    }
    for project in &flow.projects {
        engine
            .upsert_project(TeamProjectInput {
                project_id: project.id.clone(),
                member_user_ids: project.members.clone(),
            })
            .map_err(|source| {
                EvalError::scenario(format!(
                    "scenario {} failed to upsert team project {}: {source}",
                    scenario.id, project.id
                ))
            })?;
    }
    for grant in &flow.grants {
        engine
            .grant_project_member(&grant.project_id, &grant.user_id)
            .map_err(|source| {
                EvalError::scenario(format!(
                    "scenario {} failed to grant team project {} to {}: {source}",
                    scenario.id, grant.project_id, grant.user_id
                ))
            })?;
    }
    for memory in &flow.memories {
        match engine.remember(mneme_core::TeamRememberInput {
            actor: team_actor(&memory.actor),
            text: memory.text.clone(),
            scope: memory.scope.clone(),
        }) {
            Ok(_) => {}
            Err(_) => denied_count += 1,
        }
    }
    for promotion in &flow.promotions {
        match engine.create_promotion(TeamPromotionCreateInput {
            actor: team_actor(&promotion.actor),
            source_memory_id: promotion.source_memory_id.clone(),
            note: promotion.note.clone(),
        }) {
            Ok(_) => {}
            Err(_) => denied_count += 1,
        }
    }
    for review in &flow.reviews {
        match engine.review_promotion(TeamPromotionReviewInput {
            actor: team_actor(&review.actor),
            promotion_id: review.promotion_id.clone(),
            approve: review.approve,
        }) {
            Ok(_) => {}
            Err(_) => denied_count += 1,
        }
    }
    for revocation in &flow.revoke_users {
        match engine.revoke_user(team_actor(&revocation.actor), &revocation.target_id) {
            Ok(_) => {}
            Err(_) => denied_count += 1,
        }
    }
    for revocation in &flow.revoke_agents {
        match engine.revoke_agent(team_actor(&revocation.actor), &revocation.target_id) {
            Ok(_) => {}
            Err(_) => denied_count += 1,
        }
    }
    for context in &flow.contexts {
        let actor = team_actor(&context.actor);
        last_context_actor = Some(actor.clone());
        last_context_query = Some(context.query.clone());
        last_context_pack = Some(engine.build_context_pack(TeamContextQuery {
            actor,
            query: context.query.clone(),
            max_items: context.max_items.unwrap_or(DEFAULT_TEAM_CONTEXT_MAX_ITEMS),
        }));
    }

    let mut sync_memory_count = 0usize;
    let mut sync_omitted_count = 0usize;
    let mut handoff_context_item_count = 0usize;
    if let (Some(actor), Some(query)) = (&last_context_actor, &last_context_query) {
        if let Ok(envelope) = engine.export_sync_envelope(TeamSyncExportInput {
            actor: actor.clone(),
            include_project_scopes: true,
        }) {
            sync_memory_count = envelope.memories.len();
            sync_omitted_count = envelope.omitted.len();
        }
        if let Ok(handoff) = engine.build_handoff_package(TeamContextQuery {
            actor: actor.clone(),
            query: query.clone(),
            max_items: DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
        }) {
            handoff_context_item_count = handoff.context_pack.items.len();
        }
    }
    let firewall = engine.firewall_report();
    let ontology = engine.ontology_report();

    let state = engine.state();
    let validation = validate_team_state(&state);
    let active_memory_count = state
        .memories
        .iter()
        .filter(|memory| memory.status == TeamMemoryStatus::Active)
        .count();
    let blocked_secret_count = state
        .memories
        .iter()
        .filter(|memory| memory.status == TeamMemoryStatus::BlockedSecret)
        .count();
    let quarantined_count = state
        .memories
        .iter()
        .filter(|memory| memory.status == TeamMemoryStatus::Quarantined)
        .count();
    let pending_promotion_count = state
        .promotions
        .iter()
        .filter(|promotion| promotion.status == TeamPromotionStatus::Pending)
        .count();
    let approved_promotion_count = state
        .promotions
        .iter()
        .filter(|promotion| promotion.status == TeamPromotionStatus::Approved)
        .count();
    let rejected_promotion_count = state
        .promotions
        .iter()
        .filter(|promotion| promotion.status == TeamPromotionStatus::Rejected)
        .count();
    denied_count += state
        .audit
        .iter()
        .filter(|audit| audit.kind.as_str() == "team.policy.deny" && !audit.allowed)
        .count();

    let mut team_context = last_context_pack.map(team_context_actual);
    let mut team_actual = TeamActual {
        validation_ok: validation.ok,
        memory_count: state.memories.len(),
        active_memory_count,
        blocked_secret_count,
        quarantined_count,
        promotion_count: state.promotions.len(),
        pending_promotion_count,
        approved_promotion_count,
        rejected_promotion_count,
        denied_count,
        scope_leak_count: 0,
        secret_leak_count: 0,
        sync_memory_count,
        sync_omitted_count,
        handoff_context_item_count,
        firewall_ok: firewall.ok,
        firewall_high_count: firewall.high_count,
        ontology_entity_count: ontology.entity_count,
        ontology_relation_count: ontology.relation_count,
        ontology_attribute_count: ontology.attribute_count,
        context_pack: team_context.take(),
    };
    apply_seeded_fault(
        &mut team_actual,
        &state,
        last_context_actor.as_ref(),
        options.fault_mode,
    );
    team_actual.scope_leak_count =
        count_scope_leaks(&team_actual, &state, last_context_actor.as_ref());
    team_actual.secret_leak_count = count_secret_leaks(&team_actual);

    Ok(ActualState {
        events: state
            .events
            .iter()
            .map(|event| RecordedEvent {
                id: event.id.clone(),
                speaker_id: event.actor_user_id.clone(),
                actor_agent_id: event.actor_agent_id.clone(),
                text: event.text.clone(),
                scope: event.scope.clone(),
                trust_level: "team_policy".to_owned(),
            })
            .collect(),
        claims: state
            .memories
            .iter()
            .map(|memory| Claim {
                id: memory.id.clone(),
                subject: memory.id.clone(),
                predicate: "stores".to_owned(),
                object: memory.text.clone(),
                status: memory.status.as_str().to_owned(),
                scope: memory.scope.clone(),
                source_event_ids: memory.source_event_ids.clone(),
            })
            .collect(),
        sessions: Vec::new(),
        context_pack: None,
        budget: BudgetActual::default(),
        audit: state
            .audit
            .iter()
            .map(|event| AuditEvent {
                kind: event.kind.as_str().to_owned(),
                target_id: event.target_id.clone(),
            })
            .collect(),
        store: None,
        quality: None,
        curation: None,
        team: Some(team_actual),
    })
}

fn apply_seeded_fault(
    actual: &mut TeamActual,
    state: &mneme_core::TeamMemoryState,
    context_actor: Option<&TeamActor>,
    fault_mode: FaultMode,
) {
    match fault_mode {
        FaultMode::None
        | FaultMode::SkipClaims
        | FaultMode::BypassAcl
        | FaultMode::LeakSecrets
        | FaultMode::DropCitations
        | FaultMode::UnapprovedPromotion
        | FaultMode::IgnoreRevocation
        | FaultMode::LeakQuarantined => {}
    }
    match fault_mode {
        FaultMode::None | FaultMode::SkipClaims => {}
        FaultMode::BypassAcl => {
            let Some(actor) = context_actor else {
                return;
            };
            if let Some(memory) = state.memories.iter().find(|memory| {
                memory.status == TeamMemoryStatus::Active
                    && !scope_allowed_for_actor(state, actor, &memory.scope)
            }) {
                push_fault_context_item(actual, memory);
            }
        }
        FaultMode::LeakSecrets => {
            if let Some(memory) = state
                .memories
                .iter()
                .find(|memory| memory.status == TeamMemoryStatus::BlockedSecret)
            {
                push_fault_context_item(actual, memory);
            }
        }
        FaultMode::DropCitations => {
            if let Some(context) = &mut actual.context_pack {
                for item in &mut context.items {
                    item.source_event_ids.clear();
                    item.source_memory_ids.clear();
                }
            }
        }
        FaultMode::UnapprovedPromotion => {
            if let Some(promotion) = state
                .promotions
                .iter()
                .find(|promotion| promotion.status == TeamPromotionStatus::Pending)
            {
                actual.pending_promotion_count = actual.pending_promotion_count.saturating_sub(1);
                actual.approved_promotion_count = actual.approved_promotion_count.saturating_add(1);
                if let Some(memory) = state
                    .memories
                    .iter()
                    .find(|memory| memory.id == promotion.source_memory_id)
                {
                    push_fault_context_item(actual, memory);
                }
            }
        }
        FaultMode::IgnoreRevocation => {
            actual.denied_count = 0;
            if let Some(memory) = state
                .memories
                .iter()
                .find(|memory| memory.status == TeamMemoryStatus::Active)
            {
                push_fault_context_item(actual, memory);
            }
        }
        FaultMode::LeakQuarantined => {
            if let Some(memory) = state
                .memories
                .iter()
                .find(|memory| memory.status == TeamMemoryStatus::Quarantined)
            {
                push_fault_context_item(actual, memory);
            }
        }
    }
}

fn push_fault_context_item(actual: &mut TeamActual, memory: &mneme_core::TeamMemoryRecord) {
    let context = actual
        .context_pack
        .get_or_insert_with(TeamContextActual::default);
    if context.items.iter().any(|item| item.memory_id == memory.id) {
        return;
    }
    context.items.push(TeamContextItemActual {
        memory_id: memory.id.clone(),
        memory_text: memory.text.clone(),
        scope: memory.scope.clone(),
        source_event_ids: memory.source_event_ids.clone(),
        source_memory_ids: memory.source_memory_ids.clone(),
        score: 1,
    });
}

fn team_context_actual(pack: TeamContextPack) -> TeamContextActual {
    TeamContextActual {
        items: pack
            .items
            .into_iter()
            .map(|item| TeamContextItemActual {
                memory_id: item.memory_id,
                memory_text: item.memory_text,
                scope: item.scope,
                source_event_ids: item.source_event_ids,
                source_memory_ids: item.source_memory_ids,
                score: item.score,
            })
            .collect(),
        omitted: pack
            .omitted
            .into_iter()
            .map(|item| TeamOmittedItemActual {
                memory_id: item.memory_id,
                memory_text: item.memory_text,
                reason: item.reason,
            })
            .collect(),
    }
}

fn team_actor(actor: &TeamFlowActor) -> TeamActor {
    TeamActor {
        user_id: actor.user_id.clone(),
        agent_id: actor.agent_id.clone(),
    }
}

fn parse_team_role(value: &str, scenario_id: &str) -> Result<TeamRole, EvalError> {
    value.parse::<TeamRole>().map_err(|source| {
        EvalError::scenario(format!(
            "scenario {scenario_id} has invalid team role {value}: {source}"
        ))
    })
}

fn count_scope_leaks(
    actual: &TeamActual,
    state: &mneme_core::TeamMemoryState,
    actor: Option<&TeamActor>,
) -> usize {
    let Some(actor) = actor else {
        return 0;
    };
    actual
        .context_pack
        .as_ref()
        .map(|context| {
            context
                .items
                .iter()
                .filter(|item| !scope_allowed_for_actor(state, actor, &item.scope))
                .count()
        })
        .unwrap_or_default()
}

fn count_secret_leaks(actual: &TeamActual) -> usize {
    actual
        .context_pack
        .as_ref()
        .map(|context| {
            context
                .items
                .iter()
                .filter(|item| looks_like_secret(&item.memory_text))
                .count()
        })
        .unwrap_or_default()
}

fn scope_allowed_for_actor(
    state: &mneme_core::TeamMemoryState,
    actor: &TeamActor,
    scope: &str,
) -> bool {
    let Some(user) = state.users.iter().find(|user| user.id == actor.user_id) else {
        return false;
    };
    if !user.active {
        return false;
    }
    if let Some(agent_id) = &actor.agent_id {
        let Some(agent) = state.agents.iter().find(|agent| &agent.id == agent_id) else {
            return false;
        };
        if !agent.active || agent.owner_user_id != actor.user_id {
            return false;
        }
    }
    if scope == "team" {
        return true;
    }
    if let Some(user_id) = scope.strip_prefix("private:") {
        return user_id == actor.user_id;
    }
    if let Some(project_id) = scope.strip_prefix("project:") {
        return state.projects.iter().any(|project| {
            project.active
                && project.id == project_id
                && project
                    .member_user_ids
                    .iter()
                    .any(|member| member == &actor.user_id)
        });
    }
    if let Some(agent_id) = scope.strip_prefix("agent-private:") {
        return actor.agent_id.as_deref() == Some(agent_id);
    }
    false
}

fn looks_like_secret(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.contains("api_key=")
        || text.contains("secret=")
        || text.contains("token=")
        || text.contains("password=")
}
