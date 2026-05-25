#!/usr/bin/env python3
"""Minimal stdio adapter for exposing Mneme v2 team-memory tools.

This script intentionally stays thin: it delegates every operation to the
public `mneme team ... --json` CLI so policy, ACL, firewall, and sync behavior
remain owned by mneme-core.
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from typing import Any


TOOLS = [
    {
        "name": "mneme_team_context",
        "description": "Read a policy-filtered Mneme v2 team context pack.",
        "inputSchema": {
            "type": "object",
            "required": ["query", "actor"],
            "properties": {
                "query": {"type": "string"},
                "actor": {"type": "string"},
                "agent": {"type": "string"},
                "max_items": {"type": "integer"},
            },
        },
    },
    {
        "name": "mneme_team_remember",
        "description": "Write scoped Mneme v2 team memory through policy.",
        "inputSchema": {
            "type": "object",
            "required": ["text", "actor", "scope"],
            "properties": {
                "text": {"type": "string"},
                "actor": {"type": "string"},
                "agent": {"type": "string"},
                "scope": {"type": "string"},
            },
        },
    },
    {
        "name": "mneme_team_handoff",
        "description": "Build a policy-filtered agent handoff package.",
        "inputSchema": {
            "type": "object",
            "required": ["query", "actor"],
            "properties": {
                "query": {"type": "string"},
                "actor": {"type": "string"},
                "agent": {"type": "string"},
                "max_items": {"type": "integer"},
            },
        },
    },
    {
        "name": "mneme_team_promote",
        "description": "Create a reviewable team-memory promotion candidate.",
        "inputSchema": {
            "type": "object",
            "required": ["memory_id", "actor"],
            "properties": {
                "memory_id": {"type": "string"},
                "actor": {"type": "string"},
                "agent": {"type": "string"},
                "note": {"type": "string"},
            },
        },
    },
    {
        "name": "mneme_team_review",
        "description": "Approve or reject a promotion candidate.",
        "inputSchema": {
            "type": "object",
            "required": ["promotion_id", "actor", "approve"],
            "properties": {
                "promotion_id": {"type": "string"},
                "actor": {"type": "string"},
                "agent": {"type": "string"},
                "approve": {"type": "boolean"},
            },
        },
    },
    {
        "name": "mneme_team_firewall",
        "description": "Scan active team memory for leakage or poisoning risk.",
        "inputSchema": {"type": "object", "properties": {}},
    },
    {
        "name": "mneme_team_ontology",
        "description": "Return actor-scoped v2 entity, relation, and attribute projection.",
        "inputSchema": {
            "type": "object",
            "required": ["actor"],
            "properties": {
                "actor": {"type": "string"},
                "agent": {"type": "string"},
            },
        },
    },
]


def main() -> int:
    parser = argparse.ArgumentParser(description="Mneme v2 team-memory stdio adapter")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        print(json.dumps({"ok": True, "tool_count": len(TOOLS), "tools": [t["name"] for t in TOOLS]}))
        return 0

    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
            response = handle_request(request)
        except Exception as error:  # noqa: BLE001 - adapter must report JSON-RPC errors.
            response = error_response(None, -32603, str(error))
        sys.stdout.write(json.dumps(response) + "\n")
        sys.stdout.flush()
    return 0


def handle_request(request: dict[str, Any]) -> dict[str, Any]:
    request_id = request.get("id")
    method = request.get("method")
    if method == "initialize":
        return result_response(
            request_id,
            {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "mneme-team-memory", "version": "0.62.0"},
            },
        )
    if method == "tools/list":
        return result_response(request_id, {"tools": TOOLS})
    if method == "tools/call":
        params = request.get("params") or {}
        name = params.get("name")
        arguments = params.get("arguments") or {}
        return result_response(request_id, call_tool(name, arguments))
    if method == "notifications/initialized":
        return result_response(request_id, {})
    return error_response(request_id, -32601, f"unknown method: {method}")


def call_tool(name: str, arguments: dict[str, Any]) -> dict[str, Any]:
    if name == "mneme_team_context":
        output = run_mneme(["team", "context", require(arguments, "query"), *actor_args(arguments), *max_args(arguments)])
    elif name == "mneme_team_remember":
        output = run_mneme(["team", "remember", require(arguments, "text"), *actor_args(arguments), "--scope", require(arguments, "scope")])
    elif name == "mneme_team_handoff":
        output = run_mneme(["team", "handoff", require(arguments, "query"), *actor_args(arguments), *max_args(arguments)])
    elif name == "mneme_team_promote":
        command = ["team", "promote", require(arguments, "memory_id"), *actor_args(arguments)]
        if arguments.get("note"):
            command.extend(["--note", str(arguments["note"])])
        output = run_mneme(command)
    elif name == "mneme_team_review":
        command = ["team", "review", require(arguments, "promotion_id"), *actor_args(arguments)]
        command.append("--approve" if bool(arguments.get("approve")) else "--reject")
        output = run_mneme(command)
    elif name == "mneme_team_firewall":
        output = run_mneme(["team", "firewall"])
    elif name == "mneme_team_ontology":
        output = run_mneme(["team", "ontology", *actor_args(arguments)])
    else:
        raise ValueError(f"unknown tool: {name}")
    return {"content": [{"type": "text", "text": output}]}


def run_mneme(args: list[str]) -> str:
    binary = os.environ.get("MNEME_BIN", "mneme")
    store = os.environ.get("MNEME_TEAM_STORE")
    command = [binary, *args, "--json"]
    if store:
        command.extend(["--store", store])
    completed = subprocess.run(command, text=True, capture_output=True, check=False, timeout=30)
    if completed.returncode != 0:
        raise RuntimeError((completed.stderr or completed.stdout).strip())
    return completed.stdout.strip()


def actor_args(arguments: dict[str, Any]) -> list[str]:
    args = ["--actor", require(arguments, "actor")]
    if arguments.get("agent"):
        args.extend(["--agent", str(arguments["agent"])])
    return args


def max_args(arguments: dict[str, Any]) -> list[str]:
    if arguments.get("max_items") is None:
        return []
    return ["--max-items", str(arguments["max_items"])]


def require(arguments: dict[str, Any], key: str) -> str:
    value = arguments.get(key)
    if value is None or str(value).strip() == "":
        raise ValueError(f"missing required argument: {key}")
    return str(value)


def result_response(request_id: Any, result: dict[str, Any]) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "result": result}


def error_response(request_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": request_id, "error": {"code": code, "message": message}}


if __name__ == "__main__":
    raise SystemExit(main())
