use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use mneme_core::{BuildStage, PRODUCT_NAME};
use serde::Serialize;

use crate::error::EvalError;
use crate::report::{
    AcceptanceGateReport, AcceptanceReport, BaselineReport, BaselineRunReport,
    BaselineScenarioMetadata, EvalReport, ScenarioReport, ScenarioValidationReport,
    ValidationReport,
};
use crate::runtime::replay_scenario;
use crate::scenario::load_scenario;
use crate::target::{
    build_target, CommandExtractorOptions, FaultMode, TargetKind, TargetRunOptions,
};

const DEFAULT_BASELINE_ITERATIONS: usize = 3;
const MAX_BASELINE_ITERATIONS: usize = 100;

/// Runs the Mneme eval harness command-line interface.
pub fn run_cli(args: impl IntoIterator<Item = String>) -> Result<(), EvalError> {
    let mut args = args.into_iter();
    let _program = args.next();
    let Some(command) = args.next() else {
        print_doctor();
        return Ok(());
    };
    match command.as_str() {
        "doctor" => {
            print_doctor();
            Ok(())
        }
        "--version" | "version" => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        "validate" => run_validate(args.collect()),
        "acceptance" => run_acceptance(args.collect()),
        "baseline" => run_baseline(args.collect()),
        "replay" => run_replay(args.collect()),
        "run" => run_suite(args.collect()),
        _ => Err(EvalError::invalid_cli(format!(
            "unknown mneme-eval command: {command}\navailable commands: doctor, version, validate, acceptance, baseline, replay, run"
        ))),
    }
}

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
            "--iterations" => {
                idx += 1;
                let Some(value) = raw_args.get(idx) else {
                    return Err(EvalError::invalid_cli("--iterations requires a value"));
                };
                options.iterations = parse_iterations(value)?;
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
            "--json".to_owned(),
        ])?;
        assert_eq!(suite, "model");
        assert_eq!(options.target_kind, TargetKind::MnemeV1Command);
        assert_eq!(options.iterations, 2);
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
}
