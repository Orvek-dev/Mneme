#!/usr/bin/env python3
"""Prepare a local Mneme v1 real-use pilot and triage sanitized feedback."""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
RUN_LABEL_RE = re.compile(r"^[A-Za-z0-9._/-]+$")
FEEDBACK_CATEGORIES = [
    "recall_miss",
    "wrong_memory",
    "irrelevant_context",
    "privacy_redaction",
    "scope_leak",
    "workflow_friction",
    "docs_gap",
    "performance",
    "cli_ux",
    "other",
]
SEVERITIES = ["blocker", "high", "medium", "low"]
NEXT_ACTIONS = ["candidate", "fix", "docs", "ignore", "defer"]
REQUIRED_FINDING_FIELDS = [
    "id",
    "title",
    "category",
    "severity",
    "summary",
    "expected",
    "actual",
    "next_action",
]
PRIVATE_PATTERNS = [
    (re.compile(r"/Users/[^\s\"']+"), "[redacted:local_path]"),
    (re.compile(r"sk-[A-Za-z0-9_-]{8,}"), "[redacted:key_like]"),
    (
        re.compile(
            r"(?i)\b(OPENAI_API_KEY|API_KEY|TOKEN|ACCESS_TOKEN|PASSWORD)\s*=\s*[^\s\"']+"
        ),
        lambda match: f"{match.group(1)}=[redacted:secret]",
    ),
]


class PilotFailure(RuntimeError):
    """Raised when pilot setup or feedback triage fails."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-real-use-pilot-contract",
        "feedback_categories": FEEDBACK_CATEGORIES,
        "severities": SEVERITIES,
        "next_actions": NEXT_ACTIONS,
        "required_finding_fields": REQUIRED_FINDING_FIELDS,
        "default_output": "evals/runs/v1-real-use-pilot/<run-label>",
        "privacy_policy": "feedback is sanitized before any derived artifact is written",
    }


def feedback_template() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "pilot_label": "local-v1-real-use",
        "findings": [
            {
                "id": "pilot-001",
                "title": "Context recall missed release preference",
                "category": "recall_miss",
                "severity": "medium",
                "summary": "Describe the behavior without private project text.",
                "expected": "The relevant remembered preference should appear in context.",
                "actual": "The context omitted the relevant preference.",
                "next_action": "candidate",
                "notes": "Keep concrete private content out of this file.",
            }
        ],
    }


def sanitize_text(value: str) -> tuple[str, list[str]]:
    sanitized = value
    findings: list[str] = []
    for pattern, replacement in PRIVATE_PATTERNS:
        if pattern.search(sanitized):
            findings.append(pattern.pattern)
            sanitized = pattern.sub(replacement, sanitized)
    return sanitized, findings


def sanitize_value(value: Any) -> tuple[Any, list[str]]:
    if isinstance(value, str):
        return sanitize_text(value)
    if isinstance(value, list):
        sanitized_items = []
        findings: list[str] = []
        for item in value:
            sanitized_item, item_findings = sanitize_value(item)
            sanitized_items.append(sanitized_item)
            findings.extend(item_findings)
        return sanitized_items, findings
    if isinstance(value, dict):
        sanitized_dict = {}
        findings: list[str] = []
        for key, item in value.items():
            sanitized_item, item_findings = sanitize_value(item)
            sanitized_dict[key] = sanitized_item
            findings.extend(item_findings)
        return sanitized_dict, findings
    return value, []


def triage_feedback(feedback: dict[str, Any]) -> dict[str, Any]:
    errors: list[str] = []
    if feedback.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION}")
    findings = feedback.get("findings")
    if not isinstance(findings, list):
        errors.append("findings must be a list")
        findings = []

    sanitized_feedback, redaction_findings = sanitize_value(feedback)
    sanitized_findings = sanitized_feedback.get("findings", [])
    triaged_findings: list[dict[str, Any]] = []

    for index, finding in enumerate(sanitized_findings, start=1):
        if not isinstance(finding, dict):
            errors.append(f"finding {index} must be an object")
            continue
        missing = [
            field
            for field in REQUIRED_FINDING_FIELDS
            if not str(finding.get(field, "")).strip()
        ]
        if missing:
            errors.append(f"finding {index} missing fields: {', '.join(missing)}")
        category = finding.get("category")
        severity = finding.get("severity")
        next_action = finding.get("next_action")
        if category not in FEEDBACK_CATEGORIES:
            errors.append(f"finding {index} category is invalid: {category}")
        if severity not in SEVERITIES:
            errors.append(f"finding {index} severity is invalid: {severity}")
        if next_action not in NEXT_ACTIONS:
            errors.append(f"finding {index} next_action is invalid: {next_action}")
        triaged_findings.append(
            {
                "id": finding.get("id"),
                "title": finding.get("title"),
                "category": category,
                "severity": severity,
                "next_action": next_action,
                "summary": finding.get("summary"),
                "expected": finding.get("expected"),
                "actual": finding.get("actual"),
                "needs_candidate": next_action == "candidate",
                "needs_code_change": next_action == "fix",
                "needs_docs_change": next_action == "docs",
            }
        )

    category_counts = Counter(
        item["category"] for item in triaged_findings if item.get("category")
    )
    severity_counts = Counter(
        item["severity"] for item in triaged_findings if item.get("severity")
    )
    next_action_counts = Counter(
        item["next_action"] for item in triaged_findings if item.get("next_action")
    )
    redaction_count = len(redaction_findings)
    ok = not errors and redaction_count == 0
    decision_status = "pilot_feedback_triaged" if ok else "blocked_private_feedback"
    if errors:
        decision_status = "invalid_feedback"

    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-real-use-feedback-triage",
        "ok": ok,
        "decision_status": decision_status,
        "finding_count": len(triaged_findings),
        "redaction_count": redaction_count,
        "category_counts": dict(sorted(category_counts.items())),
        "severity_counts": dict(sorted(severity_counts.items())),
        "next_action_counts": dict(sorted(next_action_counts.items())),
        "errors": errors,
        "findings": triaged_findings,
        "sanitized_feedback": sanitized_feedback,
        "recommended_next_actions": feedback_next_actions(
            errors=errors,
            redaction_count=redaction_count,
            next_action_counts=next_action_counts,
        ),
    }


def feedback_next_actions(
    errors: list[str], redaction_count: int, next_action_counts: Counter[str]
) -> list[str]:
    if errors:
        return ["Fix the feedback JSON schema before using it for pilot decisions."]
    if redaction_count:
        return [
            "Edit the source feedback to remove private paths or secret-like values.",
            "Use only sanitized artifacts for public issues or scenario candidates.",
        ]
    actions = ["Review triaged findings before opening public issues."]
    if next_action_counts.get("candidate", 0) > 0:
        actions.append("Turn candidate-worthy findings into sanitized eval scenarios.")
    if next_action_counts.get("fix", 0) > 0:
        actions.append("Prioritize code fixes for blocker and high severity findings.")
    if next_action_counts.get("docs", 0) > 0:
        actions.append("Patch docs for repeated workflow or onboarding gaps.")
    return actions


def issue_draft(triage: dict[str, Any]) -> str:
    lines = [
        "# V1 Real-Use Pilot Feedback Draft",
        "",
        f"Decision: `{triage['decision_status']}`",
        f"Findings: {triage['finding_count']}",
        "",
        "## Findings",
        "",
    ]
    for finding in triage["findings"]:
        lines.extend(
            [
                f"### {finding['id']}: {finding['title']}",
                "",
                f"- Category: `{finding['category']}`",
                f"- Severity: `{finding['severity']}`",
                f"- Next action: `{finding['next_action']}`",
                f"- Summary: {finding['summary']}",
                f"- Expected: {finding['expected']}",
                f"- Actual: {finding['actual']}",
                "",
            ]
        )
    lines.extend(["## Next Actions", ""])
    for action in triage["recommended_next_actions"]:
        lines.append(f"- {action}")
    lines.append("")
    return "\n".join(lines)


class PilotRun:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.run_label = args.run_label or time.strftime("local-%Y%m%d-%H%M%S")
        if not RUN_LABEL_RE.match(self.run_label):
            raise PilotFailure(
                "run label may contain only letters, digits, '-', '_', '.', or '/'"
            )
        self.out_dir = (
            Path(args.out_dir)
            if args.out_dir
            else ROOT / "evals" / "runs" / "v1-real-use-pilot" / self.run_label
        )
        self.workspace_dir = self.out_dir / "workspace"
        self.store = self.workspace_dir / ".mneme" / "mneme-v1.json"
        self.config = self.workspace_dir / ".mneme" / "mneme-agent-hook.env"
        self.reports_dir = self.out_dir / "reports"
        self.commands_dir = self.out_dir / "commands"
        self.mneme_bin = ROOT / "target/debug/mneme"
        self.command_artifacts: list[dict[str, Any]] = []
        self.preflight: dict[str, Any] = {"status": "skipped"}
        self.feedback: dict[str, Any] | None = None

    def prepare(self) -> None:
        if self.out_dir.exists():
            if not self.args.force:
                raise PilotFailure(
                    f"output directory already exists: {self.out_dir}; use --force"
                )
            shutil.rmtree(self.out_dir)
        self.reports_dir.mkdir(parents=True)
        self.commands_dir.mkdir(parents=True)
        self.workspace_dir.mkdir(parents=True)
        write_json(self.out_dir / "pilot-contract.json", contract())
        write_json(self.out_dir / "feedback-template.json", feedback_template())
        if not self.args.no_build:
            self.run_external("build-mneme-cli", ["cargo", "build", "-q", "-p", "mneme-cli"])

    def run_external(
        self, name: str, command: list[str], env: dict[str, str] | None = None
    ) -> None:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
        )
        stdout_path = self.commands_dir / f"{name}.stdout.txt"
        stderr_path = self.commands_dir / f"{name}.stderr.txt"
        stdout_path.write_text(result.stdout, encoding="utf-8")
        stderr_path.write_text(result.stderr, encoding="utf-8")
        self.command_artifacts.append(
            {
                "name": name,
                "argv": redact_repo_paths(command),
                "returncode": result.returncode,
                "stdout": str(stdout_path),
                "stderr": str(stderr_path),
            }
        )
        if result.returncode != 0:
            raise PilotFailure(f"{name} failed with exit {result.returncode}: {result.stderr}")

    def run_mneme(self, name: str, args: list[str]) -> dict[str, Any]:
        result = subprocess.run(
            [str(self.mneme_bin), *args],
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        stdout_path = self.commands_dir / f"{name}.json"
        stderr_path = self.commands_dir / f"{name}.stderr.txt"
        stdout_path.write_text(result.stdout, encoding="utf-8")
        stderr_path.write_text(result.stderr, encoding="utf-8")
        self.command_artifacts.append(
            {
                "name": name,
                "argv": redact_repo_paths([str(self.mneme_bin), *args]),
                "returncode": result.returncode,
                "stdout": str(stdout_path),
                "stderr": str(stderr_path),
            }
        )
        if result.returncode != 0:
            raise PilotFailure(f"{name} failed with exit {result.returncode}: {result.stderr}")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise PilotFailure(f"{name} did not emit JSON: {exc}") from exc

    def run_preflight(self) -> None:
        if self.args.skip_preflight:
            self.preflight = {"status": "skipped"}
            return
        preflight_dir = self.out_dir / "manual-dogfood-preflight"
        env = os.environ.copy()
        env["MNEME_DOGFOOD_RUN_LABEL"] = f"{self.run_label}-manual-preflight"
        env["MNEME_DOGFOOD_OUT_DIR"] = str(preflight_dir)
        self.run_external(
            "manual-dogfood-preflight",
            [str(ROOT / "scripts/v1-manual-dogfood.py"), "--out-dir", str(preflight_dir), "--force"],
            env,
        )
        summary = read_json(preflight_dir / "summary.json")
        if summary.get("decision_status") != "v1_manual_dogfood_passed":
            raise PilotFailure(
                f"manual dogfood preflight failed: {summary.get('decision_status')}"
            )
        self.preflight = {
            "status": "passed",
            "out_dir": str(preflight_dir),
            "decision_status": summary["decision_status"],
            "mock_record_count": summary.get("mock_record_count"),
            "passed_workflows": summary.get("passed_workflows"),
        }

    def init_workspace(self) -> None:
        init_report = self.run_mneme(
            "pilot-init",
            [
                "init",
                "--store",
                str(self.store),
                "--config",
                str(self.config),
                "--no-bin",
                "--force",
                "--json",
            ],
        )
        doctor_report = self.run_mneme(
            "pilot-doctor",
            [
                "doctor",
                "--store",
                str(self.store),
                "--config",
                str(self.config),
                "--json",
            ],
        )
        if not init_report.get("store_created") and not init_report.get("store_overwritten"):
            raise PilotFailure("pilot init did not create or overwrite the store")
        if not doctor_report.get("ok"):
            raise PilotFailure("pilot doctor did not report a valid workspace")
        self.write_runbook()

    def triage_feedback_file(self) -> None:
        if not self.args.feedback:
            self.feedback = {"status": "not_provided"}
            return
        source = Path(self.args.feedback)
        feedback = read_json(source)
        triage = triage_feedback(feedback)
        sanitized_path = self.reports_dir / "sanitized-feedback.json"
        triage_path = self.reports_dir / "feedback-triage.json"
        issue_path = self.reports_dir / "issue-draft.md"
        write_json(sanitized_path, triage["sanitized_feedback"])
        write_json(triage_path, {key: value for key, value in triage.items() if key != "sanitized_feedback"})
        issue_path.write_text(issue_draft(triage), encoding="utf-8")
        self.feedback = {
            "status": "triaged",
            "source": source.name,
            "triage_report": str(triage_path),
            "sanitized_feedback": str(sanitized_path),
            "issue_draft": str(issue_path),
            "decision_status": triage["decision_status"],
            "finding_count": triage["finding_count"],
            "redaction_count": triage["redaction_count"],
            "category_counts": triage["category_counts"],
            "severity_counts": triage["severity_counts"],
            "next_action_counts": triage["next_action_counts"],
        }
        if triage["errors"]:
            raise PilotFailure("; ".join(triage["errors"]))

    def write_runbook(self) -> None:
        runbook = self.out_dir / "pilot-runbook.md"
        runbook.write_text(
            "\n".join(
                [
                    "# V1 Real-Use Pilot Runbook",
                    "",
                    "This bundle is local-only and ignored by git.",
                    "",
                    "## Workspace",
                    "",
                    f"- Store: `{self.store}`",
                    f"- Config: `{self.config}`",
                    "",
                    "## Commands",
                    "",
                    "```sh",
                    f"target/debug/mneme doctor --store {self.store} --config {self.config}",
                    f"target/debug/mneme remember \"user prefers concise pilot notes\" --store {self.store}",
                    f"target/debug/mneme context \"pilot notes\" --store {self.store} --json",
                    f"target/debug/mneme quality --store {self.store} --json",
                    "```",
                    "",
                    "Use `feedback-template.json` as the source format for sanitized pilot feedback.",
                    "Do not paste private conversation text, secrets, or local project paths into feedback intended for public issues.",
                    "",
                ]
            ),
            encoding="utf-8",
        )

    def write_summary(self, status: str, error: str | None = None) -> dict[str, Any]:
        feedback_status = self.feedback or {"status": "not_provided"}
        decision_status = "ready_for_real_use_pilot"
        if feedback_status.get("status") == "triaged":
            decision_status = feedback_status.get("decision_status", "pilot_feedback_triaged")
        if status != "passed":
            decision_status = "blocked"
        report = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-real-use-pilot",
            "run_label": self.run_label,
            "status": status,
            "decision_status": decision_status,
            "out_dir": str(self.out_dir),
            "workspace": {
                "path": str(self.workspace_dir),
                "store": str(self.store),
                "config": str(self.config),
            },
            "preflight": self.preflight,
            "feedback": feedback_status,
            "reports": {
                "contract": str(self.out_dir / "pilot-contract.json"),
                "feedback_template": str(self.out_dir / "feedback-template.json"),
                "runbook": str(self.out_dir / "pilot-runbook.md"),
                "commands": str(self.commands_dir),
            },
            "command_artifacts": self.command_artifacts,
            "recommended_next_actions": pilot_next_actions(feedback_status),
        }
        if error:
            report["error"] = error
        write_json(self.out_dir / "summary.json", report)
        return report


def pilot_next_actions(feedback: dict[str, Any]) -> list[str]:
    if feedback.get("status") == "not_provided":
        return [
            "Use the generated workspace for private real-use pilot sessions.",
            "Record only sanitized behavior feedback in feedback-template.json.",
            "Rerun this script with --feedback <path> to triage pilot findings.",
        ]
    if feedback.get("decision_status") == "blocked_private_feedback":
        return [
            "Edit the source feedback to remove private paths or secret-like values.",
            "Review sanitized-feedback.json before creating public issues.",
        ]
    return [
        "Review issue-draft.md and decide which findings become fixes, docs, or eval candidates.",
        "Promote only sanitized findings into public issues or scenario candidates.",
    ]


def check_feedback_file(path: Path) -> dict[str, Any]:
    feedback = read_json(path)
    triage = triage_feedback(feedback)
    return {key: value for key, value in triage.items() if key != "sanitized_feedback"}


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def redact_repo_paths(argv: list[str]) -> list[str]:
    root = str(ROOT)
    return [arg.replace(root, "<repo>") for arg in argv]


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Prepare local v1 real-use pilot evidence and triage sanitized feedback."
    )
    parser.add_argument("--run-label", help="Run label for evals/runs/v1-real-use-pilot.")
    parser.add_argument("--out-dir", help="Explicit output directory for the pilot bundle.")
    parser.add_argument("--feedback", help="Optional feedback JSON to sanitize and triage.")
    parser.add_argument("--force", action="store_true", help="Replace an existing output directory.")
    parser.add_argument("--skip-preflight", action="store_true", help="Skip Phase 36 manual dogfood preflight.")
    parser.add_argument("--no-build", action="store_true", help="Do not build mneme-cli before pilot setup.")
    parser.add_argument("--check-contract", action="store_true", help="Print the pilot feedback contract and exit.")
    parser.add_argument("--check-feedback", help="Validate and summarize a feedback JSON file without creating a pilot bundle.")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.check_contract:
        print(json.dumps(contract(), ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    if args.check_feedback:
        report = check_feedback_file(Path(args.check_feedback))
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if report["decision_status"] != "invalid_feedback" else 1

    run = PilotRun(args)
    try:
        run.prepare()
        run.run_preflight()
        run.init_workspace()
        run.triage_feedback_file()
        summary = run.write_summary("passed")
        print(f"v1-real-use-pilot: wrote {run.out_dir}")
        print(f"v1-real-use-pilot: summary {run.out_dir / 'summary.json'}")
        print(f"v1-real-use-pilot: decision {summary['decision_status']}")
        return 0
    except Exception as exc:  # noqa: BLE001 - always write a local summary.
        run.write_summary("failed", error=str(exc))
        print(f"v1-real-use-pilot: failed: {exc}", file=sys.stderr)
        print(f"v1-real-use-pilot: summary {run.out_dir / 'summary.json'}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
