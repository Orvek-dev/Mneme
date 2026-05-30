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
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use mneme_core::{
    validate_acceptance_contract, validate_state, AcceptanceBaseline, AcceptanceContract,
    AcceptanceCriterion, AcceptanceCriterionKind, AcceptanceValidationReport, BuildStage,
    ClaimRecord, ClaimStatus, CommandExtractor, CompactionReport, ContextPack, ContextQuery,
    EngineSnapshot, EventInput, ExtractorError, JsonFileStore, JsonTeamFileStore, MnemeConfig,
    MnemeEngine, MnemeExtractor, MnemeState, MnemeStore, OutcomeGateResult,
    OutcomeJudgmentCriterionResult, OutcomeJudgmentReport, OutcomeJudgmentVerdict,
    RuleBasedExtractor, SessionBeginInput, SessionBeginReport, SessionEndInput, SessionEndReport,
    SessionError, SessionMemoryInputMode, SessionRecord, StateValidationReport, StoreError,
    StoreErrorKind, StoreFileInspection, StoreFileStatus, StoreInspection, StoreRepairReport,
    StoreRestoreReport, TeamActor, TeamAdapterManifest, TeamAgentInput, TeamContextPack,
    TeamContextQuery, TeamFirewallReport, TeamHandoffPackage, TeamMemoryConfig, TeamMemoryEngine,
    TeamMemoryQualityReport, TeamMemoryRecord, TeamMemoryState, TeamOntologyReport,
    TeamProjectInput, TeamPromotionCreateInput, TeamPromotionRecord, TeamPromotionReviewInput,
    TeamPromotionReviewReport, TeamRole, TeamRunBeginInput, TeamRunBeginReport, TeamRunEndInput,
    TeamRunEndReport, TeamRunHandoffInput, TeamRunNoteInput, TeamRunNoteReport,
    TeamStateValidationReport, TeamSyncApplyReport, TeamSyncEnvelope, TeamSyncExportInput,
    TeamUserInput, ValidationSeverity, VerifierReport, DEFAULT_CONTEXT_MAX_ITEMS,
    DEFAULT_TEAM_CONTEXT_MAX_ITEMS, PRODUCT_NAME,
};
use serde::{Deserialize, Serialize};

const AGENT_HOOK_SCHEMA_VERSION: &str = "mneme.agent_hook.v1";
const STORE_MUTATION_RETRY_ATTEMPTS: usize = 80;
const STORE_MUTATION_RETRY_BASE_MS: u64 = 5;

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
        run_doctor(Vec::new(), writer)?;
        return Ok(());
    };
    let raw_args = args.collect::<Vec<_>>();
    match command.as_str() {
        "help" => run_help(raw_args, writer),
        "--help" | "-h" => print_help(None, writer),
        "init" => run_command_or_help("init", raw_args, writer, run_init),
        "doctor" => run_command_or_help("doctor", raw_args, writer, run_doctor),
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
        "quality" => run_command_or_help("quality", raw_args, writer, run_quality),
        "curate" => run_command_or_help("curate", raw_args, writer, run_curate),
        "context" => run_command_or_help("context", raw_args, writer, run_context),
        "snapshot" => run_command_or_help("snapshot", raw_args, writer, run_snapshot),
        "begin" => run_command_or_help("begin", raw_args, writer, run_begin),
        "end" => run_command_or_help("end", raw_args, writer, run_end),
        "outcome" => run_command_or_help("outcome", raw_args, writer, run_outcome),
        "hook" => run_command_or_help("hook", raw_args, writer, run_agent_hook),
        "mcp" => run_command_or_help("mcp", raw_args, writer, run_mcp),
        "team" => run_command_or_help("team", raw_args, writer, run_team),
        "validate" => run_command_or_help("validate", raw_args, writer, run_validate_store),
        "export" => run_command_or_help("export", raw_args, writer, run_export),
        "review" => run_command_or_help("review", raw_args, writer, run_review),
        "import" => run_command_or_help("import", raw_args, writer, run_import),
        "compact" => run_command_or_help("compact", raw_args, writer, run_compact),
        "repair" => run_command_or_help("repair", raw_args, writer, run_repair),
        "restore" => run_command_or_help("restore", raw_args, writer, run_restore),
        _ => Err(CliError::invalid_cli(format!(
            "unknown mneme command: {command}\navailable commands: init, doctor, version, ingest, remember, correct, forget, claims, quality, curate, context, snapshot, begin, end, outcome, hook, mcp, team, validate, export, review, import, compact, repair, restore"
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
                "unknown mneme help topic: {command}\navailable help topics: init, doctor, version, ingest, remember, correct, forget, claims, quality, curate, context, snapshot, begin, end, outcome, hook, mcp, team, validate, export, review, import, compact, repair, restore"
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
        "quality" => Some(MNEME_QUALITY_HELP),
        "curate" => Some(MNEME_CURATE_HELP),
        "context" => Some(MNEME_CONTEXT_HELP),
        "snapshot" => Some(MNEME_SNAPSHOT_HELP),
        "begin" => Some(MNEME_BEGIN_HELP),
        "end" => Some(MNEME_END_HELP),
        "outcome" => Some(MNEME_OUTCOME_HELP),
        "hook" => Some(MNEME_HOOK_HELP),
        "mcp" => Some(MNEME_MCP_HELP),
        "team" => Some(MNEME_TEAM_HELP),
        "validate" => Some(MNEME_VALIDATE_HELP),
        "export" => Some(MNEME_EXPORT_HELP),
        "review" => Some(MNEME_REVIEW_HELP),
        "import" => Some(MNEME_IMPORT_HELP),
        "compact" => Some(MNEME_COMPACT_HELP),
        "repair" => Some(MNEME_REPAIR_HELP),
        "restore" => Some(MNEME_RESTORE_HELP),
        _ => None,
    }
}

const MNEME_HELP: &str = r#"Mneme local CLI

Usage:
  mneme <command> [options]
  mneme help [command]

Commands:
  init        Initialize a local .mneme store and agent hook profile.
  doctor      Inspect workspace store and agent hook profile health.
  version     Print the CLI version.
  ingest      Ingest one event, optionally through a command extractor.
  remember    Save an explicit memory claim.
  correct     Supersede one claim with another claim.
  forget      Mark a claim as forgotten.
  claims      Review stored memory claims.
  quality     Inspect memory quality findings and review queue.
  curate      Plan or apply guided memory cleanup actions.
  context     Build a cited context pack for a query.
  snapshot    Print the current store snapshot.
  begin       Start an agent task session and retrieve context.
  end         Close an agent task session and optionally remember claims.
  outcome     Author, validate, inspect, or judge outcome gates.
  hook        Agent hook JSON contract for begin/end automation.
  mcp         Generate MCP client configuration snippets.
  team        Manage v2 team memory, policy, promotion, and audit.
  validate    Inspect the current store and backup.
  export      Export the current store state to JSON.
  review      Export a human-readable memory review artifact.
  import      Import a store state from JSON.
  compact     Remove non-active claims and unreferenced events.
  repair      Restore the current store from its backup when possible.
  restore     Roll back the current store from a valid backup.

Common options:
  --store <path>  Use an isolated JSON store.
  --json          Print JSON output.

Examples:
  mneme init
  mneme remember "user prefers local-first tools" --store /tmp/mneme.json
  mneme claims --status active --store /tmp/mneme.json --json
  mneme quality --store /tmp/mneme.json --json
  mneme curate --store /tmp/mneme.json --json
  mneme context "local-first" --store /tmp/mneme.json --json
  mneme team init --store /tmp/mneme-team.json --json
  mneme outcome template --kind rust --include-judgment --output acceptance.json
  mneme outcome validate acceptance.json --json
  mneme outcome status session-001 --store /tmp/mneme.json --json
  mneme outcome judge session-001 --id ux-review --verdict pass --reviewer lee --store /tmp/mneme.json --json
  mneme mcp config --client all --json
  mneme team remember "Atlas deploys require rollback notes" --actor alice --scope team --store /tmp/mneme-team.json
  mneme hook begin "Draft setup plan" --query "local-first" --store /tmp/mneme.json
  mneme restore --check --store /tmp/mneme.json --json
  mneme help begin"#;

const MNEME_INIT_HELP: &str = r#"Usage: mneme init [--store <path>] [--config <path>] [--agent <id>] [--scope <scope>] [--max-items <n>] [--bin <path>] [--no-bin] [--extractor-command <program>] [--force] [--json]

Initialize a local workspace by creating a valid v1 store and an agent hook
runtime profile. Defaults to .mneme/mneme-v1.json and
.mneme/mneme-agent-hook.env in the current directory.

Examples:
  mneme init
  mneme init --agent codex --scope private --max-items 3
  mneme init --store /tmp/mneme.json --config /tmp/mneme-agent-hook.env --bin /usr/local/bin/mneme --json
  mneme init --extractor-command ./mneme-extractor-wrapper"#;

const MNEME_DOCTOR_HELP: &str = r#"Usage: mneme doctor [--store <path>] [--config <path>] [--json]

Inspect local CLI build information, workspace store health, and the agent hook
runtime profile. The command reports health without mutating files.

Examples:
  mneme doctor
  mneme doctor --json
  mneme doctor --store /tmp/mneme.json --config /tmp/mneme-agent-hook.env --json"#;

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

const MNEME_QUALITY_HELP: &str = r#"Usage: mneme quality [--store <path>] [--json]

Inspect stored memory quality without mutating files. The report highlights
duplicate active claims, blocked-secret claims, inactive lifecycle history, and
the next review commands to run.

Example:
  mneme quality --store /tmp/mneme.json --json"#;

const MNEME_CURATE_HELP: &str = r#"Usage: mneme curate [--store <path>] [--apply] [--compact] [--json]

Build a guided memory cleanup plan. By default this is a dry run that does not
mutate the store. Use --apply to forget redundant duplicate active claims. Add
--compact to remove non-active records, including blocked-secret, superseded,
and forgotten claims, after the applied cleanup writes a backup.

Examples:
  mneme curate --store /tmp/mneme.json --json
  mneme curate --apply --compact --store /tmp/mneme.json --json"#;

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

const MNEME_BEGIN_HELP: &str = r#"Usage: mneme begin <task> [--acceptance <path>] [--query <query>] [--scope <scope>]... [--max-items <n>] [--agent <id>] [--store <path>] [--json]

Start an agent task session and retrieve task-scoped context. Defaults to the
private scope unless one or more --scope values are provided. Results are capped
to 8 ranked items by default. When --acceptance is supplied, Mneme stores a
mneme.acceptance.v1 contract and captures the git/worktree baseline before the
agent starts work.

Example:
  mneme begin "Draft setup plan" --query "local-first" --scope private --max-items 3 --agent codex --store /tmp/mneme.json --json
  mneme begin "Implement parser" --acceptance acceptance.json --store /tmp/mneme.json --json"#;

const MNEME_END_HELP: &str = r#"Usage: mneme end <session-id> [--summary <text>] [--remember <claim>]... [--verifier-report <path>] [--verifier-command <program>] [--verifier-arg <arg>]... [--agent <id>] [--extractor rule|command] [--extractor-command <program>] [--extractor-arg <arg>]... [--store <path>] [--json]

Close an agent task session and optionally write memory claims. The default
rule extractor treats --remember values as explicit claims; the command
extractor receives --remember values as raw memory notes. If the session has an
acceptance contract, provide either --verifier-report or --verifier-command so
the core can store a first-class gate_result. Hook end exits non-zero when that
gate_result is not passed.

Example:
  mneme end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store /tmp/mneme.json --json
  mneme end session-001 --remember "For future plans, keep summaries direct." --extractor command --extractor-command ./mneme-extractor-wrapper --store /tmp/mneme.json
  mneme end session-001 --summary "Implemented parser" --verifier-command scripts/mneme-outcome-verifier.py --store /tmp/mneme.json --json"#;

const MNEME_OUTCOME_HELP: &str = r#"Usage:
  mneme outcome template [--kind rust|node|docs|generic] [--task-id <id>] [--include-judgment] [--output <path>] [--json]
  mneme outcome validate <acceptance.json> [--json]
  mneme outcome status <session-id> [--store <path>] [--json]
  mneme outcome judge <session-id> [--judgment-report <path> | --id <criterion-id> --verdict pass|fail] [--evidence <text>] [--reviewer <id>] [--task-id <id>] [--store <path>] [--json]

Author, validate, inspect, or resolve first-class outcome gates.
`template` writes a starter mneme.acceptance.v1 contract for common task types.
`validate` checks the contract shape before a session starts. `status` is
read-only. `judge` applies an external human/model verdict to a pending
judgment criterion. Mneme validates and stores the verdict, but does not perform
the subjective judgment itself. If the updated gate is not completed, `judge`
writes output and exits non-zero.

Example:
  mneme outcome template --kind rust --include-judgment --output acceptance.json
  mneme outcome validate acceptance.json --json
  mneme outcome status session-001 --store /tmp/mneme.json --json
  mneme outcome judge session-001 --id ux-review --verdict pass --evidence "Reviewer accepted the UX" --reviewer lee --store /tmp/mneme.json --json"#;

const MNEME_HOOK_HELP: &str = r#"Usage:
  mneme hook doctor [--store <path>]
  mneme hook begin <task> [--acceptance <path>] [--query <query>] [--scope <scope>]... [--max-items <n>] [--agent <id>] [--store <path>]
  mneme hook end <session-id> [--summary <text>] [--remember <claim>]... [--verifier-report <path>] [--verifier-command <program>] [--verifier-arg <arg>]... [--agent <id>] [--extractor rule|command] [--extractor-command <program>] [--extractor-arg <arg>]... [--store <path>]

Run agent doctor/begin/end hooks with the stable mneme.agent_hook.v1 JSON envelope.
Success and failure both write JSON to stdout. Failures exit non-zero.

Examples:
  mneme hook doctor --store /tmp/mneme.json
  mneme hook begin "Draft setup plan" --query "local-first" --agent codex --store /tmp/mneme.json
  mneme hook end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store /tmp/mneme.json"#;

const MNEME_MCP_HELP: &str = r#"Usage:
  mneme mcp config [--client codex|claude-code|cursor|all] [--mcp-bin <path>] [--mode personal|team|all] [--v1-store <path>] [--team-store <path>] [--json]

Generate local stdio MCP client configuration snippets for Mneme. The command
does not mutate client config files; it prints the exact command/snippet to
review and apply.

Examples:
  mneme mcp config --client all
  mneme mcp config --client codex --mode personal --json
  mneme mcp config --client cursor --mcp-bin /usr/local/bin/mneme-mcp --team-store .mneme/mneme-team-v2.json"#;

const MNEME_TEAM_HELP: &str = r#"Usage:
  mneme team init [--workspace <id>] [--admin <user>] [--store <path>] [--json]
  mneme team user add <user> [--role admin|maintainer|member] [--store <path>] [--json]
  mneme team agent add <agent> --owner <user> [--store <path>] [--json]
  mneme team project add <project> [--member <user>]... [--store <path>] [--json]
  mneme team project grant <project> <user> [--store <path>] [--json]
  mneme team remember <text> --actor <user> [--agent <agent>] --scope <scope> [--store <path>] [--json]
  mneme team context <query> --actor <user> [--agent <agent>] [--max-items <n>] [--store <path>] [--json]
  mneme team handoff <query> --actor <user> [--agent <agent>] [--max-items <n>] [--store <path>] [--json]
  mneme team promote <memory-id> --actor <user> [--agent <agent>] [--note <text>] [--store <path>] [--json]
  mneme team review <promotion-id> --actor <user> [--agent <agent>] --approve|--reject [--store <path>] [--json]
  mneme team sync export <path> --actor <user> [--agent <agent>] [--include-projects] [--store <path>] [--json]
  mneme team sync import <path> [--apply] [--actor <admin-or-maintainer>] [--agent <agent>] [--store <path>] [--json]
  mneme team firewall [--store <path>] [--json]
  mneme team ontology [--actor <user>] [--agent <agent>] [--store <path>] [--json]
  mneme team adapter manifest [--json]
  mneme team revoke-user <user> --actor <admin> [--store <path>] [--json]
  mneme team revoke-agent <agent> --actor <admin> [--store <path>] [--json]
  mneme team validate [--store <path>] [--json]
  mneme team snapshot [--store <path>] [--json]

Manage Mneme v2 team memory in a local JSON store. Scopes are team,
private:<user>, project:<project>, or agent-private:<agent>. Team promotion is
reviewed: members can propose a private/project memory, and an admin or
maintainer must approve before it becomes team-readable memory.

Examples:
  mneme team init --admin alice
  mneme team user add bob --role member
  mneme team project add atlas --member bob
  mneme team remember "Atlas deploys require rollback notes" --actor bob --scope project:atlas
  mneme team promote team-memory-001 --actor bob
  mneme team review team-promotion-001 --actor alice --approve
  mneme team context "rollback notes" --actor alice --json
  mneme team handoff "handoff deploy notes" --actor bob --json
  mneme team sync export /tmp/mneme-team-sync.json --actor alice --include-projects --json"#;

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

Remove non-active claims and unreferenced events.

Example:
  mneme compact --store /tmp/mneme.json"#;

const MNEME_REPAIR_HELP: &str = r#"Usage:
  mneme repair [--store <path>] [--json]
  mneme repair --check [--store <path>] [--json]

Inspect repair readiness without mutating files, restore a corrupted current
store from backup, or normalize a compatible legacy store schema.

Example:
  mneme repair --check --store /tmp/mneme.json --json
  mneme repair --store /tmp/mneme.json"#;

const MNEME_RESTORE_HELP: &str = r#"Usage:
  mneme restore --check [--store <path>] [--json]
  mneme restore [--store <path>] [--json]

Inspect or roll back the current store from a valid <store>.bak file. Restore
is explicit and works even when the current store is valid. Before replacing the
current store, Mneme preserves that current file as the new backup so the
restore can be reversed.

Example:
  mneme restore --check --store /tmp/mneme.json --json
  mneme restore --store /tmp/mneme.json --json"#;

#[derive(Debug, Clone, Default)]
struct CommonOptions {
    json: bool,
    store_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum McpClient {
    Codex,
    ClaudeCode,
    Cursor,
    All,
}

impl McpClient {
    fn parse(value: String) -> Result<Self, CliError> {
        match value.as_str() {
            "codex" => Ok(Self::Codex),
            "claude-code" | "claude" => Ok(Self::ClaudeCode),
            "cursor" => Ok(Self::Cursor),
            "all" => Ok(Self::All),
            _ => Err(CliError::invalid_cli(format!(
                "unknown MCP client: {value}\navailable clients: codex, claude-code, cursor, all"
            ))),
        }
    }

    const fn selected(self) -> &'static [Self] {
        match self {
            Self::Codex => &[Self::Codex],
            Self::ClaudeCode => &[Self::ClaudeCode],
            Self::Cursor => &[Self::Cursor],
            Self::All => &[Self::Codex, Self::ClaudeCode, Self::Cursor],
        }
    }
}

#[derive(Debug, Clone)]
struct McpConfigOptions {
    client: McpClient,
    mcp_bin: String,
    mode: String,
    v1_store: Option<PathBuf>,
    team_store: Option<PathBuf>,
    json: bool,
}

impl Default for McpConfigOptions {
    fn default() -> Self {
        Self {
            client: McpClient::All,
            mcp_bin: "mneme-mcp".to_owned(),
            mode: "all".to_owned(),
            v1_store: None,
            team_store: None,
            json: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct DoctorOptions {
    common: CommonOptions,
    config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default)]
struct RepairOptions {
    common: CommonOptions,
    check: bool,
}

#[derive(Debug, Clone, Default)]
struct RestoreOptions {
    common: CommonOptions,
    check: bool,
}

#[derive(Debug, Clone, Default)]
struct CurateOptions {
    common: CommonOptions,
    apply: bool,
    compact: bool,
}

#[derive(Debug, Clone)]
struct TeamInitOptions {
    common: CommonOptions,
    workspace_id: String,
    admin_user_id: Option<String>,
}

impl Default for TeamInitOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            workspace_id: "team".to_owned(),
            admin_user_id: None,
        }
    }
}

#[derive(Debug, Clone)]
struct TeamUserAddOptions {
    common: CommonOptions,
    role: TeamRole,
}

impl Default for TeamUserAddOptions {
    fn default() -> Self {
        Self {
            common: CommonOptions::default(),
            role: TeamRole::Member,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TeamAgentAddOptions {
    common: CommonOptions,
    owner_user_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct TeamProjectAddOptions {
    common: CommonOptions,
    member_user_ids: Vec<String>,
}

#[derive(Debug, Clone, Default)]
struct TeamActorOptions {
    common: CommonOptions,
    actor_user_id: Option<String>,
    actor_agent_id: Option<String>,
}

#[derive(Debug, Clone)]
struct TeamRememberOptions {
    actor: TeamActorOptions,
    scope: String,
}

impl Default for TeamRememberOptions {
    fn default() -> Self {
        Self {
            actor: TeamActorOptions::default(),
            scope: "team".to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
struct TeamContextOptions {
    actor: TeamActorOptions,
    max_items: usize,
}

impl Default for TeamContextOptions {
    fn default() -> Self {
        Self {
            actor: TeamActorOptions::default(),
            max_items: DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
        }
    }
}

#[derive(Debug, Clone)]
struct TeamRunBeginOptions {
    actor: TeamActorOptions,
    query: Option<String>,
    scope: Option<String>,
    max_items: usize,
}

impl Default for TeamRunBeginOptions {
    fn default() -> Self {
        Self {
            actor: TeamActorOptions::default(),
            query: None,
            scope: None,
            max_items: DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TeamRunNoteOptions {
    actor: TeamActorOptions,
    scope: String,
}

#[derive(Debug, Clone, Default)]
struct TeamRunEndOptions {
    actor: TeamActorOptions,
    summary: Option<String>,
    next_steps: Vec<String>,
    remember: Vec<String>,
    scope: Option<String>,
}

#[derive(Debug, Clone)]
struct TeamRunHandoffOptions {
    actor: TeamActorOptions,
    query: Option<String>,
    max_items: usize,
}

impl Default for TeamRunHandoffOptions {
    fn default() -> Self {
        Self {
            actor: TeamActorOptions::default(),
            query: None,
            max_items: DEFAULT_TEAM_CONTEXT_MAX_ITEMS,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TeamSyncExportOptions {
    actor: TeamActorOptions,
    include_project_scopes: bool,
}

#[derive(Debug, Clone, Default)]
struct TeamSyncImportOptions {
    actor: TeamActorOptions,
    apply: bool,
}

#[derive(Debug, Clone, Default)]
struct TeamPromotionCreateOptions {
    actor: TeamActorOptions,
    note: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct TeamPromotionReviewOptions {
    actor: TeamActorOptions,
    approve: Option<bool>,
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
    extractor_command: Option<String>,
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
            extractor_command: None,
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
    acceptance_path: Option<PathBuf>,
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
    extractor: ExtractorOptions,
    verifier_report_path: Option<PathBuf>,
    verifier: VerifierOptions,
}

#[derive(Debug, Clone, Default)]
struct OutcomeJudgeOptions {
    common: CommonOptions,
    judgment_report_path: Option<PathBuf>,
    task_id: Option<String>,
    reviewer: Option<String>,
    id: Option<String>,
    verdict: Option<OutcomeJudgmentVerdict>,
    evidence: Option<String>,
}

#[derive(Debug, Clone)]
struct OutcomeTemplateOptions {
    json: bool,
    kind: OutcomeTemplateKind,
    task_id: Option<String>,
    output_path: Option<PathBuf>,
    include_judgment: bool,
}

impl Default for OutcomeTemplateOptions {
    fn default() -> Self {
        Self {
            json: false,
            kind: OutcomeTemplateKind::Rust,
            task_id: None,
            output_path: None,
            include_judgment: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OutcomeTemplateKind {
    Rust,
    Node,
    Docs,
    Generic,
}

impl OutcomeTemplateKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Node => "node",
            Self::Docs => "docs",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Default)]
enum ExtractorOptions {
    #[default]
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

#[derive(Debug, Clone, Default)]
enum VerifierOptions {
    #[default]
    None,
    Command {
        program: Option<String>,
        args: Vec<String>,
    },
}

impl VerifierOptions {
    fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
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
struct DoctorReport {
    command: &'static str,
    ok: bool,
    version: &'static str,
    build_stage: &'static str,
    workspace: String,
    default_store: String,
    store: StoreInspection,
    profile: AgentHookProfileInspection,
    checks: Vec<DoctorCheckReport>,
    recommendations: Vec<String>,
}

#[derive(Debug, Serialize)]
struct DoctorCheckReport {
    name: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct AgentHookProfileInspection {
    path: String,
    status: &'static str,
    loaded: bool,
    values: AgentHookProfileValues,
    issues: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
struct AgentHookProfileValues {
    mneme_bin: Option<String>,
    mneme_store: Option<String>,
    mneme_agent_id: Option<String>,
    mneme_scope: Option<String>,
    mneme_max_items: Option<String>,
    mneme_extractor_command: Option<String>,
    mneme_verifier_command: Option<String>,
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
    extractor_command: Option<String>,
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

#[derive(Debug, Clone, Serialize)]
struct MemoryQualityReport {
    command: String,
    store: String,
    ok: bool,
    health: String,
    claim_count: usize,
    active_claim_count: usize,
    blocked_secret_claim_count: usize,
    superseded_claim_count: usize,
    forgotten_claim_count: usize,
    inactive_claim_count: usize,
    duplicate_active_group_count: usize,
    duplicate_active_claim_count: usize,
    review_item_count: usize,
    findings: Vec<MemoryQualityFinding>,
    review_queue: Vec<MemoryReviewQueueItem>,
    next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryQualityFinding {
    kind: String,
    severity: String,
    claim_ids: Vec<String>,
    detail: String,
    recommendation: String,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryReviewQueueItem {
    kind: String,
    priority: String,
    claim_ids: Vec<String>,
    status: Option<String>,
    scope: Option<String>,
    claim_text: Option<String>,
    reason: String,
    suggested_commands: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MemoryCurationReport {
    command: String,
    store: String,
    mode: String,
    ok: bool,
    changed: bool,
    backup_path: String,
    before: MemoryQualityReport,
    after: Option<MemoryQualityReport>,
    plan: MemoryCurationPlan,
    applied: MemoryCurationApplied,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryCurationPlan {
    action_count: usize,
    applyable_action_count: usize,
    manual_action_count: usize,
    duplicate_forget_count: usize,
    compact_target_count: usize,
    blocked_secret_review_count: usize,
    compact_recommended: bool,
    compact_requested: bool,
    actions: Vec<MemoryCurationAction>,
    next_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MemoryCurationAction {
    kind: String,
    status: String,
    claim_ids: Vec<String>,
    kept_claim_id: Option<String>,
    claim_text: Option<String>,
    reason: String,
    safety: String,
}

#[derive(Debug, Clone, Default, Serialize)]
struct MemoryCurationApplied {
    event_count: usize,
    forgotten_claim_count: usize,
    compacted: bool,
    compaction: Option<CompactionReport>,
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
    quality: MemoryQualityReport,
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
    lineage_id: Option<String>,
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
            lineage_id: session.lineage_id.clone(),
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
    extractor: String,
    verifier: String,
    gate_result: Option<OutcomeGateResult>,
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
    acceptance_enabled: bool,
    report: SessionBeginReport,
}

#[derive(Debug, Serialize)]
struct AgentHookEndReport {
    schema_version: &'static str,
    ok: bool,
    operation: &'static str,
    recoverable: bool,
    store: String,
    extractor: String,
    verifier: String,
    session_id: String,
    gate_ok: Option<bool>,
    gate_status: Option<String>,
    remembered_event_count: usize,
    remembered_claim_count: usize,
    remembered_event_ids: Vec<String>,
    remembered_claim_ids: Vec<String>,
    report: SessionEndReport,
}

#[derive(Debug, Serialize)]
struct OutcomeStatusReport {
    command: &'static str,
    store: String,
    session_id: String,
    status: String,
    gate_result: Option<OutcomeGateResult>,
    session: SessionRecord,
}

#[derive(Debug, Serialize)]
struct OutcomeJudgeCliReport {
    command: &'static str,
    store: String,
    session_id: String,
    status: String,
    completed: bool,
    gate_result: OutcomeGateResult,
    session: SessionRecord,
}

#[derive(Debug, Serialize)]
struct OutcomeValidateCliReport {
    command: &'static str,
    path: String,
    validation: AcceptanceValidationReport,
}

#[derive(Debug, Serialize)]
struct OutcomeTemplateCliReport {
    command: &'static str,
    kind: &'static str,
    path: Option<String>,
    validation: AcceptanceValidationReport,
    acceptance: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct VerifierCommandRequest<'a> {
    schema_version: &'static str,
    store: String,
    workspace: String,
    session_id: &'a str,
    session: &'a SessionRecord,
    acceptance: &'a AcceptanceContract,
}

#[derive(Debug, Deserialize)]
struct RawAcceptanceContract {
    schema_version: String,
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    baseline: AcceptanceBaseline,
    #[serde(default)]
    criteria: Vec<RawAcceptanceCriterion>,
}

#[derive(Debug, Deserialize)]
struct RawAcceptanceCriterion {
    id: String,
    kind: AcceptanceCriterionKind,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    command: serde_json::Value,
    #[serde(default)]
    diff_touches: serde_json::Value,
    #[serde(default)]
    diff_scope: serde_json::Value,
    #[serde(default)]
    symbol_present: serde_json::Value,
    #[serde(default)]
    judgment: serde_json::Value,
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
struct McpConfigReport {
    command: &'static str,
    mode: String,
    mcp_bin: String,
    v1_store: String,
    team_store: String,
    snippets: Vec<McpClientSnippet>,
    next_commands: Vec<String>,
}

#[derive(Debug, Serialize)]
struct McpClientSnippet {
    client: &'static str,
    description: &'static str,
    format: &'static str,
    snippet: String,
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
struct TeamInitReport {
    command: &'static str,
    store: String,
    workspace_id: String,
    user_count: usize,
    agent_count: usize,
    project_count: usize,
    memory_count: usize,
    audit_count: usize,
    admin_user_id: Option<String>,
    next_commands: Vec<String>,
}

#[derive(Debug, Serialize)]
struct TeamEntityReport<T: Serialize> {
    command: &'static str,
    store: String,
    entity: T,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamRememberReport {
    command: &'static str,
    store: String,
    memory: TeamMemoryRecord,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamContextReport {
    command: &'static str,
    store: String,
    actor_user_id: String,
    actor_agent_id: Option<String>,
    query: String,
    item_count: usize,
    omitted_count: usize,
    context_pack: TeamContextPack,
}

#[derive(Debug, Serialize)]
struct TeamHandoffCliReport {
    command: &'static str,
    store: String,
    actor_user_id: String,
    actor_agent_id: Option<String>,
    query: String,
    context_item_count: usize,
    sync_memory_count: usize,
    firewall_ok: bool,
    package: TeamHandoffPackage,
}

#[derive(Debug, Serialize)]
struct TeamRunBeginCliReport {
    command: &'static str,
    store: String,
    actor_user_id: String,
    actor_agent_id: Option<String>,
    report: TeamRunBeginReport,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamRunNoteCliReport {
    command: &'static str,
    store: String,
    report: TeamRunNoteReport,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamRunEndCliReport {
    command: &'static str,
    store: String,
    report: TeamRunEndReport,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamRunHandoffCliReport {
    command: &'static str,
    store: String,
    run_id: String,
    context_item_count: usize,
    sync_memory_count: usize,
    firewall_ok: bool,
    package: TeamHandoffPackage,
}

#[derive(Debug, Serialize)]
struct TeamPromotionReport {
    command: &'static str,
    store: String,
    promotion: TeamPromotionRecord,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamPromotionReviewCliReport {
    command: &'static str,
    store: String,
    report: TeamPromotionReviewReport,
}

#[derive(Debug, Serialize)]
struct TeamSyncExportCliReport {
    command: &'static str,
    store: String,
    path: String,
    memory_count: usize,
    event_count: usize,
    omitted_count: usize,
    envelope: TeamSyncEnvelope,
}

#[derive(Debug, Serialize)]
struct TeamSyncImportCliReport {
    command: &'static str,
    store: String,
    path: String,
    applied: bool,
    report: TeamSyncApplyReport,
}

#[derive(Debug, Serialize)]
struct TeamFirewallCliReport {
    command: &'static str,
    store: String,
    firewall: TeamFirewallReport,
}

#[derive(Debug, Serialize)]
struct TeamQualityCliReport {
    command: &'static str,
    store: String,
    quality: TeamMemoryQualityReport,
}

#[derive(Debug, Serialize)]
struct TeamOntologyCliReport {
    command: &'static str,
    store: String,
    ontology: TeamOntologyReport,
}

#[derive(Debug, Serialize)]
struct TeamAdapterCliReport {
    command: &'static str,
    manifest: TeamAdapterManifest,
}

#[derive(Debug, Serialize)]
struct TeamValidationCliReport {
    command: &'static str,
    store: String,
    validation: TeamStateValidationReport,
}

#[derive(Debug, Serialize)]
struct TeamSnapshotReport {
    command: &'static str,
    store: String,
    snapshot: TeamMemoryState,
}

#[derive(Debug, Serialize)]
struct RepairCliReport {
    command: &'static str,
    mode: &'static str,
    ok: bool,
    store: String,
    action: String,
    current_status: StoreFileStatus,
    backup_status: StoreFileStatus,
    repair_available: bool,
    recommendations: Vec<String>,
    inspection: StoreInspection,
    #[serde(skip_serializing_if = "Option::is_none")]
    repair: Option<StoreRepairReport>,
}

#[derive(Debug, Serialize)]
struct RestoreCliReport {
    command: &'static str,
    mode: &'static str,
    ok: bool,
    store: String,
    backup_path: String,
    action: String,
    current_status: StoreFileStatus,
    backup_status: StoreFileStatus,
    restore_available: bool,
    recommendations: Vec<String>,
    inspection: StoreInspection,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore: Option<StoreRestoreReport>,
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

fn run_doctor(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_doctor_args(raw_args)?;
    let report = build_doctor_report(&options)?;
    emit_doctor_report(&report, options.common.json, writer)
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
            options.extractor_command.as_deref(),
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
        extractor_command: options.extractor_command.clone(),
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

fn run_quality(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "quality")?;
    let store_path = resolve_store_path(&options)?;
    let engine = load_engine(&store_path)?;
    let snapshot = engine.snapshot();
    let report = build_memory_quality_report(&store_path, &snapshot.claims, true);
    emit_quality_report(&report, options.json, writer)
}

fn run_curate(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_curate_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let mut engine = load_engine(&store_path)?;
    let before_snapshot = engine.snapshot();
    let before = build_memory_quality_report(&store_path, &before_snapshot.claims, true);
    let mut plan =
        build_memory_curation_plan(&store_path, &before_snapshot.claims, options.compact, true);
    let mut applied = MemoryCurationApplied::default();

    if options.apply {
        for action in &mut plan.actions {
            match action.kind.as_str() {
                "forget_duplicate_active" => {
                    for claim_id in action.claim_ids.clone() {
                        require_active_claim_id(&engine, &claim_id)?;
                        engine
                            .ingest_event(EventInput {
                                speaker_id: "system".to_owned(),
                                actor_agent_id: Some("mneme-curate".to_owned()),
                                text: format!("forget-id: {claim_id}"),
                                scope: "private".to_owned(),
                                trust_level: "system".to_owned(),
                            })
                            .map_err(CliError::extractor)?;
                        applied.event_count += 1;
                        applied.forgotten_claim_count += 1;
                    }
                    action.status = "applied".to_owned();
                }
                "compact_non_active_records" if options.compact => {
                    let compaction = engine.compact();
                    applied.compacted =
                        compaction.removed_claims > 0 || compaction.removed_events > 0;
                    applied.compaction = Some(compaction);
                    action.status = "applied".to_owned();
                }
                "compact_non_active_records" => {
                    action.status = "skipped".to_owned();
                }
                "review_blocked_secret" => {
                    action.status = "manual".to_owned();
                }
                _ => {}
            }
        }
        if applied.event_count > 0 || applied.compaction.is_some() {
            persist_engine(&store_path, &engine)?;
        }
    }

    let after = if options.apply {
        let after_snapshot = engine.snapshot();
        Some(build_memory_quality_report(
            &store_path,
            &after_snapshot.claims,
            true,
        ))
    } else {
        None
    };
    let changed = applied.event_count > 0 || applied.compaction.is_some();
    if changed {
        plan.next_commands.push(format!(
            "mneme restore --check --store \"{}\" --json",
            store_path.display()
        ));
        plan.next_commands.push(format!(
            "mneme restore --store \"{}\" --json",
            store_path.display()
        ));
    }
    let report = MemoryCurationReport {
        command: "curate".to_owned(),
        store: store_path.display().to_string(),
        mode: if options.apply {
            "apply".to_owned()
        } else {
            "dry_run".to_owned()
        },
        ok: true,
        changed,
        backup_path: JsonFileStore::new(store_path.clone())
            .backup_path()
            .display()
            .to_string(),
        before,
        after,
        plan,
        applied,
    };
    emit_curate_report(&report, options.common.json, writer)
}

fn run_event_command(
    command: &str,
    event_text: String,
    options: EventOptions,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    let store_path = resolve_store_path(&options.common)?;
    let extractor_name = options.extractor.name().to_owned();
    let input = EventInput {
        speaker_id: options.speaker_id,
        actor_agent_id: options.actor_agent_id,
        text: event_text,
        scope: options.scope,
        trust_level: options.trust_level,
    };
    let snapshot = retry_store_mutation(&store_path, || {
        let mut engine = load_engine(&store_path)?;
        match &options.extractor {
            ExtractorOptions::Rule => engine.ingest_event(input.clone()),
            ExtractorOptions::Command { .. } => {
                let extractor = build_extractor(&options.extractor)?;
                engine.ingest_event_with_extractor(input.clone(), extractor.as_ref())
            }
        }
        .map_err(CliError::extractor)?;
        persist_engine_once(&store_path, &engine)
            .map_err(|source| CliError::store_error("save store", &store_path, source))?;
        Ok(engine.snapshot())
    })?;
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
    let input = EventInput {
        speaker_id: options.speaker_id,
        actor_agent_id: options.actor_agent_id,
        text: event_text,
        scope: options.scope,
        trust_level: options.trust_level,
    };
    let snapshot = retry_store_mutation(&store_path, || {
        let mut engine = load_engine(&store_path)?;
        require_active_claim_id(&engine, &claim_id)?;
        engine
            .ingest_event(input.clone())
            .map_err(CliError::extractor)?;
        persist_engine_once(&store_path, &engine)
            .map_err(|source| CliError::store_error("save store", &store_path, source))?;
        Ok(engine.snapshot())
    })?;
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
    let acceptance = options
        .acceptance_path
        .as_deref()
        .map(load_acceptance_contract)
        .transpose()?;
    if let Some(contract) = acceptance.as_ref() {
        ensure_acceptance_valid(contract)?;
    }
    let mut engine = load_engine(&store_path)?;
    let report = engine.begin_session(SessionBeginInput {
        task,
        lineage_id: None,
        actor_agent_id: options.actor_agent_id,
        query: options.query,
        allowed_scopes: effective_allowed_scopes(options.allowed_scopes),
        max_items: effective_max_items(options.max_items),
        acceptance,
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
    let extractor_name = options.extractor.name().to_owned();
    let verifier_name = options.verifier.name().to_owned();
    let mut engine = load_engine(&store_path)?;
    let verifier_report = load_or_run_verifier_report(&store_path, &engine, &session_id, &options)?;
    let report = end_session_for_cli(
        &mut engine,
        SessionEndInput {
            session_id,
            actor_agent_id: options.actor_agent_id,
            scope: None,
            summary: options.summary,
            remember: options.remember,
            verifier_report,
        },
        &options.extractor,
    )?;
    let gate_result = report.session.gate_result.clone();
    persist_engine(&store_path, &engine)?;
    if gate_blocks_completion(&gate_result) {
        emit_failed_gate_cli_report(
            writer,
            &EndCliReport {
                store: store_path.display().to_string(),
                extractor: extractor_name,
                verifier: verifier_name,
                gate_result,
                report,
            },
            options.common.json,
        )?;
        return Err(CliError::reported(1));
    }
    let cli_report = EndCliReport {
        store: store_path.display().to_string(),
        extractor: extractor_name,
        verifier: verifier_name,
        gate_result,
        report,
    };
    emit_end_report(&cli_report, options.common.json, writer)
}

fn run_outcome(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    if raw_args.first().is_some_and(|arg| arg == "template") {
        raw_args.remove(0);
        let options = parse_outcome_template_args(raw_args)?;
        let acceptance = build_acceptance_template(&options);
        let contract = acceptance_template_to_contract(&acceptance)?;
        let validation = validate_acceptance_contract(&contract);
        let path = if let Some(path) = &options.output_path {
            let text = serde_json::to_string_pretty(&acceptance).map_err(CliError::json)?;
            std::fs::write(path, format!("{text}\n"))
                .map_err(|source| CliError::io("write outcome template", path, source))?;
            Some(path.display().to_string())
        } else {
            None
        };
        let report = OutcomeTemplateCliReport {
            command: "outcome.template",
            kind: options.kind.as_str(),
            path,
            validation,
            acceptance,
        };
        return emit_outcome_template_report(&report, options.json, writer);
    }
    if raw_args.first().is_some_and(|arg| arg == "validate") {
        raw_args.remove(0);
        let (path, options) = parse_outcome_validate_args(raw_args)?;
        let contract = load_acceptance_contract(&path)?;
        let validation = validate_acceptance_contract(&contract);
        let report = OutcomeValidateCliReport {
            command: "outcome.validate",
            path: path.display().to_string(),
            validation,
        };
        emit_outcome_validate_report(&report, options.json, writer)?;
        if !report.validation.ok {
            return Err(CliError::reported(1));
        }
        return Ok(());
    }
    if raw_args.first().is_some_and(|arg| arg == "status") {
        raw_args.remove(0);
        let (session_id, options) = parse_outcome_status_args(raw_args)?;
        let store_path = resolve_store_path(&options)?;
        let engine = load_engine(&store_path)?;
        let session = engine
            .snapshot()
            .sessions
            .into_iter()
            .find(|session| session.id == session_id)
            .ok_or_else(|| CliError::lifecycle(format!("unknown session: {session_id}")))?;
        let status = session
            .gate_result
            .as_ref()
            .map(|gate| gate.status.as_str().to_owned())
            .unwrap_or_else(|| "no_acceptance_gate".to_owned());
        let report = OutcomeStatusReport {
            command: "outcome.status",
            store: store_path.display().to_string(),
            session_id: session.id.clone(),
            status,
            gate_result: session.gate_result.clone(),
            session,
        };
        return emit_outcome_status_report(&report, options.json, writer);
    }
    if raw_args.first().is_some_and(|arg| arg == "judge") {
        raw_args.remove(0);
        let (session_id, options) = parse_outcome_judge_args(raw_args)?;
        let store_path = resolve_store_path(&options.common)?;
        let judgment_report = load_or_build_judgment_report(&options)?;
        let mut engine = load_engine(&store_path)?;
        let apply_report = engine
            .apply_outcome_judgment(&session_id, judgment_report)
            .map_err(CliError::session)?;
        let gate_result = apply_report.gate_result;
        let report = OutcomeJudgeCliReport {
            command: "outcome.judge",
            store: store_path.display().to_string(),
            session_id: apply_report.session.id.clone(),
            status: gate_result.status.as_str().to_owned(),
            completed: gate_result.completed,
            gate_result,
            session: apply_report.session,
        };
        persist_engine(&store_path, &engine)?;
        emit_outcome_judge_report(&report, options.common.json, writer)?;
        if !report.completed {
            return Err(CliError::reported(1));
        }
        return Ok(());
    }
    Err(CliError::invalid_cli(
        "usage: mneme outcome <template|validate|status|judge> ...",
    ))
}

fn end_session_for_cli(
    engine: &mut MnemeEngine,
    input: SessionEndInput,
    options: &ExtractorOptions,
) -> Result<SessionEndReport, CliError> {
    if input.remember.is_empty() {
        return engine.end_session(input).map_err(CliError::session);
    }
    let memory_input_mode = memory_input_mode_for_extractor(options);
    let extractor = build_extractor(options)?;
    engine
        .end_session_with_extractor(input, extractor.as_ref(), memory_input_mode)
        .map_err(CliError::session)
}

fn memory_input_mode_for_extractor(options: &ExtractorOptions) -> SessionMemoryInputMode {
    match options {
        ExtractorOptions::Rule => SessionMemoryInputMode::ExplicitClaim,
        ExtractorOptions::Command { .. } => SessionMemoryInputMode::RawEvent,
    }
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
            if matches!(error.kind, CliErrorKind::Reported) {
                return Err(error);
            }
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
    let acceptance = options
        .acceptance_path
        .as_deref()
        .map(load_acceptance_contract)
        .transpose()?;
    if let Some(contract) = acceptance.as_ref() {
        ensure_acceptance_valid(contract)?;
    }
    let mut engine = load_engine(&store_path)?;
    let report = engine.begin_session(SessionBeginInput {
        task,
        lineage_id: None,
        actor_agent_id: options.actor_agent_id,
        query: options.query,
        allowed_scopes: effective_allowed_scopes(options.allowed_scopes),
        max_items: effective_max_items(options.max_items),
        acceptance,
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
        acceptance_enabled: report.session.acceptance.is_some(),
        report,
    };
    write_json(writer, &hook_report)
}

fn run_agent_hook_end(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (session_id, options) = parse_end_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let extractor_name = options.extractor.name().to_owned();
    let verifier_name = options.verifier.name().to_owned();
    let mut engine = load_engine(&store_path)?;
    let verifier_report = load_or_run_verifier_report(&store_path, &engine, &session_id, &options)?;
    let report = end_session_for_cli(
        &mut engine,
        SessionEndInput {
            session_id,
            actor_agent_id: options.actor_agent_id,
            scope: None,
            summary: options.summary,
            remember: options.remember,
            verifier_report,
        },
        &options.extractor,
    )?;
    persist_engine(&store_path, &engine)?;
    let gate_result = report.session.gate_result.clone();
    let gate_ok = gate_result.as_ref().map(|gate| gate.completed);
    let gate_status = gate_result
        .as_ref()
        .map(|gate| gate.status.as_str().to_owned());
    let hook_report = AgentHookEndReport {
        schema_version: AGENT_HOOK_SCHEMA_VERSION,
        ok: !gate_blocks_completion(&gate_result),
        operation: "end",
        recoverable: false,
        store: store_path.display().to_string(),
        extractor: extractor_name,
        verifier: verifier_name,
        session_id: report.session.id.clone(),
        gate_ok,
        gate_status,
        remembered_event_count: report.remembered_event_ids.len(),
        remembered_claim_count: report.remembered_claim_ids.len(),
        remembered_event_ids: report.remembered_event_ids.clone(),
        remembered_claim_ids: report.remembered_claim_ids.clone(),
        report,
    };
    write_json(writer, &hook_report)?;
    if !hook_report.ok {
        return Err(CliError::reported(1));
    }
    Ok(())
}

fn run_mcp(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme mcp config [--client codex|claude-code|cursor|all] [options]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "config" => run_mcp_config(raw_args, writer),
        value => Err(CliError::invalid_cli(format!(
            "unknown mcp operation: {value}\navailable mcp operations: config"
        ))),
    }
}

fn run_mcp_config(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    if wants_command_help(&raw_args) {
        return print_help(Some("mcp"), writer);
    }
    let options = parse_mcp_config_args(raw_args)?;
    let v1_store = options.v1_store.clone().unwrap_or(default_store_path()?);
    let team_store = options
        .team_store
        .clone()
        .unwrap_or(default_team_store_path()?);
    let snippets = build_mcp_snippets(&options, &v1_store, &team_store)?;
    let report = McpConfigReport {
        command: "mcp.config",
        mode: options.mode,
        mcp_bin: options.mcp_bin,
        v1_store: v1_store.display().to_string(),
        team_store: team_store.display().to_string(),
        snippets,
        next_commands: vec![
            "mneme-mcp --self-test".to_owned(),
            "mneme-eval run --suite mcp --target mneme-mcp".to_owned(),
        ],
    };
    emit_mcp_config_report(&report, options.json, writer)
}

fn run_team(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    if raw_args.is_empty() {
        return Err(CliError::invalid_cli(
            "usage: mneme team <init|user|agent|project|remember|context|handoff|run|promote|promotion|review|sync|firewall|quality|ontology|adapter|revoke-user|revoke-agent|validate|snapshot> [options]",
        ));
    }
    let mut raw_args = raw_args;
    let operation = raw_args.remove(0);
    match operation.as_str() {
        "init" => run_team_init(raw_args, writer),
        "user" => run_team_user(raw_args, writer),
        "agent" => run_team_agent(raw_args, writer),
        "project" => run_team_project(raw_args, writer),
        "remember" => run_team_remember(raw_args, writer),
        "context" => run_team_context(raw_args, writer),
        "handoff" => run_team_handoff(raw_args, writer),
        "run" => run_team_run(raw_args, writer),
        "promote" => run_team_promote(raw_args, writer),
        "promotion" => run_team_promotion(raw_args, writer),
        "review" => run_team_review(raw_args, writer),
        "sync" => run_team_sync(raw_args, writer),
        "firewall" => run_team_firewall(raw_args, writer),
        "quality" => run_team_quality(raw_args, writer),
        "ontology" => run_team_ontology(raw_args, writer),
        "adapter" => run_team_adapter(raw_args, writer),
        "revoke-user" => run_team_revoke_user(raw_args, writer),
        "revoke-agent" => run_team_revoke_agent(raw_args, writer),
        "validate" => run_team_validate(raw_args, writer),
        "snapshot" => run_team_snapshot(raw_args, writer),
        value => Err(CliError::invalid_cli(format!(
            "unknown team operation: {value}\navailable team operations: init, user, agent, project, remember, context, handoff, run, promote, promotion, review, sync, firewall, quality, ontology, adapter, revoke-user, revoke-agent, validate, snapshot"
        ))),
    }
}

fn run_team_init(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_team_init_args(raw_args)?;
    let store_path = resolve_team_store_path(&options.common)?;
    let mut engine = TeamMemoryEngine::new(TeamMemoryConfig {
        workspace_id: options.workspace_id,
    });
    if let Some(admin_user_id) = &options.admin_user_id {
        engine.upsert_user(TeamUserInput {
            user_id: admin_user_id.clone(),
            role: TeamRole::Admin,
        });
    }
    persist_team_engine(&store_path, &engine)?;
    let state = engine.state();
    let report = TeamInitReport {
        command: "team.init",
        store: store_path.display().to_string(),
        workspace_id: state.workspace_id,
        user_count: state.users.len(),
        agent_count: state.agents.len(),
        project_count: state.projects.len(),
        memory_count: state.memories.len(),
        audit_count: state.audit.len(),
        admin_user_id: options.admin_user_id,
        next_commands: vec![
            format!(
                "mneme team user add bob --role member --store \"{}\"",
                store_path.display()
            ),
            format!(
                "mneme team remember \"Team memory\" --actor <user> --scope team --store \"{}\"",
                store_path.display()
            ),
            format!(
                "mneme team validate --store \"{}\" --json",
                store_path.display()
            ),
        ],
    };
    emit_team_init_report(&report, options.common.json, writer)
}

fn run_team_user(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team user add <user> [--role admin|maintainer|member] [--store <path>] [--json]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "add" => {
            let (user_id, options) = parse_team_user_add_args(raw_args)?;
            let store_path = resolve_team_store_path(&options.common)?;
            let mut engine = load_team_engine(&store_path)?;
            let user = engine.upsert_user(TeamUserInput {
                user_id,
                role: options.role,
            });
            persist_team_engine(&store_path, &engine)?;
            let report = TeamEntityReport {
                command: "team.user.add",
                store: store_path.display().to_string(),
                entity: user,
                validation: mneme_core::validate_team_state(&engine.state()),
            };
            emit_team_entity_report(&report, options.common.json, writer)
        }
        value => Err(CliError::invalid_cli(format!(
            "unknown team user operation: {value}\navailable team user operations: add"
        ))),
    }
}

fn run_team_agent(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team agent add <agent> --owner <user> [--store <path>] [--json]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "add" => {
            let (agent_id, options) = parse_team_agent_add_args(raw_args)?;
            let owner_user_id = options.owner_user_id.ok_or_else(|| {
                CliError::invalid_cli("mneme team agent add requires --owner <user>")
            })?;
            let store_path = resolve_team_store_path(&options.common)?;
            let mut engine = load_team_engine(&store_path)?;
            let agent = engine
                .upsert_agent(TeamAgentInput {
                    agent_id,
                    owner_user_id,
                })
                .map_err(team_policy_error)?;
            persist_team_engine(&store_path, &engine)?;
            let report = TeamEntityReport {
                command: "team.agent.add",
                store: store_path.display().to_string(),
                entity: agent,
                validation: mneme_core::validate_team_state(&engine.state()),
            };
            emit_team_entity_report(&report, options.common.json, writer)
        }
        value => Err(CliError::invalid_cli(format!(
            "unknown team agent operation: {value}\navailable team agent operations: add"
        ))),
    }
}

fn run_team_project(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team project <add|grant> [options]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "add" => {
            let (project_id, options) = parse_team_project_add_args(raw_args)?;
            let store_path = resolve_team_store_path(&options.common)?;
            let mut engine = load_team_engine(&store_path)?;
            let project = engine
                .upsert_project(TeamProjectInput {
                    project_id,
                    member_user_ids: options.member_user_ids,
                })
                .map_err(team_policy_error)?;
            persist_team_engine(&store_path, &engine)?;
            let report = TeamEntityReport {
                command: "team.project.add",
                store: store_path.display().to_string(),
                entity: project,
                validation: mneme_core::validate_team_state(&engine.state()),
            };
            emit_team_entity_report(&report, options.common.json, writer)
        }
        "grant" => {
            let (project_id, user_id, options) = parse_team_project_grant_args(raw_args)?;
            let store_path = resolve_team_store_path(&options)?;
            let mut engine = load_team_engine(&store_path)?;
            let project = engine
                .grant_project_member(&project_id, &user_id)
                .map_err(team_policy_error)?;
            persist_team_engine(&store_path, &engine)?;
            let report = TeamEntityReport {
                command: "team.project.grant",
                store: store_path.display().to_string(),
                entity: project,
                validation: mneme_core::validate_team_state(&engine.state()),
            };
            emit_team_entity_report(&report, options.json, writer)
        }
        value => Err(CliError::invalid_cli(format!(
            "unknown team project operation: {value}\navailable team project operations: add, grant"
        ))),
    }
}

fn run_team_remember(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (text, options) = parse_team_remember_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let memory = engine
        .remember(mneme_core::TeamRememberInput {
            actor,
            text,
            scope: options.scope,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamRememberReport {
        command: "team.remember",
        store: store_path.display().to_string(),
        memory,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_remember_report(&report, options.actor.common.json, writer)
}

fn run_team_context(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (query, options) = parse_team_context_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let context_pack = engine.build_context_pack(TeamContextQuery {
        actor: actor.clone(),
        query: query.clone(),
        max_items: options.max_items,
    });
    persist_team_engine(&store_path, &engine)?;
    let report = TeamContextReport {
        command: "team.context",
        store: store_path.display().to_string(),
        actor_user_id: actor.user_id,
        actor_agent_id: actor.agent_id,
        query,
        item_count: context_pack.items.len(),
        omitted_count: context_pack.omitted.len(),
        context_pack,
    };
    emit_team_context_report(&report, options.actor.common.json, writer)
}

fn run_team_handoff(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (query, options) = parse_team_context_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let package = engine
        .build_handoff_package(TeamContextQuery {
            actor: actor.clone(),
            query: query.clone(),
            max_items: options.max_items,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamHandoffCliReport {
        command: "team.handoff",
        store: store_path.display().to_string(),
        actor_user_id: actor.user_id,
        actor_agent_id: actor.agent_id,
        query,
        context_item_count: package.context_pack.items.len(),
        sync_memory_count: package.sync_envelope.memories.len(),
        firewall_ok: package.firewall.ok,
        package,
    };
    emit_team_handoff_report(&report, options.actor.common.json, writer)
}

fn run_team_run(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team run <begin|note|end|handoff> [options]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "begin" => run_team_run_begin(raw_args, writer),
        "note" => run_team_run_note(raw_args, writer),
        "end" => run_team_run_end(raw_args, writer),
        "handoff" => run_team_run_handoff(raw_args, writer),
        value => Err(CliError::invalid_cli(format!(
            "unknown team run operation: {value}\navailable team run operations: begin, note, end, handoff"
        ))),
    }
}

fn run_team_run_begin(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (task, options) = parse_team_run_begin_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let report = engine
        .begin_run(TeamRunBeginInput {
            actor: actor.clone(),
            task,
            query: options.query,
            scope: options.scope,
            max_items: Some(options.max_items),
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let cli_report = TeamRunBeginCliReport {
        command: "team.run.begin",
        store: store_path.display().to_string(),
        actor_user_id: actor.user_id,
        actor_agent_id: actor.agent_id,
        report,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_run_begin_report(&cli_report, options.actor.common.json, writer)
}

fn run_team_run_note(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (run_id, text, options) = parse_team_run_note_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let report = engine
        .note_run(TeamRunNoteInput {
            actor,
            run_id,
            text,
            scope: options.scope,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let cli_report = TeamRunNoteCliReport {
        command: "team.run.note",
        store: store_path.display().to_string(),
        report,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_run_note_report(&cli_report, options.actor.common.json, writer)
}

fn run_team_run_end(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (run_id, options) = parse_team_run_end_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let summary = options
        .summary
        .ok_or_else(|| CliError::invalid_cli("mneme team run end requires --summary <text>"))?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let report = engine
        .end_run(TeamRunEndInput {
            actor,
            run_id,
            summary,
            next_steps: options.next_steps,
            remember: options.remember,
            scope: options.scope,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let cli_report = TeamRunEndCliReport {
        command: "team.run.end",
        store: store_path.display().to_string(),
        report,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_run_end_report(&cli_report, options.actor.common.json, writer)
}

fn run_team_run_handoff(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (run_id, options) = parse_team_run_handoff_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let package = engine
        .build_run_handoff_package(TeamRunHandoffInput {
            actor,
            run_id: run_id.clone(),
            query: options.query,
            max_items: Some(options.max_items),
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamRunHandoffCliReport {
        command: "team.run.handoff",
        store: store_path.display().to_string(),
        run_id,
        context_item_count: package.context_pack.items.len(),
        sync_memory_count: package.sync_envelope.memories.len(),
        firewall_ok: package.firewall.ok,
        package,
    };
    emit_team_run_handoff_report(&report, options.actor.common.json, writer)
}

fn run_team_promote(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (memory_id, options) = parse_team_promote_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let promotion = engine
        .create_promotion(TeamPromotionCreateInput {
            actor,
            source_memory_id: memory_id,
            note: options.note,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamPromotionReport {
        command: "team.promote",
        store: store_path.display().to_string(),
        promotion,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_promotion_report(&report, options.actor.common.json, writer)
}

fn run_team_promotion(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team promotion report <promotion-id> [--store <path>] [--json]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "report" => {
            let (promotion_id, options) = parse_pathless_target_args(
                raw_args,
                "usage: mneme team promotion report <promotion-id> [--store <path>] [--json]",
            )?;
            let store_path = resolve_team_store_path(&options)?;
            let engine = load_team_engine(&store_path)?;
            let report = engine
                .promotion_review_report(&promotion_id)
                .map_err(team_policy_error)?;
            let cli_report = TeamPromotionReviewCliReport {
                command: "team.promotion.report",
                store: store_path.display().to_string(),
                report,
            };
            emit_team_promotion_review_report(&cli_report, options.json, writer)
        }
        value => Err(CliError::invalid_cli(format!(
            "unknown team promotion operation: {value}\navailable team promotion operations: report"
        ))),
    }
}

fn run_team_review(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (promotion_id, options) = parse_team_review_args(raw_args)?;
    let approve = options.approve.ok_or_else(|| {
        CliError::invalid_cli("mneme team review requires exactly one of --approve or --reject")
    })?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let promotion = engine
        .review_promotion(TeamPromotionReviewInput {
            actor,
            promotion_id,
            approve,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamPromotionReport {
        command: "team.review",
        store: store_path.display().to_string(),
        promotion,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_promotion_report(&report, options.actor.common.json, writer)
}

fn run_team_sync(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team sync <export|import> [options]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "export" => run_team_sync_export(raw_args, writer),
        "import" => run_team_sync_import(raw_args, writer),
        value => Err(CliError::invalid_cli(format!(
            "unknown team sync operation: {value}\navailable team sync operations: export, import"
        ))),
    }
}

fn run_team_sync_export(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (path, options) = parse_team_sync_export_args(raw_args)?;
    let actor = required_team_actor(&options.actor)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let envelope = engine
        .export_sync_envelope(TeamSyncExportInput {
            actor,
            include_project_scopes: options.include_project_scopes,
        })
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    write_team_sync_envelope(&path, &envelope)?;
    let report = TeamSyncExportCliReport {
        command: "team.sync.export",
        store: store_path.display().to_string(),
        path: path.display().to_string(),
        memory_count: envelope.memories.len(),
        event_count: envelope.events.len(),
        omitted_count: envelope.omitted.len(),
        envelope,
    };
    emit_team_sync_export_report(&report, options.actor.common.json, writer)
}

fn run_team_sync_import(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (path, options) = parse_team_sync_import_args(raw_args)?;
    let store_path = resolve_team_store_path(&options.actor.common)?;
    let envelope = read_team_sync_envelope(&path)?;
    let mut engine = load_team_engine(&store_path)?;
    let actor = if options.actor.actor_user_id.is_some() {
        Some(required_team_actor(&options.actor)?)
    } else {
        None
    };
    let apply_report = engine.apply_sync_envelope(envelope, options.apply, actor);
    if options.apply && apply_report.ok {
        persist_team_engine(&store_path, &engine)?;
    }
    let report = TeamSyncImportCliReport {
        command: "team.sync.import",
        store: store_path.display().to_string(),
        path: path.display().to_string(),
        applied: options.apply,
        report: apply_report,
    };
    emit_team_sync_import_report(&report, options.actor.common.json, writer)?;
    if report.report.ok {
        Ok(())
    } else {
        Err(CliError::store(
            "import team sync envelope",
            &store_path,
            "sync envelope rejected",
        ))
    }
}

fn run_team_firewall(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "team firewall")?;
    let store_path = resolve_team_store_path(&options)?;
    let engine = load_team_engine(&store_path)?;
    let firewall = engine.firewall_report();
    let report = TeamFirewallCliReport {
        command: "team.firewall",
        store: store_path.display().to_string(),
        firewall,
    };
    emit_team_firewall_report(&report, options.json, writer)?;
    if report.firewall.ok {
        Ok(())
    } else {
        Err(CliError::store(
            "scan team firewall",
            &store_path,
            "active memory firewall finding",
        ))
    }
}

fn run_team_quality(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "team quality")?;
    let store_path = resolve_team_store_path(&options)?;
    let engine = load_team_engine(&store_path)?;
    let quality = engine.quality_report();
    let report = TeamQualityCliReport {
        command: "team.quality",
        store: store_path.display().to_string(),
        quality,
    };
    emit_team_quality_report(&report, options.json, writer)?;
    if report.quality.ok {
        Ok(())
    } else {
        Err(CliError::store(
            "scan team quality",
            &store_path,
            "high-severity team memory quality finding",
        ))
    }
}

fn run_team_ontology(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_team_ontology_args(raw_args)?;
    let store_path = resolve_team_store_path(&options.common)?;
    let engine = load_team_engine(&store_path)?;
    let ontology = if options.actor_user_id.is_some() {
        engine
            .ontology_report_for_actor(required_team_actor(&options)?)
            .map_err(team_policy_error)?
    } else {
        engine.ontology_report()
    };
    let report = TeamOntologyCliReport {
        command: "team.ontology",
        store: store_path.display().to_string(),
        ontology,
    };
    emit_team_ontology_report(&report, options.common.json, writer)
}

fn run_team_adapter(mut raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let Some(subcommand) = raw_args.first().cloned() else {
        return Err(CliError::invalid_cli(
            "usage: mneme team adapter manifest [--json]",
        ));
    };
    raw_args.remove(0);
    match subcommand.as_str() {
        "manifest" => {
            let options = parse_no_position_args(raw_args, "team adapter manifest")?;
            let report = TeamAdapterCliReport {
                command: "team.adapter.manifest",
                manifest: TeamMemoryEngine::adapter_manifest(),
            };
            emit_team_adapter_report(&report, options.json, writer)
        }
        value => Err(CliError::invalid_cli(format!(
            "unknown team adapter operation: {value}\navailable team adapter operations: manifest"
        ))),
    }
}

fn run_team_revoke_user(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (user_id, options) = parse_team_actor_target_args(
        raw_args,
        "usage: mneme team revoke-user <user> --actor <admin> [--store <path>] [--json]",
    )?;
    let actor = required_team_actor(&options)?;
    let store_path = resolve_team_store_path(&options.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let user = engine
        .revoke_user(actor, &user_id)
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamEntityReport {
        command: "team.revoke-user",
        store: store_path.display().to_string(),
        entity: user,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_entity_report(&report, options.common.json, writer)
}

fn run_team_revoke_agent(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let (agent_id, options) = parse_team_actor_target_args(
        raw_args,
        "usage: mneme team revoke-agent <agent> --actor <admin> [--store <path>] [--json]",
    )?;
    let actor = required_team_actor(&options)?;
    let store_path = resolve_team_store_path(&options.common)?;
    let mut engine = load_team_engine(&store_path)?;
    let agent = engine
        .revoke_agent(actor, &agent_id)
        .map_err(team_policy_error)?;
    persist_team_engine(&store_path, &engine)?;
    let report = TeamEntityReport {
        command: "team.revoke-agent",
        store: store_path.display().to_string(),
        entity: agent,
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_entity_report(&report, options.common.json, writer)
}

fn run_team_validate(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "team validate")?;
    let store_path = resolve_team_store_path(&options)?;
    let engine = load_team_engine(&store_path)?;
    let report = TeamValidationCliReport {
        command: "team.validate",
        store: store_path.display().to_string(),
        validation: mneme_core::validate_team_state(&engine.state()),
    };
    emit_team_validation_report(&report, options.json, writer)?;
    if report.validation.ok {
        Ok(())
    } else {
        Err(CliError::store(
            "validate team store",
            &store_path,
            "team store is not valid",
        ))
    }
}

fn run_team_snapshot(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_no_position_args(raw_args, "team snapshot")?;
    let store_path = resolve_team_store_path(&options)?;
    let engine = load_team_engine(&store_path)?;
    let report = TeamSnapshotReport {
        command: "team.snapshot",
        store: store_path.display().to_string(),
        snapshot: engine.state(),
    };
    emit_team_snapshot_report(&report, options.json, writer)
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
    let options = parse_repair_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let store = JsonFileStore::new(store_path.clone());
    if options.check {
        let inspection = store.inspect();
        let action = repair_check_action(&inspection);
        let ok = repair_action_ok(action);
        let report = RepairCliReport {
            command: "repair",
            mode: "check",
            ok,
            store: store_path.display().to_string(),
            action: action.to_owned(),
            current_status: inspection.current.status,
            backup_status: inspection.backup.status,
            repair_available: inspection.repair_available,
            recommendations: repair_recommendations(action, &store_path),
            inspection,
            repair: None,
        };
        return emit_repair_report(&report, options.common.json, writer);
    }

    let repair = store
        .repair_from_backup()
        .map_err(|source| CliError::store_error("repair store", &store_path, source))?;
    let ok = repair.repaired || repair.after.current.status == StoreFileStatus::Valid;
    let action = repair.action.clone();
    let inspection = repair.after.clone();
    let report = RepairCliReport {
        command: "repair",
        mode: "repair",
        ok,
        store: store_path.display().to_string(),
        action: action.clone(),
        current_status: inspection.current.status,
        backup_status: inspection.backup.status,
        repair_available: inspection.repair_available,
        recommendations: repair_recommendations(&action, &store_path),
        inspection,
        repair: Some(repair),
    };
    emit_repair_report(&report, options.common.json, writer)?;
    if report.ok {
        Ok(())
    } else {
        Err(CliError::store(
            "repair store",
            &store_path,
            "store could not be repaired",
        ))
    }
}

fn run_restore(raw_args: Vec<String>, writer: &mut impl Write) -> Result<(), CliError> {
    let options = parse_restore_args(raw_args)?;
    let store_path = resolve_store_path(&options.common)?;
    let store = JsonFileStore::new(store_path.clone());
    if options.check {
        let inspection = store.inspect();
        let action = restore_check_action(&inspection);
        let ok = restore_action_ok(action);
        let report = RestoreCliReport {
            command: "restore",
            mode: "check",
            ok,
            store: store_path.display().to_string(),
            backup_path: inspection.backup_path.clone(),
            action: action.to_owned(),
            current_status: inspection.current.status,
            backup_status: inspection.backup.status,
            restore_available: restore_available(&inspection),
            recommendations: restore_recommendations(action, &store_path),
            inspection,
            restore: None,
        };
        return emit_restore_report(&report, options.common.json, writer);
    }

    let restore = store
        .restore_from_backup()
        .map_err(|source| CliError::store_error("restore store", &store_path, source))?;
    let ok = restore.restored && restore.after.current.status == StoreFileStatus::Valid;
    let action = restore.action.clone();
    let inspection = restore.after.clone();
    let report = RestoreCliReport {
        command: "restore",
        mode: "restore",
        ok,
        store: store_path.display().to_string(),
        backup_path: inspection.backup_path.clone(),
        action: action.clone(),
        current_status: inspection.current.status,
        backup_status: inspection.backup.status,
        restore_available: restore_available(&inspection),
        recommendations: restore_recommendations(&action, &store_path),
        inspection,
        restore: Some(restore),
    };
    emit_restore_report(&report, options.common.json, writer)?;
    if report.ok {
        Ok(())
    } else {
        Err(CliError::store(
            "restore store",
            &store_path,
            "store could not be restored from backup",
        ))
    }
}

fn parse_repair_args(raw_args: Vec<String>) -> Result<RepairOptions, CliError> {
    let mut options = RepairOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--check" => {
                options.check = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown repair option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected repair argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_restore_args(raw_args: Vec<String>) -> Result<RestoreOptions, CliError> {
    let mut options = RestoreOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--check" => {
                options.check = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown restore option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected restore argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_curate_args(raw_args: Vec<String>) -> Result<CurateOptions, CliError> {
    let mut options = CurateOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--apply" => {
                options.apply = true;
            }
            "--compact" => {
                options.compact = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown curate option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected curate argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_mcp_config_args(raw_args: Vec<String>) -> Result<McpConfigOptions, CliError> {
    let mut options = McpConfigOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--client" => {
                idx += 1;
                options.client = McpClient::parse(required_arg(&raw_args, idx, "--client")?)?;
            }
            "--mcp-bin" => {
                idx += 1;
                options.mcp_bin =
                    require_nonempty(required_arg(&raw_args, idx, "--mcp-bin")?, "mcp binary")?;
            }
            "--mode" => {
                idx += 1;
                options.mode = parse_mcp_mode(required_arg(&raw_args, idx, "--mode")?)?;
            }
            "--v1-store" => {
                idx += 1;
                options.v1_store = Some(PathBuf::from(required_arg(&raw_args, idx, "--v1-store")?));
            }
            "--team-store" => {
                idx += 1;
                options.team_store =
                    Some(PathBuf::from(required_arg(&raw_args, idx, "--team-store")?));
            }
            "--json" => {
                options.json = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown mcp config option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected mcp config argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_mcp_mode(value: String) -> Result<String, CliError> {
    match value.as_str() {
        "personal" | "team" | "all" => Ok(value),
        _ => Err(CliError::invalid_cli(format!(
            "unknown MCP mode: {value}\navailable modes: personal, team, all"
        ))),
    }
}

fn parse_team_init_args(raw_args: Vec<String>) -> Result<TeamInitOptions, CliError> {
    let mut options = TeamInitOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--workspace" => {
                idx += 1;
                options.workspace_id =
                    require_nonempty(required_arg(&raw_args, idx, "--workspace")?, "workspace")?;
            }
            "--admin" => {
                idx += 1;
                options.admin_user_id = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--admin")?,
                    "admin user id",
                )?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team init option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected team init argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_team_user_add_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamUserAddOptions), CliError> {
    let mut options = TeamUserAddOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--role" => {
                idx += 1;
                options.role = parse_team_role(required_arg(&raw_args, idx, "--role")?)?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team user add option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team user add <user> [--role admin|maintainer|member] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "user id")?, options))
}

fn parse_team_agent_add_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamAgentAddOptions), CliError> {
    let mut options = TeamAgentAddOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--owner" => {
                idx += 1;
                options.owner_user_id = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--owner")?,
                    "owner user id",
                )?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team agent add option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team agent add <agent> --owner <user> [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "agent id")?,
        options,
    ))
}

fn parse_team_project_add_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamProjectAddOptions), CliError> {
    let mut options = TeamProjectAddOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--member" => {
                idx += 1;
                options.member_user_ids.push(require_nonempty(
                    required_arg(&raw_args, idx, "--member")?,
                    "member user id",
                )?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team project add option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team project add <project> [--member <user>]... [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "project id")?,
        options,
    ))
}

fn parse_team_project_grant_args(
    raw_args: Vec<String>,
) -> Result<(String, String, CommonOptions), CliError> {
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
                    "unknown team project grant option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 2 {
        return Err(CliError::invalid_cli(
            "usage: mneme team project grant <project> <user> [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "project id")?,
        require_nonempty(positionals.remove(0), "user id")?,
        options,
    ))
}

fn parse_team_remember_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamRememberOptions), CliError> {
    let mut options = TeamRememberOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--scope" => {
                idx += 1;
                options.scope =
                    require_nonempty(required_arg(&raw_args, idx, "--scope")?, "scope")?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team remember option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team remember <text> --actor <user> [--agent <agent>] --scope <scope> [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "memory text")?,
        options,
    ))
}

fn parse_team_context_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamContextOptions), CliError> {
    let mut options = TeamContextOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--max-items" => {
                idx += 1;
                options.max_items = parse_max_items(required_arg(&raw_args, idx, "--max-items")?)?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team context option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team context <query> --actor <user> [--agent <agent>] [--max-items <n>] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "query")?, options))
}

fn parse_team_run_begin_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamRunBeginOptions), CliError> {
    let mut options = TeamRunBeginOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--query" => {
                idx += 1;
                options.query = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--query")?,
                    "run query",
                )?);
            }
            "--scope" => {
                idx += 1;
                options.scope = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--scope")?,
                    "run scope",
                )?);
            }
            "--max-items" => {
                idx += 1;
                options.max_items = parse_max_items(required_arg(&raw_args, idx, "--max-items")?)?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team run begin option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team run begin <task> --actor <user> [--agent <agent>] [--query <query>] [--scope <scope>] [--max-items <n>] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "task")?, options))
}

fn parse_team_run_note_args(
    raw_args: Vec<String>,
) -> Result<(String, String, TeamRunNoteOptions), CliError> {
    let mut options = TeamRunNoteOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--scope" => {
                idx += 1;
                options.scope =
                    require_nonempty(required_arg(&raw_args, idx, "--scope")?, "scope")?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team run note option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 2 || options.scope.trim().is_empty() {
        return Err(CliError::invalid_cli(
            "usage: mneme team run note <run-id> <text> --actor <user> [--agent <agent>] --scope <scope> [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "run id")?,
        require_nonempty(positionals.remove(0), "note text")?,
        options,
    ))
}

fn parse_team_run_end_args(raw_args: Vec<String>) -> Result<(String, TeamRunEndOptions), CliError> {
    let mut options = TeamRunEndOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--summary" => {
                idx += 1;
                options.summary = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--summary")?,
                    "run summary",
                )?);
            }
            "--next" => {
                idx += 1;
                options.next_steps.push(require_nonempty(
                    required_arg(&raw_args, idx, "--next")?,
                    "next step",
                )?);
            }
            "--remember" => {
                idx += 1;
                options.remember.push(require_nonempty(
                    required_arg(&raw_args, idx, "--remember")?,
                    "run memory",
                )?);
            }
            "--scope" => {
                idx += 1;
                options.scope = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--scope")?,
                    "scope",
                )?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team run end option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team run end <run-id> --actor <user> [--agent <agent>] --summary <text> [--next <text>]... [--remember <text>]... [--scope <scope>] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "run id")?, options))
}

fn parse_team_run_handoff_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamRunHandoffOptions), CliError> {
    let mut options = TeamRunHandoffOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--query" => {
                idx += 1;
                options.query = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--query")?,
                    "handoff query",
                )?);
            }
            "--max-items" => {
                idx += 1;
                options.max_items = parse_max_items(required_arg(&raw_args, idx, "--max-items")?)?;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team run handoff option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team run handoff <run-id> --actor <user> [--agent <agent>] [--query <query>] [--max-items <n>] [--store <path>] [--json]",
        ));
    }
    Ok((require_nonempty(positionals.remove(0), "run id")?, options))
}

fn parse_team_sync_export_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, TeamSyncExportOptions), CliError> {
    let mut options = TeamSyncExportOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--include-projects" => {
                options.include_project_scopes = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team sync export option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team sync export <path> --actor <user> [--agent <agent>] [--include-projects] [--store <path>] [--json]",
        ));
    }
    Ok((PathBuf::from(positionals.remove(0)), options))
}

fn parse_team_sync_import_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, TeamSyncImportOptions), CliError> {
    let mut options = TeamSyncImportOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--apply" => {
                options.apply = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team sync import option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team sync import <path> [--apply] [--actor <admin-or-maintainer>] [--agent <agent>] [--store <path>] [--json]",
        ));
    }
    Ok((PathBuf::from(positionals.remove(0)), options))
}

fn parse_team_ontology_args(raw_args: Vec<String>) -> Result<TeamActorOptions, CliError> {
    let mut options = TeamActorOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options)? {
            idx += 1;
            continue;
        }
        let value = &raw_args[idx];
        if value.starts_with('-') {
            return Err(CliError::invalid_cli(format!(
                "unknown team ontology option: {value}"
            )));
        }
        return Err(CliError::invalid_cli(format!(
            "unexpected team ontology argument: {value}"
        )));
    }
    Ok(options)
}

fn parse_team_promote_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamPromotionCreateOptions), CliError> {
    let mut options = TeamPromotionCreateOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--note" => {
                idx += 1;
                options.note = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--note")?,
                    "promotion note",
                )?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team promote option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team promote <memory-id> --actor <user> [--agent <agent>] [--note <text>] [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "memory id")?,
        options,
    ))
}

fn parse_team_review_args(
    raw_args: Vec<String>,
) -> Result<(String, TeamPromotionReviewOptions), CliError> {
    let mut options = TeamPromotionReviewOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options.actor)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--approve" => set_team_review_decision(&mut options, true)?,
            "--reject" => set_team_review_decision(&mut options, false)?,
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team review option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme team review <promotion-id> --actor <user> [--agent <agent>] --approve|--reject [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "promotion id")?,
        options,
    ))
}

fn parse_team_actor_target_args(
    raw_args: Vec<String>,
    usage: &'static str,
) -> Result<(String, TeamActorOptions), CliError> {
    let mut options = TeamActorOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_team_actor_option(&raw_args, &mut idx, &mut options)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown team actor option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(usage));
    }
    Ok((
        require_nonempty(positionals.remove(0), "target id")?,
        options,
    ))
}

fn parse_pathless_target_args(
    raw_args: Vec<String>,
    usage: &'static str,
) -> Result<(String, CommonOptions), CliError> {
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
                    "unknown target option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(usage));
    }
    Ok((
        require_nonempty(positionals.remove(0), "target id")?,
        options,
    ))
}

fn parse_team_actor_option(
    raw_args: &[String],
    idx: &mut usize,
    options: &mut TeamActorOptions,
) -> Result<bool, CliError> {
    if parse_common_option(raw_args, idx, &mut options.common)? {
        return Ok(true);
    }
    match raw_args[*idx].as_str() {
        "--actor" => {
            *idx += 1;
            options.actor_user_id = Some(require_nonempty(
                required_arg(raw_args, *idx, "--actor")?,
                "actor user id",
            )?);
            Ok(true)
        }
        "--agent" => {
            *idx += 1;
            options.actor_agent_id = Some(require_nonempty(
                required_arg(raw_args, *idx, "--agent")?,
                "actor agent id",
            )?);
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn set_team_review_decision(
    options: &mut TeamPromotionReviewOptions,
    approve: bool,
) -> Result<(), CliError> {
    if options.approve.is_some() {
        return Err(CliError::invalid_cli(
            "mneme team review accepts only one of --approve or --reject",
        ));
    }
    options.approve = Some(approve);
    Ok(())
}

fn parse_team_role(value: String) -> Result<TeamRole, CliError> {
    value
        .parse::<TeamRole>()
        .map_err(|source| CliError::invalid_cli(source.to_string()))
}

fn parse_doctor_args(raw_args: Vec<String>) -> Result<DoctorOptions, CliError> {
    let mut options = DoctorOptions::default();
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
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown doctor option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected doctor argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
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
            "--extractor-command" => {
                idx += 1;
                options.extractor_command = Some(require_nonempty(
                    required_arg(&raw_args, idx, "--extractor-command")?,
                    "extractor command",
                )?);
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

fn set_verifier_program(options: &mut VerifierOptions, program: String) {
    let args = match options {
        VerifierOptions::Command { args, .. } => std::mem::take(args),
        VerifierOptions::None => Vec::new(),
    };
    *options = VerifierOptions::Command {
        program: Some(program),
        args,
    };
}

fn push_verifier_arg(options: &mut VerifierOptions, arg: String) {
    match options {
        VerifierOptions::Command { args, .. } => args.push(arg),
        VerifierOptions::None => {
            *options = VerifierOptions::Command {
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
            "--acceptance" => {
                idx += 1;
                options.acceptance_path =
                    Some(PathBuf::from(required_arg(&raw_args, idx, "--acceptance")?));
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
            "usage: mneme begin <task> [--acceptance <path>] [--query <query>] [--scope <scope>]... [--max-items <n>] [--agent <id>] [--store <path>] [--json]",
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
            "--verifier-report" => {
                idx += 1;
                options.verifier_report_path = Some(PathBuf::from(required_arg(
                    &raw_args,
                    idx,
                    "--verifier-report",
                )?));
            }
            "--verifier-command" => {
                idx += 1;
                set_verifier_program(
                    &mut options.verifier,
                    required_arg(&raw_args, idx, "--verifier-command")?,
                );
            }
            "--verifier-arg" => {
                idx += 1;
                push_verifier_arg(
                    &mut options.verifier,
                    required_arg(&raw_args, idx, "--verifier-arg")?,
                );
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
            "usage: mneme end <session-id> [--summary <text>] [--remember <claim>]... [--verifier-report <path>] [--verifier-command <program>] [--verifier-arg <arg>]... [--agent <id>] [--extractor rule|command] [--extractor-command <program>] [--extractor-arg <arg>]... [--store <path>] [--json]",
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

fn parse_outcome_status_args(raw_args: Vec<String>) -> Result<(String, CommonOptions), CliError> {
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
                    "unknown outcome status option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme outcome status <session-id> [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "session id")?,
        options,
    ))
}

fn parse_outcome_validate_args(
    raw_args: Vec<String>,
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
                    "unknown outcome validate option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme outcome validate <acceptance.json> [--json]",
        ));
    }
    Ok((PathBuf::from(positionals.remove(0)), options))
}

fn parse_outcome_template_args(raw_args: Vec<String>) -> Result<OutcomeTemplateOptions, CliError> {
    let mut options = OutcomeTemplateOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--kind" => {
                idx += 1;
                options.kind =
                    parse_outcome_template_kind(required_arg(&raw_args, idx, "--kind")?)?;
            }
            "--task-id" => {
                idx += 1;
                options.task_id = Some(required_arg(&raw_args, idx, "--task-id")?);
            }
            "--output" => {
                idx += 1;
                options.output_path =
                    Some(PathBuf::from(required_arg(&raw_args, idx, "--output")?));
            }
            "--include-judgment" => options.include_judgment = true,
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown outcome template option: {value}"
                )));
            }
            value => {
                return Err(CliError::invalid_cli(format!(
                    "unexpected outcome template argument: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok(options)
}

fn parse_outcome_template_kind(value: String) -> Result<OutcomeTemplateKind, CliError> {
    match value.as_str() {
        "rust" => Ok(OutcomeTemplateKind::Rust),
        "node" | "npm" | "javascript" => Ok(OutcomeTemplateKind::Node),
        "docs" | "documentation" => Ok(OutcomeTemplateKind::Docs),
        "generic" => Ok(OutcomeTemplateKind::Generic),
        _ => Err(CliError::invalid_cli(format!(
            "unknown outcome template kind: {value}\navailable kinds: rust, node, docs, generic"
        ))),
    }
}

fn parse_outcome_judge_args(
    raw_args: Vec<String>,
) -> Result<(String, OutcomeJudgeOptions), CliError> {
    let mut options = OutcomeJudgeOptions::default();
    let mut positionals = Vec::new();
    let mut idx = 0;
    while idx < raw_args.len() {
        if parse_common_option(&raw_args, &mut idx, &mut options.common)? {
            idx += 1;
            continue;
        }
        match raw_args[idx].as_str() {
            "--judgment-report" => {
                idx += 1;
                options.judgment_report_path = Some(PathBuf::from(required_arg(
                    &raw_args,
                    idx,
                    "--judgment-report",
                )?));
            }
            "--task-id" => {
                idx += 1;
                options.task_id = Some(required_arg(&raw_args, idx, "--task-id")?);
            }
            "--reviewer" => {
                idx += 1;
                options.reviewer = Some(required_arg(&raw_args, idx, "--reviewer")?);
            }
            "--id" => {
                idx += 1;
                options.id = Some(required_arg(&raw_args, idx, "--id")?);
            }
            "--verdict" => {
                idx += 1;
                options.verdict = Some(parse_judgment_verdict(required_arg(
                    &raw_args,
                    idx,
                    "--verdict",
                )?)?);
            }
            "--pass" => options.verdict = Some(OutcomeJudgmentVerdict::Pass),
            "--fail" => options.verdict = Some(OutcomeJudgmentVerdict::Fail),
            "--evidence" => {
                idx += 1;
                options.evidence = Some(required_arg(&raw_args, idx, "--evidence")?);
            }
            value if value.starts_with('-') => {
                return Err(CliError::invalid_cli(format!(
                    "unknown outcome judge option: {value}"
                )));
            }
            value => positionals.push(value.to_owned()),
        }
        idx += 1;
    }
    if positionals.len() != 1 {
        return Err(CliError::invalid_cli(
            "usage: mneme outcome judge <session-id> [--judgment-report <path> | --id <criterion-id> --verdict pass|fail] [--evidence <text>] [--reviewer <id>] [--task-id <id>] [--store <path>] [--json]",
        ));
    }
    Ok((
        require_nonempty(positionals.remove(0), "session id")?,
        options,
    ))
}

fn parse_judgment_verdict(value: String) -> Result<OutcomeJudgmentVerdict, CliError> {
    match value.as_str() {
        "pass" | "passed" | "accept" | "accepted" => Ok(OutcomeJudgmentVerdict::Pass),
        "fail" | "failed" | "reject" | "rejected" => Ok(OutcomeJudgmentVerdict::Fail),
        _ => Err(CliError::invalid_cli(format!(
            "unknown judgment verdict: {value}\navailable verdicts: pass, fail"
        ))),
    }
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

fn resolve_team_store_path(options: &CommonOptions) -> Result<PathBuf, CliError> {
    match &options.store_path {
        Some(path) => Ok(path.clone()),
        None => default_team_store_path(),
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

fn load_acceptance_contract(path: &Path) -> Result<AcceptanceContract, CliError> {
    let text = std::fs::read_to_string(path)
        .map_err(|source| CliError::io("read acceptance", path, source))?;
    let raw: RawAcceptanceContract =
        serde_json::from_str(&text).map_err(|source| CliError::json_file("parse", path, source))?;
    let mut baseline = raw.baseline;
    let captured = capture_acceptance_baseline()?;
    if baseline.git_head.trim().is_empty() {
        baseline.git_head = captured.git_head;
    }
    if baseline.diff_base.trim().is_empty() {
        baseline.diff_base = captured.diff_base;
    }
    if baseline.worktree.trim().is_empty() {
        baseline.worktree = captured.worktree;
    }
    baseline.dirty |= captured.dirty;
    baseline.warnings.extend(captured.warnings);
    baseline.warnings.sort();
    baseline.warnings.dedup();
    let criteria = raw
        .criteria
        .into_iter()
        .map(|criterion| {
            let config = criterion_config_for_kind(&criterion);
            AcceptanceCriterion {
                id: criterion.id,
                kind: criterion.kind,
                description: criterion.description,
                config,
            }
        })
        .collect();
    Ok(AcceptanceContract {
        schema_version: raw.schema_version,
        task_id: raw.task_id,
        baseline,
        criteria,
    })
}

fn ensure_acceptance_valid(contract: &AcceptanceContract) -> Result<(), CliError> {
    let report = validate_acceptance_contract(contract);
    if report.ok {
        return Ok(());
    }
    Err(CliError::invalid_cli(format!(
        "invalid acceptance contract: {}",
        report.errors.join("; ")
    )))
}

fn acceptance_template_to_contract(
    value: &serde_json::Value,
) -> Result<AcceptanceContract, CliError> {
    let raw: RawAcceptanceContract =
        serde_json::from_value(value.clone()).map_err(CliError::json)?;
    let criteria = raw
        .criteria
        .into_iter()
        .map(|criterion| {
            let config = criterion_config_for_kind(&criterion);
            AcceptanceCriterion {
                id: criterion.id,
                kind: criterion.kind,
                description: criterion.description,
                config,
            }
        })
        .collect();
    Ok(AcceptanceContract {
        schema_version: raw.schema_version,
        task_id: raw.task_id,
        baseline: raw.baseline,
        criteria,
    })
}

fn build_acceptance_template(options: &OutcomeTemplateOptions) -> serde_json::Value {
    let task_id = options
        .task_id
        .clone()
        .unwrap_or_else(|| format!("{}-task", options.kind.as_str()));
    let mut criteria = match options.kind {
        OutcomeTemplateKind::Rust => vec![
            serde_json::json!({
                "id": "cargo-test",
                "kind": "command",
                "description": "Rust tests must pass.",
                "command": {"argv": ["cargo", "test", "--workspace", "--all-targets"], "expect_exit": 0}
            }),
            serde_json::json!({
                "id": "cargo-clippy",
                "kind": "command",
                "description": "Rust clippy must pass with warnings denied.",
                "command": {"argv": ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"], "expect_exit": 0}
            }),
            serde_json::json!({
                "id": "source-or-docs-changed",
                "kind": "diff_touches",
                "description": "The task should touch source, tests, scripts, or docs.",
                "diff_touches": {"paths": ["crates", "src", "tests", "scripts", "docs"]}
            }),
            serde_json::json!({
                "id": "repo-scope",
                "kind": "diff_scope",
                "description": "Changes stay inside public repository implementation and docs.",
                "diff_scope": {"allowed_paths": ["crates", "src", "tests", "scripts", "docs", "README.md", "CHANGELOG.md", "Cargo.toml", "Cargo.lock"]}
            }),
        ],
        OutcomeTemplateKind::Node => vec![
            serde_json::json!({
                "id": "npm-test",
                "kind": "command",
                "description": "Node test script must pass.",
                "command": {"argv": ["npm", "test"], "expect_exit": 0}
            }),
            serde_json::json!({
                "id": "npm-build",
                "kind": "command",
                "description": "Node build script must pass.",
                "command": {"argv": ["npm", "run", "build"], "expect_exit": 0}
            }),
            serde_json::json!({
                "id": "app-or-docs-changed",
                "kind": "diff_touches",
                "diff_touches": {"paths": ["src", "app", "lib", "docs", "README.md", "package.json"]}
            }),
            serde_json::json!({
                "id": "repo-scope",
                "kind": "diff_scope",
                "diff_scope": {"allowed_paths": ["src", "app", "lib", "tests", "docs", "README.md", "package.json", "package-lock.json", "pnpm-lock.yaml", "yarn.lock"]}
            }),
        ],
        OutcomeTemplateKind::Docs => vec![
            serde_json::json!({
                "id": "docs-changed",
                "kind": "diff_touches",
                "description": "The task should touch docs or README content.",
                "diff_touches": {"paths": ["docs", "README.md", "CHANGELOG.md"]}
            }),
            serde_json::json!({
                "id": "docs-scope",
                "kind": "diff_scope",
                "description": "Documentation-only task should stay inside docs and top-level docs files.",
                "diff_scope": {"allowed_paths": ["docs", "README.md", "CHANGELOG.md"]}
            }),
        ],
        OutcomeTemplateKind::Generic => vec![
            serde_json::json!({
                "id": "expected-files-changed",
                "kind": "diff_touches",
                "description": "Replace paths with the files or directories this task must modify.",
                "diff_touches": {"paths": ["CHANGE_ME"]}
            }),
            serde_json::json!({
                "id": "allowed-scope",
                "kind": "diff_scope",
                "description": "Replace allowed_paths with the safe task boundary.",
                "diff_scope": {"allowed_paths": ["CHANGE_ME"]}
            }),
        ],
    };
    if options.include_judgment {
        criteria.push(serde_json::json!({
            "id": "external-review",
            "kind": "judgment",
            "description": "External reviewer accepts the outcome.",
            "judgment": {"rubric": "Reviewer confirms the result satisfies the user-visible task goal."}
        }));
    }
    serde_json::json!({
        "schema_version": "mneme.acceptance.v1",
        "task_id": task_id,
        "criteria": criteria
    })
}

fn criterion_config_for_kind(criterion: &RawAcceptanceCriterion) -> serde_json::Value {
    match criterion.kind {
        AcceptanceCriterionKind::Command => criterion.command.clone(),
        AcceptanceCriterionKind::DiffTouches => criterion.diff_touches.clone(),
        AcceptanceCriterionKind::DiffScope => criterion.diff_scope.clone(),
        AcceptanceCriterionKind::SymbolPresent => criterion.symbol_present.clone(),
        AcceptanceCriterionKind::Judgment => criterion.judgment.clone(),
    }
}

fn capture_acceptance_baseline() -> Result<AcceptanceBaseline, CliError> {
    let worktree = env::current_dir()
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))?;
    let mut baseline = AcceptanceBaseline {
        git_head: "unknown".to_owned(),
        diff_base: "unknown".to_owned(),
        worktree: worktree.display().to_string(),
        dirty: false,
        warnings: Vec::new(),
    };
    match run_git_output(&worktree, &["rev-parse", "HEAD"]) {
        Ok(head) if !head.trim().is_empty() => {
            baseline.git_head = head.trim().to_owned();
            baseline.diff_base = baseline.git_head.clone();
        }
        _ => baseline.warnings.push("git_head_unavailable".to_owned()),
    }
    match run_git_output(&worktree, &["status", "--porcelain"]) {
        Ok(status) => {
            baseline.dirty = !status.trim().is_empty();
            if baseline.dirty {
                baseline.warnings.push("dirty_worktree_at_begin".to_owned());
            }
        }
        _ => baseline.warnings.push("git_status_unavailable".to_owned()),
    }
    Ok(baseline)
}

fn run_git_output(worktree: &Path, args: &[&str]) -> Result<String, CliError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(worktree)
        .output()
        .map_err(|source| CliError::io("run git", worktree, source))?;
    if !output.status.success() {
        return Err(CliError::lifecycle(format!(
            "git {} failed with status {}",
            args.join(" "),
            output.status
        )));
    }
    String::from_utf8(output.stdout).map_err(|source| {
        CliError::lifecycle(format!(
            "git {} returned non-utf8 output: {source}",
            args.join(" ")
        ))
    })
}

fn load_or_run_verifier_report(
    store_path: &Path,
    engine: &MnemeEngine,
    session_id: &str,
    options: &EndOptions,
) -> Result<Option<VerifierReport>, CliError> {
    if let Some(path) = &options.verifier_report_path {
        let text = std::fs::read_to_string(path)
            .map_err(|source| CliError::io("read verifier report", path, source))?;
        let report = serde_json::from_str(&text)
            .map_err(|source| CliError::json_file("parse", path, source))?;
        return Ok(Some(report));
    }
    let program = match &options.verifier {
        VerifierOptions::None => env::var("MNEME_VERIFIER_COMMAND")
            .ok()
            .filter(|value| !value.trim().is_empty()),
        VerifierOptions::Command { program, .. } => program.clone().or_else(|| {
            env::var("MNEME_VERIFIER_COMMAND")
                .ok()
                .filter(|value| !value.trim().is_empty())
        }),
    };
    let Some(program) = program else {
        return Ok(None);
    };
    let args = match &options.verifier {
        VerifierOptions::Command { args, .. } => args.clone(),
        VerifierOptions::None => Vec::new(),
    };
    run_verifier_command(store_path, engine, session_id, &program, &args).map(Some)
}

fn load_or_build_judgment_report(
    options: &OutcomeJudgeOptions,
) -> Result<OutcomeJudgmentReport, CliError> {
    if let Some(path) = &options.judgment_report_path {
        if options.id.is_some() || options.verdict.is_some() || options.evidence.is_some() {
            return Err(CliError::invalid_cli(
                "--judgment-report cannot be combined with inline --id/--verdict/--evidence",
            ));
        }
        let text = std::fs::read_to_string(path)
            .map_err(|source| CliError::io("read judgment report", path, source))?;
        return serde_json::from_str(&text)
            .map_err(|source| CliError::json_file("parse", path, source));
    }
    let id = options
        .id
        .clone()
        .ok_or_else(|| CliError::invalid_cli("outcome judge requires --id or --judgment-report"))?;
    let verdict = options.verdict.ok_or_else(|| {
        CliError::invalid_cli("outcome judge requires --verdict pass|fail, --pass, or --fail")
    })?;
    Ok(OutcomeJudgmentReport {
        schema_version: "mneme.judgment.v1".to_owned(),
        task_id: options.task_id.clone(),
        reviewer: options.reviewer.clone(),
        results: vec![OutcomeJudgmentCriterionResult {
            id,
            verdict,
            evidence: options.evidence.clone(),
        }],
    })
}

fn run_verifier_command(
    store_path: &Path,
    engine: &MnemeEngine,
    session_id: &str,
    program: &str,
    args: &[String],
) -> Result<VerifierReport, CliError> {
    let snapshot = engine.snapshot();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .ok_or_else(|| CliError::lifecycle(format!("unknown session: {session_id}")))?;
    let acceptance = session.acceptance.as_ref().ok_or_else(|| {
        CliError::lifecycle(format!(
            "session {session_id} has no acceptance contract for verifier command"
        ))
    })?;
    let workspace = env::current_dir()
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))?;
    let request = VerifierCommandRequest {
        schema_version: "mneme.verifier_request.v1",
        store: store_path.display().to_string(),
        workspace: workspace.display().to_string(),
        session_id,
        session,
        acceptance,
    };
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| CliError::io("spawn verifier command", Path::new(program), source))?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| CliError::lifecycle("verifier command stdin unavailable"))?;
        serde_json::to_writer(stdin, &request).map_err(CliError::json)?;
    }
    let output = child
        .wait_with_output()
        .map_err(|source| CliError::io("wait verifier command", Path::new(program), source))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::lifecycle(format!(
            "verifier command failed with status {}: {}",
            output.status,
            stderr.trim()
        )));
    }
    serde_json::from_slice(&output.stdout).map_err(CliError::json)
}

fn gate_blocks_completion(gate_result: &Option<OutcomeGateResult>) -> bool {
    gate_result.as_ref().is_some_and(|gate| !gate.completed)
}

fn default_store_path() -> Result<PathBuf, CliError> {
    env::current_dir()
        .map(|dir| dir.join(".mneme").join("mneme-v1.json"))
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))
}

fn default_team_store_path() -> Result<PathBuf, CliError> {
    env::current_dir()
        .map(|dir| dir.join(".mneme").join("mneme-team-v2.json"))
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))
}

fn build_mcp_snippets(
    options: &McpConfigOptions,
    v1_store: &Path,
    team_store: &Path,
) -> Result<Vec<McpClientSnippet>, CliError> {
    let mut snippets = Vec::new();
    for client in options.client.selected() {
        snippets.push(match client {
            McpClient::Codex => build_codex_mcp_snippet(options, v1_store, team_store)?,
            McpClient::ClaudeCode => build_claude_code_mcp_snippet(options, v1_store, team_store),
            McpClient::Cursor => build_cursor_mcp_snippet(options, v1_store, team_store)?,
            McpClient::All => unreachable!("selected() never returns All"),
        });
    }
    Ok(snippets)
}

fn build_codex_mcp_snippet(
    options: &McpConfigOptions,
    v1_store: &Path,
    team_store: &Path,
) -> Result<McpClientSnippet, CliError> {
    let args = mcp_server_args(options, v1_store, team_store);
    let args_literal = args
        .iter()
        .map(|arg| quoted_string(arg))
        .collect::<Result<Vec<_>, _>>()?
        .join(", ");
    let snippet = format!(
        "[mcp_servers.mneme]\ncommand = {}\nargs = [{}]\n",
        quoted_string(&options.mcp_bin)?,
        args_literal
    );
    Ok(McpClientSnippet {
        client: "codex",
        description: "Add this table to the Codex MCP config file.",
        format: "toml",
        snippet,
    })
}

fn build_claude_code_mcp_snippet(
    options: &McpConfigOptions,
    v1_store: &Path,
    team_store: &Path,
) -> McpClientSnippet {
    let command = mcp_server_command(options, v1_store, team_store);
    McpClientSnippet {
        client: "claude-code",
        description: "Run this command to register Mneme as a user-scoped stdio MCP server.",
        format: "shell",
        snippet: format!("claude mcp add --transport stdio --scope user mneme -- {command}"),
    }
}

fn build_cursor_mcp_snippet(
    options: &McpConfigOptions,
    v1_store: &Path,
    team_store: &Path,
) -> Result<McpClientSnippet, CliError> {
    let args = mcp_server_args(options, v1_store, team_store);
    let config = serde_json::json!({
        "mcpServers": {
            "mneme": {
                "command": options.mcp_bin,
                "args": args,
            }
        }
    });
    Ok(McpClientSnippet {
        client: "cursor",
        description: "Add this object to Cursor MCP configuration.",
        format: "json",
        snippet: serde_json::to_string_pretty(&config).map_err(CliError::json)?,
    })
}

fn mcp_server_args(options: &McpConfigOptions, v1_store: &Path, team_store: &Path) -> Vec<String> {
    vec![
        "--mode".to_owned(),
        options.mode.clone(),
        "--v1-store".to_owned(),
        v1_store.display().to_string(),
        "--team-store".to_owned(),
        team_store.display().to_string(),
    ]
}

fn mcp_server_command(options: &McpConfigOptions, v1_store: &Path, team_store: &Path) -> String {
    let mut parts = vec![options.mcp_bin.clone()];
    parts.extend(mcp_server_args(options, v1_store, team_store));
    parts
        .iter()
        .map(|part| shell_quote(part))
        .collect::<Vec<_>>()
        .join(" ")
}

fn quoted_string(value: &str) -> Result<String, CliError> {
    serde_json::to_string(value).map_err(CliError::json)
}

fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '_' | '-' | ':' | '='))
    {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', "'\"'\"'"))
    }
}

fn default_init_config_path(store_path: &Path) -> PathBuf {
    store_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join("mneme-agent-hook.env")
}

fn build_doctor_report(options: &DoctorOptions) -> Result<DoctorReport, CliError> {
    let workspace = env::current_dir()
        .map_err(|source| CliError::io("read current dir", Path::new("."), source))?;
    let default_store = default_store_path()?;
    let store_path = resolve_store_path(&options.common)?;
    let config_path = options
        .config_path
        .clone()
        .unwrap_or_else(|| default_init_config_path(&store_path));
    let store = JsonFileStore::new(store_path.clone());
    let inspection = store.inspect();
    let profile = inspect_agent_hook_profile(&config_path, &store_path, &workspace);
    let checks = doctor_checks(&inspection, &profile);
    let recommendations = doctor_recommendations(&inspection, &profile);
    let ok = checks.iter().all(|check| check.status == "pass");
    Ok(DoctorReport {
        command: "doctor",
        ok,
        version: env!("CARGO_PKG_VERSION"),
        build_stage: BuildStage::PersonalCoreV1.as_str(),
        workspace: workspace.display().to_string(),
        default_store: default_store.display().to_string(),
        store: inspection,
        profile,
        checks,
        recommendations,
    })
}

fn inspect_agent_hook_profile(
    path: &Path,
    store_path: &Path,
    workspace: &Path,
) -> AgentHookProfileInspection {
    let mut inspection = AgentHookProfileInspection {
        path: path.display().to_string(),
        status: "missing",
        loaded: false,
        values: AgentHookProfileValues::default(),
        issues: Vec::new(),
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        if path.exists() {
            inspection.status = "invalid";
            inspection
                .issues
                .push("profile exists but could not be read".to_owned());
        }
        return inspection;
    };
    inspection.loaded = true;
    inspection.status = "valid";
    for (line_idx, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            inspection
                .issues
                .push(format!("line {} is not KEY=VALUE", line_idx + 1));
            continue;
        };
        let value = strip_optional_profile_quotes(raw_value);
        match key {
            "MNEME_BIN" => assign_profile_value(
                &mut inspection.values.mneme_bin,
                value,
                key,
                &mut inspection.issues,
            ),
            "MNEME_STORE" => assign_profile_value(
                &mut inspection.values.mneme_store,
                value,
                key,
                &mut inspection.issues,
            ),
            "MNEME_AGENT_ID" => assign_profile_value(
                &mut inspection.values.mneme_agent_id,
                value,
                key,
                &mut inspection.issues,
            ),
            "MNEME_SCOPE" => assign_profile_value(
                &mut inspection.values.mneme_scope,
                value,
                key,
                &mut inspection.issues,
            ),
            "MNEME_MAX_ITEMS" => assign_profile_value(
                &mut inspection.values.mneme_max_items,
                value,
                key,
                &mut inspection.issues,
            ),
            "MNEME_EXTRACTOR_COMMAND" => assign_profile_value(
                &mut inspection.values.mneme_extractor_command,
                value,
                key,
                &mut inspection.issues,
            ),
            "MNEME_VERIFIER_COMMAND" => assign_profile_value(
                &mut inspection.values.mneme_verifier_command,
                value,
                key,
                &mut inspection.issues,
            ),
            unknown => inspection
                .issues
                .push(format!("unknown profile key: {unknown}")),
        }
    }
    validate_profile_values(&mut inspection, store_path, workspace);
    if !inspection.issues.is_empty() {
        inspection.status = "invalid";
    }
    inspection
}

fn strip_optional_profile_quotes(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_owned()
    } else {
        trimmed.to_owned()
    }
}

fn assign_profile_value(
    target: &mut Option<String>,
    value: String,
    key: &str,
    issues: &mut Vec<String>,
) {
    if target.is_some() {
        issues.push(format!("duplicate profile key: {key}"));
    }
    if value.trim().is_empty() {
        issues.push(format!("{key} must not be empty"));
    }
    *target = Some(value);
}

fn validate_profile_values(
    inspection: &mut AgentHookProfileInspection,
    store_path: &Path,
    workspace: &Path,
) {
    let values = &inspection.values;
    let Some(profile_store) = &values.mneme_store else {
        inspection.issues.push("MNEME_STORE is missing".to_owned());
        return;
    };
    if values.mneme_agent_id.is_none() {
        inspection
            .issues
            .push("MNEME_AGENT_ID is missing".to_owned());
    }
    if values.mneme_scope.is_none() {
        inspection.issues.push("MNEME_SCOPE is missing".to_owned());
    }
    match &values.mneme_max_items {
        Some(value) => match value.parse::<usize>() {
            Ok(0) => inspection
                .issues
                .push("MNEME_MAX_ITEMS must be greater than zero".to_owned()),
            Ok(_) => {}
            Err(source) => inspection.issues.push(format!(
                "MNEME_MAX_ITEMS is not a positive integer: {source}"
            )),
        },
        None => inspection
            .issues
            .push("MNEME_MAX_ITEMS is missing".to_owned()),
    }
    let profile_store_path = profile_value_path(profile_store, workspace);
    if !paths_equivalent_or_equal(&profile_store_path, store_path) {
        inspection.issues.push(format!(
            "MNEME_STORE points to {}, expected {}",
            profile_store_path.display(),
            store_path.display()
        ));
    }
    if let Some(bin) = &values.mneme_bin {
        let bin_path = profile_value_path(bin, workspace);
        if !bin_path.is_file() {
            inspection.issues.push(format!(
                "MNEME_BIN is not an executable file: {}",
                bin_path.display()
            ));
        }
    }
    if let Some(command) = &values.mneme_extractor_command {
        if looks_like_profile_path(command) {
            let command_path = profile_value_path(command, workspace);
            if !command_path.is_file() {
                inspection.issues.push(format!(
                    "MNEME_EXTRACTOR_COMMAND is not an executable file: {}",
                    command_path.display()
                ));
            }
        }
    }
    if let Some(command) = &values.mneme_verifier_command {
        if looks_like_profile_path(command) {
            let command_path = profile_value_path(command, workspace);
            if !command_path.is_file() {
                inspection.issues.push(format!(
                    "MNEME_VERIFIER_COMMAND is not an executable file: {}",
                    command_path.display()
                ));
            }
        }
    }
}

fn profile_value_path(value: &str, workspace: &Path) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        workspace.join(path)
    }
}

fn looks_like_profile_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\')
}

fn paths_equivalent_or_equal(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left.display().to_string() == right.display().to_string(),
    }
}

fn doctor_checks(
    inspection: &StoreInspection,
    profile: &AgentHookProfileInspection,
) -> Vec<DoctorCheckReport> {
    let store_check = match inspection.current.status {
        StoreFileStatus::Valid => DoctorCheckReport {
            name: "store.current",
            status: "pass",
            detail: "store is valid".to_owned(),
        },
        StoreFileStatus::Missing => DoctorCheckReport {
            name: "store.current",
            status: "fail",
            detail: "store is missing".to_owned(),
        },
        StoreFileStatus::Invalid => DoctorCheckReport {
            name: "store.current",
            status: "fail",
            detail: inspection
                .current
                .error
                .clone()
                .unwrap_or_else(|| "store is invalid".to_owned()),
        },
    };
    let profile_check = match profile.status {
        "valid" => DoctorCheckReport {
            name: "profile.agent_hook",
            status: "pass",
            detail: "agent hook profile is valid".to_owned(),
        },
        "missing" => DoctorCheckReport {
            name: "profile.agent_hook",
            status: "fail",
            detail: "agent hook profile is missing".to_owned(),
        },
        _ => DoctorCheckReport {
            name: "profile.agent_hook",
            status: "fail",
            detail: profile.issues.join("; "),
        },
    };
    vec![store_check, profile_check]
}

fn doctor_recommendations(
    inspection: &StoreInspection,
    profile: &AgentHookProfileInspection,
) -> Vec<String> {
    let mut recommendations = Vec::new();
    if inspection.current.status == StoreFileStatus::Missing || profile.status == "missing" {
        recommendations
            .push("run `mneme init` to create the local store and hook profile".to_owned());
    }
    if inspection.current.status == StoreFileStatus::Invalid && inspection.repair_available {
        recommendations.push("run `mneme repair` to restore the store from backup".to_owned());
    } else if inspection.current.status == StoreFileStatus::Invalid {
        recommendations.push(
            "inspect the store or run `mneme init --force` only if overwriting is intentional"
                .to_owned(),
        );
    }
    if profile.status == "invalid" {
        recommendations
            .push("run `mneme init --force` to regenerate the agent hook profile".to_owned());
    }
    recommendations
}

fn repair_check_action(inspection: &StoreInspection) -> &'static str {
    if inspection.current.status == StoreFileStatus::Valid {
        if store_file_needs_normalization(&inspection.current) {
            "normalization_available"
        } else {
            "current_valid"
        }
    } else if inspection.repair_available {
        "repair_available"
    } else {
        "backup_unavailable"
    }
}

fn repair_action_ok(action: &str) -> bool {
    matches!(
        action,
        "current_valid"
            | "normalization_available"
            | "normalized_current"
            | "repair_available"
            | "restored_from_backup"
    )
}

fn restore_check_action(inspection: &StoreInspection) -> &'static str {
    if restore_available(inspection) {
        "restore_available"
    } else {
        "backup_unavailable"
    }
}

fn restore_available(inspection: &StoreInspection) -> bool {
    inspection.backup.status == StoreFileStatus::Valid
}

fn restore_action_ok(action: &str) -> bool {
    matches!(action, "restore_available" | "restored_from_backup")
}

fn store_file_needs_normalization(file: &StoreFileInspection) -> bool {
    file.validation.as_ref().is_some_and(|validation| {
        validation.issues.iter().any(|issue| {
            issue.severity == ValidationSeverity::Warning
                && matches!(
                    issue.code.as_str(),
                    "schema.legacy"
                        | "schema.old"
                        | "metadata.store_id_missing"
                        | "metadata.generation_zero"
                )
        })
    })
}

fn repair_recommendations(action: &str, store_path: &Path) -> Vec<String> {
    match action {
        "current_valid" => vec!["no repair is needed; the current store is valid".to_owned()],
        "normalization_available" => vec![format!(
            "run `mneme repair --store \"{}\"` to normalize legacy-compatible schema metadata",
            store_path.display()
        )],
        "normalized_current" => vec![
            "legacy-compatible schema metadata was normalized and the previous file was kept as backup"
                .to_owned(),
            format!(
                "run `mneme validate --store \"{}\"` to verify the normalized store",
                store_path.display()
            ),
        ],
        "repair_available" => vec![format!(
            "run `mneme repair --store \"{}\"` to restore the current store from backup",
            store_path.display()
        )],
        "restored_from_backup" => vec![format!(
            "run `mneme validate --store \"{}\"` to verify the restored store",
            store_path.display()
        )],
        "backup_unavailable" => vec![
            "no valid backup is available for automatic repair".to_owned(),
            "inspect the store manually or run `mneme init --force` only if overwriting is intentional"
                .to_owned(),
        ],
        _ => Vec::new(),
    }
}

fn restore_recommendations(action: &str, store_path: &Path) -> Vec<String> {
    match action {
        "restore_available" => vec![format!(
            "run `mneme restore --store \"{}\"` to replace current with backup; current will become the new backup",
            store_path.display()
        )],
        "restored_from_backup" => vec![
            format!(
                "run `mneme validate --store \"{}\"` to verify the restored store",
                store_path.display()
            ),
            format!(
                "run `mneme restore --store \"{}\"` again to swap back to the pre-restore current store",
                store_path.display()
            ),
        ],
        "backup_unavailable" => vec![
            "no valid backup is available for restore".to_owned(),
            "run `mneme validate` and inspect the store before making further changes".to_owned(),
        ],
        _ => Vec::new(),
    }
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
    extractor_command: Option<&str>,
) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create dir", parent, source))?;
    }
    let profile = render_agent_hook_profile(
        store_path,
        agent_id,
        scope,
        max_items,
        bin_path,
        extractor_command,
    )?;
    std::fs::write(path, profile).map_err(|source| CliError::io("write", path, source))
}

fn render_agent_hook_profile(
    store_path: &Path,
    agent_id: &str,
    scope: &str,
    max_items: usize,
    bin_path: Option<&Path>,
    extractor_command: Option<&str>,
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
    match extractor_command {
        Some(command) => {
            let command_value = single_line_value(command.to_owned(), "extractor command")?;
            profile.push_str(&format!("MNEME_EXTRACTOR_COMMAND={command_value}\n"));
        }
        None => {
            profile.push_str("# Optional session-end command extractor.\n");
            profile.push_str("# MNEME_EXTRACTOR_COMMAND=./mneme-extractor-wrapper\n");
        }
    }
    profile.push_str("# Optional session-end outcome verifier for gated sessions.\n");
    profile.push_str("# MNEME_VERIFIER_COMMAND=scripts/mneme-outcome-verifier.py\n");
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
    persist_engine_once(path, engine)
        .map_err(|source| CliError::store_error("save store", path, source))
}

fn persist_engine_once(path: &Path, engine: &MnemeEngine) -> Result<(), StoreError> {
    let mut store = JsonFileStore::new(path.to_path_buf());
    engine.persist(&mut store)
}

fn retry_store_mutation<T>(
    path: &Path,
    mut operation: impl FnMut() -> Result<T, CliError>,
) -> Result<T, CliError> {
    let mut last_error: Option<CliError> = None;
    for attempt in 0..STORE_MUTATION_RETRY_ATTEMPTS {
        match operation() {
            Ok(value) => return Ok(value),
            Err(error) if matches!(error.kind, CliErrorKind::StoreLock) => {
                last_error = Some(error);
                let delay_ms = STORE_MUTATION_RETRY_BASE_MS
                    .saturating_mul((attempt as u64 % 10).saturating_add(1));
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        CliError::store(
            "save store",
            path,
            "store stayed locked until retry budget was exhausted",
        )
    }))
}

fn load_team_engine(path: &Path) -> Result<TeamMemoryEngine, CliError> {
    let store = JsonTeamFileStore::new(path.to_path_buf());
    TeamMemoryEngine::from_store(TeamMemoryConfig::default(), &store)
        .map_err(|source| CliError::store("load team store", path, source))
}

fn persist_team_engine(path: &Path, engine: &TeamMemoryEngine) -> Result<(), CliError> {
    let mut last_error: Option<CliError> = None;
    for attempt in 0..STORE_MUTATION_RETRY_ATTEMPTS {
        match persist_team_engine_once(path, engine) {
            Ok(()) => return Ok(()),
            Err(source) if team_store_error_is_lock_conflict(&source) => {
                last_error = Some(CliError::store("save team store", path, source));
                let delay_ms = STORE_MUTATION_RETRY_BASE_MS
                    .saturating_mul((attempt as u64 % 10).saturating_add(1));
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(source) => return Err(CliError::store("save team store", path, source)),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        CliError::store(
            "save team store",
            path,
            "team store stayed locked until retry budget was exhausted",
        )
    }))
}

fn persist_team_engine_once(
    path: &Path,
    engine: &TeamMemoryEngine,
) -> Result<(), mneme_core::TeamStoreError> {
    let mut store = JsonTeamFileStore::new(path.to_path_buf());
    engine.persist(&mut store)
}

fn team_store_error_is_lock_conflict(error: &mneme_core::TeamStoreError) -> bool {
    error.to_string().contains("team store lock already exists")
}

fn team_policy_error(source: impl Display) -> CliError {
    CliError::lifecycle(format!("team policy: {source}"))
}

fn required_team_actor(options: &TeamActorOptions) -> Result<TeamActor, CliError> {
    let user_id = options
        .actor_user_id
        .clone()
        .ok_or_else(|| CliError::invalid_cli("team operation requires --actor <user>"))?;
    Ok(TeamActor {
        user_id,
        agent_id: options.actor_agent_id.clone(),
    })
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

fn write_team_sync_envelope(path: &Path, envelope: &TeamSyncEnvelope) -> Result<(), CliError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)
            .map_err(|source| CliError::io("create dir", parent, source))?;
    }
    let json = serde_json::to_string_pretty(envelope).map_err(CliError::json)?;
    std::fs::write(path, format!("{json}\n")).map_err(|source| CliError::io("write", path, source))
}

fn read_team_sync_envelope(path: &Path) -> Result<TeamSyncEnvelope, CliError> {
    let text =
        std::fs::read_to_string(path).map_err(|source| CliError::io("read", path, source))?;
    serde_json::from_str(&text).map_err(|source| CliError::json_file("parse", path, source))
}

fn build_memory_quality_report(
    store_path: &Path,
    claims: &[ClaimRecord],
    redact_sensitive: bool,
) -> MemoryQualityReport {
    let active_claim_count = count_claims_with_status(claims, ClaimStatus::Active);
    let blocked_secret_claim_count = count_claims_with_status(claims, ClaimStatus::BlockedSecret);
    let superseded_claim_count = count_claims_with_status(claims, ClaimStatus::Superseded);
    let forgotten_claim_count = count_claims_with_status(claims, ClaimStatus::Forgotten);
    let inactive_claim_count = superseded_claim_count + forgotten_claim_count;
    let mut findings = Vec::new();
    let mut review_queue = Vec::new();
    let mut duplicate_active_group_count = 0;
    let mut duplicate_active_claim_count = 0;

    let mut active_groups = BTreeMap::<String, Vec<&ClaimRecord>>::new();
    for claim in claims
        .iter()
        .filter(|claim| claim.status == ClaimStatus::Active)
    {
        active_groups
            .entry(quality_claim_key(claim))
            .or_default()
            .push(claim);
    }
    for group in active_groups.values().filter(|group| group.len() > 1) {
        duplicate_active_group_count += 1;
        duplicate_active_claim_count += group.len();
        let claim_ids = group
            .iter()
            .map(|claim| claim.id.clone())
            .collect::<Vec<_>>();
        let scope = group.first().map(|claim| claim.scope.clone());
        findings.push(MemoryQualityFinding {
            kind: "duplicate_active".to_owned(),
            severity: "review".to_owned(),
            claim_ids: claim_ids.clone(),
            detail: format!("{} active claims have the same text and scope", group.len()),
            recommendation: "review duplicate IDs and forget the redundant active claims"
                .to_owned(),
        });
        let mut suggested_commands = vec![format!(
            "mneme claims --status active --store \"{}\" --json",
            store_path.display()
        )];
        suggested_commands.extend(group.iter().skip(1).map(|claim| {
            format!(
                "mneme forget --claim-id {} --store \"{}\"",
                claim.id,
                store_path.display()
            )
        }));
        review_queue.push(MemoryReviewQueueItem {
            kind: "duplicate_active".to_owned(),
            priority: "high".to_owned(),
            claim_ids,
            status: Some(ClaimStatus::Active.as_str().to_owned()),
            scope,
            claim_text: group
                .first()
                .map(|claim| quality_claim_text(claim, redact_sensitive)),
            reason: "multiple active claims currently express the same memory".to_owned(),
            suggested_commands,
        });
    }

    let blocked_claims = claims
        .iter()
        .filter(|claim| claim.status == ClaimStatus::BlockedSecret)
        .collect::<Vec<_>>();
    if !blocked_claims.is_empty() {
        let claim_ids = blocked_claims
            .iter()
            .map(|claim| claim.id.clone())
            .collect::<Vec<_>>();
        findings.push(MemoryQualityFinding {
            kind: "blocked_secret".to_owned(),
            severity: "privacy".to_owned(),
            claim_ids: claim_ids.clone(),
            detail: format!("{} claims are blocked as secret-like", blocked_claims.len()),
            recommendation:
                "confirm these claims stay blocked, or compact them after local privacy review"
                    .to_owned(),
        });
        for claim in blocked_claims {
            review_queue.push(MemoryReviewQueueItem {
                kind: "blocked_secret".to_owned(),
                priority: "high".to_owned(),
                claim_ids: vec![claim.id.clone()],
                status: Some(ClaimStatus::BlockedSecret.as_str().to_owned()),
                scope: Some(claim.scope.clone()),
                claim_text: Some(quality_claim_text(claim, redact_sensitive)),
                reason: "secret-like memory is retained but excluded from active context"
                    .to_owned(),
                suggested_commands: vec![
                    format!(
                        "mneme claims --status blocked_secret --store \"{}\" --json",
                        store_path.display()
                    ),
                    format!(
                        "mneme curate --apply --compact --store \"{}\"",
                        store_path.display()
                    ),
                ],
            });
        }
    }

    if inactive_claim_count > 0 {
        let inactive_ids = claims
            .iter()
            .filter(|claim| {
                matches!(
                    claim.status,
                    ClaimStatus::Superseded | ClaimStatus::Forgotten
                )
            })
            .map(|claim| claim.id.clone())
            .collect::<Vec<_>>();
        findings.push(MemoryQualityFinding {
            kind: "inactive_history".to_owned(),
            severity: "cleanup".to_owned(),
            claim_ids: inactive_ids.clone(),
            detail: format!("{inactive_claim_count} inactive claims remain in history"),
            recommendation:
                "export a review artifact, then run compact when historical inactive claims are no longer needed"
                    .to_owned(),
        });
        review_queue.push(MemoryReviewQueueItem {
            kind: "inactive_history".to_owned(),
            priority: "medium".to_owned(),
            claim_ids: inactive_ids,
            status: None,
            scope: None,
            claim_text: None,
            reason: "superseded or forgotten claims are retained for audit until compaction"
                .to_owned(),
            suggested_commands: vec![
                format!(
                    "mneme review /tmp/mneme-review.md --store \"{}\"",
                    store_path.display()
                ),
                format!("mneme compact --store \"{}\"", store_path.display()),
            ],
        });
    }

    let mut next_commands = vec![
        format!("mneme claims --store \"{}\" --json", store_path.display()),
        format!(
            "mneme review /tmp/mneme-review.md --store \"{}\"",
            store_path.display()
        ),
    ];
    if inactive_claim_count > 0 {
        next_commands.push(format!(
            "mneme compact --store \"{}\"",
            store_path.display()
        ));
    }
    if blocked_secret_claim_count > 0 || duplicate_active_claim_count > 0 {
        next_commands.push(format!(
            "mneme curate --store \"{}\" --json",
            store_path.display()
        ));
    }
    if claims.is_empty() {
        next_commands.push(format!(
            "mneme remember \"user prefers ...\" --store \"{}\"",
            store_path.display()
        ));
    }

    let ok = findings.is_empty();
    MemoryQualityReport {
        command: "quality".to_owned(),
        store: store_path.display().to_string(),
        ok,
        health: if ok {
            "ok".to_owned()
        } else {
            "attention_required".to_owned()
        },
        claim_count: claims.len(),
        active_claim_count,
        blocked_secret_claim_count,
        superseded_claim_count,
        forgotten_claim_count,
        inactive_claim_count,
        duplicate_active_group_count,
        duplicate_active_claim_count,
        review_item_count: review_queue.len(),
        findings,
        review_queue,
        next_commands,
    }
}

fn build_memory_curation_plan(
    store_path: &Path,
    claims: &[ClaimRecord],
    compact_requested: bool,
    redact_sensitive: bool,
) -> MemoryCurationPlan {
    let mut actions = Vec::new();
    let mut duplicate_forget_count = 0;
    let mut blocked_secret_review_count = 0;
    let mut compact_target_ids = Vec::new();

    let mut active_groups = BTreeMap::<String, Vec<&ClaimRecord>>::new();
    for claim in claims
        .iter()
        .filter(|claim| claim.status == ClaimStatus::Active)
    {
        active_groups
            .entry(quality_claim_key(claim))
            .or_default()
            .push(claim);
    }

    for group in active_groups.values().filter(|group| group.len() > 1) {
        let kept_claim = group[0];
        let duplicate_claims = group.iter().skip(1).copied().collect::<Vec<_>>();
        let duplicate_ids = duplicate_claims
            .iter()
            .map(|claim| claim.id.clone())
            .collect::<Vec<_>>();
        duplicate_forget_count += duplicate_ids.len();
        compact_target_ids.extend(duplicate_ids.iter().cloned());
        actions.push(MemoryCurationAction {
            kind: "forget_duplicate_active".to_owned(),
            status: "planned".to_owned(),
            claim_ids: duplicate_ids,
            kept_claim_id: Some(kept_claim.id.clone()),
            claim_text: Some(quality_claim_text(kept_claim, redact_sensitive)),
            reason: "keep the earliest active duplicate and forget redundant active copies"
                .to_owned(),
            safety: "deterministic_active_claim_only".to_owned(),
        });
    }

    for claim in claims
        .iter()
        .filter(|claim| claim.status == ClaimStatus::BlockedSecret)
    {
        blocked_secret_review_count += 1;
        compact_target_ids.push(claim.id.clone());
        actions.push(MemoryCurationAction {
            kind: "review_blocked_secret".to_owned(),
            status: "manual".to_owned(),
            claim_ids: vec![claim.id.clone()],
            kept_claim_id: None,
            claim_text: Some(quality_claim_text(claim, redact_sensitive)),
            reason: "blocked-secret records stay out of active context; compact only after review"
                .to_owned(),
            safety: "manual_privacy_review".to_owned(),
        });
    }

    compact_target_ids.extend(
        claims
            .iter()
            .filter(|claim| {
                matches!(
                    claim.status,
                    ClaimStatus::Superseded | ClaimStatus::Forgotten
                )
            })
            .map(|claim| claim.id.clone()),
    );
    dedupe_strings(&mut compact_target_ids);

    let compact_recommended = !compact_target_ids.is_empty();
    if compact_recommended {
        actions.push(MemoryCurationAction {
            kind: "compact_non_active_records".to_owned(),
            status: if compact_requested {
                "planned".to_owned()
            } else {
                "available".to_owned()
            },
            claim_ids: compact_target_ids.clone(),
            kept_claim_id: None,
            claim_text: None,
            reason: "remove blocked-secret, superseded, and forgotten records after review"
                .to_owned(),
            safety: "requires_explicit_compact".to_owned(),
        });
    }

    let applyable_action_count = actions
        .iter()
        .filter(|action| {
            action.kind == "forget_duplicate_active"
                || (action.kind == "compact_non_active_records" && compact_requested)
        })
        .count();
    let manual_action_count = actions
        .iter()
        .filter(|action| action.kind == "review_blocked_secret")
        .count();
    let mut next_commands = Vec::new();
    if duplicate_forget_count > 0 {
        next_commands.push(format!(
            "mneme curate --apply --store \"{}\"",
            store_path.display()
        ));
    }
    if compact_recommended {
        next_commands.push(format!(
            "mneme curate --apply --compact --store \"{}\"",
            store_path.display()
        ));
    }
    if actions.is_empty() {
        next_commands.push(format!(
            "mneme quality --store \"{}\" --json",
            store_path.display()
        ));
    } else {
        next_commands.push(format!(
            "mneme review /tmp/mneme-review.md --store \"{}\"",
            store_path.display()
        ));
    }

    MemoryCurationPlan {
        action_count: actions.len(),
        applyable_action_count,
        manual_action_count,
        duplicate_forget_count,
        compact_target_count: compact_target_ids.len(),
        blocked_secret_review_count,
        compact_recommended,
        compact_requested,
        actions,
        next_commands,
    }
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = std::collections::BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

fn quality_claim_key(claim: &ClaimRecord) -> String {
    [
        normalize_quality_value(&claim.subject),
        normalize_quality_value(&claim.predicate),
        normalize_quality_value(&claim.object),
        normalize_quality_value(&claim.scope),
    ]
    .join("\u{1f}")
}

fn normalize_quality_value(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn quality_claim_text(claim: &ClaimRecord, redact_sensitive: bool) -> String {
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
    format!("{subject} {predicate} {object}")
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
    let quality = build_memory_quality_report(store_path, &snapshot.claims, redact_sensitive);
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
        quality,
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

    output.push_str("\n## Memory Quality\n\n");
    output.push_str(&format!("- Health: `{}`\n", report.quality.health));
    output.push_str(&format!(
        "- Findings: `{}`\n",
        report.quality.findings.len()
    ));
    output.push_str(&format!(
        "- Review queue items: `{}`\n",
        report.quality.review_item_count
    ));
    output.push_str(&format!(
        "- Duplicate active groups: `{}`\n",
        report.quality.duplicate_active_group_count
    ));
    output.push_str(&format!(
        "- Blocked secret claims: `{}`\n",
        report.quality.blocked_secret_claim_count
    ));
    output.push_str(&format!(
        "- Inactive claims: `{}`\n",
        report.quality.inactive_claim_count
    ));

    output.push_str("\n### Quality Findings\n\n");
    if report.quality.findings.is_empty() {
        output.push_str("_No quality findings._\n");
    } else {
        output.push_str("| Kind | Severity | Claims | Recommendation |\n");
        output.push_str("| --- | --- | --- | --- |\n");
        for finding in &report.quality.findings {
            output.push_str(&format!(
                "| `{}` | `{}` | {} | {} |\n",
                escape_markdown_cell(&finding.kind),
                escape_markdown_cell(&finding.severity),
                escape_markdown_cell(&finding.claim_ids.join(", ")),
                escape_markdown_cell(&finding.recommendation)
            ));
        }
    }

    output.push_str("\n### Review Queue\n\n");
    if report.quality.review_queue.is_empty() {
        output.push_str("_No review queue items._\n");
    } else {
        output.push_str("| Priority | Kind | Claims | Reason | Suggested Commands |\n");
        output.push_str("| --- | --- | --- | --- | --- |\n");
        for item in &report.quality.review_queue {
            output.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} |\n",
                escape_markdown_cell(&item.priority),
                escape_markdown_cell(&item.kind),
                escape_markdown_cell(&item.claim_ids.join(", ")),
                escape_markdown_cell(&item.reason),
                escape_markdown_cell(&item.suggested_commands.join("; "))
            ));
        }
    }

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

fn emit_doctor_report(
    report: &DoctorReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(writer, "{PRODUCT_NAME} local CLI: {}", report.build_stage)
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(writer, "version: {}", report.version)
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(writer, "workspace: {}", report.workspace)
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(writer, "default store: {}", report.default_store)
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "store: {} ({})",
        report.store.path,
        store_file_status(report.store.current.status)
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "backup: {} ({}, repair_available={})",
        report.store.backup_path,
        store_file_status(report.store.backup.status),
        report.store.repair_available
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "agent hook profile: {} ({})",
        report.profile.path, report.profile.status
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    if let Some(store) = &report.profile.values.mneme_store {
        writeln!(writer, "profile store: {store}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    if let Some(agent_id) = &report.profile.values.mneme_agent_id {
        writeln!(writer, "profile agent: {agent_id}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    if let Some(scope) = &report.profile.values.mneme_scope {
        writeln!(writer, "profile scope: {scope}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    if let Some(max_items) = &report.profile.values.mneme_max_items {
        writeln!(writer, "profile max items: {max_items}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    if let Some(bin) = &report.profile.values.mneme_bin {
        writeln!(writer, "profile bin: {bin}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    if let Some(command) = &report.profile.values.mneme_extractor_command {
        writeln!(writer, "profile extractor command: {command}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    if let Some(command) = &report.profile.values.mneme_verifier_command {
        writeln!(writer, "profile verifier command: {command}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for issue in &report.profile.issues {
        writeln!(writer, "profile issue: {issue}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    writeln!(
        writer,
        "health: {}",
        if report.ok {
            "ok"
        } else {
            "attention_required"
        }
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for recommendation in &report.recommendations {
        writeln!(writer, "recommendation: {recommendation}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn store_file_status(status: StoreFileStatus) -> &'static str {
    match status {
        StoreFileStatus::Missing => "missing",
        StoreFileStatus::Valid => "valid",
        StoreFileStatus::Invalid => "invalid",
    }
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
    if let Some(command) = &report.extractor_command {
        writeln!(writer, "mneme: extractor command {command}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
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

fn emit_quality_report(
    report: &MemoryQualityReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: quality {} (health={}, findings={}, review_items={})",
        report.store,
        report.health,
        report.findings.len(),
        report.review_item_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for finding in &report.findings {
        writeln!(
            writer,
            "- [{}] {}: {} ({})",
            finding.severity,
            finding.kind,
            finding.detail,
            finding.claim_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
        writeln!(writer, "  recommendation: {}", finding.recommendation)
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for item in &report.review_queue {
        writeln!(
            writer,
            "review: [{}] {} ({})",
            item.priority,
            item.kind,
            item.claim_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
        for command in &item.suggested_commands {
            writeln!(writer, "  command: {command}")
                .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
        }
    }
    Ok(())
}

fn emit_curate_report(
    report: &MemoryCurationReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: curate {} (mode={}, actions={}, changed={})",
        report.store, report.mode, report.plan.action_count, report.changed
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "mneme: before health={} findings={} review_items={}",
        report.before.health,
        report.before.findings.len(),
        report.before.review_item_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    if let Some(after) = &report.after {
        writeln!(
            writer,
            "mneme: after health={} findings={} review_items={}",
            after.health,
            after.findings.len(),
            after.review_item_count
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for action in &report.plan.actions {
        writeln!(
            writer,
            "- [{}] {} ({})",
            action.status,
            action.kind,
            action.claim_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
        writeln!(writer, "  safety: {}", action.safety)
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for command in &report.plan.next_commands {
        writeln!(writer, "next: {command}")
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
        "mneme: ended session {} from {} (extractor={}, verifier={}, gate={}, remembered_events={}, remembered_claims={})",
        report.report.session.id,
        report.store,
        report.extractor,
        report.verifier,
        report
            .gate_result
            .as_ref()
            .map(|gate| gate.status.as_str())
            .unwrap_or("none"),
        report.report.remembered_event_ids.len(),
        report.report.remembered_claim_ids.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_failed_gate_cli_report(
    writer: &mut impl Write,
    report: &EndCliReport,
    json: bool,
) -> Result<(), CliError> {
    emit_end_report(report, json, writer)
}

fn emit_outcome_status_report(
    report: &OutcomeStatusReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: outcome {} for {} ({})",
        report.status, report.session_id, report.store
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_outcome_validate_report(
    report: &OutcomeValidateCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: outcome acceptance {} for {} (criteria={}, errors={}, warnings={})",
        if report.validation.ok {
            "valid"
        } else {
            "invalid"
        },
        report.path,
        report.validation.criterion_count,
        report.validation.errors.len(),
        report.validation.warnings.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for error in &report.validation.errors {
        writeln!(writer, "error: {error}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for warning in &report.validation.warnings {
        writeln!(writer, "warning: {warning}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_outcome_template_report(
    report: &OutcomeTemplateCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    if let Some(path) = &report.path {
        return writeln!(
            writer,
            "mneme: wrote outcome template {path} (kind={}, valid={})",
            report.kind, report.validation.ok
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source));
    }
    let text = serde_json::to_string_pretty(&report.acceptance).map_err(CliError::json)?;
    writeln!(writer, "{text}")
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_outcome_judge_report(
    report: &OutcomeJudgeCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: outcome judgment {} for {} (completed={}, {})",
        report.status, report.session_id, report.completed, report.store
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

fn emit_mcp_config_report(
    report: &McpConfigReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: MCP config (mode={}, v1_store={}, team_store={})",
        report.mode, report.v1_store, report.team_store
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for snippet in &report.snippets {
        writeln!(
            writer,
            "\n## {}\n{}\n\n```{}\n{}```",
            snippet.client, snippet.description, snippet.format, snippet.snippet
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    writeln!(writer, "\nNext checks:")
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for command in &report.next_commands {
        writeln!(writer, "- {command}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
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
        "mneme: repair {} (mode={}, action={}, ok={})",
        report.store, report.mode, report.action, report.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "mneme: current={} backup={} repair_available={}",
        store_file_status(report.current_status),
        store_file_status(report.backup_status),
        report.repair_available
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    if let Some(repair) = &report.repair {
        writeln!(
            writer,
            "mneme: repaired={} before={} after={}",
            repair.repaired,
            store_file_status(repair.before.current.status),
            store_file_status(repair.after.current.status)
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for recommendation in &report.recommendations {
        writeln!(writer, "recommendation: {recommendation}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_restore_report(
    report: &RestoreCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: restore {} (mode={}, action={}, ok={})",
        report.store, report.mode, report.action, report.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    writeln!(
        writer,
        "mneme: current={} backup={} restore_available={}",
        store_file_status(report.current_status),
        store_file_status(report.backup_status),
        report.restore_available
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    if let Some(restore) = &report.restore {
        writeln!(
            writer,
            "mneme: restored={} current_preserved_as_backup={}",
            restore.restored, restore.current_preserved_as_backup
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    for recommendation in &report.recommendations {
        writeln!(writer, "recommendation: {recommendation}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_init_report(
    report: &TeamInitReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team initialized {} (workspace={}, users={}, memories={}, audit={})",
        report.store,
        report.workspace_id,
        report.user_count,
        report.memory_count,
        report.audit_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for command in &report.next_commands {
        writeln!(writer, "next: {command}")
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_entity_report<T: Serialize>(
    report: &TeamEntityReport<T>,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: {} {} (validation_ok={}, users={}, agents={}, projects={}, memories={}, audit={})",
        report.command,
        report.store,
        report.validation.ok,
        report.validation.user_count,
        report.validation.agent_count,
        report.validation.project_count,
        report.validation.memory_count,
        report.validation.audit_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_remember_report(
    report: &TeamRememberReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team remembered {} in {} [{}] (validation_ok={})",
        report.memory.id, report.store, report.memory.scope, report.validation.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_context_report(
    report: &TeamContextReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team context {} (actor={}, items={}, omitted={})",
        report.store, report.actor_user_id, report.item_count, report.omitted_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for item in &report.context_pack.items {
        writeln!(
            writer,
            "- {} [{}; {}]",
            item.memory_text,
            item.scope,
            item.source_event_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_handoff_report(
    report: &TeamHandoffCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team handoff {} (actor={}, items={}, sync_memories={}, firewall_ok={})",
        report.store,
        report.actor_user_id,
        report.context_item_count,
        report.sync_memory_count,
        report.firewall_ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for item in &report.package.context_pack.items {
        writeln!(
            writer,
            "- {} [{}; {}]",
            item.memory_text,
            item.scope,
            item.source_event_ids.join(",")
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_run_begin_report(
    report: &TeamRunBeginCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team run began {} (run={}, actor={}, context_items={}, validation_ok={})",
        report.store,
        report.report.run.id,
        report.actor_user_id,
        report.report.context_pack.items.len(),
        report.validation.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_run_note_report(
    report: &TeamRunNoteCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team run note {} (run={}, memory={}, validation_ok={})",
        report.store, report.report.run.id, report.report.memory.id, report.validation.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_run_end_report(
    report: &TeamRunEndCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team run ended {} (run={}, remembered={}, validation_ok={})",
        report.store,
        report.report.run.id,
        report.report.remembered_memory_ids.len(),
        report.validation.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_run_handoff_report(
    report: &TeamRunHandoffCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team run handoff {} (run={}, items={}, sync_memories={}, firewall_ok={})",
        report.store,
        report.run_id,
        report.context_item_count,
        report.sync_memory_count,
        report.firewall_ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_promotion_report(
    report: &TeamPromotionReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: {} {} (promotion={}, status={}, produced={:?}, validation_ok={})",
        report.command,
        report.store,
        report.promotion.id,
        report.promotion.status.as_str(),
        report.promotion.produced_memory_id,
        report.validation.ok
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_promotion_review_report(
    report: &TeamPromotionReviewCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team promotion report {} (promotion={}, ok_to_approve={}, risks={})",
        report.store,
        report.report.promotion.id,
        report.report.ok_to_approve,
        report.report.risk_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_sync_export_report(
    report: &TeamSyncExportCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team sync exported {} -> {} (memories={}, events={}, omitted={})",
        report.store, report.path, report.memory_count, report.event_count, report.omitted_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_sync_import_report(
    report: &TeamSyncImportCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team sync import {} <- {} (mode={}, ok={}, memories_applied={}, rejected={})",
        report.store,
        report.path,
        report.report.mode,
        report.report.ok,
        report.report.applied.memories,
        report.report.rejected.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_firewall_report(
    report: &TeamFirewallCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team firewall {} (ok={}, high={}, findings={})",
        report.store, report.firewall.ok, report.firewall.high_count, report.firewall.finding_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for finding in &report.firewall.findings {
        writeln!(
            writer,
            "- {:?} {} {} ({})",
            finding.severity, finding.kind, finding.memory_id, finding.detail
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_quality_report(
    report: &TeamQualityCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team quality {} (ok={}, health={}, duplicates={}, conflicts={}, findings={})",
        report.store,
        report.quality.ok,
        report.quality.health,
        report.quality.duplicate_group_count,
        report.quality.conflict_group_count,
        report.quality.findings.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for finding in &report.quality.findings {
        writeln!(
            writer,
            "- {:?} {} ({})",
            finding.severity, finding.kind, finding.detail
        )
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_ontology_report(
    report: &TeamOntologyCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team ontology {} (entities={}, relations={}, attributes={})",
        report.store,
        report.ontology.entity_count,
        report.ontology.relation_count,
        report.ontology.attribute_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_adapter_report(
    report: &TeamAdapterCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team adapter manifest (protocol={}, tools={})",
        report.manifest.protocol,
        report.manifest.tools.len()
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    for tool in &report.manifest.tools {
        writeln!(writer, "- {}", tool.name)
            .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))?;
    }
    Ok(())
}

fn emit_team_validation_report(
    report: &TeamValidationCliReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team validate {} (ok={}, errors={}, users={}, agents={}, projects={}, memories={}, promotions={}, audit={})",
        report.store,
        report.validation.ok,
        report.validation.error_count,
        report.validation.user_count,
        report.validation.agent_count,
        report.validation.project_count,
        report.validation.memory_count,
        report.validation.promotion_count,
        report.validation.audit_count
    )
    .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

fn emit_team_snapshot_report(
    report: &TeamSnapshotReport,
    json: bool,
    writer: &mut impl Write,
) -> Result<(), CliError> {
    if json {
        return write_json(writer, report);
    }
    writeln!(
        writer,
        "mneme: team snapshot {} (workspace={}, users={}, agents={}, projects={}, memories={}, promotions={}, audit={})",
        report.store,
        report.snapshot.workspace_id,
        report.snapshot.users.len(),
        report.snapshot.agents.len(),
        report.snapshot.projects.len(),
        report.snapshot.memories.len(),
        report.snapshot.promotions.len(),
        report.snapshot.audit.len()
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
        assert!(text.contains("quality"));
        assert!(text.contains("curate"));
        assert!(text.contains("restore"));
        assert!(text.contains("review"));

        let mut init_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "init".to_owned(), "--help".to_owned()],
            &mut init_output,
        )?;
        let init_text = String::from_utf8(init_output)?;
        assert!(init_text.contains("Usage: mneme init"));
        assert!(init_text.contains("--config <path>"));
        assert!(init_text.contains("--extractor-command <program>"));
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

        let mut mcp_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "mcp".to_owned(), "--help".to_owned()],
            &mut mcp_output,
        )?;
        let mcp_text = String::from_utf8(mcp_output)?;
        assert!(mcp_text.contains("mneme mcp config"));
        assert!(mcp_text.contains("codex|claude-code|cursor|all"));
        assert!(mcp_text.contains("local stdio MCP"));

        let mut review_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "review".to_owned(), "--help".to_owned()],
            &mut review_output,
        )?;
        let review_text = String::from_utf8(review_output)?;
        assert!(review_text.contains("Usage: mneme review <path>"));
        assert!(review_text.contains("--format markdown|json"));
        assert!(review_text.contains("--include-sensitive"));

        let mut quality_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "quality".to_owned(),
                "--help".to_owned(),
            ],
            &mut quality_output,
        )?;
        let quality_text = String::from_utf8(quality_output)?;
        assert!(quality_text.contains("Usage: mneme quality"));
        assert!(quality_text.contains("duplicate active claims"));

        let mut curate_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "curate".to_owned(), "--help".to_owned()],
            &mut curate_output,
        )?;
        let curate_text = String::from_utf8(curate_output)?;
        assert!(curate_text.contains("Usage: mneme curate"));
        assert!(curate_text.contains("--apply"));
        assert!(curate_text.contains("--compact"));

        let mut repair_output = Vec::new();
        run_cli_with_writer(
            vec!["mneme".to_owned(), "repair".to_owned(), "--help".to_owned()],
            &mut repair_output,
        )?;
        let repair_text = String::from_utf8(repair_output)?;
        assert!(repair_text.contains("mneme repair --check"));
        assert!(repair_text.contains("normalize"));

        let mut restore_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "restore".to_owned(),
                "--help".to_owned(),
            ],
            &mut restore_output,
        )?;
        let restore_text = String::from_utf8(restore_output)?;
        assert!(restore_text.contains("Usage:"));
        assert!(restore_text.contains("mneme restore --check"));
        assert!(restore_text.contains("roll back"));
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
    fn mcp_config_json_lists_supported_client_snippets() -> Result<(), Box<dyn std::error::Error>> {
        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "mcp".to_owned(),
                "config".to_owned(),
                "--client".to_owned(),
                "all".to_owned(),
                "--mcp-bin".to_owned(),
                "/tmp/mneme-mcp".to_owned(),
                "--mode".to_owned(),
                "all".to_owned(),
                "--v1-store".to_owned(),
                "/tmp/mneme-v1.json".to_owned(),
                "--team-store".to_owned(),
                "/tmp/mneme-team-v2.json".to_owned(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let value: serde_json::Value = serde_json::from_slice(&output)?;
        assert_eq!(value["command"], "mcp.config");
        assert_eq!(value["mode"], "all");
        assert_eq!(value["snippets"].as_array().map(Vec::len), Some(3));
        assert!(value["snippets"][0]["snippet"]
            .as_str()
            .is_some_and(|snippet| snippet.contains("[mcp_servers.mneme]")));
        assert!(value["snippets"][1]["snippet"]
            .as_str()
            .is_some_and(|snippet| snippet.contains("claude mcp add")));
        assert!(value["snippets"][2]["snippet"]
            .as_str()
            .is_some_and(|snippet| snippet.contains("\"mcpServers\"")));
        Ok(())
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
    fn init_can_install_agent_hook_extractor_command() -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store_path("init-extractor-store");
        let config = temp_store_path("init-extractor-profile").with_extension("env");
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
                "--no-bin".to_owned(),
                "--extractor-command".to_owned(),
                "/bin/sh".to_owned(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"extractor_command\": \"/bin/sh\""));

        let profile = std::fs::read_to_string(&config)?;
        assert!(profile.contains("MNEME_EXTRACTOR_COMMAND=/bin/sh"));

        let mut doctor_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--json".to_owned(),
            ],
            &mut doctor_output,
        )?;
        let doctor_text = String::from_utf8(doctor_output)?;
        assert!(doctor_text.contains("\"ok\": true"));
        assert!(doctor_text.contains("\"mneme_extractor_command\": \"/bin/sh\""));

        let mut doctor_plain_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
            ],
            &mut doctor_plain_output,
        )?;
        let doctor_plain_text = String::from_utf8(doctor_plain_output)?;
        assert!(doctor_plain_text.contains("profile extractor command: /bin/sh"));

        for path in [&store, &config] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
            let _ = std::fs::remove_file(format!("{}.lock", path.display()));
        }
        Ok(())
    }

    #[test]
    fn doctor_reports_workspace_health_before_and_after_init(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let store = temp_store_path("doctor-store");
        let config = temp_store_path("doctor-profile").with_extension("env");
        for path in [&store, &config] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
            let _ = std::fs::remove_file(format!("{}.lock", path.display()));
        }

        let mut missing_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--json".to_owned(),
            ],
            &mut missing_output,
        )?;
        let missing_text = String::from_utf8(missing_output)?;
        assert!(missing_text.contains("\"command\": \"doctor\""));
        assert!(missing_text.contains("\"ok\": false"));
        assert!(missing_text.contains("\"status\": \"missing\""));
        assert!(missing_text.contains("mneme init"));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "init".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--no-bin".to_owned(),
            ],
            &mut Vec::new(),
        )?;

        let mut healthy_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--json".to_owned(),
            ],
            &mut healthy_output,
        )?;
        let healthy_text = String::from_utf8(healthy_output)?;
        assert!(healthy_text.contains("\"ok\": true"));
        assert!(healthy_text.contains("\"status\": \"valid\""));
        assert!(healthy_text.contains("\"mneme_agent_id\": \"codex\""));
        assert!(healthy_text.contains("\"mneme_scope\": \"private\""));
        assert!(healthy_text.contains("\"mneme_max_items\": \"3\""));

        std::fs::write(&config, "MNEME_STORE=/tmp/other.json\nUNKNOWN=value\n")?;
        let mut invalid_profile_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--json".to_owned(),
            ],
            &mut invalid_profile_output,
        )?;
        let invalid_profile_text = String::from_utf8(invalid_profile_output)?;
        assert!(invalid_profile_text.contains("\"ok\": false"));
        assert!(invalid_profile_text.contains("unknown profile key"));
        assert!(invalid_profile_text.contains("MNEME_STORE points to"));

        std::fs::write(&store, "{not-json\n")?;
        let mut invalid_store_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "doctor".to_owned(),
                "--store".to_owned(),
                store.display().to_string(),
                "--config".to_owned(),
                config.display().to_string(),
                "--json".to_owned(),
            ],
            &mut invalid_store_output,
        )?;
        let invalid_store_text = String::from_utf8(invalid_store_output)?;
        assert!(invalid_store_text.contains("\"ok\": false"));
        assert!(invalid_store_text.contains("\"name\": \"store.current\""));
        assert!(invalid_store_text.contains("\"status\": \"fail\""));

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
    fn quality_reports_review_queue_without_mutation() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("quality-review-loop");
        let markdown_path = temp_store_path("quality-review-loop").with_extension("md");
        for path in [&path, &markdown_path] {
            let _ = std::fs::remove_file(path);
            let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        }

        for claim in [
            "user prefers quality loops",
            "user prefers quality loops",
            "user token API_KEY=FAKE_TEST_VALUE",
            "user prefers old review notes",
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
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "--claim-id".to_owned(),
                "claim-004".to_owned(),
                "user prefers current review notes".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "quality".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"command\": \"quality\""));
        assert!(text.contains("\"health\": \"attention_required\""));
        assert!(text.contains("\"duplicate_active_group_count\": 1"));
        assert!(text.contains("\"blocked_secret_claim_count\": 1"));
        assert!(text.contains("\"inactive_claim_count\": 1"));
        assert!(text.contains("\"kind\": \"duplicate_active\""));
        assert!(text.contains("\"kind\": \"blocked_secret\""));
        assert!(text.contains("\"kind\": \"inactive_history\""));
        assert!(text.contains("mneme forget --claim-id claim-002"));
        assert!(text.contains("[redacted:blocked_secret]"));
        assert!(!text.contains("API_KEY=FAKE_TEST_VALUE"));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "review".to_owned(),
                markdown_path.display().to_string(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        let markdown = std::fs::read_to_string(&markdown_path)?;
        assert!(markdown.contains("## Memory Quality"));
        assert!(markdown.contains("duplicate_active"));
        assert!(markdown.contains("blocked_secret"));
        assert!(markdown.contains("inactive_history"));
        assert!(markdown.contains("mneme compact"));
        assert!(!markdown.contains("API_KEY=FAKE_TEST_VALUE"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&markdown_path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        Ok(())
    }

    #[test]
    fn curate_plans_and_applies_guided_cleanup() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("guided-curation");
        let backup_path = std::path::PathBuf::from(format!("{}.bak", path.display()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);

        for claim in [
            "user prefers curated memory",
            "user prefers curated memory",
            "user token API_KEY=FAKE_TEST_VALUE",
            "user prefers old curation notes",
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
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "--claim-id".to_owned(),
                "claim-004".to_owned(),
                "user prefers current curation notes".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut dry_run_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "curate".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut dry_run_output,
        )?;
        let dry_run = String::from_utf8(dry_run_output)?;
        assert!(dry_run.contains("\"command\": \"curate\""));
        assert!(dry_run.contains("\"mode\": \"dry_run\""));
        assert!(dry_run.contains("\"changed\": false"));
        assert!(dry_run.contains("\"duplicate_forget_count\": 1"));
        assert!(dry_run.contains("\"blocked_secret_review_count\": 1"));
        assert!(dry_run.contains("\"compact_target_count\": 3"));
        assert!(dry_run.contains("\"kind\": \"forget_duplicate_active\""));
        assert!(dry_run.contains("\"kind\": \"review_blocked_secret\""));
        assert!(dry_run.contains("\"kind\": \"compact_non_active_records\""));
        assert!(dry_run.contains("[redacted:blocked_secret]"));
        assert!(!dry_run.contains("API_KEY=FAKE_TEST_VALUE"));

        let mut quality_after_dry_run = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "quality".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut quality_after_dry_run,
        )?;
        let quality_after_dry_run = String::from_utf8(quality_after_dry_run)?;
        assert!(quality_after_dry_run.contains("\"duplicate_active_group_count\": 1"));
        assert!(quality_after_dry_run.contains("\"blocked_secret_claim_count\": 1"));
        assert!(quality_after_dry_run.contains("\"inactive_claim_count\": 1"));

        let mut apply_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "curate".to_owned(),
                "--apply".to_owned(),
                "--compact".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut apply_output,
        )?;
        let apply = String::from_utf8(apply_output)?;
        assert!(apply.contains("\"mode\": \"apply\""));
        assert!(apply.contains("\"changed\": true"));
        assert!(apply.contains("\"forgotten_claim_count\": 1"));
        assert!(apply.contains("\"compacted\": true"));
        assert!(apply.contains("\"health\": \"ok\""));
        assert!(apply.contains("\"duplicate_active_group_count\": 0"));
        assert!(apply.contains("\"blocked_secret_claim_count\": 0"));
        assert!(apply.contains("\"inactive_claim_count\": 0"));
        assert!(apply.contains("mneme restore --check"));
        assert!(!apply.contains("API_KEY=FAKE_TEST_VALUE"));
        assert!(backup_path.exists());

        let mut final_quality_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "quality".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut final_quality_output,
        )?;
        let final_quality = String::from_utf8(final_quality_output)?;
        assert!(final_quality.contains("\"health\": \"ok\""));
        assert!(final_quality.contains("\"review_item_count\": 0"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
        Ok(())
    }

    #[test]
    fn restore_rolls_back_curated_store_from_backup() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("restore-curation");
        let backup_path = std::path::PathBuf::from(format!("{}.bak", path.display()));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);

        for claim in [
            "user prefers reversible curation",
            "user prefers reversible curation",
            "user token API_KEY=FAKE_TEST_VALUE",
            "user prefers old restore notes",
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
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "correct".to_owned(),
                "--claim-id".to_owned(),
                "claim-004".to_owned(),
                "user prefers current restore notes".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "curate".to_owned(),
                "--apply".to_owned(),
                "--compact".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut check_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "restore".to_owned(),
                "--check".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut check_output,
        )?;
        let check = String::from_utf8(check_output)?;
        assert!(check.contains("\"command\": \"restore\""));
        assert!(check.contains("\"mode\": \"check\""));
        assert!(check.contains("\"action\": \"restore_available\""));
        assert!(check.contains("\"restore_available\": true"));

        let mut restore_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "restore".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut restore_output,
        )?;
        let restore = String::from_utf8(restore_output)?;
        assert!(restore.contains("\"mode\": \"restore\""));
        assert!(restore.contains("\"action\": \"restored_from_backup\""));
        assert!(restore.contains("\"restored\": true"));
        assert!(restore.contains("\"current_preserved_as_backup\": true"));

        let mut restored_quality_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "quality".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut restored_quality_output,
        )?;
        let restored_quality = String::from_utf8(restored_quality_output)?;
        assert!(restored_quality.contains("\"health\": \"attention_required\""));
        assert!(restored_quality.contains("\"duplicate_active_group_count\": 1"));
        assert!(restored_quality.contains("\"blocked_secret_claim_count\": 1"));
        assert!(restored_quality.contains("\"inactive_claim_count\": 1"));
        assert!(!restored_quality.contains("API_KEY=FAKE_TEST_VALUE"));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "restore".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;
        let mut final_quality_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "quality".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut final_quality_output,
        )?;
        let final_quality = String::from_utf8(final_quality_output)?;
        assert!(final_quality.contains("\"health\": \"ok\""));
        assert!(final_quality.contains("\"review_item_count\": 0"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&backup_path);
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

        let mut check_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "repair".to_owned(),
                "--check".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut check_output,
        )?;
        let check_text = String::from_utf8(check_output)?;
        assert!(check_text.contains("\"mode\": \"check\""));
        assert!(check_text.contains("\"action\": \"repair_available\""));
        assert!(check_text.contains("\"ok\": true"));

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
        assert!(repair_text.contains("\"action\": \"restored_from_backup\""));

        let mut valid_check_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "repair".to_owned(),
                "--check".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut valid_check_output,
        )?;
        let valid_check_text = String::from_utf8(valid_check_output)?;
        assert!(valid_check_text.contains("\"action\": \"current_valid\""));

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

    #[cfg(unix)]
    #[test]
    fn end_command_extractor_records_raw_memory_note() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("end-command-extractor");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "begin".to_owned(),
                "Draft planning docs".to_owned(),
                "--agent".to_owned(),
                "codex".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let response = r#"{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"direct planning docs"}}"#;
        let no_claim = r#"{"schema_version":"mneme.extractor.command.v1","claim":null}"#;
        let script = format!(
            "request=$(cat); case \"$request\" in *remember:*) printf '%s\\n' '{no_claim}' ;; *\"keep explanations direct\"*) printf '%s\\n' '{response}' ;; *) printf '%s\\n' '{no_claim}' ;; esac"
        );
        let mut end_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "end".to_owned(),
                "session-001".to_owned(),
                "--remember".to_owned(),
                "For future planning docs, keep explanations direct.".to_owned(),
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
            &mut end_output,
        )?;
        let end_text = String::from_utf8(end_output)?;
        assert!(end_text.contains("\"extractor\": \"command\""));
        assert!(end_text.contains("\"remembered_claim_ids\": ["));
        assert!(end_text.contains("claim-001"));

        let mut context_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "context".to_owned(),
                "planning docs".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut context_output,
        )?;
        let context_text = String::from_utf8(context_output)?;
        assert!(context_text.contains("direct planning docs"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        Ok(())
    }

    #[test]
    fn end_summary_only_does_not_require_command_extractor(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("end-summary-only-command");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "begin".to_owned(),
                "Summarize task".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let mut end_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "end".to_owned(),
                "session-001".to_owned(),
                "--summary".to_owned(),
                "Only summarized the task".to_owned(),
                "--extractor".to_owned(),
                "command".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut end_output,
        )?;
        let end_text = String::from_utf8(end_output)?;
        assert!(end_text.contains("\"extractor\": \"command\""));
        assert!(end_text.contains("\"remembered_event_ids\": []"));
        assert!(end_text.contains("\"remembered_claim_ids\": []"));

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

    #[cfg(unix)]
    #[test]
    fn hook_end_accepts_command_extractor() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("hook-end-command-extractor");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));

        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "hook".to_owned(),
                "begin".to_owned(),
                "Draft planning docs".to_owned(),
                "--agent".to_owned(),
                "codex".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut Vec::new(),
        )?;

        let response = r#"{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"direct planning docs"}}"#;
        let script = format!("cat >/dev/null; printf '%s\\n' '{response}'");
        let mut end_output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "hook".to_owned(),
                "end".to_owned(),
                "session-001".to_owned(),
                "--summary".to_owned(),
                "Prepared planning docs".to_owned(),
                "--remember".to_owned(),
                "For future planning docs, keep explanations direct.".to_owned(),
                "--extractor-command".to_owned(),
                "/bin/sh".to_owned(),
                "--extractor-arg".to_owned(),
                "-c".to_owned(),
                "--extractor-arg".to_owned(),
                script,
                "--store".to_owned(),
                path.display().to_string(),
            ],
            &mut end_output,
        )?;
        let end_text = String::from_utf8(end_output)?;
        assert!(end_text.contains("\"operation\": \"end\""));
        assert!(end_text.contains("\"extractor\": \"command\""));
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

    #[test]
    fn remember_retries_transient_store_lock() -> Result<(), Box<dyn std::error::Error>> {
        let path = temp_store_path("remember-lock-retry");
        let store = JsonFileStore::new(path.clone());
        let lock_path = store.lock_path();
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&lock_path);
        std::fs::write(&lock_path, "held by test\n")?;

        let release_lock = lock_path.clone();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(30));
            let _ = std::fs::remove_file(release_lock);
        });

        let mut output = Vec::new();
        run_cli_with_writer(
            vec![
                "mneme".to_owned(),
                "remember".to_owned(),
                "user prefers retrying transient locks".to_owned(),
                "--store".to_owned(),
                path.display().to_string(),
                "--json".to_owned(),
            ],
            &mut output,
        )?;
        handle.join().expect("lock releaser should not panic");
        let text = String::from_utf8(output)?;
        assert!(text.contains("\"command\": \"remember\""));
        assert!(text.contains("\"claim_count\": 1"));

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(format!("{}.bak", path.display()));
        let _ = std::fs::remove_file(store.lock_path());
        Ok(())
    }

    fn temp_store_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("mneme-cli-{name}-{}.json", std::process::id()))
    }
}
