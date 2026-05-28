//! MCP JSON-RPC surface for Mneme local memory stores.

use std::env;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use mneme_core::{
    validate_state, validate_team_state, ClaimStatus, ContextQuery, EventInput, JsonFileStore,
    JsonTeamFileStore, MnemeConfig, MnemeEngine, SessionBeginInput, SessionEndInput, StoreError,
    TeamActor, TeamAgentInput, TeamContextQuery, TeamMemoryConfig, TeamMemoryEngine,
    TeamProjectInput, TeamPromotionCreateInput, TeamPromotionReviewInput, TeamRememberInput,
    TeamRole, TeamRunBeginInput, TeamRunEndInput, TeamRunHandoffInput, TeamRunNoteInput,
    TeamSyncEnvelope, TeamSyncExportInput, TeamUserInput, DEFAULT_CONTEXT_MAX_ITEMS,
    DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
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
        if name.starts_with("mneme_v2_") && !self.config.mode.includes_team() {
            return Err(McpError::invalid_request(
                "v2 tools are disabled for this server mode",
            ));
        }
        match name {
            "mneme_mcp_status" => Ok(self.mcp_status()),
            "mneme_v1_ingest" => self.v1_ingest(arguments, false, "ingest"),
            "mneme_v1_remember" => self.v1_ingest(arguments, true, "remember"),
            "mneme_v1_context" => self.v1_context(arguments),
            "mneme_v1_begin" => self.v1_begin(arguments),
            "mneme_v1_end" => self.v1_end(arguments),
            "mneme_v1_continuity_begin" => self.v1_continuity_begin(arguments),
            "mneme_v1_continuity_end" => self.v1_continuity_end(arguments),
            "mneme_v1_continuity_handoff" => self.v1_continuity_handoff(arguments),
            "mneme_v1_forget" => self.v1_lifecycle(arguments, "forget"),
            "mneme_v1_correct" => self.v1_lifecycle(arguments, "correct"),
            "mneme_v1_quality" => self.v1_quality(),
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
            "v1": v1,
            "v2": v2,
            "continuity_contract": {
                "mcp_accessible": true,
                "begin_required": true,
                "end_write_back_required": true,
                "read_and_honor_required": true,
                "shared_scope_or_lineage_required": true,
                "sequential_handoff_required": true,
            }
        })
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
            "item_count": context_pack.items.len(),
            "omitted_count": context_pack.omitted.len(),
            "context_pack": context_pack,
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
        });
        self.persist_v1(&engine)?;
        Ok(json!({
            "command": "v1.begin",
            "store": self.config.v1_store.display().to_string(),
            "session_id": report.session.id,
            "report": report,
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
            "context_item_count": report.context_pack.items.len(),
            "omitted_count": report.context_pack.omitted.len(),
            "report": report,
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
            "source_session_count": source_sessions.len(),
            "context_item_count": context_pack.items.len(),
            "omitted_count": context_pack.omitted.len(),
            "source_sessions": source_sessions,
            "context_pack": context_pack,
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
            "item_count": context_pack.items.len(),
            "omitted_count": context_pack.omitted.len(),
            "context_pack": context_pack,
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
            "context_item_count": package.context_pack.items.len(),
            "sync_memory_count": package.sync_envelope.memories.len(),
            "firewall_ok": package.firewall.ok,
            "package": package,
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
            "report": report,
            "validation": validate_team_state(&engine.state()),
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
            "context_item_count": package.context_pack.items.len(),
            "sync_memory_count": package.sync_envelope.memories.len(),
            "firewall_ok": package.firewall.ok,
            "package": package,
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
        .map(|(key, value)| (key.to_owned(), value))
        .collect::<Map<_, _>>();
    json!({
        "type": "object",
        "required": required,
        "properties": props,
    })
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

fn global_tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "mneme_mcp_status",
        description:
            "Check Mneme MCP installation, store paths, tool inventory, and continuity contract.",
        input_schema: object_schema(&[], Vec::new()),
    }]
}

fn personal_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "mneme_v1_remember",
            description: "Store one explicit v1 personal-memory claim.",
            input_schema: object_schema(
                &["text"],
                vec![
                    ("text", string_schema()),
                    ("scope", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v1_ingest",
            description: "Append one raw v1 event for evals or advanced adapters.",
            input_schema: object_schema(
                &["text"],
                vec![
                    ("text", string_schema()),
                    ("speaker", string_schema()),
                    ("scope", string_schema()),
                    ("trust", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v1_context",
            description: "Read a scoped v1 context pack.",
            input_schema: object_schema(
                &["query"],
                vec![
                    ("query", string_schema()),
                    ("scopes", string_array_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v1_begin",
            description: "Begin a v1 agent session and return task-scoped context.",
            input_schema: object_schema(
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
        },
        ToolDefinition {
            name: "mneme_v1_end",
            description: "End a v1 agent session and record optional memories.",
            input_schema: object_schema(
                &["session_id"],
                vec![
                    ("session_id", string_schema()),
                    ("scope", string_schema()),
                    ("summary", string_schema()),
                    ("remember", string_array_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v1_continuity_begin",
            description:
                "Begin a v1 continuity session with explicit lineage/scope read discipline.",
            input_schema: object_schema(
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
        },
        ToolDefinition {
            name: "mneme_v1_continuity_end",
            description:
                "End a v1 continuity session and write back memory into the shared lineage/scope.",
            input_schema: object_schema(
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
        },
        ToolDefinition {
            name: "mneme_v1_continuity_handoff",
            description: "Build a v1 continuity handoff package for another agent/session.",
            input_schema: object_schema(
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
        },
        ToolDefinition {
            name: "mneme_v1_forget",
            description: "Forget v1 memory by claim id or text.",
            input_schema: object_schema(
                &[],
                vec![
                    ("claim_id", string_schema()),
                    ("text", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v1_correct",
            description: "Correct v1 memory by claim id or old/new text.",
            input_schema: object_schema(
                &["new_text"],
                vec![
                    ("claim_id", string_schema()),
                    ("old_text", string_schema()),
                    ("new_text", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v1_quality",
            description: "Summarize v1 personal-memory quality counters.",
            input_schema: object_schema(&[], Vec::new()),
        },
        ToolDefinition {
            name: "mneme_v1_validate",
            description: "Validate the v1 JSON store.",
            input_schema: object_schema(&[], Vec::new()),
        },
        ToolDefinition {
            name: "mneme_v1_snapshot",
            description: "Return the v1 store snapshot.",
            input_schema: object_schema(&[], Vec::new()),
        },
    ]
}

fn team_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "mneme_v2_team_init",
            description: "Initialize a v2 team store.",
            input_schema: object_schema(
                &[],
                vec![("workspace", string_schema()), ("admin", string_schema())],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_user_add",
            description: "Add or update a v2 team user.",
            input_schema: object_schema(
                &["user", "role"],
                vec![("user", string_schema()), ("role", string_schema())],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_agent_add",
            description: "Add or update a v2 team agent.",
            input_schema: object_schema(
                &["agent", "owner"],
                vec![("agent", string_schema()), ("owner", string_schema())],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_project_add",
            description: "Add or update a v2 team project.",
            input_schema: object_schema(
                &["project"],
                vec![
                    ("project", string_schema()),
                    ("members", string_array_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_project_grant",
            description: "Grant one user access to a v2 project.",
            input_schema: object_schema(
                &["project", "user"],
                vec![("project", string_schema()), ("user", string_schema())],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_team_remember",
            description: "Write scoped v2 team memory through policy.",
            input_schema: object_schema(
                &["text", "actor", "scope"],
                vec![
                    ("text", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("scope", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_team_context",
            description: "Read a policy-filtered v2 team context pack.",
            input_schema: object_schema(
                &["query", "actor"],
                vec![
                    ("query", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_team_handoff",
            description: "Build a policy-filtered v2 handoff package.",
            input_schema: object_schema(
                &["query", "actor"],
                vec![
                    ("query", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_run_begin",
            description: "Open a v2 team task run.",
            input_schema: object_schema(
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
        },
        ToolDefinition {
            name: "mneme_v2_run_note",
            description: "Attach scoped memory to an open v2 run.",
            input_schema: object_schema(
                &["run_id", "text", "actor", "scope"],
                vec![
                    ("run_id", string_schema()),
                    ("text", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("scope", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_run_end",
            description: "Close a v2 team task run.",
            input_schema: object_schema(
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
        },
        ToolDefinition {
            name: "mneme_v2_run_handoff",
            description: "Build a policy-filtered v2 handoff package for one run.",
            input_schema: object_schema(
                &["run_id", "actor"],
                vec![
                    ("run_id", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("query", string_schema()),
                    ("max_items", int_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_promote",
            description: "Create a v2 team-memory promotion candidate.",
            input_schema: object_schema(
                &["memory_id", "actor"],
                vec![
                    ("memory_id", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("note", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_promotion_report",
            description: "Inspect v2 promotion quality and reviewer risk.",
            input_schema: object_schema(&["promotion_id"], vec![("promotion_id", string_schema())]),
        },
        ToolDefinition {
            name: "mneme_v2_review",
            description: "Approve or reject a v2 promotion candidate.",
            input_schema: object_schema(
                &["promotion_id", "actor", "approve"],
                vec![
                    ("promotion_id", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("approve", bool_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_sync_export",
            description: "Export a connector-safe v2 sync envelope.",
            input_schema: object_schema(
                &["actor"],
                vec![
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                    ("include_projects", bool_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_sync_import",
            description: "Dry-run or apply a connector sync envelope.",
            input_schema: object_schema(
                &["envelope"],
                vec![
                    ("envelope", json!({"type": "object"})),
                    ("apply", bool_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_firewall",
            description: "Scan v2 team memory for leakage and poisoning risk.",
            input_schema: object_schema(&[], Vec::new()),
        },
        ToolDefinition {
            name: "mneme_v2_quality",
            description: "Analyze v2 duplicates, conflicts, stale candidates, and run state.",
            input_schema: object_schema(&[], Vec::new()),
        },
        ToolDefinition {
            name: "mneme_v2_ontology",
            description: "Return v2 entity, relation, and attribute projection.",
            input_schema: object_schema(
                &[],
                vec![("actor", string_schema()), ("agent", string_schema())],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_revoke_user",
            description: "Revoke a v2 user through an admin actor.",
            input_schema: object_schema(
                &["user", "actor"],
                vec![
                    ("user", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_revoke_agent",
            description: "Revoke a v2 agent through an admin actor.",
            input_schema: object_schema(
                &["target_agent", "actor"],
                vec![
                    ("target_agent", string_schema()),
                    ("actor", string_schema()),
                    ("agent", string_schema()),
                ],
            ),
        },
        ToolDefinition {
            name: "mneme_v2_validate",
            description: "Validate the v2 team JSON store.",
            input_schema: object_schema(&[], Vec::new()),
        },
        ToolDefinition {
            name: "mneme_v2_snapshot",
            description: "Return the v2 team store snapshot.",
            input_schema: object_schema(&[], Vec::new()),
        },
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
            .all(|tool| tool.name == "mneme_mcp_status" || tool.name.starts_with("mneme_v1_")));
        assert!(team
            .tools()
            .iter()
            .all(|tool| tool.name == "mneme_mcp_status" || tool.name.starts_with("mneme_v2_")));
        assert_eq!(
            personal.tools().len() + team.tools().len() - global_tools().len(),
            all.tools().len()
        );
    }
}
