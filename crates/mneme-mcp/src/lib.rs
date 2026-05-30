//! MCP JSON-RPC surface for Mneme local memory stores.

use std::env;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use mneme_core::{
    validate_state, validate_team_state, ClaimStatus, ContextQuery, EventInput, JsonFileStore,
    JsonTeamFileStore, MnemeConfig, MnemeEngine, OutcomeJudgmentCriterionResult,
    OutcomeJudgmentReport, OutcomeJudgmentVerdict, SessionBackfillInput, SessionBeginInput,
    SessionEndInput, StoreError, TeamActor, TeamAgentInput, TeamContextQuery, TeamMemoryConfig,
    TeamMemoryEngine, TeamProjectInput, TeamPromotionCreateInput, TeamPromotionReviewInput,
    TeamRememberInput, TeamRole, TeamRunBeginInput, TeamRunEndInput, TeamRunHandoffInput,
    TeamRunNoteInput, TeamSyncEnvelope, TeamSyncExportInput, TeamUserInput,
    DEFAULT_CONTEXT_MAX_ITEMS, DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
};
use serde::Serialize;
use serde_json::{json, Map, Value};

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// MCP tool exposure mode.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum ServerMode {
    /// Expose v1 personal-memory tools.
    Personal,
    /// Expose v2 team-memory tools.
    Team,
    /// Expose both v1 and v2 tools.
    All,
}

impl ServerMode {
    /// Parses a CLI/env mode value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "personal" | "v1" => Some(Self::Personal),
            "team" | "v2" => Some(Self::Team),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    /// Stable string form.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Team => "team",
            Self::All => "all",
        }
    }

    fn includes_personal(self) -> bool {
        matches!(self, Self::Personal | Self::All)
    }

    fn includes_team(self) -> bool {
        matches!(self, Self::Team | Self::All)
    }
}

/// Runtime configuration for one local Mneme MCP server.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Tool exposure mode.
    pub mode: ServerMode,
    /// V1 personal-memory JSON store.
    pub v1_store: PathBuf,
    /// V2 team-memory JSON store.
    pub team_store: PathBuf,
    /// Workspace id used when a missing v2 store must be bootstrapped in memory.
    pub team_workspace_id: String,
}

impl McpServerConfig {
    /// Builds config from the current process environment and working directory.
    pub fn from_env() -> Result<Self, McpError> {
        let mode = env::var("MNEME_MCP_MODE")
            .ok()
            .and_then(|value| ServerMode::parse(&value))
            .unwrap_or(ServerMode::All);
        let v1_store = env::var_os("MNEME_V1_STORE")
            .map(PathBuf::from)
            .or_else(|| env::var_os("MNEME_STORE").map(PathBuf::from))
            .unwrap_or(default_v1_store_path()?);
        let team_store = env::var_os("MNEME_TEAM_STORE")
            .map(PathBuf::from)
            .unwrap_or(default_team_store_path()?);
        let team_workspace_id =
            env::var("MNEME_TEAM_WORKSPACE_ID").unwrap_or_else(|_| "team".to_owned());
        Ok(Self {
            mode,
            v1_store,
            team_store,
            team_workspace_id,
        })
    }
}

/// Local MCP server facade.
#[derive(Debug, Clone)]
pub struct McpServer {
    config: McpServerConfig,
}

impl McpServer {
    /// Creates a new server facade.
    #[must_use]
    pub const fn new(config: McpServerConfig) -> Self {
        Self { config }
    }

    /// Current configuration.
    #[must_use]
    pub const fn config(&self) -> &McpServerConfig {
        &self.config
    }

    /// Handles one parsed JSON-RPC request.
    pub fn handle_request(&self, request: Value) -> Value {
        let request_id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match method {
            "initialize" => result_response(
                request_id,
                json!({
                    "protocolVersion": MCP_PROTOCOL_VERSION,
                    "capabilities": {"tools": {}},
                    "serverInfo": {
                        "name": "mneme-mcp",
                        "version": env!("CARGO_PKG_VERSION"),
                        "mode": self.config.mode.as_str(),
                    }
                }),
            ),
            "notifications/initialized" => result_response(request_id, json!({})),
            "tools/list" => result_response(request_id, json!({ "tools": self.tools() })),
            "tools/call" => {
                let params = request.get("params").and_then(Value::as_object);
                let name = params
                    .and_then(|params| params.get("name"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let arguments = params
                    .and_then(|params| params.get("arguments"))
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                match self.call_tool(name, &arguments) {
                    Ok(value) => result_response(request_id, tool_result(value)),
                    Err(error) => error_response(request_id, error.code, error.to_string()),
                }
            }
            _ => error_response(request_id, -32601, format!("unknown method: {method}")),
        }
    }

    /// Handles one JSON-RPC line and returns one JSON-RPC response line.
    pub fn handle_json_line(&self, line: &str) -> String {
        let response = match serde_json::from_str::<Value>(line) {
            Ok(request) => {
                if is_json_rpc_notification(&request) {
                    return String::new();
                }
                self.handle_request(request)
            }
            Err(source) => error_response(Value::Null, -32700, format!("parse json: {source}")),
        };
        format!("{response}\n")
    }

    /// Tool inventory for the configured mode.
    #[must_use]
    pub fn tools(&self) -> Vec<ToolDefinition> {
        let mut tools = global_tools();
        if self.config.mode.includes_personal() {
            tools.extend(personal_workflow_tools());
            tools.extend(personal_tools());
        }
        if self.config.mode.includes_team() {
            tools.extend(team_tools());
        }
        tools
    }

    /// Calls one named tool.
    pub fn call_tool(&self, name: &str, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        if name.starts_with("mneme_v1_") && !self.config.mode.includes_personal() {
            return Err(McpError::invalid_request(
                "v1 tools are disabled for this server mode",
            ));
        }
        if is_personal_workflow_tool(name) && !self.config.mode.includes_personal() {
            return Err(McpError::invalid_request(
                "personal workflow tools are disabled for this server mode",
            ));
        }
        if name.starts_with("mneme_v2_") && !self.config.mode.includes_team() {
            return Err(McpError::invalid_request(
                "v2 tools are disabled for this server mode",
            ));
        }
        match name {
            "mneme_mcp_status" => Ok(self.mcp_status()),
            "mneme_agent_guide" => Ok(self.agent_guide(arguments)),
            "mneme_task_start" => self.task_start(arguments),
            "mneme_task_finish" => self.task_finish(arguments),
            "mneme_prepare_handoff" => self.prepare_handoff(arguments),
            "mneme_import_previous_context" => self.import_previous_context(arguments),
            "mneme_v1_ingest" => self.v1_ingest(arguments, false, "ingest"),
            "mneme_v1_remember" => self.v1_ingest(arguments, true, "remember"),
            "mneme_v1_context" => self.v1_context(arguments),
            "mneme_v1_begin" => self.v1_begin(arguments),
            "mneme_v1_end" => self.v1_end(arguments),
            "mneme_v1_continuity_begin" => self.v1_continuity_begin(arguments),
            "mneme_v1_continuity_end" => self.v1_continuity_end(arguments),
            "mneme_v1_continuity_handoff" => self.v1_continuity_handoff(arguments),
            "mneme_v1_backfill_context" => self.v1_backfill_context(arguments),
            "mneme_v1_forget" => self.v1_lifecycle(arguments, "forget"),
            "mneme_v1_correct" => self.v1_lifecycle(arguments, "correct"),
            "mneme_v1_quality" => self.v1_quality(),
            "mneme_v1_outcome_status" => self.v1_outcome_status(arguments),
            "mneme_v1_outcome_judge" => self.v1_outcome_judge(arguments),
            "mneme_v1_validate" => self.v1_validate(),
            "mneme_v1_snapshot" => self.v1_snapshot(),
            "mneme_v2_team_init" => self.v2_init(arguments),
            "mneme_v2_user_add" => self.v2_user_add(arguments),
            "mneme_v2_agent_add" => self.v2_agent_add(arguments),
            "mneme_v2_project_add" => self.v2_project_add(arguments),
            "mneme_v2_project_grant" => self.v2_project_grant(arguments),
            "mneme_v2_team_remember" => self.v2_remember(arguments),
            "mneme_v2_team_context" => self.v2_context(arguments),
            "mneme_v2_team_handoff" => self.v2_handoff(arguments),
            "mneme_v2_run_begin" => self.v2_run_begin(arguments),
            "mneme_v2_run_note" => self.v2_run_note(arguments),
            "mneme_v2_run_end" => self.v2_run_end(arguments),
            "mneme_v2_run_handoff" => self.v2_run_handoff(arguments),
            "mneme_v2_promote" => self.v2_promote(arguments),
            "mneme_v2_promotion_report" => self.v2_promotion_report(arguments),
            "mneme_v2_review" => self.v2_review(arguments),
            "mneme_v2_sync_export" => self.v2_sync_export(arguments),
            "mneme_v2_sync_import" => self.v2_sync_import(arguments),
            "mneme_v2_firewall" => self.v2_firewall(),
            "mneme_v2_quality" => self.v2_quality(),
            "mneme_v2_ontology" => self.v2_ontology(arguments),
            "mneme_v2_revoke_user" => self.v2_revoke_user(arguments),
            "mneme_v2_revoke_agent" => self.v2_revoke_agent(arguments),
            "mneme_v2_validate" => self.v2_validate(),
            "mneme_v2_snapshot" => self.v2_snapshot(),
            _ => Err(McpError::method_not_found(format!("unknown tool: {name}"))),
        }
    }

    fn mcp_status(&self) -> Value {
        let tools = self.tools();
        let v1 = if self.config.mode.includes_personal() {
            match self.load_v1_engine() {
                Ok(engine) => {
                    let validation = validate_state(&engine.state());
                    let snapshot = engine.snapshot();
                    json!({
                        "enabled": true,
                        "store": self.config.v1_store.display().to_string(),
                        "exists": self.config.v1_store.exists(),
                        "parent_exists": parent_exists(&self.config.v1_store),
                        "validation": validation,
                        "event_count": snapshot.events.len(),
                        "claim_count": snapshot.claims.len(),
                        "session_count": snapshot.sessions.len(),
                    })
                }
                Err(error) => json!({
                    "enabled": true,
                    "store": self.config.v1_store.display().to_string(),
                    "exists": self.config.v1_store.exists(),
                    "parent_exists": parent_exists(&self.config.v1_store),
                    "error": error.to_string(),
                }),
            }
        } else {
            json!({"enabled": false})
        };
        let v2 = if self.config.mode.includes_team() {
            match self.load_v2_engine() {
                Ok(engine) => {
                    let state = engine.state();
                    json!({
                        "enabled": true,
                        "store": self.config.team_store.display().to_string(),
                        "exists": self.config.team_store.exists(),
                        "parent_exists": parent_exists(&self.config.team_store),
                        "workspace_id": state.workspace_id,
                        "validation": validate_team_state(&state),
                        "user_count": state.users.len(),
                        "agent_count": state.agents.len(),
                        "project_count": state.projects.len(),
                        "memory_count": state.memories.len(),
                        "run_count": state.runs.len(),
                    })
                }
                Err(error) => json!({
                    "enabled": true,
                    "store": self.config.team_store.display().to_string(),
                    "exists": self.config.team_store.exists(),
                    "parent_exists": parent_exists(&self.config.team_store),
                    "error": error.to_string(),
                }),
            }
        } else {
            json!({"enabled": false})
        };
        json!({
            "command": "mcp.status",
            "schema_version": "mneme.mcp_status.v1",
            "server": {
                "name": "mneme-mcp",
                "version": env!("CARGO_PKG_VERSION"),
                "protocol": MCP_PROTOCOL_VERSION,
                "mode": self.config.mode.as_str(),
            },
            "tool_count": tools.len(),
            "tools": tools.iter().map(|tool| tool.name).collect::<Vec<_>>(),
            "recommended_agent_tools": [
                "mneme_agent_guide",
                "mneme_task_start",
                "mneme_task_finish",
                "mneme_prepare_handoff",
                "mneme_import_previous_context"
            ],
            "v1": v1,
            "v2": v2,
            "continuity_contract": {
                "mcp_accessible": true,
                "begin_required": true,
                "end_write_back_required": true,
                "read_and_honor_required": true,
                "shared_scope_or_lineage_required": true,
                "sequential_handoff_required": true,
                "partial_context_warning_required": true,
                "backfill_supported": true,
                "preferred_agent_loop": [
                    "mneme_mcp_status",
                    "mneme_agent_guide",
                    "mneme_task_start",
                    "mneme_task_finish",
                    "mneme_prepare_handoff"
                ],
            }
        })
    }

    fn agent_guide(&self, arguments: &Map<String, Value>) -> Value {
        let situation =
            optional_string(arguments, "situation").unwrap_or_else(|| "task".to_owned());
        json!({
            "command": "mcp.agent_guide",
            "schema_version": "mneme.mcp_agent_guide.v1",
            "mode": self.config.mode.as_str(),
            "situation": situation,
            "recommended_tools_first": [
                "mneme_task_start",
                "mneme_task_finish",
                "mneme_prepare_handoff",
                "mneme_import_previous_context"
            ],
            "advanced_tools": [
                "mneme_v1_context",
                "mneme_v1_continuity_begin",
                "mneme_v1_continuity_end",
                "mneme_v2_run_begin",
                "mneme_v2_run_end",
                "mneme_v2_run_handoff"
            ],
            "rules": [
                "Use the same lineage and scope for a task across sessions.",
                "Treat Mneme context as partial cited memory, not a full transcript.",
                "Call mneme_task_finish before stopping if mneme_task_start returned a session_id.",
                "Store only durable non-secret facts in remember.",
                "Use mneme_prepare_handoff before another agent continues the task."
            ],
            "next_recommended_actions": guide_next_actions(),
        })
    }

    fn task_start(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let task = required_string(arguments, "task")?;
        let query = optional_string(arguments, "query").unwrap_or_else(|| task.clone());
        let mut begin_args = arguments.clone();
        begin_args.insert("query".to_owned(), Value::String(query.clone()));
        let begin = self.v1_continuity_begin(&begin_args)?;
        let include_handoff = optional_bool(arguments, "include_handoff").unwrap_or(true);
        let handoff = if include_handoff {
            let mut handoff_args = arguments.clone();
            handoff_args.insert("query".to_owned(), Value::String(query));
            Some(self.v1_continuity_handoff(&handoff_args)?)
        } else {
            None
        };
        Ok(json!({
            "command": "mcp.task_start",
            "schema_version": "mneme.mcp_task_start.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": begin.get("session_id").cloned().unwrap_or(Value::Null),
            "lineage_id": begin.get("lineage_id").cloned().unwrap_or(Value::Null),
            "continuity_scope": begin.get("continuity_scope").cloned().unwrap_or(Value::Null),
            "partial_context": begin.get("partial_context").cloned().unwrap_or(Value::Bool(true)),
            "not_full_transcript": begin.get("not_full_transcript").cloned().unwrap_or(Value::Bool(true)),
            "warning": begin.get("warning").cloned().unwrap_or(Value::Null),
            "handoff": handoff,
            "report": begin.get("report").cloned().unwrap_or(Value::Null),
            "next_recommended_actions": task_start_next_actions(),
        }))
    }

    fn task_finish(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let finish = self.v1_continuity_end(arguments)?;
        Ok(json!({
            "command": "mcp.task_finish",
            "schema_version": "mneme.mcp_task_finish.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": finish.get("session_id").cloned().unwrap_or(Value::Null),
            "lineage_id": finish.get("lineage_id").cloned().unwrap_or(Value::Null),
            "continuity_scope": finish.get("continuity_scope").cloned().unwrap_or(Value::Null),
            "write_back_ok": finish.get("write_back_ok").cloned().unwrap_or(Value::Bool(false)),
            "remembered_event_count": finish.get("remembered_event_count").cloned().unwrap_or(Value::from(0)),
            "remembered_claim_count": finish.get("remembered_claim_count").cloned().unwrap_or(Value::from(0)),
            "gate_result": finish.pointer("/report/session/gate_result").cloned().unwrap_or(Value::Null),
            "report": finish.get("report").cloned().unwrap_or(Value::Null),
            "next_recommended_actions": task_finish_next_actions(),
        }))
    }

    fn prepare_handoff(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let handoff = self.v1_continuity_handoff(arguments)?;
        Ok(json!({
            "command": "mcp.prepare_handoff",
            "schema_version": "mneme.mcp_prepare_handoff.v1",
            "store": self.config.v1_store.display().to_string(),
            "lineage_id": handoff.get("lineage_id").cloned().unwrap_or(Value::Null),
            "continuity_scope": handoff.get("continuity_scope").cloned().unwrap_or(Value::Null),
            "partial_context": handoff.get("partial_context").cloned().unwrap_or(Value::Bool(true)),
            "not_full_transcript": handoff.get("not_full_transcript").cloned().unwrap_or(Value::Bool(true)),
            "warning": handoff.get("warning").cloned().unwrap_or(Value::Null),
            "source_session_count": handoff.get("source_session_count").cloned().unwrap_or(Value::from(0)),
            "context_item_count": handoff.get("context_item_count").cloned().unwrap_or(Value::from(0)),
            "source_sessions": handoff.get("source_sessions").cloned().unwrap_or(Value::Array(Vec::new())),
            "context_pack": handoff.get("context_pack").cloned().unwrap_or(Value::Null),
            "next_recommended_actions": handoff_next_actions(),
        }))
    }

    fn import_previous_context(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let backfill = self.v1_backfill_context(arguments)?;
        Ok(json!({
            "command": "mcp.import_previous_context",
            "schema_version": "mneme.mcp_import_previous_context.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": backfill.get("session_id").cloned().unwrap_or(Value::Null),
            "lineage_id": backfill.get("lineage_id").cloned().unwrap_or(Value::Null),
            "partial_context": backfill.get("partial_context").cloned().unwrap_or(Value::Bool(true)),
            "not_full_transcript": backfill.get("not_full_transcript").cloned().unwrap_or(Value::Bool(true)),
            "warning": backfill.get("warning").cloned().unwrap_or(Value::Null),
            "remembered_event_count": backfill.get("remembered_event_count").cloned().unwrap_or(Value::from(0)),
            "remembered_claim_count": backfill.get("remembered_claim_count").cloned().unwrap_or(Value::from(0)),
            "report": backfill.get("report").cloned().unwrap_or(Value::Null),
            "next_recommended_actions": import_next_actions(),
        }))
    }

    fn v1_ingest(
        &self,
        arguments: &Map<String, Value>,
        prefix_remember: bool,
        command: &'static str,
    ) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let text = required_string(arguments, "text")?;
        let input = EventInput {
            speaker_id: optional_string(arguments, "speaker").unwrap_or_else(|| "user".to_owned()),
            actor_agent_id: optional_string(arguments, "agent"),
            text: if prefix_remember {
                format!("remember: {text}")
            } else {
                text
            },
            scope: optional_string(arguments, "scope").unwrap_or_else(|| "private".to_owned()),
            trust_level: optional_string(arguments, "trust")
                .unwrap_or_else(|| "trusted_user".to_owned()),
        };
        engine.ingest_event(input).map_err(McpError::tool)?;
        self.persist_v1(&engine)?;
        let snapshot = engine.snapshot();
        Ok(json!({
            "command": format!("v1.{command}"),
            "store": self.config.v1_store.display().to_string(),
            "event_count": snapshot.events.len(),
            "claim_count": snapshot.claims.len(),
            "latest_claim": snapshot.claims.last(),
        }))
    }

    fn v1_context(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let query = required_string(arguments, "query")?;
        let scopes =
            optional_string_vec(arguments, "scopes").unwrap_or_else(|| vec!["private".to_owned()]);
        let max_items = optional_usize(arguments, "max_items").unwrap_or(DEFAULT_CONTEXT_MAX_ITEMS);
        let context_pack = engine.build_context_pack_with(
            ContextQuery::with_allowed_scopes(query.clone(), scopes).with_max_items(max_items),
        );
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.context",
            "store": self.config.v1_store.display().to_string(),
            "query": query,
            "partial_context": context_pack.metadata.partial_context,
            "not_full_transcript": context_pack.metadata.not_full_transcript,
            "warning": context_pack.metadata.warning.clone(),
            "item_count": context_pack.items.len(),
            "omitted_count": context_pack.omitted.len(),
            "context_pack": context_pack,
            "next_recommended_actions": partial_context_next_actions(),
        }))
    }

    fn v1_begin(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let task = required_string(arguments, "task")?;
        let scopes =
            optional_string_vec(arguments, "scopes").unwrap_or_else(|| vec!["private".to_owned()]);
        let report = engine.begin_session(SessionBeginInput {
            task,
            lineage_id: optional_string(arguments, "lineage"),
            actor_agent_id: optional_string(arguments, "agent"),
            query: optional_string(arguments, "query"),
            allowed_scopes: scopes,
            max_items: optional_usize(arguments, "max_items").unwrap_or(DEFAULT_CONTEXT_MAX_ITEMS),
            acceptance: None,
        });
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.begin",
            "store": self.config.v1_store.display().to_string(),
            "session_id": report.session.id,
            "partial_context": report.context_pack.metadata.partial_context,
            "not_full_transcript": report.context_pack.metadata.not_full_transcript,
            "warning": report.context_pack.metadata.warning.clone(),
            "report": report,
            "next_recommended_actions": task_start_next_actions(),
        }))
    }

    fn v1_end(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let report = engine
            .end_session(SessionEndInput {
                session_id: required_string(arguments, "session_id")?,
                actor_agent_id: optional_string(arguments, "agent"),
                scope: optional_string(arguments, "scope"),
                summary: optional_string(arguments, "summary"),
                remember: optional_string_vec(arguments, "remember").unwrap_or_default(),
                verifier_report: None,
            })
            .map_err(McpError::tool)?;
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.end",
            "store": self.config.v1_store.display().to_string(),
            "report": report,
        }))
    }

    fn v1_continuity_begin(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let task = required_string(arguments, "task")?;
        let lineage_id = optional_string(arguments, "lineage");
        let continuity_scope = continuity_scope(arguments);
        let scopes = optional_string_vec(arguments, "scopes")
            .unwrap_or_else(|| continuity_allowed_scopes(&continuity_scope));
        let query = optional_string(arguments, "query").unwrap_or_else(|| {
            lineage_id
                .as_ref()
                .map_or_else(|| task.clone(), |lineage| format!("{task} {lineage}"))
        });
        let report = engine.begin_session(SessionBeginInput {
            task,
            lineage_id: lineage_id.clone(),
            actor_agent_id: optional_string(arguments, "agent"),
            query: Some(query.clone()),
            allowed_scopes: scopes.clone(),
            max_items: optional_usize(arguments, "max_items").unwrap_or(DEFAULT_CONTEXT_MAX_ITEMS),
            acceptance: None,
        });
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.continuity.begin",
            "schema_version": "mneme.v1_continuity.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": report.session.id,
            "lineage_id": lineage_id,
            "continuity_scope": continuity_scope,
            "allowed_scopes": scopes,
            "read_and_honor_required": true,
            "partial_context": report.context_pack.metadata.partial_context,
            "not_full_transcript": report.context_pack.metadata.not_full_transcript,
            "warning": report.context_pack.metadata.warning.clone(),
            "context_item_count": report.context_pack.items.len(),
            "omitted_count": report.context_pack.omitted.len(),
            "report": report,
            "next_recommended_actions": task_start_next_actions(),
        }))
    }

    fn v1_continuity_end(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let continuity_scope = continuity_scope(arguments);
        let remember = optional_string_vec(arguments, "remember").unwrap_or_default();
        let report = engine
            .end_session(SessionEndInput {
                session_id: required_string(arguments, "session_id")?,
                actor_agent_id: optional_string(arguments, "agent"),
                scope: Some(continuity_scope.clone()),
                summary: optional_string(arguments, "summary"),
                remember,
                verifier_report: None,
            })
            .map_err(McpError::tool)?;
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.continuity.end",
            "schema_version": "mneme.v1_continuity.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": report.session.id,
            "lineage_id": report.session.lineage_id,
            "continuity_scope": continuity_scope,
            "write_back_required": true,
            "write_back_ok": !report.remembered_claim_ids.is_empty(),
            "remembered_event_count": report.remembered_event_ids.len(),
            "remembered_claim_count": report.remembered_claim_ids.len(),
            "report": report,
            "next_recommended_actions": task_finish_next_actions(),
        }))
    }

    fn v1_continuity_handoff(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let query = required_string(arguments, "query")?;
        let lineage_id = optional_string(arguments, "lineage");
        let continuity_scope = continuity_scope(arguments);
        let scopes = optional_string_vec(arguments, "scopes")
            .unwrap_or_else(|| continuity_allowed_scopes(&continuity_scope));
        let context_pack = engine.build_context_pack_with(
            ContextQuery::with_allowed_scopes(query.clone(), scopes.clone()).with_max_items(
                optional_usize(arguments, "max_items").unwrap_or(DEFAULT_CONTEXT_MAX_ITEMS),
            ),
        );
        self.persist_v1(&engine)?;
        let snapshot = engine.snapshot();
        let source_sessions = snapshot
            .sessions
            .iter()
            .filter(|session| session.status.as_str() == "closed")
            .filter(|session| match lineage_id.as_ref() {
                Some(lineage) => session.lineage_id.as_ref() == Some(lineage),
                None => true,
            })
            .cloned()
            .collect::<Vec<_>>();
        Ok(json!({
            "command": "v1.continuity.handoff",
            "schema_version": "mneme.v1_continuity.v1",
            "store": self.config.v1_store.display().to_string(),
            "lineage_id": lineage_id,
            "continuity_scope": continuity_scope,
            "allowed_scopes": scopes,
            "query": query,
            "sequential_handoff_required": true,
            "read_and_honor_required": true,
            "partial_context": context_pack.metadata.partial_context,
            "not_full_transcript": context_pack.metadata.not_full_transcript,
            "warning": context_pack.metadata.warning.clone(),
            "source_session_count": source_sessions.len(),
            "context_item_count": context_pack.items.len(),
            "omitted_count": context_pack.omitted.len(),
            "source_sessions": source_sessions,
            "context_pack": context_pack,
            "next_recommended_actions": handoff_next_actions(),
        }))
    }

    fn v1_backfill_context(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let report = engine
            .backfill_context(SessionBackfillInput {
                task: optional_string(arguments, "task"),
                lineage_id: optional_string(arguments, "lineage"),
                actor_agent_id: optional_string(arguments, "agent"),
                scope: optional_string(arguments, "scope"),
                summary: required_string(arguments, "summary")?,
                remember: optional_string_vec(arguments, "remember").unwrap_or_default(),
            })
            .map_err(McpError::tool)?;
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.backfill_context",
            "schema_version": "mneme.v1_backfill_context.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": report.session.id,
            "lineage_id": report.session.lineage_id,
            "partial_context": report.partial_context,
            "not_full_transcript": report.not_full_transcript,
            "warning": report.warning,
            "remembered_event_count": report.remembered_event_ids.len(),
            "remembered_claim_count": report.remembered_claim_ids.len(),
            "report": report,
            "next_recommended_actions": import_next_actions(),
        }))
    }

    fn v1_lifecycle(
        &self,
        arguments: &Map<String, Value>,
        command: &'static str,
    ) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let text = match command {
            "forget" => {
                if let Some(claim_id) = optional_string(arguments, "claim_id") {
                    format!("forget-id: {claim_id}")
                } else {
                    format!("forget: {}", required_string(arguments, "text")?)
                }
            }
            "correct" => {
                let new_text = required_string(arguments, "new_text")?;
                if let Some(claim_id) = optional_string(arguments, "claim_id") {
                    format!("correct-id: {claim_id} -> {new_text}")
                } else {
                    format!(
                        "correct: {} -> {new_text}",
                        required_string(arguments, "old_text")?
                    )
                }
            }
            _ => return Err(McpError::invalid_request("unknown v1 lifecycle command")),
        };
        engine
            .ingest_event(EventInput {
                speaker_id: optional_string(arguments, "speaker")
                    .unwrap_or_else(|| "user".to_owned()),
                actor_agent_id: optional_string(arguments, "agent"),
                text,
                scope: optional_string(arguments, "scope").unwrap_or_else(|| "private".to_owned()),
                trust_level: optional_string(arguments, "trust")
                    .unwrap_or_else(|| "trusted_user".to_owned()),
            })
            .map_err(McpError::tool)?;
        self.persist_v1(&engine)?;
        let snapshot = engine.snapshot();
        Ok(json!({
            "command": format!("v1.{command}"),
            "store": self.config.v1_store.display().to_string(),
            "event_count": snapshot.events.len(),
            "claim_count": snapshot.claims.len(),
            "claims": snapshot.claims,
        }))
    }

    fn v1_quality(&self) -> Result<Value, McpError> {
        let engine = self.load_v1_engine()?;
        let snapshot = engine.snapshot();
        let active_count = snapshot
            .claims
            .iter()
            .filter(|claim| claim.status == ClaimStatus::Active)
            .count();
        let blocked_secret_count = snapshot
            .claims
            .iter()
            .filter(|claim| claim.status == ClaimStatus::BlockedSecret)
            .count();
        Ok(json!({
            "command": "v1.quality",
            "store": self.config.v1_store.display().to_string(),
            "ok": blocked_secret_count == 0,
            "claim_count": snapshot.claims.len(),
            "active_count": active_count,
            "blocked_secret_count": blocked_secret_count,
            "session_count": snapshot.sessions.len(),
        }))
    }

    fn v1_outcome_status(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let engine = self.load_v1_engine()?;
        let session_id = required_string(arguments, "session_id")?;
        let session = engine
            .snapshot()
            .sessions
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| McpError::tool(format!("unknown session: {session_id}")))?;
        let status = session
            .gate_result
            .as_ref()
            .map(|gate| gate.status.as_str().to_owned())
            .unwrap_or_else(|| "no_acceptance_gate".to_owned());
        Ok(json!({
            "command": "v1.outcome.status",
            "schema_version": "mneme.v1_outcome_status.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": session.id,
            "status": status,
            "gate_result": session.gate_result,
            "next_recommended_actions": outcome_status_next_actions(),
        }))
    }

    fn v1_outcome_judge(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v1_engine()?;
        let session_id = required_string(arguments, "session_id")?;
        let id = required_string(arguments, "id")?;
        let verdict = parse_judgment_verdict(&required_string(arguments, "verdict")?)?;
        let report = OutcomeJudgmentReport {
            schema_version: "mneme.judgment.v1".to_owned(),
            task_id: optional_string(arguments, "task_id"),
            reviewer: optional_string(arguments, "reviewer"),
            results: vec![OutcomeJudgmentCriterionResult {
                id,
                verdict,
                evidence: optional_string(arguments, "evidence"),
            }],
        };
        let apply_report = engine
            .apply_outcome_judgment(&session_id, report)
            .map_err(McpError::tool)?;
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.outcome.judge",
            "schema_version": "mneme.v1_outcome_judge.v1",
            "store": self.config.v1_store.display().to_string(),
            "session_id": apply_report.session.id,
            "status": apply_report.gate_result.status.as_str(),
            "completed": apply_report.gate_result.completed,
            "gate_result": apply_report.gate_result,
            "next_recommended_actions": outcome_status_next_actions(),
        }))
    }

    fn v1_validate(&self) -> Result<Value, McpError> {
        let engine = self.load_v1_engine()?;
        let validation = validate_state(&engine.state());
        Ok(json!({
            "command": "v1.validate",
            "store": self.config.v1_store.display().to_string(),
            "validation": validation,
        }))
    }

    fn v1_snapshot(&self) -> Result<Value, McpError> {
        let engine = self.load_v1_engine()?;
        Ok(json!({
            "command": "v1.snapshot",
            "store": self.config.v1_store.display().to_string(),
            "snapshot": engine.snapshot(),
        }))
    }

    fn v2_init(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let workspace_id = optional_string(arguments, "workspace")
            .unwrap_or_else(|| self.config.team_workspace_id.clone());
        let mut engine = TeamMemoryEngine::new(TeamMemoryConfig {
            workspace_id: workspace_id.clone(),
        });
        if let Some(admin) = optional_string(arguments, "admin") {
            engine.upsert_user(TeamUserInput {
                user_id: admin,
                role: TeamRole::Admin,
            });
        }
        self.persist_v2(&engine)?;
        let state = engine.state();
        Ok(json!({
            "command": "v2.team.init",
            "store": self.config.team_store.display().to_string(),
            "workspace_id": workspace_id,
            "user_count": state.users.len(),
            "agent_count": state.agents.len(),
            "project_count": state.projects.len(),
            "memory_count": state.memories.len(),
            "validation": validate_team_state(&state),
        }))
    }

    fn v2_user_add(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let role = required_string(arguments, "role")?
            .parse::<TeamRole>()
            .map_err(McpError::tool)?;
        let entity = engine.upsert_user(TeamUserInput {
            user_id: required_string(arguments, "user")?,
            role,
        });
        self.persist_v2(&engine)?;
        Ok(entity_report(
            "v2.user.add",
            &self.config.team_store,
            entity,
            &engine,
        ))
    }

    fn v2_agent_add(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let entity = engine
            .upsert_agent(TeamAgentInput {
                agent_id: required_string(arguments, "agent")?,
                owner_user_id: required_string(arguments, "owner")?,
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(entity_report(
            "v2.agent.add",
            &self.config.team_store,
            entity,
            &engine,
        ))
    }

    fn v2_project_add(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let entity = engine
            .upsert_project(TeamProjectInput {
                project_id: required_string(arguments, "project")?,
                member_user_ids: optional_string_vec(arguments, "members").unwrap_or_default(),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(entity_report(
            "v2.project.add",
            &self.config.team_store,
            entity,
            &engine,
        ))
    }

    fn v2_project_grant(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let entity = engine
            .grant_project_member(
                &required_string(arguments, "project")?,
                &required_string(arguments, "user")?,
            )
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(entity_report(
            "v2.project.grant",
            &self.config.team_store,
            entity,
            &engine,
        ))
    }

    fn v2_remember(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let memory = engine
            .remember(TeamRememberInput {
                actor: actor(arguments)?,
                text: required_string(arguments, "text")?,
                scope: required_string(arguments, "scope")?,
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.team.remember",
            "store": self.config.team_store.display().to_string(),
            "memory": memory,
            "validation": validate_team_state(&engine.state()),
        }))
    }

    fn v2_context(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let query = required_string(arguments, "query")?;
        let actor = actor(arguments)?;
        let context_pack = engine.build_context_pack(TeamContextQuery {
            actor: actor.clone(),
            query: query.clone(),
            max_items: optional_usize(arguments, "max_items")
                .unwrap_or(DEFAULT_TEAM_CONTEXT_MAX_ITEMS),
        });
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.team.context",
            "store": self.config.team_store.display().to_string(),
            "actor_user_id": actor.user_id,
            "actor_agent_id": actor.agent_id,
            "query": query,
            "partial_context": context_pack.metadata.partial_context,
            "not_full_transcript": context_pack.metadata.not_full_transcript,
            "warning": context_pack.metadata.warning.clone(),
            "item_count": context_pack.items.len(),
            "omitted_count": context_pack.omitted.len(),
            "context_pack": context_pack,
            "next_recommended_actions": partial_context_next_actions(),
        }))
    }

    fn v2_handoff(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let query = required_string(arguments, "query")?;
        let actor = actor(arguments)?;
        let package = engine
            .build_handoff_package(TeamContextQuery {
                actor: actor.clone(),
                query: query.clone(),
                max_items: optional_usize(arguments, "max_items")
                    .unwrap_or(DEFAULT_TEAM_CONTEXT_MAX_ITEMS),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.team.handoff",
            "store": self.config.team_store.display().to_string(),
            "actor_user_id": actor.user_id,
            "actor_agent_id": actor.agent_id,
            "query": query,
            "partial_context": package.metadata.partial_context,
            "not_full_transcript": package.metadata.not_full_transcript,
            "warning": package.metadata.warning.clone(),
            "context_item_count": package.context_pack.items.len(),
            "sync_memory_count": package.sync_envelope.memories.len(),
            "firewall_ok": package.firewall.ok,
            "package": package,
            "next_recommended_actions": handoff_next_actions(),
        }))
    }

    fn v2_run_begin(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let actor = actor(arguments)?;
        let report = engine
            .begin_run(TeamRunBeginInput {
                actor: actor.clone(),
                task: required_string(arguments, "task")?,
                query: optional_string(arguments, "query"),
                scope: optional_string(arguments, "scope"),
                max_items: optional_usize(arguments, "max_items"),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.run.begin",
            "store": self.config.team_store.display().to_string(),
            "actor_user_id": actor.user_id,
            "actor_agent_id": actor.agent_id,
            "run_id": report.run.id,
            "partial_context": report.context_pack.metadata.partial_context,
            "not_full_transcript": report.context_pack.metadata.not_full_transcript,
            "warning": report.context_pack.metadata.warning.clone(),
            "report": report,
            "validation": validate_team_state(&engine.state()),
            "next_recommended_actions": task_start_next_actions(),
        }))
    }

    fn v2_run_note(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let report = engine
            .note_run(TeamRunNoteInput {
                actor: actor(arguments)?,
                run_id: required_string(arguments, "run_id")?,
                text: required_string(arguments, "text")?,
                scope: required_string(arguments, "scope")?,
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.run.note",
            "store": self.config.team_store.display().to_string(),
            "report": report,
            "validation": validate_team_state(&engine.state()),
        }))
    }

    fn v2_run_end(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let report = engine
            .end_run(TeamRunEndInput {
                actor: actor(arguments)?,
                run_id: required_string(arguments, "run_id")?,
                summary: required_string(arguments, "summary")?,
                next_steps: optional_string_vec(arguments, "next").unwrap_or_default(),
                remember: optional_string_vec(arguments, "remember").unwrap_or_default(),
                scope: optional_string(arguments, "scope"),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.run.end",
            "store": self.config.team_store.display().to_string(),
            "report": report,
            "validation": validate_team_state(&engine.state()),
        }))
    }

    fn v2_run_handoff(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let package = engine
            .build_run_handoff_package(TeamRunHandoffInput {
                actor: actor(arguments)?,
                run_id: required_string(arguments, "run_id")?,
                query: optional_string(arguments, "query"),
                max_items: optional_usize(arguments, "max_items"),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.run.handoff",
            "store": self.config.team_store.display().to_string(),
            "partial_context": package.metadata.partial_context,
            "not_full_transcript": package.metadata.not_full_transcript,
            "warning": package.metadata.warning.clone(),
            "context_item_count": package.context_pack.items.len(),
            "sync_memory_count": package.sync_envelope.memories.len(),
            "firewall_ok": package.firewall.ok,
            "package": package,
            "next_recommended_actions": handoff_next_actions(),
        }))
    }

    fn v2_promote(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let promotion = engine
            .create_promotion(TeamPromotionCreateInput {
                actor: actor(arguments)?,
                source_memory_id: required_string(arguments, "memory_id")?,
                note: optional_string(arguments, "note"),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.promote",
            "store": self.config.team_store.display().to_string(),
            "promotion": promotion,
            "validation": validate_team_state(&engine.state()),
        }))
    }

    fn v2_promotion_report(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let engine = self.load_v2_engine()?;
        let report = engine
            .promotion_review_report(&required_string(arguments, "promotion_id")?)
            .map_err(McpError::tool)?;
        Ok(json!({
            "command": "v2.promotion.report",
            "store": self.config.team_store.display().to_string(),
            "report": report,
        }))
    }

    fn v2_review(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let promotion = engine
            .review_promotion(TeamPromotionReviewInput {
                actor: actor(arguments)?,
                promotion_id: required_string(arguments, "promotion_id")?,
                approve: optional_bool(arguments, "approve").unwrap_or(false),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.review",
            "store": self.config.team_store.display().to_string(),
            "promotion": promotion,
            "validation": validate_team_state(&engine.state()),
        }))
    }

    fn v2_sync_export(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let envelope = engine
            .export_sync_envelope(TeamSyncExportInput {
                actor: actor(arguments)?,
                include_project_scopes: optional_bool(arguments, "include_projects")
                    .unwrap_or(false),
            })
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(json!({
            "command": "v2.sync.export",
            "store": self.config.team_store.display().to_string(),
            "memory_count": envelope.memories.len(),
            "event_count": envelope.events.len(),
            "omitted_count": envelope.omitted.len(),
            "envelope": envelope,
        }))
    }

    fn v2_sync_import(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let envelope_value = arguments
            .get("envelope")
            .cloned()
            .ok_or_else(|| McpError::invalid_request("missing required argument: envelope"))?;
        let envelope = serde_json::from_value::<TeamSyncEnvelope>(envelope_value)
            .map_err(|source| McpError::invalid_request(format!("invalid envelope: {source}")))?;
        let actor = if arguments.get("actor").is_some() {
            Some(actor(arguments)?)
        } else {
            None
        };
        let apply = optional_bool(arguments, "apply").unwrap_or(false);
        let report = engine.apply_sync_envelope(envelope, apply, actor);
        if apply && report.ok {
            self.persist_v2(&engine)?;
        }
        Ok(json!({
            "command": "v2.sync.import",
            "store": self.config.team_store.display().to_string(),
            "applied": apply,
            "report": report,
        }))
    }

    fn v2_firewall(&self) -> Result<Value, McpError> {
        let engine = self.load_v2_engine()?;
        Ok(json!({
            "command": "v2.firewall",
            "store": self.config.team_store.display().to_string(),
            "firewall": engine.firewall_report(),
        }))
    }

    fn v2_quality(&self) -> Result<Value, McpError> {
        let engine = self.load_v2_engine()?;
        Ok(json!({
            "command": "v2.quality",
            "store": self.config.team_store.display().to_string(),
            "quality": engine.quality_report(),
        }))
    }

    fn v2_ontology(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let engine = self.load_v2_engine()?;
        let ontology = if arguments.get("actor").is_some() {
            engine
                .ontology_report_for_actor(actor(arguments)?)
                .map_err(McpError::tool)?
        } else {
            engine.ontology_report()
        };
        Ok(json!({
            "command": "v2.ontology",
            "store": self.config.team_store.display().to_string(),
            "ontology": ontology,
        }))
    }

    fn v2_revoke_user(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let entity = engine
            .revoke_user(actor(arguments)?, &required_string(arguments, "user")?)
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(entity_report(
            "v2.revoke.user",
            &self.config.team_store,
            entity,
            &engine,
        ))
    }

    fn v2_revoke_agent(&self, arguments: &Map<String, Value>) -> Result<Value, McpError> {
        let mut engine = self.load_v2_engine()?;
        let entity = engine
            .revoke_agent(
                actor(arguments)?,
                &required_string(arguments, "target_agent")?,
            )
            .map_err(McpError::tool)?;
        self.persist_v2(&engine)?;
        Ok(entity_report(
            "v2.revoke.agent",
            &self.config.team_store,
            entity,
            &engine,
        ))
    }

    fn v2_validate(&self) -> Result<Value, McpError> {
        let engine = self.load_v2_engine()?;
        Ok(json!({
            "command": "v2.validate",
            "store": self.config.team_store.display().to_string(),
            "validation": validate_team_state(&engine.state()),
        }))
    }

    fn v2_snapshot(&self) -> Result<Value, McpError> {
        let engine = self.load_v2_engine()?;
        Ok(json!({
            "command": "v2.snapshot",
            "store": self.config.team_store.display().to_string(),
            "snapshot": engine.state(),
        }))
    }

    fn load_v1_engine(&self) -> Result<MnemeEngine, McpError> {
        let store = JsonFileStore::new(self.config.v1_store.clone());
        MnemeEngine::from_store(MnemeConfig::default(), &store).map_err(McpError::store)
    }

    fn persist_v1(&self, engine: &MnemeEngine) -> Result<(), McpError> {
        let mut store = JsonFileStore::new(self.config.v1_store.clone());
        engine.persist(&mut store).map_err(McpError::store)
    }

    fn load_v2_engine(&self) -> Result<TeamMemoryEngine, McpError> {
        let store = JsonTeamFileStore::new(self.config.team_store.clone());
        TeamMemoryEngine::from_store(
            TeamMemoryConfig {
                workspace_id: self.config.team_workspace_id.clone(),
            },
            &store,
        )
        .map_err(McpError::tool)
    }

    fn persist_v2(&self, engine: &TeamMemoryEngine) -> Result<(), McpError> {
        let mut store = JsonTeamFileStore::new(self.config.team_store.clone());
        engine.persist(&mut store).map_err(McpError::tool)
    }
}

/// MCP tool definition.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    /// Tool name.
    pub name: &'static str,
    /// Tool description.
    pub description: &'static str,
    /// JSON schema for tool arguments.
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
    /// MCP tool behavior annotations for model/client planning.
    pub annotations: ToolAnnotations,
}

/// MCP tool annotations.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolAnnotations {
    /// Human-readable title.
    pub title: &'static str,
    /// True when the tool does not mutate local Mneme stores.
    pub read_only_hint: bool,
    /// True when the tool can delete, revoke, import, or otherwise perform risky mutation.
    pub destructive_hint: bool,
    /// True when repeat calls with the same arguments should not add duplicate state.
    pub idempotent_hint: bool,
    /// True when the tool may interact with external systems outside local stores.
    pub open_world_hint: bool,
}

/// MCP server error.
#[derive(Debug, Clone)]
pub struct McpError {
    code: i64,
    message: String,
}

impl McpError {
    fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: message.into(),
        }
    }

    fn method_not_found(message: impl Into<String>) -> Self {
        Self {
            code: -32601,
            message: message.into(),
        }
    }

    fn tool(source: impl Display) -> Self {
        Self {
            code: -32001,
            message: source.to_string(),
        }
    }

    fn store(source: StoreError) -> Self {
        Self {
            code: -32002,
            message: source.to_string(),
        }
    }
}

impl Display for McpError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for McpError {}

fn default_v1_store_path() -> Result<PathBuf, McpError> {
    env::current_dir()
        .map(|dir| dir.join(".mneme").join("mneme-v1.json"))
        .map_err(|source| McpError::tool(format!("read current dir: {source}")))
}

fn default_team_store_path() -> Result<PathBuf, McpError> {
    env::current_dir()
        .map(|dir| dir.join(".mneme").join("mneme-team-v2.json"))
        .map_err(|source| McpError::tool(format!("read current dir: {source}")))
}

fn parent_exists(path: &Path) -> bool {
    path.parent().is_some_and(Path::exists)
}

fn continuity_scope(arguments: &Map<String, Value>) -> String {
    optional_string(arguments, "scope")
        .and_then(non_empty_string)
        .or_else(|| {
            optional_string(arguments, "lineage")
                .and_then(non_empty_string)
                .map(|lineage| format!("lineage:{lineage}"))
        })
        .unwrap_or_else(|| "private".to_owned())
}

fn continuity_allowed_scopes(scope: &str) -> Vec<String> {
    let mut scopes = vec![scope.to_owned()];
    if scope != "private" {
        scopes.push("private".to_owned());
    }
    scopes
}

fn non_empty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn result_response(request_id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": request_id, "result": result})
}

fn error_response(request_id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message.into()}})
}

fn tool_result(value: Value) -> Value {
    json!({
        "content": [{"type": "text", "text": value.to_string()}],
        "structuredContent": value,
    })
}

fn is_personal_workflow_tool(name: &str) -> bool {
    matches!(
        name,
        "mneme_task_start"
            | "mneme_task_finish"
            | "mneme_prepare_handoff"
            | "mneme_import_previous_context"
    )
}

fn guide_next_actions() -> Vec<&'static str> {
    vec![
        "Call mneme_task_start before planning meaningful work.",
        "Use cited memories as partial context and verify against the current repo.",
        "Call mneme_task_finish with a summary and durable remember items before stopping.",
    ]
}

fn partial_context_next_actions() -> Vec<&'static str> {
    vec![
        "Treat this as scoped, ranked memory rather than the full transcript.",
        "Use citations and source counts as confidence signals.",
        "Verify stale or surprising memory against current files before acting.",
    ]
}

fn task_start_next_actions() -> Vec<&'static str> {
    vec![
        "Read the cited context before planning.",
        "Keep the returned session_id, lineage, and scope for task finish.",
        "Call mneme_task_finish before stopping.",
    ]
}

fn task_finish_next_actions() -> Vec<&'static str> {
    vec![
        "If another agent will continue, call mneme_prepare_handoff.",
        "If the session used an outcome gate, inspect mneme_v1_outcome_status before treating the work as done.",
        "Do not store secrets, raw credentials, or private local paths.",
        "Use the same lineage and scope for follow-up work.",
    ]
}

fn outcome_status_next_actions() -> Vec<&'static str> {
    vec![
        "Treat passed gate_result as completed work evidence.",
        "If status is failed or error, continue from the failed criterion evidence before handoff.",
        "If status is pending_judgment, request an external verdict before closing the task.",
    ]
}

fn handoff_next_actions() -> Vec<&'static str> {
    vec![
        "Pass the handoff package to the next agent as partial cited context.",
        "The receiving agent should call mneme_task_start with the same lineage and scope.",
        "Do not treat omitted or redacted memory as available context.",
    ]
}

fn import_next_actions() -> Vec<&'static str> {
    vec![
        "Use this imported context as a summarized historical session, not a raw transcript.",
        "Follow with mneme_task_start using the same lineage and scope.",
        "Keep future durable facts in mneme_task_finish remember items.",
    ]
}

fn is_json_rpc_notification(request: &Value) -> bool {
    request.get("id").is_none()
        && request
            .get("method")
            .and_then(Value::as_str)
            .is_some_and(|method| method.starts_with("notifications/"))
}

fn required_string(arguments: &Map<String, Value>, key: &str) -> Result<String, McpError> {
    optional_string(arguments, key)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| McpError::invalid_request(format!("missing required argument: {key}")))
}

fn optional_string(arguments: &Map<String, Value>, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn optional_bool(arguments: &Map<String, Value>, key: &str) -> Option<bool> {
    arguments.get(key).and_then(Value::as_bool)
}

fn optional_usize(arguments: &Map<String, Value>, key: &str) -> Option<usize> {
    arguments
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn optional_string_vec(arguments: &Map<String, Value>, key: &str) -> Option<Vec<String>> {
    let value = arguments.get(key)?;
    if let Some(text) = value.as_str() {
        return Some(vec![text.to_owned()]);
    }
    let array = value.as_array()?;
    Some(
        array
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
    )
}

fn parse_judgment_verdict(value: &str) -> Result<OutcomeJudgmentVerdict, McpError> {
    match value {
        "pass" | "passed" | "accept" | "accepted" => Ok(OutcomeJudgmentVerdict::Pass),
        "fail" | "failed" | "reject" | "rejected" => Ok(OutcomeJudgmentVerdict::Fail),
        _ => Err(McpError::invalid_request(format!(
            "unknown judgment verdict: {value}; expected pass or fail"
        ))),
    }
}

fn actor(arguments: &Map<String, Value>) -> Result<TeamActor, McpError> {
    Ok(TeamActor {
        user_id: required_string(arguments, "actor")?,
        agent_id: optional_string(arguments, "agent"),
    })
}

fn entity_report<T: Serialize>(
    command: &'static str,
    store: &Path,
    entity: T,
    engine: &TeamMemoryEngine,
) -> Value {
    json!({
        "command": command,
        "store": store.display().to_string(),
        "entity": entity,
        "validation": validate_team_state(&engine.state()),
    })
}

fn object_schema(required: &[&str], properties: Vec<(&'static str, Value)>) -> Value {
    let props = properties
        .into_iter()
        .map(|(key, value)| (key.to_owned(), enrich_property_schema(key, value)))
        .collect::<Map<_, _>>();
    json!({
        "type": "object",
        "required": required,
        "properties": props,
    })
}

fn enrich_property_schema(key: &str, mut schema: Value) -> Value {
    if let Some(object) = schema.as_object_mut() {
        object
            .entry("description")
            .or_insert_with(|| Value::String(field_description(key).to_owned()));
        if let Some(examples) = field_examples(key) {
            object
                .entry("examples")
                .or_insert_with(|| Value::Array(examples.into_iter().map(Value::String).collect()));
        }
    }
    schema
}

fn field_description(key: &str) -> &'static str {
    match key {
        "actor" => "Team user id performing the action, for example alice or reviewer-bob.",
        "admin" => "Optional admin user id to create during team initialization.",
        "agent" => "Agent id making the call, for example codex, claude-code, cursor, planner, or reviewer.",
        "apply" => "When false, dry-run only. When true, apply the sync envelope and mutate the store.",
        "approve" => "True to approve a promotion candidate, false to reject it.",
        "claim_id" => "Stable claim id returned by Mneme, such as claim-001.",
        "envelope" => "Connector-safe V2 sync envelope previously exported by Mneme.",
        "include_handoff" => "When true, task_start also builds a handoff/context package before opening the session.",
        "include_projects" => "Include connector-safe project metadata in the sync export when policy allows it.",
        "lineage" => "Stable task lineage shared across sessions. Use an issue id, branch name, or task slug.",
        "max_items" => "Maximum context items to return. Keep small unless the agent explicitly needs more context.",
        "memory_id" => "V2 memory id to promote, review, or inspect.",
        "members" => "Team user ids that can access the project scope.",
        "new_text" => "Replacement durable memory text.",
        "next" => "Next steps another agent or future session should continue.",
        "note" => "Reviewer or promotion note explaining why the action is being taken.",
        "old_text" => "Existing memory text to replace when claim_id is not available.",
        "owner" => "User id that owns the agent.",
        "project" => "Project id used for project-scoped memory, for example atlas.",
        "promotion_id" => "Promotion candidate id returned by mneme_v2_promote.",
        "query" => "Plain-language retrieval query for scoped memory. Use task-specific terms, not secrets.",
        "remember" => "Durable non-secret facts to save for future sessions. Do not include raw transcripts.",
        "role" => "Team role such as admin, member, or reviewer.",
        "run_id" => "V2 task-run id returned by mneme_v2_run_begin.",
        "scope" => "Memory scope. For V1 use private or project:<name>. For V2 use private:<user>, project:<id>, agent-private:<agent>, or team.",
        "scopes" => "Allowed V1 scopes to read. Include the shared task scope and private only when appropriate.",
        "session_id" => "V1 session id returned by mneme_task_start or mneme_v1_begin.",
        "situation" => "Short description of what the agent is trying to do, used to return tool guidance.",
        "speaker" => "Speaker id for raw events, usually user or agent.",
        "summary" => "Concise public-safe summary of completed work. Do not include secrets or raw transcripts.",
        "target_agent" => "Agent id to revoke.",
        "task" => "Concrete task the agent is starting or backfilling.",
        "text" => "Memory or event text. Store durable non-secret facts only.",
        "trust" => "Trust level for raw events, such as trusted_user, agent_summary, or untrusted_transcript.",
        "user" => "Team user id.",
        "workspace" => "Team workspace id for the local V2 store.",
        _ => "Tool argument.",
    }
}

fn field_examples(key: &str) -> Option<Vec<String>> {
    let examples = match key {
        "actor" => vec!["alice", "reviewer-bob"],
        "agent" => vec!["codex", "claude-code", "cursor"],
        "lineage" => vec!["issue-42-auth-refactor", "branch:feat/mcp-memory"],
        "query" => vec![
            "release checklist evidence",
            "continuity smoke before release",
        ],
        "scope" => vec!["private", "project:atlas", "team"],
        "summary" => vec!["Implemented the parser and verified cargo test."],
        "remember" => vec!["Project Atlas release requires local dogfood evidence."],
        "task" => vec!["Continue the MCP handoff validation"],
        "text" => vec!["user prefers concise release notes"],
        _ => return None,
    };
    Some(examples.into_iter().map(str::to_owned).collect())
}

fn string_schema() -> Value {
    json!({"type": "string"})
}

fn bool_schema() -> Value {
    json!({"type": "boolean"})
}

fn int_schema() -> Value {
    json!({"type": "integer", "minimum": 1})
}

fn string_array_schema() -> Value {
    json!({"type": "array", "items": {"type": "string"}})
}

fn tool_definition(
    name: &'static str,
    description: &'static str,
    input_schema: Value,
) -> ToolDefinition {
    ToolDefinition {
        name,
        description,
        input_schema,
        annotations: tool_annotations(name),
    }
}

fn tool_annotations(name: &'static str) -> ToolAnnotations {
    let read_only = matches!(
        name,
        "mneme_mcp_status"
            | "mneme_agent_guide"
            | "mneme_v1_context"
            | "mneme_v1_quality"
            | "mneme_v1_outcome_status"
            | "mneme_v1_validate"
            | "mneme_v1_snapshot"
            | "mneme_v1_continuity_handoff"
            | "mneme_prepare_handoff"
            | "mneme_v2_team_context"
            | "mneme_v2_team_handoff"
            | "mneme_v2_run_handoff"
            | "mneme_v2_promotion_report"
            | "mneme_v2_firewall"
            | "mneme_v2_quality"
            | "mneme_v2_ontology"
            | "mneme_v2_validate"
            | "mneme_v2_snapshot"
            | "mneme_v2_sync_export"
    );
    let destructive = matches!(
        name,
        "mneme_v1_forget"
            | "mneme_v1_correct"
            | "mneme_v2_sync_import"
            | "mneme_v2_review"
            | "mneme_v2_revoke_user"
            | "mneme_v2_revoke_agent"
    );
    let idempotent = matches!(
        name,
        "mneme_mcp_status"
            | "mneme_agent_guide"
            | "mneme_v1_context"
            | "mneme_v1_quality"
            | "mneme_v1_outcome_status"
            | "mneme_v1_validate"
            | "mneme_v1_snapshot"
            | "mneme_v2_team_context"
            | "mneme_v2_team_handoff"
            | "mneme_v2_firewall"
            | "mneme_v2_quality"
            | "mneme_v2_ontology"
            | "mneme_v2_validate"
            | "mneme_v2_snapshot"
    );
    ToolAnnotations {
        title: tool_title(name),
        read_only_hint: read_only,
        destructive_hint: destructive,
        idempotent_hint: idempotent,
        open_world_hint: false,
    }
}

fn tool_title(name: &str) -> &'static str {
    match name {
        "mneme_mcp_status" => "Mneme MCP Status",
        "mneme_agent_guide" => "Mneme Agent Guide",
        "mneme_task_start" => "Start Mneme Task",
        "mneme_task_finish" => "Finish Mneme Task",
        "mneme_prepare_handoff" => "Prepare Mneme Handoff",
        "mneme_import_previous_context" => "Import Previous Context",
        "mneme_v1_remember" => "V1 Remember",
        "mneme_v1_ingest" => "V1 Ingest",
        "mneme_v1_context" => "V1 Context",
        "mneme_v1_begin" => "V1 Begin",
        "mneme_v1_end" => "V1 End",
        "mneme_v1_continuity_begin" => "V1 Continuity Begin",
        "mneme_v1_continuity_end" => "V1 Continuity End",
        "mneme_v1_continuity_handoff" => "V1 Continuity Handoff",
        "mneme_v1_backfill_context" => "V1 Backfill Context",
        "mneme_v1_forget" => "V1 Forget",
        "mneme_v1_correct" => "V1 Correct",
        "mneme_v1_outcome_status" => "V1 Outcome Status",
        "mneme_v2_team_init" => "V2 Team Init",
        "mneme_v2_user_add" => "V2 User Add",
        "mneme_v2_agent_add" => "V2 Agent Add",
        "mneme_v2_project_add" => "V2 Project Add",
        "mneme_v2_project_grant" => "V2 Project Grant",
        "mneme_v2_team_remember" => "V2 Team Remember",
        "mneme_v2_team_context" => "V2 Team Context",
        "mneme_v2_team_handoff" => "V2 Team Handoff",
        "mneme_v2_run_begin" => "V2 Run Begin",
        "mneme_v2_run_note" => "V2 Run Note",
        "mneme_v2_run_end" => "V2 Run End",
        "mneme_v2_run_handoff" => "V2 Run Handoff",
        "mneme_v2_promote" => "V2 Promote",
        "mneme_v2_promotion_report" => "V2 Promotion Report",
        "mneme_v2_review" => "V2 Review",
        "mneme_v2_sync_export" => "V2 Sync Export",
        "mneme_v2_sync_import" => "V2 Sync Import",
        "mneme_v2_firewall" => "V2 Firewall",
        "mneme_v2_quality" => "V2 Quality",
        "mneme_v2_ontology" => "V2 Ontology",
        "mneme_v2_revoke_user" => "V2 Revoke User",
        "mneme_v2_revoke_agent" => "V2 Revoke Agent",
        "mneme_v2_validate" => "V2 Validate",
        "mneme_v2_snapshot" => "V2 Snapshot",
        _ => "Mneme Tool",
    }
}

fn global_tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition(
            "mneme_mcp_status",
            "Check Mneme MCP installation, store paths, tool inventory, recommended agent tools, and continuity contract. Use first.",
            object_schema(&[], Vec::new()),
        ),
        tool_definition(
            "mneme_agent_guide",
            "Explain which Mneme MCP tools an agent should use for the current situation. Use when unsure which tool comes next.",
            object_schema(&[], vec![("situation", string_schema())]),
        ),
    ]
}

fn personal_workflow_tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition(
            "mneme_task_start",
            "Preferred task-start tool for agents. It reads partial cited context, opens a continuity session, and can include a handoff package.",
            object_schema(
                &["task"],
                vec![
                    ("task", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("query", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                    ("agent", string_schema()),
                    ("include_handoff", bool_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_task_finish",
            "Preferred task-finish tool for agents. It closes the session and writes durable non-secret memory back into the same lineage/scope.",
            object_schema(
                &["session_id"],
                vec![
                    ("session_id", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("summary", string_schema()),
                    ("remember", string_array_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_prepare_handoff",
            "Preferred handoff tool for agents. It packages partial cited context for the next sequential agent without exposing a full transcript.",
            object_schema(
                &["query"],
                vec![
                    ("query", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_import_previous_context",
            "Import a public-safe summary of prior work that happened before Mneme was active. Do not paste secrets or raw transcripts.",
            object_schema(
                &["summary"],
                vec![
                    ("summary", string_schema()),
                    ("remember", string_array_schema()),
                    ("task", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
    ]
}

fn personal_tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition(
            "mneme_v1_remember",
            "Store one explicit durable V1 personal-memory claim. Use for explicit facts only; do not store secrets or transient task notes.",
            object_schema(
                &["text"],
                vec![
                    ("text", string_schema()),
                    ("scope", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_ingest",
            "Append one raw V1 event for evals or advanced adapters. Most agents should prefer mneme_task_start/finish.",
            object_schema(
                &["text"],
                vec![
                    ("text", string_schema()),
                    ("speaker", string_schema()),
                    ("scope", string_schema()),
                    ("trust", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_context",
            "Read a scoped V1 context pack. Use for advanced reads; most task starts should use mneme_task_start.",
            object_schema(
                &["query"],
                vec![
                    ("query", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_begin",
            "Begin a V1 agent session and return task-scoped context. Prefer mneme_task_start unless you need low-level control.",
            object_schema(
                &["task"],
                vec![
                    ("task", string_schema()),
                    ("lineage", string_schema()),
                    ("query", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_end",
            "End a V1 agent session and record optional memories. Prefer mneme_task_finish for normal agent loops.",
            object_schema(
                &["session_id"],
                vec![
                    ("session_id", string_schema()),
                    ("scope", string_schema()),
                    ("summary", string_schema()),
                    ("remember", string_array_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_continuity_begin",
            "Begin a V1 continuity session with explicit lineage/scope read discipline. Prefer mneme_task_start for normal agents.",
            object_schema(
                &["task"],
                vec![
                    ("task", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("query", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_continuity_end",
            "End a V1 continuity session and write back memory into the shared lineage/scope. Prefer mneme_task_finish for normal agents.",
            object_schema(
                &["session_id"],
                vec![
                    ("session_id", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("summary", string_schema()),
                    ("remember", string_array_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_continuity_handoff",
            "Build a V1 continuity handoff package for another agent/session. Prefer mneme_prepare_handoff for normal agents.",
            object_schema(
                &["query"],
                vec![
                    ("query", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_backfill_context",
            "Import a public-safe summary of previous context that was not captured live. Prefer mneme_import_previous_context for normal agents.",
            object_schema(
                &["summary"],
                vec![
                    ("summary", string_schema()),
                    ("remember", string_array_schema()),
                    ("task", string_schema()),
                    ("lineage", string_schema()),
                    ("scope", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_forget",
            "Forget V1 memory by claim id or text. Use only when a cited memory is wrong, unsafe, or no longer useful.",
            object_schema(
                &[],
                vec![
                    ("claim_id", string_schema()),
                    ("text", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_correct",
            "Correct V1 memory by claim id or old/new text. Use when replacing a durable fact with a better cited version.",
            object_schema(
                &["new_text"],
                vec![
                    ("claim_id", string_schema()),
                    ("old_text", string_schema()),
                    ("new_text", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_quality",
            "Summarize V1 personal-memory quality counters before trusting or publishing memory state.",
            object_schema(&[], Vec::new()),
        ),
        tool_definition(
            "mneme_v1_outcome_status",
            "Inspect the first-class V1 outcome gate result for a session. Use before treating gated work as completed.",
            object_schema(&["session_id"], vec![("session_id", string_schema())]),
        ),
        tool_definition(
            "mneme_v1_outcome_judge",
            "Apply an external reviewer/model verdict to a pending V1 judgment gate. Use only when a user or reviewer has provided an explicit pass/fail verdict.",
            object_schema(
                &["session_id", "id", "verdict"],
                vec![
                    ("session_id", string_schema()),
                    ("id", string_schema()),
                    ("verdict", string_schema()),
                    ("evidence", string_schema()),
                    ("reviewer", string_schema()),
                    ("task_id", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v1_validate",
            "Validate the V1 JSON store without mutating memory.",
            object_schema(&[], Vec::new()),
        ),
        tool_definition(
            "mneme_v1_snapshot",
            "Return the V1 store snapshot for inspection. Avoid exposing this output to unauthorized users.",
            object_schema(&[], Vec::new()),
        ),
    ]
}

fn team_tools() -> Vec<ToolDefinition> {
    vec![
        tool_definition(
            "mneme_v2_team_init",
            "Initialize a V2 team store. Use once per isolated team store before adding users, agents, projects, or runs.",
            object_schema(
                &[],
                vec![("workspace", string_schema()), ("admin", string_schema())],
            ),
        ),
        tool_definition(
            "mneme_v2_user_add",
            "Add or update a V2 team user. Use during setup; do not call for every memory write.",
            object_schema(
                &["user", "role"],
                vec![("user", string_schema()), ("role", string_schema())],
            ),
        ),
        tool_definition(
            "mneme_v2_agent_add",
            "Add or update a V2 team agent and owner. Use during setup before run or handoff flows.",
            object_schema(
                &["agent", "owner"],
                vec![("agent", string_schema()), ("owner", string_schema())],
            ),
        ),
        tool_definition(
            "mneme_v2_project_add",
            "Add or update a V2 project and members. Use to define project access boundaries.",
            object_schema(
                &["project"],
                vec![
                    ("project", string_schema()),
                    ("members", string_array_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_project_grant",
            "Grant one user access to a V2 project. Use when a team member should read project-scoped memory.",
            object_schema(
                &["project", "user"],
                vec![("project", string_schema()), ("user", string_schema())],
            ),
        ),
        tool_definition(
            "mneme_v2_team_remember",
            "Write scoped V2 team memory through policy. Use durable, non-secret facts only.",
            object_schema(
                &["text", "actor", "scope"],
                vec![
                    ("text", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("scope", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_team_context",
            "Read a policy-filtered V2 team context pack for one actor. Treat output as partial context, not a full transcript.",
            object_schema(
                &["query", "actor"],
                vec![
                    ("query", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_team_handoff",
            "Build a policy-filtered V2 handoff package for another team agent. Use before sequential continuation.",
            object_schema(
                &["query", "actor"],
                vec![
                    ("query", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_run_begin",
            "Open a V2 team task run and read actor-scoped context. Use at the start of team-agent work.",
            object_schema(
                &["task", "actor"],
                vec![
                    ("task", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("query", string_schema()),
                    ("scope", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_run_note",
            "Attach scoped memory to an open V2 run. Use for durable non-secret notes discovered during the run.",
            object_schema(
                &["run_id", "text", "actor", "scope"],
                vec![
                    ("run_id", string_schema()),
                    ("text", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("scope", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_run_end",
            "Close a V2 team task run with summary, next steps, and optional durable memories. Call before stopping.",
            object_schema(
                &["run_id", "summary", "actor"],
                vec![
                    ("run_id", string_schema()),
                    ("summary", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("next", string_array_schema()),
                    ("remember", string_array_schema()),
                    ("scope", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_run_handoff",
            "Build a policy-filtered V2 handoff package for one closed or active run.",
            object_schema(
                &["run_id", "actor"],
                vec![
                    ("run_id", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("query", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_promote",
            "Create a V2 promotion candidate before moving memory into broader team visibility.",
            object_schema(
                &["memory_id", "actor"],
                vec![
                    ("memory_id", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("note", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_promotion_report",
            "Inspect V2 promotion quality and reviewer risk without changing review state.",
            object_schema(&["promotion_id"], vec![("promotion_id", string_schema())]),
        ),
        tool_definition(
            "mneme_v2_review",
            "Approve or reject a V2 promotion candidate. Use carefully because this changes team memory visibility.",
            object_schema(
                &["promotion_id", "actor", "approve"],
                vec![
                    ("promotion_id", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("approve", bool_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_sync_export",
            "Export a connector-safe V2 sync envelope. Use for read-only inspection or connector handoff.",
            object_schema(
                &["actor"],
                vec![
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("include_projects", bool_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_sync_import",
            "Dry-run or apply a V2 connector sync envelope. Prefer dry-run first; apply mutates the local team store.",
            object_schema(
                &["envelope"],
                vec![
                    ("envelope", json!({"type": "object"})),
                    ("apply", bool_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_firewall",
            "Scan V2 team memory for leakage, quarantine, and poisoning risk before handoff or sync.",
            object_schema(&[], Vec::new()),
        ),
        tool_definition(
            "mneme_v2_quality",
            "Analyze V2 duplicates, conflicts, stale candidates, and run state before release or handoff.",
            object_schema(&[], Vec::new()),
        ),
        tool_definition(
            "mneme_v2_ontology",
            "Return actor-scoped V2 entity, relation, and attribute projection. Use for inspection, not broad semantic proof.",
            object_schema(
                &[],
                vec![("actor", string_schema()), ("agent", string_schema())],
            ),
        ),
        tool_definition(
            "mneme_v2_revoke_user",
            "Revoke a V2 user through an admin actor. Use carefully because it changes access to team memory.",
            object_schema(
                &["user", "actor"],
                vec![
                    ("user", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_revoke_agent",
            "Revoke a V2 agent through an admin actor. Use carefully because it changes agent access.",
            object_schema(
                &["target_agent", "actor"],
                vec![
                    ("target_agent", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        ),
        tool_definition(
            "mneme_v2_validate",
            "Validate the V2 team JSON store without mutating memory.",
            object_schema(&[], Vec::new()),
        ),
        tool_definition(
            "mneme_v2_snapshot",
            "Return the V2 team store snapshot for inspection. Avoid sharing with unauthorized actors.",
            object_schema(&[], Vec::new()),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(mode: ServerMode) -> McpServerConfig {
        let root = env::temp_dir().join(format!("mneme-mcp-test-{}", std::process::id()));
        McpServerConfig {
            mode,
            v1_store: root.join("mneme-v1.json"),
            team_store: root.join("mneme-team-v2.json"),
            team_workspace_id: "team".to_owned(),
        }
    }

    #[test]
    fn notification_lines_do_not_emit_json_rpc_responses() {
        let server = McpServer::new(test_config(ServerMode::All));
        let response = server.handle_json_line(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}"#,
        );
        assert!(response.is_empty());
    }

    #[test]
    fn server_mode_filters_tool_inventory() {
        let personal = McpServer::new(test_config(ServerMode::Personal));
        let team = McpServer::new(test_config(ServerMode::Team));
        let all = McpServer::new(test_config(ServerMode::All));

        assert!(personal
            .tools()
            .iter()
            .all(|tool| tool.name == "mneme_mcp_status"
                || tool.name == "mneme_agent_guide"
                || is_personal_workflow_tool(tool.name)
                || tool.name.starts_with("mneme_v1_")));
        assert!(team
            .tools()
            .iter()
            .all(|tool| tool.name == "mneme_mcp_status"
                || tool.name == "mneme_agent_guide"
                || tool.name.starts_with("mneme_v2_")));
        assert_eq!(
            personal.tools().len() + team.tools().len() - global_tools().len(),
            all.tools().len()
        );
    }

    #[test]
    fn backfill_tool_marks_partial_context_and_feeds_handoff() {
        let server = McpServer::new(test_config(ServerMode::Personal));
        let backfill = server.handle_request(json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": {
                "name": "mneme_v1_backfill_context",
                "arguments": {
                    "summary": "Earlier session decided to keep MCP evidence reduced.",
                    "remember": ["MCP evidence format uses reduced public-safe summaries"],
                    "lineage": "mcp-evidence",
                    "scope": "lineage:mcp-evidence",
                    "agent": "codex"
                }
            }
        }));
        let backfill_content = backfill
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .expect("backfill should return structured content");
        assert_eq!(backfill_content["partial_context"], true);
        assert_eq!(backfill_content["not_full_transcript"], true);
        assert_eq!(backfill_content["remembered_claim_count"], 1);

        let handoff = server.handle_request(json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "mneme_v1_continuity_handoff",
                "arguments": {
                    "query": "MCP evidence public-safe",
                    "lineage": "mcp-evidence",
                    "scope": "lineage:mcp-evidence"
                }
            }
        }));
        let handoff_content = handoff
            .get("result")
            .and_then(|result| result.get("structuredContent"))
            .expect("handoff should return structured content");
        assert_eq!(handoff_content["partial_context"], true);
        assert_eq!(handoff_content["source_session_count"], 1);
        assert_eq!(
            handoff_content["context_pack"]["metadata"]["source_session_count"],
            1
        );
        assert_eq!(handoff_content["context_item_count"], 1);
    }
}
