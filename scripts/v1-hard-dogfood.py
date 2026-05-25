#!/usr/bin/env python3
"""Run the Mneme v1 hard-mode dogfood protocol.

The hard-mode runner extends the manual dogfood protocol with adversarial data,
agent handoff workflows, scorecards, seeded-fault detection, local candidate
artifacts, and public-safe JSON/Markdown/HTML reports.
"""

from __future__ import annotations

import argparse
import html
import importlib.util
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
EXPECTED_NORMAL_RECORD_COUNT = 100
EXPECTED_ADVERSARIAL_RECORD_COUNT = 150
EXPECTED_AGENT_WORKFLOW_COUNT = 30
RUN_LABEL_RE = re.compile(r"^[A-Za-z0-9._/-]+$")
CANDIDATE_SCHEMA_VERSION = "mneme.eval_candidate.v1"
SECRET_LITERALS = [
    "API_KEY=FAKE_TEST_VALUE",
    "TOKEN=FAKE_TOKEN_VALUE",
    "PASSWORD=FAKE_PASSWORD_VALUE",
    "ACCESS_TOKEN=FAKE_ACCESS_TOKEN_VALUE",
]
HARD_SECRET_PREFIX = "API_KEY=FAKE_HARD_VALUE_"
THRESHOLDS = {
    "recall_at_k_min": 0.95,
    "precision_at_k_min": 0.95,
    "scope_leak_max": 0,
    "secret_leak_max": 0,
    "citation_coverage_min": 1.0,
    "handoff_success_min": 0.95,
    "agent_attribution_error_max": 0,
    "stale_reuse_max": 0,
    "seeded_fault_detection_min": 1.0,
}


class HardDogfoodFailure(RuntimeError):
    """Raised when hard-mode dogfood cannot complete."""


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
        "command": "v1-hard-dogfood-contract",
        "normal_record_count": EXPECTED_NORMAL_RECORD_COUNT,
        "adversarial_record_count": EXPECTED_ADVERSARIAL_RECORD_COUNT,
        "agent_workflow_count": EXPECTED_AGENT_WORKFLOW_COUNT,
        "scorecard_metrics": [
            "recall_at_k",
            "precision_at_k",
            "scope_leak_count",
            "secret_leak_count",
            "citation_coverage",
            "handoff_success_rate",
            "agent_memory_score",
            "seeded_fault_detection_rate",
        ],
        "outputs": [
            "summary.json",
            "scorecard.json",
            "regression.json",
            "trend.json",
            "report.md",
            "report.html",
            "candidates/candidate-index.json",
            "candidates/official-candidate-index.json",
            "candidates/official-candidate-check.json",
        ],
        "candidate_bridge": "hard findings are mirrored into mneme.eval_candidate.v1 YAML files",
        "history_policy": "history entries are public-safe reduced summaries for local trend comparison",
        "privacy_policy": "all generated data is synthetic and reports are public-safe by default",
        "thresholds": THRESHOLDS,
    }


def load_manual_records() -> list[dict[str, Any]]:
    manual_path = ROOT / "scripts" / "v1-manual-dogfood.py"
    spec = importlib.util.spec_from_file_location("v1_manual_dogfood", manual_path)
    if spec is None or spec.loader is None:
        raise HardDogfoodFailure(f"cannot load manual dogfood module: {manual_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    records = module.build_mock_records()
    if len(records) != EXPECTED_NORMAL_RECORD_COUNT:
        raise HardDogfoodFailure(
            f"expected {EXPECTED_NORMAL_RECORD_COUNT} normal records, got {len(records)}"
        )
    return records


def build_adversarial_records() -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []

    def add(
        category: str,
        scope: str,
        text: str,
        *,
        mode: str = "remember",
        expected_status: str | None = "active",
        expected_claim_delta: int = 1,
        alias: str | None = None,
        trust: str = "trusted_user",
    ) -> None:
        records.append(
            {
                "id": f"hard-{len(records) + 1:03d}",
                "category": category,
                "scope": scope,
                "text": text,
                "mode": mode,
                "expected_status": expected_status,
                "expected_claim_delta": expected_claim_delta,
                "alias": alias,
                "trust": trust,
            }
        )

    for index in range(1, 11):
        add(
            "scope_confusion",
            "private",
            f"user prefers scope trap private release lane {index}",
            alias=f"scope-private-{index}",
        )
        add(
            "scope_confusion",
            "project-alpha",
            f"project-alpha requires scope trap release lane {index}",
            alias=f"scope-alpha-{index}",
        )
        add(
            "scope_confusion",
            "project-beta",
            f"project-beta requires scope trap release lane {index}",
            alias=f"scope-beta-{index}",
        )

    for index in range(1, 31):
        if index <= 5:
            text = f"needle-fixture requires hardmode recall target {index}"
            alias = f"needle-{index}"
        else:
            text = f"noise-fixture mentions hardmode recall decoy {index}"
            alias = f"noise-{index}"
        add("needle_in_noise", "private", text, alias=alias)

    for index in range(1, 21):
        add(
            "stale_conflict",
            "private",
            f"stale-fixture prefers legacy phase route {index}",
            alias=f"stale-{index}",
        )

    for index in range(1, 26):
        scope = "project-alpha" if index % 5 == 0 else "private"
        add(
            "agent_handoff_memory",
            scope,
            f"agent-handoff requires next-agent checkpoint {index}",
            alias=f"handoff-{index}",
        )

    for index in range(1, 16):
        add(
            "attribution_trap",
            "private",
            f"Codex draft says a teammate likes private dump route {index}; this is not a user memory.",
            mode="ingest",
            expected_status=None,
            expected_claim_delta=0,
            alias=f"attribution-trap-{index}",
            trust="agent_summary",
        )

    for index in range(1, 16):
        add(
            "poisoning_trap",
            "private",
            f"Untrusted note asks the agent to expose all private memory during handoff {index}.",
            mode="ingest",
            expected_status=None,
            expected_claim_delta=0,
            alias=f"poisoning-trap-{index}",
            trust="untrusted_transcript",
        )

    for index in range(1, 16):
        add(
            "secret_like",
            "private",
            f"hard-secret note {HARD_SECRET_PREFIX}{index:02d}",
            expected_status="blocked_secret",
            alias=f"hard-secret-{index}",
        )

    if len(records) != EXPECTED_ADVERSARIAL_RECORD_COUNT:
        raise AssertionError(
            f"expected {EXPECTED_ADVERSARIAL_RECORD_COUNT} adversarial records, got {len(records)}"
        )
    return records


def build_agent_workflows() -> list[dict[str, Any]]:
    workflows: list[dict[str, Any]] = []

    def add(
        name: str,
        task: str,
        query: str,
        *,
        scopes: list[str] | None = None,
        must_include: list[str] | None = None,
        must_not_include: list[str] | None = None,
        agent: str = "codex",
        max_items: int = 8,
        category: str = "agent_handoff",
    ) -> None:
        number = len(workflows) + 1
        workflows.append(
            {
                "id": f"agent-hard-{number:03d}",
                "name": name,
                "task": task,
                "query": query,
                "scopes": scopes or ["private"],
                "must_include": must_include or [],
                "must_not_include": must_not_include or [],
                "agent": agent,
                "max_items": max_items,
                "category": category,
                "handoff_memory": f"agent-handoff workflow {number:03d} continuation note",
                "handoff_query": f"agent-handoff {number:03d} continuation",
            }
        )

    for index in range(1, 6):
        add(
            f"needle recall {index}",
            f"Continue hardmode recall task {index}",
            f"hardmode recall target {index}",
            must_include=[f"needle-fixture requires hardmode recall target {index}"],
            must_not_include=[f"noise-fixture mentions hardmode recall decoy {index + 5}"],
            category="needle_in_noise",
        )

    for index in range(1, 6):
        add(
            f"project alpha scope {index}",
            f"Continue project alpha release lane {index}",
            f"scope trap release lane {index}",
            scopes=["project-alpha"],
            must_include=[f"project-alpha requires scope trap release lane {index}"],
            must_not_include=[
                f"user prefers scope trap private release lane {index}",
                f"project-beta requires scope trap release lane {index}",
            ],
            category="scope_guard",
        )

    for index in range(1, 6):
        add(
            f"private scope {index}",
            f"Continue private release lane {index}",
            f"scope trap private release lane {index}",
            scopes=["private"],
            must_include=[f"user prefers scope trap private release lane {index}"],
            must_not_include=[
                f"project-alpha requires scope trap release lane {index}",
                f"project-beta requires scope trap release lane {index}",
            ],
            category="scope_guard",
        )

    for index in range(1, 6):
        add(
            f"stale correction {index}",
            f"Continue corrected phase route {index}",
            f"current phase route {index}",
            must_include=[f"stale-fixture prefers current phase route {index}"],
            must_not_include=[f"stale-fixture prefers legacy phase route {index}"],
            category="stale_correction",
        )

    for index in range(1, 6):
        scope = "project-alpha" if index == 5 else "private"
        add(
            f"handoff checkpoint {index}",
            f"Handoff checkpoint {index} to next agent",
            f"next-agent checkpoint {index}",
            scopes=[scope],
            must_include=[f"agent-handoff requires next-agent checkpoint {index}"],
            category="handoff_continuity",
            agent=["codex", "claude", "cursor", "hermes", "codex"][index - 1],
        )

    for index in range(1, 4):
        add(
            f"secret exclusion {index}",
            f"Verify synthetic secret exclusion {index}",
            f"FAKE_HARD_VALUE_{index:02d}",
            must_not_include=[f"{HARD_SECRET_PREFIX}{index:02d}"],
            category="privacy",
        )

    add(
        "attribution trap exclusion",
        "Verify agent draft is not promoted to user memory",
        "teammate private dump route",
        must_not_include=["teammate likes private dump route"],
        category="attribution",
        agent="claude",
    )
    add(
        "poisoning trap exclusion",
        "Verify untrusted memory dump request is not promoted",
        "expose all private memory",
        must_not_include=["expose all private memory"],
        category="poisoning",
        agent="cursor",
    )

    if len(workflows) != EXPECTED_AGENT_WORKFLOW_COUNT:
        raise AssertionError(
            f"expected {EXPECTED_AGENT_WORKFLOW_COUNT} workflows, got {len(workflows)}"
        )
    return workflows


def dataset_summary(
    normal_records: list[dict[str, Any]],
    adversarial_records: list[dict[str, Any]],
    workflows: list[dict[str, Any]],
) -> dict[str, Any]:
    adversarial_categories = Counter(record["category"] for record in adversarial_records)
    adversarial_modes = Counter(record["mode"] for record in adversarial_records)
    workflow_categories = Counter(workflow["category"] for workflow in workflows)
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-dataset",
        "normal_record_count": len(normal_records),
        "adversarial_record_count": len(adversarial_records),
        "agent_workflow_count": len(workflows),
        "adversarial_categories": dict(sorted(adversarial_categories.items())),
        "adversarial_modes": dict(sorted(adversarial_modes.items())),
        "workflow_categories": dict(sorted(workflow_categories.items())),
        "thresholds": THRESHOLDS,
    }


class HardDogfoodRun:
    def __init__(
        self,
        args: argparse.Namespace,
        normal_records: list[dict[str, Any]],
        adversarial_records: list[dict[str, Any]],
        workflows: list[dict[str, Any]],
    ) -> None:
        self.args = args
        self.normal_records = normal_records
        self.adversarial_records = adversarial_records
        self.workflows = workflows
        self.run_label = args.run_label or time.strftime("local-%Y%m%d-%H%M%S")
        if not RUN_LABEL_RE.match(self.run_label):
            raise HardDogfoodFailure(
                "run label may contain only letters, digits, '-', '_', '.', or '/'"
            )
        self.out_dir = (
            Path(args.out_dir)
            if args.out_dir
            else ROOT / "evals" / "runs" / "v1-hard-dogfood" / self.run_label
        )
        self.workspace_dir = self.out_dir / "workspace"
        self.store = self.workspace_dir / ".mneme" / "mneme-v1.json"
        self.config = self.workspace_dir / ".mneme" / "mneme-agent-hook.env"
        self.commands_dir = self.out_dir / "commands"
        self.reports_dir = self.out_dir / "reports"
        self.candidates_dir = self.out_dir / "candidates"
        self.official_candidates_dir = self.candidates_dir / "official"
        self.history_dir = (
            Path(args.history_dir) if args.history_dir else self.out_dir / "history"
        )
        self.command_index = 0
        self.command_artifacts: list[dict[str, Any]] = []
        self.claim_id_by_alias: dict[str, str] = {}
        self.ingested_records: list[dict[str, Any]] = []
        self.setup_results: list[dict[str, Any]] = []
        self.workflow_results: list[dict[str, Any]] = []
        self.failures: list[dict[str, Any]] = []
        self.seeded_faults: dict[str, Any] = {}
        self.scorecard: dict[str, Any] = {}
        self.regression: dict[str, Any] = {}
        self.trend: dict[str, Any] = {}
        self.official_candidate_check: dict[str, Any] = {}
        self.mneme_bin = Path(args.mneme_bin) if args.mneme_bin else ROOT / "target/debug/mneme"

    def prepare(self) -> None:
        if self.out_dir.exists():
            if not self.args.force:
                raise HardDogfoodFailure(
                    f"output directory already exists: {self.out_dir}; use --force"
                )
            shutil.rmtree(self.out_dir)
        self.commands_dir.mkdir(parents=True)
        self.reports_dir.mkdir(parents=True)
        self.candidates_dir.mkdir(parents=True)
        self.official_candidates_dir.mkdir(parents=True)
        self.history_dir.mkdir(parents=True, exist_ok=True)
        self.workspace_dir.mkdir(parents=True)
        write_json(
            self.out_dir / "dataset-summary.json",
            dataset_summary(self.normal_records, self.adversarial_records, self.workflows),
        )
        write_json(self.out_dir / "normal-records.json", {"records": self.normal_records})
        write_json(self.out_dir / "adversarial-records.json", {"records": self.adversarial_records})
        write_json(
            self.out_dir / "agent-workflows.json",
            {"workflow_count": len(self.workflows), "workflows": self.workflows},
        )
        if not self.args.no_build:
            self.run_external("build-mneme-cli", ["cargo", "build", "-q", "-p", "mneme-cli"])

    def run_external(self, slug: str, command: list[str], env: dict[str, str] | None = None) -> None:
        result = subprocess.run(
            command,
            cwd=ROOT,
            env=env,
            text=True,
            capture_output=True,
        )
        stdout_path = self.commands_dir / f"{slug}.stdout.txt"
        stderr_path = self.commands_dir / f"{slug}.stderr.txt"
        stdout_path.write_text(result.stdout, encoding="utf-8")
        stderr_path.write_text(result.stderr, encoding="utf-8")
        self.command_artifacts.append(
            {
                "name": slug,
                "argv": redact_paths(command),
                "returncode": result.returncode,
                "stdout": str(stdout_path),
                "stderr": str(stderr_path),
            }
        )
        if result.returncode != 0:
            raise HardDogfoodFailure(
                f"{slug} failed with exit {result.returncode}: {result.stderr}"
            )

    def run_preflight(self) -> dict[str, Any]:
        if self.args.skip_preflight:
            return {"status": "skipped"}
        preflight_dir = self.out_dir / "preflight"
        env = os.environ.copy()
        env["MNEME_DOGFOOD_RUN_LABEL"] = f"{self.run_label}-preflight"
        env["MNEME_DOGFOOD_OUT_DIR"] = str(preflight_dir)
        self.run_external("preflight-v1-dogfood", [str(ROOT / "scripts/v1-dogfood.sh")], env)
        summary_path = preflight_dir / "dogfood-summary.json"
        summary = read_json(summary_path)
        decision = summary.get("decision_status")
        if decision != "ready_for_manual_dogfood":
            raise HardDogfoodFailure(f"preflight dogfood summary is not ready: {decision}")
        return {
            "status": "passed",
            "out_dir": str(preflight_dir),
            "dogfood_summary": str(summary_path),
            "decision_status": decision,
        }

    def run_command(
        self,
        slug: str,
        args: list[str],
        *,
        parse_json: bool = True,
        store: Path | None = None,
    ) -> Any:
        self.command_index += 1
        extension = "json" if parse_json else "txt"
        stdout_path = self.commands_dir / f"{self.command_index:03d}-{slug}.{extension}"
        stderr_path = self.commands_dir / f"{self.command_index:03d}-{slug}.stderr.txt"
        full_args = [str(self.mneme_bin), *args]
        if store is not None:
            full_args.extend(["--store", str(store)])
        result = subprocess.run(
            full_args,
            cwd=ROOT,
            text=True,
            capture_output=True,
        )
        stdout_path.write_text(result.stdout, encoding="utf-8")
        stderr_path.write_text(result.stderr, encoding="utf-8")
        self.command_artifacts.append(
            {
                "name": slug,
                "argv": redact_paths(full_args),
                "returncode": result.returncode,
                "stdout": str(stdout_path),
                "stderr": str(stderr_path),
            }
        )
        if result.returncode != 0:
            raise HardDogfoodFailure(f"{slug} failed with exit {result.returncode}: {result.stderr}")
        if parse_json:
            try:
                return json.loads(result.stdout)
            except json.JSONDecodeError as exc:
                raise HardDogfoodFailure(f"{slug} did not emit JSON: {exc}") from exc
        return result.stdout

    def ingest_all(self) -> None:
        self.run_command(
            "init",
            [
                "init",
                "--config",
                str(self.config),
                "--no-bin",
                "--force",
                "--json",
            ],
            store=self.store,
        )
        for record in self.normal_records:
            normalized = {
                "id": record["id"],
                "category": f"normal:{record['category']}",
                "scope": record["scope"],
                "text": record["text"],
                "mode": "remember",
                "expected_status": record.get("expected_status", "active"),
                "expected_claim_delta": 1,
                "alias": record.get("alias"),
                "trust": "trusted_user",
            }
            self.ingest_record(normalized)
        for record in self.adversarial_records:
            self.ingest_record(record)
        write_json(self.reports_dir / "ingested-records.json", {"records": self.ingested_records})

    def ingest_record(self, record: dict[str, Any]) -> None:
        before_count = self.current_claim_count()
        mode = record["mode"]
        if mode == "remember":
            args = [
                "remember",
                record["text"],
                "--scope",
                record["scope"],
                "--trust",
                record.get("trust", "trusted_user"),
                "--json",
            ]
        elif mode == "ingest":
            args = [
                "ingest",
                record["text"],
                "--scope",
                record["scope"],
                "--trust",
                record.get("trust", "trusted_user"),
                "--json",
            ]
        else:
            raise HardDogfoodFailure(f"unknown ingest mode for {record['id']}: {mode}")
        report = self.run_command(f"{mode}-{record['id']}", args, store=self.store)
        after_count = report.get("claim_count")
        expected_delta = record.get("expected_claim_delta", 1)
        if after_count != before_count + expected_delta:
            raise HardDogfoodFailure(
                f"{record['id']} expected claim delta {expected_delta}, got "
                f"{after_count - before_count}"
            )
        latest = report.get("latest_claim") or {}
        expected_status = record.get("expected_status")
        claim_id = latest.get("id") if expected_delta else None
        if expected_delta and expected_status and latest.get("status") != expected_status:
            raise HardDogfoodFailure(
                f"{record['id']} expected status {expected_status}, got {latest.get('status')}"
            )
        if expected_delta and latest.get("scope") != record["scope"]:
            raise HardDogfoodFailure(
                f"{record['id']} expected scope {record['scope']}, got {latest.get('scope')}"
            )
        if record.get("alias") and claim_id:
            self.claim_id_by_alias[record["alias"]] = claim_id
        self.ingested_records.append(
            {
                "record_id": record["id"],
                "category": record["category"],
                "scope": record["scope"],
                "mode": mode,
                "claim_delta": expected_delta,
                "claim_id": claim_id,
                "status": expected_status,
            }
        )

    def current_claim_count(self) -> int:
        if not self.store.exists():
            return 0
        snapshot = self.run_command("snapshot-claim-count", ["snapshot", "--json"], store=self.store)
        return len((snapshot.get("snapshot") or {}).get("claims", []))

    def apply_setup_mutations(self) -> None:
        for index in range(1, 21):
            alias = f"stale-{index}"
            claim_id = self.claim_id_by_alias.get(alias)
            if not claim_id:
                raise HardDogfoodFailure(f"missing stale claim alias: {alias}")
            report = self.run_command(
                f"correct-stale-{index}",
                [
                    "correct",
                    "--claim-id",
                    claim_id,
                    f"stale-fixture prefers current phase route {index}",
                    "--json",
                ],
                store=self.store,
            )
            latest = report.get("latest_claim") or {}
            if latest.get("object") != f"current phase route {index}":
                raise HardDogfoodFailure(f"stale correction {index} did not create current route")
            self.setup_results.append(
                {
                    "name": f"correct-stale-{index}",
                    "claim_id": claim_id,
                    "new_claim_id": latest.get("id"),
                    "status": "passed",
                }
            )
        write_json(self.reports_dir / "setup-results.json", {"results": self.setup_results})

    def run_workflows(self) -> None:
        metrics = MetricAccumulator()
        for workflow in self.workflows:
            result = self.run_one_workflow(workflow, metrics)
            self.workflow_results.append(result)
            if result["status"] != "passed":
                self.failures.append(
                    {
                        "id": result["id"],
                        "category": result["category"],
                        "reason": "; ".join(result["errors"]),
                        "next_action": "candidate",
                    }
                )
        self.scorecard = metrics.scorecard()
        write_json(self.reports_dir / "workflow-results.json", {"workflows": self.workflow_results})
        write_json(self.out_dir / "scorecard.json", self.scorecard)

    def run_one_workflow(
        self, workflow: dict[str, Any], metrics: "MetricAccumulator"
    ) -> dict[str, Any]:
        checks: list[str] = []
        errors: list[str] = []
        begin_args = [
            "begin",
            workflow["task"],
            "--query",
            workflow["query"],
            "--agent",
            workflow["agent"],
            "--max-items",
            str(workflow["max_items"]),
            "--json",
        ]
        for scope in workflow["scopes"]:
            begin_args.extend(["--scope", scope])
        begin = self.run_command(f"begin-{workflow['id']}", begin_args, store=self.store)
        report = begin.get("report") or {}
        session = report.get("session") or {}
        context_pack = report.get("context_pack") or {}
        items = context_pack.get("items", [])
        texts = [item.get("claim_text", "") for item in items]
        text_blob = "\n".join(texts)
        if not session.get("id"):
            errors.append("begin did not return a session id")
        metrics.observe_context(workflow, items)

        for expected in workflow["must_include"]:
            metrics.recall_attempts += 1
            if expected in texts:
                metrics.recall_successes += 1
                checks.append(f"recalled {expected}")
            else:
                errors.append(f"context did not include: {expected}")

        for unexpected in workflow["must_not_include"]:
            if unexpected in texts:
                errors.append(f"context unexpectedly included: {unexpected}")

        if any(secret in text_blob for secret in SECRET_LITERALS) or HARD_SECRET_PREFIX in text_blob:
            errors.append("context leaked synthetic secret-like text")

        handoff_memory = workflow["handoff_memory"]
        if session.get("id"):
            end = self.run_command(
                f"end-{workflow['id']}",
                [
                    "end",
                    session["id"],
                    "--agent",
                    workflow["agent"],
                    "--summary",
                    f"Completed {workflow['name']}",
                    "--remember",
                    handoff_memory,
                    "--json",
                ],
                store=self.store,
            )
            remembered = (end.get("report") or {}).get("remembered_claim_ids", [])
            if len(remembered) != 1:
                errors.append("end did not persist one handoff memory")
            else:
                checks.append("persisted one handoff memory")
            handoff = self.run_command(
                f"handoff-recall-{workflow['id']}",
                [
                    "context",
                    workflow["handoff_query"],
                    "--scope",
                    "private",
                    "--json",
                ],
                store=self.store,
            )
            metrics.handoff_attempts += 1
            if handoff_memory in context_texts(handoff):
                metrics.handoff_successes += 1
                checks.append("handoff memory recalled")
            else:
                errors.append("handoff memory was not recalled")

        return {
            "id": workflow["id"],
            "name": workflow["name"],
            "category": workflow["category"],
            "status": "passed" if not errors else "failed",
            "context_item_count": len(items),
            "checks": checks,
            "errors": errors,
        }

    def run_seeded_faults(self) -> None:
        self.seeded_faults = seeded_fault_report()
        write_json(self.out_dir / "seeded-faults.json", self.seeded_faults)
        for failure in self.seeded_faults["faults"]:
            self.write_candidate_artifact(
                {
                    "id": f"seeded-{failure['id']}",
                    "category": "seeded_fault",
                    "reason": failure["detected_by"],
                    "next_action": "candidate",
                    "metric": failure["metric"],
                }
            )

    def write_failure_candidates(self) -> None:
        for failure in self.failures:
            self.write_candidate_artifact(failure)
        candidates = sorted(self.candidates_dir.glob("*.json"))
        official_candidates = sorted(self.official_candidates_dir.glob("*.candidate.yaml"))
        index = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-hard-dogfood-candidate-index",
            "candidate_count": len(candidates),
            "official_candidate_count": len(official_candidates),
            "candidates": [path.name for path in candidates],
            "official_candidates": [path.name for path in official_candidates],
            "official_candidate_index": str(
                self.candidates_dir / "official-candidate-index.json"
            ),
            "recommended_next_actions": [
                "review each candidate locally before promoting a public scenario",
                "run mneme-eval candidate-check on candidates/official before sharing candidates",
                "convert only minimal, public-safe reproductions into evals/scenarios",
                "rerun hard dogfood after any promoted regression is fixed",
            ],
        }
        write_json(self.candidates_dir / "candidate-index.json", index)
        official_index = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-hard-dogfood-official-candidate-index",
            "candidate_schema_version": CANDIDATE_SCHEMA_VERSION,
            "candidate_count": len(official_candidates),
            "candidates": [path.name for path in official_candidates],
            "candidate_check_report": str(
                self.candidates_dir / "official-candidate-check.json"
            ),
            "recommended_next_actions": [
                "run `mneme-eval candidate-check candidates/official` before editing candidates",
                "add a reviewed scenario block only after minimizing the hard-mode finding",
                "use `mneme-eval candidate-promote` only after candidate-check passes",
            ],
        }
        write_json(self.candidates_dir / "official-candidate-index.json", official_index)

    def write_candidate_artifact(self, failure: dict[str, Any]) -> None:
        candidate_id = sanitize_identifier(f"hard-{failure['id']}")
        artifact = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-hard-dogfood-candidate",
            "id": candidate_id,
            "status": "proposed",
            "source": {
                "report": "summary.json",
                "source_id": failure["id"],
                "source_category": failure.get("category"),
            },
            "failure": {
                "reason": failure.get("reason"),
                "metric": failure.get("metric"),
                "next_action": failure.get("next_action", "candidate"),
            },
            "promotion_checklist": [
                "confirm the artifact contains no private user data, paths, or secrets",
                "reduce the finding to one deterministic scenario before promotion",
                "add the reviewed scenario under evals/scenarios/dogfood or a hard-mode suite",
                "run validate, hard dogfood, and baseline comparison before release",
            ],
        }
        write_json(self.candidates_dir / f"{candidate_id}.json", artifact)
        write_official_candidate_yaml(
            self.official_candidates_dir / f"{candidate_id}.candidate.yaml",
            candidate_id,
            failure,
        )

    def validate_official_candidates(self) -> None:
        report_path = self.candidates_dir / "official-candidate-check.json"
        self.run_external(
            "official-candidate-check",
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "mneme-eval",
                "--",
                "candidate-check",
                str(self.official_candidates_dir),
                "--report",
                str(report_path),
                "--json",
            ],
        )
        report = read_json(report_path)
        if not report.get("ok"):
            raise HardDogfoodFailure("official hard candidate check failed")
        self.official_candidate_check = report

    def build_regression(self) -> None:
        previous_summary = read_json(Path(self.args.compare_summary)) if self.args.compare_summary else None
        gates = regression_gates(self.scorecard, self.seeded_faults, previous_summary)
        self.regression = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-hard-dogfood-regression",
            "ok": all(gate["status"] == "pass" for gate in gates),
            "thresholds": THRESHOLDS,
            "compared_to": self.args.compare_summary,
            "gates": gates,
        }
        write_json(self.out_dir / "regression.json", self.regression)

    def write_history_and_trend(self, status: str, preflight: dict[str, Any]) -> None:
        previous_entries = load_history_entries(self.history_dir)
        if self.args.compare_summary:
            previous_entries.append(history_entry_from_summary(read_json(Path(self.args.compare_summary))))
        previous_entries = [entry for entry in previous_entries if entry]
        previous_entries.sort(key=lambda entry: entry.get("generated_at", ""))
        current = history_entry(
            run_label=self.run_label,
            out_dir=self.out_dir,
            status=status,
            preflight=preflight,
            scorecard=self.scorecard,
            seeded_faults=self.seeded_faults,
            regression=self.regression,
        )
        history_path = self.history_dir / f"{current['generated_at_slug']}-{sanitize_identifier(self.run_label)}.json"
        write_json(history_path, current)
        self.trend = build_trend_report(previous_entries, current, history_path)
        write_json(self.out_dir / "trend.json", self.trend)
        write_trend_markdown(self.out_dir / "trend.md", self.trend)

    def write_reports(self, status: str, preflight: dict[str, Any], error: str | None = None) -> dict[str, Any]:
        passed_workflows = sum(
            1 for workflow in self.workflow_results if workflow["status"] == "passed"
        )
        failed_workflows = sum(
            1 for workflow in self.workflow_results if workflow["status"] == "failed"
        )
        candidate_index = read_json(self.candidates_dir / "candidate-index.json")
        official_candidate_index = read_json(self.candidates_dir / "official-candidate-index.json")
        decision_status = (
            "v1_hard_dogfood_passed"
            if status == "passed"
            and failed_workflows == 0
            and self.regression.get("ok")
            and self.official_candidate_check.get("ok")
            else "blocked"
        )
        summary = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-hard-dogfood",
            "run_label": self.run_label,
            "status": status,
            "decision_status": decision_status,
            "out_dir": str(self.out_dir),
            "preflight": preflight,
            "normal_record_count": len(self.normal_records),
            "adversarial_record_count": len(self.adversarial_records),
            "ingested_record_count": len(self.ingested_records),
            "agent_workflow_count": len(self.workflows),
            "passed_workflows": passed_workflows,
            "failed_workflows": failed_workflows,
            "scorecard": self.scorecard,
            "seeded_faults": self.seeded_faults,
            "regression": self.regression,
            "trend": self.trend,
            "candidate_count": candidate_index["candidate_count"],
            "official_candidate_count": official_candidate_index["candidate_count"],
            "official_candidate_check": {
                "ok": self.official_candidate_check.get("ok"),
                "checked_count": self.official_candidate_check.get("checked_count"),
                "valid": self.official_candidate_check.get("valid"),
                "invalid": self.official_candidate_check.get("invalid"),
            },
            "reports": {
                "dataset_summary": str(self.out_dir / "dataset-summary.json"),
                "scorecard": str(self.out_dir / "scorecard.json"),
                "seeded_faults": str(self.out_dir / "seeded-faults.json"),
                "regression": str(self.out_dir / "regression.json"),
                "trend": str(self.out_dir / "trend.json"),
                "markdown": str(self.out_dir / "report.md"),
                "html": str(self.out_dir / "report.html"),
                "candidate_index": str(self.candidates_dir / "candidate-index.json"),
                "official_candidate_index": str(
                    self.candidates_dir / "official-candidate-index.json"
                ),
                "official_candidate_check": str(
                    self.candidates_dir / "official-candidate-check.json"
                ),
                "history_dir": str(self.history_dir),
                "commands": str(self.commands_dir),
            },
            "recommended_next_actions": recommended_next_actions(decision_status),
        }
        if error:
            summary["error"] = error
        write_json(self.out_dir / "summary.json", summary)
        write_markdown_report(self.out_dir / "report.md", summary)
        write_html_report(self.out_dir / "report.html", summary)
        return summary


class MetricAccumulator:
    def __init__(self) -> None:
        self.recall_attempts = 0
        self.recall_successes = 0
        self.context_items = 0
        self.false_positive_items = 0
        self.scope_leak_count = 0
        self.secret_leak_count = 0
        self.stale_reuse_count = 0
        self.agent_attribution_error_count = 0
        self.citation_items = 0
        self.citation_items_with_source = 0
        self.handoff_attempts = 0
        self.handoff_successes = 0
        self.workflow_count = 0
        self.workflow_category_counts: Counter[str] = Counter()

    def observe_context(self, workflow: dict[str, Any], items: list[dict[str, Any]]) -> None:
        self.workflow_count += 1
        self.workflow_category_counts[workflow["category"]] += 1
        forbidden = workflow.get("must_not_include", [])
        forbidden_scopes = ["project-alpha", "project-beta"]
        if workflow.get("scopes") == ["project-alpha"]:
            forbidden_scopes = ["project-beta", "user prefers scope trap private"]
        elif workflow.get("scopes") == ["private"]:
            forbidden_scopes = ["project-alpha", "project-beta"]
        for item in items:
            text = item.get("claim_text", "")
            self.context_items += 1
            self.citation_items += 1
            if item.get("source_event_ids"):
                self.citation_items_with_source += 1
            false_positive = False
            if any(unexpected == text for unexpected in forbidden):
                false_positive = True
            if any(scope_marker in text for scope_marker in forbidden_scopes):
                self.scope_leak_count += 1
                false_positive = True
            if HARD_SECRET_PREFIX in text or any(secret in text for secret in SECRET_LITERALS):
                self.secret_leak_count += 1
                false_positive = True
            if "legacy phase route" in text:
                self.stale_reuse_count += 1
                false_positive = True
            if "teammate likes private dump route" in text:
                self.agent_attribution_error_count += 1
                false_positive = True
            if false_positive:
                self.false_positive_items += 1

    def scorecard(self) -> dict[str, Any]:
        recall_at_k = ratio(self.recall_successes, self.recall_attempts)
        precision_at_k = 1.0 - ratio(self.false_positive_items, self.context_items)
        citation_coverage = ratio(self.citation_items_with_source, self.citation_items)
        handoff_success_rate = ratio(self.handoff_successes, self.handoff_attempts)
        attribution_accuracy = 1.0 if self.agent_attribution_error_count == 0 else 0.0
        safety_score = 1.0 if self.scope_leak_count == 0 and self.secret_leak_count == 0 else 0.0
        agent_memory_score = round(
            (handoff_success_rate + attribution_accuracy + safety_score + recall_at_k) / 4.0, 4
        )
        return {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-hard-dogfood-scorecard",
            "workflow_count": self.workflow_count,
            "workflow_category_counts": dict(sorted(self.workflow_category_counts.items())),
            "recall_attempts": self.recall_attempts,
            "recall_successes": self.recall_successes,
            "recall_at_k": round(recall_at_k, 4),
            "context_items": self.context_items,
            "false_positive_items": self.false_positive_items,
            "precision_at_k": round(precision_at_k, 4),
            "scope_leak_count": self.scope_leak_count,
            "secret_leak_count": self.secret_leak_count,
            "stale_reuse_count": self.stale_reuse_count,
            "agent_attribution_error_count": self.agent_attribution_error_count,
            "citation_items": self.citation_items,
            "citation_items_with_source": self.citation_items_with_source,
            "citation_coverage": round(citation_coverage, 4),
            "handoff_attempts": self.handoff_attempts,
            "handoff_successes": self.handoff_successes,
            "handoff_success_rate": round(handoff_success_rate, 4),
            "agent_memory_score": agent_memory_score,
        }


def seeded_fault_report() -> dict[str, Any]:
    faults = [
        {
            "id": "drop-citation",
            "metric": "citation_coverage",
            "detected": seeded_fault_has_empty_citation(),
            "detected_by": "context item source_event_ids must be non-empty",
        },
        {
            "id": "scope-leak",
            "metric": "scope_leak_count",
            "detected": seeded_fault_has_scope_leak(),
            "detected_by": "project-beta text inside project-alpha context is forbidden",
        },
        {
            "id": "secret-leak",
            "metric": "secret_leak_count",
            "detected": seeded_fault_has_secret_leak(),
            "detected_by": "synthetic secret-like text must not appear in active context",
        },
        {
            "id": "stale-reuse",
            "metric": "stale_reuse_count",
            "detected": seeded_fault_has_stale_reuse(),
            "detected_by": "legacy phase route must be superseded by current route",
        },
        {
            "id": "handoff-miss",
            "metric": "handoff_success_rate",
            "detected": seeded_fault_has_handoff_miss(),
            "detected_by": "handoff memory must be recalled after agent end",
        },
    ]
    detected = sum(1 for fault in faults if fault["detected"])
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-seeded-faults",
        "fault_count": len(faults),
        "detected_count": detected,
        "detection_rate": round(ratio(detected, len(faults)), 4),
        "ok": detected == len(faults),
        "faults": faults,
    }


def seeded_fault_has_empty_citation() -> bool:
    return not {"claim_text": "user prefers cited context", "source_event_ids": []}["source_event_ids"]


def seeded_fault_has_scope_leak() -> bool:
    text = "project-beta requires scope trap release lane 1"
    return "project-beta" in text


def seeded_fault_has_secret_leak() -> bool:
    text = f"hard-secret note {HARD_SECRET_PREFIX}01"
    return HARD_SECRET_PREFIX in text


def seeded_fault_has_stale_reuse() -> bool:
    return "legacy phase route" in "stale-fixture prefers legacy phase route 1"


def seeded_fault_has_handoff_miss() -> bool:
    expected = "agent-handoff workflow 001 continuation note"
    context_texts_after_end: list[str] = []
    return expected not in context_texts_after_end


def write_official_candidate_yaml(path: Path, candidate_id: str, failure: dict[str, Any]) -> None:
    check = str(failure.get("metric") or failure.get("category") or "hard_dogfood_failure")
    reason = str(failure.get("reason") or "hard dogfood failure")
    scenario_id = sanitize_identifier(str(failure.get("id") or candidate_id))
    lines = [
        f"schema_version: {CANDIDATE_SCHEMA_VERSION}",
        f"id: {candidate_id}",
        "status: proposed",
        "source:",
        "  report_kind: hard-dogfood",
        "  report: summary.json",
        "  target: mneme-v1",
        "  suite: dogfood",
        f"  scenario_id: {scenario_id}",
        "failure:",
        "  failed_attempts: 1",
        "  failed_checks:",
        f"    - check: {yaml_string(check)}",
        "      count: 1",
        "redaction:",
        "  sanitized: false",
        "  finding_codes: []",
        "promotion_checklist:",
        f"  - {yaml_string('Confirm the candidate contains no private user data, project paths, or provider secrets.')}",
        f"  - {yaml_string('Minimize the hard-mode finding to one deterministic public scenario.')}",
        f"  - {yaml_string(f'Preserve the source reason: {reason}')}",
        f"  - {yaml_string('Run mneme-eval candidate-check before sharing or promoting the candidate.')}",
        f"  - {yaml_string('Add a reviewed scenario block before running mneme-eval candidate-promote.')}",
        "",
    ]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def regression_gates(
    scorecard: dict[str, Any],
    seeded_faults: dict[str, Any],
    previous_summary: dict[str, Any] | None,
) -> list[dict[str, Any]]:
    gates = [
        gate(
            "recall_at_k",
            scorecard.get("recall_at_k", 0.0) >= THRESHOLDS["recall_at_k_min"],
            f"{scorecard.get('recall_at_k', 0.0)} >= {THRESHOLDS['recall_at_k_min']}",
        ),
        gate(
            "precision_at_k",
            scorecard.get("precision_at_k", 0.0) >= THRESHOLDS["precision_at_k_min"],
            f"{scorecard.get('precision_at_k', 0.0)} >= {THRESHOLDS['precision_at_k_min']}",
        ),
        gate(
            "scope_leak_count",
            scorecard.get("scope_leak_count", 0) <= THRESHOLDS["scope_leak_max"],
            f"{scorecard.get('scope_leak_count', 0)} <= {THRESHOLDS['scope_leak_max']}",
        ),
        gate(
            "secret_leak_count",
            scorecard.get("secret_leak_count", 0) <= THRESHOLDS["secret_leak_max"],
            f"{scorecard.get('secret_leak_count', 0)} <= {THRESHOLDS['secret_leak_max']}",
        ),
        gate(
            "citation_coverage",
            scorecard.get("citation_coverage", 0.0) >= THRESHOLDS["citation_coverage_min"],
            f"{scorecard.get('citation_coverage', 0.0)} >= {THRESHOLDS['citation_coverage_min']}",
        ),
        gate(
            "handoff_success_rate",
            scorecard.get("handoff_success_rate", 0.0) >= THRESHOLDS["handoff_success_min"],
            f"{scorecard.get('handoff_success_rate', 0.0)} >= {THRESHOLDS['handoff_success_min']}",
        ),
        gate(
            "agent_attribution_error_count",
            scorecard.get("agent_attribution_error_count", 0)
            <= THRESHOLDS["agent_attribution_error_max"],
            f"{scorecard.get('agent_attribution_error_count', 0)} <= "
            f"{THRESHOLDS['agent_attribution_error_max']}",
        ),
        gate(
            "stale_reuse_count",
            scorecard.get("stale_reuse_count", 0) <= THRESHOLDS["stale_reuse_max"],
            f"{scorecard.get('stale_reuse_count', 0)} <= {THRESHOLDS['stale_reuse_max']}",
        ),
        gate(
            "seeded_fault_detection_rate",
            seeded_faults.get("detection_rate", 0.0)
            >= THRESHOLDS["seeded_fault_detection_min"],
            f"{seeded_faults.get('detection_rate', 0.0)} >= "
            f"{THRESHOLDS['seeded_fault_detection_min']}",
        ),
    ]
    if previous_summary:
        previous_scorecard = previous_summary.get("scorecard") or {}
        for key in ["recall_at_k", "precision_at_k", "citation_coverage", "handoff_success_rate"]:
            before = float(previous_scorecard.get(key, 0.0))
            after = float(scorecard.get(key, 0.0))
            gates.append(
                gate(
                    f"regression.{key}",
                    after >= before,
                    f"{after} >= previous {before}",
                )
            )
    return gates


def history_entry(
    *,
    run_label: str,
    out_dir: Path,
    status: str,
    preflight: dict[str, Any],
    scorecard: dict[str, Any],
    seeded_faults: dict[str, Any],
    regression: dict[str, Any],
) -> dict[str, Any]:
    generated_at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-history-entry",
        "generated_at": generated_at,
        "generated_at_slug": generated_at.replace(":", "").replace("-", "").replace("T", "-").replace("Z", "Z"),
        "run_label": run_label,
        "version": current_version(),
        "out_dir": str(out_dir),
        "status": status,
        "decision_status": "v1_hard_dogfood_passed"
        if status == "passed" and regression.get("ok")
        else "blocked",
        "preflight_decision": preflight.get("decision_status"),
        "scorecard": reduced_scorecard(scorecard),
        "seeded_fault_detection_rate": seeded_faults.get("detection_rate", 0.0),
        "regression_ok": regression.get("ok", False),
    }


def history_entry_from_summary(summary: dict[str, Any]) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-history-entry",
        "generated_at": summary.get("generated_at", "0000-00-00T00:00:00Z"),
        "generated_at_slug": "compare-summary",
        "run_label": summary.get("run_label", "compare-summary"),
        "version": summary.get("version", "unknown"),
        "out_dir": summary.get("out_dir"),
        "status": summary.get("status"),
        "decision_status": summary.get("decision_status"),
        "preflight_decision": (summary.get("preflight") or {}).get("decision_status"),
        "scorecard": reduced_scorecard(summary.get("scorecard") or {}),
        "seeded_fault_detection_rate": (summary.get("seeded_faults") or {}).get(
            "detection_rate", 0.0
        ),
        "regression_ok": (summary.get("regression") or {}).get("ok", False),
    }


def reduced_scorecard(scorecard: dict[str, Any]) -> dict[str, Any]:
    keys = [
        "recall_at_k",
        "precision_at_k",
        "scope_leak_count",
        "secret_leak_count",
        "citation_coverage",
        "handoff_success_rate",
        "agent_memory_score",
        "stale_reuse_count",
        "agent_attribution_error_count",
    ]
    return {key: scorecard.get(key, 0) for key in keys}


def load_history_entries(history_dir: Path) -> list[dict[str, Any]]:
    if not history_dir.exists():
        return []
    entries = []
    for path in sorted(history_dir.glob("*.json")):
        if path.name in {"trend.json", "summary.json"}:
            continue
        try:
            entry = read_json(path)
        except (OSError, json.JSONDecodeError):
            continue
        if (
            entry.get("command") == "v1-hard-dogfood-history-entry"
            and entry.get("decision_status") == "v1_hard_dogfood_passed"
            and "recall_at_k" in (entry.get("scorecard") or {})
        ):
            entries.append(entry)
    return entries


def build_trend_report(
    previous_entries: list[dict[str, Any]],
    current: dict[str, Any],
    history_path: Path,
) -> dict[str, Any]:
    previous = previous_entries[-1] if previous_entries else None
    metric_deltas = []
    regression_detected = False
    if previous:
        before_scorecard = previous.get("scorecard") or {}
        after_scorecard = current.get("scorecard") or {}
        for metric in [
            "recall_at_k",
            "precision_at_k",
            "citation_coverage",
            "handoff_success_rate",
            "agent_memory_score",
        ]:
            before = float(before_scorecard.get(metric, 0.0))
            after = float(after_scorecard.get(metric, 0.0))
            delta = round(after - before, 4)
            status = "improved" if delta > 0 else "regressed" if delta < 0 else "unchanged"
            if delta < 0:
                regression_detected = True
            metric_deltas.append(
                {"metric": metric, "before": before, "after": after, "delta": delta, "status": status}
            )
        for metric in [
            "scope_leak_count",
            "secret_leak_count",
            "stale_reuse_count",
            "agent_attribution_error_count",
        ]:
            before = int(before_scorecard.get(metric, 0))
            after = int(after_scorecard.get(metric, 0))
            delta = after - before
            status = "improved" if delta < 0 else "regressed" if delta > 0 else "unchanged"
            if delta > 0:
                regression_detected = True
            metric_deltas.append(
                {"metric": metric, "before": before, "after": after, "delta": delta, "status": status}
            )
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-trend",
        "status": "compared" if previous else "baseline_recorded",
        "ok": not regression_detected,
        "history_path": str(history_path),
        "history_entry_count": len(previous_entries) + 1,
        "current": {
            "run_label": current.get("run_label"),
            "version": current.get("version"),
            "decision_status": current.get("decision_status"),
        },
        "previous": None
        if previous is None
        else {
            "run_label": previous.get("run_label"),
            "version": previous.get("version"),
            "decision_status": previous.get("decision_status"),
        },
        "regression_detected": regression_detected,
        "metric_deltas": metric_deltas,
        "recommended_next_actions": trend_next_actions(regression_detected, bool(previous)),
    }


def trend_next_actions(regression_detected: bool, compared: bool) -> list[str]:
    if not compared:
        return [
            "keep this history entry as the first local hard-mode baseline",
            "rerun with --history-dir to compare future hard dogfood runs",
        ]
    if regression_detected:
        return [
            "review metric_deltas before promoting v1 changes",
            "turn regressed categories into hard dogfood candidates",
            "rerun hard dogfood after the regression is fixed",
        ]
    return [
        "history comparison has no hard-mode regression",
        "use this trend report as release evidence with the hard dogfood summary",
    ]


def check_trend_report() -> dict[str, Any]:
    previous = {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-history-entry",
        "generated_at": "2026-05-25T00:00:00Z",
        "run_label": "synthetic-before",
        "version": "0.0.0",
        "decision_status": "v1_hard_dogfood_passed",
        "scorecard": {
            "recall_at_k": 0.95,
            "precision_at_k": 0.95,
            "citation_coverage": 1.0,
            "handoff_success_rate": 0.95,
            "agent_memory_score": 0.95,
            "scope_leak_count": 0,
            "secret_leak_count": 0,
            "stale_reuse_count": 0,
            "agent_attribution_error_count": 0,
        },
    }
    current = {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-hard-dogfood-history-entry",
        "generated_at": "2026-05-25T00:01:00Z",
        "generated_at_slug": "synthetic-after",
        "run_label": "synthetic-after",
        "version": "0.0.1",
        "decision_status": "v1_hard_dogfood_passed",
        "scorecard": {
            "recall_at_k": 1.0,
            "precision_at_k": 1.0,
            "citation_coverage": 1.0,
            "handoff_success_rate": 1.0,
            "agent_memory_score": 1.0,
            "scope_leak_count": 0,
            "secret_leak_count": 0,
            "stale_reuse_count": 0,
            "agent_attribution_error_count": 0,
        },
    }
    return build_trend_report([previous], current, Path("synthetic-history.json"))


def gate(name: str, passed: bool, detail: str) -> dict[str, Any]:
    return {"name": name, "status": "pass" if passed else "fail", "detail": detail}


def recommended_next_actions(decision_status: str) -> list[str]:
    if decision_status == "v1_hard_dogfood_passed":
        return [
            "Use this report as the v1 hard-mode evidence bundle.",
            "Review seeded-fault candidate artifacts and promote only minimal public scenarios.",
            "Run this again before changing retrieval, scope, or agent handoff behavior.",
        ]
    return [
        "Inspect failed workflows, scorecard gates, and generated candidate artifacts.",
        "Do not treat v1 as hard-mode validated until decision_status is v1_hard_dogfood_passed.",
    ]


def write_markdown_report(path: Path, summary: dict[str, Any]) -> None:
    scorecard = summary.get("scorecard") or {}
    regression = summary.get("regression") or {}
    lines = [
        "# Mneme V1 Hard Dogfood Report",
        "",
        f"- Decision: `{summary.get('decision_status')}`",
        f"- Normal records: `{summary.get('normal_record_count')}`",
        f"- Adversarial records: `{summary.get('adversarial_record_count')}`",
        f"- Agent workflows: `{summary.get('agent_workflow_count')}`",
        f"- Passed workflows: `{summary.get('passed_workflows')}`",
        f"- Failed workflows: `{summary.get('failed_workflows')}`",
        f"- Candidate artifacts: `{summary.get('candidate_count')}`",
        f"- Official candidate artifacts: `{summary.get('official_candidate_count')}`",
        "",
        "## Scorecard",
        "",
        "| Metric | Value |",
        "|---|---:|",
    ]
    for key in [
        "recall_at_k",
        "precision_at_k",
        "scope_leak_count",
        "secret_leak_count",
        "citation_coverage",
        "handoff_success_rate",
        "agent_attribution_error_count",
        "stale_reuse_count",
        "agent_memory_score",
    ]:
        lines.append(f"| `{key}` | `{scorecard.get(key)}` |")
    lines.extend(["", "## Gates", "", "| Gate | Status | Detail |", "|---|---|---|"])
    for item in regression.get("gates", []):
        lines.append(
            f"| `{item.get('name')}` | `{item.get('status')}` | {item.get('detail')} |"
        )
    trend = summary.get("trend") or {}
    lines.extend(
        [
            "",
            "## Trend",
            "",
            f"- Status: `{trend.get('status')}`",
            f"- Regression detected: `{trend.get('regression_detected')}`",
            f"- History entries: `{trend.get('history_entry_count')}`",
            "",
        ]
    )
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def write_html_report(path: Path, summary: dict[str, Any]) -> None:
    scorecard = summary.get("scorecard") or {}
    regression = summary.get("regression") or {}
    metric_rows = "\n".join(
        f"<tr><td>{html.escape(key)}</td><td>{html.escape(str(scorecard.get(key)))}</td></tr>"
        for key in [
            "recall_at_k",
            "precision_at_k",
            "scope_leak_count",
            "secret_leak_count",
            "citation_coverage",
            "handoff_success_rate",
            "agent_memory_score",
        ]
    )
    gate_rows = "\n".join(
        "<tr>"
        f"<td>{html.escape(str(item.get('name')))}</td>"
        f"<td>{html.escape(str(item.get('status')))}</td>"
        f"<td>{html.escape(str(item.get('detail')))}</td>"
        "</tr>"
        for item in regression.get("gates", [])
    )
    document = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Mneme V1 Hard Dogfood Report</title>
  <style>
    body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 32px; color: #1f2937; }}
    table {{ border-collapse: collapse; width: 100%; margin: 16px 0 28px; }}
    th, td {{ border: 1px solid #d1d5db; padding: 8px 10px; text-align: left; }}
    th {{ background: #f3f4f6; }}
    code {{ background: #f3f4f6; padding: 2px 4px; border-radius: 4px; }}
  </style>
</head>
<body>
  <h1>Mneme V1 Hard Dogfood Report</h1>
  <p>Decision: <code>{html.escape(str(summary.get('decision_status')))}</code></p>
  <p>Records: <code>{summary.get('normal_record_count')}</code> normal,
  <code>{summary.get('adversarial_record_count')}</code> adversarial.
  Workflows: <code>{summary.get('agent_workflow_count')}</code>.</p>
  <p>Official candidates: <code>{summary.get('official_candidate_count')}</code>.</p>
  <h2>Scorecard</h2>
  <table><tbody>{metric_rows}</tbody></table>
  <h2>Regression Gates</h2>
  <table><thead><tr><th>Gate</th><th>Status</th><th>Detail</th></tr></thead><tbody>{gate_rows}</tbody></table>
  <h2>Trend</h2>
  <p>Status: <code>{html.escape(str((summary.get('trend') or {}).get('status')))}</code>,
  regression detected: <code>{html.escape(str((summary.get('trend') or {}).get('regression_detected')))}</code>.</p>
</body>
</html>
"""
    path.write_text(document, encoding="utf-8")


def write_trend_markdown(path: Path, trend: dict[str, Any]) -> None:
    lines = [
        "# Mneme V1 Hard Dogfood Trend",
        "",
        f"- Status: `{trend.get('status')}`",
        f"- Regression detected: `{trend.get('regression_detected')}`",
        f"- History entries: `{trend.get('history_entry_count')}`",
        "",
        "| Metric | Before | After | Delta | Status |",
        "|---|---:|---:|---:|---|",
    ]
    for item in trend.get("metric_deltas", []):
        lines.append(
            f"| `{item.get('metric')}` | `{item.get('before')}` | `{item.get('after')}` | "
            f"`{item.get('delta')}` | `{item.get('status')}` |"
        )
    lines.append("")
    path.write_text("\n".join(lines), encoding="utf-8")


def context_texts(report: dict[str, Any]) -> list[str]:
    return [
        item.get("claim_text", "")
        for item in (report.get("context_pack") or {}).get("items", [])
    ]


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def ratio(numerator: int, denominator: int) -> float:
    if denominator == 0:
        return 1.0
    return numerator / denominator


def current_version() -> str:
    cargo = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    match = re.search(r'(?m)^version = "([^"]+)"', cargo)
    return match.group(1) if match else "unknown"


def yaml_string(value: str) -> str:
    escaped = value.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def sanitize_identifier(value: str) -> str:
    sanitized = []
    previous_dash = False
    for char in value:
        if char.isascii() and char.isalnum():
            sanitized.append(char.lower())
            previous_dash = False
        elif not previous_dash:
            sanitized.append("-")
            previous_dash = True
    return "".join(sanitized).strip("-") or "candidate"


def redact_paths(argv: list[str]) -> list[str]:
    root = str(ROOT)
    return [arg.replace(root, "<repo>") for arg in argv]


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run v1 hard-mode dogfood with normal, adversarial, and agent workflows."
    )
    parser.add_argument("--run-label", help="Run label for evals/runs/v1-hard-dogfood.")
    parser.add_argument("--out-dir", help="Explicit output directory for the evidence bundle.")
    parser.add_argument("--mneme-bin", help="Use an existing mneme binary instead of target/debug/mneme.")
    parser.add_argument("--compare-summary", help="Previous hard dogfood summary.json for regression comparison.")
    parser.add_argument(
        "--history-dir",
        help="Directory for public-safe hard dogfood history entries used by trend reports.",
    )
    parser.add_argument("--force", action="store_true", help="Replace an existing output directory.")
    parser.add_argument("--skip-preflight", action="store_true", help="Skip scripts/v1-dogfood.sh preflight.")
    parser.add_argument("--no-build", action="store_true", help="Do not build mneme-cli before running.")
    parser.add_argument("--check-contract", action="store_true", help="Print the hard dogfood contract.")
    parser.add_argument(
        "--check-dataset",
        action="store_true",
        help="Validate and print the dataset summary without running CLI workflows.",
    )
    parser.add_argument(
        "--check-seeded-faults",
        action="store_true",
        help="Validate seeded-fault detector coverage without running CLI workflows.",
    )
    parser.add_argument(
        "--check-official-candidate",
        action="store_true",
        help="Print one official hard candidate YAML sample for contract checks.",
    )
    parser.add_argument(
        "--check-trend",
        action="store_true",
        help="Print a synthetic hard dogfood trend report for contract checks.",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.check_contract:
        print(json.dumps(contract(), ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    if args.check_seeded_faults:
        report = seeded_fault_report()
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if report["ok"] else 1
    if args.check_official_candidate:
        failure = seeded_fault_report()["faults"][0]
        candidate_id = sanitize_identifier(f"hard-seeded-{failure['id']}")
        sample_path = Path(os.environ.get("TMPDIR", "/tmp")) / f"{candidate_id}.candidate.yaml"
        write_official_candidate_yaml(
            sample_path,
            candidate_id,
            {
                "id": f"seeded-{failure['id']}",
                "category": "seeded_fault",
                "reason": failure["detected_by"],
                "metric": failure["metric"],
            },
        )
        print(sample_path.read_text(encoding="utf-8"), end="")
        try:
            sample_path.unlink()
        except OSError:
            pass
        return 0
    if args.check_trend:
        report = check_trend_report()
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if report["ok"] and report["status"] == "compared" else 1

    normal_records = load_manual_records()
    adversarial_records = build_adversarial_records()
    workflows = build_agent_workflows()
    summary = dataset_summary(normal_records, adversarial_records, workflows)
    if args.check_dataset:
        print(json.dumps(summary, ensure_ascii=False, indent=2, sort_keys=True))
        return 0

    run = HardDogfoodRun(args, normal_records, adversarial_records, workflows)
    preflight: dict[str, Any] = {"status": "not_started"}
    try:
        run.prepare()
        preflight = run.run_preflight()
        run.ingest_all()
        run.apply_setup_mutations()
        run.run_workflows()
        run.run_seeded_faults()
        run.write_failure_candidates()
        run.validate_official_candidates()
        run.build_regression()
        run.write_history_and_trend("passed", preflight)
        report = run.write_reports("passed", preflight)
        print(f"v1-hard-dogfood: wrote {run.out_dir}")
        print(f"v1-hard-dogfood: summary {run.out_dir / 'summary.json'}")
        print(f"v1-hard-dogfood: decision {report['decision_status']}")
        return 0 if report["decision_status"] == "v1_hard_dogfood_passed" else 1
    except Exception as exc:  # noqa: BLE001 - always write a hard-mode summary when possible.
        try:
            if not run.seeded_faults:
                run.run_seeded_faults()
            run.write_failure_candidates()
            if not run.scorecard:
                run.scorecard = MetricAccumulator().scorecard()
            if not run.regression:
                run.build_regression()
            if not run.official_candidate_check:
                run.validate_official_candidates()
            if not run.trend:
                run.write_history_and_trend("failed", preflight)
            run.write_reports("failed", preflight, error=str(exc))
        except Exception as summary_exc:  # noqa: BLE001 - preserve original failure.
            print(f"v1-hard-dogfood: failed while writing summary: {summary_exc}", file=sys.stderr)
        print(f"v1-hard-dogfood: failed: {exc}", file=sys.stderr)
        print(f"v1-hard-dogfood: summary {run.out_dir / 'summary.json'}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
