#!/usr/bin/env python3
"""Run the Mneme v1 manual dogfood protocol with synthetic public data."""

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
EXPECTED_RECORD_COUNT = 100
EXPECTED_WORKFLOW_COUNT = 25
RUN_LABEL_RE = re.compile(r"^[A-Za-z0-9._/-]+$")
SECRET_LITERALS = [
    "API_KEY=FAKE_TEST_VALUE",
    "TOKEN=FAKE_TOKEN_VALUE",
    "PASSWORD=FAKE_PASSWORD_VALUE",
    "ACCESS_TOKEN=FAKE_ACCESS_TOKEN_VALUE",
]


class DogfoodFailure(RuntimeError):
    """Raised when a manual dogfood check fails."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


def build_mock_records() -> list[dict[str, Any]]:
    records: list[dict[str, Any]] = []

    def add(
        category: str,
        scope: str,
        text: str,
        expected_status: str = "active",
        alias: str | None = None,
    ) -> None:
        records.append(
            {
                "id": f"mock-{len(records) + 1:03d}",
                "category": category,
                "scope": scope,
                "text": text,
                "expected_status": expected_status,
                "alias": alias,
            }
        )

    for text in [
        "user prefers concise implementation briefs",
        "user prefers phase-sized development summaries",
        "user prefers direct technical tradeoffs",
        "user prefers public-safe release notes",
        "user prefers local-first memory workflows",
        "user prefers deterministic evaluation gates",
        "user prefers markdown checklists for reviews",
        "user prefers Korean user-facing summaries",
        "user prefers CLI-first automation",
        "user prefers release evidence before promotion",
        "user prefers stable naming over clever names",
        "user prefers low-noise dashboards",
        "user prefers exact dates in status updates",
        "user prefers scoped memory recall",
        "user prefers rollback commands documented",
        "user prefers small public README examples",
        "user prefers automated PR merge after checks",
        "user prefers dogfood artifacts with JSON summaries",
    ]:
        add("personal_preference", "private", text)

    for text in [
        "project-alpha requires PR before release",
        "project-alpha requires main merge after green checks",
        "project-alpha requires annotated version tags",
        "project-alpha requires changelog entries per release",
        "project-alpha prefers eval evidence before implementation phases",
        "project-alpha prefers candidate promotion after triage",
        "project-alpha requires public-safe docs only",
        "project-alpha prefers deterministic local scripts",
        "project-alpha requires release workflow verification",
        "project-alpha prefers phase briefings after merge",
        "project-alpha requires ignored run artifacts",
        "project-alpha prefers cost-aware CI triggers",
        "project-alpha requires no private template files",
        "project-alpha prefers v1 dogfood before v2 planning",
    ]:
        add("project_alpha", "project-alpha", text)

    for text in [
        "project-beta requires team memory review",
        "project-beta prefers workspace-scoped context",
        "project-beta requires member-visible audit summaries",
        "project-beta prefers role-based recall policies",
        "project-beta requires shared release notes",
        "project-beta prefers team onboarding examples",
        "project-beta requires permission-aware retrieval",
        "project-beta prefers project-level quality dashboards",
        "project-beta requires reviewable memory diffs",
        "project-beta prefers opt-in shared memory",
    ]:
        add("project_beta", "project-beta", text)

    for text in [
        "mneme-cli supports isolated store paths",
        "mneme-cli supports JSON command reports",
        "mneme-cli supports doctor health checks",
        "mneme-cli supports safe review artifacts",
        "mneme-cli supports guided curation plans",
        "mneme-cli supports restore check mode",
        "mneme-cli supports explicit claim IDs",
        "mneme-cli supports scoped context queries",
        "mneme-cli supports agent hook envelopes",
        "mneme-cli supports local install smoke checks",
    ]:
        add("tooling_contract", "private", text)

    for text in [
        "release-flow requires local quality gate before PR",
        "release-flow requires PR checks before merge",
        "release-flow requires main CI before tag",
        "release-flow requires annotated tags",
        "release-flow requires GitHub release verification",
        "release-flow prefers prerelease tags before v1",
        "release-flow requires changelog updates",
        "release-flow requires public safety checks",
        "release-flow prefers cost-aware automation",
        "release-flow requires evidence links in final briefs",
    ]:
        add("release_quality", "private", text)

    for text in [
        "agent-workflow prefers begin context before implementation",
        "agent-workflow prefers end summaries after implementation",
        "agent-workflow requires remembered decisions to cite sessions",
        "agent-workflow prefers hook doctor before installation",
        "agent-workflow requires extractor checks to be explicit",
        "agent-workflow prefers command-backed extraction only when opted in",
        "agent-workflow requires recoverable lock errors",
        "agent-workflow prefers scoped task context",
        "agent-workflow requires stable JSON envelopes",
        "agent-workflow prefers no provider calls during routine diagnostics",
    ]:
        add("agent_workflow", "private", text)

    for text in [
        "ranking-fixture prefers launch review checklists",
        "ranking-fixture prefers launch templates",
        "ranking-fixture prefers review summaries",
        "ranking-fixture prefers color palettes",
        "ranking-fixture prefers launch retrospectives",
        "ranking-fixture prefers release briefs",
        "ranking-fixture prefers issue triage notes",
        "ranking-fixture prefers roadmap snapshots",
        "ranking-fixture prefers planning memos",
        "ranking-fixture prefers archive notes",
    ]:
        add("ranking_context", "private", text)

    add(
        "correction_seed",
        "private",
        "user prefers legacy naming for dogfood",
        alias="legacy_dogfood_naming",
    )
    add(
        "correction_seed",
        "private",
        "user prefers outdated release checklist wording",
        alias="outdated_release_checklist",
    )
    add(
        "correction_seed",
        "private",
        "user prefers old agent summary format",
        alias="old_agent_summary_format",
    )
    add(
        "correction_seed",
        "project-alpha",
        "project-alpha requires legacy tag naming",
        alias="legacy_tag_naming",
    )
    add(
        "correction_seed",
        "project-beta",
        "project-beta prefers old review routing",
        alias="old_review_routing",
    )

    add(
        "forget_seed",
        "private",
        "user prefers temporary launch color notes",
        alias="temporary_launch_color_notes",
    )
    add(
        "forget_seed",
        "private",
        "user prefers one-off weekend planning notes",
        alias="one_off_weekend_planning",
    )
    add(
        "forget_seed",
        "project-alpha",
        "project-alpha prefers temporary demo notes",
        alias="temporary_demo_notes",
    )
    add(
        "forget_seed",
        "project-beta",
        "project-beta prefers obsolete onboarding notes",
        alias="obsolete_onboarding_notes",
    )
    add(
        "forget_seed",
        "private",
        "user prefers scratch migration notes",
        alias="scratch_migration_notes",
    )

    add("duplicate_seed", "private", "user prefers duplicate dogfood review loops")
    add("duplicate_seed", "private", "user prefers duplicate dogfood review loops")
    add("duplicate_seed", "private", "user prefers duplicate release checklists")
    add("duplicate_seed", "private", "user prefers duplicate release checklists")

    add(
        "secret_like",
        "private",
        "user note API_KEY=FAKE_TEST_VALUE",
        expected_status="blocked_secret",
    )
    add(
        "secret_like",
        "private",
        "user token TOKEN=FAKE_TOKEN_VALUE",
        expected_status="blocked_secret",
    )
    add(
        "secret_like",
        "private",
        "user password PASSWORD=FAKE_PASSWORD_VALUE",
        expected_status="blocked_secret",
    )
    add(
        "secret_like",
        "private",
        "user access_token ACCESS_TOKEN=FAKE_ACCESS_TOKEN_VALUE",
        expected_status="blocked_secret",
    )

    if len(records) != EXPECTED_RECORD_COUNT:
        raise AssertionError(f"expected {EXPECTED_RECORD_COUNT} records, got {len(records)}")
    return records


WORKFLOW_DEFINITIONS = [
    "personal-context-release-evidence",
    "personal-context-korean-summary",
    "project-alpha-allowed-scope",
    "project-alpha-denied-private-scope",
    "context-ranking-max-items",
    "correct-by-claim-id",
    "corrected-memory-recall",
    "corrected-old-memory-inactive",
    "forget-by-claim-id",
    "forgotten-memory-not-recalled",
    "quality-detects-review-queue",
    "safe-review-redaction",
    "curation-dry-run-plan",
    "curation-apply-compact",
    "post-curation-quality-ok",
    "restore-check-available",
    "restore-and-swap-back",
    "export-curated-store",
    "import-curated-store",
    "imported-store-recall",
    "agent-begin-private-context",
    "agent-end-remembers-summary",
    "hook-doctor-json-envelope",
    "hook-begin-project-context",
    "hook-end-remembers-summary",
]


def dataset_summary(records: list[dict[str, Any]]) -> dict[str, Any]:
    categories = Counter(record["category"] for record in records)
    scopes = Counter(record["scope"] for record in records)
    statuses = Counter(record["expected_status"] for record in records)
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-manual-dogfood-dataset",
        "mock_record_count": len(records),
        "workflow_count": len(WORKFLOW_DEFINITIONS),
        "categories": dict(sorted(categories.items())),
        "scopes": dict(sorted(scopes.items())),
        "expected_statuses": dict(sorted(statuses.items())),
        "workflows": WORKFLOW_DEFINITIONS,
    }


class DogfoodRun:
    def __init__(self, args: argparse.Namespace, records: list[dict[str, Any]]) -> None:
        self.args = args
        self.records = records
        self.run_label = args.run_label or time.strftime("local-%Y%m%d-%H%M%S")
        if not RUN_LABEL_RE.match(self.run_label):
            raise DogfoodFailure(
                "run label may contain only letters, digits, '-', '_', '.', or '/'"
            )
        self.out_dir = (
            Path(args.out_dir)
            if args.out_dir
            else ROOT / "evals" / "runs" / "v1-manual-dogfood" / self.run_label
        )
        self.workspace_dir = self.out_dir / "workspace"
        self.store = self.workspace_dir / ".mneme" / "mneme-v1.json"
        self.config = self.workspace_dir / ".mneme" / "mneme-agent-hook.env"
        self.commands_dir = self.out_dir / "commands"
        self.reports_dir = self.out_dir / "reports"
        self.command_index = 0
        self.command_artifacts: list[dict[str, Any]] = []
        self.claim_id_by_record_id: dict[str, str] = {}
        self.claim_id_by_alias: dict[str, str] = {}
        self.workflow_results: list[dict[str, Any]] = []
        self.preflight: dict[str, Any] = {"status": "skipped"}
        self.mneme_bin = Path(args.mneme_bin) if args.mneme_bin else ROOT / "target/debug/mneme"

    def prepare(self) -> None:
        if self.out_dir.exists():
            if not self.args.force:
                raise DogfoodFailure(
                    f"output directory already exists: {self.out_dir}; use --force"
                )
            shutil.rmtree(self.out_dir)
        self.commands_dir.mkdir(parents=True)
        self.reports_dir.mkdir(parents=True)
        self.workspace_dir.mkdir(parents=True)
        write_json(self.out_dir / "mock-dataset.json", dataset_summary(self.records))
        write_json(self.out_dir / "mock-records.json", {"records": self.records})
        write_json(
            self.out_dir / "workflow-plan.json",
            {
                "schema_version": SCHEMA_VERSION,
                "workflow_count": len(WORKFLOW_DEFINITIONS),
                "workflows": WORKFLOW_DEFINITIONS,
            },
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
        artifact = self.commands_dir / f"{slug}.stdout.txt"
        artifact.write_text(result.stdout, encoding="utf-8")
        stderr_artifact = self.commands_dir / f"{slug}.stderr.txt"
        stderr_artifact.write_text(result.stderr, encoding="utf-8")
        self.command_artifacts.append(
            {
                "name": slug,
                "argv": command,
                "returncode": result.returncode,
                "stdout": str(artifact),
                "stderr": str(stderr_artifact),
            }
        )
        if result.returncode != 0:
            raise DogfoodFailure(f"{slug} failed with exit {result.returncode}: {result.stderr}")

    def run_preflight(self) -> None:
        if self.args.skip_preflight:
            self.preflight = {"status": "skipped"}
            return
        preflight_dir = self.out_dir / "preflight"
        env = os.environ.copy()
        env["MNEME_DOGFOOD_RUN_LABEL"] = f"{self.run_label}-preflight"
        env["MNEME_DOGFOOD_OUT_DIR"] = str(preflight_dir)
        self.run_external("preflight-v1-dogfood", [str(ROOT / "scripts/v1-dogfood.sh")], env)
        summary_path = preflight_dir / "dogfood-summary.json"
        summary = read_json(summary_path)
        decision = summary.get("decision_status")
        if decision != "ready_for_manual_dogfood":
            raise DogfoodFailure(f"preflight dogfood summary is not ready: {decision}")
        self.preflight = {
            "status": "passed",
            "out_dir": str(preflight_dir),
            "dogfood_summary": str(summary_path),
            "decision_status": decision,
        }

    def run_command(
        self,
        slug: str,
        args: list[str],
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
            raise DogfoodFailure(f"{slug} failed with exit {result.returncode}: {result.stderr}")
        if parse_json:
            try:
                return json.loads(result.stdout)
            except json.JSONDecodeError as exc:
                raise DogfoodFailure(f"{slug} did not emit JSON: {exc}") from exc
        return result.stdout

    def ingest_records(self) -> list[dict[str, Any]]:
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
        ingested: list[dict[str, Any]] = []
        for record in self.records:
            report = self.run_command(
                f"remember-{record['id']}",
                [
                    "remember",
                    record["text"],
                    "--scope",
                    record["scope"],
                    "--json",
                ],
                store=self.store,
            )
            latest = report.get("latest_claim") or {}
            claim_id = latest.get("id")
            if not claim_id:
                raise DogfoodFailure(f"{record['id']} did not create a claim")
            if latest.get("status") != record["expected_status"]:
                raise DogfoodFailure(
                    f"{record['id']} expected status {record['expected_status']}, "
                    f"got {latest.get('status')}"
                )
            if latest.get("scope") != record["scope"]:
                raise DogfoodFailure(
                    f"{record['id']} expected scope {record['scope']}, got {latest.get('scope')}"
                )
            self.claim_id_by_record_id[record["id"]] = claim_id
            if record.get("alias"):
                self.claim_id_by_alias[record["alias"]] = claim_id
            ingested.append(
                {
                    "record_id": record["id"],
                    "category": record["category"],
                    "scope": record["scope"],
                    "claim_id": claim_id,
                    "status": latest.get("status"),
                }
            )
        write_json(self.reports_dir / "ingested-records.json", {"records": ingested})
        return ingested

    def run_workflows(self) -> None:
        workflow_functions = [
            self.workflow_personal_context_release_evidence,
            self.workflow_personal_context_korean_summary,
            self.workflow_project_alpha_allowed_scope,
            self.workflow_project_alpha_denied_private_scope,
            self.workflow_context_ranking_max_items,
            self.workflow_correct_by_claim_id,
            self.workflow_corrected_memory_recall,
            self.workflow_corrected_old_memory_inactive,
            self.workflow_forget_by_claim_id,
            self.workflow_forgotten_memory_not_recalled,
            self.workflow_quality_detects_review_queue,
            self.workflow_safe_review_redaction,
            self.workflow_curation_dry_run_plan,
            self.workflow_curation_apply_compact,
            self.workflow_post_curation_quality_ok,
            self.workflow_restore_check_available,
            self.workflow_restore_and_swap_back,
            self.workflow_export_curated_store,
            self.workflow_import_curated_store,
            self.workflow_imported_store_recall,
            self.workflow_agent_begin_private_context,
            self.workflow_agent_end_remembers_summary,
            self.workflow_hook_doctor_json_envelope,
            self.workflow_hook_begin_project_context,
            self.workflow_hook_end_remembers_summary,
        ]
        if len(workflow_functions) != EXPECTED_WORKFLOW_COUNT:
            raise AssertionError(
                f"expected {EXPECTED_WORKFLOW_COUNT} workflows, got {len(workflow_functions)}"
            )
        for index, function in enumerate(workflow_functions, start=1):
            name = WORKFLOW_DEFINITIONS[index - 1]
            checks: list[str] = []
            try:
                function(checks)
                result = {
                    "index": index,
                    "name": name,
                    "status": "passed",
                    "checks": checks,
                }
            except Exception as exc:  # noqa: BLE001 - report dogfood failures uniformly.
                result = {
                    "index": index,
                    "name": name,
                    "status": "failed",
                    "checks": checks,
                    "error": str(exc),
                }
                self.workflow_results.append(result)
                write_json(self.reports_dir / "workflow-results.json", {"workflows": self.workflow_results})
                raise DogfoodFailure(f"workflow {index} {name} failed: {exc}") from exc
            self.workflow_results.append(result)
        write_json(self.reports_dir / "workflow-results.json", {"workflows": self.workflow_results})

    def workflow_personal_context_release_evidence(self, checks: list[str]) -> None:
        report = self.context("release evidence", "context-release-evidence")
        require_context_contains(report, "user prefers release evidence before promotion")
        checks.append("release evidence preference recalled")

    def workflow_personal_context_korean_summary(self, checks: list[str]) -> None:
        report = self.context("Korean summaries", "context-korean-summary")
        require_context_contains(report, "user prefers Korean user-facing summaries")
        checks.append("Korean summary preference recalled")

    def workflow_project_alpha_allowed_scope(self, checks: list[str]) -> None:
        report = self.context(
            "PR release",
            "context-project-alpha-allowed",
            scopes=["project-alpha"],
        )
        require_context_contains(report, "project-alpha requires PR before release")
        checks.append("project-alpha scoped memory recalled when allowed")

    def workflow_project_alpha_denied_private_scope(self, checks: list[str]) -> None:
        report = self.context(
            "PR release",
            "context-project-alpha-denied",
            scopes=["private"],
        )
        require_context_absent(report, "project-alpha requires PR before release")
        if not any(
            item.get("reason") == "scope_denied:project-alpha"
            for item in report["context_pack"].get("omitted", [])
        ):
            raise DogfoodFailure("project-alpha denial did not appear in omitted reasons")
        checks.append("project-alpha memory omitted under private-only scope")

    def workflow_context_ranking_max_items(self, checks: list[str]) -> None:
        report = self.context(
            "launch review",
            "context-ranking-max-items",
            max_items=3,
        )
        if report.get("item_count") != 3:
            raise DogfoodFailure(f"expected exactly 3 context items, got {report.get('item_count')}")
        require_context_contains(report, "ranking-fixture prefers launch review checklists")
        checks.append("context ranking honors max-items cap")

    def workflow_correct_by_claim_id(self, checks: list[str]) -> None:
        claim_id = self.claim_id_by_alias["legacy_dogfood_naming"]
        report = self.run_command(
            "correct-legacy-dogfood-naming",
            [
                "correct",
                "--claim-id",
                claim_id,
                "user prefers stable naming for dogfood",
                "--json",
            ],
            store=self.store,
        )
        latest = report.get("latest_claim") or {}
        if latest.get("object") != "stable naming for dogfood":
            raise DogfoodFailure("corrected claim object was not stable naming for dogfood")
        checks.append(f"corrected {claim_id} by claim id")

    def workflow_corrected_memory_recall(self, checks: list[str]) -> None:
        report = self.context("stable naming dogfood", "context-corrected-memory")
        require_context_contains(report, "user prefers stable naming for dogfood")
        checks.append("corrected memory is recalled")

    def workflow_corrected_old_memory_inactive(self, checks: list[str]) -> None:
        report = self.run_command(
            "claims-active-after-correct",
            ["claims", "--status", "active", "--json"],
            store=self.store,
        )
        active_texts = claim_texts(report.get("claims", []))
        if "user prefers legacy naming for dogfood" in active_texts:
            raise DogfoodFailure("legacy corrected memory is still active")
        checks.append("old corrected memory is no longer active")

    def workflow_forget_by_claim_id(self, checks: list[str]) -> None:
        claim_id = self.claim_id_by_alias["temporary_launch_color_notes"]
        self.run_command(
            "forget-temporary-launch-color-notes",
            ["forget", "--claim-id", claim_id, "--json"],
            store=self.store,
        )
        checks.append(f"forgot {claim_id} by claim id")

    def workflow_forgotten_memory_not_recalled(self, checks: list[str]) -> None:
        report = self.context("temporary launch color", "context-forgotten-memory")
        require_context_absent(report, "user prefers temporary launch color notes")
        checks.append("forgotten memory is absent from context")

    def workflow_quality_detects_review_queue(self, checks: list[str]) -> None:
        report = self.run_command(
            "quality-before-curation",
            ["quality", "--json"],
            store=self.store,
        )
        if report.get("health") != "attention_required":
            raise DogfoodFailure("quality did not require attention before curation")
        if report.get("duplicate_active_group_count", 0) < 2:
            raise DogfoodFailure("quality did not detect duplicate active groups")
        if report.get("blocked_secret_claim_count", 0) < 4:
            raise DogfoodFailure("quality did not detect blocked secrets")
        if report.get("inactive_claim_count", 0) < 2:
            raise DogfoodFailure("quality did not detect inactive lifecycle records")
        checks.append("quality review queue detects duplicates, secrets, and inactive records")

    def workflow_safe_review_redaction(self, checks: list[str]) -> None:
        review_md = self.reports_dir / "manual-review-before-curation.md"
        review_json = self.reports_dir / "manual-review-before-curation.json"
        self.run_command(
            "review-safe-markdown",
            ["review", str(review_md), "--json"],
            store=self.store,
        )
        self.run_command(
            "review-safe-json",
            ["review", str(review_json), "--format", "json", "--json"],
            store=self.store,
        )
        for path in [review_md, review_json]:
            text = path.read_text(encoding="utf-8")
            for literal in SECRET_LITERALS:
                if literal in text:
                    raise DogfoodFailure(f"safe review artifact leaked {literal}")
        checks.append("safe review artifacts redact synthetic secret literals")

    def workflow_curation_dry_run_plan(self, checks: list[str]) -> None:
        report = self.run_command(
            "curation-dry-run",
            ["curate", "--json"],
            store=self.store,
        )
        plan = report.get("plan") or {}
        if plan.get("duplicate_forget_count", 0) < 2:
            raise DogfoodFailure("curation dry run did not plan duplicate cleanup")
        if plan.get("blocked_secret_review_count", 0) < 4:
            raise DogfoodFailure("curation dry run did not surface blocked secrets")
        if not plan.get("compact_recommended"):
            raise DogfoodFailure("curation dry run did not recommend compaction")
        checks.append("curation dry run plans duplicate cleanup and compaction")

    def workflow_curation_apply_compact(self, checks: list[str]) -> None:
        report = self.run_command(
            "curation-apply-compact",
            ["curate", "--apply", "--compact", "--json"],
            store=self.store,
        )
        if not report.get("changed"):
            raise DogfoodFailure("curation apply did not change the store")
        after = report.get("after") or {}
        if after.get("health") != "ok":
            raise DogfoodFailure("curation apply did not leave quality health ok")
        checks.append("curation apply compact cleaned the review queue")

    def workflow_post_curation_quality_ok(self, checks: list[str]) -> None:
        report = self.run_command(
            "quality-after-curation",
            ["quality", "--json"],
            store=self.store,
        )
        if report.get("health") != "ok" or report.get("review_item_count") != 0:
            raise DogfoodFailure("post-curation quality is not ok")
        checks.append("post-curation quality is ok")

    def workflow_restore_check_available(self, checks: list[str]) -> None:
        report = self.run_command(
            "restore-check-after-curation",
            ["restore", "--check", "--json"],
            store=self.store,
        )
        if not report.get("restore_available"):
            raise DogfoodFailure("restore check did not report restore availability")
        checks.append("restore check sees the pre-curation backup")

    def workflow_restore_and_swap_back(self, checks: list[str]) -> None:
        first = self.run_command(
            "restore-pre-curation",
            ["restore", "--json"],
            store=self.store,
        )
        if not first.get("ok"):
            raise DogfoodFailure("first restore did not succeed")
        attention = self.run_command(
            "quality-after-restore",
            ["quality", "--json"],
            store=self.store,
        )
        if attention.get("health") != "attention_required":
            raise DogfoodFailure("restored pre-curation store did not require attention")
        second = self.run_command(
            "restore-swap-back-curated",
            ["restore", "--json"],
            store=self.store,
        )
        if not second.get("ok"):
            raise DogfoodFailure("second restore did not swap back")
        healthy = self.run_command(
            "quality-after-swap-back",
            ["quality", "--json"],
            store=self.store,
        )
        if healthy.get("health") != "ok":
            raise DogfoodFailure("swap-back store is not healthy")
        checks.append("restore can roll back and swap back to curated state")

    def workflow_export_curated_store(self, checks: list[str]) -> None:
        export_path = self.reports_dir / "curated-store-export.json"
        report = self.run_command(
            "export-curated-store",
            ["export", str(export_path), "--json"],
            store=self.store,
        )
        if report.get("claim_count", 0) < 80:
            raise DogfoodFailure("curated export has fewer claims than expected")
        checks.append("curated store exported")

    def workflow_import_curated_store(self, checks: list[str]) -> None:
        export_path = self.reports_dir / "curated-store-export.json"
        import_store = self.workspace_dir / ".mneme" / "mneme-v1-imported.json"
        report = self.run_command(
            "import-curated-store",
            ["import", str(export_path), "--json"],
            store=import_store,
        )
        validation = report.get("validation") or {}
        if not validation.get("ok"):
            raise DogfoodFailure("imported store validation failed")
        self.import_store = import_store
        checks.append("curated store imported into a second isolated store")

    def workflow_imported_store_recall(self, checks: list[str]) -> None:
        report = self.context(
            "stable naming dogfood",
            "context-imported-corrected-memory",
            store=self.import_store,
        )
        require_context_contains(report, "user prefers stable naming for dogfood")
        checks.append("imported store recalls corrected memory")

    def workflow_agent_begin_private_context(self, checks: list[str]) -> None:
        report = self.run_command(
            "agent-begin-private-context",
            [
                "begin",
                "Draft dogfood release brief",
                "--query",
                "release evidence",
                "--agent",
                "codex",
                "--json",
            ],
            store=self.store,
        )
        session = (report.get("report") or {}).get("session") or {}
        context = (report.get("report") or {}).get("context_pack") or {}
        if not session.get("id") or len(context.get("items", [])) == 0:
            raise DogfoodFailure("agent begin did not capture session context")
        self.agent_session_id = session["id"]
        checks.append("agent begin captured private release context")

    def workflow_agent_end_remembers_summary(self, checks: list[str]) -> None:
        report = self.run_command(
            "agent-end-remembers-summary",
            [
                "end",
                self.agent_session_id,
                "--summary",
                "Drafted a dogfood release brief",
                "--remember",
                "user prefers dogfood session summaries",
                "--json",
            ],
            store=self.store,
        )
        end_report = report.get("report") or {}
        if len(end_report.get("remembered_claim_ids", [])) != 1:
            raise DogfoodFailure("agent end did not remember one summary claim")
        checks.append("agent end persisted a remembered summary")

    def workflow_hook_doctor_json_envelope(self, checks: list[str]) -> None:
        report = self.run_command(
            "hook-doctor-json-envelope",
            ["hook", "doctor"],
            store=self.store,
        )
        if report.get("schema_version") != "mneme.agent_hook.v1" or not report.get("ok"):
            raise DogfoodFailure("hook doctor did not emit a successful hook envelope")
        checks.append("hook doctor emitted a valid JSON envelope")

    def workflow_hook_begin_project_context(self, checks: list[str]) -> None:
        report = self.run_command(
            "hook-begin-project-context",
            [
                "hook",
                "begin",
                "Plan project alpha release",
                "--query",
                "PR release",
                "--scope",
                "project-alpha",
                "--agent",
                "codex",
            ],
            store=self.store,
        )
        if report.get("context_item_count", 0) == 0:
            raise DogfoodFailure("hook begin did not retrieve project context")
        self.hook_session_id = report["session_id"]
        checks.append("hook begin retrieved project-alpha context")

    def workflow_hook_end_remembers_summary(self, checks: list[str]) -> None:
        report = self.run_command(
            "hook-end-remembers-summary",
            [
                "hook",
                "end",
                self.hook_session_id,
                "--summary",
                "Planned project alpha release",
                "--remember",
                "project-alpha prefers release plans with dogfood evidence",
            ],
            store=self.store,
        )
        if report.get("remembered_claim_count") != 1:
            raise DogfoodFailure("hook end did not remember one project summary")
        checks.append("hook end persisted a project summary")

    def context(
        self,
        query: str,
        slug: str,
        scopes: list[str] | None = None,
        max_items: int | None = None,
        store: Path | None = None,
    ) -> dict[str, Any]:
        args = ["context", query, "--json"]
        for scope in scopes or []:
            args.extend(["--scope", scope])
        if max_items is not None:
            args.extend(["--max-items", str(max_items)])
        return self.run_command(slug, args, store=store or self.store)

    def write_summary(
        self,
        status: str,
        ingested: list[dict[str, Any]] | None = None,
        error: str | None = None,
    ) -> dict[str, Any]:
        passed_workflows = sum(
            1 for workflow in self.workflow_results if workflow["status"] == "passed"
        )
        failed_workflows = sum(
            1 for workflow in self.workflow_results if workflow["status"] == "failed"
        )
        report = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-manual-dogfood",
            "run_label": self.run_label,
            "status": status,
            "decision_status": "v1_manual_dogfood_passed"
            if status == "passed"
            else "blocked",
            "out_dir": str(self.out_dir),
            "preflight": self.preflight,
            "mock_record_count": len(self.records),
            "ingested_record_count": len(ingested or []),
            "workflow_count": len(WORKFLOW_DEFINITIONS),
            "passed_workflows": passed_workflows,
            "failed_workflows": failed_workflows,
            "store": str(self.store),
            "reports": {
                "mock_dataset": str(self.out_dir / "mock-dataset.json"),
                "mock_records": str(self.out_dir / "mock-records.json"),
                "workflow_plan": str(self.out_dir / "workflow-plan.json"),
                "workflow_results": str(self.reports_dir / "workflow-results.json"),
                "commands": str(self.commands_dir),
            },
            "workflows": self.workflow_results,
            "command_artifacts": self.command_artifacts,
            "recommended_next_actions": recommended_next_actions(status),
        }
        if error:
            report["error"] = error
        summary_path = self.out_dir / "summary.json"
        write_json(summary_path, report)
        return report


def recommended_next_actions(status: str) -> list[str]:
    if status == "passed":
        return [
            "Review the ignored evidence bundle before promoting v1 behavior.",
            "Turn any observed product friction into sanitized scenario candidates or issues.",
            "Use this result with live provider baselines before changing extraction behavior.",
        ]
    return [
        "Inspect the failed workflow entry and command artifact paths in summary.json.",
        "Do not treat this run as v1 dogfood evidence until status is passed.",
    ]


def claim_texts(claims: list[dict[str, Any]]) -> list[str]:
    return [
        " ".join([claim.get("subject", ""), claim.get("predicate", ""), claim.get("object", "")])
        .strip()
        for claim in claims
    ]


def context_texts(report: dict[str, Any]) -> list[str]:
    return [
        item.get("claim_text", "")
        for item in (report.get("context_pack") or {}).get("items", [])
    ]


def require_context_contains(report: dict[str, Any], expected: str) -> None:
    if expected not in context_texts(report):
        raise DogfoodFailure(f"context did not include: {expected}")


def require_context_absent(report: dict[str, Any], unexpected: str) -> None:
    if unexpected in context_texts(report):
        raise DogfoodFailure(f"context unexpectedly included: {unexpected}")


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def redact_paths(argv: list[str]) -> list[str]:
    root = str(ROOT)
    return [arg.replace(root, "<repo>") for arg in argv]


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run v1 manual dogfood with 100 synthetic records and 25 workflows."
    )
    parser.add_argument("--run-label", help="Run label for evals/runs/v1-manual-dogfood.")
    parser.add_argument("--out-dir", help="Explicit output directory for the evidence bundle.")
    parser.add_argument("--mneme-bin", help="Use an existing mneme binary instead of target/debug/mneme.")
    parser.add_argument("--force", action="store_true", help="Replace an existing output directory.")
    parser.add_argument("--skip-preflight", action="store_true", help="Skip scripts/v1-dogfood.sh preflight.")
    parser.add_argument("--no-build", action="store_true", help="Do not build mneme-cli before running.")
    parser.add_argument(
        "--check-dataset",
        action="store_true",
        help="Validate and print the synthetic dataset summary without running CLI workflows.",
    )
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    records = build_mock_records()
    summary = dataset_summary(records)
    if args.check_dataset:
        print(json.dumps(summary, ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    if len(WORKFLOW_DEFINITIONS) != EXPECTED_WORKFLOW_COUNT:
        raise AssertionError(
            f"expected {EXPECTED_WORKFLOW_COUNT} workflows, got {len(WORKFLOW_DEFINITIONS)}"
        )

    run = DogfoodRun(args, records)
    ingested: list[dict[str, Any]] = []
    try:
        run.prepare()
        run.run_preflight()
        ingested = run.ingest_records()
        run.run_workflows()
        report = run.write_summary("passed", ingested=ingested)
        print(f"v1-manual-dogfood: wrote {run.out_dir}")
        print(f"v1-manual-dogfood: summary {run.out_dir / 'summary.json'}")
        print(f"v1-manual-dogfood: decision {report['decision_status']}")
        return 0
    except Exception as exc:  # noqa: BLE001 - always write a dogfood summary.
        run.write_summary("failed", ingested=ingested, error=str(exc))
        print(f"v1-manual-dogfood: failed: {exc}", file=sys.stderr)
        print(f"v1-manual-dogfood: summary {run.out_dir / 'summary.json'}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
