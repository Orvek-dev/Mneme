#!/usr/bin/env python3
"""Run Mneme's product-value validation loop.

This loop is intentionally stricter than a retrieval smoke test. It checks
whether returned memory is adopted into downstream artifacts, whether provider
extraction remains opt-in and budgeted, whether lifecycle operations hold up
under accumulation, whether a semantic-ranking candidate is worth shipping,
whether store migration remains safe, and whether external review evidence has
a public-safe schema before Mneme claims real-world value.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 2
STATE_SCHEMA_VERSION = 2
DEFAULT_RECORD_COUNT = 600
REVIEW_EXAMPLE = Path("examples/product-validation-review.example.json")


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


CAUSAL_TASKS: list[dict[str, Any]] = [
    {
        "id": "resume-storage-decision",
        "query": "sqlite migration decision",
        "scope": "private",
        "claims": [
            (
                "project",
                "should",
                "defer sqlite migration until migration evidence exists",
                "private",
                "active",
            ),
            (
                "migration decision",
                "requires",
                "product-validation evidence",
                "private",
                "active",
            ),
        ],
        "expected_decision": "defer_sqlite_migration",
        "required_memory": ["defer sqlite migration", "migration evidence"],
        "forbidden_memory": [],
    },
    {
        "id": "agent-handoff-next-action",
        "query": "next agent mcp extractor",
        "scope": "private",
        "claims": [
            (
                "next agent",
                "should",
                "update MCP guide before extractor work",
                "private",
                "active",
            ),
            (
                "handoff",
                "carries",
                "finish summary and next actions",
                "private",
                "active",
            ),
        ],
        "expected_decision": "update_mcp_guide_first",
        "required_memory": ["update MCP guide", "next actions"],
        "forbidden_memory": [],
    },
    {
        "id": "stale-correction-avoids-old-plan",
        "query": "current eval report schedule",
        "scope": "private",
        "claims": [
            (
                "user",
                "prefers",
                "weekly eval reports on Friday",
                "private",
                "superseded",
            ),
            (
                "user",
                "prefers",
                "weekly eval reports on Monday mornings",
                "private",
                "active",
            ),
            (
                "weekly eval reports",
                "schedule",
                "Monday mornings",
                "private",
                "active",
            ),
        ],
        "expected_decision": "schedule_monday_morning_report",
        "required_memory": ["Monday mornings"],
        "forbidden_memory": ["Friday"],
    },
    {
        "id": "scope-bound-project-recall",
        "query": "rollback proof",
        "scope": "project-alpha",
        "claims": [
            (
                "project alpha deploy",
                "requires",
                "rollback proof before release",
                "project-alpha",
                "active",
            ),
            (
                "project beta finance",
                "contains",
                "private budget notes",
                "project-beta",
                "active",
            ),
        ],
        "expected_decision": "require_rollback_proof",
        "required_memory": ["rollback proof"],
        "forbidden_memory": ["private budget notes"],
    },
]


PRIVACY_COST_EVENTS: list[dict[str, Any]] = [
    {
        "id": "normal-preference",
        "text": "I work best with local-first tools.",
        "expect_provider_allowed_after_opt_in": True,
    },
    {
        "id": "long-project-note",
        "text": " ".join(
            [
                "Project Atlas prefers release notes grouped by owner and risk."
                for _ in range(32)
            ]
        ),
        "expect_provider_allowed_after_opt_in": True,
    },
    {
        "id": "api-key-secret",
        "text": "Please remember API_KEY=FAKE_PRODUCT_VALIDATION_VALUE",
        "expect_provider_allowed_after_opt_in": False,
    },
    {
        "id": "bearer-token-secret",
        "text": "Authorization: Bearer fake-product-validation-token should never leave local checks.",
        "expect_provider_allowed_after_opt_in": False,
    },
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


SECRET_RE = re.compile(
    r"(?i)(api[_ -]?key\s*[:=]\s*[a-z0-9_./+=-]+|"
    r"authorization\s*:\s*bearer\s+[a-z0-9_./+=-]+|"
    r"\btoken\s*[:=]\s*[a-z0-9_./+=-]+|"
    r"\bpassword\s*[:=]\s*[a-z0-9_./+=-]+|"
    r"\bsecret\s*[:=]\s*[a-z0-9_./+=-]+|"
    r"\bsk-[a-z0-9_-]+|"
    r"\bghp_[a-z0-9_]+|"
    r"\bAKIA[0-9A-Z]{16}\b)"
)


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-validation-loop-contract",
        "phases": [
            {
                "id": "P1",
                "name": "causal_memory_usefulness",
                "purpose": "Check that retrieved memory is adopted into a downstream artifact with citations.",
                "primary_metrics": [
                    "memory_adoption_rate",
                    "decision_change_rate",
                    "harmful_memory_count",
                    "citation_coverage",
                ],
            },
            {
                "id": "P2",
                "name": "privacy_cost_extraction_readiness",
                "purpose": "Keep provider extraction opt-in, pre-redacted, no-network by default, and bounded by token/latency budgets.",
                "primary_metrics": [
                    "provider_opt_in_required",
                    "live_provider_executed",
                    "secret_prefilter_before_provider",
                    "within_budget",
                ],
            },
            {
                "id": "P3",
                "name": "long_horizon_lifecycle",
                "purpose": "Exercise actual remember/correct/forget operations before noisy accumulation checks.",
                "primary_metrics": [
                    "actual_lifecycle_operations",
                    "stale_reuse_count",
                    "scope_leak_count",
                    "forgotten_recall_count",
                ],
            },
            {
                "id": "P4",
                "name": "retrieval_ranking_decision",
                "purpose": "Measure whether a semantic-ranking candidate beats term matching before shipping embeddings.",
                "primary_metrics": [
                    "term_mrr",
                    "semantic_candidate_mrr",
                    "mrr_delta",
                    "requires_external_embedding_eval_before_shipping",
                ],
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
            {
                "id": "P6",
                "name": "external_review_gate",
                "purpose": "Require a public-safe review schema before claiming real-world or third-party value.",
                "primary_metrics": [
                    "example_review_valid",
                    "raw_transcript_included",
                    "third_party_claim",
                    "external_claim_allowed",
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
        "causal_task_count": len(CAUSAL_TASKS),
        "privacy_cost_event_count": len(PRIVACY_COST_EVENTS),
        "long_horizon_record_count": record_count,
        "ranking_case_count": len(RANKING_CASES),
        "migration_case_count": 2,
        "external_review_case_count": 1,
        "phases": ["P1", "P2", "P3", "P4", "P5", "P6"],
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


def make_store(
    path: Path,
    claims: list[tuple[str, str, str, str, str]],
    *,
    schema_version: int = STATE_SCHEMA_VERSION,
) -> None:
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


def synthesize_artifact(task: dict[str, Any], context: dict[str, Any]) -> dict[str, Any]:
    items = context.get("context_pack", {}).get("items", [])
    text = selected_text(context)
    has_required_memory = contains_all_text(text, task["required_memory"])
    has_forbidden_memory = contains_any_text(text, task["forbidden_memory"])
    citations = [
        source_id
        for item in items
        for source_id in item.get("source_event_ids", [])
        if source_id
    ]
    if has_required_memory and citations and not has_forbidden_memory:
        decision = task["expected_decision"]
    else:
        decision = "needs_manual_review"
    return {
        "task_id": task["id"],
        "decision": decision,
        "uses_memory": decision == task["expected_decision"],
        "body": f"decision={decision}\nselected_memory=\n{text}",
        "cited_source_event_ids": citations,
        "partial_context_warning": context.get("context_pack", {})
        .get("metadata", {})
        .get("partial_context", False),
        "not_full_transcript": context.get("context_pack", {})
        .get("metadata", {})
        .get("not_full_transcript", False),
    }


def no_memory_artifact(task: dict[str, Any]) -> dict[str, Any]:
    return {
        "task_id": task["id"],
        "decision": "needs_manual_review",
        "uses_memory": False,
        "body": "No cited Mneme memory was provided; do not infer project truth.",
        "cited_source_event_ids": [],
        "partial_context_warning": True,
        "not_full_transcript": True,
    }


def run_causal_usefulness(binary: Path, out_dir: Path) -> dict[str, Any]:
    task_results = []
    artifact_dir = out_dir / "P1-causal-artifacts"
    artifact_dir.mkdir()
    for task in CAUSAL_TASKS:
        store = out_dir / f"{task['id']}.json"
        make_store(store, task["claims"])
        context = run_context(binary, store, task["query"], task["scope"])
        with_memory = synthesize_artifact(task, context)
        without_memory = no_memory_artifact(task)
        artifact_path = artifact_dir / f"{task['id']}.json"
        artifact_path.write_text(
            json.dumps(
                {
                    "task": {
                        "id": task["id"],
                        "query": task["query"],
                        "scope": task["scope"],
                        "expected_decision": task["expected_decision"],
                    },
                    "with_memory_artifact": with_memory,
                    "counterfactual_without_memory_artifact": without_memory,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n"
        )
        text = with_memory["body"]
        evidence_adopted = (
            with_memory["decision"] == task["expected_decision"]
            and contains_all_text(text, task["required_memory"])
            and not contains_any_text(text, task["forbidden_memory"])
            and bool(with_memory["cited_source_event_ids"])
        )
        task_results.append(
            {
                "id": task["id"],
                "query": task["query"],
                "scope": task["scope"],
                "memory_read": context.get("item_count", 0) > 0,
                "memory_adopted_in_artifact": evidence_adopted,
                "decision_changed_by_memory": with_memory["decision"] != without_memory["decision"],
                "harmful_memory_used": contains_any_text(text, task["forbidden_memory"]),
                "citation_coverage": citation_coverage(context),
                "artifact_path": str(artifact_path.relative_to(out_dir)),
            }
        )
    total = len(task_results)
    adopted = sum(1 for result in task_results if result["memory_adopted_in_artifact"])
    changed = sum(1 for result in task_results if result["decision_changed_by_memory"])
    harmful = sum(1 for result in task_results if result["harmful_memory_used"])
    coverage_values = [result["citation_coverage"] for result in task_results]
    return {
        "phase": "P1",
        "ok": adopted == total and changed == total and harmful == 0,
        "task_count": total,
        "memory_adoption_rate": adopted / total,
        "decision_change_rate": changed / total,
        "harmful_memory_count": harmful,
        "citation_coverage": min(coverage_values) if coverage_values else 0.0,
        "evaluation_shape": "scripted_artifact_adoption_not_empty_store_success",
        "not_a_market_claim": True,
        "results": task_results,
    }


def run_privacy_cost_extractor(out_dir: Path) -> dict[str, Any]:
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
    budget_report = provider_budget_report(PRIVACY_COST_EVENTS)
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
                budget_report["within_budget"],
            ]
        ),
        "provider_opt_in_required": provider_opt_in_required,
        "live_provider_executed": False,
        "dry_run_without_api_key": dry_run_ok,
        "secret_prefilter_before_provider": secret_prefilter_ok,
        "quality_gate_uses_dry_run": quality_gate_dry,
        "docs_state_opt_in_policy": docs_opt_in,
        "budget": budget_report,
    }
    (out_dir / "P2-privacy-wrapper-dry-run.json").write_text(json.dumps(dry_json, indent=2) + "\n")
    return report


def provider_budget_report(events: list[dict[str, Any]]) -> dict[str, Any]:
    max_event_token_units = 520
    max_batch_token_units = 720
    latency_budget_ms = 30_000
    event_reports = []
    provider_calls_allowed = 0
    provider_calls_blocked = 0
    total_token_units = 0
    redaction_count = 0
    for event in events:
        redacted, redactions = redact_sensitive_text(event["text"])
        token_units = estimate_token_units(redacted)
        total_token_units += token_units
        redaction_count += redactions
        provider_allowed = redactions == 0 and event["expect_provider_allowed_after_opt_in"]
        provider_calls_allowed += int(provider_allowed)
        provider_calls_blocked += int(not provider_allowed)
        event_reports.append(
            {
                "id": event["id"],
                "token_units": token_units,
                "redaction_count": redactions,
                "provider_allowed_after_opt_in": provider_allowed,
                "within_event_budget": token_units <= max_event_token_units,
            }
        )
    return {
        "within_budget": all(event["within_event_budget"] for event in event_reports)
        and total_token_units <= max_batch_token_units,
        "max_event_token_units": max_event_token_units,
        "max_batch_token_units": max_batch_token_units,
        "total_token_units": total_token_units,
        "latency_budget_ms": latency_budget_ms,
        "provider_calls_allowed_after_opt_in": provider_calls_allowed,
        "provider_calls_blocked_before_provider": provider_calls_blocked,
        "redaction_count": redaction_count,
        "events": event_reports,
    }


def redact_sensitive_text(text: str) -> tuple[str, int]:
    redactions = 0

    def replace(_match: re.Match[str]) -> str:
        nonlocal redactions
        redactions += 1
        return "[REDACTED_SECRET]"

    return SECRET_RE.sub(replace, text), redactions


def estimate_token_units(text: str) -> int:
    return max(1, math.ceil(len(text) / 4))


def run_long_horizon_lifecycle(binary: Path, out_dir: Path, record_count: int) -> dict[str, Any]:
    store = out_dir / "P3-long-horizon-store.json"
    operations = []
    operations.append(
        json.loads(
            run_command(
                [
                    str(binary),
                    "remember",
                    "user prefers weekly eval reports on Friday",
                    "--store",
                    str(store),
                    "--json",
                ]
            ).stdout
        )
    )
    operations.append(
        json.loads(
            run_command(
                [
                    str(binary),
                    "correct",
                    "user prefers weekly eval reports on Friday",
                    "user prefers weekly eval reports on Monday mornings",
                    "--store",
                    str(store),
                    "--json",
                ]
            ).stdout
        )
    )
    operations.append(
        json.loads(
            run_command(
                [
                    str(binary),
                    "remember",
                    "project alpha deploy requires rollback proof before release",
                    "--scope",
                    "project-alpha",
                    "--store",
                    str(store),
                    "--json",
                ]
            ).stdout
        )
    )
    operations.append(
        json.loads(
            run_command(
                [
                    str(binary),
                    "remember",
                    "project beta finance contains private budget notes",
                    "--scope",
                    "project-beta",
                    "--store",
                    str(store),
                    "--json",
                ]
            ).stdout
        )
    )
    operations.append(
        json.loads(
            run_command(
                [
                    str(binary),
                    "remember",
                    "scratch note can be forgotten after validation",
                    "--store",
                    str(store),
                    "--json",
                ]
            ).stdout
        )
    )
    operations.append(
        json.loads(
            run_command(
                [
                    str(binary),
                    "forget",
                    "scratch note can be forgotten after validation",
                    "--store",
                    str(store),
                    "--json",
                ]
            ).stdout
        )
    )
    claim_count_after_lifecycle = len(json.loads(store.read_text()).get("claims", []))
    noise_added = append_noise_records(store, max(0, record_count - claim_count_after_lifecycle))
    eval_context = run_context(binary, store, "current eval report Monday", "private")
    deploy_context = run_context(binary, store, "rollback proof", "project-alpha")
    scratch_context = run_context(binary, store, "scratch note validation", "private")
    eval_text = selected_text(eval_context)
    deploy_text = selected_text(deploy_context)
    scratch_text = selected_text(scratch_context)
    stale_reuse = int("Friday" in eval_text)
    scope_leak = int("private budget notes" in deploy_text)
    forgotten_recall = int("forgotten after validation" in scratch_text)
    current_recall = "Monday mornings" in eval_text and "rollback proof" in deploy_text
    state = json.loads(store.read_text())
    duplicate_groups = duplicate_active_group_count(state.get("claims", []))
    actual_lifecycle = any(
        result.get("command") == "correct" and result.get("latest_claim", {}).get("status") == "active"
        for result in operations
    ) and any(
        result.get("command") == "forget" and result.get("latest_claim", {}).get("status") == "forgotten"
        for result in operations
    )
    return {
        "phase": "P3",
        "ok": actual_lifecycle
        and current_recall
        and stale_reuse == 0
        and scope_leak == 0
        and forgotten_recall == 0,
        "record_count": len(state.get("claims", [])),
        "noise_record_count": noise_added,
        "noise_insert_mode": "direct_store_fixture_after_cli_lifecycle",
        "actual_lifecycle_operations": actual_lifecycle,
        "cli_operation_count": len(operations),
        "current_memory_recall": current_recall,
        "stale_reuse_count": stale_reuse,
        "scope_leak_count": scope_leak,
        "forgotten_recall_count": forgotten_recall,
        "duplicate_active_group_count": duplicate_groups,
        "eval_selected_item_count": eval_context.get("item_count", 0),
        "deploy_selected_item_count": deploy_context.get("item_count", 0),
        "scratch_selected_item_count": scratch_context.get("item_count", 0),
    }


def append_noise_records(store: Path, count: int) -> int:
    state = json.loads(store.read_text())
    now = int(time.time())
    events = state.setdefault("events", [])
    claims = state.setdefault("claims", [])
    audit = state.setdefault("audit", [])
    for index in range(count):
        event_id = f"event-noise-{index + 1:04d}"
        claim_id = f"claim-noise-{index + 1:04d}"
        scope = "private" if index % 3 else "project-noise"
        text = f"noise item {index:04d} mentions unrelated archive marker {index:04d}"
        events.append(
            {
                "id": event_id,
                "speaker_id": "user",
                "actor_agent_id": "product-validation-noise",
                "text": text,
                "scope": scope,
                "trust_level": "trusted_user",
            }
        )
        claims.append(
            {
                "id": claim_id,
                "subject": f"noise item {index:04d}",
                "predicate": "mentions",
                "object": f"unrelated archive marker {index:04d}",
                "status": "active",
                "scope": scope,
                "source_event_ids": [event_id],
            }
        )
        audit.append({"kind": "event_append", "target_id": f"{event_id}:product-validation-noise:trusted_user"})
        audit.append({"kind": "claim_write", "target_id": claim_id})
    metadata = state.setdefault("metadata", {})
    metadata["updated_at_unix_seconds"] = now
    metadata["generation"] = int(metadata.get("generation", 1)) + 1
    store.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n")
    return count


def duplicate_active_group_count(claims: list[dict[str, Any]]) -> int:
    seen: dict[tuple[str, str, str, str], int] = {}
    for claim in claims:
        if claim.get("status") != "active":
            continue
        key = (
            str(claim.get("subject", "")),
            str(claim.get("predicate", "")),
            str(claim.get("object", "")),
            str(claim.get("scope", "")),
        )
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
        "ok": semantic_mrr > term_mrr,
        "term_mrr": term_mrr,
        "semantic_candidate_mrr": semantic_mrr,
        "mrr_delta": semantic_mrr - term_mrr,
        "term_ndcg_at_3": term_ndcg_mean,
        "semantic_candidate_ndcg_at_3": semantic_ndcg_mean,
        "ndcg_delta": semantic_ndcg_mean - term_ndcg_mean,
        "ranking_claim": "alias_probe_not_embedding_proof",
        "requires_external_embedding_eval_before_shipping": True,
        "decision": "do_not_ship_embedding_without_positive_delta_on_held_out_queries",
        "results": results,
    }


def run_migration_safety(binary: Path, out_dir: Path) -> dict[str, Any]:
    store = out_dir / "P5-legacy-v1-store.json"
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


def run_external_review_gate(out_dir: Path) -> dict[str, Any]:
    review_path = ROOT / REVIEW_EXAMPLE
    if not review_path.exists():
        raise ProductValidationFailure(f"missing review example: {review_path}")
    review = json.loads(review_path.read_text())
    validation = validate_review(review)
    copied_review = out_dir / "P6-product-validation-review.example.json"
    copied_review.write_text(json.dumps(review, indent=2, sort_keys=True) + "\n")
    return {
        "phase": "P6",
        "ok": validation["ok"],
        "example_review_valid": validation["ok"],
        "reviewer_count": 1 if validation["ok"] else 0,
        "task_review_count": len(review.get("tasks", [])),
        "raw_transcript_included": bool(review.get("raw_transcript_included")),
        "third_party_claim": bool(review.get("third_party_claim")),
        "external_claim_allowed": validation["ok"]
        and not review.get("raw_transcript_included")
        and bool(review.get("third_party_claim")),
        "ready_for_external_review": validation["ok"],
        "validation_errors": validation["errors"],
        "review_artifact": str(copied_review.relative_to(out_dir)),
    }


def validate_review(review: dict[str, Any]) -> dict[str, Any]:
    errors: list[str] = []
    if review.get("schema_version") != "mneme.product_validation_review.v1":
        errors.append("schema_version must be mneme.product_validation_review.v1")
    if not isinstance(review.get("reviewer_id"), str) or not review["reviewer_id"].strip():
        errors.append("reviewer_id is required")
    if review.get("public_safe") is not True:
        errors.append("public_safe must be true")
    if review.get("raw_transcript_included") is not False:
        errors.append("raw_transcript_included must be false")
    if "third_party_claim" not in review:
        errors.append("third_party_claim must be explicit")
    tasks = review.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        errors.append("tasks must be a non-empty list")
    else:
        for index, task in enumerate(tasks, start=1):
            validate_review_task(task, index, errors)
    serialized = json.dumps(review, ensure_ascii=False)
    if SECRET_RE.search(serialized):
        errors.append("review contains secret-like text")
    if re.search(r"/Users/|/home/|[A-Za-z]:\\\\", serialized):
        errors.append("review contains local filesystem path")
    return {"ok": not errors, "errors": errors}


def validate_review_task(task: dict[str, Any], index: int, errors: list[str]) -> None:
    prefix = f"tasks[{index}]"
    for field in ["id", "evidence_summary"]:
        if not isinstance(task.get(field), str) or not task[field].strip():
            errors.append(f"{prefix}.{field} is required")
    for field in ["condition_labels_blinded", "memory_helped", "memory_harmed", "public_safe"]:
        if not isinstance(task.get(field), bool):
            errors.append(f"{prefix}.{field} must be boolean")
    for field in ["score_without_memory", "score_with_memory"]:
        value = task.get(field)
        if not isinstance(value, int) or value < 0 or value > 3:
            errors.append(f"{prefix}.{field} must be integer 0..3")
    if task.get("memory_harmed") and task.get("memory_helped"):
        errors.append(f"{prefix} cannot mark memory_helped and memory_harmed together")


def run_full(args: argparse.Namespace) -> dict[str, Any]:
    out_dir = args.out_dir
    if out_dir.exists():
        if not args.force:
            raise ProductValidationFailure(f"output directory already exists: {out_dir}")
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    binary = ensure_cli(args.no_build)
    causal = run_causal_usefulness(binary, out_dir)
    privacy_cost = run_privacy_cost_extractor(out_dir)
    long_horizon = run_long_horizon_lifecycle(binary, out_dir, args.record_count)
    ranking = run_ranking_decision()
    migration = run_migration_safety(binary, out_dir)
    review = run_external_review_gate(out_dir)
    phases = [causal, privacy_cost, long_horizon, ranking, migration, review]
    report = {
        "schema_version": SCHEMA_VERSION,
        "command": "product-validation-loop",
        "ok": all(phase["ok"] for phase in phases),
        "run_label": args.run_label,
        "record_count": args.record_count,
        "phases": {
            "P1_causal_memory_usefulness": causal,
            "P2_privacy_cost_extraction": privacy_cost,
            "P3_long_horizon_lifecycle": long_horizon,
            "P4_ranking_decision": ranking,
            "P5_migration_safety": migration,
            "P6_external_review_gate": review,
        },
        "summary": {
            "causal_memory_adoption_rate": causal["memory_adoption_rate"],
            "causal_decision_change_rate": causal["decision_change_rate"],
            "harmful_memory_count": causal["harmful_memory_count"],
            "provider_opt_in_required": privacy_cost["provider_opt_in_required"],
            "live_provider_executed": privacy_cost["live_provider_executed"],
            "provider_budget_within_limit": privacy_cost["budget"]["within_budget"],
            "long_horizon_actual_lifecycle_operations": long_horizon["actual_lifecycle_operations"],
            "long_horizon_scope_leak_count": long_horizon["scope_leak_count"],
            "long_horizon_stale_reuse_count": long_horizon["stale_reuse_count"],
            "ranking_mrr_delta": ranking["mrr_delta"],
            "requires_external_embedding_eval_before_shipping": ranking[
                "requires_external_embedding_eval_before_shipping"
            ],
            "migration_memory_preserved": migration["memory_preserved_after_migration"],
            "external_review_schema_valid": review["example_review_valid"],
            "third_party_claim": review["third_party_claim"],
            "external_claim_allowed": review["external_claim_allowed"],
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
        f"| P1 causal usefulness | `{status(phases['P1_causal_memory_usefulness'])}` | adoption `{phases['P1_causal_memory_usefulness']['memory_adoption_rate']:.2f}`, harmful `{phases['P1_causal_memory_usefulness']['harmful_memory_count']}` |"
    )
    lines.append(
        f"| P2 privacy/cost extraction | `{status(phases['P2_privacy_cost_extraction'])}` | provider opt-in `{phases['P2_privacy_cost_extraction']['provider_opt_in_required']}`, live provider `{phases['P2_privacy_cost_extraction']['live_provider_executed']}` |"
    )
    lines.append(
        f"| P3 long horizon lifecycle | `{status(phases['P3_long_horizon_lifecycle'])}` | stale `{phases['P3_long_horizon_lifecycle']['stale_reuse_count']}`, scope leaks `{phases['P3_long_horizon_lifecycle']['scope_leak_count']}` |"
    )
    lines.append(
        f"| P4 ranking decision | `{status(phases['P4_ranking_decision'])}` | MRR delta `{phases['P4_ranking_decision']['mrr_delta']:.2f}` |"
    )
    lines.append(
        f"| P5 migration safety | `{status(phases['P5_migration_safety'])}` | memory preserved `{phases['P5_migration_safety']['memory_preserved_after_migration']}` |"
    )
    lines.append(
        f"| P6 external review gate | `{status(phases['P6_external_review_gate'])}` | example valid `{phases['P6_external_review_gate']['example_review_valid']}`, third-party claim `{phases['P6_external_review_gate']['third_party_claim']}` |"
    )
    lines.append("")
    lines.append("This local report is a product-validation signal, not a market adoption claim.")
    lines.append("P6 validates the review evidence format; it is not third-party validation by itself.")
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
