use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use mneme_core::{BuildStage, PRODUCT_NAME};

use crate::error::EvalError;
use crate::report::{EvalReport, ScenarioReport};
use crate::runtime::{replay_scenario, FaultMode, ReplayOptions};
use crate::scenario::load_scenario;

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
        "replay" => run_replay(args.collect()),
        "run" => run_suite(args.collect()),
        _ => Err(EvalError::invalid_cli(format!(
            "unknown mneme-eval command: {command}\navailable commands: doctor, version, replay, run"
        ))),
    }
}

fn print_doctor() {
    println!(
        "{PRODUCT_NAME} eval harness: {}",
        BuildStage::Bootstrap.as_str()
    );
}

#[derive(Debug, Clone)]
struct CommandOptions {
    json: bool,
    report_path: Option<PathBuf>,
    fault_mode: FaultMode,
}

impl Default for CommandOptions {
    fn default() -> Self {
        Self {
            json: false,
            report_path: None,
            fault_mode: FaultMode::None,
        }
    }
}

fn run_replay(raw_args: Vec<String>) -> Result<(), EvalError> {
    let (path, options) = parse_replay_args(raw_args)?;
    let scenario = load_scenario(&path)?;
    let scenario_report = replay_scenario(
        &scenario,
        ReplayOptions {
            fault_mode: options.fault_mode,
        },
    );
    let report = EvalReport::from_results(vec![scenario_report]);
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
    let mut results = Vec::new();
    for path in paths {
        let scenario = load_scenario(&path)?;
        results.push(replay_scenario(
            &scenario,
            ReplayOptions {
                fault_mode: options.fault_mode,
            },
        ));
    }
    let report = EvalReport::from_results(results);
    emit_report(&report, &options)?;
    if report.ok {
        Ok(())
    } else {
        Err(EvalError::scenario("eval failed"))
    }
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
            "usage: mneme-eval replay <scenario.yaml> [--json] [--report <path>]",
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

fn write_report(path: &Path, report: &EvalReport) -> Result<(), EvalError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| EvalError::io("create dir", parent, source))?;
    }
    let json =
        serde_json::to_string_pretty(report).map_err(|source| EvalError::json(path, source))?;
    fs::write(path, format!("{json}\n")).map_err(|source| EvalError::io("write", path, source))
}

fn print_human_report(report: &EvalReport) {
    println!(
        "eval: {} scenario(s), {} passed, {} failed",
        report.scenario_count, report.passed, report.failed
    );
    for scenario in &report.results {
        print_scenario_report(scenario);
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
    fn scenario_file_filter_accepts_yaml_only() {
        assert!(is_scenario_file(Path::new("a.yaml")));
        assert!(is_scenario_file(Path::new("a.yml")));
        assert!(!is_scenario_file(Path::new(".gitkeep")));
    }
}
