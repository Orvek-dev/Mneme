use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use mneme_core::{BuildStage, PRODUCT_NAME};
use serde::Serialize;

use crate::candidate::{
    check_candidates, generate_candidates, promote_candidate, CandidateCheckReport,
    CandidateGenerateConfig, CandidatePromoteConfig, CandidatePromoteReport, CandidateReport,
};
use crate::error::EvalError;
use crate::redaction;
use crate::report::{
    AcceptanceGateReport, AcceptanceReport, BaselineCategorySummary, BaselineFailedCheckSummary,
    BaselineMetadata, BaselineReport, BaselineRunReport, BaselineScenarioMetadata,
    BaselineScenarioSummary, EvalReport, ScenarioReport, ScenarioValidationReport,
    ValidationReport,
};
use crate::runtime::replay_scenario;
use crate::scenario::load_scenario;
use crate::target::{
    build_target, CommandExtractorOptions, FaultMode, TargetKind, TargetRunOptions,
};
use crate::trend::{compare_baselines, BaselineCompareConfig, BaselineCompareReport};

const DEFAULT_BASELINE_ITERATIONS: usize = 3;
const MAX_BASELINE_ITERATIONS: usize = 100;
const BASELINE_SUMMARY_SCHEMA_VERSION: u32 = 1;
const BASELINE_SUMMARY_LIMIT: usize = 5;

/// Runs the Mneme eval harness command-line interface.
pub fn run_cli(args: impl IntoIterator<Item = String>) -> Result<(), EvalError> {
    let mut args = args.into_iter();
    let _program = args.next();
    let Some(command) = args.next() else {
        print_doctor();
        return Ok(());
    };
    let raw_args = args.collect::<Vec<_>>();
    match command.as_str() {
        "help" => run_help(raw_args),
        "--help" | "-h" => {
            print_help(None)?;
            Ok(())
        }
        "doctor" => {
            if wants_command_help(&raw_args) {
                print_help(Some("doctor"))?;
            } else {
                print_doctor();
            }
            Ok(())
        }
        "--version" | "version" => {
            if wants_command_help(&raw_args) {
                print_help(Some("version"))?;
            } else {
                println!("{}", env!("CARGO_PKG_VERSION"));
            }
            Ok(())
        }
        "validate" => run_command_or_help("validate", raw_args, run_validate),
        "acceptance" => run_command_or_help("acceptance", raw_args, run_acceptance),
        "baseline" => run_command_or_help("baseline", raw_args, run_baseline),
        "baseline-gate" => run_command_or_help("baseline-gate", raw_args, run_baseline_gate),
        "baseline-summary" => {
            run_command_or_help("baseline-summary", raw_args, run_baseline_summary)
        }
        "baseline-compare" => {
            run_command_or_help("baseline-compare", raw_args, run_baseline_compare)
        }
        "candidate" => run_command_or_help("candidate", raw_args, run_candidate),
        "candidate-check" => run_command_or_help("candidate-check", raw_args, run_candidate_check),
        "candidate-promote" => {
            run_command_or_help("candidate-promote", raw_args, run_candidate_promote)
        }
        "replay" => run_command_or_help("replay", raw_args, run_replay),
        "run" => run_command_or_help("run", raw_args, run_suite),
        _ => Err(EvalError::invalid_cli(format!(
            "unknown mneme-eval command: {command}\navailable commands: doctor, version, validate, acceptance, baseline, baseline-gate, baseline-summary, baseline-compare, candidate, candidate-check, candidate-promote, replay, run"
        ))),
    }
}

fn run_command_or_help<F>(
    command: &'static str,
    raw_args: Vec<String>,
    run: F,
) -> Result<(), EvalError>
where
    F: FnOnce(Vec<String>) -> Result<(), EvalError>,
{
    if wants_command_help(&raw_args) {
        print_help(Some(command))?;
        Ok(())
    } else {
        run(raw_args)
    }
}

fn wants_command_help(raw_args: &[String]) -> bool {
    raw_args.len() == 1 && matches!(raw_args[0].as_str(), "--help" | "-h")
}

fn run_help(raw_args: Vec<String>) -> Result<(), EvalError> {
    match raw_args.as_slice() {
        [] => print_help(None),
        [command] => print_help(Some(command)),
        _ => Err(EvalError::invalid_cli(
            "usage: mneme-eval help [command]\nexample: mneme-eval help baseline",
        )),
    }
}

fn print_help(command: Option<&str>) -> Result<(), EvalError> {
    let text = match command {
        None => MNEME_EVAL_HELP,
        Some(command) => command_help(command).ok_or_else(|| {
            EvalError::invalid_cli(format!(
                "unknown mneme-eval help topic: {command}\navailable help topics: doctor, version, validate, acceptance, baseline, baseline-gate, baseline-summary, baseline-compare, candidate, candidate-check, candidate-promote, replay, run"
            ))
        })?,
    };
    println!("{text}");
    Ok(())
}

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "doctor" => Some(MNEME_EVAL_DOCTOR_HELP),
        "version" | "--version" => Some(MNEME_EVAL_VERSION_HELP),
        "validate" => Some(MNEME_EVAL_VALIDATE_HELP),
        "acceptance" => Some(MNEME_EVAL_ACCEPTANCE_HELP),
        "baseline" => Some(MNEME_EVAL_BASELINE_HELP),
        "baseline-gate" => Some(MNEME_EVAL_BASELINE_GATE_HELP),
        "baseline-summary" => Some(MNEME_EVAL_BASELINE_SUMMARY_HELP),
        "baseline-compare" => Some(MNEME_EVAL_BASELINE_COMPARE_HELP),
        "candidate" => Some(MNEME_EVAL_CANDIDATE_HELP),
        "candidate-check" => Some(MNEME_EVAL_CANDIDATE_CHECK_HELP),
        "candidate-promote" => Some(MNEME_EVAL_CANDIDATE_PROMOTE_HELP),
        "replay" => Some(MNEME_EVAL_REPLAY_HELP),
        "run" => Some(MNEME_EVAL_RUN_HELP),
        _ => None,
    }
}

const MNEME_EVAL_HELP: &str = r#"Mneme eval harness

Usage:
  mneme-eval <command> [options]
  mneme-eval help [command]

Commands:
  doctor         Show harness stage and available targets.
  version        Print the eval harness version.
  validate       Validate one scenario or a scenario suite.
  run            Replay a scenario suite against a target.
  replay         Replay one scenario against a target.
  acceptance     Run acceptance gates for a target and suite.
  baseline       Repeat a suite and summarize pass rates.
  baseline-gate  Gate a saved baseline JSON report.
  baseline-summary
                 Summarize baseline failures for triage.
  baseline-compare
                 Compare two baseline reports for regressions.
  candidate      Create local scenario candidate artifacts from failed reports.
  candidate-check
                 Validate local scenario candidate artifacts.
  candidate-promote
                 Promote a reviewed candidate into a scenario suite.

Targets:
  fake, mneme-v1, mneme-v1-command

Examples:
  mneme-eval validate --suite core
  mneme-eval run --suite core --target mneme-v1
  mneme-eval help baseline
  mneme-eval help baseline-summary
  mneme-eval help baseline-compare
  mneme-eval help candidate"#;

const MNEME_EVAL_DOCTOR_HELP: &str = r#"Usage: mneme-eval doctor

Show harness build stage and available eval targets."#;

const MNEME_EVAL_VERSION_HELP: &str = r#"Usage: mneme-eval version

Print the eval harness version."#;

const MNEME_EVAL_VALIDATE_HELP: &str = r#"Usage:
  mneme-eval validate <scenario.yaml> [--json] [--report <path>]
  mneme-eval validate --suite <name> [--json] [--report <path>]

Validate scenario files without running a target.

Example:
  mneme-eval validate --suite core"#;

const MNEME_EVAL_RUN_HELP: &str = r#"Usage: mneme-eval run [--suite <name>] [--target fake|mneme-v1|mneme-v1-command] [--extractor-command <program>] [--extractor-arg <arg>]... [--seeded-fault <name>] [--json] [--report <path>]

Replay a scenario suite against a target. Defaults are --suite core and
--target fake.

Example:
  mneme-eval run --suite core --target mneme-v1"#;

const MNEME_EVAL_REPLAY_HELP: &str = r#"Usage: mneme-eval replay <scenario.yaml> [--target fake|mneme-v1|mneme-v1-command] [--extractor-command <program>] [--extractor-arg <arg>]... [--seeded-fault <name>] [--json] [--report <path>]

Replay one scenario against a target.

Example:
  mneme-eval replay evals/scenarios/core/same-turn-explicit-remember.yaml --target mneme-v1"#;

const MNEME_EVAL_ACCEPTANCE_HELP: &str = r#"Usage: mneme-eval acceptance [--suite <name>] [--target fake|mneme-v1|mneme-v1-command] [--extractor-command <program>] [--extractor-arg <arg>]... [--json] [--report <path>]

Run acceptance gates for a suite and target.

Example:
  mneme-eval acceptance --suite core --target mneme-v1"#;

const MNEME_EVAL_BASELINE_HELP: &str = r#"Usage: mneme-eval baseline [--suite <name>] [--target fake|mneme-v1|mneme-v1-command] [--extractor-command <program>] [--extractor-arg <arg>]... [--seeded-fault <name>] [--iterations <n>] [--provider-label <label>] [--model-label <label>] [--run-label <label>] [--live-provider] [--json] [--report <path>]

Repeat a suite and summarize aggregate, category, and per-scenario pass rates.

Example:
  MNEME_OPENAI_DRY_RUN=1 mneme-eval baseline --suite model --target mneme-v1-command --extractor-command wrappers/openai_extractor.py --iterations 2 --provider-label openai --model-label dry-run"#;

const MNEME_EVAL_BASELINE_GATE_HELP: &str = r#"Usage: mneme-eval baseline-gate <baseline-report.json> [--min-pass-rate <0..1>] [--min-category-pass-rate <0..1>] [--max-failed-iterations <n>] [--max-failed-scenario-runs <n>] [--require-live-provider] [--allow-missing-provider-label] [--allow-missing-model-label] [--require-run-label] [--json] [--report <path>]

Gate a saved baseline report before treating it as usable.

Example:
  mneme-eval baseline-gate evals/reports/openai-dry-run-baseline.json"#;

const MNEME_EVAL_BASELINE_SUMMARY_HELP: &str = r#"Usage: mneme-eval baseline-summary <baseline-report.json> [--json] [--report <path>]

Summarize a saved baseline report for provider triage. This command exits
successfully even when the baseline failed, so failed reports can be inspected
without bypassing baseline-gate.

Example:
  mneme-eval baseline-summary evals/reports/openai-live-baseline.json --report evals/reports/openai-live-baseline.summary.json"#;

const MNEME_EVAL_BASELINE_COMPARE_HELP: &str = r#"Usage: mneme-eval baseline-compare <before-baseline.json> <after-baseline.json> [--max-pass-rate-drop <0..1>] [--max-category-drop <0..1>] [--fail-on-regression] [--json] [--report <path>]

Compare two saved baseline reports and summarize aggregate, category,
scenario, and failed-check changes. By default this command emits a report even
when regressions are found; use --fail-on-regression to make regressions exit
non-zero.

Example:
  mneme-eval baseline-compare evals/reports/before.json evals/reports/after.json --fail-on-regression"#;

const MNEME_EVAL_CANDIDATE_HELP: &str = r#"Usage: mneme-eval candidate <eval-or-baseline-report.json> [--out-dir <dir>] [--prefix <label>] [--limit <n>] [--suite <name>] [--json] [--report <path>]

Create local, review-only scenario candidate artifacts from failed eval or
baseline reports. Generated candidates are sanitized and should be reviewed
before any scenario block is promoted into evals/scenarios/<suite>/.

Example:
  mneme-eval candidate evals/reports/openai-live-baseline.json --out-dir evals/candidates/openai --limit 3"#;

const MNEME_EVAL_CANDIDATE_CHECK_HELP: &str = r#"Usage: mneme-eval candidate-check <candidate.yaml|dir> [--json] [--report <path>]

Validate local scenario candidate artifacts before sharing or promoting them.

Example:
  mneme-eval candidate-check evals/candidates/openai --json"#;

const MNEME_EVAL_CANDIDATE_PROMOTE_HELP: &str = r#"Usage: mneme-eval candidate-promote <candidate.yaml> [--suite <name>] [--filename <name.yaml>] [--scenario-root <dir>] [--apply] [--json] [--report <path>]

Validate a reviewed candidate and promote only its nested scenario block into a
public scenario suite. The command is a dry run by default; pass --apply to
write evals/scenarios/<suite>/<filename>.

Example:
  mneme-eval candidate-promote evals/candidates/openai/example.candidate.yaml --suite model --filename dogfood-example.yaml --apply"#;

fn print_doctor() {
    println!(
        "{PRODUCT_NAME} eval harness: {}",
        BuildStage::PersonalCoreV1.as_str()
    );
    println!("available eval targets: {}", TargetKind::available());
}

#[derive(Debug, Clone)]
struct CommandOptions {
    json: bool,
    report_path: Option<PathBuf>,
    target_kind: TargetKind,
    fault_mode: FaultMode,
    extractor_command: Option<String>,
    extractor_args: Vec<String>,
    iterations: usize,
    live_provider: bool,
    provider_label: Option<String>,
    model_label: Option<String>,
    run_label: Option<String>,
}

#[derive(Debug, Clone)]
struct BaselineGateOptions {
    json: bool,
    report_path: Option<PathBuf>,
    min_pass_rate: f64,
    min_category_pass_rate: f64,
    max_failed_iterations: usize,
    max_failed_scenario_runs: usize,
    require_live_provider: bool,
    require_provider_label: bool,
    require_model_label: bool,
    require_run_label: bool,
}

#[derive(Debug, Clone, Default)]
struct BaselineSummaryOptions {
    json: bool,
    report_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct BaselineCompareOptions {
    json: bool,
    report_path: Option<PathBuf>,
    max_pass_rate_drop: f64,
    max_category_drop: f64,
    fail_on_regression: bool,
}

#[derive(Debug, Clone)]
struct CandidateOptions {
    json: bool,
    report_path: Option<PathBuf>,
    out_dir: PathBuf,
    prefix: String,
    limit: Option<usize>,
    suite: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct CandidateCheckOptions {
    json: bool,
    report_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct CandidatePromoteOptions {
    json: bool,
    report_path: Option<PathBuf>,
    scenario_root: PathBuf,
    suite: Option<String>,
    filename: Option<String>,
    apply: bool,
}

#[derive(Debug, Clone, Serialize)]
struct BaselineSummaryReport {
    report_schema_version: u32,
    command: &'static str,
    source: String,
    suite: String,
    target: String,
    ok: bool,
    triage_status: String,
    baseline_metadata: BaselineMetadata,
    iterations: usize,
    scenario_count: usize,
    total_scenario_runs: usize,
    pass_rate: f64,
    failed_iterations: usize,
    failed_scenario_runs: usize,
    failed_category_count: usize,
    failed_scenario_count: usize,
    failed_check_count: usize,
    redaction_findings: Vec<String>,
    top_failed_categories: Vec<BaselineCategorySummary>,
    top_failed_scenarios: Vec<BaselineScenarioSummary>,
    top_failed_checks: Vec<BaselineFailedCheckSummary>,
    recommended_next_actions: Vec<String>,
}

impl Default for CommandOptions {
    fn default() -> Self {
        Self {
            json: false,
            report_path: None,
            target_kind: TargetKind::Fake,
            fault_mode: FaultMode::None,
            extractor_command: None,
            extractor_args: Vec::new(),
            iterations: DEFAULT_BASELINE_ITERATIONS,
            live_provider: false,
            provider_label: None,
            model_label: None,
            run_label: None,
        }
    }
}

impl Default for BaselineGateOptions {
    fn default() -> Self {
        Self {
            json: false,
            report_path: None,
            min_pass_rate: 1.0,
            min_category_pass_rate: 1.0,
            max_failed_iterations: 0,
            max_failed_scenario_runs: 0,
            require_live_provider: false,
            require_provider_label: true,
            require_model_label: true,
            require_run_label: false,
        }
    }
}

impl Default for CandidateOptions {
    fn default() -> Self {
        Self {
            json: false,
            report_path: None,
            out_dir: PathBuf::from("evals/candidates"),
            prefix: "dogfood".to_owned(),
            limit: None,
            suite: None,
        }
    }
}

impl Default for BaselineCompareOptions {
    fn default() -> Self {
        Self {
            json: false,
            report_path: None,
            max_pass_rate_drop: 0.0,
            max_category_drop: 0.0,
            fail_on_regression: false,
        }
    }
}

impl Default for CandidatePromoteOptions {
    fn default() -> Self {
        Self {
            json: false,
            report_path: None,
            scenario_root: PathBuf::from("evals/scenarios"),
            suite: None,
            filename: None,
            apply: false,
        }
    }
}

#[derive(Debug, Clone)]
enum ValidationTarget {
    Scenario(PathBuf),
    Suite(String),
}

fn run_validate(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (target, options) = parse_validate_args(raw_args)?;
    let paths = match target {
        ValidationTarget::Scenario(path) => vec![path],
        ValidationTarget::Suite(suite) => scenario_paths_for_suite(&suite)?,
    };
    let report = validate_paths(paths);
    emit_validation_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("scenario validation failed"))
    }
}

fn validate_paths(paths: Vec<PathBuf>) -> ValidationReport {
    let results = paths
        .iter()
        .map(|path| match load_scenario(path) {
            Ok(scenario) => ScenarioValidationReport::pass(
                path.display().to_string(),
                scenario.id,
                scenario.tags,
            ),
            Err(error) => {
                ScenarioValidationReport::fail(path.display().to_string(), error.to_string())
            }
        })
        .collect();
    ValidationReport::from_results(results)
}

fn run_acceptance(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (suite, options) = parse_acceptance_args(raw_args)?;
    let report = build_acceptance_report(&suite, &options);
    emit_acceptance_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("acceptance gate failed"))
    }
}

fn build_acceptance_report(suite: &str, options: &CommandOptions) -> AcceptanceReport {
    let target_kind = options.target_kind;
    let target_name = target_kind.as_str();
    let mut gates = Vec::new();
    let paths = match scenario_paths_for_suite(suite) {
        Ok(paths) => paths,
        Err(error) => {
            gates.push(AcceptanceGateReport::fail(
                "suite.discovery",
                error.to_string(),
            ));
            return AcceptanceReport::from_gates(target_name, gates);
        }
    };

    let validation = validate_paths(paths.clone());
    if validation.ok {
        gates.push(AcceptanceGateReport::pass(
            "scenario.validation",
            format!("{} scenario(s) valid in suite {suite}", validation.valid),
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "scenario.validation",
            format!(
                "{} invalid scenario(s) in suite {suite}",
                validation.invalid
            ),
        ));
    }

    gates.push(check_invalid_fixtures_are_rejected());

    let mut suite_options = options.clone();
    suite_options.fault_mode = FaultMode::None;
    let suite_report = eval_report_for_paths(&paths, &suite_options);
    match &suite_report {
        Ok(report) if report.ok => gates.push(AcceptanceGateReport::pass(
            "target.core-suite",
            format!(
                "{} scenario(s) passed for target {}",
                report.passed, report.target
            ),
        )),
        Ok(report) => gates.push(AcceptanceGateReport::fail(
            "target.core-suite",
            format!(
                "{} scenario(s) failed for target {}",
                report.failed, report.target
            ),
        )),
        Err(error) => gates.push(AcceptanceGateReport::fail(
            "target.core-suite",
            error.to_string(),
        )),
    }

    match &suite_report {
        Ok(report)
            if report.report_schema_version == 1
                && report.target == target_name
                && !report.target_metadata.extractor.is_empty()
                && report.scenario_count == paths.len()
                && !report.results.is_empty() =>
        {
            gates.push(AcceptanceGateReport::pass(
                "report.contract",
                "schema version, target metadata, counts, and results are present",
            ));
        }
        Ok(_) => {
            gates.push(AcceptanceGateReport::fail(
                "report.contract",
                "report metadata or result shape did not match contract",
            ));
        }
        Err(error) => {
            gates.push(AcceptanceGateReport::fail(
                "report.contract",
                format!("cannot evaluate report contract: {error}"),
            ));
        }
    }

    for fault_mode in [
        FaultMode::SkipClaims,
        FaultMode::LeakSecrets,
        FaultMode::DropCitations,
    ] {
        gates.push(check_seeded_fault_is_detected(
            &paths,
            target_kind,
            options,
            fault_mode,
        ));
    }

    AcceptanceReport::from_gates(target_name, gates)
}

fn check_invalid_fixtures_are_rejected() -> AcceptanceGateReport {
    let paths = match invalid_fixture_paths() {
        Ok(paths) => paths,
        Err(error) => {
            return AcceptanceGateReport::fail("invalid-fixtures.rejected", error.to_string());
        }
    };
    if paths.is_empty() {
        return AcceptanceGateReport::fail(
            "invalid-fixtures.rejected",
            "no invalid fixtures found",
        );
    }

    let accepted = paths
        .iter()
        .filter(|path| load_scenario(path).is_ok())
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>();
    if accepted.is_empty() {
        AcceptanceGateReport::pass(
            "invalid-fixtures.rejected",
            format!("{} invalid fixture(s) rejected", paths.len()),
        )
    } else {
        AcceptanceGateReport::fail(
            "invalid-fixtures.rejected",
            format!("unexpected valid fixture(s): {}", accepted.join(", ")),
        )
    }
}

fn check_seeded_fault_is_detected(
    paths: &[PathBuf],
    target_kind: TargetKind,
    options: &CommandOptions,
    fault_mode: FaultMode,
) -> AcceptanceGateReport {
    let gate_name = format!("seeded-fault.{}", fault_mode.as_str());
    let mut fault_options = options.clone();
    fault_options.target_kind = target_kind;
    fault_options.fault_mode = fault_mode;
    match eval_report_for_paths(paths, &fault_options) {
        Ok(report) if !report.ok => AcceptanceGateReport::pass(
            gate_name,
            format!(
                "fault detected for target {} with {} failed scenario(s)",
                report.target, report.failed
            ),
        ),
        Ok(report) => AcceptanceGateReport::fail(
            gate_name,
            format!(
                "fault unexpectedly passed for target {} across {} scenario(s)",
                report.target, report.scenario_count
            ),
        ),
        Err(error) => AcceptanceGateReport::fail(gate_name, error.to_string()),
    }
}

fn run_replay(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (path, options) = parse_replay_args(raw_args)?;
    let report = eval_report_for_paths(&[path], &options)?;
    emit_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("eval failed"))
    }
}

fn run_suite(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (suite, options) = parse_suite_args(raw_args)?;
    let paths = scenario_paths_for_suite(&suite)?;
    let report = eval_report_for_paths(&paths, &options)?;
    emit_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("eval failed"))
    }
}

fn run_baseline(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (suite, options) = parse_baseline_args(raw_args)?;
    let paths = scenario_paths_for_suite(&suite)?;
    let report = baseline_report_for_paths(&suite, &paths, &options)?;
    emit_baseline_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("baseline failed"))
    }
}

fn run_baseline_gate(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (path, options) = parse_baseline_gate_args(raw_args)?;
    let raw_json =
        fs::read_to_string(&path).map_err(|source| EvalError::io("read", &path, source))?;
    let baseline: BaselineReport =
        serde_json::from_str(&raw_json).map_err(|source| EvalError::parse_json(&path, source))?;
    let report = build_baseline_gate_report(&path, &raw_json, &baseline, &options);
    emit_baseline_gate_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("baseline gate failed"))
    }
}

fn run_baseline_summary(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (path, options) = parse_baseline_summary_args(raw_args)?;
    let raw_json =
        fs::read_to_string(&path).map_err(|source| EvalError::io("read", &path, source))?;
    let baseline: BaselineReport =
        serde_json::from_str(&raw_json).map_err(|source| EvalError::parse_json(&path, source))?;
    let report = build_baseline_summary_report(&path, &raw_json, &baseline);
    emit_baseline_summary_report(&report, &options)
}

fn run_baseline_compare(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (before_path, after_path, options) = parse_baseline_compare_args(raw_args)?;
    let config = BaselineCompareConfig {
        before_path,
        after_path,
        max_pass_rate_drop: options.max_pass_rate_drop,
        max_category_drop: options.max_category_drop,
    };
    let report = compare_baselines(&config)?;
    emit_baseline_compare_report(&report, &options)?;
    if options.fail_on_regression && !report.ok {
        Err(EvalError::scenario("baseline regression detected"))
    } else {
        Ok(())
    }
}

fn run_candidate(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (source_path, options) = parse_candidate_args(raw_args)?;
    let config = CandidateGenerateConfig {
        source_path,
        out_dir: options.out_dir.clone(),
        prefix: options.prefix.clone(),
        limit: options.limit,
        suite_override: options.suite.clone(),
    };
    let report = generate_candidates(&config)?;
    emit_candidate_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("candidate generation failed"))
    }
}

fn run_candidate_check(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (path, options) = parse_candidate_check_args(raw_args)?;
    let report = check_candidates(&path)?;
    emit_candidate_check_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("candidate validation failed"))
    }
}

fn run_candidate_promote(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (path, options) = parse_candidate_promote_args(raw_args)?;
    let config = CandidatePromoteConfig {
        source_path: path,
        scenario_root: options.scenario_root.clone(),
        suite_override: options.suite.clone(),
        filename: options.filename.clone(),
        apply: options.apply,
    };
    let report = promote_candidate(&config)?;
    emit_candidate_promote_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("candidate promotion failed"))
    }
}

fn build_baseline_summary_report(
    path: &Path,
    raw_json: &str,
    baseline: &BaselineReport,
) -> BaselineSummaryReport {
    let redaction_findings = redaction::findings(raw_json);
    let top_failed_categories = top_failed_categories(baseline);
    let top_failed_scenarios = top_failed_scenarios(baseline);
    let top_failed_checks = top_failed_checks(baseline);
    let recommended_next_actions = recommended_baseline_actions(
        baseline,
        &redaction_findings,
        &top_failed_categories,
        &top_failed_scenarios,
        &top_failed_checks,
    );
    BaselineSummaryReport {
        report_schema_version: BASELINE_SUMMARY_SCHEMA_VERSION,
        command: "baseline-summary",
        source: path.display().to_string(),
        suite: baseline.suite.clone(),
        target: baseline.target.clone(),
        ok: baseline.ok && redaction_findings.is_empty(),
        triage_status: baseline_triage_status(baseline, &redaction_findings).to_owned(),
        baseline_metadata: baseline.baseline_metadata.clone(),
        iterations: baseline.iterations,
        scenario_count: baseline.scenario_count,
        total_scenario_runs: baseline.total_scenario_runs,
        pass_rate: baseline.pass_rate,
        failed_iterations: baseline.failed_iterations,
        failed_scenario_runs: baseline.failed_scenario_runs,
        failed_category_count: baseline.failure_summary.failed_categories.len(),
        failed_scenario_count: baseline.failure_summary.failed_scenarios.len(),
        failed_check_count: baseline
            .failure_summary
            .failed_checks
            .iter()
            .map(|check| check.count)
            .sum(),
        redaction_findings,
        top_failed_categories,
        top_failed_scenarios,
        top_failed_checks,
        recommended_next_actions,
    }
}

fn baseline_triage_status(
    baseline: &BaselineReport,
    redaction_findings: &[String],
) -> &'static str {
    match (baseline.ok, redaction_findings.is_empty()) {
        (true, true) => "passing",
        (true, false) => "redaction_required",
        (false, true) => "failing",
        (false, false) => "failing_redaction_required",
    }
}

fn top_failed_categories(baseline: &BaselineReport) -> Vec<BaselineCategorySummary> {
    let mut categories = baseline.failure_summary.failed_categories.clone();
    categories.sort_by(|left, right| {
        right
            .failed
            .cmp(&left.failed)
            .then_with(|| left.pass_rate.total_cmp(&right.pass_rate))
            .then_with(|| left.category.cmp(&right.category))
    });
    categories.truncate(BASELINE_SUMMARY_LIMIT);
    categories
}

fn top_failed_scenarios(baseline: &BaselineReport) -> Vec<BaselineScenarioSummary> {
    let mut scenarios = baseline.failure_summary.failed_scenarios.clone();
    scenarios.sort_by(|left, right| {
        right
            .failed
            .cmp(&left.failed)
            .then_with(|| left.pass_rate.total_cmp(&right.pass_rate))
            .then_with(|| left.scenario_id.cmp(&right.scenario_id))
    });
    scenarios.truncate(BASELINE_SUMMARY_LIMIT);
    scenarios
}

fn top_failed_checks(baseline: &BaselineReport) -> Vec<BaselineFailedCheckSummary> {
    let mut checks = baseline.failure_summary.failed_checks.clone();
    checks.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.check.cmp(&right.check))
    });
    checks.truncate(BASELINE_SUMMARY_LIMIT);
    checks
}

fn recommended_baseline_actions(
    baseline: &BaselineReport,
    redaction_findings: &[String],
    categories: &[BaselineCategorySummary],
    scenarios: &[BaselineScenarioSummary],
    checks: &[BaselineFailedCheckSummary],
) -> Vec<String> {
    let mut actions = Vec::new();
    if !redaction_findings.is_empty() {
        actions.push(format!(
            "redact or keep local before sharing: {}",
            redaction_findings.join(", ")
        ));
    }
    if !baseline.baseline_metadata.live_provider {
        actions.push(
            "treat this as dry-run evidence; run a local live baseline before provider calibration"
                .to_owned(),
        );
    }
    if let Some(category) = categories.first() {
        actions.push(format!(
            "start with category `{}` ({}/{} failed attempts, pass_rate={:.2}%)",
            category.category,
            category.failed,
            category.attempts,
            category.pass_rate * 100.0
        ));
    }
    if let Some(scenario) = scenarios.first() {
        actions.push(format!(
            "replay scenario `{}` against the same target and extractor",
            scenario.scenario_id
        ));
    }
    if let Some(check) = checks.first() {
        actions.push(format!(
            "inspect failed check `{}` across {} occurrence(s)",
            check.check, check.count
        ));
    }
    if baseline.failed_iterations > 0 {
        actions.push(format!(
            "compare failed iteration artifacts before changing wrapper prompts (failed_iterations={})",
            baseline.failed_iterations
        ));
    }
    if baseline.ok && redaction_findings.is_empty() {
        actions.push("baseline passed; keep this summary with the baseline report".to_owned());
    }
    actions
}

fn build_baseline_gate_report(
    path: &Path,
    raw_json: &str,
    baseline: &BaselineReport,
    options: &BaselineGateOptions,
) -> AcceptanceReport {
    let mut gates = Vec::new();
    let target = format!("baseline-gate:{}:{}", baseline.suite, baseline.target);

    if baseline.report_schema_version == 1
        && !baseline.suite.is_empty()
        && !baseline.target.is_empty()
        && baseline.iterations > 0
        && baseline.scenario_count > 0
    {
        gates.push(AcceptanceGateReport::pass(
            "report.contract",
            format!(
                "{} has schema v{} for suite {}",
                path.display(),
                baseline.report_schema_version,
                baseline.suite
            ),
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "report.contract",
            "baseline report is missing required metadata, iterations, or scenarios",
        ));
    }

    if baseline.target == TargetKind::MnemeV1Command.as_str()
        && baseline.target_metadata.opt_in
        && baseline.target_metadata.command_configured
    {
        gates.push(AcceptanceGateReport::pass(
            "target.command-extractor",
            "baseline used the opt-in command extractor target",
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "target.command-extractor",
            format!(
                "expected target={} opt_in=true command_configured=true, actual target={} opt_in={} command_configured={}",
                TargetKind::MnemeV1Command.as_str(),
                baseline.target,
                baseline.target_metadata.opt_in,
                baseline.target_metadata.command_configured
            ),
        ));
    }

    if baseline.ok {
        gates.push(AcceptanceGateReport::pass(
            "baseline.ok",
            "baseline reported no failed iterations or scenario runs",
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "baseline.ok",
            format!(
                "failed_iterations={} failed_scenario_runs={}",
                baseline.failed_iterations, baseline.failed_scenario_runs
            ),
        ));
    }

    gates.push(check_rate_gate(
        "pass-rate.minimum",
        baseline.pass_rate,
        options.min_pass_rate,
        "aggregate pass_rate",
    ));

    let failing_categories = baseline
        .category_pass_rates
        .iter()
        .filter(|category| category.pass_rate < options.min_category_pass_rate)
        .map(|category| format!("{}={:.4}", category.category, category.pass_rate))
        .collect::<Vec<_>>();
    if baseline.category_pass_rates.is_empty() {
        gates.push(AcceptanceGateReport::fail(
            "category-pass-rate.minimum",
            "baseline report has no category pass rates",
        ));
    } else if failing_categories.is_empty() {
        gates.push(AcceptanceGateReport::pass(
            "category-pass-rate.minimum",
            format!(
                "{} categor{} met minimum {:.4}",
                baseline.category_pass_rates.len(),
                if baseline.category_pass_rates.len() == 1 {
                    "y"
                } else {
                    "ies"
                },
                options.min_category_pass_rate
            ),
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "category-pass-rate.minimum",
            format!(
                "category pass rate below {:.4}: {}",
                options.min_category_pass_rate,
                failing_categories.join(", ")
            ),
        ));
    }

    gates.push(check_maximum_gate(
        "failed-iterations.maximum",
        baseline.failed_iterations,
        options.max_failed_iterations,
    ));
    gates.push(check_maximum_gate(
        "failed-scenario-runs.maximum",
        baseline.failed_scenario_runs,
        options.max_failed_scenario_runs,
    ));

    gates.push(check_optional_metadata_gate(
        "metadata.provider-label",
        baseline.baseline_metadata.provider_label.as_deref(),
        options.require_provider_label,
    ));
    gates.push(check_optional_metadata_gate(
        "metadata.model-label",
        baseline.baseline_metadata.model_label.as_deref(),
        options.require_model_label,
    ));
    gates.push(check_optional_metadata_gate(
        "metadata.run-label",
        baseline.baseline_metadata.run_label.as_deref(),
        options.require_run_label,
    ));

    if options.require_live_provider {
        if baseline.baseline_metadata.live_provider {
            gates.push(AcceptanceGateReport::pass(
                "metadata.live-provider",
                "baseline is marked as live provider",
            ));
        } else {
            gates.push(AcceptanceGateReport::fail(
                "metadata.live-provider",
                "baseline must be marked with --live-provider",
            ));
        }
    } else {
        gates.push(AcceptanceGateReport::pass(
            "metadata.live-provider",
            format!(
                "live_provider={} accepted by this gate",
                baseline.baseline_metadata.live_provider
            ),
        ));
    }

    let redaction_findings = redaction::findings(raw_json);
    if redaction_findings.is_empty() {
        gates.push(AcceptanceGateReport::pass(
            "redaction.scan",
            "no obvious secrets or local absolute paths found in baseline report",
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "redaction.scan",
            format!(
                "potentially sensitive pattern(s): {}",
                redaction_findings.join(", ")
            ),
        ));
    }

    if baseline.failure_summary.failed_checks.is_empty() {
        gates.push(AcceptanceGateReport::pass(
            "failure-summary.empty",
            "baseline failure summary has no failed checks",
        ));
    } else {
        let failed_checks = baseline
            .failure_summary
            .failed_checks
            .iter()
            .map(|check| format!("{}={}", check.check, check.count))
            .collect::<Vec<_>>();
        gates.push(AcceptanceGateReport::fail(
            "failure-summary.empty",
            format!("failed checks present: {}", failed_checks.join(", ")),
        ));
    }

    AcceptanceReport::from_gates(target, gates)
}

fn check_rate_gate(
    name: impl Into<String>,
    actual: f64,
    minimum: f64,
    label: &str,
) -> AcceptanceGateReport {
    if actual >= minimum {
        AcceptanceGateReport::pass(
            name,
            format!("{label} {:.4} met minimum {:.4}", actual, minimum),
        )
    } else {
        AcceptanceGateReport::fail(
            name,
            format!("{label} {:.4} below minimum {:.4}", actual, minimum),
        )
    }
}

fn check_maximum_gate(
    name: impl Into<String>,
    actual: usize,
    maximum: usize,
) -> AcceptanceGateReport {
    if actual <= maximum {
        AcceptanceGateReport::pass(name, format!("{actual} is within maximum {maximum}"))
    } else {
        AcceptanceGateReport::fail(name, format!("{actual} exceeds maximum {maximum}"))
    }
}

fn check_optional_metadata_gate(
    name: impl Into<String>,
    value: Option<&str>,
    required: bool,
) -> AcceptanceGateReport {
    match (required, value.filter(|value| !value.trim().is_empty())) {
        (true, Some(value)) => AcceptanceGateReport::pass(name, format!("present: {value}")),
        (true, None) => AcceptanceGateReport::fail(name, "required metadata label is missing"),
        (false, Some(value)) => AcceptanceGateReport::pass(name, format!("present: {value}")),
        (false, None) => AcceptanceGateReport::pass(name, "not required and not present"),
    }
}

fn baseline_report_for_paths(
    suite: &str,
    paths: &[PathBuf],
    options: &CommandOptions,
) -> Result<BaselineReport, EvalError> {
    let target = build_target(options.target_kind);
    let target_name = target.name().to_owned();
    let run_options = target_run_options(options);
    let target_metadata = target.metadata(&run_options);
    let scenarios = scenario_metadata_for_paths(paths)?;
    let scenario_ids = scenarios
        .iter()
        .map(|scenario| scenario.id.clone())
        .collect::<Vec<_>>();
    let mut runs = Vec::new();
    for iteration in 1..=options.iterations {
        let run = match eval_report_for_paths(paths, options) {
            Ok(report) => BaselineRunReport::from_eval_report(iteration, report),
            Err(error) => {
                BaselineRunReport::from_error(iteration, &scenario_ids, error.to_string())
            }
        };
        runs.push(run);
    }
    Ok(BaselineReport::from_runs(
        suite.to_owned(),
        target_name,
        target_metadata,
        baseline_metadata(options),
        scenarios,
        runs,
    ))
}

fn eval_report_for_paths(
    paths: &[PathBuf],
    options: &CommandOptions,
) -> Result<EvalReport, EvalError> {
    let target = build_target(options.target_kind);
    let target_name = target.name();
    let run_options = target_run_options(options);
    let target_metadata = target.metadata(&run_options);
    let mut results = Vec::new();
    for path in paths {
        let scenario = load_scenario(path)?;
        results.push(replay_scenario(
            &scenario,
            target.as_ref(),
            run_options.clone(),
        )?);
    }
    Ok(EvalReport::from_results(
        target_name,
        target_metadata,
        results,
    ))
}

fn scenario_metadata_for_paths(
    paths: &[PathBuf],
) -> Result<Vec<BaselineScenarioMetadata>, EvalError> {
    paths
        .iter()
        .map(|path| {
            load_scenario(path)
                .map(|scenario| BaselineScenarioMetadata::new(scenario.id, scenario.tags))
        })
        .collect()
}

fn target_run_options(options: &CommandOptions) -> TargetRunOptions {
    TargetRunOptions {
        fault_mode: options.fault_mode,
        command_extractor: command_extractor_options(options),
    }
}

fn command_extractor_options(options: &CommandOptions) -> Option<CommandExtractorOptions> {
    let program = options
        .extractor_command
        .clone()
        .or_else(|| env::var("MNEME_EVAL_EXTRACTOR_COMMAND").ok())
        .filter(|value| !value.trim().is_empty())?;
    Some(CommandExtractorOptions {
        program,
        args: options.extractor_args.clone(),
    })
}

fn baseline_metadata(options: &CommandOptions) -> BaselineMetadata {
    BaselineMetadata::new(
        options.live_provider,
        options.provider_label.clone(),
        options.model_label.clone(),
        options.run_label.clone(),
    )
}

fn parse_validate_args(
    raw_args: Vec<String>,
) -> Result<(ValidationTarget, CommandOptions), EvalError> {
    let mut path = None;
    let mut suite = None;
    let mut options = CommandOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--suite" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--suite requires a name"));
                };
                suite = Some(value.clone());
            }
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown validate option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli("validate accepts one scenario path"));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    match (path, suite) {
        (Some(path), None) => Ok((ValidationTarget::Scenario(path), options)),
        (None, Some(suite)) => Ok((ValidationTarget::Suite(suite), options)),
        (None, None) => Err(EvalError::invalid_cli(
            "usage: mneme-eval validate <scenario.yaml> [--json] [--report <path>] or mneme-eval validate --suite <name> [--json] [--report <path>]",
        )),
        (Some(_), Some(_)) => Err(EvalError::invalid_cli(
            "validate accepts either one scenario path or --suite, not both",
        )),
    }
}

fn parse_acceptance_args(raw_args: Vec<String>) -> Result<(String, CommandOptions), EvalError> {
    let mut suite = None;
    let mut options = CommandOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--suite" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--suite requires a name"));
                };
                suite = Some(value.clone());
            }
            "--target" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--target requires a name"));
                };
                options.target_kind = parse_target_kind(value)?;
            }
            "--extractor-command" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--extractor-command requires a program",
                    ));
                };
                options.extractor_command = Some(value.clone());
            }
            "--extractor-arg" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--extractor-arg requires a value"));
                };
                options.extractor_args.push(value.clone());
            }
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            value => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown acceptance option: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok((suite.unwrap_or_else(|| "core".to_owned()), options))
}

fn parse_replay_args(raw_args: Vec<String>) -> Result<(PathBuf, CommandOptions), EvalError> {
    let mut path = None;
    let mut options = CommandOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--target" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--target requires a name"));
                };
                options.target_kind = parse_target_kind(value)?;
            }
            "--extractor-command" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--extractor-command requires a program",
                    ));
                };
                options.extractor_command = Some(value.clone());
            }
            "--extractor-arg" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--extractor-arg requires a value"));
                };
                options.extractor_args.push(value.clone());
            }
            "--seeded-fault" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--seeded-fault requires a value"));
                };
                options.fault_mode = FaultMode::parse(value).ok_or_else(|| {
                    EvalError::invalid_cli(format!("unknown seeded fault: {value}"))
                })?;
            }
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown replay option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli("replay accepts one scenario path"));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    let Some(path) = path else {
        return Err(EvalError::invalid_cli(
            "usage: mneme-eval replay <scenario.yaml> [--target fake|mneme-v1|mneme-v1-command] [--extractor-command <program>] [--extractor-arg <arg>]... [--json] [--report <path>]",
        ));
    };
    Ok((path, options))
}

fn parse_suite_args(raw_args: Vec<String>) -> Result<(String, CommandOptions), EvalError> {
    let mut suite = None;
    let mut options = CommandOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--suite" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--suite requires a name"));
                };
                suite = Some(value.clone());
            }
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--target" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--target requires a name"));
                };
                options.target_kind = parse_target_kind(value)?;
            }
            "--extractor-command" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--extractor-command requires a program",
                    ));
                };
                options.extractor_command = Some(value.clone());
            }
            "--extractor-arg" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--extractor-arg requires a value"));
                };
                options.extractor_args.push(value.clone());
            }
            "--seeded-fault" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--seeded-fault requires a value"));
                };
                options.fault_mode = FaultMode::parse(value).ok_or_else(|| {
                    EvalError::invalid_cli(format!("unknown seeded fault: {value}"))
                })?;
            }
            value => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown run option: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok((suite.unwrap_or_else(|| "core".to_owned()), options))
}

fn parse_baseline_args(raw_args: Vec<String>) -> Result<(String, CommandOptions), EvalError> {
    let mut suite = None;
    let mut options = CommandOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--suite" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--suite requires a name"));
                };
                suite = Some(value.clone());
            }
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--target" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--target requires a name"));
                };
                options.target_kind = parse_target_kind(value)?;
            }
            "--extractor-command" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--extractor-command requires a program",
                    ));
                };
                options.extractor_command = Some(value.clone());
            }
            "--extractor-arg" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--extractor-arg requires a value"));
                };
                options.extractor_args.push(value.clone());
            }
            "--seeded-fault" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--seeded-fault requires a value"));
                };
                options.fault_mode = FaultMode::parse(value).ok_or_else(|| {
                    EvalError::invalid_cli(format!("unknown seeded fault: {value}"))
                })?;
            }
            "--iterations" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--iterations requires a value"));
                };
                options.iterations = parse_iterations(value)?;
            }
            "--live-provider" => options.live_provider = true,
            "--provider-label" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--provider-label requires a value"));
                };
                options.provider_label = Some(parse_label("--provider-label", value)?);
            }
            "--model-label" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--model-label requires a value"));
                };
                options.model_label = Some(parse_label("--model-label", value)?);
            }
            "--run-label" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--run-label requires a value"));
                };
                options.run_label = Some(parse_label("--run-label", value)?);
            }
            value => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown baseline option: {value}"
                )));
            }
        }
        idx += 1;
    }
    Ok((suite.unwrap_or_else(|| "core".to_owned()), options))
}

fn parse_baseline_gate_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, BaselineGateOptions), EvalError> {
    let mut path = None;
    let mut options = BaselineGateOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--min-pass-rate" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--min-pass-rate requires a value"));
                };
                options.min_pass_rate = parse_rate("--min-pass-rate", value)?;
            }
            "--min-category-pass-rate" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--min-category-pass-rate requires a value",
                    ));
                };
                options.min_category_pass_rate = parse_rate("--min-category-pass-rate", value)?;
            }
            "--max-failed-iterations" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--max-failed-iterations requires a value",
                    ));
                };
                options.max_failed_iterations =
                    parse_nonnegative_usize("--max-failed-iterations", value)?;
            }
            "--max-failed-scenario-runs" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--max-failed-scenario-runs requires a value",
                    ));
                };
                options.max_failed_scenario_runs =
                    parse_nonnegative_usize("--max-failed-scenario-runs", value)?;
            }
            "--require-live-provider" => options.require_live_provider = true,
            "--allow-missing-provider-label" => options.require_provider_label = false,
            "--allow-missing-model-label" => options.require_model_label = false,
            "--require-run-label" => options.require_run_label = true,
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown baseline-gate option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli(
                        "baseline-gate accepts one baseline report path",
                    ));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    let Some(path) = path else {
        return Err(EvalError::invalid_cli(
            "usage: mneme-eval baseline-gate <baseline-report.json> [--min-pass-rate <0..1>] [--min-category-pass-rate <0..1>] [--max-failed-iterations <n>] [--max-failed-scenario-runs <n>] [--require-live-provider] [--allow-missing-provider-label] [--allow-missing-model-label] [--require-run-label] [--json] [--report <path>]",
        ));
    };
    Ok((path, options))
}

fn parse_baseline_summary_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, BaselineSummaryOptions), EvalError> {
    let mut path = None;
    let mut options = BaselineSummaryOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown baseline-summary option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli(
                        "baseline-summary accepts one baseline report path",
                    ));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    let Some(path) = path else {
        return Err(EvalError::invalid_cli(
            "usage: mneme-eval baseline-summary <baseline-report.json> [--json] [--report <path>]",
        ));
    };
    Ok((path, options))
}

fn parse_baseline_compare_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, PathBuf, BaselineCompareOptions), EvalError> {
    let mut paths = Vec::new();
    let mut options = BaselineCompareOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--max-pass-rate-drop" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--max-pass-rate-drop requires a value",
                    ));
                };
                options.max_pass_rate_drop = parse_rate("--max-pass-rate-drop", value)?;
            }
            "--max-category-drop" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli(
                        "--max-category-drop requires a value",
                    ));
                };
                options.max_category_drop = parse_rate("--max-category-drop", value)?;
            }
            "--fail-on-regression" => options.fail_on_regression = true,
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown baseline-compare option: {value}"
                )));
            }
            value => paths.push(PathBuf::from(value)),
        }
        idx += 1;
    }
    match paths.as_slice() {
        [before, after] => Ok((before.clone(), after.clone(), options)),
        _ => Err(EvalError::invalid_cli(
            "usage: mneme-eval baseline-compare <before-baseline.json> <after-baseline.json> [--max-pass-rate-drop <0..1>] [--max-category-drop <0..1>] [--fail-on-regression] [--json] [--report <path>]",
        )),
    }
}

fn parse_candidate_args(raw_args: Vec<String>) -> Result<(PathBuf, CandidateOptions), EvalError> {
    let mut path = None;
    let mut options = CandidateOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--out-dir" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--out-dir requires a path"));
                };
                options.out_dir = PathBuf::from(value);
            }
            "--prefix" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--prefix requires a value"));
                };
                options.prefix = parse_label("--prefix", value)?;
            }
            "--limit" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--limit requires a value"));
                };
                let limit = parse_nonnegative_usize("--limit", value)?;
                if limit == 0 {
                    return Err(EvalError::invalid_cli("--limit must be greater than zero"));
                }
                options.limit = Some(limit);
            }
            "--suite" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--suite requires a name"));
                };
                options.suite = Some(parse_label("--suite", value)?);
            }
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown candidate option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli(
                        "candidate accepts one eval or baseline report path",
                    ));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    let Some(path) = path else {
        return Err(EvalError::invalid_cli(
            "usage: mneme-eval candidate <eval-or-baseline-report.json> [--out-dir <dir>] [--prefix <label>] [--limit <n>] [--suite <name>] [--json] [--report <path>]",
        ));
    };
    Ok((path, options))
}

fn parse_candidate_check_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, CandidateCheckOptions), EvalError> {
    let mut path = None;
    let mut options = CandidateCheckOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown candidate-check option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli(
                        "candidate-check accepts one candidate file or directory path",
                    ));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    let Some(path) = path else {
        return Err(EvalError::invalid_cli(
            "usage: mneme-eval candidate-check <candidate.yaml|dir> [--json] [--report <path>]",
        ));
    };
    Ok((path, options))
}

fn parse_candidate_promote_args(
    raw_args: Vec<String>,
) -> Result<(PathBuf, CandidatePromoteOptions), EvalError> {
    let mut path = None;
    let mut options = CandidatePromoteOptions::default();
    let mut idx = 0;
    while idx < raw_args.len() {
        match raw_args[idx].as_str() {
            "--json" => options.json = true,
            "--report" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--report requires a path"));
                };
                options.report_path = Some(PathBuf::from(value));
            }
            "--scenario-root" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--scenario-root requires a path"));
                };
                options.scenario_root = PathBuf::from(value);
            }
            "--suite" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--suite requires a name"));
                };
                options.suite = Some(parse_label("--suite", value)?);
            }
            "--filename" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--filename requires a name"));
                };
                options.filename = Some(value.clone());
            }
            "--apply" => options.apply = true,
            value if value.starts_with('-') => {
                return Err(EvalError::invalid_cli(format!(
                    "unknown candidate-promote option: {value}"
                )));
            }
            value => {
                if path.is_some() {
                    return Err(EvalError::invalid_cli(
                        "candidate-promote accepts one candidate file path",
                    ));
                }
                path = Some(PathBuf::from(value));
            }
        }
        idx += 1;
    }
    let Some(path) = path else {
        return Err(EvalError::invalid_cli(
            "usage: mneme-eval candidate-promote <candidate.yaml> [--suite <name>] [--filename <name.yaml>] [--scenario-root <dir>] [--apply] [--json] [--report <path>]",
        ));
    };
    Ok((path, options))
}

fn parse_iterations(value: &str) -> Result<usize, EvalError> {
    let iterations = value.parse::<usize>().map_err(|_| {
        EvalError::invalid_cli(format!("--iterations must be a positive integer: {value}"))
    })?;
    if !(1..=MAX_BASELINE_ITERATIONS).contains(&iterations) {
        return Err(EvalError::invalid_cli(format!(
            "--iterations must be between 1 and {MAX_BASELINE_ITERATIONS}"
        )));
    }
    Ok(iterations)
}

fn parse_nonnegative_usize(option: &str, value: &str) -> Result<usize, EvalError> {
    value.parse::<usize>().map_err(|_| {
        EvalError::invalid_cli(format!("{option} must be a non-negative integer: {value}"))
    })
}

fn parse_rate(option: &str, value: &str) -> Result<f64, EvalError> {
    let rate = value
        .parse::<f64>()
        .map_err(|_| EvalError::invalid_cli(format!("{option} must be a number: {value}")))?;
    if !(0.0..=1.0).contains(&rate) {
        return Err(EvalError::invalid_cli(format!(
            "{option} must be between 0.0 and 1.0"
        )));
    }
    Ok(rate)
}

fn parse_label(option: &str, value: &str) -> Result<String, EvalError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(EvalError::invalid_cli(format!(
            "{option} must not be empty"
        )));
    }
    if trimmed.len() > 80 {
        return Err(EvalError::invalid_cli(format!(
            "{option} must be 80 characters or fewer"
        )));
    }
    if !trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/'))
    {
        return Err(EvalError::invalid_cli(format!(
            "{option} may contain only ASCII letters, digits, '-', '_', '.', or '/'"
        )));
    }
    Ok(trimmed.to_owned())
}

fn parse_target_kind(value: &str) -> Result<TargetKind, EvalError> {
    TargetKind::parse(value).ok_or_else(|| {
        EvalError::invalid_cli(format!(
            "unknown eval target: {value}\navailable targets: {}",
            TargetKind::available()
        ))
    })
}

fn scenario_paths_for_suite(suite: &str) -> Result<Vec<PathBuf>, EvalError> {
    let root = env::current_dir()
        .map_err(|source| EvalError::io("read current dir", Path::new("."), source))?;
    let suite_dir = root.join("evals").join("scenarios").join(suite);
    let mut paths = Vec::new();
    collect_scenario_paths(&suite_dir, &mut paths)?;
    paths.sort();
    if paths.is_empty() {
        return Err(EvalError::scenario(format!(
            "suite {suite} has no scenario files at {}",
            suite_dir.display()
        )));
    }
    Ok(paths)
}

fn invalid_fixture_paths() -> Result<Vec<PathBuf>, EvalError> {
    let root = env::current_dir()
        .map_err(|source| EvalError::io("read current dir", Path::new("."), source))?;
    let invalid_dir = root.join("evals").join("fixtures").join("invalid");
    let mut paths = Vec::new();
    collect_scenario_paths(&invalid_dir, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_scenario_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), EvalError> {
    let entries = fs::read_dir(dir).map_err(|source| EvalError::io("read dir", dir, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| EvalError::io("read dir entry", dir, source))?;
        let path = entry.path();
        if path.is_dir() {
            collect_scenario_paths(&path, paths)?;
        } else if is_scenario_file(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_scenario_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
}

fn emit_report(report: &EvalReport, options: &CommandOptions) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_human_report(report);
    }
    Ok(())
}

fn emit_validation_report(
    report: &ValidationReport,
    options: &CommandOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_validation_report(report);
    }
    Ok(())
}

fn emit_acceptance_report(
    report: &AcceptanceReport,
    options: &CommandOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_acceptance_report(report);
    }
    Ok(())
}

fn emit_baseline_report(
    report: &BaselineReport,
    options: &CommandOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_baseline_report(report);
    }
    Ok(())
}

fn emit_baseline_gate_report(
    report: &AcceptanceReport,
    options: &BaselineGateOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_acceptance_report(report);
    }
    Ok(())
}

fn emit_baseline_summary_report(
    report: &BaselineSummaryReport,
    options: &BaselineSummaryOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_baseline_summary_report(report);
    }
    Ok(())
}

fn emit_baseline_compare_report(
    report: &BaselineCompareReport,
    options: &BaselineCompareOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_baseline_compare_report(report);
    }
    Ok(())
}

fn emit_candidate_report(
    report: &CandidateReport,
    options: &CandidateOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_candidate_report(report);
    }
    Ok(())
}

fn emit_candidate_check_report(
    report: &CandidateCheckReport,
    options: &CandidateCheckOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_candidate_check_report(report);
    }
    Ok(())
}

fn emit_candidate_promote_report(
    report: &CandidatePromoteReport,
    options: &CandidatePromoteOptions,
) -> Result<(), EvalError> {
    if let Some(path) = &options.report_path {
        write_report(path, report)?;
    }
    if options.json {
        let json = serde_json::to_string_pretty(report)
            .map_err(|source| EvalError::json(Path::new("<stdout>"), source))?;
        println!("{json}");
    } else {
        print_candidate_promote_report(report);
    }
    Ok(())
}

fn write_report<T: Serialize>(path: &Path, report: &T) -> Result<(), EvalError> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|source| EvalError::io("create dir", parent, source))?;
    }
    let json =
        serde_json::to_string_pretty(report).map_err(|source| EvalError::json(path, source))?;
    fs::write(path, format!("{json}\n")).map_err(|source| EvalError::io("write", path, source))
}

fn print_human_report(report: &EvalReport) {
    println!(
        "eval: target={}, {} scenario(s), {} passed, {} failed",
        report.target, report.scenario_count, report.passed, report.failed
    );
    for scenario in &report.results {
        print_scenario_report(scenario);
    }
}

fn print_validation_report(report: &ValidationReport) {
    println!(
        "validation: {} scenario(s), {} valid, {} invalid",
        report.scenario_count, report.valid, report.invalid
    );
    for result in &report.results {
        let status = if result.ok { "PASS" } else { "FAIL" };
        match &result.scenario_id {
            Some(scenario_id) => println!("- {status} {} ({scenario_id})", result.path),
            None => println!("- {status} {}", result.path),
        }
        if let Some(error) = &result.error {
            println!("  - {error}");
        }
    }
}

fn print_acceptance_report(report: &AcceptanceReport) {
    println!(
        "acceptance: target={}, {} gate(s), {} passed, {} failed",
        report.target, report.gate_count, report.passed, report.failed
    );
    for gate in &report.gates {
        let status = if gate.status == crate::report::CheckStatus::Pass {
            "PASS"
        } else {
            "FAIL"
        };
        println!("- {status} {}: {}", gate.name, gate.detail);
    }
}

fn print_baseline_report(report: &BaselineReport) {
    println!(
        "baseline: suite={}, target={}, {} iteration(s), pass_rate={:.2}%, {} passed, {} failed",
        report.suite,
        report.target,
        report.iterations,
        report.pass_rate * 100.0,
        report.passed_iterations,
        report.failed_iterations
    );
    for run in &report.runs {
        let status = if run.ok { "PASS" } else { "FAIL" };
        println!(
            "- {status} iteration {}: {}/{} scenario(s)",
            run.iteration, run.passed, run.scenario_count
        );
        if let Some(error) = &run.error {
            println!("  - error={error}");
        }
        for result in &run.results {
            if result.ok {
                continue;
            }
            if result.failed_checks.is_empty() {
                println!("  - {} failed", result.scenario_id);
            } else {
                println!(
                    "  - {} failed checks={}",
                    result.scenario_id,
                    result.failed_checks.join(",")
                );
            }
        }
    }
}

fn print_baseline_summary_report(report: &BaselineSummaryReport) {
    println!(
        "baseline-summary: source={}, suite={}, target={}, status={}, pass_rate={:.2}%, failed_iterations={}, failed_scenario_runs={}",
        report.source,
        report.suite,
        report.target,
        report.triage_status,
        report.pass_rate * 100.0,
        report.failed_iterations,
        report.failed_scenario_runs
    );
    println!(
        "metadata: live_provider={} provider={} model={} run={}",
        report.baseline_metadata.live_provider,
        option_label(report.baseline_metadata.provider_label.as_deref()),
        option_label(report.baseline_metadata.model_label.as_deref()),
        option_label(report.baseline_metadata.run_label.as_deref())
    );
    if report.redaction_findings.is_empty() {
        println!("redaction: ok");
    } else {
        println!(
            "redaction: findings={}",
            report.redaction_findings.join(", ")
        );
    }
    print_summary_categories(&report.top_failed_categories);
    print_summary_scenarios(&report.top_failed_scenarios);
    print_summary_checks(&report.top_failed_checks);
    println!("next:");
    for action in &report.recommended_next_actions {
        println!("- {action}");
    }
}

fn print_baseline_compare_report(report: &BaselineCompareReport) {
    println!(
        "baseline-compare: suite={}, target={}, regression={}, pass_rate={:.2}% -> {:.2}% ({:+.2} pp)",
        report.suite,
        report.target,
        report.regression_detected,
        report.aggregate.before_pass_rate * 100.0,
        report.aggregate.after_pass_rate * 100.0,
        report.aggregate.pass_rate_delta * 100.0
    );
    if !report.comparable {
        println!("comparable: false");
    }
    if !report.redaction_findings.is_empty() {
        println!(
            "redaction: findings={}",
            report.redaction_findings.join(", ")
        );
    }
    println!(
        "failed_scenario_runs: {} -> {} ({:+})",
        report.aggregate.before_failed_scenario_runs,
        report.aggregate.after_failed_scenario_runs,
        report.aggregate.failed_scenario_runs_delta
    );
    print_rate_changes(
        "regressed categories",
        &report.category_changes,
        "regressed",
    );
    print_rate_changes("regressed scenarios", &report.scenario_changes, "regressed");
    if report.new_failed_scenarios.is_empty() {
        println!("new failed scenarios: none");
    } else {
        println!(
            "new failed scenarios: {}",
            report.new_failed_scenarios.join(", ")
        );
    }
    if report.resolved_scenarios.is_empty() {
        println!("resolved scenarios: none");
    } else {
        println!(
            "resolved scenarios: {}",
            report.resolved_scenarios.join(", ")
        );
    }
    if report.new_failed_checks.is_empty() {
        println!("new failed checks: none");
    } else {
        println!("new failed checks:");
        for check in &report.new_failed_checks {
            println!(
                "- {}: {} -> {} ({:+})",
                check.check, check.before_count, check.after_count, check.delta
            );
        }
    }
    if !report.notes.is_empty() {
        println!("notes:");
        for note in &report.notes {
            println!("- {note}");
        }
    }
    println!("next:");
    for action in &report.recommended_next_actions {
        println!("- {action}");
    }
}

fn print_rate_changes(label: &str, changes: &[crate::trend::RateDelta], status: &str) {
    let matching = changes
        .iter()
        .filter(|change| change.status == status)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        println!("{label}: none");
        return;
    }
    println!("{label}:");
    for change in matching {
        let before = change.before_pass_rate.unwrap_or(0.0) * 100.0;
        let after = change.after_pass_rate.unwrap_or(0.0) * 100.0;
        let delta = change.pass_rate_delta.unwrap_or(0.0) * 100.0;
        println!(
            "- {}: {:.2}% -> {:.2}% ({:+.2} pp), failed {} -> {}",
            change.name, before, after, delta, change.before_failed, change.after_failed
        );
    }
}

fn option_label(value: Option<&str>) -> &str {
    value
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("<missing>")
}

fn print_summary_categories(categories: &[BaselineCategorySummary]) {
    if categories.is_empty() {
        println!("top failed categories: none");
        return;
    }
    println!("top failed categories:");
    for category in categories {
        println!(
            "- {}: failed={}/{} pass_rate={:.2}%",
            category.category,
            category.failed,
            category.attempts,
            category.pass_rate * 100.0
        );
    }
}

fn print_summary_scenarios(scenarios: &[BaselineScenarioSummary]) {
    if scenarios.is_empty() {
        println!("top failed scenarios: none");
        return;
    }
    println!("top failed scenarios:");
    for scenario in scenarios {
        println!(
            "- {}: failed={}/{} pass_rate={:.2}%",
            scenario.scenario_id,
            scenario.failed,
            scenario.attempts,
            scenario.pass_rate * 100.0
        );
    }
}

fn print_summary_checks(checks: &[BaselineFailedCheckSummary]) {
    if checks.is_empty() {
        println!("top failed checks: none");
        return;
    }
    println!("top failed checks:");
    for check in checks {
        println!("- {}: count={}", check.check, check.count);
    }
}

fn print_candidate_report(report: &CandidateReport) {
    println!(
        "candidate: source={}, kind={}, target={}, generated={}, failed_scenarios={}, out_dir={}",
        report.source,
        report.source_kind,
        report.target,
        report.candidate_count,
        report.failed_scenario_count,
        report.out_dir
    );
    if !report.redaction_finding_codes.is_empty() {
        println!(
            "- redaction sanitized codes={}",
            report.redaction_finding_codes.join(",")
        );
    }
    for candidate in &report.candidates {
        println!(
            "- {} {} -> {} (failed_attempts={}, failed_checks={})",
            candidate.status,
            candidate.source_scenario_id,
            candidate.path,
            candidate.failed_attempts,
            candidate.failed_check_count
        );
    }
    for action in &report.recommended_next_actions {
        println!("- next {action}");
    }
}

fn print_candidate_check_report(report: &CandidateCheckReport) {
    println!(
        "candidate-check: source={}, {} checked, {} valid, {} invalid",
        report.source, report.checked_count, report.valid, report.invalid
    );
    for result in &report.results {
        let status = if result.ok { "PASS" } else { "FAIL" };
        match &result.candidate_id {
            Some(candidate_id) => println!("- {status} {} ({candidate_id})", result.path),
            None => println!("- {status} {}", result.path),
        }
        for error in &result.errors {
            println!("  - {error}");
        }
    }
}

fn print_candidate_promote_report(report: &CandidatePromoteReport) {
    println!(
        "candidate-promote: candidate={}, suite={}, destination={}, applied={}, ok={}",
        report.candidate_id, report.target_suite, report.destination, report.applied, report.ok
    );
    for gate in &report.gates {
        let status = if gate.status == crate::report::CheckStatus::Pass {
            "PASS"
        } else {
            "FAIL"
        };
        println!("- {status} {}: {}", gate.name, gate.detail);
    }
    println!("next:");
    for action in &report.recommended_next_actions {
        println!("- {action}");
    }
}

fn print_scenario_report(scenario: &ScenarioReport) {
    let status = if scenario.ok { "PASS" } else { "FAIL" };
    println!("- {status} {}", scenario.scenario_id);
    for check in &scenario.checks {
        if scenario.ok {
            continue;
        }
        if check.status == crate::report::CheckStatus::Fail {
            let artifact = check.artifact.as_deref().unwrap_or("n/a");
            println!(
                "  - {} expected={} actual={} artifact={}",
                check.name, check.expected, check.actual, artifact
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_help_includes_usage_examples() {
        let help = command_help("baseline-gate").expect("baseline-gate help");
        assert!(help.contains("Usage: mneme-eval baseline-gate"));
        assert!(help.contains("baseline-report.json"));

        let summary_help = command_help("baseline-summary").expect("baseline-summary help");
        assert!(summary_help.contains("Usage: mneme-eval baseline-summary"));
        assert!(summary_help.contains("--report <path>"));

        let compare_help = command_help("baseline-compare").expect("baseline-compare help");
        assert!(compare_help.contains("Usage: mneme-eval baseline-compare"));
        assert!(compare_help.contains("--fail-on-regression"));

        let candidate_help = command_help("candidate").expect("candidate help");
        assert!(candidate_help.contains("Usage: mneme-eval candidate"));
        assert!(candidate_help.contains("--out-dir <dir>"));

        let candidate_check_help = command_help("candidate-check").expect("candidate-check help");
        assert!(candidate_check_help.contains("Usage: mneme-eval candidate-check"));
        assert!(candidate_check_help.contains("candidate artifacts"));

        let candidate_promote_help =
            command_help("candidate-promote").expect("candidate-promote help");
        assert!(candidate_promote_help.contains("Usage: mneme-eval candidate-promote"));
        assert!(candidate_promote_help.contains("--apply"));

        let general = command_help("run").expect("run help");
        assert!(general.contains("mneme-eval run"));
        assert!(general.contains("--target"));
    }

    #[test]
    fn unknown_command_points_to_help() {
        let result = run_cli(vec!["mneme-eval".to_owned(), "unknown".to_owned()]);
        let error = result.expect_err("unknown command should fail");
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("mneme-eval help"));
    }

    #[test]
    fn parse_replay_requires_path() {
        let result = parse_replay_args(vec!["--json".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_replay_accepts_explicit_fake_target() -> Result<(), EvalError> {
        let (_, options) = parse_replay_args(vec![
            "scenario.yaml".to_owned(),
            "--target".to_owned(),
            "fake".to_owned(),
        ])?;
        assert_eq!(options.target_kind, TargetKind::Fake);
        Ok(())
    }

    #[test]
    fn parse_suite_rejects_unknown_target() {
        let result = parse_suite_args(vec![
            "--suite".to_owned(),
            "core".to_owned(),
            "--target".to_owned(),
            "mneme-v2".to_owned(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_suite_accepts_mneme_v1_target() -> Result<(), EvalError> {
        let (_, options) = parse_suite_args(vec![
            "--suite".to_owned(),
            "core".to_owned(),
            "--target".to_owned(),
            "mneme-v1".to_owned(),
        ])?;
        assert_eq!(options.target_kind, TargetKind::MnemeV1);
        Ok(())
    }

    #[test]
    fn parse_suite_accepts_command_target_and_extractor_command() -> Result<(), EvalError> {
        let (_, options) = parse_suite_args(vec![
            "--suite".to_owned(),
            "model".to_owned(),
            "--target".to_owned(),
            "mneme-v1-command".to_owned(),
            "--extractor-command".to_owned(),
            "evals/fixtures/command-extractor.sh".to_owned(),
            "--extractor-arg".to_owned(),
            "--example".to_owned(),
        ])?;
        assert_eq!(options.target_kind, TargetKind::MnemeV1Command);
        assert_eq!(
            options.extractor_command.as_deref(),
            Some("evals/fixtures/command-extractor.sh")
        );
        assert_eq!(options.extractor_args, vec!["--example"]);
        Ok(())
    }

    #[test]
    fn parse_acceptance_defaults_to_core_fake() -> Result<(), EvalError> {
        let (suite, options) = parse_acceptance_args(Vec::new())?;
        assert_eq!(suite, "core");
        assert_eq!(options.target_kind, TargetKind::Fake);
        Ok(())
    }

    #[test]
    fn parse_acceptance_accepts_suite_and_report_options() -> Result<(), EvalError> {
        let (suite, options) = parse_acceptance_args(vec![
            "--suite".to_owned(),
            "core".to_owned(),
            "--target".to_owned(),
            "fake".to_owned(),
            "--json".to_owned(),
            "--report".to_owned(),
            "evals/reports/acceptance.json".to_owned(),
        ])?;
        assert_eq!(suite, "core");
        assert_eq!(options.target_kind, TargetKind::Fake);
        assert!(options.json);
        assert_eq!(
            options.report_path.as_deref(),
            Some(Path::new("evals/reports/acceptance.json"))
        );
        Ok(())
    }

    #[test]
    fn parse_baseline_accepts_iterations_and_command_target() -> Result<(), EvalError> {
        let (suite, options) = parse_baseline_args(vec![
            "--suite".to_owned(),
            "model".to_owned(),
            "--target".to_owned(),
            "mneme-v1-command".to_owned(),
            "--extractor-command".to_owned(),
            "wrappers/openai_extractor.py".to_owned(),
            "--iterations".to_owned(),
            "2".to_owned(),
            "--provider-label".to_owned(),
            "openai".to_owned(),
            "--model-label".to_owned(),
            "gpt-5.4-mini".to_owned(),
            "--run-label".to_owned(),
            "local-baseline".to_owned(),
            "--live-provider".to_owned(),
            "--seeded-fault".to_owned(),
            "skip-claims".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(suite, "model");
        assert_eq!(options.target_kind, TargetKind::MnemeV1Command);
        assert_eq!(options.iterations, 2);
        assert!(options.live_provider);
        assert_eq!(options.provider_label.as_deref(), Some("openai"));
        assert_eq!(options.model_label.as_deref(), Some("gpt-5.4-mini"));
        assert_eq!(options.run_label.as_deref(), Some("local-baseline"));
        assert_eq!(options.fault_mode, FaultMode::SkipClaims);
        assert!(options.json);
        assert_eq!(
            options.extractor_command.as_deref(),
            Some("wrappers/openai_extractor.py")
        );
        Ok(())
    }

    #[test]
    fn parse_baseline_rejects_zero_iterations() {
        let result = parse_baseline_args(vec![
            "--suite".to_owned(),
            "model".to_owned(),
            "--iterations".to_owned(),
            "0".to_owned(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_baseline_rejects_invalid_labels() {
        let result = parse_baseline_args(vec!["--provider-label".to_owned(), "open ai".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_baseline_gate_accepts_thresholds() -> Result<(), EvalError> {
        let (path, options) = parse_baseline_gate_args(vec![
            "evals/reports/openai-live-baseline.json".to_owned(),
            "--min-pass-rate".to_owned(),
            "0.95".to_owned(),
            "--min-category-pass-rate".to_owned(),
            "0.9".to_owned(),
            "--max-failed-iterations".to_owned(),
            "1".to_owned(),
            "--max-failed-scenario-runs".to_owned(),
            "2".to_owned(),
            "--require-live-provider".to_owned(),
            "--require-run-label".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(
            path,
            PathBuf::from("evals/reports/openai-live-baseline.json")
        );
        assert_eq!(options.min_pass_rate, 0.95);
        assert_eq!(options.min_category_pass_rate, 0.9);
        assert_eq!(options.max_failed_iterations, 1);
        assert_eq!(options.max_failed_scenario_runs, 2);
        assert!(options.require_live_provider);
        assert!(options.require_run_label);
        assert!(options.json);
        Ok(())
    }

    #[test]
    fn parse_baseline_gate_rejects_invalid_rate() {
        let result = parse_baseline_gate_args(vec![
            "baseline.json".to_owned(),
            "--min-pass-rate".to_owned(),
            "1.5".to_owned(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_baseline_summary_accepts_report_options() -> Result<(), EvalError> {
        let (path, options) = parse_baseline_summary_args(vec![
            "evals/reports/openai-live-baseline.json".to_owned(),
            "--report".to_owned(),
            "evals/reports/openai-live-baseline.summary.json".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(
            path,
            PathBuf::from("evals/reports/openai-live-baseline.json")
        );
        assert_eq!(
            options.report_path.as_deref(),
            Some(Path::new("evals/reports/openai-live-baseline.summary.json"))
        );
        assert!(options.json);
        Ok(())
    }

    #[test]
    fn parse_baseline_compare_accepts_regression_options() -> Result<(), EvalError> {
        let (before, after, options) = parse_baseline_compare_args(vec![
            "evals/reports/before.json".to_owned(),
            "evals/reports/after.json".to_owned(),
            "--max-pass-rate-drop".to_owned(),
            "0.02".to_owned(),
            "--max-category-drop".to_owned(),
            "0.05".to_owned(),
            "--fail-on-regression".to_owned(),
            "--report".to_owned(),
            "evals/reports/compare.json".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(before, PathBuf::from("evals/reports/before.json"));
        assert_eq!(after, PathBuf::from("evals/reports/after.json"));
        assert_eq!(options.max_pass_rate_drop, 0.02);
        assert_eq!(options.max_category_drop, 0.05);
        assert!(options.fail_on_regression);
        assert!(options.json);
        assert_eq!(
            options.report_path.as_deref(),
            Some(Path::new("evals/reports/compare.json"))
        );
        Ok(())
    }

    #[test]
    fn parse_candidate_accepts_generation_options() -> Result<(), EvalError> {
        let (path, options) = parse_candidate_args(vec![
            "evals/reports/baseline.json".to_owned(),
            "--out-dir".to_owned(),
            "evals/candidates/local".to_owned(),
            "--prefix".to_owned(),
            "dogfood".to_owned(),
            "--limit".to_owned(),
            "3".to_owned(),
            "--suite".to_owned(),
            "model".to_owned(),
            "--report".to_owned(),
            "evals/reports/candidate.json".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(path, PathBuf::from("evals/reports/baseline.json"));
        assert_eq!(options.out_dir, PathBuf::from("evals/candidates/local"));
        assert_eq!(options.prefix, "dogfood");
        assert_eq!(options.limit, Some(3));
        assert_eq!(options.suite.as_deref(), Some("model"));
        assert_eq!(
            options.report_path.as_deref(),
            Some(Path::new("evals/reports/candidate.json"))
        );
        assert!(options.json);
        Ok(())
    }

    #[test]
    fn parse_candidate_check_accepts_report_options() -> Result<(), EvalError> {
        let (path, options) = parse_candidate_check_args(vec![
            "evals/candidates/local".to_owned(),
            "--report".to_owned(),
            "evals/reports/candidate-check.json".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(path, PathBuf::from("evals/candidates/local"));
        assert_eq!(
            options.report_path.as_deref(),
            Some(Path::new("evals/reports/candidate-check.json"))
        );
        assert!(options.json);
        Ok(())
    }

    #[test]
    fn parse_candidate_promote_accepts_apply_options() -> Result<(), EvalError> {
        let (path, options) = parse_candidate_promote_args(vec![
            "evals/candidates/local/example.candidate.yaml".to_owned(),
            "--suite".to_owned(),
            "model".to_owned(),
            "--filename".to_owned(),
            "dogfood-example.yaml".to_owned(),
            "--scenario-root".to_owned(),
            "/tmp/mneme-scenarios".to_owned(),
            "--apply".to_owned(),
            "--report".to_owned(),
            "evals/reports/promote.json".to_owned(),
            "--json".to_owned(),
        ])?;
        assert_eq!(
            path,
            PathBuf::from("evals/candidates/local/example.candidate.yaml")
        );
        assert_eq!(options.suite.as_deref(), Some("model"));
        assert_eq!(options.filename.as_deref(), Some("dogfood-example.yaml"));
        assert_eq!(options.scenario_root, PathBuf::from("/tmp/mneme-scenarios"));
        assert!(options.apply);
        assert!(options.json);
        assert_eq!(
            options.report_path.as_deref(),
            Some(Path::new("evals/reports/promote.json"))
        );
        Ok(())
    }

    #[test]
    fn baseline_gate_accepts_passing_dry_run_report() -> Result<(), Box<dyn std::error::Error>> {
        let report = passing_baseline_report(false);
        let raw_json = serde_json::to_string(&report)?;
        let gate = build_baseline_gate_report(
            Path::new("baseline.json"),
            &raw_json,
            &report,
            &BaselineGateOptions::default(),
        );
        assert!(gate.ok);
        assert_eq!(gate.failed, 0);
        Ok(())
    }

    #[test]
    fn baseline_gate_requires_live_provider_when_configured(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let report = passing_baseline_report(false);
        let raw_json = serde_json::to_string(&report)?;
        let options = BaselineGateOptions {
            require_live_provider: true,
            ..BaselineGateOptions::default()
        };
        let gate =
            build_baseline_gate_report(Path::new("baseline.json"), &raw_json, &report, &options);
        assert!(!gate.ok);
        assert!(gate
            .gates
            .iter()
            .any(|gate| gate.name == "metadata.live-provider" && gate.detail.contains("must")));
        Ok(())
    }

    #[test]
    fn baseline_gate_flags_redaction_findings() -> Result<(), Box<dyn std::error::Error>> {
        let report = passing_baseline_report(true);
        let key_name = format!("{}{}", "OPENAI_", "API_KEY");
        let raw_json = format!(
            "{}\n{}=placeholder\n",
            serde_json::to_string(&report)?,
            key_name
        );
        let expected_finding = key_name;
        let gate = build_baseline_gate_report(
            Path::new("baseline.json"),
            &raw_json,
            &report,
            &BaselineGateOptions::default(),
        );
        assert!(!gate.ok);
        assert!(gate.gates.iter().any(|gate| {
            gate.name == "redaction.scan" && gate.detail.contains(&expected_finding)
        }));
        Ok(())
    }

    #[test]
    fn baseline_gate_flags_key_prefix_findings() -> Result<(), Box<dyn std::error::Error>> {
        let report = passing_baseline_report(true);
        let raw_json = format!(
            "{}\n{}{}example\n",
            serde_json::to_string(&report)?,
            "s",
            "k-"
        );
        let gate = build_baseline_gate_report(
            Path::new("baseline.json"),
            &raw_json,
            &report,
            &BaselineGateOptions::default(),
        );
        assert!(!gate.ok);
        assert!(gate
            .gates
            .iter()
            .any(|gate| gate.name == "redaction.scan" && gate.detail.contains("sk-")));
        Ok(())
    }

    #[test]
    fn baseline_summary_reports_failed_triage() -> Result<(), Box<dyn std::error::Error>> {
        let report = failing_baseline_report(true);
        let raw_json = serde_json::to_string(&report)?;
        let summary = build_baseline_summary_report(Path::new("baseline.json"), &raw_json, &report);
        assert_eq!(summary.command, "baseline-summary");
        assert_eq!(summary.triage_status, "failing");
        assert_eq!(summary.failed_category_count, 1);
        assert_eq!(summary.failed_scenario_count, 1);
        assert_eq!(summary.failed_check_count, 1);
        assert_eq!(summary.top_failed_categories[0].category, "recall");
        assert_eq!(summary.top_failed_scenarios[0].scenario_id, "scenario-a");
        assert_eq!(summary.top_failed_checks[0].check, "check");
        assert!(summary
            .recommended_next_actions
            .iter()
            .any(|action| action.contains("replay scenario `scenario-a`")));
        Ok(())
    }

    #[test]
    fn baseline_summary_reports_redaction_findings() -> Result<(), Box<dyn std::error::Error>> {
        let report = passing_baseline_report(true);
        let raw_json = format!(
            "{}\n{}{}example\n",
            serde_json::to_string(&report)?,
            "s",
            "k-"
        );
        let summary = build_baseline_summary_report(Path::new("baseline.json"), &raw_json, &report);
        assert_eq!(summary.triage_status, "redaction_required");
        assert_eq!(summary.redaction_findings, vec!["sk-"]);
        assert!(summary
            .recommended_next_actions
            .iter()
            .any(|action| action.contains("redact or keep local")));
        Ok(())
    }

    #[test]
    fn parse_validate_requires_a_target() {
        let result = parse_validate_args(vec!["--json".to_owned()]);
        assert!(result.is_err());
    }

    #[test]
    fn parse_validate_rejects_mixed_targets() {
        let result = parse_validate_args(vec![
            "scenario.yaml".to_owned(),
            "--suite".to_owned(),
            "core".to_owned(),
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn validate_paths_reports_invalid_scenarios() -> Result<(), Box<dyn std::error::Error>> {
        let path = env::temp_dir().join(format!("mneme-eval-invalid-{}.yaml", std::process::id()));
        fs::write(
            &path,
            "id: invalid-empty-events\nevents: []\nexpected:\n  event_append:\n    count: 1\n",
        )?;

        let report = validate_paths(vec![path.clone()]);
        let _ = fs::remove_file(path);

        assert!(!report.ok);
        assert_eq!(report.invalid, 1);
        assert!(report.results[0]
            .error
            .as_deref()
            .is_some_and(|error| error.contains("has no events")));
        Ok(())
    }

    #[test]
    fn scenario_file_filter_accepts_yaml_only() {
        assert!(is_scenario_file(Path::new("a.yaml")));
        assert!(is_scenario_file(Path::new("a.yml")));
        assert!(!is_scenario_file(Path::new(".gitkeep")));
    }

    fn passing_baseline_report(live_provider: bool) -> BaselineReport {
        let passed = EvalReport::from_results(
            TargetKind::MnemeV1Command.as_str(),
            crate::target::EvalTargetMetadata::command(true),
            vec![ScenarioReport::new(
                "scenario-a".to_owned(),
                vec!["category-recall".to_owned()],
                vec![crate::report::CheckReport::pass(
                    "check", "expected", "expected",
                )],
            )],
        );
        BaselineReport::from_runs(
            "model",
            TargetKind::MnemeV1Command.as_str(),
            crate::target::EvalTargetMetadata::command(true),
            BaselineMetadata::new(
                live_provider,
                Some("openai".to_owned()),
                Some("dry-run".to_owned()),
                Some("test-run".to_owned()),
            ),
            vec![BaselineScenarioMetadata::new(
                "scenario-a",
                vec!["category-recall".to_owned()],
            )],
            vec![BaselineRunReport::from_eval_report(1, passed)],
        )
    }

    fn failing_baseline_report(live_provider: bool) -> BaselineReport {
        let failed = EvalReport::from_results(
            TargetKind::MnemeV1Command.as_str(),
            crate::target::EvalTargetMetadata::command(true),
            vec![ScenarioReport::new(
                "scenario-a".to_owned(),
                vec!["category-recall".to_owned()],
                vec![crate::report::CheckReport::fail(
                    "check", "expected", "actual", "artifact",
                )],
            )],
        );
        BaselineReport::from_runs(
            "model",
            TargetKind::MnemeV1Command.as_str(),
            crate::target::EvalTargetMetadata::command(true),
            BaselineMetadata::new(
                live_provider,
                Some("openai".to_owned()),
                Some("dry-run".to_owned()),
                Some("test-run".to_owned()),
            ),
            vec![BaselineScenarioMetadata::new(
                "scenario-a",
                vec!["category-recall".to_owned()],
            )],
            vec![BaselineRunReport::from_eval_report(1, failed)],
        )
    }
}
