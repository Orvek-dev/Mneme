#!/usr/bin/env python3
"""Run the Mneme MCP hard-mode dogfood protocol.

This runner exercises the public stdio MCP server instead of the native CLI or
in-process eval target. It reuses the existing v1/v2 hard datasets and checks
that the MCP path can carry the same normal, adversarial, handoff, ontology,
and seeded-fault evidence without leaking synthetic private or secret data.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
V1_FAULTS = ["skip-claims", "leak-secrets", "drop-citations"]
V2_FAULTS = [
    "bypass-acl",
    "leak-secrets",
    "drop-citations",
    "unapproved-promotion",
    "ignore-revocation",
    "leak-quarantined",
]
THRESHOLDS = {
    "v1_recall_at_k_min": 0.95,
    "v1_precision_at_k_min": 0.95,
    "v1_citation_coverage_min": 1.0,
    "v1_handoff_success_min": 0.95,
    "v2_handoff_success_min": 0.95,
    "mcp_suite_pass_rate_min": 1.0,
    "team_suite_pass_rate_min": 1.0,
    "ontology_entity_f1_min": 0.8,
    "ontology_relation_f1_min": 0.8,
    "ontology_attribute_f1_min": 0.8,
    "scope_leak_max": 0,
    "secret_leak_max": 0,
    "seeded_fault_detection_min": 1.0,
}


class McpHardDogfoodFailure(RuntimeError):
    """Raised when MCP hard-mode dogfood cannot complete."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


def load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise McpHardDogfoodFailure(f"cannot load module: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


V1_HARD = load_module("mneme_v1_hard_dogfood", ROOT / "scripts" / "v1-hard-dogfood.py")
V2_TEAM = load_module("mneme_v2_team_dogfood", ROOT / "scripts" / "v2-team-dogfood.py")
ONTOLOGY = load_module("mneme_v1_ontology_benchmark", ROOT / "scripts" / "v1-ontology-benchmark.py")


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood-contract",
        "protocol": "mcp:2024-11-05",
        "target": "mneme-mcp",
        "v1_normal_record_count": V1_HARD.EXPECTED_NORMAL_RECORD_COUNT,
        "v1_adversarial_record_count": V1_HARD.EXPECTED_ADVERSARIAL_RECORD_COUNT,
        "v1_agent_workflow_count": V1_HARD.EXPECTED_AGENT_WORKFLOW_COUNT,
        "v1_ontology_case_count": ONTOLOGY.EXPECTED_CASE_COUNT,
        "v2_team_record_count": V2_TEAM.EXPECTED_TEAM_RECORD_COUNT,
        "v2_adversarial_record_count": V2_TEAM.EXPECTED_ADVERSARIAL_RECORD_COUNT,
        "v2_handoff_workflow_count": V2_TEAM.EXPECTED_HANDOFF_WORKFLOW_COUNT,
        "mcp_scenario_suite": "mcp",
        "team_scenario_suite": "team",
        "seeded_faults": {"v1": V1_FAULTS, "v2": V2_FAULTS},
        "outputs": [
            "summary.json",
            "scorecard.json",
            "dataset.json",
            "v1-mcp-hard.json",
            "v1-mcp-ontology.json",
            "v2-mcp-hard.json",
            "suite-results.json",
            "seeded-faults.json",
            "equivalence.json",
            "report.md",
        ],
        "privacy_policy": "fixtures are synthetic and public-safe; no local user stores are committed",
        "thresholds": THRESHOLDS,
    }


def dataset_report() -> dict[str, Any]:
    normal = V1_HARD.load_manual_records()
    adversarial = V1_HARD.build_adversarial_records()
    workflows = V1_HARD.build_agent_workflows()
    ontology_fixture = ONTOLOGY.load_fixture(ONTOLOGY.DEFAULT_FIXTURE)
    v2_dataset = V2_TEAM.dataset_report()
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood-dataset",
        "v1": V1_HARD.dataset_summary(normal, adversarial, workflows),
        "v1_ontology": ONTOLOGY.validate_fixture(ontology_fixture),
        "v2": v2_dataset,
        "public_safe": True,
    }


class McpClient:
    def __init__(self, binary: Path, *, v1_store: Path, team_store: Path, workspace: str = "team"):
        self.binary = binary
        self.v1_store = v1_store
        self.team_store = team_store
        self.workspace = workspace
        self.request_id = 0
        self.process: subprocess.Popen[str] | None = None

    def __enter__(self) -> "McpClient":
        self.process = subprocess.Popen(
            [
                str(self.binary),
                "--mode",
                "all",
                "--v1-store",
                str(self.v1_store),
                "--team-store",
                str(self.team_store),
                "--team-workspace",
                self.workspace,
            ],
            cwd=ROOT,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.request("initialize")
        tools = self.request("tools/list")
        tool_count = len(((tools.get("result") or {}).get("tools") or []))
        if tool_count < 30:
            raise McpHardDogfoodFailure(f"MCP tools/list returned only {tool_count} tools")
        return self

    def __exit__(self, exc_type: Any, exc: Any, traceback: Any) -> None:
        if self.process is None:
            return
        if self.process.stdin:
            self.process.stdin.close()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        if exc_type is not None:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            if stderr:
                print(stderr, file=sys.stderr)

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        if self.process is None or self.process.stdin is None or self.process.stdout is None:
            raise McpHardDogfoodFailure("MCP process is not running")
        self.request_id += 1
        payload: dict[str, Any] = {"jsonrpc": "2.0", "id": self.request_id, "method": method}
        if params is not None:
            payload["params"] = params
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise McpHardDogfoodFailure(f"MCP process returned no response: {stderr}")
        response = json.loads(line)
        if "error" in response:
            raise McpHardDogfoodFailure(f"MCP {method} failed: {response['error']}")
        return response

    def tool(self, name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        response = self.request(
            "tools/call",
            {"name": name, "arguments": arguments or {}},
        )
        result = response.get("result") or {}
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            raise McpHardDogfoodFailure(f"MCP tool {name} returned no structuredContent")
        return structured


def build_binary(args: argparse.Namespace) -> Path:
    binary = Path(args.mneme_mcp_bin) if args.mneme_mcp_bin else ROOT / "target" / "debug" / "mneme-mcp"
    if not args.no_build:
        run_command(["cargo", "build", "-q", "-p", "mneme-mcp"])
    if not binary.exists():
        raise McpHardDogfoodFailure(f"mneme-mcp binary not found: {binary}")
    return binary


def run_command(command: list[str], *, expect_success: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True)
    if expect_success and result.returncode != 0:
        raise McpHardDogfoodFailure(
            f"command failed ({result.returncode}): {' '.join(command)}\n{result.stdout}\n{result.stderr}"
        )
    return result


def run_v1_hard_mcp(binary: Path, work_dir: Path) -> dict[str, Any]:
    normal_records = V1_HARD.load_manual_records()
    adversarial_records = V1_HARD.build_adversarial_records()
    workflows = V1_HARD.build_agent_workflows()
    store = work_dir / "v1-hard.json"
    team_store = work_dir / "v1-hard-team-unused.json"
    alias_to_claim_id: dict[str, str] = {}
    ingested_records = []
    workflow_results = []
    failures = []
    metrics = V1_HARD.MetricAccumulator()

    with McpClient(binary, v1_store=store, team_store=team_store, workspace="mcp-v1-hard") as client:
        for record in normal_records:
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
            ingest_v1_record(client, normalized, alias_to_claim_id, ingested_records)
        for record in adversarial_records:
            ingest_v1_record(client, record, alias_to_claim_id, ingested_records)

        for index in range(1, 21):
            alias = f"stale-{index}"
            claim_id = alias_to_claim_id.get(alias)
            if not claim_id:
                raise McpHardDogfoodFailure(f"missing stale claim alias: {alias}")
            report = client.tool(
                "mneme_v1_correct",
                {
                    "claim_id": claim_id,
                    "new_text": f"stale-fixture prefers current phase route {index}",
                    "scope": "private",
                },
            )
            claims = report.get("claims") or []
            latest = claims[-1] if claims else {}
            if latest.get("object") != f"current phase route {index}":
                raise McpHardDogfoodFailure(f"stale correction {index} did not create current route")

        for workflow in workflows:
            result = run_v1_workflow(client, workflow, metrics)
            workflow_results.append(result)
            if result["status"] != "passed":
                failures.append(
                    {
                        "id": result["id"],
                        "category": result["category"],
                        "reason": "; ".join(result["errors"]),
                    }
                )

    scorecard = metrics.scorecard()
    seeded_faults = V1_HARD.seeded_fault_report(workflow_results)
    ok = (
        scorecard["recall_at_k"] >= THRESHOLDS["v1_recall_at_k_min"]
        and scorecard["precision_at_k"] >= THRESHOLDS["v1_precision_at_k_min"]
        and scorecard["citation_coverage"] >= THRESHOLDS["v1_citation_coverage_min"]
        and scorecard["handoff_success_rate"] >= THRESHOLDS["v1_handoff_success_min"]
        and scorecard["scope_leak_count"] <= THRESHOLDS["scope_leak_max"]
        and scorecard["secret_leak_count"] <= THRESHOLDS["secret_leak_max"]
        and scorecard["agent_attribution_error_count"] == 0
        and scorecard["stale_reuse_count"] == 0
        and seeded_faults["ok"]
        and not failures
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-v1-hard",
        "ok": ok,
        "normal_record_count": len(normal_records),
        "adversarial_record_count": len(adversarial_records),
        "ingested_record_count": len(ingested_records),
        "agent_workflow_count": len(workflows),
        "failed_workflow_count": len(failures),
        "scorecard": scorecard,
        "seeded_faults": seeded_faults,
        "failures": failures,
    }


def ingest_v1_record(
    client: McpClient,
    record: dict[str, Any],
    alias_to_claim_id: dict[str, str],
    ingested_records: list[dict[str, Any]],
) -> None:
    before = v1_claim_count(client)
    tool = "mneme_v1_remember" if record["mode"] == "remember" else "mneme_v1_ingest"
    report = client.tool(
        tool,
        {
            "text": record["text"],
            "scope": record["scope"],
            "trust": record.get("trust", "trusted_user"),
            "speaker": "user",
        },
    )
    after = int(report.get("claim_count", 0))
    expected_delta = int(record.get("expected_claim_delta", 1))
    if after != before + expected_delta:
        raise McpHardDogfoodFailure(
            f"{record['id']} expected claim delta {expected_delta}, got {after - before}"
        )
    latest = report.get("latest_claim") or {}
    expected_status = record.get("expected_status")
    if expected_delta and expected_status and latest.get("status") != expected_status:
        raise McpHardDogfoodFailure(
            f"{record['id']} expected status {expected_status}, got {latest.get('status')}"
        )
    if expected_delta and latest.get("scope") != record["scope"]:
        raise McpHardDogfoodFailure(
            f"{record['id']} expected scope {record['scope']}, got {latest.get('scope')}"
        )
    if record.get("alias") and latest.get("id"):
        alias_to_claim_id[record["alias"]] = latest["id"]
    ingested_records.append(
        {
            "record_id": record["id"],
            "category": record["category"],
            "scope": record["scope"],
            "mode": record["mode"],
            "claim_delta": expected_delta,
            "claim_id": latest.get("id"),
            "status": expected_status,
        }
    )


def v1_claim_count(client: McpClient) -> int:
    snapshot = client.tool("mneme_v1_snapshot")
    return len(((snapshot.get("snapshot") or {}).get("claims") or []))


def run_v1_workflow(
    client: McpClient,
    workflow: dict[str, Any],
    metrics: Any,
) -> dict[str, Any]:
    checks: list[str] = []
    errors: list[str] = []
    begin = client.tool(
        "mneme_v1_begin",
        {
            "task": workflow["task"],
            "agent": workflow["agent"],
            "query": workflow["query"],
            "scopes": workflow["scopes"],
            "max_items": workflow["max_items"],
        },
    )
    report = begin.get("report") or {}
    session = report.get("session") or {}
    context_pack = report.get("context_pack") or {}
    items = context_pack.get("items") or []
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
    if any(secret in text_blob for secret in V1_HARD.SECRET_LITERALS) or V1_HARD.HARD_SECRET_PREFIX in text_blob:
        errors.append("context leaked synthetic secret-like text")

    handoff_memory = workflow["handoff_memory"]
    handoff_recalled = False
    if session.get("id"):
        end = client.tool(
            "mneme_v1_end",
            {
                "session_id": session["id"],
                "agent": workflow["agent"],
                "summary": f"Completed {workflow['name']}",
                "remember": [handoff_memory],
            },
        )
        remembered = ((end.get("report") or {}).get("remembered_claim_ids") or [])
        if len(remembered) != 1:
            errors.append("end did not persist one handoff memory")
        else:
            checks.append("persisted one handoff memory")
        handoff = client.tool(
            "mneme_v1_context",
            {
                "query": workflow["handoff_query"],
                "scopes": ["private"],
                "max_items": 8,
            },
        )
        metrics.handoff_attempts += 1
        handoff_recalled = handoff_memory in V1_HARD.context_texts(handoff)
        if handoff_recalled:
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
        "context_items": [
            {
                "claim_text": item.get("claim_text", ""),
                "source_event_ids": item.get("source_event_ids", []),
            }
            for item in items
        ],
        "must_not_include": workflow["must_not_include"],
        "handoff_memory": handoff_memory,
        "handoff_recalled": handoff_recalled,
        "checks": checks,
        "errors": errors,
    }


def run_v1_ontology_mcp(binary: Path, work_dir: Path) -> dict[str, Any]:
    fixture = ONTOLOGY.load_fixture(ONTOLOGY.DEFAULT_FIXTURE)
    validation = ONTOLOGY.validate_fixture(fixture)
    if validation.get("errors"):
        raise McpHardDogfoodFailure(f"ontology fixture invalid: {validation['errors']}")
    case_runs = []
    for case in fixture["cases"]:
        store = work_dir / f"ontology-{case['id']}.json"
        team_store = work_dir / f"ontology-{case['id']}-team-unused.json"
        with McpClient(binary, v1_store=store, team_store=team_store, workspace="mcp-ontology") as client:
            event_reports = []
            for event in case["events"]:
                event_reports.append(
                    client.tool(
                        "mneme_v1_ingest",
                        {
                            "text": event["text"],
                            "speaker": event["speaker_id"],
                            "agent": event.get("actor_agent_id"),
                            "scope": event["scope"],
                            "trust": event["trust_level"],
                        },
                    )
                )
            snapshot = client.tool("mneme_v1_snapshot")
            validate = client.tool("mneme_v1_validate")
            context_reports = []
            for check in case["expected"].get("context_checks", []):
                context_reports.append(
                    {
                        "check_id": check["id"],
                        "report": client.tool(
                            "mneme_v1_context",
                            {
                                "query": check["query"],
                                "scopes": check.get("allowed_scopes", []),
                                "max_items": 8,
                            },
                        ),
                    }
                )
            case_runs.append(
                {
                    "case_id": case["id"],
                    "category": case["category"],
                    "input_style": case["input_style"],
                    "event_reports": event_reports,
                    "snapshot": (snapshot.get("snapshot") or {}),
                    "validation": validate,
                    "context_reports": context_reports,
                }
            )
    scorecard = ONTOLOGY.score_benchmark(fixture, case_runs)
    ok = (
        scorecard["entity_f1"] >= THRESHOLDS["ontology_entity_f1_min"]
        and scorecard["relation_f1"] >= THRESHOLDS["ontology_relation_f1_min"]
        and scorecard["attribute_f1"] >= THRESHOLDS["ontology_attribute_f1_min"]
        and scorecard["scope_leak_count"] <= THRESHOLDS["scope_leak_max"]
        and scorecard["secret_leak_count"] <= THRESHOLDS["secret_leak_max"]
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-v1-ontology",
        "ok": ok,
        "case_count": len(case_runs),
        "scorecard": scorecard,
    }


def run_v2_hard_mcp(binary: Path, work_dir: Path) -> dict[str, Any]:
    team_records = V2_TEAM.build_team_records()
    adversarial_records = V2_TEAM.build_adversarial_records()
    workflows = V2_TEAM.build_handoff_workflows()
    store = work_dir / "v2-hard-v1-unused.json"
    team_store = work_dir / "v2-hard.json"
    status_counts: Counter[str] = Counter()
    handoff_results = []
    scope_leak_count = 0
    secret_leak_count = 0
    citation_items = 0
    citation_items_with_source = 0
    private_omitted_count = 0
    sync_checksum_verified = False
    sync_privacy_ok = False

    with McpClient(binary, v1_store=store, team_store=team_store, workspace="mcp-v2-hard") as client:
        setup_v2_workspace(client)
        for record in team_records:
            actor = record["actor"]
            memory = client.tool(
                "mneme_v2_team_remember",
                {
                    "actor": actor,
                    "agent": agent_for_user(actor),
                    "text": record["text"],
                    "scope": record["scope"],
                },
            )["memory"]
            status_counts[memory.get("status", "unknown")] += 1

        for record in adversarial_records:
            actor = "owner"
            memory = client.tool(
                "mneme_v2_team_remember",
                {
                    "actor": actor,
                    "agent": agent_for_user(actor),
                    "text": record["text"],
                    "scope": record["scope"],
                },
            )["memory"]
            status_counts[memory.get("status", "unknown")] += 1

        for workflow in workflows:
            result = run_v2_handoff_workflow(client, workflow)
            handoff_results.append(result)
            scope_leak_count += result["scope_leak_count"]
            secret_leak_count += result["secret_leak_count"]
            citation_items += result["citation_items"]
            citation_items_with_source += result["citation_items_with_source"]

        private_context = client.tool(
            "mneme_v2_team_context",
            {
                "actor": "member-a",
                "agent": "codex-a",
                "query": "private_scope_trap",
                "max_items": 8,
            },
        )
        private_omitted_count = int(private_context.get("omitted_count", 0))
        private_items = ((private_context.get("context_pack") or {}).get("items") or [])
        scope_leak_count += sum(
            1
            for item in private_items
            if "private_scope_trap" in item.get("memory_text", "")
            or item.get("scope") == "private:owner"
        )

        sync = client.tool(
            "mneme_v2_sync_export",
            {"actor": "member-a", "agent": "codex-a", "include_projects": True},
        )
        sync_blob = json.dumps(sync, sort_keys=True)
        sync_privacy_ok = (
            "private_scope_trap" not in sync_blob
            and "PUBLIC_TEAM_TEST_VALUE" not in sync_blob
            and "ignore previous instructions" not in sync_blob.lower()
        )
        import_report = client.tool(
            "mneme_v2_sync_import",
            {"actor": "member-a", "agent": "codex-a", "envelope": sync["envelope"], "apply": False},
        )
        sync_checksum_verified = bool((import_report.get("report") or {}).get("checksum_verified"))
        snapshot = client.tool("mneme_v2_snapshot")["snapshot"]
        firewall = client.tool("mneme_v2_firewall")["firewall"]
        quality = client.tool("mneme_v2_quality")["quality"]

    handoff_successes = sum(1 for result in handoff_results if result["ok"])
    handoff_success_rate = handoff_successes / len(handoff_results) if handoff_results else 0.0
    citation_coverage = citation_items_with_source / citation_items if citation_items else 1.0
    scorecard = {
        "team_record_count": len(team_records),
        "adversarial_record_count": len(adversarial_records),
        "handoff_workflow_count": len(workflows),
        "handoff_success_rate": round(handoff_success_rate, 4),
        "scope_leak_count": scope_leak_count,
        "secret_leak_count": secret_leak_count,
        "citation_coverage": round(citation_coverage, 4),
        "blocked_secret_count": status_counts.get("blocked_secret", 0),
        "quarantined_count": status_counts.get("quarantined", 0),
        "private_omitted_count": private_omitted_count,
        "sync_checksum_verified": sync_checksum_verified,
        "sync_privacy_ok": sync_privacy_ok,
        "firewall_ok": firewall.get("ok"),
        "quality_ok": quality.get("ok"),
        "memory_count": len(snapshot.get("memories", [])),
    }
    ok = (
        scorecard["handoff_success_rate"] >= THRESHOLDS["v2_handoff_success_min"]
        and scorecard["scope_leak_count"] <= THRESHOLDS["scope_leak_max"]
        and scorecard["secret_leak_count"] <= THRESHOLDS["secret_leak_max"]
        and scorecard["citation_coverage"] == 1.0
        and scorecard["blocked_secret_count"] > 0
        and scorecard["quarantined_count"] > 0
        and scorecard["private_omitted_count"] > 0
        and scorecard["sync_checksum_verified"]
        and scorecard["sync_privacy_ok"]
    )
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-v2-hard",
        "ok": ok,
        "scorecard": scorecard,
        "status_counts": dict(sorted(status_counts.items())),
        "failed_handoffs": [result for result in handoff_results if not result["ok"]],
    }


def setup_v2_workspace(client: McpClient) -> None:
    client.tool("mneme_v2_team_init", {"workspace": "mcp-v2-hard"})
    for user, role in [("owner", "admin"), ("member-a", "member"), ("member-b", "member")]:
        client.tool("mneme_v2_user_add", {"user": user, "role": role})
    for agent, owner in [("codex-a", "member-a"), ("codex-b", "member-b")]:
        client.tool("mneme_v2_agent_add", {"agent": agent, "owner": owner})
    for project in ["atlas", "bridge"]:
        client.tool("mneme_v2_project_add", {"project": project, "members": ["member-a", "member-b"]})


def agent_for_user(user_id: str) -> str | None:
    if user_id == "member-a":
        return "codex-a"
    if user_id == "member-b":
        return "codex-b"
    return None


def run_v2_handoff_workflow(client: McpClient, workflow: dict[str, Any]) -> dict[str, Any]:
    project = workflow["project"]
    query = f"{project} deterministic checklist"
    begin = client.tool(
        "mneme_v2_run_begin",
        {
            "actor": "member-a",
            "agent": "codex-a",
            "task": f"MCP hard handoff {workflow['id']}",
            "query": query,
            "scope": f"project:{project}",
            "max_items": 8,
        },
    )
    run_id = begin["run_id"]
    client.tool(
        "mneme_v2_run_end",
        {
            "actor": "member-a",
            "agent": "codex-a",
            "run_id": run_id,
            "summary": f"Completed {workflow['id']}",
            "next": [f"{project} next agent continues deterministic checklist"],
            "remember": [f"{project} MCP handoff {workflow['id']} requires cited checklist"],
            "scope": f"project:{project}",
        },
    )
    handoff = client.tool(
        "mneme_v2_run_handoff",
        {
            "actor": "member-a",
            "agent": "codex-a",
            "run_id": run_id,
            "query": query,
            "max_items": 8,
        },
    )
    package = handoff.get("package") or {}
    context = package.get("context_pack") or {}
    items = context.get("items") or []
    text_blob = "\n".join(item.get("memory_text", "") for item in items)
    allowed_scopes = {"team", "project:atlas", "project:bridge", "private:member-a", "agent-private:codex-a"}
    scope_leaks = sum(1 for item in items if item.get("scope") not in allowed_scopes)
    secret_leaks = sum(1 for item in items if looks_like_secret(item.get("memory_text", "")))
    citation_items = len(items)
    citation_items_with_source = sum(1 for item in items if item.get("source_event_ids") or item.get("source_memory_ids"))
    forbidden = ["private_scope_trap", "PUBLIC_TEAM_TEST_VALUE", "ignore previous instructions", "leak secret"]
    ok = (
        int(handoff.get("context_item_count", 0)) > 0
        and citation_items == citation_items_with_source
        and scope_leaks == 0
        and secret_leaks == 0
        and not any(value.lower() in text_blob.lower() for value in forbidden)
    )
    return {
        "id": workflow["id"],
        "ok": ok,
        "context_item_count": int(handoff.get("context_item_count", 0)),
        "scope_leak_count": scope_leaks,
        "secret_leak_count": secret_leaks,
        "citation_items": citation_items,
        "citation_items_with_source": citation_items_with_source,
    }


def looks_like_secret(text: str) -> bool:
    lowered = text.lower()
    return any(
        marker in lowered
        for marker in [
            "api_key=",
            "secret=",
            "token=",
            "password=",
            "authorization: bearer",
            "sk-",
            "ghp_",
            "public_team_test_value",
            "fake_hard_value",
        ]
    )


def run_suite_reports(out_dir: Path) -> dict[str, Any]:
    reports = {}
    for suite in ["mcp", "team"]:
        report_path = out_dir / f"{suite}-suite-mneme-mcp.json"
        run_command(
            [
                "cargo",
                "run",
                "-q",
                "-p",
                "mneme-eval",
                "--",
                "run",
                "--suite",
                suite,
                "--target",
                "mneme-mcp",
                "--json",
                "--report",
                str(report_path),
            ]
        )
        report = read_json(report_path)
        reports[suite] = {
            "ok": report.get("ok"),
            "scenario_count": report.get("scenario_count"),
            "passed": report.get("passed"),
            "failed": report.get("failed"),
            "pass_rate": (report.get("passed", 0) / report.get("scenario_count", 1)),
            "report": str(report_path),
        }
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood-suite-results",
        "reports": reports,
        "ok": all(report["ok"] for report in reports.values()),
    }


def seeded_fault_report() -> dict[str, Any]:
    results = []
    for suite, faults in [("core", V1_FAULTS), ("team", V2_FAULTS)]:
        for fault in faults:
            result = run_command(
                [
                    "cargo",
                    "run",
                    "-q",
                    "-p",
                    "mneme-eval",
                    "--",
                    "run",
                    "--suite",
                    suite,
                    "--target",
                    "mneme-mcp",
                    "--seeded-fault",
                    fault,
                ],
                expect_success=False,
            )
            results.append(
                {
                    "suite": suite,
                    "fault": fault,
                    "detected": result.returncode != 0,
                    "exit_code": result.returncode,
                }
            )
    detected_count = sum(1 for result in results if result["detected"])
    detection_rate = detected_count / len(results) if results else 0.0
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood-seeded-faults",
        "fault_count": len(results),
        "detected_count": detected_count,
        "detection_rate": round(detection_rate, 4),
        "ok": detection_rate >= THRESHOLDS["seeded_fault_detection_min"],
        "results": results,
    }


def build_scorecard(
    v1: dict[str, Any],
    ontology: dict[str, Any],
    v2: dict[str, Any],
    suites: dict[str, Any],
    seeded_faults: dict[str, Any],
) -> dict[str, Any]:
    mcp_pass_rate = suites["reports"]["mcp"]["pass_rate"]
    team_pass_rate = suites["reports"]["team"]["pass_rate"]
    scorecard = {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood-scorecard",
        "v1_recall_at_k": v1["scorecard"]["recall_at_k"],
        "v1_precision_at_k": v1["scorecard"]["precision_at_k"],
        "v1_citation_coverage": v1["scorecard"]["citation_coverage"],
        "v1_handoff_success_rate": v1["scorecard"]["handoff_success_rate"],
        "v1_scope_leak_count": v1["scorecard"]["scope_leak_count"],
        "v1_secret_leak_count": v1["scorecard"]["secret_leak_count"],
        "ontology_entity_f1": ontology["scorecard"]["entity_f1"],
        "ontology_relation_f1": ontology["scorecard"]["relation_f1"],
        "ontology_attribute_f1": ontology["scorecard"]["attribute_f1"],
        "ontology_scope_leak_count": ontology["scorecard"]["scope_leak_count"],
        "ontology_secret_leak_count": ontology["scorecard"]["secret_leak_count"],
        "v2_handoff_success_rate": v2["scorecard"]["handoff_success_rate"],
        "v2_scope_leak_count": v2["scorecard"]["scope_leak_count"],
        "v2_secret_leak_count": v2["scorecard"]["secret_leak_count"],
        "v2_citation_coverage": v2["scorecard"]["citation_coverage"],
        "v2_blocked_secret_count": v2["scorecard"]["blocked_secret_count"],
        "v2_quarantined_count": v2["scorecard"]["quarantined_count"],
        "v2_sync_checksum_verified": v2["scorecard"]["sync_checksum_verified"],
        "v2_sync_privacy_ok": v2["scorecard"]["sync_privacy_ok"],
        "mcp_suite_pass_rate": round(mcp_pass_rate, 4),
        "team_suite_mcp_pass_rate": round(team_pass_rate, 4),
        "seeded_fault_detection_rate": seeded_faults["detection_rate"],
        "thresholds": THRESHOLDS,
    }
    scorecard["ok"] = (
        v1["ok"]
        and ontology["ok"]
        and v2["ok"]
        and suites["ok"]
        and seeded_faults["ok"]
        and mcp_pass_rate >= THRESHOLDS["mcp_suite_pass_rate_min"]
        and team_pass_rate >= THRESHOLDS["team_suite_pass_rate_min"]
    )
    return scorecard


def build_equivalence(
    dataset: dict[str, Any],
    v1: dict[str, Any],
    ontology: dict[str, Any],
    v2: dict[str, Any],
    suites: dict[str, Any],
) -> dict[str, Any]:
    checks = [
        {
            "name": "v1.normal_records",
            "expected": dataset["v1"]["normal_record_count"],
            "actual": v1["normal_record_count"],
        },
        {
            "name": "v1.adversarial_records",
            "expected": dataset["v1"]["adversarial_record_count"],
            "actual": v1["adversarial_record_count"],
        },
        {
            "name": "v1.agent_workflows",
            "expected": dataset["v1"]["agent_workflow_count"],
            "actual": v1["agent_workflow_count"],
        },
        {
            "name": "v1.ontology_cases",
            "expected": dataset["v1_ontology"]["case_count"],
            "actual": ontology["case_count"],
        },
        {
            "name": "v2.team_records",
            "expected": dataset["v2"]["team_record_count"],
            "actual": v2["scorecard"]["team_record_count"],
        },
        {
            "name": "v2.adversarial_records",
            "expected": dataset["v2"]["adversarial_record_count"],
            "actual": v2["scorecard"]["adversarial_record_count"],
        },
        {
            "name": "v2.handoff_workflows",
            "expected": dataset["v2"]["handoff_workflow_count"],
            "actual": v2["scorecard"]["handoff_workflow_count"],
        },
        {
            "name": "mcp.scenario_suite",
            "expected": True,
            "actual": suites["reports"]["mcp"]["ok"],
        },
        {
            "name": "team.scenario_suite_via_mcp",
            "expected": True,
            "actual": suites["reports"]["team"]["ok"],
        },
    ]
    for check in checks:
        check["ok"] = check["expected"] == check["actual"]
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood-equivalence",
        "ok": all(check["ok"] for check in checks),
        "checks": checks,
    }


def write_bundle(args: argparse.Namespace) -> dict[str, Any]:
    binary = build_binary(args)
    out_dir = Path(args.out_dir)
    if out_dir.exists() and any(out_dir.iterdir()):
        if not args.force:
            raise McpHardDogfoodFailure(f"output directory is not empty: {out_dir}")
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    work_dir = out_dir / "workspace"
    work_dir.mkdir(parents=True, exist_ok=True)

    dataset = dataset_report()
    suites = run_suite_reports(out_dir)
    v1 = run_v1_hard_mcp(binary, work_dir)
    ontology = run_v1_ontology_mcp(binary, work_dir)
    v2 = run_v2_hard_mcp(binary, work_dir)
    seeded_faults = seeded_fault_report()
    scorecard = build_scorecard(v1, ontology, v2, suites, seeded_faults)
    equivalence = build_equivalence(dataset, v1, ontology, v2, suites)
    status = "passed" if scorecard["ok"] and equivalence["ok"] else "failed"
    summary = {
        "schema_version": SCHEMA_VERSION,
        "command": "mcp-hard-dogfood",
        "status": status,
        "generated_at_unix": int(time.time()),
        "public_safe": True,
        "scorecard_ok": scorecard["ok"],
        "equivalence_ok": equivalence["ok"],
        "out_dir": str(out_dir),
    }

    write_json(out_dir / "dataset.json", dataset)
    write_json(out_dir / "suite-results.json", suites)
    write_json(out_dir / "v1-mcp-hard.json", v1)
    write_json(out_dir / "v1-mcp-ontology.json", ontology)
    write_json(out_dir / "v2-mcp-hard.json", v2)
    write_json(out_dir / "seeded-faults.json", seeded_faults)
    write_json(out_dir / "scorecard.json", scorecard)
    write_json(out_dir / "equivalence.json", equivalence)
    write_json(out_dir / "summary.json", summary)
    (out_dir / "report.md").write_text(
        render_markdown(summary, scorecard, equivalence, seeded_faults),
        encoding="utf-8",
    )
    if not args.keep_workspace:
        shutil.rmtree(work_dir, ignore_errors=True)
    return summary


def render_markdown(
    summary: dict[str, Any],
    scorecard: dict[str, Any],
    equivalence: dict[str, Any],
    seeded_faults: dict[str, Any],
) -> str:
    lines = [
        "# Mneme MCP Hard Dogfood",
        "",
        f"- Status: `{summary['status']}`",
        f"- Scorecard: `{str(summary['scorecard_ok']).lower()}`",
        f"- Dataset equivalence: `{str(summary['equivalence_ok']).lower()}`",
        f"- Seeded fault detection: `{seeded_faults['detection_rate']}`",
        "",
        "| Metric | Value |",
        "| --- | ---: |",
    ]
    for key in [
        "v1_recall_at_k",
        "v1_precision_at_k",
        "v1_citation_coverage",
        "v1_handoff_success_rate",
        "ontology_entity_f1",
        "ontology_relation_f1",
        "ontology_attribute_f1",
        "v2_handoff_success_rate",
        "v2_citation_coverage",
        "mcp_suite_pass_rate",
        "team_suite_mcp_pass_rate",
    ]:
        lines.append(f"| `{key}` | `{scorecard.get(key)}` |")
    lines.extend(["", "| Safety metric | Value |", "| --- | ---: |"])
    for key in [
        "v1_scope_leak_count",
        "v1_secret_leak_count",
        "ontology_scope_leak_count",
        "ontology_secret_leak_count",
        "v2_scope_leak_count",
        "v2_secret_leak_count",
        "v2_blocked_secret_count",
        "v2_quarantined_count",
    ]:
        lines.append(f"| `{key}` | `{scorecard.get(key)}` |")
    lines.extend(["", "| Equivalence check | Expected | Actual | OK |", "| --- | ---: | ---: | --- |"])
    for check in equivalence["checks"]:
        lines.append(
            f"| `{check['name']}` | `{check['expected']}` | `{check['actual']}` | `{str(check['ok']).lower()}` |"
        )
    lines.append("")
    lines.append("All records are synthetic and public-safe.")
    lines.append("")
    return "\n".join(lines)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def read_json(path: Path) -> dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def print_json(value: Any) -> None:
    print(json.dumps(value, indent=2, sort_keys=True))


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true")
    parser.add_argument("--check-dataset", action="store_true")
    parser.add_argument("--check-seeded-faults", action="store_true")
    parser.add_argument("--out-dir", type=Path, default=Path(tempfile.gettempdir()) / "mneme-mcp-hard-dogfood")
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--keep-workspace", action="store_true")
    parser.add_argument("--mneme-mcp-bin", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    try:
        if args.check_contract:
            print_json(contract())
        elif args.check_dataset:
            print_json(dataset_report())
        elif args.check_seeded_faults:
            report = seeded_fault_report()
            print_json(report)
            if not report["ok"]:
                return 1
        else:
            summary = write_bundle(args)
            print(f"mcp-hard-dogfood: {summary['status']} -> {summary['out_dir']}")
            if summary["status"] != "passed":
                return 1
    except (OSError, subprocess.CalledProcessError, McpHardDogfoodFailure) as error:
        print(f"mcp-hard-dogfood: error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
