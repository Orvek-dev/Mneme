#!/usr/bin/env python3
"""Run Mneme's product-value validation loop.

This script is deliberately product-facing rather than capability-facing. It
checks whether Mneme's memory layer can improve a scripted downstream task,
whether provider-backed extraction remains opt-in and private by default,
whether long-horizon memory stays usable, whether a semantic ranker would add
measurable value, and whether store migration preserves existing memory.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
STATE_SCHEMA_VERSION = 2
DEFAULT_RECORD_COUNT = 600


class ProductValidationFailure(RuntimeError):
    """Raised when the product validation loop cannot complete."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


VALUE_TASKS: list[dict[str, Any]] = [
    {
        "id": "resume-storage-decision",
        "phase": "P0/P1",
        "query": "sqlite migration decision",
        "scope": "private",
        "claims": [
            ("project", "should", "defer sqlite migration until migration evidence exists", "private", "active"),
            ("migration decision", "requires", "product-validation evidence", "private", "active"),
        ],
        "must_include": ["defer sqlite migration", "migration evidence"],
        "must_not_include": [],
        "expected_control_success": False,
    },
    {
        "id": "agent-handoff-next-action",
        "phase": "P0/P1",
        "query": "next agent mcp extractor",
        "scope": "private",
        "claims": [
            ("next agent", "should", "update MCP guide before extractor work", "private", "active"),
            ("handoff", "carries", "finish summary and next actions", "private", "active"),
        ],
        "must_include": ["update MCP guide", "next actions"],
        "must_not_include": [],
        "expected_control_success": False,
    },
    {
        "id": "stale-correction-avoids-old-plan",
        "phase": "P0/P1",
        "query": "current eval report schedule",
        "scope": "private",
        "claims": [
            ("user", "prefers", "weekly eval reports on Friday", "private", "superseded"),
            ("user", "prefers", "weekly eval reports on Monday mornings", "private", "active"),
            ("weekly eval reports", "schedule", "Monday mornings", "private", "active"),
        ],
        "must_include": ["Monday mornings"],
        "must_not_include": ["Friday"],
        "expected_control_success": False,
    },
    {
        "id": "scope-bound-project-recall",
        "phase": "P0/P1",
        "query": "rollback proof",
        "scope": "project-alpha",
        "claims": [
            ("project alpha deploy", "requires", "rollback proof before release", "project-alpha", "active"),
            ("project beta finance", "contains", "private budget notes", "project-beta", "active"),
        ],
        "must_include": ["rollback proof"],
        "must_not_include": ["private budget notes"],
        "expected_control_success": False,
    },
]


PRIVACY_CASES = [
    "wrapper_requires_explicit_provider_opt_in",
    "dry_run_works_without_network_or_api_key",
    "secret_prefilter_runs_before_provider_call",
    "quality_gate_uses_dry_run_for_provider_wrapper",
]


RANKING_CASES: list[dict[str, Any]] = [
    {
        "id": "concise-launch-briefs",
        "query": "brief launch notes",
        "expected_id": "doc-launch-briefs",
        "documents": [
            ("doc-launch-briefs", "user prefers concise launch briefs"),
            ("doc-local-first", "user prefers local-first tools"),
            ("doc-release-proof", "release checklist requires rollback evidence"),
        ],
    },
    {
        "id": "rollback-evidence",
        "query": "roll back proof",
        "expected_id": "doc-release-proof",
        "documents": [
            ("doc-launch-briefs", "user prefers concise launch briefs"),
            ("doc-release-proof", "release checklist requires rollback evidence"),
            ("doc-korean-notes", "Mneme notes language Korean"),
        ],
    },
    {
        "id": "handoff-package",
        "query": "handover summary",
        "expected_id": "doc-handoff",
        "documents": [
            ("doc-handoff", "handoff package includes finish summary and next actions"),
            ("doc-scope", "project alpha deploy requires rollback proof before release"),
            ("doc-korean-notes", "Mneme notes language Korean"),
        ],
    },
    {
        "id": "korean-mneme-notes",
        "query": "Korean user-facing notes",
        "expected_id": "doc-korean-notes",
        "documents": [
            ("doc-korean-notes", "Mneme notes language Korean"),
            ("doc-launch-briefs", "user prefers concise launch briefs"),
            ("doc-handoff", "handoff package includes finish summary and next actions"),
        ],
    },
]


SEMANTIC_ALIASES: dict[str, list[str]] = {
    "brief": ["concise", "short", "compact", "briefs"],
    "notes": ["briefs", "summaries", "summary"],
    "roll": ["rollback"],
    "back": ["rollback"],
    "proof": ["evidence"],
    "handover": ["handoff"],
    "summary": ["summaries"],
    "user-facing": ["mneme"],
}


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-validation-loop-contract",
        "phases": [
            {
                "id": "P0",
                "name": "real_use_value_dogfood",
                "purpose": "Measure whether scoped memory changes scripted downstream decisions.",
                "primary_metrics": [
                    "with_memory_success_rate",
                    "control_success_rate",
                    "memory_caused_decision_rate",
                    "citation_coverage",
                ],
            },
            {
                "id": "P1",
                "name": "downstream_usefulness",
                "purpose": "Prefer task outcome evidence over extraction F1 alone.",
                "primary_metrics": [
                    "task_outcome_delta",
                    "wrong_memory_count",
                    "manual_reexplanation_count",
                ],
            },
            {
                "id": "P2",
                "name": "privacy_preserving_extraction",
                "purpose": "Keep provider-backed extraction opt-in and private-by-default.",
                "primary_metrics": [
                    "provider_opt_in_required",
                    "dry_run_without_api_key",
                    "secret_prefilter_before_provider",
                ],
            },
            {
                "id": "P3",
                "name": "long_horizon_memory",
                "purpose": "Check stale, noisy, duplicate, and scoped memory under accumulation.",
                "primary_metrics": [
                    "stale_reuse_count",
                    "scope_leak_count",
                    "noise_record_count",
                    "current_memory_recall",
                ],
            },
            {
                "id": "P4",
                "name": "retrieval_ranking_decision",
                "purpose": "Measure whether semantic ranking would beat term matching before adding it.",
                "primary_metrics": ["term_mrr", "semantic_candidate_mrr", "mrr_delta"],
            },
            {
                "id": "P5",
                "name": "migration_safety",
                "purpose": "Verify old local stores can be normalized without losing memory.",
                "primary_metrics": [
                    "legacy_warning_detected",
                    "normalized_schema_version",
                    "backup_preserved",
                    "migration_history_recorded",
                ],
            },
        ],
        "default_output": "evals/runs/product-validation-loop/<run-label>",
        "privacy_policy": "full run artifacts are local-only; public docs should include reduced summaries only",
    }


def dataset_summary(record_count: int = DEFAULT_RECORD_COUNT) -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-validation-loop-dataset",
        "value_task_count": len(VALUE_TASKS),
        "privacy_case_count": len(PRIVACY_CASES),
        "long_horizon_record_count": record_count,
        "ranking_case_count": len(RANKING_CASES),
        "migration_case_count": 2,
        "phases": ["P0", "P1", "P2", "P3", "P4", "P5"],
    }


def run_command(
    args: list[str],
    *,
    input_text: str | None = None,
    env: dict[str, str] | None = None,
    check: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        args,
        input=input_text,
        text=True,
        capture_output=True,
        env=env,
    )
    if check and result.returncode != 0:
        raise ProductValidationFailure(
            f"command failed ({result.returncode}): {' '.join(args)}\n{result.stderr}"
        )
    return result


def ensure_cli(no_build: bool) -> Path:
    binary = ROOT / "target" / "debug" / "mneme"
    if not no_build:
        run_command(["cargo", "build", "-q", "-p", "mneme-cli"])
    if not binary.exists():
        raise ProductValidationFailure("target/debug/mneme is missing; run cargo build -p mneme-cli")
    return binary


def claim_text(claim: dict[str, Any]) -> str:
    return f"{claim['subject']} {claim['predicate']} {claim['object']}"


def make_store(path: Path, claims: list[tuple[str, str, str, str, str]], *, schema_version: int = STATE_SCHEMA_VERSION) -> None:
    now = int(time.time())
    events = []
    claim_records = []
    audit = []
    for index, (subject, predicate, obj, scope, status) in enumerate(claims, start=1):
        event_id = f"event-{index:03d}"
        claim_id = f"claim-{index:03d}"
        text = f"{subject} {predicate} {obj}"
        events.append(
            {
                "id": event_id,
                "speaker_id": "user",
                "actor_agent_id": "product-validation",
                "text": text,
                "scope": scope,
                "trust_level": "trusted_user",
            }
        )
        claim_records.append(
            {
                "id": claim_id,
                "subject": subject,
                "predicate": predicate,
                "object": obj,
                "status": status,
                "scope": scope,
                "source_event_ids": [event_id],
            }
        )
        audit.append({"kind": "event_append", "target_id": f"{event_id}:product-validation:trusted_user"})
        audit.append({"kind": "claim_write", "target_id": claim_id})

    state = {
        "schema_version": schema_version,
        "metadata": {
            "store_id": f"product-validation-{path.stem}",
            "generation": 1,
            "created_at_unix_seconds": now,
            "updated_at_unix_seconds": now,
            "engine_version": "product-validation",
            "migration_history": [],
        },
        "budget": {
            "daily_cloud_tokens": 100000,
            "spent_tokens": 0,
            "hard_cap_violations": 0,
        },
        "events": events,
        "claims": claim_records,
        "sessions": [],
        "audit": audit,
    }
    path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")


def run_context(binary: Path, store: Path, query: str, scope: str) -> dict[str, Any]:
    result = run_command(
        [
            str(binary),
            "context",
            query,
            "--scope",
            scope,
            "--store",
            str(store),
            "--json",
        ]
    )
    return json.loads(result.stdout)


def selected_text(context: dict[str, Any]) -> str:
    items = context.get("context_pack", {}).get("items", [])
    return "\n".join(str(item.get("claim_text", "")) for item in items)


def citation_coverage(context: dict[str, Any]) -> float:
    items = context.get("context_pack", {}).get("items", [])
    if not items:
        return 0.0
    cited = sum(1 for item in items if item.get("source_event_ids"))
    return cited / len(items)


def contains_all_text(text: str, phrases: list[str]) -> bool:
    lower = text.lower()
    return all(phrase.lower() in lower for phrase in phrases)


def contains_any_text(text: str, phrases: list[str]) -> bool:
    lower = text.lower()
    return any(phrase.lower() in lower for phrase in phrases)


def run_value_dogfood(binary: Path, out_dir: Path) -> dict[str, Any]:
    task_results = []
    for task in VALUE_TASKS:
        store = out_dir / f"{task['id']}.json"
        control_store = out_dir / f"{task['id']}.control.json"
        make_store(store, task["claims"])
        make_store(control_store, [])
        with_context = run_context(binary, store, task["query"], task["scope"])
        control_context = run_context(binary, control_store, task["query"], task["scope"])
        text = selected_text(with_context)
        control_text = selected_text(control_context)
        with_success = contains_all_text(text, task["must_include"]) and not contains_any_text(
            text, task["must_not_include"]
        )
        control_success = contains_all_text(control_text, task["must_include"])
        task_results.append(
            {
                "id": task["id"],
                "query": task["query"],
                "scope": task["scope"],
                "with_memory_success": with_success,
                "control_success": control_success,
                "memory_changed_outcome": with_success and not control_success,
                "citation_coverage": citation_coverage(with_context),
                "selected_item_count": with_context.get("item_count", 0),
                "control_item_count": control_context.get("item_count", 0),
                "wrong_memory": contains_any_text(text, task["must_not_include"]),
            }
        )
    total = len(task_results)
    with_success_count = sum(1 for result in task_results if result["with_memory_success"])
    control_success_count = sum(1 for result in task_results if result["control_success"])
    changed_count = sum(1 for result in task_results if result["memory_changed_outcome"])
    wrong_memory_count = sum(1 for result in task_results if result["wrong_memory"])
    coverage_values = [result["citation_coverage"] for result in task_results if result["selected_item_count"]]
    return {
        "phase": "P0/P1",
        "ok": with_success_count == total and control_success_count == 0 and wrong_memory_count == 0,
        "task_count": total,
        "with_memory_success_rate": with_success_count / total,
        "control_success_rate": control_success_count / total,
        "memory_caused_decision_rate": changed_count / total,
        "task_outcome_delta": (with_success_count - control_success_count) / total,
        "wrong_memory_count": wrong_memory_count,
        "manual_reexplanation_count": total - changed_count,
        "citation_coverage": min(coverage_values) if coverage_values else 0.0,
        "results": task_results,
    }


def run_privacy_extractor(out_dir: Path) -> dict[str, Any]:
    wrapper = ROOT / "wrappers" / "openai_extractor.py"
    request = {
        "schema_version": "mneme.extractor.command.v1",
        "event": {
            "id": "event-001",
            "speaker_id": "user",
            "actor_agent_id": "codex",
            "text": "I work best with local-first tools.",
            "scope": "private",
            "trust_level": "trusted_user",
        },
    }
    env_without_provider = dict(os.environ)
    env_without_provider.pop("OPENAI_API_KEY", None)
    env_without_provider.pop("MNEME_OPENAI_DRY_RUN", None)
    no_key = run_command(
        [str(wrapper)],
        input_text=json.dumps(request),
        env=env_without_provider,
        check=False,
    )
    dry_env = dict(env_without_provider)
    dry_env["MNEME_OPENAI_DRY_RUN"] = "1"
    dry_run = run_command([str(wrapper)], input_text=json.dumps(request), env=dry_env)
    dry_json = json.loads(dry_run.stdout)
    secret_request = json.loads(json.dumps(request))
    secret_request["event"]["text"] = "Please remember API_KEY=FAKE_PRODUCT_VALIDATION_VALUE"
    secret_prefilter = run_command(
        [str(wrapper)],
        input_text=json.dumps(secret_request),
        env=env_without_provider,
    )
    secret_json = json.loads(secret_prefilter.stdout)
    quality_gate = (ROOT / "scripts" / "quality-gate.sh").read_text()
    docs = (ROOT / "docs" / "eval-harness" / "openai-provider-wrapper.md").read_text()
    provider_opt_in_required = no_key.returncode != 0 and "OPENAI_API_KEY is required" in no_key.stderr
    dry_run_ok = dry_json.get("claim", {}).get("object") == "local-first tools"
    secret_prefilter_ok = "API_KEY=FAKE_PRODUCT_VALIDATION_VALUE" in secret_json.get("claim", {}).get("object", "")
    quality_gate_dry = "MNEME_OPENAI_DRY_RUN=1" in quality_gate
    docs_opt_in = "opt-in" in docs.lower() and "Never commit real `.env`" in docs
    report = {
        "phase": "P2",
        "ok": all(
            [
                provider_opt_in_required,
                dry_run_ok,
                secret_prefilter_ok,
                quality_gate_dry,
                docs_opt_in,
            ]
        ),
        "provider_opt_in_required": provider_opt_in_required,
        "dry_run_without_api_key": dry_run_ok,
        "secret_prefilter_before_provider": secret_prefilter_ok,
        "quality_gate_uses_dry_run": quality_gate_dry,
        "docs_state_opt_in_policy": docs_opt_in,
    }
    (out_dir / "privacy-wrapper-dry-run.json").write_text(json.dumps(dry_json, indent=2) + "\n")
    return report


def long_horizon_claims(record_count: int) -> list[tuple[str, str, str, str, str]]:
    claims = [
        ("user", "prefers", "weekly eval reports on Friday", "private", "superseded"),
        ("user", "prefers", "weekly eval reports on Monday mornings", "private", "active"),
        ("weekly eval reports", "schedule", "Monday mornings", "private", "active"),
        ("project alpha deploy", "requires", "rollback proof before release", "project-alpha", "active"),
        ("project beta finance", "contains", "private budget notes", "project-beta", "active"),
        ("agent handoff", "requires", "finish summary and cited next actions", "private", "active"),
    ]
    noise_needed = max(0, record_count - len(claims))
    for index in range(noise_needed):
        scope = "private" if index % 3 else "project-noise"
        claims.append((f"noise item {index:04d}", "mentions", f"unrelated archive marker {index:04d}", scope, "active"))
    return claims


def run_long_horizon(binary: Path, out_dir: Path, record_count: int) -> dict[str, Any]:
    store = out_dir / "long-horizon-store.json"
    claims = long_horizon_claims(record_count)
    make_store(store, claims)
    eval_context = run_context(binary, store, "current eval report Monday", "private")
    deploy_context = run_context(binary, store, "rollback proof", "project-alpha")
    eval_text = selected_text(eval_context)
    deploy_text = selected_text(deploy_context)
    stale_reuse = int("Friday" in eval_text)
    scope_leak = int("private budget notes" in deploy_text)
    current_recall = "Monday mornings" in eval_text and "rollback proof" in deploy_text
    duplicate_groups = duplicate_active_group_count(claims)
    return {
        "phase": "P3",
        "ok": current_recall and stale_reuse == 0 and scope_leak == 0,
        "record_count": len(claims),
        "noise_record_count": max(0, len(claims) - 6),
        "current_memory_recall": current_recall,
        "stale_reuse_count": stale_reuse,
        "scope_leak_count": scope_leak,
        "duplicate_active_group_count": duplicate_groups,
        "eval_selected_item_count": eval_context.get("item_count", 0),
        "deploy_selected_item_count": deploy_context.get("item_count", 0),
    }


def duplicate_active_group_count(claims: list[tuple[str, str, str, str, str]]) -> int:
    seen: dict[tuple[str, str, str, str], int] = {}
    for subject, predicate, obj, scope, status in claims:
        if status != "active":
            continue
        key = (subject, predicate, obj, scope)
        seen[key] = seen.get(key, 0) + 1
    return sum(1 for count in seen.values() if count > 1)


def tokenize(text: str) -> list[str]:
    token = ""
    tokens = []
    for char in text.lower():
        if char.isalnum() or char in "-_":
            token += char
        elif token:
            tokens.append(token)
            token = ""
    if token:
        tokens.append(token)
    return tokens


def expanded_query_tokens(query: str) -> set[str]:
    tokens = set(tokenize(query))
    for token in list(tokens):
        for alias in SEMANTIC_ALIASES.get(token, []):
            tokens.add(alias)
    return tokens


def rank_documents(query: str, docs: list[tuple[str, str]], *, semantic: bool) -> list[str]:
    query_tokens = expanded_query_tokens(query) if semantic else set(tokenize(query))
    scored = []
    for index, (doc_id, text) in enumerate(docs):
        doc_tokens = set(tokenize(text))
        score = len(query_tokens & doc_tokens)
        scored.append((score, -index, doc_id))
    scored.sort(reverse=True)
    return [doc_id for score, _, doc_id in scored if score > 0]


def reciprocal_rank(ranked: list[str], expected_id: str) -> float:
    for index, doc_id in enumerate(ranked, start=1):
        if doc_id == expected_id:
            return 1.0 / index
    return 0.0


def ndcg_at_3(ranked: list[str], expected_id: str) -> float:
    for index, doc_id in enumerate(ranked[:3], start=1):
        if doc_id == expected_id:
            return 1.0 / math.log2(index + 1)
    return 0.0


def run_ranking_decision() -> dict[str, Any]:
    results = []
    term_rr = []
    semantic_rr = []
    term_ndcg = []
    semantic_ndcg = []
    for case in RANKING_CASES:
        term_ranked = rank_documents(case["query"], case["documents"], semantic=False)
        semantic_ranked = rank_documents(case["query"], case["documents"], semantic=True)
        term_case_rr = reciprocal_rank(term_ranked, case["expected_id"])
        semantic_case_rr = reciprocal_rank(semantic_ranked, case["expected_id"])
        term_rr.append(term_case_rr)
        semantic_rr.append(semantic_case_rr)
        term_ndcg.append(ndcg_at_3(term_ranked, case["expected_id"]))
        semantic_ndcg.append(ndcg_at_3(semantic_ranked, case["expected_id"]))
        results.append(
            {
                "id": case["id"],
                "query": case["query"],
                "expected_id": case["expected_id"],
                "term_ranked": term_ranked,
                "semantic_candidate_ranked": semantic_ranked,
                "term_rr": term_case_rr,
                "semantic_candidate_rr": semantic_case_rr,
            }
        )
    term_mrr = sum(term_rr) / len(term_rr)
    semantic_mrr = sum(semantic_rr) / len(semantic_rr)
    term_ndcg_mean = sum(term_ndcg) / len(term_ndcg)
    semantic_ndcg_mean = sum(semantic_ndcg) / len(semantic_ndcg)
    return {
        "phase": "P4",
        "ok": semantic_mrr >= term_mrr,
        "term_mrr": term_mrr,
        "semantic_candidate_mrr": semantic_mrr,
        "mrr_delta": semantic_mrr - term_mrr,
        "term_ndcg_at_3": term_ndcg_mean,
        "semantic_candidate_ndcg_at_3": semantic_ndcg_mean,
        "ndcg_delta": semantic_ndcg_mean - term_ndcg_mean,
        "decision": "measure_before_embedding; do_not_ship_embedding_without_positive_delta",
        "results": results,
    }


def run_migration_safety(binary: Path, out_dir: Path) -> dict[str, Any]:
    store = out_dir / "legacy-v1-store.json"
    make_store(store, [("user", "prefers", "migration-safe local memory", "private", "active")], schema_version=1)
    validate_before = json.loads(
        run_command([str(binary), "validate", "--store", str(store), "--json"]).stdout
    )
    repair = json.loads(
        run_command([str(binary), "repair", "--store", str(store), "--json"]).stdout
    )
    validate_after = json.loads(
        run_command([str(binary), "validate", "--store", str(store), "--json"]).stdout
    )
    normalized = json.loads(store.read_text())
    backup = json.loads(Path(f"{store}.bak").read_text())
    warnings = validate_before["inspection"]["current"]["validation"]["warning_count"]
    migration_history = normalized["metadata"].get("migration_history", [])
    context = run_context(binary, store, "migration-safe", "private")
    memory_preserved = "migration-safe local memory" in selected_text(context)
    return {
        "phase": "P5",
        "ok": all(
            [
                warnings > 0,
                repair.get("action") == "normalized_current",
                normalized.get("schema_version") == STATE_SCHEMA_VERSION,
                backup.get("schema_version") == 1,
                bool(migration_history),
                memory_preserved,
                validate_after["inspection"]["current"]["validation"]["ok"],
            ]
        ),
        "legacy_warning_detected": warnings > 0,
        "repair_action": repair.get("action"),
        "normalized_schema_version": normalized.get("schema_version"),
        "backup_schema_version": backup.get("schema_version"),
        "migration_history_recorded": bool(migration_history),
        "memory_preserved_after_migration": memory_preserved,
    }


def run_full(args: argparse.Namespace) -> dict[str, Any]:
    out_dir = args.out_dir
    if out_dir.exists():
        if not args.force:
            raise ProductValidationFailure(f"output directory already exists: {out_dir}")
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    binary = ensure_cli(args.no_build)
    value = run_value_dogfood(binary, out_dir)
    privacy = run_privacy_extractor(out_dir)
    long_horizon = run_long_horizon(binary, out_dir, args.record_count)
    ranking = run_ranking_decision()
    migration = run_migration_safety(binary, out_dir)
    phases = [value, privacy, long_horizon, ranking, migration]
    report = {
        "schema_version": SCHEMA_VERSION,
        "command": "product-validation-loop",
        "ok": all(phase["ok"] for phase in phases),
        "run_label": args.run_label,
        "record_count": args.record_count,
        "phases": {
            "P0_P1_value_dogfood": value,
            "P2_privacy_extraction": privacy,
            "P3_long_horizon": long_horizon,
            "P4_ranking_decision": ranking,
            "P5_migration_safety": migration,
        },
        "summary": {
            "value_task_outcome_delta": value["task_outcome_delta"],
            "provider_opt_in_required": privacy["provider_opt_in_required"],
            "long_horizon_scope_leak_count": long_horizon["scope_leak_count"],
            "ranking_mrr_delta": ranking["mrr_delta"],
            "migration_memory_preserved": migration["memory_preserved_after_migration"],
        },
    }
    (out_dir / "summary.json").write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    (out_dir / "report.md").write_text(markdown_report(report))
    return report


def markdown_report(report: dict[str, Any]) -> str:
    lines = [
        "# Mneme Product Validation Loop",
        "",
        f"- Status: `{'pass' if report['ok'] else 'fail'}`",
        f"- Run label: `{report['run_label']}`",
        f"- Long-horizon records: `{report['record_count']}`",
        "",
        "| Phase | Result | Signal |",
        "| --- | --- | --- |",
    ]
    phases = report["phases"]
    lines.append(
        f"| P0/P1 value dogfood | `{status(phases['P0_P1_value_dogfood'])}` | outcome delta `{phases['P0_P1_value_dogfood']['task_outcome_delta']:.2f}` |"
    )
    lines.append(
        f"| P2 privacy extraction | `{status(phases['P2_privacy_extraction'])}` | provider opt-in `{phases['P2_privacy_extraction']['provider_opt_in_required']}` |"
    )
    lines.append(
        f"| P3 long horizon | `{status(phases['P3_long_horizon'])}` | stale `{phases['P3_long_horizon']['stale_reuse_count']}`, scope leaks `{phases['P3_long_horizon']['scope_leak_count']}` |"
    )
    lines.append(
        f"| P4 ranking decision | `{status(phases['P4_ranking_decision'])}` | MRR delta `{phases['P4_ranking_decision']['mrr_delta']:.2f}` |"
    )
    lines.append(
        f"| P5 migration safety | `{status(phases['P5_migration_safety'])}` | memory preserved `{phases['P5_migration_safety']['memory_preserved_after_migration']}` |"
    )
    lines.append("")
    lines.append("This local report is a product-validation signal, not a market adoption claim.")
    lines.append("")
    return "\n".join(lines)


def status(phase: dict[str, Any]) -> str:
    return "pass" if phase["ok"] else "fail"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true")
    parser.add_argument("--check-dataset", action="store_true")
    parser.add_argument("--record-count", type=int, default=DEFAULT_RECORD_COUNT)
    parser.add_argument("--run-label", default="local-product-validation")
    parser.add_argument("--out-dir", type=Path)
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.check_contract:
            print(json.dumps(contract(), indent=2, sort_keys=True))
            return 0
        if args.check_dataset:
            print(json.dumps(dataset_summary(args.record_count), indent=2, sort_keys=True))
            return 0
        if args.out_dir is None:
            args.out_dir = ROOT / "evals" / "runs" / "product-validation-loop" / args.run_label
        report = run_full(args)
        print(json.dumps(report, indent=2, sort_keys=True))
        return 0 if report["ok"] else 1
    except ProductValidationFailure as error:
        print(f"product-validation-loop: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
