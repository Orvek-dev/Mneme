//! Local developer CLI for the Mneme v1 personal-memory core.
//!
//! This crate exposes the `mneme` binary and a small library entry point for
//! embedding the same CLI parser in tests or local tooling. Product integrations
//! should prefer `mneme-core` APIs when they need direct engine access; use
//! [`run_cli`] when the command-line contract is the desired boundary.

use std::collections::BTreeMap;
use std::env;
use std::fmt::{Display, Formatter};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use mneme_core::{
    validate_state, BuildStage, ClaimRecord, ClaimStatus, CommandExtractor, CompactionReport,
    ContextPack, ContextQuery, EngineSnapshot, EventInput, ExtractorError, JsonFileStore,
    MnemeConfig, MnemeEngine, MnemeExtractor, MnemeState, MnemeStore, RuleBasedExtractor,
    SessionBeginInput, SessionBeginReport, SessionEndInput, SessionEndReport, SessionError,
    SessionRecord, StateValidationReport, StoreError, StoreErrorKind, StoreFileStatus,
    StoreInspection, StoreRepairReport, DEFAULT_CONTEXT_MAX_ITEMS, PRODUCT_NAME,
};
use serde::Serialize;

const AGENT_HOOK_SCHEMA_VERSION: &str = "mneme.agent_hook.v1";

/// Error type returned by the Mneme local CLI.
#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: i32,
    kind: CliErrorKind,
    recoverable: bool,
    print_message: bool,
}

impl CliError {
    fn invalid_cli(message: impl Into<String>) -> Self {
        Self {
            message: format_invalid_cli_message(message.into()),
            exit_code: 2,
            kind: CliErrorKind::InvalidCli,
            recoverable: false,
            print_message: true,
        }
    }

    fn io(action: &str, path: &Path, source: io::Error) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
            kind: CliErrorKind::Io,
            recoverable: true,
            print_message: true,
        }
    }

    fn store(action: &str, path: &Path, source: impl Display) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
            kind: CliErrorKind::Store,
            recoverable: true,
            print_message: true,
        }
    }

    fn store_error(action: &str, path: &Path, source: StoreError) -> Self {
        let kind = match source.kind() {
            StoreErrorKind::Store => CliErrorKind::Store,
            StoreErrorKind::LockConflict => CliErrorKind::StoreLock,
        };
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
            kind,
            recoverable: true,
            print_message: true,
        }
    }

    fn json(source: serde_json::Error) -> Self {
        Self {
            message: format!("serialize CLI output: {source}"),
            exit_code: 1,
            kind: CliErrorKind::Json,
            recoverable: false,
            print_message: true,
        }
    }

    fn json_file(action: &str, path: &Path, source: serde_json::Error) -> Self {
        Self {
            message: format!("{action} {}: {source}", path.display()),
            exit_code: 1,
            kind: CliErrorKind::Json,
            recoverable: false,
            print_message: true,
        }
    }

    fn extractor(source: ExtractorError) -> Self {
        Self {
            message: format!("extract memory claim: {source}"),
            exit_code: 1,
            kind: CliErrorKind::Extractor,
            recoverable: true,
            print_message: true,
        }
    }

    fn lifecycle(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
            kind: CliErrorKind::Lifecycle,
            recoverable: false,
            print_message: true,
        }
    }

    fn session(source: SessionError) -> Self {
        Self {
            message: format!("agent session: {source}"),
            exit_code: 1,
            kind: CliErrorKind::Session,
            recoverable: false,
            print_message: true,
        }
    }

    fn reported(exit_code: i32) -> Self {
        Self {
            message: "agent hook error reported".to_owned(),
            exit_code,
            kind: CliErrorKind::Reported,
            recoverable: false,
            print_message: false,
        }
    }

    #[must_use]
    /// Process exit code that matches the error category.
    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    #[must_use]
    /// Whether the CLI entry point should print this error to stderr.
    pub fn should_print(&self) -> bool {
        self.print_message
    }
}

#[derive(Debug, Clone, Copy)]
enum CliErrorKind {
    InvalidCli,
    Io,
    Store,
    StoreLock,
    Json,
    Extractor,
    Lifecycle,
    Session,
    Reported,
}

impl CliErrorKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidCli => "invalid_cli",
            Self::Io => "io",
            Self::Store => "store",
            Self::StoreLock => "store_lock",
            Self::Json => "json",
            Self::Extractor => "extractor",
            Self::Lifecycle => "lifecycle",
            Self::Session => "session",
            Self::Reported => "reported",
        }
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
        "init" => run_command_or_help("init", raw_args, writer, run_init),
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
        "claims" => run_command_or_help("claims", raw_args, writer, run_claims),
        "context" => run_command_or_help("context", raw_args, writer, run_context),
        "snapshot" => run_command_or_help("snapshot", raw_args, writer, run_snapshot),
        "begin" => run_command_or_help("begin", raw_args, writer, run_begin),
        "end" => run_command_or_help("end", raw_args, writer, run_end),
        "hook" => run_command_or_help("hook", raw_args, writer, run_agent_hook),
        "validate" => run_command_or_help("validate", raw_args, writer, run_validate_store),
        "export" => run_command_or_help("export", raw_args, writer, run_export),
        "review" => run_command_or_help("review", raw_args, writer, run_review),
        "import" => run_command_or_help("import", raw_args, writer, run_import),
        "compact" => run_command_or_help("compact", raw_args, writer, run_compact),
        "repair" => run_command_or_help("repair", raw_args, writer, run_repair),
        _ => Err(CliError::invalid_cli(format!(
            "unknown mneme command: {command}\navailable commands: init, doctor, version, ingest, remember, correct, forget, claims, context, snapshot, begin, end, hook, validate, export, review, import, compact, repair"
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
                "unknown mneme help topic: {command}\navailable help topics: init, doctor, version, ingest, remember, correct, forget, claims, context, snapshot, begin, end, hook, validate, export, review, import, compact, repair"
            ))
        })?,
    };
    writeln!(writer, "{text}")
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "init" => Some(MNEME_INIT_HELP),
        "doctor" => Some(MNEME_DOCTOR_HELP),
        "version" | "--version" => Some(MNEME_VERSION_HELP),
        "ingest" => Some(MNEME_INGEST_HELP),
        "remember" => Some(MNEME_REMEMBER_HELP),
        "correct" => Some(MNEME_CORRECT_HELP),
        "forget" => Some(MNEME_FORGET_HELP),
        "claims" => Some(MNEME_CLAIMS_HELP),
        "context" => Some(MNEME_CONTEXT_HELP),
        "snapshot" => Some(MNEME_SNAPSHOT_HELP),
        "begin" => Some(MNEME_BEGIN_HELP),
        "end" => Some(MNEME_END_HELP),
        "hook" => Some(MNEME_HOOK_HELP),
        "validate" => Some(MNEME_VALIDATE_HELP),
        "export" => Some(MNEME_EXPORT_HELP),
        "review" => Some(MNEME_REVIEW_HELP),
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
  init        Initialize a local .mneme store and agent hook profile.
  doctor      Show local CLI and default store information.
  version     Print the CLI version.
  ingest      Ingest one event, optionally through a command extractor.
  remember    Save an explicit memory claim.
  correct     Supersede one claim with another claim.
  forget      Mark a claim as forgotten.
  claims      Review stored memory claims.
  context     Build a cited context pack for a query.
  snapshot    Print the current store snapshot.
  begin       Start an agent task session and retrieve context.
  end         Close an agent task session and optionally remember claims.
  hook        Agent hook JSON contract for begin/end automation.
  validate    Inspect the current store and backup.
  export      Export the current store state to JSON.
  review      Export a human-readable memory review artifact.
  import      Import a store state from JSON.
  compact     Remove inactive claims and unreferenced events.
  repair      Restore the current store from its backup when possible.

Common options:
  --store <path>  Use an isolated JSON store.
  --json          Print JSON output.

Examples:
  mneme init
  mneme remember "user prefers local-first tools" --store /tmp/mneme.json
  mneme claims --status active --store /tmp/mneme.json --json
  mneme context "local-first" --store /tmp/mneme.json --json
  mneme hook begin "Draft setup plan" --query "local-first" --store /tmp/mneme.json
  mneme help begin"#;

const MNEME_INIT_HELP: &str = r#"Usage: mneme init [--store <path>] [--config <path>] [--agent <id>] [--scope <scope>] [--max-items <n>] [--bin <path>] [--no-bin] [--force] [--json]

Initialize a local workspace by creating a valid v1 store and an agent hook
runtime profile. Defaults to .mneme/mneme-v1.json and
.mneme/mneme-agent-hook.env in the current directory.

Examples:
  mneme init
  mneme init --agent codex --scope private --max-items 3
  mneme init --store /tmp/mneme.json --config /tmp/mneme-agent-hook.env --bin /usr/local/bin/mneme --json"#;

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

const MNEME_CORRECT_HELP: &str = r#"Usage:
  mneme correct <old-claim> <new-claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]
  mneme correct --claim-id <id> <new-claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]

Supersede an existing claim with a replacement claim.

Example:
  mneme correct "user prefers local-first tools" "user prefers desktop IDE" --store /tmp/mneme.json
  mneme correct --claim-id claim-001 "user prefers terminal workflows" --store /tmp/mneme.json"#;

const MNEME_FORGET_HELP: &str = r#"Usage:
  mneme forget <claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]
  mneme forget --claim-id <id> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]

Mark matching active claims as forgotten.

Example:
  mneme forget "user prefers desktop IDE" --store /tmp/mneme.json
  mneme forget --claim-id claim-001 --store /tmp/mneme.json"#;

const MNEME_CLAIMS_HELP: &str = r#"Usage: mneme claims [--status <status>]... [--scope <scope>]... [--store <path>] [--json]

Review stored memory claims. Defaults to all statuses and scopes. Supported
statuses: active, blocked_secret, superseded, forgotten.

Example:
  mneme claims --status active --store /tmp/mneme.json --json"#;

const MNEME_CONTEXT_HELP: &str = r#"Usage: mneme context <query> [--scope <scope>]... [--max-items <n>] [--store <path>] [--json]

Build a cited context pack for a query. Defaults to the private scope unless
one or more --scope values are provided. Results are deterministically ranked
and capped to 8 items by default.

Example:
  mneme context "local-first" --scope private --max-items 3 --store /tmp/mneme.json --json"#;

const MNEME_SNAPSHOT_HELP: &str = r#"Usage: mneme snapshot [--store <path>] [--json]

Print the current store snapshot.

Example:
  mneme snapshot --store /tmp/mneme.json --json"#;

const MNEME_BEGIN_HELP: &str = r#"Usage: mneme begin <task> [--query <query>] [--scope <scope>]... [--max-items <n>] [--agent <id>] [--store <path>] [--json]

Start an agent task session and retrieve task-scoped context. Defaults to the
private scope unless one or more --scope values are provided. Results are capped
to 8 ranked items by default.

Example:
  mneme begin "Draft setup plan" --query "local-first" --scope private --max-items 3 --agent codex --store /tmp/mneme.json --json"#;

const MNEME_END_HELP: &str = r#"Usage: mneme end <session-id> [--summary <text>] [--remember <claim>]... [--agent <id>] [--store <path>] [--json]

Close an agent task session and optionally write explicit memory claims.

Example:
  mneme end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store /tmp/mneme.json --json"#;

const MNEME_HOOK_HELP: &str = r#"Usage:
  mneme hook doctor [--store <path>]
  mneme hook begin <task> [--query <query>] [--scope <scope>]... [--max-items <n>] [--agent <id>] [--store <path>]
  mneme hook end <session-id> [--summary <text>] [--remember <claim>]... [--agent <id>] [--store <path>]

Run agent doctor/begin/end hooks with the stable mneme.agent_hook.v1 JSON envelope.
Success and failure both write JSON to stdout. Failures exit non-zero.

Examples:
  mneme hook doctor --store /tmp/mneme.json
  mneme hook begin "Draft setup plan" --query "local-first" --agent codex --store /tmp/mneme.json
  mneme hook end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store /tmp/mneme.json"#;

const MNEME_VALIDATE_HELP: &str = r#"Usage: mneme validate [--store <path>] [--json]

Inspect the current store and backup.

Example:
  mneme validate --store /tmp/mneme.json"#;

const MNEME_EXPORT_HELP: &str = r#"Usage: mneme export <path> [--store <path>] [--json]

Export the current store state to JSON.

Example:
  mneme export /tmp/mneme-export.json --store /tmp/mneme.json"#;

const MNEME_REVIEW_HELP: &str = r#"Usage: mneme review <path> [--store <path>] [--format markdown|json] [--include-sensitive] [--json]

Export a memory review artifact summarizing stored claims, lifecycle status,
scope distribution, source events, sessions, and store metadata. Sensitive
claim text is redacted by default. Use --include-sensitive only for local,
private inspection.

Examples:
  mneme review /tmp/mneme-review.md --store /tmp/mneme.json
  mneme review /tmp/mneme-review.json --format json --store /tmp/mneme.json --json"#;

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
struct InitOptions {
    common: CommonOptions,
    config_path: Option<PathBuf>,
    agent_id: String,
    scope: String,
    max_items: usize,
    bin_path: Option<PathBuf>,
    include_bin: bool,
    force: bool,
}

impl Default for InitOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            config_path: None,
            agent_id: "codex".to_owned(),
            scope: "private".to_owned(),
            max_items: 3,
            bin_path: None,
            include_bin: true,
            force: false,
        }
    }
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
    allowed_scopes: Vec<String>,
    max_items: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct RetrievalOptions {
    common: CommonOptions,
    allowed_scopes: Vec<String>,
    max_items: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct ClaimsOptions {
    common: CommonOptions,
    statuses: Vec<ClaimStatus>,
    scopes: Vec<String>,
}

#[derive(Debug, Clone)]
struct ReviewOptions {
    common: CommonOptions,
    format: ReviewFormat,
    include_sensitive: bool,
}

impl Default for ReviewOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            format: ReviewFormat::Markdown,
            include_sensitive: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ReviewFormat {
    Markdown,
    Json,
}

impl ReviewFormat {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Markdown => "markdown",
            Self::Json => "json",
        }
    }
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
struct InitReport {
    command: &'static str,
    workspace: String,
    store: String,
    config: String,
    store_created: bool,
    store_overwritten: bool,
    config_written: bool,
    config_overwritten: bool,
    agent_id: String,
    scope: String,
    max_items: usize,
    bin: Option<String>,
    next_commands: Vec<String>,
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
struct ClaimsReport {
    store: String,
    total_count: usize,
    claim_count: usize,
    filters: ClaimsFilterReport,
    claims: Vec<ClaimSummary>,
}

#[derive(Debug, Serialize)]
struct ClaimsFilterReport {
    statuses: Vec<String>,
    scopes: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SnapshotReport {
    store: String,
    snapshot: EngineSnapshot,
}

#[derive(Debug, Serialize)]
struct ReviewReport {
    command: String,
    store: String,
    path: String,
    format: String,
    schema_version: u32,
    generation: u64,
    event_count: usize,
    claim_count: usize,
    session_count: usize,
    active_claim_count: usize,
    blocked_secret_claim_count: usize,
    superseded_claim_count: usize,
    forgotten_claim_count: usize,
    redaction: ReviewRedactionReport,
    status_counts: Vec<ClaimStatusCount>,
    scope_counts: Vec<ClaimScopeCount>,
    claims: Vec<ClaimSummary>,
    sessions: Vec<SessionSummary>,
}

#[derive(Debug, Serialize)]
struct ReviewRedactionReport {
    enabled: bool,
    policy: String,
    redacted_claim_count: usize,
    redacted_field_count: usize,
}

#[derive(Debug, Default)]
struct ReviewRedactionCounters {
    claim_count: usize,
    field_count: usize,
}

#[derive(Debug, Clone, Copy)]
enum ReviewClaimField {
    Subject,
    Predicate,
    Object,
}

#[derive(Debug, Serialize)]
struct ClaimStatusCount {
    status: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct ClaimScopeCount {
    scope: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct SessionSummary {
    id: String,
    status: String,
    task: String,
    actor_agent_id: Option<String>,
    context_query: String,
    context_claim_ids: Vec<String>,
    memory_event_ids: Vec<String>,
}

impl From<&SessionRecord> for SessionSummary {
    fn from(session: &SessionRecord) -> Self {
        Self {
            id: session.id.clone(),
            status: session.status.as_str().to_owned(),
            task: session.task.clone(),
            actor_agent_id: session.actor_agent_id.clone(),
            context_query: session.context_query.clone(),
            context_claim_ids: session.context_claim_ids.clone(),
            memory_event_ids: session.memory_event_ids.clone(),
        }
    }
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
struct AgentHookBeginReport {
    schema_version: &'static str,
    ok: bool,
    operation: &'static str,
    recoverable: bool,
    store: String,
    session_id: String,
    context_item_count: usize,
    omitted_count: usize,
    context_claim_ids: Vec<String>,
    report: SessionBeginReport,
}

#[derive(Debug, Serialize)]
struct AgentHookEndReport {
    schema_version: &'static str,
    ok: bool,
    operation: &'static str,
    recoverable: bool,
    store: String,
    session_id: String,
    remembered_event_count: usize,
    remembered_claim_count: usize,
    remembered_event_ids: Vec<String>,
    remembered_claim_ids: Vec<String>,
    report: SessionEndReport,
}

#[derive(Debug, Serialize)]
struct AgentHookDoctorReport {
    schema_version: &'static str,
    ok: bool,
    operation: &'static str,
    recoverable: bool,
    store: String,
    default_store: String,
    version: &'static str,
    build_stage: &'static str,
    operations: Vec<&'static str>,
    inspection: StoreInspection,
}

#[derive(Debug, Serialize)]
struct AgentHookErrorReport {
    schema_version: &'static str,
    ok: bool,
    operation: Option<String>,
    recoverable: bool,
    error: AgentHookErrorBody,
}

#[derive(Debug, Serialize)]
struct AgentHookErrorBody {
    kind: &'static str,
    message: String,
    exit_code: i32,
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

enum CorrectTarget {
    Text {
        old_claim: String,
        new_claim: String,
    },
    ClaimId {
        claim_id: String,
        new_claim: String,
    },
}

enum ForgetTarget {
    Text(String),
    ClaimId(String),
}

fn run_init(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_init_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let config_path = options
        .config_path
        .clone()
        .unwrap_or_else(|| default_init_config_path(&store_path));
    let workspace = env::current_dir()
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))?;
    let bin_path = resolve_init_bin_path(&options)?;

    let store_exists = store_path.exists();
    let store = JsonFileStore::new(store_path.clone());
    let inspection = store.inspect();
    if store_exists && !options.force && inspection.current.status != StoreFileStatus::Valid {
        return Err(CliError::store(
            "init store",
            &store_path,
            "store exists but is not valid; run mneme repair or mneme init --force",
        ));
    }

    let store_created = !store_exists;
    let store_overwritten = store_exists && options.force;
    if store_created || store_overwritten {
        let engine = MnemeEngine::new(MnemeConfig::default());
        let mut store = JsonFileStore::new(store_path.clone());
        engine
            .persist(&mut store)
            .map_err(|source| CliError::store_error("init store", &store_path, source))?;
    }

    let config_exists = config_path.exists();
    let config_written = !config_exists || options.force;
    let config_overwritten = config_exists && options.force;
    if config_written {
        write_agent_hook_profile(
            &config_path,
            &store_path,
            &options.agent_id,
            &options.scope,
            options.max_items,
            bin_path.as_deref(),
        )?;
    }

    let report = InitReport {
        command: "init",
        workspace: workspace.display().to_string(),
        store: store_path.display().to_string(),
        config: config_path.display().to_string(),
        store_created,
        store_overwritten,
        config_written,
        config_overwritten,
        agent_id: options.agent_id,
        scope: options.scope,
        max_items: options.max_items,
        bin: bin_path.map(|path| path.display().to_string()),
        next_commands: vec![
            "mneme doctor".to_owned(),
            format!("mneme validate --store \"{}\"", store_path.display()),
            "scripts/mneme-agent-hook.sh doctor".to_owned(),
        ],
    };
    emit_init_report(&report, options.common.json, writer)
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
    let (target, options) = parse_correct_args(raw_args)?;
    match target {
        CorrectTarget::Text {
            old_claim,
            new_claim,
        } => run_event_command(
            "correct",
            format!("correct: {old_claim} -> {new_claim}"),
            options,
            writer,
        ),
        CorrectTarget::ClaimId {
            claim_id,
            new_claim,
        } => run_claim_id_event_command(
            "correct",
            claim_id.clone(),
            format!("correct-id: {claim_id} -> {new_claim}"),
            options,
            writer,
        ),
    }
}

fn run_forget(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (target, options) = parse_forget_args(raw_args)?;
    match target {
        ForgetTarget::Text(claim) => {
            run_event_command("forget", format!("forget: {claim}"), options, writer)
        }
        ForgetTarget::ClaimId(claim_id) => run_claim_id_event_command(
            "forget",
            claim_id.clone(),
            format!("forget-id: {claim_id}"),
            options,
            writer,
        ),
    }
}

fn run_claims(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_claims_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let engine = load_engine(&store_path)?;
    let snapshot = engine.snapshot();
    let claims = snapshot
        .claims
        .iter()
        .filter(|claim| {
            (options.statuses.is_empty() || options.statuses.contains(&claim.status))
                && (options.scopes.is_empty() || options.scopes.contains(&claim.scope))
        })
        .map(ClaimSummary::from)
        .collect::<Vec<_>>();
    let report = ClaimsReport {
        store: store_path.display().to_string(),
        total_count: snapshot.claims.len(),
        claim_count: claims.len(),
        filters: ClaimsFilterReport {
            statuses: options
                .statuses
                .iter()
                .map(|status| status.as_str().to_owned())
                .collect(),
            scopes: options.scopes,
        },
        claims,
    };
    emit_claims_report(&report, options.common.json, writer)
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

fn run_claim_id_event_command(
    command: &str,
    claim_id: String,
    event_text: String,
    options: EventOptions,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    require_active_claim_id(&engine, &claim_id)?;
    engine
        .ingest_event(EventInput {
            speaker_id: options.speaker_id,
            actor_agent_id: options.actor_agent_id,
            text: event_text,
            scope: options.scope,
            trust_level: options.trust_level,
        })
        .map_err(CliError::extractor)?;
    persist_engine(&store_path, &engine)?;
    let snapshot = engine.snapshot();
    let report = EventCommandReport {
        command: command.to_owned(),
        store: store_path.display().to_string(),
        extractor: "rule".to_owned(),
        event_count: snapshot.events.len(),
        claim_count: snapshot.claims.len(),
        latest_claim: snapshot.claims.last().map(ClaimSummary::from),
    };
    emit_event_report(&report, options.common.json, writer)
}

fn run_context(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (query, options) = parse_query_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    let context_pack = engine.build_context_pack_with(
        ContextQuery::with_allowed_scopes(query, effective_allowed_scopes(options.allowed_scopes))
            .with_max_items(effective_max_items(options.max_items)),
    );
    persist_engine(&store_path, &engine)?;
    let report = ContextReport {
        store: store_path.display().to_string(),
        item_count: context_pack.items.len(),
        omitted_count: context_pack.omitted.len(),
        context_pack,
    };
    emit_context_report(&report, options.common.json, writer)
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

fn run_review(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (path, options) = parse_review_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let engine = load_engine(&store_path)?;
    let snapshot = engine.snapshot();
    let report = build_review_report(
        &store_path,
        &path,
        options.format,
        !options.include_sensitive,
        &snapshot,
    );
    write_review_artifact(&path, &report, options.format)?;
    emit_review_report(&report, options.common.json, writer)
}

fn run_begin(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (task, options) = parse_begin_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    let report = engine.begin_session(SessionBeginInput {
        task,
        actor_agent_id: options.actor_agent_id,
        query: options.query,
        allowed_scopes: effective_allowed_scopes(options.allowed_scopes),
        max_items: effective_max_items(options.max_items),
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

fn run_agent_hook(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    if wants_command_help(&raw_args) {
        return print_help(Some("hook"), writer);
    }
    let operation = raw_args.first().and_then(|value| match value.as_str() {
        "doctor" | "begin" | "end" => Some(value.clone()),
        _ => None,
    });
    match run_agent_hook_inner(raw_args, writer) {
        Ok(()) => Ok(()),
        Err(error) => {
            emit_agent_hook_error(operation, &error, writer)?;
            Err(CliError::reported(error.exit_code()))
        }
    }
}

fn run_agent_hook_inner(
    mut raw_args: Vec<String>,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if raw_args.is_empty() {
        return Err(CliError::invalid_cli(
            "usage: mneme hook <doctor|begin|end> [options]",
        ));
    }
    let operation = raw_args.remove(0);
    match operation.as_str() {
        "doctor" => run_agent_hook_doctor(raw_args, writer),
        "begin" => run_agent_hook_begin(raw_args, writer),
        "end" => run_agent_hook_end(raw_args, writer),
        value => Err(CliError::invalid_cli(format!(
            "unknown hook operation: {value}\navailable hook operations: doctor, begin, end"
        ))),
    }
}

fn run_agent_hook_doctor(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "hook doctor")?;
    let store_path = resolve_store_path(&options)?;
    let default_store = default_store_path()?;
    let store = JsonFileStore::new(store_path.clone());
    let report = AgentHookDoctorReport {
        schema_version: AGENT_HOOK_SCHEMA_VERSION,
        ok: true,
        operation: "doctor",
        recoverable: false,
        store: store_path.display().to_string(),
        default_store: default_store.display().to_string(),
        version: env!("CARGO_PKG_VERSION"),
        build_stage: BuildStage::PersonalCoreV1.as_str(),
        operations: vec!["doctor", "begin", "end"],
        inspection: store.inspect(),
    };
    write_json(writer, &report)
}

fn run_agent_hook_begin(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (task, options) = parse_begin_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    let report = engine.begin_session(SessionBeginInput {
        task,
        actor_agent_id: options.actor_agent_id,
        query: options.query,
        allowed_scopes: effective_allowed_scopes(options.allowed_scopes),
        max_items: effective_max_items(options.max_items),
    });
    persist_engine(&store_path, &engine)?;
    let hook_report = AgentHookBeginReport {
        schema_version: AGENT_HOOK_SCHEMA_VERSION,
        ok: true,
        operation: "begin",
        recoverable: false,
        store: store_path.display().to_string(),
        session_id: report.session.id.clone(),
        context_item_count: report.context_pack.items.len(),
        omitted_count: report.context_pack.omitted.len(),
        context_claim_ids: report.session.context_claim_ids.clone(),
        report,
    };
    write_json(writer, &hook_report)
}

fn run_agent_hook_end(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
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
    let hook_report = AgentHookEndReport {
        schema_version: AGENT_HOOK_SCHEMA_VERSION,
        ok: true,
        operation: "end",
        recoverable: false,
        store: store_path.display().to_string(),
        session_id: report.session.id.clone(),
        remembered_event_count: report.remembered_event_ids.len(),
        remembered_claim_count: report.remembered_claim_ids.len(),
        remembered_event_ids: report.remembered_event_ids.clone(),
        remembered_claim_ids: report.remembered_claim_ids.clone(),
        report,
    };
    write_json(writer, &hook_report)
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
        .map_err(|source| CliError::store_error("load store", &store_path, source))?
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
        .map_err(|source| CliError::store_error("save store", &store_path, source))?;
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
        .map_err(|source| CliError::store_error("repair store", &store_path, source))?;
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

fn parse_init_args(raw_args: Vec<String>) -> Result<InitOptions, CliError> {
    let mut options = InitOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--config" => {
                idx += 1;
                options.config_path =
                    Some(PathBuf::from(required_arg(&raw_args, idx, "--config")?));
            }
            "--agent" => {
                idx += 1;
                options.agent_id =
                    require_nonempty(required_arg(&raw_args, idx, "--agent")?, "agent id")?;
            }
            "--scope" => {
                idx += 1;
                options.scope =
                    require_nonempty(required_arg(&raw_args, idx, "--scope")?, "scope")?;
            }
            "--max-items" => {
                idx += 1;
                options.max_items = parse_max_items(required_arg(&raw_args, idx, "--max-items")?)?;
            }
            "--bin" => {
                idx += 1;
                options.bin_path = Some(PathBuf::from(required_arg(&raw_args, idx, "--bin")?));
                options.include_bin = true;
            }
            "--no-bin" => {
                options.include_bin = false;
                options.bin_path = None;
            }
            "--force" => {
                options.force = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown init option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected init argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
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

fn parse_correct_args(raw_args: Vec<String>) -> Result<(CorrectTarget, EventOptions), CliError> {
    let mut options = EventOptions::default();
    let mut positionals = Vec::new();
    let mut claim_id = None;
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
            "--claim-id" => {
                idx += 1;
                claim_id = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--claim-id")?,
                    "claim id",
                )?);
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
    if let Some(claim_id) = claim_id {
        if positionals.len() != 1 {
            return Err(CliError::invalid_cli(
                "usage: mneme correct --claim-id <id> <new-claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
            ));
        }
        let new_claim = require_nonempty(positionals.remove(0), "new claim")?;
        return Ok((
            CorrectTarget::ClaimId {
                claim_id,
                new_claim,
            },
            options,
        ));
    }
    if positionals.len() != 2 {
        return Err(CliError::invalid_cli(
            "usage: mneme correct <old-claim> <new-claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
        ));
    }
    let old_claim = require_nonempty(positionals.remove(0), "old claim")?;
    let new_claim = require_nonempty(positionals.remove(0), "new claim")?;
    Ok((
        CorrectTarget::Text {
            old_claim,
            new_claim,
        },
        options,
    ))
}

fn parse_forget_args(raw_args: Vec<String>) -> Result<(ForgetTarget, EventOptions), CliError> {
    let mut options = EventOptions::default();
    let mut positionals = Vec::new();
    let mut claim_id = None;
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
            "--claim-id" => {
                idx += 1;
                claim_id = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--claim-id")?,
                    "claim id",
                )?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown forget option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if let Some(claim_id) = claim_id {
        if !positionals.is_empty() {
            return Err(CliError::invalid_cli(
                "usage: mneme forget --claim-id <id> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
            ));
        }
        return Ok((ForgetTarget::ClaimId(claim_id), options));
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme forget <claim> [--store <path>] [--speaker <id>] [--agent <id>] [--scope <scope>] [--trust <trust>] [--json]",
        ));
    }
    Ok((
        ForgetTarget::Text(require_nonempty(positionals.remove(0), "claim")?),
        options,
    ))
}

fn parse_claims_args(raw_args: Vec<String>) -> Result<ClaimsOptions, CliError> {
    let mut options = ClaimsOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--status" => {
                idx += 1;
                options.statuses.push(parse_claim_status(required_arg(
                    &raw_args, idx, "--status",
                )?)?);
            }
            "--scope" => {
                idx += 1;
                options
                    .scopes
                    .push(required_arg(&raw_args, idx, "--scope")?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown claims option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected claims argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_review_args(raw_args: Vec<String>) -> Result<(PathBuf, ReviewOptions), CliError> {
    let mut options = ReviewOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--format" => {
                idx += 1;
                options.format = parse_review_format(required_arg(&raw_args, idx, "--format")?)?;
            }
            "--include-sensitive" => {
                options.include_sensitive = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown review option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme review <path> [--store <path>] [--format markdown|json] [--include-sensitive] [--json]",
        ));
    }
    Ok((PathBuf::from(positionals.remove(0)), options))
}

fn parse_query_args(raw_args: Vec<String>) -> Result<(String, RetrievalOptions), CliError> {
    let mut options = RetrievalOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--scope" => {
                idx += 1;
                options
                    .allowed_scopes
                    .push(required_arg(&raw_args, idx, "--scope")?);
            }
            "--max-items" => {
                idx += 1;
                options.max_items = Some(parse_max_items(required_arg(
                    &raw_args,
                    idx,
                    "--max-items",
                )?)?);
            }
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
            "usage: mneme context <query> [--scope <scope>]... [--max-items <n>] [--store <path>] [--json]",
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
            "--scope" => {
                idx += 1;
                options
                    .allowed_scopes
                    .push(required_arg(&raw_args, idx, "--scope")?);
            }
            "--max-items" => {
                idx += 1;
                options.max_items = Some(parse_max_items(required_arg(
                    &raw_args,
                    idx,
                    "--max-items",
                )?)?);
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
            "usage: mneme begin <task> [--query <query>] [--scope <scope>]... [--max-items <n>] [--agent <id>] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "task")?, options))
}

fn effective_allowed_scopes(scopes: Vec<String>) -> Vec<String> {
    if scopes.is_empty() {
        vec!["private".to_owned()]
    } else {
        scopes
    }
}

fn effective_max_items(max_items: Option<usize>) -> usize {
    max_items.unwrap_or(DEFAULT_CONTEXT_MAX_ITEMS)
}

fn parse_max_items(value: String) -> Result<usize, CliError> {
    value.parse::<usize>().map_err(|source| {
        CliError::invalid_cli(format!("invalid --max-items value {value}: {source}"))
    })
}

fn parse_claim_status(value: String) -> Result<ClaimStatus, CliError> {
    match value.as_str() {
        "active" => Ok(ClaimStatus::Active),
        "blocked_secret" => Ok(ClaimStatus::BlockedSecret),
        "superseded" => Ok(ClaimStatus::Superseded),
        "forgotten" => Ok(ClaimStatus::Forgotten),
        _ => Err(CliError::invalid_cli(format!(
            "unknown claim status: {value}\navailable statuses: active, blocked_secret, superseded, forgotten"
        ))),
    }
}

fn parse_review_format(value: String) -> Result<ReviewFormat, CliError> {
    match value.as_str() {
        "markdown" | "md" => Ok(ReviewFormat::Markdown),
        "json" => Ok(ReviewFormat::Json),
        _ => Err(CliError::invalid_cli(format!(
            "unknown review format: {value}\navailable review formats: markdown, json"
        ))),
    }
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

fn default_init_config_path(store_path: &Path) -> PathBuf {
    store_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("mneme-agent-hook.env")
}

fn resolve_init_bin_path(options: &InitOptions) -> Result<Option<PathBuf>, CliError> {
    if !options.include_bin {
        return Ok(None);
    }
    match &options.bin_path {
        Some(path) => Ok(Some(path.clone())),
        None => env::current_exe().map(Some).map_err(|source| {
            CliError::io(
                "read current executable",
                Path::new("<current_exe>"),
                source,
            )
        }),
    }
}

fn write_agent_hook_profile(
    path: &Path,
    store_path: &Path,
    agent_id: &str,
    scope: &str,
    max_items: usize,
    bin_path: Option<&Path>,
) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create dir", parent, source))?;
    }
    let profile = render_agent_hook_profile(store_path, agent_id, scope, max_items, bin_path)?;
    std::fs::write(path, profile).map_err(|source| CliError::io("write", path, source))
}

fn render_agent_hook_profile(
    store_path: &Path,
    agent_id: &str,
    scope: &str,
    max_items: usize,
    bin_path: Option<&Path>,
) -> Result<String, CliError> {
    let store_value = single_line_value(store_path.display().to_string(), "store path")?;
    let agent_value = single_line_value(agent_id.to_owned(), "agent id")?;
    let scope_value = single_line_value(scope.to_owned(), "scope")?;
    let mut profile = String::new();
    profile.push_str("# Generated by `mneme init`.\n");
    profile.push_str(
        "# The wrapper reads KEY=VALUE lines directly and does not execute this file.\n\n",
    );
    if let Some(path) = bin_path {
        let bin_value = single_line_value(path.display().to_string(), "binary path")?;
        profile.push_str(&format!("MNEME_BIN={bin_value}\n"));
    }
    profile.push_str(&format!("MNEME_STORE={store_value}\n"));
    profile.push_str(&format!("MNEME_AGENT_ID={agent_value}\n"));
    profile.push_str(&format!("MNEME_SCOPE={scope_value}\n"));
    profile.push_str(&format!("MNEME_MAX_ITEMS={max_items}\n"));
    Ok(profile)
}

fn single_line_value(value: String, label: &str) -> Result<String, CliError> {
    if value.contains('\n') || value.contains('\r') {
        Err(CliError::invalid_cli(format!(
            "{label} must not contain newlines"
        )))
    } else {
        Ok(value)
    }
}

fn load_engine(path: &Path) -> Result<MnemeEngine, CliError> {
    let store = JsonFileStore::new(path.to_path_buf());
    MnemeEngine::from_store(MnemeConfig::default(), &store)
        .map_err(|source| CliError::store_error("load store", path, source))
}

fn persist_engine(path: &Path, engine: &MnemeEngine) -> Result<(), CliError> {
    let mut store = JsonFileStore::new(path.to_path_buf());
    engine
        .persist(&mut store)
        .map_err(|source| CliError::store_error("save store", path, source))
}

fn require_active_claim_id(engine: &MnemeEngine, claim_id: &str) -> Result<(), CliError> {
    let claim_id = claim_id.trim();
    let snapshot = engine.snapshot();
    let claim = snapshot
        .claims
        .iter()
        .find(|claim| claim.id == claim_id)
        .ok_or_else(|| CliError::lifecycle(format!("unknown claim id: {claim_id}")))?;
    if claim.status != ClaimStatus::Active {
        return Err(CliError::lifecycle(format!(
            "claim id {claim_id} is {}; only active claims can be changed by id",
            claim.status.as_str()
        )));
    }
    Ok(())
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

fn build_review_report(
    store_path: &Path,
    path: &Path,
    format: ReviewFormat,
    redact_sensitive: bool,
    snapshot: &EngineSnapshot,
) -> ReviewReport {
    let active_claim_count = count_claims_with_status(&snapshot.claims, ClaimStatus::Active);
    let blocked_secret_claim_count =
        count_claims_with_status(&snapshot.claims, ClaimStatus::BlockedSecret);
    let superseded_claim_count =
        count_claims_with_status(&snapshot.claims, ClaimStatus::Superseded);
    let forgotten_claim_count = count_claims_with_status(&snapshot.claims, ClaimStatus::Forgotten);
    let mut scope_counts = BTreeMap::<String, usize>::new();
    for claim in &snapshot.claims {
        *scope_counts.entry(claim.scope.clone()).or_default() += 1;
    }
    let mut redaction_counters = ReviewRedactionCounters::default();
    let claims = snapshot
        .claims
        .iter()
        .map(|claim| review_claim_summary(claim, redact_sensitive, &mut redaction_counters))
        .collect();
    ReviewReport {
        command: "review".to_owned(),
        store: store_path.display().to_string(),
        path: path.display().to_string(),
        format: format.as_str().to_owned(),
        schema_version: snapshot.schema_version,
        generation: snapshot.metadata.generation,
        event_count: snapshot.events.len(),
        claim_count: snapshot.claims.len(),
        session_count: snapshot.sessions.len(),
        active_claim_count,
        blocked_secret_claim_count,
        superseded_claim_count,
        forgotten_claim_count,
        redaction: ReviewRedactionReport {
            enabled: redact_sensitive,
            policy: if redact_sensitive {
                "default_safe".to_owned()
            } else {
                "include_sensitive".to_owned()
            },
            redacted_claim_count: redaction_counters.claim_count,
            redacted_field_count: redaction_counters.field_count,
        },
        status_counts: vec![
            ClaimStatusCount {
                status: ClaimStatus::Active.as_str().to_owned(),
                count: active_claim_count,
            },
            ClaimStatusCount {
                status: ClaimStatus::BlockedSecret.as_str().to_owned(),
                count: blocked_secret_claim_count,
            },
            ClaimStatusCount {
                status: ClaimStatus::Superseded.as_str().to_owned(),
                count: superseded_claim_count,
            },
            ClaimStatusCount {
                status: ClaimStatus::Forgotten.as_str().to_owned(),
                count: forgotten_claim_count,
            },
        ],
        scope_counts: scope_counts
            .into_iter()
            .map(|(scope, count)| ClaimScopeCount { scope, count })
            .collect(),
        claims,
        sessions: snapshot.sessions.iter().map(SessionSummary::from).collect(),
    }
}

fn count_claims_with_status(claims: &[ClaimRecord], status: ClaimStatus) -> usize {
    claims.iter().filter(|claim| claim.status == status).count()
}

fn review_claim_summary(
    claim: &ClaimRecord,
    redact_sensitive: bool,
    counters: &mut ReviewRedactionCounters,
) -> ClaimSummary {
    let mut redacted_fields = 0;
    let subject = redact_review_field(
        &claim.subject,
        claim.status,
        ReviewClaimField::Subject,
        redact_sensitive,
        &mut redacted_fields,
    );
    let predicate = redact_review_field(
        &claim.predicate,
        claim.status,
        ReviewClaimField::Predicate,
        redact_sensitive,
        &mut redacted_fields,
    );
    let object = redact_review_field(
        &claim.object,
        claim.status,
        ReviewClaimField::Object,
        redact_sensitive,
        &mut redacted_fields,
    );
    if redacted_fields > 0 {
        counters.claim_count += 1;
        counters.field_count += redacted_fields;
    }
    ClaimSummary {
        id: claim.id.clone(),
        subject,
        predicate,
        object,
        status: claim.status.as_str().to_owned(),
        scope: claim.scope.clone(),
        source_event_ids: claim.source_event_ids.clone(),
    }
}

fn redact_review_field(
    value: &str,
    status: ClaimStatus,
    field: ReviewClaimField,
    redact_sensitive: bool,
    redacted_fields: &mut usize,
) -> String {
    if !redact_sensitive {
        return value.to_owned();
    }
    let blocked_secret_object =
        status == ClaimStatus::BlockedSecret && matches!(field, ReviewClaimField::Object);
    if blocked_secret_object {
        *redacted_fields += 1;
        return "[redacted:blocked_secret]".to_owned();
    }
    if looks_like_sensitive_text(value) {
        *redacted_fields += 1;
        return "[redacted:secret_like]".to_owned();
    }
    value.to_owned()
}

fn looks_like_sensitive_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.contains("api_key=")
        || lower.contains("api key")
        || lower.contains("secret=")
        || lower.contains("token=")
        || lower.contains("access_token=")
        || lower.contains("password=")
        || contains_key_like_prefix(value)
}

fn contains_key_like_prefix(value: &str) -> bool {
    value.split_whitespace().any(|part| {
        let token = part.trim_matches(|character: char| {
            !(character.is_ascii_alphanumeric() || character == '-' || character == '_')
        });
        if let Some(rest) = token.strip_prefix("sk-") {
            rest.len() >= 16
                && rest.chars().all(|character| {
                    character.is_ascii_alphanumeric() || character == '-' || character == '_'
                })
        } else {
            false
        }
    })
}

fn write_review_artifact(
    path: &Path,
    report: &ReviewReport,
    format: ReviewFormat,
) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create dir", parent, source))?;
    }
    let content = match format {
        ReviewFormat::Markdown => render_review_markdown(report),
        ReviewFormat::Json => {
            let json = serde_json::to_string_pretty(report).map_err(CliError::json)?;
            format!("{json}\n")
        }
    };
    std::fs::write(path, content).map_err(|source| CliError::io("write", path, source))
}

fn render_review_markdown(report: &ReviewReport) -> String {
    let mut output = String::new();
    output.push_str("# Mneme Memory Review\n\n");
    output.push_str("## Store\n\n");
    output.push_str(&format!("- Store: `{}`\n", report.store));
    output.push_str(&format!("- Schema version: `{}`\n", report.schema_version));
    output.push_str(&format!("- Generation: `{}`\n", report.generation));
    output.push_str(&format!("- Events: `{}`\n", report.event_count));
    output.push_str(&format!("- Claims: `{}`\n", report.claim_count));
    output.push_str(&format!("- Sessions: `{}`\n", report.session_count));
    output.push_str(&format!(
        "- Redaction: `{}` (redacted claims: `{}`, redacted fields: `{}`)\n",
        report.redaction.policy,
        report.redaction.redacted_claim_count,
        report.redaction.redacted_field_count
    ));

    output.push_str("\n## Claim Status Counts\n\n");
    output.push_str("| Status | Count |\n");
    output.push_str("| --- | ---: |\n");
    for count in &report.status_counts {
        output.push_str(&format!(
            "| `{}` | {} |\n",
            escape_markdown_cell(&count.status),
            count.count
        ));
    }

    output.push_str("\n## Scope Counts\n\n");
    if report.scope_counts.is_empty() {
        output.push_str("_No claim scopes recorded._\n");
    } else {
        output.push_str("| Scope | Count |\n");
        output.push_str("| --- | ---: |\n");
        for count in &report.scope_counts {
            output.push_str(&format!(
                "| `{}` | {} |\n",
                escape_markdown_cell(&count.scope),
                count.count
            ));
        }
    }

    output.push_str("\n## Claims\n\n");
    if report.claims.is_empty() {
        output.push_str("_No claims stored._\n");
    } else {
        output.push_str("| ID | Status | Scope | Claim | Sources |\n");
        output.push_str("| --- | --- | --- | --- | --- |\n");
        for claim in &report.claims {
            let text = format!("{} {} {}", claim.subject, claim.predicate, claim.object);
            output.push_str(&format!(
                "| `{}` | `{}` | `{}` | {} | {} |\n",
                escape_markdown_cell(&claim.id),
                escape_markdown_cell(&claim.status),
                escape_markdown_cell(&claim.scope),
                escape_markdown_cell(&text),
                escape_markdown_cell(&claim.source_event_ids.join(", "))
            ));
        }
    }

    output.push_str("\n## Sessions\n\n");
    if report.sessions.is_empty() {
        output.push_str("_No sessions recorded._\n");
    } else {
        output.push_str("| ID | Status | Task | Query | Context Claims | Memory Events |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- |\n");
        for session in &report.sessions {
            output.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} |\n",
                escape_markdown_cell(&session.id),
                escape_markdown_cell(&session.status),
                escape_markdown_cell(&session.task),
                escape_markdown_cell(&session.context_query),
                escape_markdown_cell(&session.context_claim_ids.join(", ")),
                escape_markdown_cell(&session.memory_event_ids.join(", "))
            ));
        }
    }

    output
}

fn escape_markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

fn emit_init_report(
    report: &InitReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(writer, "mneme: initialized workspace {}", report.workspace)
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "mneme: store {} ({})",
        report.store,
        init_file_action(report.store_created, report.store_overwritten)
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "mneme: agent hook profile {} ({})",
        report.config,
        init_file_action(report.config_written, report.config_overwritten)
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "mneme: verify with MNEME_AGENT_HOOK_CONFIG=\"{}\" scripts/mneme-agent-hook.sh doctor",
        report.config
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn init_file_action(written: bool, overwritten: bool) -> &'static str {
    if overwritten {
        "overwritten"
    } else if written {
        "created"
    } else {
        "existing"
    }
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
            "- {} (score={}, reason={}) [{}]",
            item.claim_text,
            item.score,
            item.match_reason,
            item.source_event_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_claims_report(
    report: &ClaimsReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: claims from {} (shown={}, total={})",
        report.store, report.claim_count, report.total_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for claim in &report.claims {
        writeln!(
            writer,
            "- {} {} {}: {} {} {} [{}]",
            claim.id,
            claim.status,
            claim.scope,
            claim.subject,
            claim.predicate,
            claim.object,
            claim.source_event_ids.join(",")
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

fn emit_review_report(
    report: &ReviewReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: review exported {} to {} (format={}, claims={}, active={}, sessions={})",
        report.store,
        report.path,
        report.format,
        report.claim_count,
        report.active_claim_count,
        report.session_count
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

fn emit_agent_hook_error(
    operation: Option<String>,
    error: &CliError,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let report = AgentHookErrorReport {
        schema_version: AGENT_HOOK_SCHEMA_VERSION,
        ok: false,
        operation,
        recoverable: error.recoverable,
        error: AgentHookErrorBody {
            kind: error.kind.as_str(),
            message: error.to_string(),
            exit_code: error.exit_code(),
        },
    };
    write_json(writer, &report)
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
        assert!(text.contains("init"));
        assert!(text.contains("hook"));
        assert!(text.contains("claims"));
        assert!(text.contains("review"));

        let mut init_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "init".to_owned(), "--help".to_owned()],
            &mut init_output,
        )?;
        let init_text = String::from_utf8(init_output)?;
        assert!(init_text.contains("Usage: mneme init"));
        assert!(init_text.contains("--config <path>"));
        assert!(init_text.contains("--force"));

        let mut command_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "begin".to_owned(), "--help".to_owned()],
            &mut command_output,
        )?;
        let command_text = String::from_utf8(command_output)?;
        assert!(command_text.contains("Usage: mneme begin <task>"));
        assert!(command_text.contains("--query <query>"));
        assert!(command_text.contains("--max-items <n>"));

        let mut hook_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "hook".to_owned(), "--help".to_owned()],
            &mut hook_output,
        )?;
        let hook_text = String::from_utf8(hook_output)?;
        assert!(hook_text.contains("mneme hook doctor"));
        assert!(hook_text.contains("mneme hook begin"));
        assert!(hook_text.contains("mneme.agent_hook.v1"));

        let mut review_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "review".to_owned(), "--help".to_owned()],
            &mut review_output,
        )?;
        let review_text = String::from_utf8(review_output)?;
        assert!(review_text.contains("Usage: mneme review <path>"));
        assert!(review_text.contains("--format markdown|json"));
        assert!(review_text.contains("--include-sensitive"));
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
    fn init_creates_store_and_agent_hook_profile() -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store_path("init-store");
        let config = temp_store_path("init-profile").with_extension("env");
        for path in [&store, &config] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
            let _ = std::fs::remove_file(format!("{}.lock", path.display()));
        }

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "init".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--agent".to_owned(),
                "codex".to_owned(),
                "--scope".to_owned(),
                "private".to_owned(),
                "--max-items".to_owned(),
                "2".to_owned(),
                "--bin".to_owned(),
                "/tmp/mneme".to_owned(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"command\": \"init\""));
        assert!(text.contains("\"store_created\": true"));
        assert!(text.contains("\"config_written\": true"));
        assert!(store.exists());
        assert!(config.exists());

        let profile = std::fs::read_to_string(&config)?;
        assert!(profile.contains("MNEME_BIN=/tmp/mneme"));
        assert!(profile.contains(&format!("MNEME_STORE={}", store.display())));
        assert!(profile.contains("MNEME_AGENT_ID=codex"));
        assert!(profile.contains("MNEME_SCOPE=private"));
        assert!(profile.contains("MNEME_MAX_ITEMS=2"));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "validate".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut second_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "init".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--bin".to_owned(),
                "/tmp/mneme".to_owned(),
                "--json".to_owned(),
            ],
            &mut second_output,
        )?;
        let second_text = String::from_utf8(second_output)?;
        assert!(second_text.contains("\"store_created\": false"));
        assert!(second_text.contains("\"config_written\": false"));

        for path in [&store, &config] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
            let _ = std::fs::remove_file(format!("{}.lock", path.display()));
        }
        Ok(())
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
    fn context_requires_allowed_scope() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("context-scope");
        let _ = std::fs::remove_file(&path);

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers project launch reviews".to_owned(),
                "--scope".to_owned(),
                "project-alpha".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut denied_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "project launch".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut denied_output,
        )?;
        let denied_text = String::from_utf8(denied_output)?;
        assert!(denied_text.contains("\"item_count\": 0"));
        assert!(denied_text.contains("scope_denied:project-alpha"));

        let mut allowed_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "project launch".to_owned(),
                "--scope".to_owned(),
                "project-alpha".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut allowed_output,
        )?;
        let allowed_text = String::from_utf8(allowed_output)?;
        assert!(allowed_text.contains("\"item_count\": 1"));
        assert!(allowed_text.contains("project launch reviews"));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn context_ranking_respects_max_items() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("context-ranking");
        let _ = std::fs::remove_file(&path);

        for claim in [
            "user prefers launch templates",
            "user prefers review summaries",
            "user prefers launch review checklists",
        ] {
            run_cli_with_writer(
                vec![
                    "mneme".to_owned(),
                    "remember".to_owned(),
                    claim.to_owned(),
                    "--store".to_owned(),
                    path.display().to_string(),
                ],
                &mut Vec::new(),
            )?;
        }

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "launch review".to_owned(),
                "--max-items".to_owned(),
                "1".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"item_count\": 1"));
        assert!(text.contains("launch review checklists"));
        assert!(text.contains("\"score\": 25"));
        assert!(text.contains("context_budget_exceeded:max_items=1"));

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

    #[test]
    fn claims_review_and_id_lifecycle_controls() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("claims-review-id-lifecycle");
        let _ = std::fs::remove_file(&path);

        for _ in 0..2 {
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
        }

        let mut claims_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "claims".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut claims_output,
        )?;
        let claims_text = String::from_utf8(claims_output)?;
        assert!(claims_text.contains("\"claim_count\": 2"));
        assert!(claims_text.contains("\"id\": \"claim-001\""));
        assert!(claims_text.contains("\"id\": \"claim-002\""));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "forget".to_owned(),
                "--claim-id".to_owned(),
                "claim-001".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut active_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "claims".to_owned(),
                "--status".to_owned(),
                "active".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut active_output,
        )?;
        let active_text = String::from_utf8(active_output)?;
        assert!(active_text.contains("\"claim_count\": 1"));
        assert!(!active_text.contains("\"id\": \"claim-001\""));
        assert!(active_text.contains("\"id\": \"claim-002\""));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "--claim-id".to_owned(),
                "claim-002".to_owned(),
                "user prefers terminal workflows".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut context_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "terminal workflows".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut context_output,
        )?;
        let context_text = String::from_utf8(context_output)?;
        assert!(context_text.contains("terminal workflows"));
        assert!(context_text.contains("\"claim_id\": \"claim-003\""));

        let result = run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "forget".to_owned(),
                "--claim-id".to_owned(),
                "claim-001".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        );
        let error = result.expect_err("inactive claim id should fail");
        assert_eq!(error.exit_code(), 1);
        assert!(error.to_string().contains("only active claims"));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn review_exports_markdown_and_json_artifacts() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("review-artifact-store");
        let markdown_path = temp_store_path("review-artifact").with_extension("md");
        let json_path = temp_store_path("review-artifact").with_extension("review.json");
        let raw_json_path = temp_store_path("review-artifact").with_extension("raw-review.json");
        for path in [&path, &markdown_path, &json_path, &raw_json_path] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        }

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
                "remember".to_owned(),
                "user prefers project launch reviews".to_owned(),
                "--scope".to_owned(),
                "project-alpha".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "--claim-id".to_owned(),
                "claim-001".to_owned(),
                "user prefers terminal workflows".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "forget".to_owned(),
                "--claim-id".to_owned(),
                "claim-003".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user token API_KEY=FAKE_TEST_VALUE".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "begin".to_owned(),
                "Draft launch review".to_owned(),
                "--query".to_owned(),
                "project launch".to_owned(),
                "--scope".to_owned(),
                "project-alpha".to_owned(),
                "--agent".to_owned(),
                "codex".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut markdown_stdout = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "review".to_owned(),
                markdown_path.display().to_string(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut markdown_stdout,
        )?;
        let markdown_report = String::from_utf8(markdown_stdout)?;
        assert!(markdown_report.contains("\"command\": \"review\""));
        assert!(markdown_report.contains("\"format\": \"markdown\""));
        assert!(markdown_report.contains("\"blocked_secret_claim_count\": 1"));
        assert!(markdown_report.contains("\"enabled\": true"));
        assert!(markdown_report.contains("\"policy\": \"default_safe\""));
        assert!(markdown_report.contains("\"redacted_claim_count\": 1"));
        assert!(markdown_report.contains("[redacted:blocked_secret]"));
        assert!(!markdown_report.contains("API_KEY=FAKE_TEST_VALUE"));

        let markdown = std::fs::read_to_string(&markdown_path)?;
        assert!(markdown.contains("# Mneme Memory Review"));
        assert!(markdown.contains("Claim Status Counts"));
        assert!(markdown.contains("Redaction: `default_safe`"));
        assert!(markdown.contains("claim-001"));
        assert!(markdown.contains("superseded"));
        assert!(markdown.contains("forgotten"));
        assert!(markdown.contains("blocked_secret"));
        assert!(markdown.contains("[redacted:blocked_secret]"));
        assert!(!markdown.contains("API_KEY=FAKE_TEST_VALUE"));
        assert!(markdown.contains("project-alpha"));
        assert!(markdown.contains("event-001"));
        assert!(markdown.contains("session-001"));

        let mut json_stdout = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "review".to_owned(),
                json_path.display().to_string(),
                "--format".to_owned(),
                "json".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut json_stdout,
        )?;
        let stdout_text = String::from_utf8(json_stdout)?;
        assert!(stdout_text.contains("\"format\": \"json\""));
        assert!(stdout_text.contains("\"policy\": \"default_safe\""));
        assert!(!stdout_text.contains("API_KEY=FAKE_TEST_VALUE"));

        let json = std::fs::read_to_string(&json_path)?;
        assert!(json.contains("\"format\": \"json\""));
        assert!(json.contains("\"scope\": \"project-alpha\""));
        assert!(json.contains("\"blocked_secret_claim_count\": 1"));
        assert!(json.contains("\"object\": \"[redacted:blocked_secret]\""));
        assert!(json.contains("\"redacted_claim_count\": 1"));
        assert!(!json.contains("API_KEY=FAKE_TEST_VALUE"));
        assert!(json.contains("\"session_count\": 1"));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "review".to_owned(),
                raw_json_path.display().to_string(),
                "--format".to_owned(),
                "json".to_owned(),
                "--include-sensitive".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        let raw_json = std::fs::read_to_string(&raw_json_path)?;
        assert!(raw_json.contains("\"enabled\": false"));
        assert!(raw_json.contains("\"policy\": \"include_sensitive\""));
        assert!(raw_json.contains("API_KEY=FAKE_TEST_VALUE"));

        for path in [&path, &markdown_path, &json_path, &raw_json_path] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        }
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

    #[test]
    fn hook_begin_end_emit_stable_json_envelope() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("hook-begin-end");
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
                "hook".to_owned(),
                "begin".to_owned(),
                "Draft setup plan".to_owned(),
                "--query".to_owned(),
                "local-first".to_owned(),
                "--agent".to_owned(),
                "codex".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut begin_output,
        )?;
        let begin_text = String::from_utf8(begin_output)?;
        assert!(begin_text.contains("\"schema_version\": \"mneme.agent_hook.v1\""));
        assert!(begin_text.contains("\"ok\": true"));
        assert!(begin_text.contains("\"operation\": \"begin\""));
        assert!(begin_text.contains("\"session_id\": \"session-001\""));
        assert!(begin_text.contains("\"context_item_count\": 1"));

        let mut end_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "hook".to_owned(),
                "end".to_owned(),
                "session-001".to_owned(),
                "--summary".to_owned(),
                "Prepared a concise setup plan".to_owned(),
                "--remember".to_owned(),
                "user prefers concise setup plans".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut end_output,
        )?;
        let end_text = String::from_utf8(end_output)?;
        assert!(end_text.contains("\"schema_version\": \"mneme.agent_hook.v1\""));
        assert!(end_text.contains("\"operation\": \"end\""));
        assert!(end_text.contains("\"remembered_event_count\": 1"));
        assert!(end_text.contains("\"remembered_claim_count\": 1"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        Ok(())
    }

    #[test]
    fn hook_doctor_emits_runtime_installation_report() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("hook-doctor");
        let _ = std::fs::remove_file(&path);

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "hook".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"schema_version\": \"mneme.agent_hook.v1\""));
        assert!(text.contains("\"ok\": true"));
        assert!(text.contains("\"operation\": \"doctor\""));
        assert!(text.contains("\"build_stage\": \"personal-core-v1\""));
        assert!(text.contains("\"doctor\""));
        assert!(text.contains("\"begin\""));
        assert!(text.contains("\"end\""));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn hook_errors_emit_json_and_nonzero_exit() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("hook-error");
        let _ = std::fs::remove_file(&path);

        let mut output = Vec::new();
        let result = run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "hook".to_owned(),
                "end".to_owned(),
                "session-404".to_owned(),
                "--summary".to_owned(),
                "Nothing happened".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut output,
        );
        let error = result.expect_err("unknown session should fail");
        assert_eq!(error.exit_code(), 1);
        assert!(!error.should_print());

        let text = String::from_utf8(output)?;
        assert!(text.contains("\"schema_version\": \"mneme.agent_hook.v1\""));
        assert!(text.contains("\"ok\": false"));
        assert!(text.contains("\"operation\": \"end\""));
        assert!(text.contains("\"kind\": \"session\""));
        assert!(text.contains("\"recoverable\": false"));

        let _ = std::fs::remove_file(&path);
        Ok(())
    }

    #[test]
    fn hook_lock_conflict_is_recoverable_store_lock_error() -> Result<(), Box<dyn std::error::Error>>
    {
        let path = temp_store_path("hook-lock");
        let store = JsonFileStore::new(path.clone());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.lock_path());
        std::fs::write(store.lock_path(), "held by test\n")?;

        let mut output = Vec::new();
        let result = run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "hook".to_owned(),
                "begin".to_owned(),
                "Draft lock plan".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut output,
        );
        let error = result.expect_err("locked store should fail");
        assert_eq!(error.exit_code(), 1);
        assert!(!error.should_print());

        let text = String::from_utf8(output)?;
        assert!(text.contains("\"ok\": false"));
        assert!(text.contains("\"operation\": \"begin\""));
        assert!(text.contains("\"kind\": \"store_lock\""));
        assert!(text.contains("\"recoverable\": true"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(store.lock_path());
        Ok(())
    }

    fn temp_store_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("mneme-cli-{name}-{}.json", std::process::id()))
    }
}
