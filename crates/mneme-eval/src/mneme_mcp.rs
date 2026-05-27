use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use mneme_core::{
    ContextPack as CoreContextPack, JsonFileStore, MnemeConfig, MnemeEngine, StoreFileStatus,
    TeamActor, TeamContextPack, TeamMemoryState, TeamMemoryStatus, TeamPromotionStatus,
    TeamRunStatus, TeamSyncEnvelope, DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
};
use mneme_mcp::{McpServer, McpServerConfig, ServerMode};
use serde_json::{json, Value};

use crate::error::EvalError;
use crate::scenario::{Scenario, TeamFlowActor};
use crate::target::{
    build_quality_actual, ActualState, AuditEvent, BudgetActual, Claim, ContextItem, ContextPack,
    EvalTarget, EvalTargetMetadata, FaultMode, OmittedItem, RecordedEvent, SessionActual,
    StoreActual, TargetRunOptions, TeamActual, TeamContextActual, TeamContextItemActual,
    TeamOmittedItemActual,
};

pub(crate) struct MnemeMcpEvalTarget;

impl EvalTarget for MnemeMcpEvalTarget {
    fn name(&self) -> &'static str {
        "mneme-mcp"
    }

    fn metadata(&self, _options: &TargetRunOptions) -> EvalTargetMetadata {
        EvalTargetMetadata {
            extractor: "mcp-json-rpc".to_owned(),
            protocol: Some("mcp:2024-11-05".to_owned()),
            opt_in: false,
            command_configured: true,
        }
    }

    fn run(
        &self,
        scenario: &Scenario,
        options: TargetRunOptions,
    ) -> Result<ActualState, EvalError> {
        let v1_store = temp_store_path(&format!("{}-mcp-v1", scenario.id));
        let team_store = temp_store_path(&format!("{}-mcp-team", scenario.id));
        cleanup_store(&v1_store);
        cleanup_store(&team_store);
        let config = McpServerConfig {
            mode: ServerMode::All,
            v1_store: v1_store.clone(),
            team_store: team_store.clone(),
            team_workspace_id: scenario
                .team_flow
                .as_ref()
                .and_then(|flow| flow.workspace_id.clone())
                .unwrap_or_else(|| "team".to_owned()),
        };
        let result = if scenario.team_flow.is_some() {
            run_team_mcp_flow(scenario, config, options.fault_mode)
        } else {
            run_personal_mcp_flow(scenario, config, options.fault_mode)
        };
        cleanup_store(&v1_store);
        cleanup_store(&team_store);
        result
    }
}

fn run_personal_mcp_flow(
    scenario: &Scenario,
    config: McpServerConfig,
    fault_mode: FaultMode,
) -> Result<ActualState, EvalError> {
    let mut server = McpServer::new(config.clone());
    assert_protocol_ready(&server, scenario)?;

    for (idx, event) in scenario.events.iter().enumerate() {
        call_tool(
            &server,
            "mneme_v1_ingest",
            json!({
                "text": event.text,
                "speaker": event.speaker_id,
                "agent": event.actor_agent_id,
                "scope": event.scope,
                "trust": event.trust_level,
            }),
            scenario,
        )?;
        if scenario
            .persistence
            .as_ref()
            .is_some_and(|persistence| persistence.restart_after_event == idx + 1)
        {
            server = McpServer::new(config.clone());
            call_tool(&server, "mneme_v1_validate", json!({}), scenario)?;
        }
    }

    if let Some(agent_flow) = &scenario.agent_flow {
        let begin = call_tool(
            &server,
            "mneme_v1_begin",
            json!({
                "task": agent_flow.begin.task,
                "agent": agent_flow.begin.actor_agent_id,
                "query": agent_flow.begin.query,
                "scopes": effective_allowed_scopes(&agent_flow.begin.allowed_scopes),
            }),
            scenario,
        )?;
        let session_id = begin
            .get("session_id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EvalError::scenario(format!(
                    "scenario {} MCP begin returned no session_id",
                    scenario.id
                ))
            })?;
        if let Some(end) = &agent_flow.end {
            call_tool(
                &server,
                "mneme_v1_end",
                json!({
                    "session_id": session_id,
                    "agent": agent_flow.begin.actor_agent_id,
                    "summary": end.summary,
                    "remember": end.remember,
                }),
                scenario,
            )?;
        }
    }

    let context_pack = if let Some(expected) = &scenario.expected.context_pack {
        let value = call_tool(
            &server,
            "mneme_v1_context",
            json!({
                "query": expected.query,
                "scopes": effective_allowed_scopes(&expected.allowed_scopes),
                "max_items": expected.max_items.unwrap_or(mneme_core::DEFAULT_CONTEXT_MAX_ITEMS),
            }),
            scenario,
        )?;
        let pack = value
            .get("context_pack")
            .cloned()
            .ok_or_else(|| {
                EvalError::scenario(format!(
                    "scenario {} MCP context returned no context_pack",
                    scenario.id
                ))
            })
            .and_then(|value| {
                serde_json::from_value::<CoreContextPack>(value).map_err(|source| {
                    EvalError::scenario(format!(
                        "scenario {} failed to parse MCP v1 context_pack: {source}",
                        scenario.id
                    ))
                })
            })?;
        Some(context_actual(pack))
    } else {
        None
    };

    let store = JsonFileStore::new(config.v1_store.clone());
    let engine = MnemeEngine::from_store(MnemeConfig::default(), &store).map_err(|source| {
        EvalError::scenario(format!(
            "scenario {} failed to load MCP v1 store: {source}",
            scenario.id
        ))
    })?;
    let snapshot = engine.snapshot();
    let claims = snapshot
        .claims
        .iter()
        .map(|claim| Claim {
            id: claim.id.clone(),
            subject: claim.subject.clone(),
            predicate: claim.predicate.clone(),
            object: claim.object.clone(),
            status: claim.status.as_str().to_owned(),
            scope: claim.scope.clone(),
            source_event_ids: claim.source_event_ids.clone(),
        })
        .collect::<Vec<_>>();
    let mut actual = ActualState {
        events: snapshot
            .events
            .into_iter()
            .map(|event| RecordedEvent {
                id: event.id,
                speaker_id: event.speaker_id,
                actor_agent_id: event.actor_agent_id,
                text: event.text,
                scope: event.scope,
                trust_level: event.trust_level,
            })
            .collect(),
        claims,
        sessions: snapshot
            .sessions
            .into_iter()
            .map(|session| SessionActual {
                id: session.id,
                task: session.task,
                actor_agent_id: session.actor_agent_id,
                status: session.status.as_str().to_owned(),
                context_claim_ids: session.context_claim_ids,
                summary: session.summary,
                memory_event_ids: session.memory_event_ids,
            })
            .collect(),
        context_pack,
        budget: BudgetActual {
            spent_tokens: snapshot.budget.spent_tokens,
            hard_cap_violations: snapshot.budget.hard_cap_violations,
        },
        audit: snapshot
            .audit
            .into_iter()
            .map(|event| AuditEvent {
                kind: event.kind.as_str().to_owned(),
                target_id: event.target_id,
            })
            .collect(),
        store: Some(store_actual(&config.v1_store)),
        quality: None,
        curation: None,
        team: None,
    };
    apply_personal_seeded_fault(&mut actual, fault_mode);
    actual.quality = scenario
        .expected
        .quality
        .as_ref()
        .map(|_| build_quality_actual(&actual.claims));
    Ok(actual)
}

fn run_team_mcp_flow(
    scenario: &Scenario,
    config: McpServerConfig,
    fault_mode: FaultMode,
) -> Result<ActualState, EvalError> {
    let flow = scenario.team_flow.as_ref().ok_or_else(|| {
        EvalError::scenario(format!(
            "scenario {} requires team_flow for mneme-mcp team run",
            scenario.id
        ))
    })?;
    let server = McpServer::new(config.clone());
    assert_protocol_ready(&server, scenario)?;
    let mut denied_count = 0usize;
    let mut last_context_actor = None;
    let mut last_context_query = None;
    let mut last_context_pack = None;
    let mut serialized_surface_parts = Vec::new();

    push_surface(
        &mut serialized_surface_parts,
        "team_init",
        &call_tool(
            &server,
            "mneme_v2_team_init",
            json!({"workspace": flow.workspace_id.clone().unwrap_or_else(|| "team".to_owned())}),
            scenario,
        )?,
    );
    for user in &flow.users {
        call_or_deny(
            &server,
            "mneme_v2_user_add",
            json!({"user": user.id, "role": user.role}),
            scenario,
            &mut denied_count,
        )?;
    }
    for agent in &flow.agents {
        call_or_deny(
            &server,
            "mneme_v2_agent_add",
            json!({"agent": agent.id, "owner": agent.owner_user_id}),
            scenario,
            &mut denied_count,
        )?;
    }
    for project in &flow.projects {
        call_or_deny(
            &server,
            "mneme_v2_project_add",
            json!({"project": project.id, "members": project.members}),
            scenario,
            &mut denied_count,
        )?;
    }
    for grant in &flow.grants {
        call_or_deny(
            &server,
            "mneme_v2_project_grant",
            json!({"project": grant.project_id, "user": grant.user_id}),
            scenario,
            &mut denied_count,
        )?;
    }
    for memory in &flow.memories {
        call_or_deny(
            &server,
            "mneme_v2_team_remember",
            json!({
                "actor": memory.actor.user_id,
                "agent": memory.actor.agent_id,
                "text": memory.text,
                "scope": memory.scope,
            }),
            scenario,
            &mut denied_count,
        )?;
    }
    for promotion in &flow.promotions {
        call_or_deny(
            &server,
            "mneme_v2_promote",
            json!({
                "actor": promotion.actor.user_id,
                "agent": promotion.actor.agent_id,
                "memory_id": promotion.source_memory_id,
                "note": promotion.note,
            }),
            scenario,
            &mut denied_count,
        )?;
    }
    for review in &flow.reviews {
        call_or_deny(
            &server,
            "mneme_v2_review",
            json!({
                "actor": review.actor.user_id,
                "agent": review.actor.agent_id,
                "promotion_id": review.promotion_id,
                "approve": review.approve,
            }),
            scenario,
            &mut denied_count,
        )?;
    }
    for revocation in &flow.revoke_users {
        call_or_deny(
            &server,
            "mneme_v2_revoke_user",
            json!({
                "actor": revocation.actor.user_id,
                "agent": revocation.actor.agent_id,
                "user": revocation.target_id,
            }),
            scenario,
            &mut denied_count,
        )?;
    }
    for revocation in &flow.revoke_agents {
        call_or_deny(
            &server,
            "mneme_v2_revoke_agent",
            json!({
                "actor": revocation.actor.user_id,
                "agent": revocation.actor.agent_id,
                "target_agent": revocation.target_id,
            }),
            scenario,
            &mut denied_count,
        )?;
    }
    for run in &flow.runs {
        let begin = match call_tool(
            &server,
            "mneme_v2_run_begin",
            json!({
                "actor": run.actor.user_id,
                "agent": run.actor.agent_id,
                "task": run.task,
                "query": run.query,
                "scope": run.scope,
                "max_items": DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
            }),
            scenario,
        ) {
            Ok(value) => value,
            Err(_) => {
                denied_count += 1;
                continue;
            }
        };
        push_surface(&mut serialized_surface_parts, "run_begin", &begin);
        last_context_actor = Some(team_actor(&run.actor));
        let run_id = begin
            .get("run_id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                EvalError::scenario(format!(
                    "scenario {} MCP run begin returned no run_id",
                    scenario.id
                ))
            })?;
        if let Some(report) = begin.get("report") {
            if let Some(query) = report
                .get("run")
                .and_then(|run| run.get("context_query"))
                .and_then(Value::as_str)
            {
                last_context_query = Some(query.to_owned());
            }
            if let Some(pack) = report.get("context_pack").cloned() {
                last_context_pack = Some(parse_team_context_pack(pack, scenario)?);
            }
        }
        for note in &run.notes {
            match call_tool(
                &server,
                "mneme_v2_run_note",
                json!({
                    "actor": run.actor.user_id,
                    "agent": run.actor.agent_id,
                    "run_id": run_id,
                    "text": note.text,
                    "scope": note.scope,
                }),
                scenario,
            ) {
                Ok(value) => push_surface(&mut serialized_surface_parts, "run_note", &value),
                Err(_) => denied_count += 1,
            }
        }
        if let Some(end) = &run.end {
            match call_tool(
                &server,
                "mneme_v2_run_end",
                json!({
                    "actor": run.actor.user_id,
                    "agent": run.actor.agent_id,
                    "run_id": run_id,
                    "summary": end.summary,
                    "next": end.next,
                    "remember": end.remember,
                    "scope": end.scope,
                }),
                scenario,
            ) {
                Ok(value) => push_surface(&mut serialized_surface_parts, "run_end", &value),
                Err(_) => denied_count += 1,
            }
        }
        if run.handoff {
            match call_tool(
                &server,
                "mneme_v2_run_handoff",
                json!({
                    "actor": run.actor.user_id,
                    "agent": run.actor.agent_id,
                    "run_id": run_id,
                    "query": run.query,
                    "max_items": DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
                }),
                scenario,
            ) {
                Ok(value) => push_surface(&mut serialized_surface_parts, "run_handoff", &value),
                Err(_) => denied_count += 1,
            }
        }
    }
    for context in &flow.contexts {
        let value = call_tool(
            &server,
            "mneme_v2_team_context",
            json!({
                "actor": context.actor.user_id,
                "agent": context.actor.agent_id,
                "query": context.query,
                "max_items": context.max_items.unwrap_or(DEFAULT_TEAM_CONTEXT_MAX_ITEMS),
            }),
            scenario,
        )?;
        last_context_actor = Some(team_actor(&context.actor));
        last_context_query = Some(context.query.clone());
        push_surface(&mut serialized_surface_parts, "context_pack", &value);
        let pack = value.get("context_pack").cloned().ok_or_else(|| {
            EvalError::scenario(format!(
                "scenario {} MCP team context returned no context_pack",
                scenario.id
            ))
        })?;
        last_context_pack = Some(parse_team_context_pack(pack, scenario)?);
    }

    let mut sync_memory_count = 0usize;
    let mut sync_omitted_count = 0usize;
    let mut sync_checksum_verified = false;
    let mut handoff_context_item_count = 0usize;
    if let (Some(actor), Some(query)) = (&last_context_actor, &last_context_query) {
        if let Ok(value) = call_tool(
            &server,
            "mneme_v2_sync_export",
            json!({"actor": actor.user_id, "agent": actor.agent_id, "include_projects": true}),
            scenario,
        ) {
            sync_memory_count = value
                .get("memory_count")
                .and_then(Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or_default();
            sync_omitted_count = value
                .get("omitted_count")
                .and_then(Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or_default();
            push_surface(&mut serialized_surface_parts, "sync_envelope", &value);
            if let Some(envelope) = value.get("envelope") {
                if let Ok(import) = call_tool(
                    &server,
                    "mneme_v2_sync_import",
                    json!({"envelope": envelope, "apply": false}),
                    scenario,
                ) {
                    sync_checksum_verified = import
                        .get("report")
                        .and_then(|report| report.get("checksum_verified"))
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    push_surface(&mut serialized_surface_parts, "sync_dry_run", &import);
                }
                let _ = serde_json::from_value::<TeamSyncEnvelope>(envelope.clone());
            }
        }
        if let Ok(value) = call_tool(
            &server,
            "mneme_v2_team_handoff",
            json!({
                "actor": actor.user_id,
                "agent": actor.agent_id,
                "query": query,
                "max_items": DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
            }),
            scenario,
        ) {
            handoff_context_item_count = value
                .get("context_item_count")
                .and_then(Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or_default();
            push_surface(&mut serialized_surface_parts, "handoff_package", &value);
        }
    }
    let firewall = call_tool(&server, "mneme_v2_firewall", json!({}), scenario)?;
    let quality = call_tool(&server, "mneme_v2_quality", json!({}), scenario)?;
    let ontology = call_tool(&server, "mneme_v2_ontology", json!({}), scenario)?;
    push_surface(&mut serialized_surface_parts, "firewall", &firewall);
    push_surface(&mut serialized_surface_parts, "quality", &quality);
    push_surface(&mut serialized_surface_parts, "ontology", &ontology);
    let snapshot = call_tool(&server, "mneme_v2_snapshot", json!({}), scenario)?;
    let state = snapshot
        .get("snapshot")
        .cloned()
        .ok_or_else(|| {
            EvalError::scenario(format!(
                "scenario {} MCP team snapshot returned no snapshot",
                scenario.id
            ))
        })
        .and_then(|value| {
            serde_json::from_value::<TeamMemoryState>(value).map_err(|source| {
                EvalError::scenario(format!(
                    "scenario {} failed to parse MCP team state: {source}",
                    scenario.id
                ))
            })
        })?;
    denied_count += state
        .audit
        .iter()
        .filter(|audit| audit.kind.as_str() == "team.policy.deny" && !audit.allowed)
        .count();
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
    let open_run_count = state
        .runs
        .iter()
        .filter(|run| run.status == TeamRunStatus::Open)
        .count();
    let closed_run_count = state
        .runs
        .iter()
        .filter(|run| run.status == TeamRunStatus::Closed)
        .count();
    let validation = mneme_core::validate_team_state(&state);
    let firewall_high_count = firewall
        .get("firewall")
        .and_then(|firewall| firewall.get("high_count"))
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_default();
    let firewall_ok = firewall
        .get("firewall")
        .and_then(|firewall| firewall.get("ok"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let quality_value = quality.get("quality").cloned().unwrap_or_else(|| json!({}));
    let quality_ok = quality_value
        .get("ok")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let quality_duplicate_group_count = value_usize(&quality_value, "duplicate_group_count");
    let quality_conflict_group_count = value_usize(&quality_value, "conflict_group_count");
    let ontology_value = ontology
        .get("ontology")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let mut team_actual = TeamActual {
        validation_ok: validation.ok,
        memory_count: state.memories.len(),
        active_memory_count,
        blocked_secret_count,
        quarantined_count,
        promotion_count: state.promotions.len(),
        run_count: state.runs.len(),
        open_run_count,
        closed_run_count,
        pending_promotion_count,
        approved_promotion_count,
        rejected_promotion_count,
        denied_count,
        scope_leak_count: 0,
        secret_leak_count: 0,
        sync_memory_count,
        sync_omitted_count,
        sync_checksum_verified,
        handoff_context_item_count,
        firewall_ok,
        firewall_high_count,
        ontology_entity_count: value_usize(&ontology_value, "entity_count"),
        ontology_relation_count: value_usize(&ontology_value, "relation_count"),
        ontology_attribute_count: value_usize(&ontology_value, "attribute_count"),
        quality_ok,
        quality_duplicate_group_count,
        quality_conflict_group_count,
        serialized_surface: serialized_surface_parts.join("\n"),
        context_pack: last_context_pack.map(team_context_actual),
    };
    apply_team_seeded_fault(
        &mut team_actual,
        &state,
        last_context_actor.as_ref(),
        fault_mode,
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

fn assert_protocol_ready(server: &McpServer, scenario: &Scenario) -> Result<(), EvalError> {
    let init = server.handle_request(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"}));
    if init
        .get("result")
        .and_then(|result| result.get("serverInfo"))
        .and_then(|info| info.get("name"))
        .and_then(Value::as_str)
        != Some("mneme-mcp")
    {
        return Err(EvalError::scenario(format!(
            "scenario {} MCP initialize failed: {init}",
            scenario.id
        )));
    }
    let tools = server.handle_request(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    let tool_count = tools
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if tool_count == 0 {
        return Err(EvalError::scenario(format!(
            "scenario {} MCP tools/list returned no tools",
            scenario.id
        )));
    }
    Ok(())
}

fn call_or_deny(
    server: &McpServer,
    name: &str,
    arguments: Value,
    scenario: &Scenario,
    denied_count: &mut usize,
) -> Result<(), EvalError> {
    if call_tool(server, name, arguments, scenario).is_err() {
        *denied_count = denied_count.saturating_add(1);
    }
    Ok(())
}

fn call_tool(
    server: &McpServer,
    name: &str,
    arguments: Value,
    scenario: &Scenario,
) -> Result<Value, EvalError> {
    let response = server.handle_request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments,
        }
    }));
    if let Some(error) = response.get("error") {
        return Err(EvalError::scenario(format!(
            "scenario {} MCP tool {name} failed: {error}",
            scenario.id
        )));
    }
    response
        .get("result")
        .and_then(|result| result.get("structuredContent"))
        .cloned()
        .ok_or_else(|| {
            EvalError::scenario(format!(
                "scenario {} MCP tool {name} returned no structuredContent: {response}",
                scenario.id
            ))
        })
}

fn context_actual(pack: CoreContextPack) -> ContextPack {
    ContextPack {
        items: pack
            .items
            .into_iter()
            .map(|item| ContextItem {
                claim_id: item.claim_id,
                claim_text: item.claim_text,
                source_event_ids: item.source_event_ids,
                score: item.score,
                matched_terms: item.matched_terms,
                match_reason: item.match_reason,
            })
            .collect(),
        omitted: pack
            .omitted
            .into_iter()
            .map(|item| OmittedItem {
                claim_id: item.claim_id,
                reason: item.reason,
            })
            .collect(),
    }
}

fn parse_team_context_pack(
    value: Value,
    scenario: &Scenario,
) -> Result<TeamContextPack, EvalError> {
    serde_json::from_value::<TeamContextPack>(value).map_err(|source| {
        EvalError::scenario(format!(
            "scenario {} failed to parse MCP team context pack: {source}",
            scenario.id
        ))
    })
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

fn effective_allowed_scopes(scopes: &[String]) -> Vec<String> {
    if scopes.is_empty() {
        vec!["private".to_owned()]
    } else {
        scopes.to_vec()
    }
}

fn store_actual(path: &Path) -> StoreActual {
    let store = JsonFileStore::new(path.to_path_buf());
    let inspection = store.inspect();
    let validation = inspection.current.validation.as_ref();
    StoreActual {
        schema_version: inspection.current.schema_version,
        valid: inspection.current.status == StoreFileStatus::Valid,
        backup_present: inspection.backup.status != StoreFileStatus::Missing,
        repair_performed: false,
        restored: false,
        compacted: false,
        imported: false,
        generation: inspection.current.generation,
        error_count: validation.map_or(1, |report| report.error_count),
    }
}

fn value_usize(value: &Value, key: &str) -> usize {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
        .unwrap_or_default()
}

fn push_surface(parts: &mut Vec<String>, label: &str, value: &Value) {
    parts.push(format!("{label}:{value}"));
}

fn apply_personal_seeded_fault(actual: &mut ActualState, fault_mode: FaultMode) {
    match fault_mode {
        FaultMode::None => {}
        FaultMode::SkipClaims => {
            actual.claims.clear();
            if let Some(context_pack) = &mut actual.context_pack {
                context_pack.items.clear();
            }
            actual.audit.retain(|event| event.kind != "claim.write");
        }
        FaultMode::LeakSecrets => {
            for claim in &mut actual.claims {
                if claim.status == "blocked_secret" {
                    claim.status = "active".to_owned();
                }
            }
        }
        FaultMode::DropCitations => {
            if let Some(context_pack) = &mut actual.context_pack {
                for item in &mut context_pack.items {
                    item.source_event_ids.clear();
                }
            }
        }
        FaultMode::BypassAcl
        | FaultMode::UnapprovedPromotion
        | FaultMode::IgnoreRevocation
        | FaultMode::LeakQuarantined => {}
    }
}

fn apply_team_seeded_fault(
    actual: &mut TeamActual,
    state: &TeamMemoryState,
    context_actor: Option<&TeamActor>,
    fault_mode: FaultMode,
) {
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

fn count_scope_leaks(
    actual: &TeamActual,
    state: &TeamMemoryState,
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

fn scope_allowed_for_actor(state: &TeamMemoryState, actor: &TeamActor, scope: &str) -> bool {
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
        || text.contains("authorization: bearer")
        || text.contains("sk-")
        || text.contains("ghp_")
}

fn temp_store_path(label: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("mneme-eval-{label}-{now}.json"))
}

fn cleanup_store(path: &Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(backup_path_for(path));
    let _ = std::fs::remove_file(lock_path_for(path));
    let _ = std::fs::remove_file(team_lock_path_for(path));
    let _ = std::fs::remove_file(temp_path_for(path));
}

fn backup_path_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.bak", path.display()))
}

fn lock_path_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.lock", path.display()))
}

fn team_lock_path_for(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return lock_path_for(path);
    };
    path.with_file_name(format!("{file_name}.lock"))
}

fn temp_path_for(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return PathBuf::from(format!("{}.tmp", path.display()));
    };
    path.with_file_name(format!(".{file_name}.tmp"))
}
