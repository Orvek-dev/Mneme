#!/usr/bin/env python3
"""Run the Mneme v1 natural-language ontology benchmark.

This runner intentionally measures current v1 without changing extraction or
storage behavior. Low ontology scores are expected until later ontology work is
implemented; the benchmark exists to make those gaps measurable.
"""

from __future__ import annotations

import argparse
import copy
import html
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
BENCHMARK_ID = "v1-natural-language-ontology-v0"
EXPECTED_CASE_COUNT = 14
RUN_LABEL_RE = re.compile(r"^[A-Za-z0-9._/-]+$")
SECRET_RE = re.compile(r"(?i)\b(api_key|token|access_token|password)\s*=")
KEY_LIKE_RE = "sk" + r"-[A-Za-z0-9_-]{16,}"
PRIVATE_TEMPLATE_RE = r"(?:^|[/\\])(?:99_[^/\\]*_template|[^/\\]*_harness)(?:[/\\]|$)"
PRIVATE_RE = re.compile(r"(/Users/|" + KEY_LIKE_RE + r"|" + PRIVATE_TEMPLATE_RE + r")")
STATUS_VALUES = {"active", "blocked_secret", "superseded", "forgotten"}
ONTOLOGY_TARGETS = {
    "entity_f1": 0.8,
    "relation_f1": 0.8,
    "attribute_f1": 0.8,
    "scope_accuracy": 0.95,
    "temporal_correctness": 0.8,
    "provenance_coverage": 1.0,
    "context_recall_at_k": 0.8,
}
CAPABILITY_ORDER = [
    "natural_language_extraction",
    "relation_mapping",
    "entity_resolution",
    "attribute_capture",
    "temporal_state",
    "multi_hop_context",
    "scope_ownership",
    "provenance",
    "safety",
]
CAPABILITY_LABELS = {
    "natural_language_extraction": "Natural-language extraction",
    "relation_mapping": "Relation mapping",
    "entity_resolution": "Entity resolution",
    "attribute_capture": "Attribute capture",
    "temporal_state": "Temporal state",
    "multi_hop_context": "Multi-hop context recall",
    "scope_ownership": "Scope ownership",
    "provenance": "Provenance coverage",
    "safety": "Safety and leak prevention",
}


class BenchmarkFailure(RuntimeError):
    """Raised when the benchmark fixture or run cannot complete."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()
DEFAULT_FIXTURE = ROOT / "evals" / "ontology" / "v1-natural-language-ontology-v0.json"


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-ontology-benchmark-contract",
        "benchmark_id": BENCHMARK_ID,
        "case_count": EXPECTED_CASE_COUNT,
        "default_fixture": str(DEFAULT_FIXTURE.relative_to(ROOT)),
        "default_output": "evals/runs/v1-ontology-benchmark/<run-label>",
        "metric_names": [
            "entity_f1",
            "relation_f1",
            "attribute_f1",
            "scope_accuracy",
            "temporal_correctness",
            "provenance_coverage",
            "context_recall_at_k",
            "context_precision_at_k",
            "scope_leak_count",
            "secret_leak_count",
            "prohibited_relation_count",
        ],
        "targets": ONTOLOGY_TARGETS,
        "decision_policy": (
            "full runs exit successfully when the benchmark is measured; low scores produce "
            "ontology_design_needed instead of a process failure"
        ),
        "privacy_policy": "fixture data is synthetic and public-safe; run bundles are ignored by git",
        "gap_analysis": {
            "command": "v1-ontology-benchmark-gap-analysis",
            "capability_buckets": CAPABILITY_ORDER,
            "readiness_statuses": ["v1_ontology_ready", "v1_ontology_design_needed"],
        },
    }


def load_fixture(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        fixture = json.load(handle)
    return fixture


def validate_fixture(fixture: dict[str, Any]) -> dict[str, Any]:
    errors: list[str] = []
    if fixture.get("schema_version") != SCHEMA_VERSION:
        errors.append(f"schema_version must be {SCHEMA_VERSION}")
    if fixture.get("benchmark_id") != BENCHMARK_ID:
        errors.append(f"benchmark_id must be {BENCHMARK_ID}")
    cases = fixture.get("cases")
    if not isinstance(cases, list):
        errors.append("cases must be a list")
        cases = []
    if len(cases) != EXPECTED_CASE_COUNT:
        errors.append(f"expected {EXPECTED_CASE_COUNT} cases, got {len(cases)}")

    case_ids: set[str] = set()
    categories: Counter[str] = Counter()
    input_styles: Counter[str] = Counter()
    entity_count = 0
    relation_count = 0
    attribute_count = 0
    context_check_count = 0
    temporal_check_count = 0
    must_not_relation_count = 0

    for case in cases:
        case_id = expect_string(case, "id", errors)
        if case_id:
            if case_id in case_ids:
                errors.append(f"duplicate case id: {case_id}")
            case_ids.add(case_id)
        category = expect_string(case, "category", errors)
        input_style = expect_string(case, "input_style", errors)
        categories[category] += 1
        input_styles[input_style] += 1
        events = case.get("events")
        if not isinstance(events, list) or not events:
            errors.append(f"{case_id}: events must be a non-empty list")
            events = []
        event_ids: set[str] = set()
        for event in events:
            event_id = expect_string(event, "id", errors)
            if event_id in event_ids:
                errors.append(f"{case_id}: duplicate event id {event_id}")
            event_ids.add(event_id)
            for field in ["speaker_id", "scope", "trust_level", "text"]:
                expect_string(event, field, errors)
            text = event.get("text", "")
            if PRIVATE_RE.search(text):
                errors.append(f"{case_id}/{event_id}: text contains private-like pattern")

        expected = case.get("expected")
        if not isinstance(expected, dict):
            errors.append(f"{case_id}: expected must be an object")
            expected = {}

        entities = expected.get("entities", [])
        if not isinstance(entities, list):
            errors.append(f"{case_id}: expected.entities must be a list")
            entities = []
        entity_ids: set[str] = set()
        for entity in entities:
            entity_id = expect_string(entity, "id", errors)
            expect_string(entity, "type", errors)
            expect_string(entity, "name", errors)
            if entity_id in entity_ids:
                errors.append(f"{case_id}: duplicate entity id {entity_id}")
            entity_ids.add(entity_id)
        entity_count += len(entities)

        relation_ids: set[str] = set()
        relations = expected.get("relations", [])
        if not isinstance(relations, list):
            errors.append(f"{case_id}: expected.relations must be a list")
            relations = []
        for relation in relations:
            validate_relation_like(case_id, relation, entity_ids, event_ids, errors)
            relation_id = expect_string(relation, "id", errors)
            if relation_id in relation_ids:
                errors.append(f"{case_id}: duplicate relation id {relation_id}")
            relation_ids.add(relation_id)
        relation_count += len(relations)

        must_not_relations = expected.get("must_not_relations", [])
        if not isinstance(must_not_relations, list):
            errors.append(f"{case_id}: expected.must_not_relations must be a list")
            must_not_relations = []
        for relation in must_not_relations:
            validate_relation_like(case_id, relation, entity_ids, event_ids, errors, allow_no_source=True)
            relation_id = expect_string(relation, "id", errors)
            if relation_id in relation_ids:
                errors.append(f"{case_id}: relation id reused by must_not relation {relation_id}")
            relation_ids.add(relation_id)
        must_not_relation_count += len(must_not_relations)

        attributes = expected.get("attributes", [])
        if not isinstance(attributes, list):
            errors.append(f"{case_id}: expected.attributes must be a list")
            attributes = []
        for attribute in attributes:
            expect_string(attribute, "id", errors)
            entity_id = expect_string(attribute, "entity", errors)
            if entity_id not in entity_ids:
                errors.append(f"{case_id}: attribute references unknown entity {entity_id}")
            for field in ["key", "value", "scope", "status"]:
                expect_string(attribute, field, errors)
            if attribute.get("status") not in STATUS_VALUES:
                errors.append(f"{case_id}: invalid attribute status {attribute.get('status')}")
            for source_event_id in attribute.get("source_event_ids", []):
                if source_event_id not in event_ids:
                    errors.append(
                        f"{case_id}: attribute {attribute.get('id')} references unknown event {source_event_id}"
                    )
        attribute_count += len(attributes)

        context_checks = expected.get("context_checks", [])
        if not isinstance(context_checks, list):
            errors.append(f"{case_id}: expected.context_checks must be a list")
            context_checks = []
        for check in context_checks:
            expect_string(check, "id", errors)
            expect_string(check, "query", errors)
            allowed_scopes = check.get("allowed_scopes", [])
            if not isinstance(allowed_scopes, list) or not allowed_scopes:
                errors.append(f"{case_id}: context check {check.get('id')} needs allowed_scopes")
            for field in ["must_include_relation_ids", "must_not_include_relation_ids"]:
                values = check.get(field, [])
                if not isinstance(values, list):
                    errors.append(f"{case_id}: context check {check.get('id')} {field} must be a list")
                    values = []
                for relation_id in values:
                    if relation_id not in relation_ids:
                        errors.append(
                            f"{case_id}: context check {check.get('id')} references unknown relation {relation_id}"
                        )
        context_check_count += len(context_checks)

        temporal_checks = expected.get("temporal_checks", [])
        if not isinstance(temporal_checks, list):
            errors.append(f"{case_id}: expected.temporal_checks must be a list")
            temporal_checks = []
        for check in temporal_checks:
            for field in ["id", "entity", "predicate", "active_object_text"]:
                expect_string(check, field, errors)
            if check.get("entity") not in entity_ids:
                errors.append(f"{case_id}: temporal check references unknown entity {check.get('entity')}")
        temporal_check_count += len(temporal_checks)

    summary = {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-ontology-benchmark-fixture",
        "benchmark_id": fixture.get("benchmark_id"),
        "case_count": len(cases),
        "categories": dict(sorted(categories.items())),
        "input_styles": dict(sorted(input_styles.items())),
        "entity_count": entity_count,
        "relation_count": relation_count,
        "attribute_count": attribute_count,
        "context_check_count": context_check_count,
        "temporal_check_count": temporal_check_count,
        "must_not_relation_count": must_not_relation_count,
        "ok": not errors,
        "errors": errors,
    }
    if errors:
        raise BenchmarkFailure("fixture validation failed: " + "; ".join(errors))
    return summary


def validate_relation_like(
    case_id: str,
    relation: dict[str, Any],
    entity_ids: set[str],
    event_ids: set[str],
    errors: list[str],
    *,
    allow_no_source: bool = False,
) -> None:
    expect_string(relation, "id", errors)
    expect_string(relation, "predicate", errors)
    expect_string(relation, "scope", errors)
    subject_id = relation.get("subject")
    subject_text = relation.get("subject_text")
    object_id = relation.get("object")
    object_text = relation.get("object_text")
    if subject_id:
        if subject_id not in entity_ids:
            errors.append(f"{case_id}: relation {relation.get('id')} unknown subject {subject_id}")
    elif not subject_text:
        errors.append(f"{case_id}: relation {relation.get('id')} needs subject or subject_text")
    if object_id:
        if object_id not in entity_ids:
            errors.append(f"{case_id}: relation {relation.get('id')} unknown object {object_id}")
    elif not object_text:
        errors.append(f"{case_id}: relation {relation.get('id')} needs object or object_text")
    if not allow_no_source:
        source_event_ids = relation.get("source_event_ids", [])
        if not isinstance(source_event_ids, list) or not source_event_ids:
            errors.append(f"{case_id}: relation {relation.get('id')} needs source_event_ids")
        for source_event_id in source_event_ids:
            if source_event_id not in event_ids:
                errors.append(
                    f"{case_id}: relation {relation.get('id')} references unknown event {source_event_id}"
                )
        if relation.get("status") not in STATUS_VALUES:
            errors.append(f"{case_id}: invalid relation status {relation.get('status')}")


def expect_string(value: dict[str, Any], key: str, errors: list[str]) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item.strip():
        errors.append(f"field {key} must be a non-empty string")
        return ""
    return item


class BenchmarkRun:
    def __init__(self, args: argparse.Namespace, fixture: dict[str, Any]) -> None:
        self.args = args
        self.fixture = fixture
        self.fixture_summary = validate_fixture(fixture)
        self.run_label = args.run_label or time.strftime("local-%Y%m%d-%H%M%S")
        if not RUN_LABEL_RE.match(self.run_label):
            raise BenchmarkFailure("run label may contain only letters, digits, '-', '_', '.', or '/'")
        self.out_dir = (
            Path(args.out_dir)
            if args.out_dir
            else ROOT / "evals" / "runs" / "v1-ontology-benchmark" / self.run_label
        )
        self.store_dir = self.out_dir / "stores"
        self.commands_dir = self.out_dir / "commands"
        self.reports_dir = self.out_dir / "reports"
        self.mneme_bin = ROOT / "target" / "debug" / "mneme"
        self.command_index = 0
        self.command_artifacts: list[dict[str, Any]] = []
        self.case_runs: list[dict[str, Any]] = []
        self.scorecard: dict[str, Any] = {}
        self.gap_analysis: dict[str, Any] = {}

    def prepare(self) -> None:
        if self.out_dir.exists():
            if not self.args.force:
                raise BenchmarkFailure(f"output directory exists, pass --force to replace: {self.out_dir}")
            shutil.rmtree(self.out_dir)
        self.store_dir.mkdir(parents=True, exist_ok=True)
        self.commands_dir.mkdir(parents=True, exist_ok=True)
        self.reports_dir.mkdir(parents=True, exist_ok=True)
        write_json(self.out_dir / "fixture.json", self.fixture)
        write_json(self.out_dir / "fixture-summary.json", self.fixture_summary)
        write_json(self.out_dir / "contract.json", contract())

    def build(self) -> None:
        if self.args.no_build:
            return
        self.run_external("cargo-build-mneme-cli", ["cargo", "build", "-q", "-p", "mneme-cli"])

    def run(self) -> None:
        for case in self.fixture["cases"]:
            case_run = self.run_case(case)
            self.case_runs.append(case_run)
        write_json(self.reports_dir / "case-runs.json", {"cases": self.case_runs})
        self.scorecard = score_benchmark(self.fixture, self.case_runs)
        write_json(self.out_dir / "scorecard.json", self.scorecard)
        self.gap_analysis = build_gap_analysis(self.fixture, self.scorecard)
        write_json(self.out_dir / "gap-analysis.json", self.gap_analysis)
        write_gap_analysis_markdown(self.out_dir / "gap-analysis.md", self.gap_analysis)

    def run_case(self, case: dict[str, Any]) -> dict[str, Any]:
        case_dir = self.reports_dir / case["id"]
        case_dir.mkdir(parents=True, exist_ok=True)
        store = self.store_dir / f"{case['id']}.json"
        event_reports = []
        for event in case["events"]:
            args = [
                str(self.mneme_bin),
                "ingest",
                event["text"],
                "--speaker",
                event["speaker_id"],
                "--scope",
                event["scope"],
                "--trust",
                event["trust_level"],
                "--store",
                str(store),
                "--json",
            ]
            if event.get("actor_agent_id"):
                args.extend(["--agent", event["actor_agent_id"]])
            event_reports.append(self.run_command(f"{case['id']}-ingest-{event['id']}", args))

        snapshot = self.run_command(
            f"{case['id']}-snapshot",
            [str(self.mneme_bin), "snapshot", "--store", str(store), "--json"],
        )
        validate = self.run_command(
            f"{case['id']}-validate",
            [str(self.mneme_bin), "validate", "--store", str(store), "--json"],
        )
        context_reports = []
        for check in case["expected"].get("context_checks", []):
            args = [
                str(self.mneme_bin),
                "context",
                check["query"],
                "--max-items",
                "8",
                "--store",
                str(store),
                "--json",
            ]
            for scope in check.get("allowed_scopes", []):
                args.extend(["--scope", scope])
            context_reports.append(
                {
                    "check_id": check["id"],
                    "report": self.run_command(f"{case['id']}-context-{check['id']}", args),
                }
            )

        result = {
            "case_id": case["id"],
            "category": case["category"],
            "input_style": case["input_style"],
            "event_reports": event_reports,
            "snapshot": snapshot.get("snapshot", {}),
            "validation": validate,
            "context_reports": context_reports,
        }
        write_json(case_dir / "case-run.json", result)
        return result

    def run_external(self, slug: str, command: list[str]) -> None:
        self.command_index += 1
        stdout_path = self.commands_dir / f"{self.command_index:03d}-{slug}.txt"
        stderr_path = self.commands_dir / f"{self.command_index:03d}-{slug}.stderr.txt"
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
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
            raise BenchmarkFailure(f"{slug} failed with exit {result.returncode}: {result.stderr}")

    def run_command(self, slug: str, command: list[str]) -> Any:
        self.command_index += 1
        stdout_path = self.commands_dir / f"{self.command_index:03d}-{slug}.json"
        stderr_path = self.commands_dir / f"{self.command_index:03d}-{slug}.stderr.txt"
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
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
            raise BenchmarkFailure(f"{slug} failed with exit {result.returncode}: {result.stderr}")
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError as exc:
            raise BenchmarkFailure(f"{slug} did not emit JSON: {exc}") from exc

    def write_summary(self, status: str, error: str | None = None) -> dict[str, Any]:
        decision_status = decision_from_scorecard(self.scorecard) if self.scorecard else "blocked"
        summary = {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-ontology-benchmark",
            "benchmark_id": BENCHMARK_ID,
            "run_label": self.run_label,
            "status": status,
            "decision_status": decision_status,
            "out_dir": str(self.out_dir),
            "fixture": self.fixture_summary,
            "scorecard": self.scorecard,
            "gap_analysis": self.gap_analysis,
            "reports": {
                "fixture": str(self.out_dir / "fixture.json"),
                "fixture_summary": str(self.out_dir / "fixture-summary.json"),
                "scorecard": str(self.out_dir / "scorecard.json"),
                "gap_analysis": str(self.out_dir / "gap-analysis.json"),
                "gap_analysis_markdown": str(self.out_dir / "gap-analysis.md"),
                "case_runs": str(self.reports_dir / "case-runs.json"),
                "markdown": str(self.out_dir / "report.md"),
                "html": str(self.out_dir / "report.html"),
                "commands": str(self.commands_dir),
            },
            "command_artifacts": self.command_artifacts,
            "recommended_next_actions": recommended_next_actions(
                decision_status,
                self.scorecard,
                self.gap_analysis,
            ),
        }
        if error:
            summary["error"] = error
        write_json(self.out_dir / "summary.json", summary)
        write_markdown_report(self.out_dir / "report.md", summary)
        write_html_report(self.out_dir / "report.html", summary)
        return summary


def score_benchmark(fixture: dict[str, Any], case_runs: list[dict[str, Any]]) -> dict[str, Any]:
    case_run_by_id = {case_run["case_id"]: case_run for case_run in case_runs}
    totals = ScoreTotals()
    case_scores = []
    for case in fixture["cases"]:
        case_score = score_case(case, case_run_by_id[case["id"]])
        case_scores.append(case_score)
        totals.add(case_score)
    scorecard = totals.scorecard()
    scorecard.update(
        {
            "schema_version": SCHEMA_VERSION,
            "command": "v1-ontology-benchmark-scorecard",
            "benchmark_id": fixture["benchmark_id"],
            "case_count": len(case_scores),
            "case_scores": case_scores,
            "case_category_counts": dict(sorted(Counter(case["category"] for case in fixture["cases"]).items())),
            "input_style_counts": dict(
                sorted(Counter(case["input_style"] for case in fixture["cases"]).items())
            ),
        }
    )
    return scorecard


def build_gap_analysis(fixture: dict[str, Any], scorecard: dict[str, Any]) -> dict[str, Any]:
    capabilities = {capability: empty_capability(capability) for capability in CAPABILITY_ORDER}
    case_gaps = []
    for case_score in scorecard.get("case_scores", []):
        counts = case_score["counts"]
        case_id = case_score["case_id"]
        case_gap_total = 0

        if case_score["input_style"] == "natural_language":
            expected = (
                counts["entity_expected"]
                + counts["relation_expected"]
                + counts["attribute_expected"]
                + counts["temporal_checks"]
            )
            matched = (
                counts["entity_matched"]
                + counts["relation_matched"]
                + counts["attribute_matched"]
                + counts["temporal_correct"]
            )
            case_gap_total += add_capability_counts(
                capabilities["natural_language_extraction"],
                case_id,
                expected,
                matched,
            )

        case_gap_total += add_capability_counts(
            capabilities["entity_resolution"],
            case_id,
            counts["entity_expected"],
            counts["entity_matched"],
        )
        case_gap_total += add_capability_counts(
            capabilities["relation_mapping"],
            case_id,
            counts["relation_expected"],
            counts["relation_matched"],
        )
        case_gap_total += add_capability_counts(
            capabilities["attribute_capture"],
            case_id,
            counts["attribute_expected"],
            counts["attribute_matched"],
        )
        case_gap_total += add_capability_counts(
            capabilities["temporal_state"],
            case_id,
            counts["temporal_checks"],
            counts["temporal_correct"],
        )
        case_gap_total += add_capability_counts(
            capabilities["multi_hop_context"],
            case_id,
            counts["context_recall_attempts"],
            counts["context_recall_successes"],
        )

        scope_expected = counts["scope_checks"] + counts["context_items"]
        scope_matched = counts["scope_correct"] + max(
            counts["context_items"] - counts["scope_leak_count"],
            0,
        )
        case_gap_total += add_capability_counts(
            capabilities["scope_ownership"],
            case_id,
            scope_expected,
            scope_matched,
        )
        case_gap_total += add_capability_counts(
            capabilities["provenance"],
            case_id,
            counts["provenance_checks"],
            counts["provenance_with_source"],
        )

        safety_violations = (
            counts["scope_leak_count"]
            + counts["secret_leak_count"]
            + counts["prohibited_relation_count"]
        )
        case_gap_total += add_safety_counts(
            capabilities["safety"],
            case_id,
            safety_violations,
        )

        if case_gap_total:
            case_gaps.append(
                {
                    "case_id": case_id,
                    "category": case_score["category"],
                    "input_style": case_score["input_style"],
                    "missing_or_violating_signals": case_gap_total,
                }
            )

    capability_summaries = [finalize_capability(capabilities[key]) for key in CAPABILITY_ORDER]
    top_gaps = [
        summary
        for summary in sorted(capability_summaries, key=capability_gap_rank)
        if summary["severity"] != "ok"
    ]
    readiness_status = (
        "v1_ontology_ready"
        if decision_from_scorecard(scorecard) == "ontology_benchmark_passed" and not top_gaps
        else "v1_ontology_design_needed"
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-ontology-benchmark-gap-analysis",
        "benchmark_id": fixture["benchmark_id"],
        "readiness_status": readiness_status,
        "decision_status": decision_from_scorecard(scorecard),
        "target_misses": target_misses(scorecard),
        "capabilities": capability_summaries,
        "top_gaps": top_gaps[:5],
        "case_gap_summary": sorted(
            case_gaps,
            key=lambda item: (-item["missing_or_violating_signals"], item["case_id"]),
        ),
        "by_category": aggregate_case_scores(scorecard, "category"),
        "by_input_style": aggregate_case_scores(scorecard, "input_style"),
        "recommended_implementation_order": implementation_priorities(top_gaps),
        "public_v1_gate": {
            "status": "pass" if readiness_status == "v1_ontology_ready" else "blocked",
            "reason": (
                "ontology targets passed"
                if readiness_status == "v1_ontology_ready"
                else "ontology capability gaps remain measurable"
            ),
        },
    }


def empty_capability(capability: str) -> dict[str, Any]:
    return {
        "capability": capability,
        "label": CAPABILITY_LABELS[capability],
        "expected": 0,
        "matched": 0,
        "missed": 0,
        "violations": 0,
        "affected_cases": set(),
    }


def add_capability_counts(capability: dict[str, Any], case_id: str, expected: int, matched: int) -> int:
    missed = max(expected - matched, 0)
    capability["expected"] += expected
    capability["matched"] += matched
    capability["missed"] += missed
    if missed:
        capability["affected_cases"].add(case_id)
    return missed


def add_safety_counts(capability: dict[str, Any], case_id: str, violations: int) -> int:
    capability["violations"] += violations
    capability["missed"] += violations
    if violations:
        capability["affected_cases"].add(case_id)
    return violations


def finalize_capability(capability: dict[str, Any]) -> dict[str, Any]:
    expected = capability["expected"]
    matched = capability["matched"]
    missed = capability["missed"]
    violations = capability["violations"]
    if capability["capability"] == "safety":
        health = 1.0 if violations == 0 else 0.0
    elif expected == 0:
        health = 1.0
    else:
        health = ratio(matched, expected)
    return {
        "capability": capability["capability"],
        "label": capability["label"],
        "expected": expected,
        "matched": matched,
        "missed": missed,
        "violations": violations,
        "health": round(health, 4),
        "severity": gap_severity(health, missed, violations),
        "affected_cases": sorted(capability["affected_cases"]),
    }


def gap_severity(health: float, missed: int, violations: int) -> str:
    if violations:
        return "blocker"
    if missed == 0:
        return "ok"
    if health < 0.5:
        return "high"
    if health < 0.8:
        return "medium"
    return "low"


def capability_gap_rank(summary: dict[str, Any]) -> tuple[int, int, float, str]:
    severity_rank = {"blocker": 0, "high": 1, "medium": 2, "low": 3, "ok": 4}
    return (
        severity_rank.get(summary["severity"], 9),
        -int(summary["missed"]),
        float(summary["health"]),
        summary["capability"],
    )


def target_misses(scorecard: dict[str, Any]) -> list[dict[str, Any]]:
    misses = []
    for metric, target in ONTOLOGY_TARGETS.items():
        value = float(scorecard.get(metric, 0.0))
        if value < target:
            misses.append(
                {
                    "metric": metric,
                    "value": round(value, 4),
                    "target": target,
                    "gap": round(target - value, 4),
                }
            )
    for metric in ["scope_leak_count", "secret_leak_count", "prohibited_relation_count"]:
        value = int(scorecard.get(metric, 0))
        if value:
            misses.append({"metric": metric, "value": value, "target": 0, "gap": value})
    return misses


def aggregate_case_scores(scorecard: dict[str, Any], field: str) -> dict[str, dict[str, Any]]:
    groups: dict[str, ScoreTotals] = {}
    for case_score in scorecard.get("case_scores", []):
        group_key = case_score.get(field, "unknown")
        groups.setdefault(group_key, ScoreTotals()).add(case_score)
    return {
        key: group_totals.scorecard()
        for key, group_totals in sorted(groups.items())
    }


def implementation_priorities(top_gaps: list[dict[str, Any]]) -> list[dict[str, Any]]:
    gap_by_capability = {gap["capability"]: gap for gap in top_gaps}
    priorities = []
    for capability in CAPABILITY_ORDER:
        gap = gap_by_capability.get(capability)
        if not gap:
            continue
        priorities.append(
            {
                "capability": capability,
                "label": gap["label"],
                "severity": gap["severity"],
                "reason": implementation_reason(capability, gap),
                "affected_cases": gap["affected_cases"][:5],
            }
        )
    return priorities


def implementation_reason(capability: str, gap: dict[str, Any]) -> str:
    if capability == "safety":
        return "Safety violations block public v1 readiness even if recall scores improve."
    if capability == "natural_language_extraction":
        return "Natural-language inputs are the main user path and must produce structured claims."
    if capability == "relation_mapping":
        return "Relations are the minimum ontology layer needed for useful agent context."
    if capability == "entity_resolution":
        return "Entity resolution keeps aliases, pronouns, and project names from fragmenting memory."
    if capability == "attribute_capture":
        return "Attributes preserve stable preferences and project settings without overloading relations."
    if capability == "temporal_state":
        return "Temporal state prevents superseded decisions from being reused as current context."
    if capability == "multi_hop_context":
        return "Agent handoff quality depends on retrieving linked context, not only lexical matches."
    if capability == "scope_ownership":
        return "Scope ownership keeps personal, project, and team memories separated."
    if capability == "provenance":
        return "Public reports and reviews require every surfaced claim to cite its source event."
    return f"{gap['label']} remains below the public v1 target."


class ScoreTotals:
    def __init__(self) -> None:
        self.entity_expected = 0
        self.entity_actual = 0
        self.entity_matched = 0
        self.relation_expected = 0
        self.relation_actual = 0
        self.relation_matched = 0
        self.attribute_expected = 0
        self.attribute_actual = 0
        self.attribute_matched = 0
        self.scope_checks = 0
        self.scope_correct = 0
        self.temporal_checks = 0
        self.temporal_correct = 0
        self.provenance_checks = 0
        self.provenance_with_source = 0
        self.context_recall_attempts = 0
        self.context_recall_successes = 0
        self.context_items = 0
        self.context_false_positive_items = 0
        self.scope_leak_count = 0
        self.secret_leak_count = 0
        self.prohibited_relation_count = 0

    def add(self, case_score: dict[str, Any]) -> None:
        counts = case_score["counts"]
        for key, value in counts.items():
            setattr(self, key, getattr(self, key) + value)

    def scorecard(self) -> dict[str, Any]:
        entity_recall = ratio(self.entity_matched, self.entity_expected)
        entity_precision = ratio(self.entity_matched, self.entity_actual)
        relation_recall = ratio(self.relation_matched, self.relation_expected)
        relation_precision = ratio(self.relation_matched, self.relation_actual)
        attribute_recall = ratio(self.attribute_matched, self.attribute_expected)
        attribute_precision = ratio(self.attribute_matched, self.attribute_actual)
        context_precision = (
            1.0 - ratio(self.context_false_positive_items, self.context_items)
            if self.context_items
            else 1.0
        )
        return {
            "entity_expected": self.entity_expected,
            "entity_actual": self.entity_actual,
            "entity_matched": self.entity_matched,
            "entity_precision": round(entity_precision, 4),
            "entity_recall": round(entity_recall, 4),
            "entity_f1": round(f1(entity_precision, entity_recall), 4),
            "relation_expected": self.relation_expected,
            "relation_actual": self.relation_actual,
            "relation_matched": self.relation_matched,
            "relation_precision": round(relation_precision, 4),
            "relation_recall": round(relation_recall, 4),
            "relation_f1": round(f1(relation_precision, relation_recall), 4),
            "attribute_expected": self.attribute_expected,
            "attribute_actual": self.attribute_actual,
            "attribute_matched": self.attribute_matched,
            "attribute_precision": round(attribute_precision, 4),
            "attribute_recall": round(attribute_recall, 4),
            "attribute_f1": round(f1(attribute_precision, attribute_recall), 4),
            "scope_accuracy": round(ratio(self.scope_correct, self.scope_checks), 4),
            "temporal_correctness": round(ratio(self.temporal_correct, self.temporal_checks), 4),
            "provenance_coverage": round(ratio(self.provenance_with_source, self.provenance_checks), 4),
            "context_recall_at_k": round(ratio(self.context_recall_successes, self.context_recall_attempts), 4),
            "context_precision_at_k": round(context_precision, 4),
            "context_items": self.context_items,
            "context_false_positive_items": self.context_false_positive_items,
            "scope_leak_count": self.scope_leak_count,
            "secret_leak_count": self.secret_leak_count,
            "prohibited_relation_count": self.prohibited_relation_count,
        }


def score_case(case: dict[str, Any], case_run: dict[str, Any]) -> dict[str, Any]:
    expected = case["expected"]
    entity_by_id = {entity["id"]: entity for entity in expected.get("entities", [])}
    expected_relations = expected.get("relations", [])
    must_not_relations = expected.get("must_not_relations", [])
    expected_attributes = expected.get("attributes", [])
    relation_by_id = {relation["id"]: relation for relation in expected_relations + must_not_relations}
    claims = case_run.get("snapshot", {}).get("claims", [])
    claim_by_id = {claim["id"]: claim for claim in claims}

    scored_entity_ids: set[str] = set()
    for relation in expected.get("relations", []):
        if relation.get("subject"):
            scored_entity_ids.add(relation["subject"])
        if relation.get("object"):
            scored_entity_ids.add(relation["object"])
    for attribute in expected.get("attributes", []):
        scored_entity_ids.add(attribute["entity"])
    expected_entity_labels = {
        normalize(entity["name"])
        for entity in expected.get("entities", [])
        if entity["id"] in scored_entity_ids
    }
    entity_lookup: dict[str, str] = {}
    for entity in expected.get("entities", []):
        if entity["id"] not in scored_entity_ids:
            continue
        entity_lookup[normalize(entity["name"])] = normalize(entity["name"])
        for alias in entity.get("aliases", []):
            entity_lookup[normalize(alias)] = normalize(entity["name"])
    actual_entity_labels = set()
    for claim in claims:
        for label in [claim.get("subject", ""), claim.get("object", "")]:
            normalized_label = normalize(label)
            if normalized_label in entity_lookup:
                actual_entity_labels.add(entity_lookup[normalized_label])
    entity_matched = len(expected_entity_labels.intersection(actual_entity_labels))

    expected_relation_keys = {
        relation_key(case, entity_by_id, relation, include_scope=True, include_status=True)
        for relation in expected_relations
    }
    expected_relation_predicates = {normalize(relation["predicate"]) for relation in expected_relations}
    actual_relation_claims = [
        claim for claim in claims if normalize(claim.get("predicate", "")) in expected_relation_predicates
    ]
    actual_relation_keys = {claim_key(claim, include_scope=True, include_status=True) for claim in actual_relation_claims}
    matched_relation_keys = expected_relation_keys.intersection(actual_relation_keys)
    relation_matched = len(matched_relation_keys)

    expected_attribute_keys = {
        attribute_key(entity_by_id, attribute, include_scope=True, include_status=True)
        for attribute in expected_attributes
    }
    expected_attribute_predicates = {normalize(attribute["key"]) for attribute in expected_attributes}
    actual_attribute_claims = [
        claim for claim in claims if normalize(claim.get("predicate", "")) in expected_attribute_predicates
    ]
    actual_attribute_keys = {claim_key(claim, include_scope=True, include_status=True) for claim in actual_attribute_claims}
    matched_attribute_keys = expected_attribute_keys.intersection(actual_attribute_keys)
    attribute_matched = len(matched_attribute_keys)

    expected_base_to_scope = {
        relation_key(case, entity_by_id, relation, include_scope=False, include_status=False): relation["scope"]
        for relation in expected_relations
    }
    expected_base_to_scope.update(
        {
            attribute_key(entity_by_id, attribute, include_scope=False, include_status=False): attribute["scope"]
            for attribute in expected_attributes
        }
    )
    actual_base_to_claim = {
        claim_key(claim, include_scope=False, include_status=False): claim for claim in claims
    }
    scope_checks = len(expected_base_to_scope)
    scope_correct = 0
    provenance_checks = 0
    provenance_with_source = 0
    for base_key, expected_scope in expected_base_to_scope.items():
        claim = actual_base_to_claim.get(base_key)
        if claim:
            if claim.get("scope") == expected_scope:
                scope_correct += 1
            provenance_checks += 1
            if claim.get("source_event_ids"):
                provenance_with_source += 1

    active_claim_texts = [
        normalize(claim_text(claim)) for claim in claims if claim.get("status") == "active"
    ]
    temporal_checks = len(expected.get("temporal_checks", []))
    temporal_correct = 0
    for check in expected.get("temporal_checks", []):
        active_ok = normalize(check["active_object_text"]) in " ".join(active_claim_texts)
        inactive_ok = all(
            normalize(inactive) not in " ".join(active_claim_texts)
            for inactive in check.get("inactive_object_texts", [])
        )
        if active_ok and inactive_ok:
            temporal_correct += 1

    prohibited_relation_count = 0
    for relation in must_not_relations:
        relation_text_norm = normalize(relation_display_text(case, entity_by_id, relation))
        if any(relation_text_norm in normalize(claim_text(claim)) for claim in claims):
            prohibited_relation_count += 1

    context_recall_attempts = 0
    context_recall_successes = 0
    context_items = 0
    context_false_positive_items = 0
    scope_leak_count = 0
    secret_leak_count = 0
    for context_run in case_run.get("context_reports", []):
        check = find_context_check(expected, context_run["check_id"])
        report = context_run["report"]
        items = (report.get("context_pack") or {}).get("items", [])
        item_texts = [normalize(item.get("claim_text", "")) for item in items]
        context_items += len(items)
        allowed_scopes = set(check.get("allowed_scopes", []))
        for item in items:
            claim = claim_by_id.get(item.get("claim_id"))
            if claim and claim.get("scope") not in allowed_scopes:
                scope_leak_count += 1
                context_false_positive_items += 1
            text = item.get("claim_text", "")
            if SECRET_RE.search(text):
                secret_leak_count += 1
                context_false_positive_items += 1
        for relation_id in check.get("must_include_relation_ids", []):
            context_recall_attempts += 1
            relation_text_norm = normalize(
                relation_display_text(case, entity_by_id, relation_by_id[relation_id])
            )
            if any(relation_text_norm in item_text or item_text in relation_text_norm for item_text in item_texts):
                context_recall_successes += 1
        for relation_id in check.get("must_not_include_relation_ids", []):
            relation_text_norm = normalize(
                relation_display_text(case, entity_by_id, relation_by_id[relation_id])
            )
            if any(relation_text_norm in item_text or item_text in relation_text_norm for item_text in item_texts):
                context_false_positive_items += 1

    counts = {
        "entity_expected": len(expected_entity_labels),
        "entity_actual": len(actual_entity_labels),
        "entity_matched": entity_matched,
        "relation_expected": len(expected_relations),
        "relation_actual": len(actual_relation_claims),
        "relation_matched": relation_matched,
        "attribute_expected": len(expected_attributes),
        "attribute_actual": len(actual_attribute_claims),
        "attribute_matched": attribute_matched,
        "scope_checks": scope_checks,
        "scope_correct": scope_correct,
        "temporal_checks": temporal_checks,
        "temporal_correct": temporal_correct,
        "provenance_checks": provenance_checks,
        "provenance_with_source": provenance_with_source,
        "context_recall_attempts": context_recall_attempts,
        "context_recall_successes": context_recall_successes,
        "context_items": context_items,
        "context_false_positive_items": context_false_positive_items,
        "scope_leak_count": scope_leak_count,
        "secret_leak_count": secret_leak_count,
        "prohibited_relation_count": prohibited_relation_count,
    }
    return {
        "case_id": case["id"],
        "category": case["category"],
        "input_style": case["input_style"],
        "counts": counts,
        "actual_claim_count": len(claims),
        "actual_active_claim_count": sum(1 for claim in claims if claim.get("status") == "active"),
    }


def relation_display_text(
    case: dict[str, Any],
    entity_by_id: dict[str, dict[str, Any]],
    relation: dict[str, Any],
) -> str:
    subject = relation.get("subject_text") or entity_by_id[relation["subject"]]["name"]
    object_value = relation.get("object_text") or entity_by_id[relation["object"]]["name"]
    return f"{subject} {relation['predicate']} {object_value}"


def relation_key(
    case: dict[str, Any],
    entity_by_id: dict[str, dict[str, Any]],
    relation: dict[str, Any],
    *,
    include_scope: bool,
    include_status: bool,
) -> tuple[str, ...]:
    base = [
        normalize(relation.get("subject_text") or entity_by_id[relation["subject"]]["name"]),
        normalize(relation["predicate"]),
        normalize(relation.get("object_text") or entity_by_id[relation["object"]]["name"]),
    ]
    if include_scope:
        base.append(relation.get("scope", ""))
    if include_status:
        base.append(relation.get("status", "active"))
    return tuple(base)


def attribute_key(
    entity_by_id: dict[str, dict[str, Any]],
    attribute: dict[str, Any],
    *,
    include_scope: bool,
    include_status: bool,
) -> tuple[str, ...]:
    base = [
        normalize(entity_by_id[attribute["entity"]]["name"]),
        normalize(attribute["key"]),
        normalize(attribute["value"]),
    ]
    if include_scope:
        base.append(attribute.get("scope", ""))
    if include_status:
        base.append(attribute.get("status", "active"))
    return tuple(base)


def claim_key(claim: dict[str, Any], *, include_scope: bool, include_status: bool) -> tuple[str, ...]:
    base = [
        normalize(claim.get("subject", "")),
        normalize(claim.get("predicate", "")),
        normalize(claim.get("object", "")),
    ]
    if include_scope:
        base.append(claim.get("scope", ""))
    if include_status:
        base.append(claim.get("status", "active"))
    return tuple(base)


def claim_text(claim: dict[str, Any]) -> str:
    return f"{claim.get('subject', '')} {claim.get('predicate', '')} {claim.get('object', '')}"


def find_context_check(expected: dict[str, Any], check_id: str) -> dict[str, Any]:
    for check in expected.get("context_checks", []):
        if check["id"] == check_id:
            return check
    raise BenchmarkFailure(f"missing context check: {check_id}")


def decision_from_scorecard(scorecard: dict[str, Any]) -> str:
    if not scorecard:
        return "blocked"
    misses = [
        metric
        for metric, target in ONTOLOGY_TARGETS.items()
        if scorecard.get(metric, 0.0) < target
    ]
    if scorecard.get("scope_leak_count", 0) > 0 or scorecard.get("secret_leak_count", 0) > 0:
        misses.append("safety_leak")
    return "ontology_benchmark_passed" if not misses else "ontology_design_needed"


def recommended_next_actions(
    decision_status: str,
    scorecard: dict[str, Any],
    gap_analysis: dict[str, Any] | None = None,
) -> list[str]:
    if decision_status == "ontology_benchmark_passed":
        return [
            "Keep this benchmark as the baseline before changing ontology extraction.",
            "Promote new real failures into this fixture only after public-safe review.",
        ]
    weakest = weakest_metrics(scorecard)
    actions = [
        "Treat this as a measurement baseline, not a release failure.",
        "Use the weakest metrics to design the next v1 ontology changes.",
        "Rerun this benchmark after any extraction, ontology, or retrieval change.",
    ]
    if weakest:
        actions.append("Current weakest metrics: " + ", ".join(weakest))
    if gap_analysis:
        priorities = [
            item["capability"]
            for item in gap_analysis.get("recommended_implementation_order", [])[:3]
        ]
        if priorities:
            actions.append("Implementation priority order: " + ", ".join(priorities))
    return actions


def weakest_metrics(scorecard: dict[str, Any]) -> list[str]:
    if not scorecard:
        return []
    ranked = []
    for metric, target in ONTOLOGY_TARGETS.items():
        value = float(scorecard.get(metric, 0.0))
        ranked.append((value - target, metric))
    return [metric for _, metric in sorted(ranked)[:3]]


def check_scorer(fixture: dict[str, Any]) -> dict[str, Any]:
    validate_fixture(fixture)
    perfect_runs = synthetic_case_runs(fixture, mode="perfect")
    faulted_runs = synthetic_case_runs(fixture, mode="faulted")
    perfect_scorecard = score_benchmark(fixture, perfect_runs)
    faulted_scorecard = score_benchmark(fixture, faulted_runs)
    detected_faults = []
    if perfect_scorecard["relation_f1"] == 1.0 and faulted_scorecard["relation_f1"] < 1.0:
        detected_faults.append("dropped_relation")
    if perfect_scorecard["context_recall_at_k"] == 1.0 and faulted_scorecard["context_recall_at_k"] < 1.0:
        detected_faults.append("context_recall_miss")
    if faulted_scorecard["secret_leak_count"] > 0:
        detected_faults.append("secret_leak")
    if perfect_scorecard["provenance_coverage"] == 1.0 and faulted_scorecard["provenance_coverage"] < 1.0:
        detected_faults.append("missing_provenance")
    ok = len(detected_faults) == 4
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-ontology-benchmark-scorer-check",
        "benchmark_id": fixture["benchmark_id"],
        "ok": ok,
        "detected_faults": detected_faults,
        "perfect": {
            "entity_f1": perfect_scorecard["entity_f1"],
            "relation_f1": perfect_scorecard["relation_f1"],
            "attribute_f1": perfect_scorecard["attribute_f1"],
            "context_recall_at_k": perfect_scorecard["context_recall_at_k"],
            "provenance_coverage": perfect_scorecard["provenance_coverage"],
            "secret_leak_count": perfect_scorecard["secret_leak_count"],
        },
        "faulted": {
            "entity_f1": faulted_scorecard["entity_f1"],
            "relation_f1": faulted_scorecard["relation_f1"],
            "attribute_f1": faulted_scorecard["attribute_f1"],
            "context_recall_at_k": faulted_scorecard["context_recall_at_k"],
            "provenance_coverage": faulted_scorecard["provenance_coverage"],
            "secret_leak_count": faulted_scorecard["secret_leak_count"],
        },
    }


def check_gap_analysis(fixture: dict[str, Any]) -> dict[str, Any]:
    validate_fixture(fixture)
    perfect_scorecard = score_benchmark(fixture, synthetic_case_runs(fixture, mode="perfect"))
    faulted_scorecard = score_benchmark(fixture, synthetic_case_runs(fixture, mode="faulted"))
    perfect_analysis = build_gap_analysis(fixture, perfect_scorecard)
    faulted_analysis = build_gap_analysis(fixture, faulted_scorecard)
    faulted_capabilities = {gap["capability"] for gap in faulted_analysis["top_gaps"]}
    expected_faulted_capabilities = {"relation_mapping", "multi_hop_context", "safety", "provenance"}
    ok = (
        perfect_analysis["readiness_status"] == "v1_ontology_ready"
        and faulted_analysis["readiness_status"] == "v1_ontology_design_needed"
        and expected_faulted_capabilities.issubset(faulted_capabilities)
        and faulted_analysis["public_v1_gate"]["status"] == "blocked"
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "v1-ontology-benchmark-gap-analysis",
        "benchmark_id": fixture["benchmark_id"],
        "ok": ok,
        "perfect": {
            "readiness_status": perfect_analysis["readiness_status"],
            "top_gaps": perfect_analysis["top_gaps"],
            "public_v1_gate": perfect_analysis["public_v1_gate"],
        },
        "faulted": {
            "readiness_status": faulted_analysis["readiness_status"],
            "top_gaps": faulted_analysis["top_gaps"],
            "recommended_implementation_order": faulted_analysis["recommended_implementation_order"],
            "public_v1_gate": faulted_analysis["public_v1_gate"],
        },
        "expected_faulted_capabilities": sorted(expected_faulted_capabilities),
    }


def synthetic_case_runs(fixture: dict[str, Any], *, mode: str) -> list[dict[str, Any]]:
    runs = []
    for case in fixture["cases"]:
        expected = case["expected"]
        entity_by_id = {entity["id"]: entity for entity in expected.get("entities", [])}
        claims = []
        claim_number = 1
        for relation in expected.get("relations", []):
            claims.append(
                synthetic_relation_claim(
                    claim_number,
                    case,
                    entity_by_id,
                    relation,
                    relation["scope"],
                    relation.get("status", "active"),
                )
            )
            claim_number += 1
        for attribute in expected.get("attributes", []):
            claims.append(
                {
                    "id": f"claim-{claim_number:03d}",
                    "subject": entity_by_id[attribute["entity"]]["name"],
                    "predicate": attribute["key"],
                    "object": attribute["value"],
                    "status": attribute.get("status", "active"),
                    "scope": attribute["scope"],
                    "source_event_ids": ["event-001"],
                }
            )
            claim_number += 1
        if mode == "faulted" and claims:
            claims = claims[1:]
            if claims:
                claims[0]["source_event_ids"] = []
        context_reports = []
        for check in expected.get("context_checks", []):
            items = []
            if mode == "perfect":
                for relation_id in check.get("must_include_relation_ids", []):
                    relation = find_relation(expected, relation_id)
                    items.append(
                        {
                            "claim_id": f"synthetic-{relation_id}",
                            "claim_text": relation_display_text(case, entity_by_id, relation),
                            "source_event_ids": ["event-001"],
                            "score": 100,
                            "matched_terms": [],
                            "match_reason": "synthetic-perfect",
                        }
                    )
            elif case["id"] == "natural-secret-exclusion":
                items.append(
                    {
                        "claim_id": "synthetic-secret-leak",
                        "claim_text": "fake setup value contains API_KEY=FAKE_ONTOLOGY_VALUE_01",
                        "source_event_ids": ["event-001"],
                        "score": 100,
                        "matched_terms": [],
                        "match_reason": "synthetic-fault",
                    }
                )
            context_reports.append(
                {
                    "check_id": check["id"],
                    "report": {
                        "context_pack": {
                            "items": items,
                            "omitted": [],
                        },
                        "item_count": len(items),
                    },
                }
            )
        runs.append(
            {
                "case_id": case["id"],
                "category": case["category"],
                "input_style": case["input_style"],
                "snapshot": {"claims": claims},
                "context_reports": context_reports,
            }
        )
    return runs


def synthetic_relation_claim(
    number: int,
    case: dict[str, Any],
    entity_by_id: dict[str, dict[str, Any]],
    relation: dict[str, Any],
    scope: str,
    status: str,
) -> dict[str, Any]:
    subject = relation.get("subject_text") or entity_by_id[relation["subject"]]["name"]
    object_value = relation.get("object_text") or entity_by_id[relation["object"]]["name"]
    return {
        "id": f"claim-{number:03d}",
        "subject": subject,
        "predicate": relation["predicate"],
        "object": object_value,
        "status": status,
        "scope": scope,
        "source_event_ids": ["event-001"],
    }


def find_relation(expected: dict[str, Any], relation_id: str) -> dict[str, Any]:
    for relation in expected.get("relations", []) + expected.get("must_not_relations", []):
        if relation["id"] == relation_id:
            return relation
    raise BenchmarkFailure(f"unknown relation id: {relation_id}")


def normalize(value: str) -> str:
    lowered = value.strip().lower()
    lowered = re.sub(r"[^a-z0-9]+", " ", lowered)
    return re.sub(r"\s+", " ", lowered).strip()


def ratio(numerator: int, denominator: int) -> float:
    if denominator == 0:
        return 0.0
    return numerator / denominator


def f1(precision: float, recall: float) -> float:
    if precision + recall == 0:
        return 0.0
    return 2 * precision * recall / (precision + recall)


def write_markdown_report(path: Path, summary: dict[str, Any]) -> None:
    scorecard = summary.get("scorecard") or {}
    gap_analysis = summary.get("gap_analysis") or {}
    lines = [
        "# V1 Ontology Benchmark Report",
        "",
        f"- Decision: `{summary.get('decision_status')}`",
        f"- Readiness: `{gap_analysis.get('readiness_status')}`",
        f"- Benchmark: `{summary.get('benchmark_id')}`",
        f"- Cases: `{(summary.get('fixture') or {}).get('case_count')}`",
        "",
        "## Scorecard",
        "",
    ]
    for key in [
        "entity_f1",
        "relation_f1",
        "attribute_f1",
        "scope_accuracy",
        "temporal_correctness",
        "provenance_coverage",
        "context_recall_at_k",
        "context_precision_at_k",
        "scope_leak_count",
        "secret_leak_count",
        "prohibited_relation_count",
    ]:
        lines.append(f"- `{key}`: `{scorecard.get(key)}`")
    lines.extend(["", "## Top Gaps", ""])
    for gap in gap_analysis.get("top_gaps", []):
        lines.append(
            f"- `{gap['capability']}`: severity `{gap['severity']}`, "
            f"health `{gap['health']}`, missed `{gap['missed']}`"
        )
    lines.extend(["", "## Next Actions", ""])
    for action in summary.get("recommended_next_actions", []):
        lines.append(f"- {action}")
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_gap_analysis_markdown(path: Path, analysis: dict[str, Any]) -> None:
    lines = [
        "# V1 Ontology Gap Analysis",
        "",
        f"- Readiness: `{analysis.get('readiness_status')}`",
        f"- Decision: `{analysis.get('decision_status')}`",
        f"- Public v1 gate: `{(analysis.get('public_v1_gate') or {}).get('status')}`",
        "",
        "## Capability Gaps",
        "",
    ]
    for capability in analysis.get("capabilities", []):
        lines.append(
            f"- `{capability['capability']}`: severity `{capability['severity']}`, "
            f"health `{capability['health']}`, missed `{capability['missed']}`, "
            f"violations `{capability['violations']}`"
        )
    lines.extend(["", "## Recommended Implementation Order", ""])
    for index, item in enumerate(analysis.get("recommended_implementation_order", []), start=1):
        cases = ", ".join(item.get("affected_cases", [])) or "none"
        lines.append(f"{index}. `{item['capability']}` ({item['severity']}): {item['reason']} Cases: {cases}.")
    lines.extend(["", "## Target Misses", ""])
    for miss in analysis.get("target_misses", []):
        lines.append(
            f"- `{miss['metric']}`: value `{miss['value']}`, "
            f"target `{miss['target']}`, gap `{miss['gap']}`"
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def write_html_report(path: Path, summary: dict[str, Any]) -> None:
    scorecard = summary.get("scorecard") or {}
    gap_analysis = summary.get("gap_analysis") or {}
    rows = []
    for key in [
        "entity_f1",
        "relation_f1",
        "attribute_f1",
        "scope_accuracy",
        "temporal_correctness",
        "provenance_coverage",
        "context_recall_at_k",
        "context_precision_at_k",
        "scope_leak_count",
        "secret_leak_count",
    ]:
        rows.append(
            f"<tr><th>{html.escape(key)}</th><td><code>{html.escape(str(scorecard.get(key)))}</code></td></tr>"
        )
    gap_items = "".join(
        (
            "<li><code>"
            + html.escape(gap["capability"])
            + "</code>: "
            + html.escape(gap["severity"])
            + ", health <code>"
            + html.escape(str(gap["health"]))
            + "</code>, missed <code>"
            + html.escape(str(gap["missed"]))
            + "</code></li>"
        )
        for gap in gap_analysis.get("top_gaps", [])
    )
    content = f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <title>Mneme v1 Ontology Benchmark</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 2rem; max-width: 960px; }}
    table {{ border-collapse: collapse; width: 100%; }}
    th, td {{ border: 1px solid #ddd; padding: 0.5rem; text-align: left; }}
  </style>
</head>
<body>
  <h1>Mneme v1 Ontology Benchmark</h1>
  <p>Decision: <code>{html.escape(str(summary.get('decision_status')))}</code></p>
  <p>Readiness: <code>{html.escape(str(gap_analysis.get('readiness_status')))}</code></p>
  <table>{''.join(rows)}</table>
  <h2>Top Gaps</h2>
  <ul>{gap_items}</ul>
</body>
</html>
"""
    path.write_text(content, encoding="utf-8")


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def redact_paths(argv: list[str]) -> list[str]:
    root = str(ROOT)
    return [arg.replace(root, "<repo>") for arg in argv]


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run the v1 natural-language and complex-ontology benchmark."
    )
    parser.add_argument("--fixture", default=str(DEFAULT_FIXTURE), help="Benchmark fixture JSON.")
    parser.add_argument("--run-label", help="Run label for evals/runs/v1-ontology-benchmark.")
    parser.add_argument("--out-dir", help="Explicit output directory for the benchmark bundle.")
    parser.add_argument("--force", action="store_true", help="Replace an existing output directory.")
    parser.add_argument("--no-build", action="store_true", help="Do not build mneme-cli before running.")
    parser.add_argument("--check-contract", action="store_true", help="Print benchmark contract and exit.")
    parser.add_argument("--check-fixture", action="store_true", help="Validate and summarize the fixture.")
    parser.add_argument("--check-scorer", action="store_true", help="Validate scorer fault detection.")
    parser.add_argument("--check-gap-analysis", action="store_true", help="Validate capability gap analysis.")
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.check_contract:
        print(json.dumps(contract(), ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    fixture = load_fixture(Path(args.fixture))
    if args.check_fixture:
        print(json.dumps(validate_fixture(fixture), ensure_ascii=False, indent=2, sort_keys=True))
        return 0
    if args.check_scorer:
        report = check_scorer(fixture)
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if report["ok"] else 1
    if args.check_gap_analysis:
        report = check_gap_analysis(fixture)
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return 0 if report["ok"] else 1

    run = BenchmarkRun(args, fixture)
    try:
        run.prepare()
        run.build()
        run.run()
        summary = run.write_summary("passed")
        print(f"v1-ontology-benchmark: wrote {run.out_dir}")
        print(f"v1-ontology-benchmark: summary {run.out_dir / 'summary.json'}")
        print(f"v1-ontology-benchmark: decision {summary['decision_status']}")
        return 0
    except Exception as exc:  # noqa: BLE001 - always try to write a local summary.
        try:
            run.write_summary("failed", error=str(exc))
        except Exception:
            pass
        print(f"v1-ontology-benchmark: failed: {exc}", file=sys.stderr)
        print(f"v1-ontology-benchmark: summary {run.out_dir / 'summary.json'}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
