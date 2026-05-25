use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::EvalError;
use crate::redaction;
use crate::report::{BaselineReport, CheckStatus, EvalReport};
use crate::scenario::{load_scenario, validate_scenario, Scenario};

const REPORT_SCHEMA_VERSION: u32 = 1;
const CANDIDATE_SCHEMA_VERSION: &str = "mneme.eval_candidate.v1";

#[derive(Debug, Clone)]
pub(crate) struct CandidateGenerateConfig {
    pub(crate) source_path: PathBuf,
    pub(crate) out_dir: PathBuf,
    pub(crate) prefix: String,
    pub(crate) limit: Option<usize>,
    pub(crate) suite_override: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateReport {
    pub(crate) report_schema_version: u32,
    pub(crate) command: &'static str,
    pub(crate) source: String,
    pub(crate) source_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) suite: Option<String>,
    pub(crate) target: String,
    pub(crate) ok: bool,
    pub(crate) out_dir: String,
    pub(crate) failed_scenario_count: usize,
    pub(crate) candidate_count: usize,
    pub(crate) redaction_findings: Vec<String>,
    pub(crate) redaction_finding_codes: Vec<String>,
    pub(crate) candidates: Vec<CandidateArtifactReport>,
    pub(crate) recommended_next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateArtifactReport {
    pub(crate) id: String,
    pub(crate) source_scenario_id: String,
    pub(crate) path: String,
    pub(crate) status: String,
    pub(crate) failed_attempts: usize,
    pub(crate) failed_check_count: usize,
    pub(crate) redaction_finding_codes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateCheckReport {
    pub(crate) report_schema_version: u32,
    pub(crate) command: &'static str,
    pub(crate) source: String,
    pub(crate) ok: bool,
    pub(crate) checked_count: usize,
    pub(crate) valid: usize,
    pub(crate) invalid: usize,
    pub(crate) results: Vec<CandidateCheckResult>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CandidateCheckResult {
    pub(crate) path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) candidate_id: Option<String>,
    pub(crate) ok: bool,
    pub(crate) errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateFile {
    schema_version: String,
    id: String,
    status: String,
    source: CandidateSource,
    failure: CandidateFailure,
    redaction: CandidateRedaction,
    promotion_checklist: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario: Option<Scenario>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateSource {
    report_kind: String,
    report: String,
    target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suite: Option<String>,
    scenario_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scenario_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateFailure {
    failed_attempts: usize,
    failed_checks: Vec<CandidateFailedCheck>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateFailedCheck {
    check: String,
    count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateRedaction {
    sanitized: bool,
    finding_codes: Vec<String>,
}

#[derive(Debug, Clone)]
struct SourceReport {
    kind: SourceReportKind,
    suite: Option<String>,
    target: String,
    failures: Vec<ScenarioFailure>,
}

#[derive(Debug, Clone, Copy)]
enum SourceReportKind {
    Baseline,
    Eval,
}

impl SourceReportKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Baseline => "baseline",
            Self::Eval => "eval",
        }
    }
}

#[derive(Debug, Clone)]
struct ScenarioFailure {
    scenario_id: String,
    failed_attempts: usize,
    failed_checks: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct ScenarioSource {
    path: PathBuf,
    scenario: Scenario,
}

pub(crate) fn generate_candidates(
    config: &CandidateGenerateConfig,
) -> Result<CandidateReport, EvalError> {
    let raw_json = fs::read_to_string(&config.source_path)
        .map_err(|source| EvalError::io("read", &config.source_path, source))?;
    let redaction_findings = redaction::findings(&raw_json);
    let redaction_codes = redaction::finding_codes(&redaction_findings);
    let report = parse_source_report(
        &raw_json,
        config.suite_override.clone(),
        &config.source_path,
    )?;
    let mut failures = report.failures;
    sort_failures(&mut failures);
    let failed_scenario_count = failures.len();
    if let Some(limit) = config.limit {
        failures.truncate(limit);
    }

    fs::create_dir_all(&config.out_dir)
        .map_err(|source| EvalError::io("create dir", &config.out_dir, source))?;

    let scenario_sources = match report.suite.as_deref() {
        Some(suite) => scenario_sources_for_suite(suite)?,
        None => BTreeMap::new(),
    };

    let mut artifacts = Vec::new();
    for failure in failures {
        let artifact = write_candidate_file(
            config,
            report.kind,
            report.suite.as_deref(),
            &report.target,
            &redaction_codes,
            &scenario_sources,
            &failure,
        )?;
        artifacts.push(artifact);
    }

    let ok = artifacts
        .iter()
        .all(|artifact| artifact.status == "generated");
    let candidate_count = artifacts.len();
    Ok(CandidateReport {
        report_schema_version: REPORT_SCHEMA_VERSION,
        command: "candidate",
        source: safe_display_path(&config.source_path),
        source_kind: report.kind.as_str().to_owned(),
        suite: report.suite,
        target: report.target,
        ok,
        out_dir: safe_display_path(&config.out_dir),
        failed_scenario_count,
        candidate_count,
        redaction_findings,
        redaction_finding_codes: redaction_codes,
        candidates: artifacts,
        recommended_next_actions: candidate_next_actions(candidate_count),
    })
}

pub(crate) fn check_candidates(path: &Path) -> Result<CandidateCheckReport, EvalError> {
    let paths = candidate_paths(path)?;
    let mut results = Vec::new();
    for candidate_path in paths {
        results.push(check_candidate_file(&candidate_path));
    }
    let checked_count = results.len();
    let valid = results.iter().filter(|result| result.ok).count();
    let invalid = checked_count.saturating_sub(valid);
    Ok(CandidateCheckReport {
        report_schema_version: REPORT_SCHEMA_VERSION,
        command: "candidate-check",
        source: safe_display_path(path),
        ok: invalid == 0,
        checked_count,
        valid,
        invalid,
        results,
    })
}

fn parse_source_report(
    raw_json: &str,
    suite_override: Option<String>,
    path: &Path,
) -> Result<SourceReport, EvalError> {
    let value: serde_json::Value =
        serde_json::from_str(raw_json).map_err(|source| EvalError::parse_json(path, source))?;
    if value.get("runs").is_some() {
        let report: BaselineReport =
            serde_json::from_value(value).map_err(|source| EvalError::parse_json(path, source))?;
        let failures = failures_from_baseline(&report);
        return Ok(SourceReport {
            kind: SourceReportKind::Baseline,
            suite: Some(report.suite),
            target: report.target,
            failures,
        });
    }
    if value.get("results").is_some() {
        let report: EvalReport =
            serde_json::from_value(value).map_err(|source| EvalError::parse_json(path, source))?;
        let failures = failures_from_eval(&report);
        return Ok(SourceReport {
            kind: SourceReportKind::Eval,
            suite: suite_override,
            target: report.target,
            failures,
        });
    }
    Err(EvalError::scenario(format!(
        "report {} is neither a baseline nor eval report",
        path.display()
    )))
}

fn failures_from_baseline(report: &BaselineReport) -> Vec<ScenarioFailure> {
    let mut failures: BTreeMap<String, ScenarioFailure> = BTreeMap::new();
    for run in &report.runs {
        for result in &run.results {
            if result.ok {
                continue;
            }
            let entry = failures
                .entry(result.scenario_id.clone())
                .or_insert_with(|| ScenarioFailure {
                    scenario_id: result.scenario_id.clone(),
                    failed_attempts: 0,
                    failed_checks: BTreeMap::new(),
                });
            entry.failed_attempts += 1;
            if result.failed_checks.is_empty() {
                *entry
                    .failed_checks
                    .entry("scenario.failed".to_owned())
                    .or_default() += 1;
            } else {
                for check in &result.failed_checks {
                    *entry.failed_checks.entry(check.clone()).or_default() += 1;
                }
            }
        }
    }
    failures.into_values().collect()
}

fn failures_from_eval(report: &EvalReport) -> Vec<ScenarioFailure> {
    report
        .results
        .iter()
        .filter(|scenario| !scenario.ok)
        .map(|scenario| {
            let mut failed_checks = BTreeMap::new();
            for check in scenario
                .checks
                .iter()
                .filter(|check| check.status == CheckStatus::Fail)
            {
                *failed_checks.entry(check.name.clone()).or_default() += 1;
            }
            if failed_checks.is_empty() {
                failed_checks.insert("scenario.failed".to_owned(), 1);
            }
            ScenarioFailure {
                scenario_id: scenario.scenario_id.clone(),
                failed_attempts: 1,
                failed_checks,
            }
        })
        .collect()
}

fn sort_failures(failures: &mut [ScenarioFailure]) {
    failures.sort_by(|left, right| {
        right
            .failed_attempts
            .cmp(&left.failed_attempts)
            .then_with(|| failed_check_total(right).cmp(&failed_check_total(left)))
            .then_with(|| left.scenario_id.cmp(&right.scenario_id))
    });
}

fn failed_check_total(failure: &ScenarioFailure) -> usize {
    failure.failed_checks.values().sum()
}

fn scenario_sources_for_suite(suite: &str) -> Result<BTreeMap<String, ScenarioSource>, EvalError> {
    let root = env::current_dir()
        .map_err(|source| EvalError::io("read current dir", Path::new("."), source))?;
    let suite_dir = root.join("evals").join("scenarios").join(suite);
    if !suite_dir.is_dir() {
        return Ok(BTreeMap::new());
    }
    let mut paths = Vec::new();
    collect_yaml_paths(&suite_dir, &mut paths)?;
    let mut sources = BTreeMap::new();
    for path in paths {
        let scenario = load_scenario(&path)?;
        sources.insert(scenario.id.clone(), ScenarioSource { path, scenario });
    }
    Ok(sources)
}

fn write_candidate_file(
    config: &CandidateGenerateConfig,
    report_kind: SourceReportKind,
    suite: Option<&str>,
    target: &str,
    redaction_codes: &[String],
    scenario_sources: &BTreeMap<String, ScenarioSource>,
    failure: &ScenarioFailure,
) -> Result<CandidateArtifactReport, EvalError> {
    let candidate_id = format!(
        "{}-{}",
        sanitize_identifier(&config.prefix),
        sanitize_identifier(&failure.scenario_id)
    );
    let path = config
        .out_dir
        .join(format!("{candidate_id}.candidate.yaml"));
    let scenario_source = scenario_sources.get(&failure.scenario_id);
    let scenario =
        scenario_source.map(|source| candidate_scenario(source, &candidate_id, suite, target));
    let candidate = CandidateFile {
        schema_version: CANDIDATE_SCHEMA_VERSION.to_owned(),
        id: candidate_id.clone(),
        status: "proposed".to_owned(),
        source: CandidateSource {
            report_kind: report_kind.as_str().to_owned(),
            report: safe_display_path(&config.source_path),
            target: target.to_owned(),
            suite: suite.map(str::to_owned),
            scenario_id: failure.scenario_id.clone(),
            scenario_path: scenario_source.map(|source| safe_display_path(&source.path)),
        },
        failure: CandidateFailure {
            failed_attempts: failure.failed_attempts,
            failed_checks: failure
                .failed_checks
                .iter()
                .map(|(check, count)| CandidateFailedCheck {
                    check: redaction::sanitize_text(check),
                    count: *count,
                })
                .collect(),
        },
        redaction: CandidateRedaction {
            sanitized: !redaction_codes.is_empty(),
            finding_codes: redaction_codes.to_vec(),
        },
        promotion_checklist: promotion_checklist(suite),
        scenario,
    };

    let yaml = serde_yaml::to_string(&candidate).map_err(|source| {
        EvalError::scenario(format!("serialize candidate {}: {source}", path.display()))
    })?;
    let sanitized_yaml = redaction::sanitize_text(&yaml);
    let candidate_findings = redaction::findings(&sanitized_yaml);
    fs::write(&path, format!("{sanitized_yaml}\n"))
        .map_err(|source| EvalError::io("write", &path, source))?;

    let status = if candidate_findings.is_empty() {
        "generated"
    } else {
        "blocked_redaction_required"
    };
    Ok(CandidateArtifactReport {
        id: candidate_id,
        source_scenario_id: failure.scenario_id.clone(),
        path: safe_display_path(&path),
        status: status.to_owned(),
        failed_attempts: failure.failed_attempts,
        failed_check_count: failed_check_total(failure),
        redaction_finding_codes: redaction::finding_codes(&candidate_findings),
    })
}

fn candidate_scenario(
    source: &ScenarioSource,
    candidate_id: &str,
    suite: Option<&str>,
    target: &str,
) -> Scenario {
    let mut scenario = source.scenario.clone();
    scenario.id = candidate_id.to_owned();
    push_unique_tag(&mut scenario.tags, "candidate");
    push_unique_tag(&mut scenario.tags, "dogfood");
    push_unique_tag(&mut scenario.tags, "needs-review");
    if let Some(suite) = suite {
        push_unique_tag(&mut scenario.tags, &format!("source-suite-{suite}"));
    }
    push_unique_tag(&mut scenario.tags, &format!("source-target-{target}"));
    scenario
}

fn push_unique_tag(tags: &mut Vec<String>, tag: &str) {
    if !tags.iter().any(|existing| existing == tag) {
        tags.push(tag.to_owned());
    }
}

fn promotion_checklist(suite: Option<&str>) -> Vec<String> {
    let target_suite = suite.unwrap_or("<suite>");
    vec![
        "Confirm the candidate contains no private user data, project paths, or provider secrets."
            .to_owned(),
        "Minimize the scenario to the smallest behavior that reproduces the failure.".to_owned(),
        format!("Move the reviewed scenario block to evals/scenarios/{target_suite}/ with a stable public filename."),
        "Run `mneme-eval validate` on the promoted scenario before adding it to a suite.".to_owned(),
        "Run the relevant suite and baseline gate before release.".to_owned(),
    ]
}

fn candidate_next_actions(candidate_count: usize) -> Vec<String> {
    if candidate_count == 0 {
        return vec![
            "no failing scenarios were found; no candidate artifacts were generated".to_owned(),
        ];
    }
    vec![
        "review candidate YAML locally before committing any promoted scenario".to_owned(),
        "copy only the reviewed `scenario` block into evals/scenarios/<suite>/".to_owned(),
        "run `mneme-eval candidate-check <dir>` before sharing candidates".to_owned(),
    ]
}

fn check_candidate_file(path: &Path) -> CandidateCheckResult {
    let mut errors = Vec::new();
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) => {
            return CandidateCheckResult {
                path: safe_display_path(path),
                candidate_id: None,
                ok: false,
                errors: vec![format!("read candidate: {error}")],
            };
        }
    };
    let findings = redaction::findings(&raw);
    if !findings.is_empty() {
        errors.push(format!(
            "redaction findings remain: {}",
            redaction::finding_codes(&findings).join(",")
        ));
    }
    let candidate: Option<CandidateFile> = match serde_yaml::from_str(&raw) {
        Ok(candidate) => Some(candidate),
        Err(error) => {
            errors.push(format!("parse candidate YAML: {error}"));
            None
        }
    };
    if let Some(candidate) = &candidate {
        validate_candidate(candidate, path, &mut errors);
    }
    CandidateCheckResult {
        path: safe_display_path(path),
        candidate_id: candidate.map(|candidate| candidate.id),
        ok: errors.is_empty(),
        errors,
    }
}

fn validate_candidate(candidate: &CandidateFile, path: &Path, errors: &mut Vec<String>) {
    if candidate.schema_version != CANDIDATE_SCHEMA_VERSION {
        errors.push(format!("schema_version must be {CANDIDATE_SCHEMA_VERSION}"));
    }
    if candidate.id.trim().is_empty() {
        errors.push("id must not be empty".to_owned());
    }
    if candidate.status != "proposed" {
        errors.push("status must be proposed".to_owned());
    }
    if candidate.source.scenario_id.trim().is_empty() {
        errors.push("source.scenario_id must not be empty".to_owned());
    }
    if candidate.failure.failed_attempts == 0 {
        errors.push("failure.failed_attempts must be greater than zero".to_owned());
    }
    if candidate.failure.failed_checks.is_empty() {
        errors.push("failure.failed_checks must not be empty".to_owned());
    }
    for check in &candidate.failure.failed_checks {
        if check.check.trim().is_empty() {
            errors.push("failure.failed_checks entries must name a check".to_owned());
        }
        if check.count == 0 {
            errors.push(format!(
                "failure.failed_checks `{}` count must be greater than zero",
                check.check
            ));
        }
    }
    if candidate.promotion_checklist.is_empty() {
        errors.push("promotion_checklist must not be empty".to_owned());
    }
    if let Some(scenario) = &candidate.scenario {
        if scenario.id != candidate.id {
            errors.push("scenario.id must match candidate id".to_owned());
        }
        if let Err(error) = validate_scenario(scenario, path) {
            errors.push(error.to_string());
        }
    }
}

fn candidate_paths(path: &Path) -> Result<Vec<PathBuf>, EvalError> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    if path.is_dir() {
        let mut paths = Vec::new();
        collect_yaml_paths(path, &mut paths)?;
        paths.sort();
        return Ok(paths);
    }
    Err(EvalError::scenario(format!(
        "candidate path {} is not a file or directory",
        path.display()
    )))
}

fn collect_yaml_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), EvalError> {
    let entries = fs::read_dir(dir).map_err(|source| EvalError::io("read dir", dir, source))?;
    for entry in entries {
        let entry = entry.map_err(|source| EvalError::io("read dir entry", dir, source))?;
        let path = entry.path();
        if path.is_dir() {
            collect_yaml_paths(&path, paths)?;
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "yaml" | "yml"))
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn sanitize_identifier(value: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_dash = false;
    for ch in value.chars() {
        let next = if ch.is_ascii_alphanumeric() {
            previous_dash = false;
            Some(ch.to_ascii_lowercase())
        } else if !previous_dash {
            previous_dash = true;
            Some('-')
        } else {
            None
        };
        if let Some(ch) = next {
            sanitized.push(ch);
        }
    }
    let trimmed = sanitized.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        "candidate".to_owned()
    } else {
        trimmed
    }
}

fn safe_display_path(path: &Path) -> String {
    let display = match env::current_dir()
        .ok()
        .and_then(|root| path.strip_prefix(root).ok().map(Path::to_path_buf))
    {
        Some(relative) => relative.display().to_string(),
        None => path.display().to_string(),
    };
    redaction::sanitize_text(&display)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{
        BaselineMetadata, BaselineRunReport, BaselineScenarioMetadata, CheckReport, ScenarioReport,
    };
    use crate::target::{EvalTargetMetadata, TargetKind};

    #[test]
    fn generates_sanitized_candidates_from_baseline_report(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let root = env::temp_dir().join(format!("mneme-candidate-test-{}", std::process::id()));
        let out_dir = root.join("candidates");
        fs::create_dir_all(&root)?;
        let report_path = root.join("baseline.json");
        let report = failing_baseline_report();
        fs::write(&report_path, serde_json::to_string(&report)?)?;

        let config = CandidateGenerateConfig {
            source_path: report_path,
            out_dir: out_dir.clone(),
            prefix: "dogfood".to_owned(),
            limit: Some(1),
            suite_override: None,
        };
        let candidate_report = generate_candidates(&config)?;
        assert!(candidate_report.ok);
        assert_eq!(candidate_report.candidate_count, 1);
        assert_eq!(
            candidate_report.redaction_finding_codes,
            vec!["api_key_assignment"]
        );
        let candidate_path = out_dir.join("dogfood-scenario-a.candidate.yaml");
        let candidate_yaml = fs::read_to_string(&candidate_path)?;
        assert!(!candidate_yaml.contains("API_KEY=FAKE_TEST_VALUE"));
        assert!(redaction::findings(&candidate_yaml).is_empty());

        let check = check_candidates(&out_dir)?;
        assert!(check.ok);
        assert_eq!(check.valid, 1);
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    fn failing_baseline_report() -> BaselineReport {
        let failed = EvalReport::from_results(
            TargetKind::MnemeV1Command.as_str(),
            EvalTargetMetadata::command(true),
            vec![ScenarioReport::new(
                "scenario-a".to_owned(),
                vec!["category-recall".to_owned()],
                vec![CheckReport::fail(
                    "claim.user.token.API_KEY=FAKE_TEST_VALUE",
                    "expected",
                    "actual",
                    "artifact",
                )],
            )],
        );
        BaselineReport::from_runs(
            "model",
            TargetKind::MnemeV1Command.as_str(),
            EvalTargetMetadata::command(true),
            BaselineMetadata::new(
                false,
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
