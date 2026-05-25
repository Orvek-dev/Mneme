use std::fs;
use std::path::Path;

use serde::Serialize;

use crate::report::{AcceptanceGateReport, CheckStatus};

pub(crate) const DOGFOOD_SUMMARY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DogfoodSummaryReport {
    pub(crate) report_schema_version: u32,
    pub(crate) command: &'static str,
    pub(crate) source: String,
    pub(crate) ok: bool,
    pub(crate) decision_status: String,
    pub(crate) artifact_count: usize,
    pub(crate) passed_artifacts: usize,
    pub(crate) failed_artifacts: usize,
    pub(crate) gate_count: usize,
    pub(crate) passed_gates: usize,
    pub(crate) failed_gates: usize,
    pub(crate) artifacts: Vec<DogfoodArtifactReport>,
    pub(crate) gates: Vec<AcceptanceGateReport>,
    pub(crate) recommended_next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DogfoodArtifactReport {
    pub(crate) name: String,
    pub(crate) path: String,
    pub(crate) present: bool,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

pub(crate) fn build_dogfood_summary_report(bundle_dir: &Path) -> DogfoodSummaryReport {
    let artifacts = vec![
        json_artifact(bundle_dir, "summary", "summary.json", check_summary),
        json_artifact(
            bundle_dir,
            "v1-readiness",
            "v1-readiness.json",
            check_v1_readiness,
        ),
        json_artifact(
            bundle_dir,
            "dogfood.validate",
            "dogfood.validate.json",
            check_dogfood_validate,
        ),
        json_artifact(
            bundle_dir,
            "dogfood.run.fake",
            "dogfood.run.fake.json",
            |json| check_eval_run(json, "fake"),
        ),
        json_artifact(
            bundle_dir,
            "dogfood.run.mneme-v1",
            "dogfood.run.mneme-v1.json",
            |json| check_eval_run(json, "mneme-v1"),
        ),
        json_artifact(
            bundle_dir,
            "dogfood.acceptance.mneme-v1",
            "dogfood.acceptance.mneme-v1.json",
            check_dogfood_acceptance,
        ),
        json_artifact(
            bundle_dir,
            "cli.doctor.post",
            "cli.doctor.post.json",
            check_cli_doctor_post,
        ),
        json_artifact(
            bundle_dir,
            "cli.context",
            "cli.context.json",
            check_cli_context,
        ),
        json_artifact(
            bundle_dir,
            "cli.quality",
            "cli.quality.json",
            check_cli_quality,
        ),
        text_artifact(
            bundle_dir,
            "cli.validate",
            "cli.validate.txt",
            check_cli_validate,
        ),
    ];
    let mut gates = artifacts
        .iter()
        .map(|artifact| {
            if artifact.ok {
                AcceptanceGateReport::pass(
                    format!("artifact.{}", artifact.name),
                    artifact.detail.clone(),
                )
            } else {
                AcceptanceGateReport::fail(
                    format!("artifact.{}", artifact.name),
                    artifact.detail.clone(),
                )
            }
        })
        .collect::<Vec<_>>();
    let artifact_count = artifacts.len();
    let passed_artifacts = artifacts.iter().filter(|artifact| artifact.ok).count();
    let failed_artifacts = artifact_count.saturating_sub(passed_artifacts);
    if failed_artifacts == 0 {
        gates.push(AcceptanceGateReport::pass(
            "decision.ready",
            "dogfood evidence bundle is ready for manual v1 dogfood review",
        ));
    } else {
        gates.push(AcceptanceGateReport::fail(
            "decision.ready",
            format!("{failed_artifacts} required artifact(s) failed triage"),
        ));
    }
    let gate_count = gates.len();
    let passed_gates = gates
        .iter()
        .filter(|gate| gate.status == CheckStatus::Pass)
        .count();
    let failed_gates = gate_count.saturating_sub(passed_gates);
    let ok = failed_gates == 0;
    DogfoodSummaryReport {
        report_schema_version: DOGFOOD_SUMMARY_SCHEMA_VERSION,
        command: "dogfood-summary",
        source: bundle_dir.display().to_string(),
        ok,
        decision_status: if ok {
            "ready_for_manual_dogfood"
        } else {
            "blocked"
        }
        .to_owned(),
        artifact_count,
        passed_artifacts,
        failed_artifacts,
        gate_count,
        passed_gates,
        failed_gates,
        artifacts,
        gates,
        recommended_next_actions: dogfood_next_actions(ok),
    }
}

fn json_artifact(
    bundle_dir: &Path,
    name: &str,
    relative_path: &str,
    check: impl FnOnce(&serde_json::Value) -> Result<String, String>,
) -> DogfoodArtifactReport {
    let path = bundle_dir.join(relative_path);
    match read_json(&path).and_then(|json| check(&json)) {
        Ok(detail) => DogfoodArtifactReport {
            name: name.to_owned(),
            path: path.display().to_string(),
            present: true,
            ok: true,
            detail,
        },
        Err(detail) => DogfoodArtifactReport {
            name: name.to_owned(),
            path: path.display().to_string(),
            present: path.exists(),
            ok: false,
            detail,
        },
    }
}

fn text_artifact(
    bundle_dir: &Path,
    name: &str,
    relative_path: &str,
    check: impl FnOnce(&str) -> Result<String, String>,
) -> DogfoodArtifactReport {
    let path = bundle_dir.join(relative_path);
    match read_text(&path).and_then(|text| check(&text)) {
        Ok(detail) => DogfoodArtifactReport {
            name: name.to_owned(),
            path: path.display().to_string(),
            present: true,
            ok: true,
            detail,
        },
        Err(detail) => DogfoodArtifactReport {
            name: name.to_owned(),
            path: path.display().to_string(),
            present: path.exists(),
            ok: false,
            detail,
        },
    }
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let text = read_text(path)?;
    serde_json::from_str(&text).map_err(|source| format!("invalid JSON: {source}"))
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|source| format!("cannot read {}: {source}", path.display()))
}

fn check_summary(json: &serde_json::Value) -> Result<String, String> {
    require_str(json, "/command", "v1-dogfood")?;
    require_str(json, "/status", "passed")?;
    Ok("v1-dogfood summary status is passed".to_owned())
}

fn check_v1_readiness(json: &serde_json::Value) -> Result<String, String> {
    require_str(json, "/command", "v1-readiness")?;
    require_bool(json, "/ok", true)?;
    require_str(json, "/readiness_status", "ready_for_v1_dogfood")?;
    require_u64_at_least(json, "/scenario_count", 22)?;
    Ok("v1 readiness is ready_for_v1_dogfood".to_owned())
}

fn check_dogfood_validate(json: &serde_json::Value) -> Result<String, String> {
    require_bool(json, "/ok", true)?;
    require_u64_at_least(json, "/valid", 4)?;
    require_u64(json, "/invalid", 0)?;
    Ok("dogfood suite validation passed".to_owned())
}

fn check_eval_run(json: &serde_json::Value, target: &str) -> Result<String, String> {
    require_str(json, "/target", target)?;
    require_bool(json, "/ok", true)?;
    require_u64_at_least(json, "/scenario_count", 4)?;
    require_u64(json, "/failed", 0)?;
    Ok(format!("dogfood run passed for target {target}"))
}

fn check_dogfood_acceptance(json: &serde_json::Value) -> Result<String, String> {
    require_str(json, "/target", "mneme-v1")?;
    require_bool(json, "/ok", true)?;
    require_u64_at_least(json, "/gate_count", 7)?;
    require_u64(json, "/failed", 0)?;
    Ok("dogfood acceptance passed for mneme-v1".to_owned())
}

fn check_cli_doctor_post(json: &serde_json::Value) -> Result<String, String> {
    require_str(json, "/command", "doctor")?;
    require_bool(json, "/ok", true)?;
    require_str(json, "/store/current/status", "valid")?;
    require_str(json, "/profile/status", "valid")?;
    Ok("post-init doctor reports valid store and profile".to_owned())
}

fn check_cli_context(json: &serde_json::Value) -> Result<String, String> {
    require_u64_at_least(json, "/item_count", 1)?;
    Ok("CLI context returned at least one memory item".to_owned())
}

fn check_cli_quality(json: &serde_json::Value) -> Result<String, String> {
    require_str(json, "/command", "quality")?;
    require_bool(json, "/ok", true)?;
    require_str(json, "/health", "ok")?;
    require_u64(json, "/review_item_count", 0)?;
    Ok("CLI quality report is healthy".to_owned())
}

fn check_cli_validate(text: &str) -> Result<String, String> {
    if text.contains("current=Valid") {
        Ok("CLI validate reports a valid current store".to_owned())
    } else {
        Err("CLI validate did not report current=Valid".to_owned())
    }
}

fn require_str(json: &serde_json::Value, pointer: &str, expected: &str) -> Result<(), String> {
    match json.pointer(pointer).and_then(|value| value.as_str()) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!("{pointer} expected {expected}, got {actual}")),
        None => Err(format!("{pointer} missing or not a string")),
    }
}

fn require_bool(json: &serde_json::Value, pointer: &str, expected: bool) -> Result<(), String> {
    match json.pointer(pointer).and_then(|value| value.as_bool()) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!("{pointer} expected {expected}, got {actual}")),
        None => Err(format!("{pointer} missing or not a bool")),
    }
}

fn require_u64(json: &serde_json::Value, pointer: &str, expected: u64) -> Result<(), String> {
    match json.pointer(pointer).and_then(|value| value.as_u64()) {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!("{pointer} expected {expected}, got {actual}")),
        None => Err(format!("{pointer} missing or not a number")),
    }
}

fn require_u64_at_least(
    json: &serde_json::Value,
    pointer: &str,
    minimum: u64,
) -> Result<(), String> {
    match json.pointer(pointer).and_then(|value| value.as_u64()) {
        Some(actual) if actual >= minimum => Ok(()),
        Some(actual) => Err(format!(
            "{pointer} expected at least {minimum}, got {actual}"
        )),
        None => Err(format!("{pointer} missing or not a number")),
    }
}

fn dogfood_next_actions(ok: bool) -> Vec<String> {
    if ok {
        vec![
            "Treat this bundle as deterministic evidence for a manual v1 dogfood pass.".to_owned(),
            "Run provider/model baselines separately before changing extraction behavior."
                .to_owned(),
        ]
    } else {
        vec![
            "Inspect failed artifact entries and rerun `scripts/v1-dogfood.sh` after fixes."
                .to_owned(),
            "Do not promote the build to manual dogfood until decision_status is ready_for_manual_dogfood."
                .to_owned(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn dogfood_summary_blocks_missing_bundle() {
        let path = PathBuf::from("missing-dogfood-bundle");
        let report = build_dogfood_summary_report(&path);
        assert!(!report.ok);
        assert_eq!(report.decision_status, "blocked");
        assert_eq!(report.failed_artifacts, report.artifact_count);
    }

    #[test]
    fn dogfood_summary_accepts_valid_bundle() -> Result<(), Box<dyn std::error::Error>> {
        let dir =
            std::env::temp_dir().join(format!("mneme-dogfood-summary-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        write_json(
            &dir,
            "summary.json",
            r#"{"command":"v1-dogfood","status":"passed"}"#,
        )?;
        write_json(
            &dir,
            "v1-readiness.json",
            r#"{"command":"v1-readiness","ok":true,"readiness_status":"ready_for_v1_dogfood","scenario_count":22}"#,
        )?;
        write_json(
            &dir,
            "dogfood.validate.json",
            r#"{"ok":true,"valid":4,"invalid":0}"#,
        )?;
        write_json(
            &dir,
            "dogfood.run.fake.json",
            r#"{"target":"fake","ok":true,"scenario_count":4,"failed":0}"#,
        )?;
        write_json(
            &dir,
            "dogfood.run.mneme-v1.json",
            r#"{"target":"mneme-v1","ok":true,"scenario_count":4,"failed":0}"#,
        )?;
        write_json(
            &dir,
            "dogfood.acceptance.mneme-v1.json",
            r#"{"target":"mneme-v1","ok":true,"gate_count":7,"failed":0}"#,
        )?;
        write_json(
            &dir,
            "cli.doctor.post.json",
            r#"{"command":"doctor","ok":true,"store":{"current":{"status":"valid"}},"profile":{"status":"valid"}}"#,
        )?;
        write_json(&dir, "cli.context.json", r#"{"item_count":1}"#)?;
        write_json(
            &dir,
            "cli.quality.json",
            r#"{"command":"quality","ok":true,"health":"ok","review_item_count":0}"#,
        )?;
        fs::write(
            dir.join("cli.validate.txt"),
            "mneme: validate x (current=Valid)\n",
        )?;

        let report = build_dogfood_summary_report(&dir);
        let _ = fs::remove_dir_all(&dir);

        assert!(report.ok);
        assert_eq!(report.decision_status, "ready_for_manual_dogfood");
        assert_eq!(report.failed_artifacts, 0);
        Ok(())
    }

    fn write_json(dir: &Path, name: &str, text: &str) -> Result<(), std::io::Error> {
        fs::write(dir.join(name), format!("{text}\n"))
    }
}
