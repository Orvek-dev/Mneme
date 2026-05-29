#!/usr/bin/env python3
"""Check Mneme long-horizon retrieval invariants at larger local store sizes."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
STATE_SCHEMA_VERSION = 2
DEFAULT_RECORD_COUNTS = [1000, 5000, 10000]


class ScaleCheckFailure(RuntimeError):
    """Raised when a scale check cannot complete."""


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
        "command": "long-horizon-scale-check-contract",
        "default_record_counts": DEFAULT_RECORD_COUNTS,
        "metrics": [
            "current_memory_recall",
            "stale_reuse_count",
            "scope_leak_count",
            "context_latency_ms",
        ],
        "claim_policy": "local scale smoke only; not a production database benchmark",
    }


def ensure_cli(no_build: bool) -> Path:
    binary = ROOT / "target" / "debug" / "mneme"
    if not no_build:
        run_command(["cargo", "build", "-q", "-p", "mneme-cli"])
    if not binary.exists():
        raise ScaleCheckFailure("target/debug/mneme is missing")
    return binary


def run_command(args: list[str]) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True)
    if result.returncode != 0:
        raise ScaleCheckFailure(
            f"command failed ({result.returncode}): {' '.join(args)}\n{result.stderr}"
        )
    return result


def build_store(path: Path, record_count: int) -> None:
    now = int(time.time())
    base_claims = [
        ("user", "prefers", "weekly eval reports on Friday", "private", "superseded"),
        ("user", "prefers", "weekly eval reports on Monday mornings", "private", "active"),
        ("weekly eval reports", "schedule", "Monday mornings", "private", "active"),
        ("project alpha deploy", "requires", "rollback proof before release", "project-alpha", "active"),
        ("project beta finance", "contains", "private budget notes", "project-beta", "active"),
    ]
    claims = []
    events = []
    audit = []
    for index, (subject, predicate, obj, scope, status) in enumerate(base_claims, start=1):
        event_id = f"event-{index:06d}"
        claim_id = f"claim-{index:06d}"
        events.append(event_record(event_id, f"{subject} {predicate} {obj}", scope))
        claims.append(claim_record(claim_id, subject, predicate, obj, scope, status, event_id))
        audit.extend(audit_records(event_id, claim_id))
    for index in range(len(base_claims) + 1, record_count + 1):
        event_id = f"event-{index:06d}"
        claim_id = f"claim-{index:06d}"
        scope = "private" if index % 4 else "project-noise"
        subject = f"noise item {index:06d}"
        obj = f"unrelated archive marker {index:06d}"
        events.append(event_record(event_id, f"{subject} mentions {obj}", scope))
        claims.append(claim_record(claim_id, subject, "mentions", obj, scope, "active", event_id))
        audit.extend(audit_records(event_id, claim_id))
    state = {
        "schema_version": STATE_SCHEMA_VERSION,
        "metadata": {
            "store_id": f"long-horizon-scale-{record_count}",
            "generation": 1,
            "created_at_unix_seconds": now,
            "updated_at_unix_seconds": now,
            "engine_version": "long-horizon-scale-check",
            "migration_history": [],
        },
        "budget": {
            "daily_cloud_tokens": 100000,
            "spent_tokens": 0,
            "hard_cap_violations": 0,
        },
        "events": events,
        "claims": claims,
        "sessions": [],
        "audit": audit,
    }
    path.write_text(json.dumps(state, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def event_record(event_id: str, text: str, scope: str) -> dict[str, Any]:
    return {
        "id": event_id,
        "speaker_id": "user",
        "actor_agent_id": "scale-check",
        "text": text,
        "scope": scope,
        "trust_level": "trusted_user",
    }


def claim_record(
    claim_id: str,
    subject: str,
    predicate: str,
    obj: str,
    scope: str,
    status: str,
    event_id: str,
) -> dict[str, Any]:
    return {
        "id": claim_id,
        "subject": subject,
        "predicate": predicate,
        "object": obj,
        "status": status,
        "scope": scope,
        "source_event_ids": [event_id],
    }


def audit_records(event_id: str, claim_id: str) -> list[dict[str, str]]:
    return [
        {"kind": "event_append", "target_id": f"{event_id}:scale-check:trusted_user"},
        {"kind": "claim_write", "target_id": claim_id},
    ]


def run_context(binary: Path, store: Path, query: str, scope: str) -> tuple[dict[str, Any], float]:
    start = time.perf_counter()
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
    elapsed_ms = (time.perf_counter() - start) * 1000
    return json.loads(result.stdout), elapsed_ms


def selected_text(context: dict[str, Any]) -> str:
    return "\n".join(
        str(item.get("claim_text", ""))
        for item in context.get("context_pack", {}).get("items", [])
    )


def run_scale_check(args: argparse.Namespace) -> dict[str, Any]:
    binary = ensure_cli(args.no_build)
    out_dir = args.out_dir
    out_dir.mkdir(parents=True, exist_ok=True)
    results = []
    for count in args.record_counts:
        store = out_dir / f"scale-{count}.json"
        build_store(store, count)
        eval_context, eval_latency = run_context(binary, store, "current eval report Monday", "private")
        deploy_context, deploy_latency = run_context(binary, store, "rollback proof", "project-alpha")
        eval_text = selected_text(eval_context)
        deploy_text = selected_text(deploy_context)
        stale_reuse = int("Friday" in eval_text)
        scope_leak = int("private budget notes" in deploy_text)
        current_recall = "Monday mornings" in eval_text and "rollback proof" in deploy_text
        max_latency = max(eval_latency, deploy_latency)
        results.append(
            {
                "record_count": count,
                "current_memory_recall": current_recall,
                "stale_reuse_count": stale_reuse,
                "scope_leak_count": scope_leak,
                "eval_context_latency_ms": round(eval_latency, 3),
                "deploy_context_latency_ms": round(deploy_latency, 3),
                "max_context_latency_ms": round(max_latency, 3),
                "within_latency_budget": max_latency <= args.max_latency_ms,
            }
        )
    report = {
        "schema_version": SCHEMA_VERSION,
        "command": "long-horizon-scale-check",
        "ok": all(
            result["current_memory_recall"]
            and result["stale_reuse_count"] == 0
            and result["scope_leak_count"] == 0
            and result["within_latency_budget"]
            for result in results
        ),
        "max_latency_ms": args.max_latency_ms,
        "claim_policy": contract()["claim_policy"],
        "results": results,
    }
    return report


def parse_record_counts(value: str) -> list[int]:
    counts = []
    for part in value.split(","):
        count = int(part.strip())
        if count < 10:
            raise argparse.ArgumentTypeError("record counts must be >= 10")
        counts.append(count)
    return counts


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true")
    parser.add_argument("--record-counts", type=parse_record_counts, default=DEFAULT_RECORD_COUNTS)
    parser.add_argument("--max-latency-ms", type=float, default=5000.0)
    parser.add_argument("--out-dir", type=Path, default=Path("/tmp/mneme-long-horizon-scale"))
    parser.add_argument("--report", type=Path)
    parser.add_argument("--no-build", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.check_contract:
        print(json.dumps(contract(), indent=2, sort_keys=True))
        return 0
    try:
        report = run_scale_check(args)
    except (ScaleCheckFailure, OSError, json.JSONDecodeError) as error:
        print(f"long-horizon-scale-check: {error}", file=sys.stderr)
        return 1
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
