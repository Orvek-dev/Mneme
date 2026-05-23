//! Local developer CLI for the Mneme v1 personal-memory core.

use std::env;
use std::fmt::{Display, Formatter};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use mneme_core::{
    BuildStage, ClaimRecord, CommandExtractor, ContextPack, EngineSnapshot, EventInput,
    ExtractorError, JsonFileStore, MnemeConfig, MnemeEngine, MnemeExtractor, RuleBasedExtractor,
    PRODUCT_NAME,
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
            message: message.into(),
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

    fn extractor(source: ExtractorError) -> Self {
        Self {
            message: format!("extract memory claim: {source}"),
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
    match command.as_str() {
        "doctor" => print_doctor(writer),
        "--version" | "version" => {
            writeln!(writer, "{}", env!("CARGO_PKG_VERSION"))
                .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
        }
        "ingest" => run_ingest(args.collect(), writer),
        "remember" => run_remember(args.collect(), writer),
        "correct" => run_correct(args.collect(), writer),
        "forget" => run_forget(args.collect(), writer),
        "context" => run_context(args.collect(), writer),
        "snapshot" => run_snapshot(args.collect(), writer),
        _ => Err(CliError::invalid_cli(format!(
            "unknown mneme command: {command}\navailable commands: doctor, version, ingest, remember, correct, forget, context, snapshot"
        ))),
    }
}

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

fn write_json<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), CliError> {
    let json = serde_json::to_string_pretty(value).map_err(CliError::json)?;
    writeln!(writer, "{json}")
        .map_err(|source| CliError::io("write", Path::new("<stdout>"), source))
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn temp_store_path(name: &str) -> PathBuf {
        env::temp_dir().join(format!("mneme-cli-{name}-{}.json", std::process::id()))
    }
}
