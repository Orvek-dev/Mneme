//! Local developer CLI for the Mneme v1 personal-memory core.
//!
//! This crate exposes the `mneme` binary and a small library entry point for
//! embedding the same CLI parser in tests or local tooling. Product integrations
//! should prefer `mneme-core` APIs when they need direct engine access; use
//! [`run_cli`] when the command-line contract is the desired boundary.

use std::env;
use std::fmt::{Display, Formatter};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use mneme_core::{
    validate_state, BuildStage, ClaimRecord, CommandExtractor, CompactionReport, ContextPack,
    EngineSnapshot, EventInput, ExtractorError, JsonFileStore, MnemeConfig, MnemeEngine,
    MnemeExtractor, MnemeState, MnemeStore, RuleBasedExtractor, SessionBeginInput,
    SessionBeginReport, SessionEndInput, SessionEndReport, SessionError, StateValidationReport,
    StoreFileStatus, StoreInspection, StoreRepairReport, PRODUCT_NAME,
};
use serde::Serialize;

/// Error type returned by the Mneme local CLI.
#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: i32,
}

impl CliError {
    fn invalid_cli(message: impl Into<String>) -> Self {
        Self {
            message: format_invalid_cli_message(message.into()),
            exit_code: 2,
        }
    }

    fn io(action: &str, path: &Path, source: io::Error) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    fn store(action: &str, path: &Path, source: impl Display) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    fn json(source: serde_json::Error) -> Self {
        Self {
            message: format!("serialize CLI output: {source}"),
            exit_code: 1,
        }
    }

    fn json_file(action: &str, path: &Path, source: serde_json::Error) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
        }
    }

    fn extractor(source: ExtractorError) -> Self {
        Self {
            message: format!("extract memory claim: {source}"),
            exit_code: 1,
        }
    }

    fn session(source: SessionError) -> Self {
        Self {
            message: format!("agent session: {source}"),
            exit_code: 1,
        }
    }

    #[must_use]
    /// Process exit code that matches the error category.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}

impl Display for CliError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CliError {}

/// Runs the Mneme local command-line interface.
pub fn run_cli(args: impl IntoIterator<Item = String>) -> Result<(), CliError> {
    let mut stdout = io::stdout().lock();
    run_cli_with_writer(args, &mut stdout)
}

fn run_cli_with_writer(
    args: impl IntoIterator<Item = String>,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let mut args = args.into_iter();
    let _program = args.next();
    let Some(command) = args.next() else {
        print_doctor(writer)?;
        return Ok(());
    };
    let raw_args = args.collect::<Vec<_>>();
    match command.as_str() {
        "help" => run_help(raw_args, writer),
        "--help" | "-h" => print_help(None, writer),
        "doctor" => {
            if wants_command_help(&raw_args) {
                print_help(Some("doctor"), writer)
            } else {
                print_doctor(writer)
            }
        }
        "--version" | "version" => {
            if wants_command_help(&raw_args) {
                print_help(Some("version"), writer)
            } else {
                writeln!(writer, "{}", env!("CARGO_PKG_VERSION"))
                    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
            }
        }
        "ingest" => run_command_or_help("ingest", raw_args, writer, run_ingest),
        "remember" => run_command_or_help("remember", raw_args, writer, run_remember),
        "correct" => run_command_or_help("correct", raw_args, writer, run_correct),
        "forget" => run_command_or_help("forget", raw_args, writer, run_forget),
        "context" => run_command_or_help("context", raw_args, writer, run_context),
        "snapshot" => run_command_or_help("snapshot", raw_args, writer, run_snapshot),
        "begin" => run_command_or_help("begin", raw_args, writer, run_begin),
        "end" => run_command_or_help("end", raw_args, writer, run_end),
        "validate" => run_command_or_help("validate", raw_args, writer, run_validate_store),
        "export" => run_command_or_help("export", raw_args, writer, run_export),
        "import" => run_command_or_help("import", raw_args, writer, run_import),
        "compact" => run_command_or_help("compact", raw_args, writer, run_compact),
        "repair" => run_command_or_help("repair", raw_args, writer, run_repair),
        _ => Err(CliError::invalid_cli(format!(
            "unknown mneme command: {command}\navailable commands: doctor, version, ingest, remember, correct, forget, context, snapshot, begin, end, validate, export, import, compact, repair"
        ))),
    }
}

fn format_invalid_cli_message(message: String) -> String {
    if message.contains("Run `mneme help") {
        message
    } else {
        format!("{message}\nRun `mneme help` or `mneme help <command>` for usage.")
    }
}

fn run_command_or_help<W, F>(
    command: &'static str,
    raw_args: Vec<String>,
    writer: &mut W,
    run: F,
) -> Result<(), CliError>
where
    W: Write,
    F: FnOnce(Vec<String>, &mut W) -> Result<(), CliError>,
{
    if wants_command_help(&raw_args) {
        print_help(Some(command), writer)
    } else {
        run(raw_args, writer)
    }
}

fn wants_command_help(raw_args: &[String]) -> bool {
    raw_args.len() == 1 && matches!(raw_args[0].as_str(), "--help" | "-h")
}

fn run_help(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    match raw_args.as_slice() {
        [] => print_help(None, writer),
        [command] => print_help(Some(command), writer),
        _ => Err(CliError::invalid_cli(
            "usage: mneme help [command]\nexample: mneme help begin",
        )),
    }
}

fn print_help(command: Option<&str>, writer: &mut impl Write) -> Result<(), CliError> {
    let text = match command {
        None => MNEME_HELP,
        Some(command) => command_help(command).ok_or_else(|| {
            CliError::invalid_cli(format!(
                "unknown mneme help topic: {command}\navailable help topics: doctor, version, ingest, remember, correct, forget, context, snapshot, begin, end, validate, export, import, compact, repair"
            ))
        })?,
    };
    writeln!(writer, "{text}")
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "doctor" => Some(MNEME_DOCTOR_HELP),
        "version" | "--version" => Some(MNEME_VERSION_HELP),
        "ingest" => Some(MNEME_INGEST_HELP),
        "remember" => Some(MNEME_REMEMBER_HELP),
        "correct" => Some(MNEME_CORRECT_HELP),
        "forget" => Some(MNEME_FORGET_HELP),
        "context" => Some(MNEME_CONTEXT_HELP),
        "snapshot" => Some(MNEME_SNAPSHOT_HELP),
        "begin" => Some(MNEME_BEGIN_HELP),
        "end" => Some(MNEME_END_HELP),
        "validate" => Some(MNEME_VALIDATE_HELP),
        "export" => Some(MNEME_EXPORT_HELP),
        "import" => Some(MNEME_IMPORT_HELP),
        "compact" => Some(MNEME_COMPACT_HELP),
        "repair" => Some(MNEME_REPAIR_HELP),
        _ => None,
    }
}

const MNEME_HELP: &str = r#"Mneme local CLI

Usage:
  mneme <command> [options]
  mneme help [command]

Commands:
  doctor      Show local CLI and default store information.
  version     Print the CLI version.
  ingest      Ingest one event, optionally through a command extractor.
  remember    Save an explicit memory claim.
  correct     Supersede one claim with another claim.
  forget      Mark a claim as forgotten.
  context     Build a cited context pack for a query.
  snapshot    Print the current store snapshot.
  begin       Start an agent task session and retrieve context.
  end         Close an agent task session and optionally remember claims.
  validate    Inspect the current store and backup.
  export      Export the current store state to JSON.
  import      Import a store state from JSON.
  compact     Remove inactive claims and unreferenced events.
  repair      Restore the current store from its backup when possible.

Common options:
  --store <path>  Use an isolated JSON store.
  --json          Print JSON output.

Examples:
  mneme remember "user prefers local-first tools" --store /tmp/mneme.json
  mneme context "local-first" --store /tmp/mneme.json --json
  mneme help begin"#;

const MNEME_DOCTOR_HELP: &str = r#"Usage: mneme doctor

Show local CLI build stage and the default store path."#;

const MNEME_VERSION_HELP: &str = r#"Usage: mneme version

Print the CLI version."#;

const MNEME_INGEST_HELP: &str = r#"Usage: mneme ingest <text> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--extractor rule|command] [--extractor-command <program>] [--extractor-arg <arg>]... [--json]

Ingest one event. Use the default rule extractor unless --extractor command is
selected.

Example:
  mneme ingest "the user prefers local-first tools" --store /tmp/mneme.json"#;

const MNEME_REMEMBER_HELP: &str = r#"Usage: mneme remember <claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]

Save an explicit durable memory claim.

Example:
  mneme remember "user prefers local-first tools" --store /tmp/mneme.json"#;

const MNEME_CORRECT_HELP: &str = r#"Usage: mneme correct <old-claim> <new-claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]

Supersede an existing claim with a replacement claim.

Example:
  mneme correct "user prefers local-first tools" "user prefers desktop IDE" --store /tmp/mneme.json"#;

const MNEME_FORGET_HELP: &str = r#"Usage: mneme forget <claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]

Mark matching active claims as forgotten.

Example:
  mneme forget "user prefers desktop IDE" --store /tmp/mneme.json"#;

const MNEME_CONTEXT_HELP: &str = r#"Usage: mneme context <query> [--store <path>] [--json]

Build a cited context pack for a query.

Example:
  mneme context "local-first" --store /tmp/mneme.json --json"#;

const MNEME_SNAPSHOT_HELP: &str = r#"Usage: mneme snapshot [--store <path>] [--json]

Print the current store snapshot.

Example:
  mneme snapshot --store /tmp/mneme.json --json"#;

const MNEME_BEGIN_HELP: &str = r#"Usage: mneme begin <task> [--query <query>] [--agent <id>] [--store <path>] [--json]

Start an agent task session and retrieve task-scoped context.

Example:
  mneme begin "Draft setup plan" --query "local-first" --agent codex --store /tmp/mneme.json --json"#;

const MNEME_END_HELP: &str = r#"Usage: mneme end <session-id> [--summary <text>] [--remember <claim>]... [--agent <id>] [--store <path>] [--json]

Close an agent task session and optionally write explicit memory claims.

Example:
  mneme end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store /tmp/mneme.json --json"#;

const MNEME_VALIDATE_HELP: &str = r#"Usage: mneme validate [--store <path>] [--json]

Inspect the current store and backup.

Example:
  mneme validate --store /tmp/mneme.json"#;

const MNEME_EXPORT_HELP: &str = r#"Usage: mneme export <path> [--store <path>] [--json]

Export the current store state to JSON.

Example:
  mneme export /tmp/mneme-export.json --store /tmp/mneme.json"#;

const MNEME_IMPORT_HELP: &str = r#"Usage: mneme import <path> [--store <path>] [--json]

Import a validated store state from JSON.

Example:
  mneme import /tmp/mneme-export.json --store /tmp/mneme-imported.json"#;

const MNEME_COMPACT_HELP: &str = r#"Usage: mneme compact [--store <path>] [--json]

Remove inactive claims and unreferenced events.

Example:
  mneme compact --store /tmp/mneme.json"#;

const MNEME_REPAIR_HELP: &str = r#"Usage: mneme repair [--store <path>] [--json]

Restore the current store from its backup when possible.

Example:
  mneme repair --store /tmp/mneme.json"#;

fn print_doctor(writer: &mut impl Write) -> Result<(), CliError> {
    writeln!(
        writer,
        "{PRODUCT_NAME} local CLI: {}",
        BuildStage::PersonalCoreV1.as_str()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(writer, "default store: {}", default_store_path()?.display())
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

#[derive(Debug, Clone, Default)]
struct CommonOptions {
    json: bool,
    store_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct EventOptions {
    common: CommonOptions,
    speaker_id: String,
    actor_agent_id: Option<String>,
    scope: String,
    trust_level: String,
    extractor: ExtractorOptions,
}

impl Default for EventOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            speaker_id: "user".to_owned(),
            actor_agent_id: None,
            scope: "private".to_owned(),
            trust_level: "trusted_user".to_owned(),
            extractor: ExtractorOptions::Rule,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct BeginOptions {
    common: CommonOptions,
    actor_agent_id: Option<String>,
    query: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct EndOptions {
    common: CommonOptions,
    actor_agent_id: Option<String>,
    summary: Option<String>,
    remember: Vec<String>,
}

#[derive(Debug, Clone)]
enum ExtractorOptions {
    Rule,
    Command {
        program: Option<String>,
        args: Vec<String>,
    },
}

impl ExtractorOptions {
    fn name(&self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Command { .. } => "command",
        }
    }
}

#[derive(Debug, Serialize)]
struct EventCommandReport {
    command: String,
    store: String,
    extractor: String,
    event_count: usize,
    claim_count: usize,
    latest_claim: Option<ClaimSummary>,
}

#[derive(Debug, Serialize)]
struct ClaimSummary {
    id: String,
    subject: String,
    predicate: String,
    object: String,
    status: String,
    scope: String,
    source_event_ids: Vec<String>,
}

impl From<&ClaimRecord> for ClaimSummary {
    fn from(claim: &ClaimRecord) -> Self {
        Self {
            id: claim.id.clone(),
            subject: claim.subject.clone(),
            predicate: claim.predicate.clone(),
            object: claim.object.clone(),
            status: claim.status.as_str().to_owned(),
            scope: claim.scope.clone(),
            source_event_ids: claim.source_event_ids.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ContextReport {
    store: String,
    item_count: usize,
    omitted_count: usize,
    context_pack: ContextPack,
}

#[derive(Debug, Serialize)]
struct SnapshotReport {
    store: String,
    snapshot: EngineSnapshot,
}

#[derive(Debug, Serialize)]
struct BeginCliReport {
    store: String,
    report: SessionBeginReport,
}

#[derive(Debug, Serialize)]
struct EndCliReport {
    store: String,
    report: SessionEndReport,
}

#[derive(Debug, Serialize)]
struct StoreValidationCliReport {
    store: String,
    inspection: StoreInspection,
}

#[derive(Debug, Serialize)]
struct ExportReport {
    command: String,
    store: String,
    path: String,
    schema_version: u32,
    generation: u64,
    event_count: usize,
    claim_count: usize,
}

#[derive(Debug, Serialize)]
struct ImportReport {
    command: String,
    store: String,
    path: String,
    validation: StateValidationReport,
    event_count: usize,
    claim_count: usize,
}

#[derive(Debug, Serialize)]
struct CompactReport {
    command: String,
    store: String,
    compaction: CompactionReport,
}

#[derive(Debug, Serialize)]
struct RepairCliReport {
    store: String,
    repair: StoreRepairReport,
}

fn run_ingest(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (text, options) = parse_ingest_args(raw_args)?;
    run_event_command("ingest", text, options, writer)
}

fn run_remember(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (claim, options) = parse_event_args(
        raw_args,
        "usage: mneme remember <claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
    )?;
    run_event_command("remember", format!("remember: {claim}"), options, writer)
}

fn run_correct(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (claims, options) = parse_correct_args(raw_args)?;
    run_event_command(
        "correct",
        format!("correct: {} -> {}", claims.0, claims.1),
        options,
        writer,
    )
}

fn run_forget(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (claim, options) = parse_event_args(
        raw_args,
        "usage: mneme forget <claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
    )?;
    run_event_command("forget", format!("forget: {claim}"), options, writer)
}

fn run_event_command(
    command: &str,
    event_text: String,
    options: EventOptions,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let store_path = resolve_store_path(&options.common)?;
    let extractor_name = options.extractor.name().to_owned();
    let extractor = build_extractor(&options.extractor)?;
    let mut engine = load_engine(&store_path)?;
    engine
        .ingest_event_with_extractor(
            EventInput {
                speaker_id: options.speaker_id,
                actor_agent_id: options.actor_agent_id,
                text: event_text,
                scope: options.scope,
                trust_level: options.trust_level,
            },
            extractor.as_ref(),
        )
        .map_err(CliError::extractor)?;
    persist_engine(&store_path, &engine)?;
    let snapshot = engine.snapshot();
    let report = EventCommandReport {
        command: command.to_owned(),
        store: store_path.display().to_string(),
        extractor: extractor_name,
        event_count: snapshot.events.len(),
        claim_count: snapshot.claims.len(),
        latest_claim: snapshot.claims.last().map(ClaimSummary::from),
    };
    emit_event_report(&report, options.common.json, writer)
}

fn run_context(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (query, options) = parse_query_args(raw_args)?;
    let store_path = resolve_store_path(&options)?;
    let mut engine = load_engine(&store_path)?;
    let context_pack = engine.build_context_pack(query);
    persist_engine(&store_path, &engine)?;
    let report = ContextReport {
        store: store_path.display().to_string(),
        item_count: context_pack.items.len(),
        omitted_count: context_pack.omitted.len(),
        context_pack,
    };
    emit_context_report(&report, options.json, writer)
}

fn run_snapshot(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_snapshot_args(raw_args)?;
    let store_path = resolve_store_path(&options)?;
    let engine = load_engine(&store_path)?;
    let report = SnapshotReport {
        store: store_path.display().to_string(),
        snapshot: engine.snapshot(),
    };
    emit_snapshot_report(&report, options.json, writer)
}

fn run_begin(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (task, options) = parse_begin_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    let report = engine.begin_session(SessionBeginInput {
        task,
        actor_agent_id: options.actor_agent_id,
        query: options.query,
    });
    persist_engine(&store_path, &engine)?;
    let cli_report = BeginCliReport {
        store: store_path.display().to_string(),
        report,
    };
    emit_begin_report(&cli_report, options.common.json, writer)
}

fn run_end(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (session_id, options) = parse_end_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    let report = engine
        .end_session(SessionEndInput {
            session_id,
            actor_agent_id: options.actor_agent_id,
            summary: options.summary,
            remember: options.remember,
        })
        .map_err(CliError::session)?;
    persist_engine(&store_path, &engine)?;
    let cli_report = EndCliReport {
        store: store_path.display().to_string(),
        report,
    };
    emit_end_report(&cli_report, options.common.json, writer)
}

fn run_validate_store(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "validate")?;
    let store_path = resolve_store_path(&options)?;
    let store = JsonFileStore::new(store_path.clone());
    let inspection = store.inspect();
    let report = StoreValidationCliReport {
        store: store_path.display().to_string(),
        inspection,
    };
    emit_store_validation_report(&report, options.json, writer)?;
    if report.inspection.current.status == StoreFileStatus::Valid {
        Ok(())
    } else {
        Err(CliError::store(
            "validate store",
            &store_path,
            "store is not valid",
        ))
    }
}

fn run_export(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (path, options) = parse_path_command_args(
        raw_args,
        "usage: mneme export <path> [--store <path>] [--json]",
    )?;
    let store_path = resolve_store_path(&options)?;
    let store = JsonFileStore::new(store_path.clone());
    let state = store
        .load()
        .map_err(|source| CliError::store("load store", &store_path, source))?
        .ok_or_else(|| CliError::store("load store", &store_path, "store does not exist"))?;
    let validation = validate_state(&state);
    if !validation.ok {
        return Err(CliError::store(
            "export store",
            &store_path,
            "store validation failed",
        ));
    }
    write_state_json(&path, &state)?;
    let report = ExportReport {
        command: "export".to_owned(),
        store: store_path.display().to_string(),
        path: path.display().to_string(),
        schema_version: state.schema_version,
        generation: state.metadata.generation,
        event_count: state.events.len(),
        claim_count: state.claims.len(),
    };
    emit_export_report(&report, options.json, writer)
}

fn run_import(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (path, options) = parse_path_command_args(
        raw_args,
        "usage: mneme import <path> [--store <path>] [--json]",
    )?;
    let store_path = resolve_store_path(&options)?;
    let text =
        std::fs::read_to_string(&path).map_err(|source| CliError::io("read", &path, source))?;
    let state: MnemeState = serde_json::from_str(&text)
        .map_err(|source| CliError::json_file("parse", &path, source))?;
    let validation = validate_state(&state);
    if !validation.ok {
        let report = ImportReport {
            command: "import".to_owned(),
            store: store_path.display().to_string(),
            path: path.display().to_string(),
            validation,
            event_count: state.events.len(),
            claim_count: state.claims.len(),
        };
        emit_import_report(&report, options.json, writer)?;
        return Err(CliError::store(
            "import store",
            &store_path,
            "import validation failed",
        ));
    }
    let mut store = JsonFileStore::new(store_path.clone());
    store
        .save(&state)
        .map_err(|source| CliError::store("save store", &store_path, source))?;
    let report = ImportReport {
        command: "import".to_owned(),
        store: store_path.display().to_string(),
        path: path.display().to_string(),
        validation,
        event_count: state.events.len(),
        claim_count: state.claims.len(),
    };
    emit_import_report(&report, options.json, writer)
}

fn run_compact(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "compact")?;
    let store_path = resolve_store_path(&options)?;
    let mut engine = load_engine(&store_path)?;
    let compaction = engine.compact();
    persist_engine(&store_path, &engine)?;
    let report = CompactReport {
        command: "compact".to_owned(),
        store: store_path.display().to_string(),
        compaction,
    };
    emit_compact_report(&report, options.json, writer)
}

fn run_repair(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "repair")?;
    let store_path = resolve_store_path(&options)?;
    let store = JsonFileStore::new(store_path.clone());
    let repair = store
        .repair_from_backup()
        .map_err(|source| CliError::store("repair store", &store_path, source))?;
    let report = RepairCliReport {
        store: store_path.display().to_string(),
        repair,
    };
    emit_repair_report(&report, options.json, writer)?;
    if report.repair.repaired || report.repair.after.current.status == StoreFileStatus::Valid {
        Ok(())
    } else {
        Err(CliError::store(
            "repair store",
            &store_path,
            "store could not be repaired",
        ))
    }
}

fn parse_event_args(
    raw_args: Vec<String>,
    usage: &'static str,
) -> Result<(String, EventOptions), CliError> {
    let mut options = EventOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--speaker" => {
                idx += 1;
                options.speaker_id = required_arg(&raw_args, idx, "--speaker")?;
            }
            "--agent" => {
                idx += 1;
                options.actor_agent_id = Some(required_arg(&raw_args, idx, "--agent")?);
            }
            "--scope" => {
                idx += 1;
                options.scope = required_arg(&raw_args, idx, "--scope")?;
            }
            "--trust" => {
                idx += 1;
                options.trust_level = required_arg(&raw_args, idx, "--trust")?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown event option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(usage));
    }
    let claim = require_nonempty(positionals.remove(0), "claim")?;
    Ok((claim, options))
}

fn parse_ingest_args(raw_args: Vec<String>) -> Result<(String, EventOptions), CliError> {
    let mut options = EventOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--speaker" => {
                idx += 1;
                options.speaker_id = required_arg(&raw_args, idx, "--speaker")?;
            }
            "--agent" => {
                idx += 1;
                options.actor_agent_id = Some(required_arg(&raw_args, idx, "--agent")?);
            }
            "--scope" => {
                idx += 1;
                options.scope = required_arg(&raw_args, idx, "--scope")?;
            }
            "--trust" => {
                idx += 1;
                options.trust_level = required_arg(&raw_args, idx, "--trust")?;
            }
            "--extractor" => {
                idx += 1;
                options.extractor =
                    parse_extractor_kind(required_arg(&raw_args, idx, "--extractor")?)?;
            }
            "--extractor-command" => {
                idx += 1;
                set_command_program(
                    &mut options.extractor,
                    required_arg(&raw_args, idx, "--extractor-command")?,
                );
            }
            "--extractor-arg" => {
                idx += 1;
                push_command_arg(
                    &mut options.extractor,
                    required_arg(&raw_args, idx, "--extractor-arg")?,
                );
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown ingest option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme ingest <text> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--extractor rule|command] [--extractor-command <program>] [--extractor-arg <arg>]... [--json]",
        ));
    }
    let text = require_nonempty(positionals.remove(0), "text")?;
    Ok((text, options))
}

fn parse_extractor_kind(value: String) -> Result<ExtractorOptions, CliError> {
    match value.as_str() {
        "rule" => Ok(ExtractorOptions::Rule),
        "command" => Ok(ExtractorOptions::Command {
            program: None,
            args: Vec::new(),
        }),
        _ => Err(CliError::invalid_cli(format!(
            "unknown extractor: {value}\navailable extractors: rule, command"
        ))),
    }
}

fn set_command_program(options: &mut ExtractorOptions, program: String) {
    let args = match options {
        ExtractorOptions::Command { args, .. } => std::mem::take(args),
        ExtractorOptions::Rule => Vec::new(),
    };
    *options = ExtractorOptions::Command {
        program: Some(program),
        args,
    };
}

fn push_command_arg(options: &mut ExtractorOptions, arg: String) {
    match options {
        ExtractorOptions::Command { args, .. } => args.push(arg),
        ExtractorOptions::Rule => {
            *options = ExtractorOptions::Command {
                program: None,
                args: vec![arg],
            };
        }
    }
}

fn parse_correct_args(raw_args: Vec<String>) -> Result<((String, String), EventOptions), CliError> {
    let mut options = EventOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--speaker" => {
                idx += 1;
                options.speaker_id = required_arg(&raw_args, idx, "--speaker")?;
            }
            "--agent" => {
                idx += 1;
                options.actor_agent_id = Some(required_arg(&raw_args, idx, "--agent")?);
            }
            "--scope" => {
                idx += 1;
                options.scope = required_arg(&raw_args, idx, "--scope")?;
            }
            "--trust" => {
                idx += 1;
                options.trust_level = required_arg(&raw_args, idx, "--trust")?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown correct option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 2 {
        return Err(CliError::invalid_cli(
            "usage: mneme correct <old-claim> <new-claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
        ));
    }
    let old_claim = require_nonempty(positionals.remove(0), "old claim")?;
    let new_claim = require_nonempty(positionals.remove(0), "new claim")?;
    Ok(((old_claim, new_claim), options))
}

fn parse_query_args(raw_args: Vec<String>) -> Result<(String, CommonOptions), CliError> {
    let mut options = CommonOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown context option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme context <query> [--store <path>] [--json]",
        ));
    }
    let query = require_nonempty(positionals.remove(0), "query")?;
    Ok((query, options))
}

fn parse_snapshot_args(raw_args: Vec<String>) -> Result<CommonOptions, CliError> {
    let mut options = CommonOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options)? {
            idx += 1;
            continue;
        }
        return Err(CliError::invalid_cli(format!(
            "unknown snapshot option: {}",
            raw_args[idx]
        )));
    }
    Ok(options)
}

fn parse_begin_args(raw_args: Vec<String>) -> Result<(String, BeginOptions), CliError> {
    let mut options = BeginOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--agent" => {
                idx += 1;
                options.actor_agent_id = Some(required_arg(&raw_args, idx, "--agent")?);
            }
            "--query" => {
                idx += 1;
                options.query = Some(required_arg(&raw_args, idx, "--query")?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown begin option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme begin <task> [--query <query>] [--agent <id>] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "task")?, options))
}

fn parse_end_args(raw_args: Vec<String>) -> Result<(String, EndOptions), CliError> {
    let mut options = EndOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--agent" => {
                idx += 1;
                options.actor_agent_id = Some(required_arg(&raw_args, idx, "--agent")?);
            }
            "--summary" => {
                idx += 1;
                options.summary = Some(required_arg(&raw_args, idx, "--summary")?);
            }
            "--remember" => {
                idx += 1;
                options
                    .remember
                    .push(required_arg(&raw_args, idx, "--remember")?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown end option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme end <session-id> [--summary <text>] [--remember <claim>]... [--agent <id>] [--store <path>] [--json]",
        ));
    }
    if options.summary.is_none() && options.remember.is_empty() {
        return Err(CliError::invalid_cli(
            "mneme end requires --summary <text> or at least one --remember <claim>",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "session id")?,
        options,
    ))
}

fn parse_no_position_args(
    raw_args: Vec<String>,
    command: &'static str,
) -> Result<CommonOptions, CliError> {
    let mut options = CommonOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options)? {
            idx += 1;
            continue;
        }
        return Err(CliError::invalid_cli(format!(
            "unknown {command} option: {}",
            raw_args[idx]
        )));
    }
    Ok(options)
}

fn parse_path_command_args(
    raw_args: Vec<String>,
    usage: &'static str,
) -> Result<(PathBuf, CommonOptions), CliError> {
    let mut options = CommonOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown path command option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(usage));
    }
    Ok((PathBuf::from(positionals.remove(0)), options))
}

fn parse_common_option(
    raw_args: &[String],
    idx: &mut usize,
    options: &mut CommonOptions,
) -> Result<bool, CliError> {
    match raw_args[*idx].as_str() {
        "--json" => {
            options.json = true;
            Ok(true)
        }
        "--store" => {
            *idx += 1;
            options.store_path = Some(PathBuf::from(required_arg(raw_args, *idx, "--store")?));
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn required_arg(raw_args: &[String], idx: usize, option: &str) -> Result<String, CliError> {
    raw_args
        .get(idx)
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .ok_or_else(|| CliError::invalid_cli(format!("{option} requires a value")))
}

fn require_nonempty(value: String, label: &str) -> Result<String, CliError> {
    if value.trim().is_empty() {
        Err(CliError::invalid_cli(format!("{label} must not be empty")))
    } else {
        Ok(value)
    }
}

fn resolve_store_path(options: &CommonOptions) -> Result<PathBuf, CliError> {
    match &options.store_path {
        Some(path) => Ok(path.clone()),
        None => default_store_path(),
    }
}

fn build_extractor(options: &ExtractorOptions) -> Result<Box<dyn MnemeExtractor>, CliError> {
    match options {
        ExtractorOptions::Rule => Ok(Box::new(RuleBasedExtractor::new())),
        ExtractorOptions::Command { program, args } => {
            let program = match program {
                Some(program) => program.clone(),
                None => env::var("MNEME_EXTRACTOR_COMMAND")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| {
                        CliError::invalid_cli(
                            "command extractor requires --extractor-command <program> or MNEME_EXTRACTOR_COMMAND",
                        )
                    })?,
            };
            Ok(Box::new(CommandExtractor::new(program, args.clone())))
        }
    }
}

fn default_store_path() -> Result<PathBuf, CliError> {
    env::current_dir()
        .map(|dir| dir.join(".mneme").join("mneme-v1.json"))
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))
}

fn load_engine(path: &Path) -> Result<MnemeEngine, CliError> {
    let store = JsonFileStore::new(path.to_path_buf());
    MnemeEngine::from_store(MnemeConfig::default(), &store)
        .map_err(|source| CliError::store("load store", path, source))
}

fn persist_engine(path: &Path, engine: &MnemeEngine) -> Result<(), CliError> {
    let mut store = JsonFileStore::new(path.to_path_buf());
    engine
        .persist(&mut store)
        .map_err(|source| CliError::store("save store", path, source))
}

fn write_state_json(path: &Path, state: &MnemeState) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create dir", parent, source))?;
    }
    let json = serde_json::to_string_pretty(state).map_err(CliError::json)?;
    std::fs::write(path, format!("{json}\n")).map_err(|source| CliError::io("write", path, source))
}

fn emit_event_report(
    report: &EventCommandReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: {} saved to {} (events={}, claims={}, extractor={})",
        report.command, report.store, report.event_count, report.claim_count, report.extractor
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_context_report(
    report: &ContextReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: context from {} (items={}, omitted={})",
        report.store, report.item_count, report.omitted_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for item in &report.context_pack.items {
        writeln!(
            writer,
            "- {} [{}]",
            item.claim_text,
            item.source_event_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_snapshot_report(
    report: &SnapshotReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: snapshot from {} (events={}, claims={}, audit={})",
        report.store,
        report.snapshot.events.len(),
        report.snapshot.claims.len(),
        report.snapshot.audit.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_begin_report(
    report: &BeginCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: began session {} from {} (context_items={})",
        report.report.session.id,
        report.store,
        report.report.context_pack.items.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for item in &report.report.context_pack.items {
        writeln!(
            writer,
            "- {} [{}]",
            item.claim_text,
            item.source_event_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_end_report(
    report: &EndCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: ended session {} from {} (remembered_events={}, remembered_claims={})",
        report.report.session.id,
        report.store,
        report.report.remembered_event_ids.len(),
        report.report.remembered_claim_ids.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_store_validation_report(
    report: &StoreValidationCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: validate {} (current={:?}, backup={:?}, repair_available={})",
        report.store,
        report.inspection.current.status,
        report.inspection.backup.status,
        report.inspection.repair_available
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_export_report(
    report: &ExportReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: exported {} to {} (events={}, claims={}, generation={})",
        report.store, report.path, report.event_count, report.claim_count, report.generation
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_import_report(
    report: &ImportReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: imported {} into {} (events={}, claims={}, validation_ok={})",
        report.path, report.store, report.event_count, report.claim_count, report.validation.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_compact_report(
    report: &CompactReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: compacted {} (events {}->{}, claims {}->{})",
        report.store,
        report.compaction.events_before,
        report.compaction.events_after,
        report.compaction.claims_before,
        report.compaction.claims_after
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_repair_report(
    report: &RepairCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: repair {} (action={}, repaired={})",
        report.store, report.repair.action, report.repair.repaired
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn write_json<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(value).map_err(CliError::json)?;
    writeln!(writer, "{json}")
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_lists_commands_and_command_usage() -> Result<(), Box<dyn std::error::Error>> {
        let mut output = Vec::new();
        run_cli_with_writer(vec!["mneme".to_owned(), "help".to_owned()], &mut output)?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("Usage:"));
        assert!(text.contains("mneme help begin"));

        let mut command_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "begin".to_owned(), "--help".to_owned()],
            &mut command_output,
        )?;
        let command_text = String::from_utf8(command_output)?;
        assert!(command_text.contains("Usage: mneme begin <task>"));
        assert!(command_text.contains("--query <query>"));
        Ok(())
    }

    #[test]
    fn invalid_command_points_to_help() {
        let result = run_cli_with_writer(
            vec!["mneme".to_owned(), "unknown".to_owned()],
            &mut Vec::new(),
        );
        let error = result.expect_err("unknown command should fail");
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("mneme help"));
    }

    #[test]
    fn remember_and_context_round_trip_with_json_store() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("remember-context");
        let _ = std::fs::remove_file(&path);

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers local-first tools".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "local-first".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("local-first tools"));
        assert!(text.contains("event-001"));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn correct_and_forget_are_persisted() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("correct-forget");
        let _ = std::fs::remove_file(&path);

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers local-first tools".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "user prefers local-first tools".to_owned(),
                "user prefers desktop IDE".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "forget".to_owned(),
                "user prefers desktop IDE".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "snapshot".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"status\": \"superseded\""));
        assert!(text.contains("\"status\": \"forgotten\""));
        assert!(text.contains("desktop IDE"));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn ingest_can_use_command_extractor() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("command-extractor");
        let _ = std::fs::remove_file(&path);
        let response = serde_json::to_string(&mneme_core::ExtractorCommandResponse::from_claim(
            mneme_core::ExtractedClaim::new("user", "prefers", "command-backed extraction"),
        ))?;
        let script = format!("cat >/dev/null; printf '%s\\n' '{}'", response);

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "ingest".to_owned(),
                "the user likes model-backed extraction".to_owned(),
                "--extractor".to_owned(),
                "command".to_owned(),
                "--extractor-command".to_owned(),
                "/bin/sh".to_owned(),
                "--extractor-arg".to_owned(),
                "-c".to_owned(),
                "--extractor-arg".to_owned(),
                script,
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let ingest_text = String::from_utf8(output)?;
        assert!(ingest_text.contains("\"extractor\": \"command\""));

        let mut context_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "command-backed".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut context_output,
        )?;
        let context_text = String::from_utf8(context_output)?;
        assert!(context_text.contains("command-backed extraction"));
        assert!(context_text.contains("event-001"));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn command_extractor_requires_program() -> Result<(), Box<dyn std::error::Error>> {
        let mut output = Vec::new();
        let result = run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "ingest".to_owned(),
                "the user likes model-backed extraction".to_owned(),
                "--extractor".to_owned(),
                "command".to_owned(),
            ],
            &mut output,
        );

        match result {
            Ok(()) => Err("command extractor without program should fail".into()),
            Err(error) => {
                assert_eq!(error.exit_code(), 2);
                assert!(error.to_string().contains("--extractor-command"));
                Ok(())
            }
        }
    }

    #[test]
    fn export_import_validate_and_compact_store() -> Result<(), Box<dyn std::error::Error>> {
        let source = temp_store_path("export-import-source");
        let target = temp_store_path("export-import-target");
        let export_path = temp_store_path("export-import-export");
        for path in [&source, &target, &export_path] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        }

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers local-first tools".to_owned(),
                "--store".to_owned(),
                source.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "user prefers local-first tools".to_owned(),
                "user prefers desktop IDE".to_owned(),
                "--store".to_owned(),
                source.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "validate".to_owned(),
                "--store".to_owned(),
                source.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "compact".to_owned(),
                "--store".to_owned(),
                source.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "export".to_owned(),
                export_path.display().to_string(),
                "--store".to_owned(),
                source.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "import".to_owned(),
                export_path.display().to_string(),
                "--store".to_owned(),
                target.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "desktop".to_owned(),
                "--store".to_owned(),
                target.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("desktop IDE"));
        assert!(!text.contains("local-first tools"));

        for path in [&source, &target, &export_path] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        }
        Ok(())
    }

    #[test]
    fn repair_command_restores_corrupted_store_from_backup(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("repair-command");
        let backup = PathBuf::from(format!("{}.bak", path.display()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers recoverable memory".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers backups".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        std::fs::write(&path, "{not-json")?;

        let mut repair_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "repair".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut repair_output,
        )?;
        let repair_text = String::from_utf8(repair_output)?;
        assert!(repair_text.contains("\"repaired\": true"));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "validate".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup);
        Ok(())
    }

    #[test]
    fn begin_and_end_agent_session_records_memory() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("begin-end-session");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers local-first tools".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut begin_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "begin".to_owned(),
                "Draft setup plan".to_owned(),
                "--query".to_owned(),
                "local-first".to_owned(),
                "--agent".to_owned(),
                "codex".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut begin_output,
        )?;
        let begin_text = String::from_utf8(begin_output)?;
        assert!(begin_text.contains("\"id\": \"session-001\""));
        assert!(begin_text.contains("local-first tools"));

        let mut end_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "end".to_owned(),
                "session-001".to_owned(),
                "--summary".to_owned(),
                "Prepared a concise setup plan".to_owned(),
                "--remember".to_owned(),
                "user prefers concise setup plans".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut end_output,
        )?;
        let end_text = String::from_utf8(end_output)?;
        assert!(end_text.contains("\"status\": \"closed\""));
        assert!(end_text.contains("event-002"));
        assert!(end_text.contains("claim-002"));

        let mut context_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "concise".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut context_output,
        )?;
        let context_text = String::from_utf8(context_output)?;
        assert!(context_text.contains("concise setup plans"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        Ok(())
    }

    fn temp_store_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("mneme-cli-{name}-{}.json", std::process::id()))
    }
}
