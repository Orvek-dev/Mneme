#!/usr/bin/env python3
"""Smoke-test Mneme MCP client installation and continuity behavior.

The protocol checks call the local mneme-mcp stdio server directly. The client
checks use isolated temporary homes/workspaces so Codex, Claude Code, and
Cursor registrations do not mutate the user's real configuration.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
EXPECTED_TOOL_COUNT = 47
REQUIRED_TOOLS = [
    "mneme_mcp_status",
    "mneme_agent_guide",
    "mneme_task_start",
    "mneme_task_finish",
    "mneme_prepare_handoff",
    "mneme_v1_continuity_begin",
    "mneme_v1_continuity_end",
    "mneme_v1_continuity_handoff",
    "mneme_v1_outcome_template",
    "mneme_v1_outcome_status",
    "mneme_v1_outcome_judge",
    "mneme_v2_team_handoff",
]


class SmokeFailure(RuntimeError):
    """Raised when the smoke test cannot prove its contract."""


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
        "command": "mcp-client-continuity-smoke-contract",
        "expected_tool_count": EXPECTED_TOOL_COUNT,
        "required_tools": REQUIRED_TOOLS,
        "checks": [
            "stdio MCP initialize/tools-list",
            "mneme_mcp_status continuity contract",
            "V1 writer continuity begin/end",
            "server restart before reader",
            "V1 reader continuity handoff/begin",
            "missing end write-back does not create handoff memory",
            "wrong scope does not return scoped memory",
            "secret-like continuity memory is not returned in context",
            "Codex MCP registration in isolated CODEX_HOME",
            "Claude Code MCP health in isolated HOME",
            "Cursor agent MCP approval and tool discovery in isolated workspace",
        ],
        "privacy_policy": "all stores and client configs are temporary; raw local paths and logs are not committed",
    }


class McpClient:
    def __init__(self, binary: Path, v1_store: Path, team_store: Path):
        self.binary = binary
        self.v1_store = v1_store
        self.team_store = team_store
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
            ],
            cwd=ROOT,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.request("initialize")
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
        if exc_type is not None and self.process.stderr:
            stderr = self.process.stderr.read().strip()
            if stderr:
                print(stderr, file=sys.stderr)

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        if self.process is None or self.process.stdin is None or self.process.stdout is None:
            raise SmokeFailure("MCP process is not running")
        self.request_id += 1
        payload: dict[str, Any] = {"jsonrpc": "2.0", "id": self.request_id, "method": method}
        if params is not None:
            payload["params"] = params
        self.process.stdin.write(json.dumps(payload) + "\n")
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise SmokeFailure(f"MCP process returned no response: {stderr}")
        response = json.loads(line)
        if "error" in response:
            raise SmokeFailure(f"MCP {method} failed: {response['error']}")
        return response

    def tool(self, name: str, arguments: dict[str, Any] | None = None) -> dict[str, Any]:
        response = self.request("tools/call", {"name": name, "arguments": arguments or {}})
        result = response.get("result") or {}
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            raise SmokeFailure(f"MCP tool {name} returned no structuredContent")
        return structured


def text_in_context(report: dict[str, Any], needle: str) -> bool:
    context = report.get("context_pack") or {}
    items = context.get("items") or []
    blob = "\n".join(str(item.get("claim_text", "")) for item in items)
    return needle in blob


def context_item_count(report: dict[str, Any]) -> int:
    context = report.get("context_pack") or {}
    return len(context.get("items") or [])


def run_protocol_checks(binary: Path, work_dir: Path) -> dict[str, Any]:
    v1_store = work_dir / "protocol-v1.json"
    team_store = work_dir / "protocol-team.json"
    with McpClient(binary, v1_store, team_store) as client:
        tools = client.request("tools/list")
        tool_names = [
            tool.get("name")
            for tool in (((tools.get("result") or {}).get("tools") or []))
            if isinstance(tool, dict)
        ]
        missing = [tool for tool in REQUIRED_TOOLS if tool not in tool_names]
        if len(tool_names) != EXPECTED_TOOL_COUNT or missing:
            raise SmokeFailure(f"unexpected tool inventory: count={len(tool_names)} missing={missing}")
        status = client.tool("mneme_mcp_status")
        contract_status = status.get("continuity_contract") or {}
        for key in [
            "mcp_accessible",
            "begin_required",
            "end_write_back_required",
            "read_and_honor_required",
            "shared_scope_or_lineage_required",
            "sequential_handoff_required",
        ]:
            if contract_status.get(key) is not True:
                raise SmokeFailure(f"continuity contract missing true {key}")
        begin = client.tool(
            "mneme_v1_continuity_begin",
            {
                "task": "Write release continuity note",
                "agent": "codex",
                "lineage": "release-065",
                "scope": "project:release-065",
                "query": "release continuity",
            },
        )
        session_id = begin.get("session_id")
        if not session_id:
            raise SmokeFailure("continuity begin returned no session_id")
        end = client.tool(
            "mneme_v1_continuity_end",
            {
                "session_id": session_id,
                "agent": "codex",
                "lineage": "release-065",
                "scope": "project:release-065",
                "summary": "Codex wrote the release continuity note",
                "remember": ["agent should run release continuity smoke before publishing"],
            },
        )
        if end.get("write_back_ok") is not True:
            raise SmokeFailure("continuity end did not write back memory")

    with McpClient(binary, v1_store, team_store) as reader:
        handoff = reader.tool(
            "mneme_v1_continuity_handoff",
            {
                "agent": "claude-code",
                "lineage": "release-065",
                "scope": "project:release-065",
                "query": "release continuity smoke",
            },
        )
        if handoff.get("source_session_count") != 1:
            raise SmokeFailure("reader handoff did not find the closed writer session")
        if not text_in_context(handoff, "release continuity smoke"):
            raise SmokeFailure("reader handoff did not retrieve the writer memory")
        reader_begin = reader.tool(
            "mneme_v1_continuity_begin",
            {
                "task": "Continue release validation",
                "agent": "claude-code",
                "lineage": "release-065",
                "scope": "project:release-065",
                "query": "release continuity smoke",
            },
        )
        report = reader_begin.get("report") or {}
        if not text_in_context(report, "release continuity smoke"):
            raise SmokeFailure("reader begin did not read and honor scoped continuity memory")

    missing_end_store = work_dir / "missing-end-v1.json"
    missing_end_team = work_dir / "missing-end-team.json"
    with McpClient(binary, missing_end_store, missing_end_team) as writer:
        writer.tool(
            "mneme_v1_continuity_begin",
            {
                "task": "Open unfinished task",
                "agent": "codex",
                "lineage": "missing-end",
                "scope": "project:missing-end",
                "query": "unfinished task",
            },
        )
    with McpClient(binary, missing_end_store, missing_end_team) as reader:
        handoff = reader.tool(
            "mneme_v1_continuity_handoff",
            {
                "agent": "claude-code",
                "lineage": "missing-end",
                "scope": "project:missing-end",
                "query": "unfinished task",
            },
        )
        if handoff.get("source_session_count") != 0 or context_item_count(handoff) != 0:
            raise SmokeFailure("unfinished session produced handoff memory without end write-back")

    wrong_scope_store = work_dir / "wrong-scope-v1.json"
    wrong_scope_team = work_dir / "wrong-scope-team.json"
    with McpClient(binary, wrong_scope_store, wrong_scope_team) as client:
        begin = client.tool(
            "mneme_v1_continuity_begin",
            {
                "task": "Write scoped task",
                "agent": "codex",
                "lineage": "scope-check",
                "scope": "project:allowed",
                "query": "strict scope",
            },
        )
        client.tool(
            "mneme_v1_continuity_end",
            {
                "session_id": begin["session_id"],
                "agent": "codex",
                "lineage": "scope-check",
                "scope": "project:allowed",
                "summary": "Scoped memory written",
                "remember": ["agent should only show strict scope memory in the allowed scope"],
            },
        )
        wrong = client.tool(
            "mneme_v1_continuity_handoff",
            {
                "agent": "claude-code",
                "lineage": "scope-check",
                "scope": "project:denied",
                "query": "strict scope memory",
            },
        )
        if context_item_count(wrong) != 0 or text_in_context(wrong, "strict scope memory"):
            raise SmokeFailure("wrong scope returned scoped continuity memory")

    secret_store = work_dir / "secret-v1.json"
    secret_team = work_dir / "secret-team.json"
    with McpClient(binary, secret_store, secret_team) as client:
        begin = client.tool(
            "mneme_v1_continuity_begin",
            {
                "task": "Write secret-like note",
                "agent": "codex",
                "lineage": "secret-check",
                "scope": "project:secret-check",
                "query": "secret token",
            },
        )
        client.tool(
            "mneme_v1_continuity_end",
            {
                "session_id": begin["session_id"],
                "agent": "codex",
                "lineage": "secret-check",
                "scope": "project:secret-check",
                "summary": "Secret-like note rejected from context",
                "remember": ["agent token API_KEY=FAKE_TEST_VALUE should not be reused"],
            },
        )
        secret = client.tool(
            "mneme_v1_continuity_handoff",
            {
                "agent": "claude-code",
                "lineage": "secret-check",
                "scope": "project:secret-check",
                "query": "API_KEY token",
            },
        )
        if text_in_context(secret, "API_KEY") or text_in_context(secret, "FAKE_TEST_VALUE"):
            raise SmokeFailure("secret-like continuity memory leaked into context")

    gate_store = work_dir / "outcome-gate-v1.json"
    gate_team = work_dir / "outcome-gate-team.json"
    with McpClient(binary, gate_store, gate_team) as client:
        begin = client.tool(
            "mneme_task_start",
            {
                "task": "Verify MCP outcome gate",
                "agent": "codex",
                "lineage": "outcome-gate",
                "scope": "project:outcome-gate",
                "query": "MCP outcome gate",
                "acceptance": {
                    "schema_version": "mneme.acceptance.v1",
                    "task_id": "mcp-outcome-gate",
                    "criteria": [
                        {
                            "id": "manual-check",
                            "kind": "command",
                            "command": {"argv": ["true"], "expect_exit": 0},
                        }
                    ],
                },
            },
        )
        if begin.get("acceptance_enabled") is not True:
            raise SmokeFailure("MCP task_start did not attach acceptance gate")
        gated_session_id = begin.get("session_id")
        finish = client.tool(
            "mneme_task_finish",
            {
                "session_id": gated_session_id,
                "agent": "codex",
                "lineage": "outcome-gate",
                "scope": "project:outcome-gate",
                "summary": "MCP outcome gate intentionally failed",
                "verifier_report": {
                    "schema_version": "mneme.verifier.v1",
                    "task_id": "mcp-outcome-gate",
                    "verifier": "mcp-client-continuity-smoke",
                    "results": [
                        {
                            "id": "manual-check",
                            "status": "fail",
                            "evidence": "smoke test rejected completion",
                        }
                    ],
                },
            },
        )
        if finish.get("completion_ok") is not False or finish.get("handoff_allowed") is not False:
            raise SmokeFailure("MCP task_finish did not expose failed gate completion guard")
        guarded_handoff = client.tool(
            "mneme_prepare_handoff",
            {
                "session_id": gated_session_id,
                "agent": "claude-code",
                "lineage": "outcome-gate",
                "scope": "project:outcome-gate",
                "query": "MCP outcome gate",
            },
        )
        if guarded_handoff.get("handoff_allowed") is not False:
            raise SmokeFailure("MCP prepare_handoff did not block unfinished gated handoff")
        if (guarded_handoff.get("gate_result") or {}).get("status") != "failed":
            raise SmokeFailure("MCP prepare_handoff did not return failed gate result")

    return {
        "ok": True,
        "tool_count": EXPECTED_TOOL_COUNT,
        "required_tools_present": True,
        "status_contract": "passed",
        "cross_agent_continuity": "passed",
        "server_restart_handoff": "passed",
        "missing_end_write_back_guard": "passed",
        "wrong_scope_guard": "passed",
        "secret_context_guard": "passed",
        "outcome_gate_handoff_guard": "passed",
    }


def run_command(
    command: list[str],
    *,
    env: dict[str, str] | None = None,
    cwd: Path | None = None,
) -> subprocess.CompletedProcess[str]:
    merged_env = os.environ.copy()
    if env:
        merged_env.update(env)
    return subprocess.run(
        command,
        cwd=cwd or ROOT,
        env=merged_env,
        text=True,
        capture_output=True,
    )


def command_status(name: str) -> dict[str, Any]:
    return {
        "client": name,
        "available": shutil.which(name) is not None,
        "status": "skipped",
        "checks": [],
    }


def run_codex_check(binary: Path, work_dir: Path, require: bool) -> dict[str, Any]:
    result = command_status("codex")
    if not result["available"]:
        if require:
            raise SmokeFailure("codex command not found")
        return result
    codex_home = work_dir / "codex-home"
    codex_home.mkdir(parents=True, exist_ok=True)
    env = {"CODEX_HOME": str(codex_home)}
    add = run_command(
        [
            "codex",
            "mcp",
            "add",
            "mneme",
            "--",
            str(binary),
            "--mode",
            "all",
            "--v1-store",
            str(work_dir / "codex-v1.json"),
            "--team-store",
            str(work_dir / "codex-team.json"),
        ],
        env=env,
    )
    listing = run_command(["codex", "mcp", "list"], env=env)
    get = run_command(["codex", "mcp", "get", "mneme"], env=env)
    if add.returncode != 0 or listing.returncode != 0 or get.returncode != 0:
        raise SmokeFailure("Codex MCP registration failed")
    if "mneme" not in listing.stdout or "enabled" not in listing.stdout or "transport: stdio" not in get.stdout:
        raise SmokeFailure("Codex MCP registration output did not confirm stdio enabled server")
    result.update(
        {
            "status": "passed",
            "checks": ["mcp add", "mcp list", "mcp get"],
            "tool_call_note": "Codex CLI exposes registration checks; protocol tool calls are covered by the stdio continuity smoke.",
        }
    )
    return result


def run_claude_check(binary: Path, work_dir: Path, require: bool) -> dict[str, Any]:
    result = command_status("claude")
    if not result["available"]:
        if require:
            raise SmokeFailure("claude command not found")
        return result
    home = work_dir / "claude-home"
    xdg = work_dir / "claude-config"
    home.mkdir(parents=True, exist_ok=True)
    xdg.mkdir(parents=True, exist_ok=True)
    env = {"HOME": str(home), "XDG_CONFIG_HOME": str(xdg)}
    add = run_command(
        [
            "claude",
            "mcp",
            "add",
            "--transport",
            "stdio",
            "--scope",
            "user",
            "mneme",
            "--",
            str(binary),
            "--mode",
            "all",
            "--v1-store",
            str(work_dir / "claude-v1.json"),
            "--team-store",
            str(work_dir / "claude-team.json"),
        ],
        env=env,
    )
    listing = run_command(["claude", "mcp", "list"], env=env)
    get = run_command(["claude", "mcp", "get", "mneme"], env=env)
    if add.returncode != 0 or listing.returncode != 0 or get.returncode != 0:
        raise SmokeFailure("Claude Code MCP registration failed")
    if "mneme" not in listing.stdout or "Connected" not in listing.stdout or "Status:" not in get.stdout:
        raise SmokeFailure("Claude Code did not report the Mneme MCP server as connected")
    result.update(
        {
            "status": "passed",
            "checks": ["mcp add", "mcp list health", "mcp get connected"],
        }
    )
    return result


def run_cursor_check(binary: Path, work_dir: Path, require: bool) -> dict[str, Any]:
    result = command_status("cursor")
    if not result["available"]:
        if require:
            raise SmokeFailure("cursor command not found")
        return result
    home = work_dir / "cursor-home"
    workspace = work_dir / "cursor-workspace"
    mcp_dir = workspace / ".cursor"
    home.mkdir(parents=True, exist_ok=True)
    mcp_dir.mkdir(parents=True, exist_ok=True)
    (mcp_dir / "mcp.json").write_text(
        json.dumps(
            {
                "mcpServers": {
                    "mneme": {
                        "command": str(binary),
                        "args": [
                            "--mode",
                            "all",
                            "--v1-store",
                            str(work_dir / "cursor-v1.json"),
                            "--team-store",
                            str(work_dir / "cursor-team.json"),
                        ],
                    }
                }
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    env = {"HOME": str(home)}
    enable = run_command(["cursor", "agent", "--trust", "mcp", "enable", "mneme"], env=env, cwd=workspace)
    listing = run_command(["cursor", "agent", "--trust", "mcp", "list"], env=env, cwd=workspace)
    tools = run_command(
        ["cursor", "agent", "--trust", "mcp", "list-tools", "mneme"],
        env=env,
        cwd=workspace,
    )
    if enable.returncode != 0 or listing.returncode != 0 or tools.returncode != 0:
        raise SmokeFailure("Cursor agent MCP tool discovery failed")
    if "mneme: ready" not in listing.stdout or f"Tools for mneme ({EXPECTED_TOOL_COUNT})" not in tools.stdout:
        raise SmokeFailure("Cursor agent did not report Mneme ready with the expected tool count")
    for tool in REQUIRED_TOOLS:
        if tool not in tools.stdout:
            raise SmokeFailure(f"Cursor agent tool listing missed {tool}")
    result.update(
        {
            "status": "passed",
            "checks": ["workspace mcp.json", "mcp enable", "mcp list ready", "mcp list-tools"],
            "tool_count": EXPECTED_TOOL_COUNT,
        }
    )
    return result


def build_binary(args: argparse.Namespace) -> Path:
    binary = Path(args.mneme_mcp_bin) if args.mneme_mcp_bin else ROOT / "target" / "debug" / "mneme-mcp"
    if not args.no_build:
        result = run_command(["cargo", "build", "-q", "-p", "mneme-mcp"])
        if result.returncode != 0:
            raise SmokeFailure(f"cargo build failed:\n{result.stdout}\n{result.stderr}")
    if not binary.exists():
        raise SmokeFailure(f"mneme-mcp binary not found: {binary}")
    return binary.resolve()


def run(args: argparse.Namespace) -> dict[str, Any]:
    binary = build_binary(args)
    with tempfile.TemporaryDirectory(prefix="mneme-mcp-client-smoke-") as raw_tmp:
        work_dir = Path(raw_tmp)
        protocol = run_protocol_checks(binary, work_dir)
        clients = []
        if not args.protocol_only:
            clients.append(run_codex_check(binary, work_dir, args.require_clients))
            clients.append(run_claude_check(binary, work_dir, args.require_clients))
            clients.append(run_cursor_check(binary, work_dir, args.require_clients))
        skipped = [client["client"] for client in clients if client["status"] == "skipped"]
        if args.require_clients and skipped:
            raise SmokeFailure(f"required clients skipped: {', '.join(skipped)}")
        ok = protocol["ok"] and all(client["status"] in {"passed", "skipped"} for client in clients)
        return {
            "schema_version": SCHEMA_VERSION,
            "command": "mcp-client-continuity-smoke",
            "ok": ok,
            "public_safe": True,
            "raw_logs_committed": False,
            "protocol": protocol,
            "clients": clients,
        }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true", help="Print the public smoke contract and exit.")
    parser.add_argument("--protocol-only", action="store_true", help="Skip Codex/Claude/Cursor CLI checks.")
    parser.add_argument("--require-clients", action="store_true", help="Fail if Codex, Claude, or Cursor is unavailable.")
    parser.add_argument("--no-build", action="store_true", help="Use an existing target/debug/mneme-mcp binary.")
    parser.add_argument("--mneme-mcp-bin", help="Path to the mneme-mcp binary.")
    parser.add_argument("--out", help="Write the JSON report to this path.")
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    try:
        report = contract() if args.check_contract else run(args)
        text = json.dumps(report, indent=2, sort_keys=True)
        if args.out:
            Path(args.out).write_text(text + "\n", encoding="utf-8")
        print(text)
    except SmokeFailure as error:
        print(json.dumps({"ok": False, "error": str(error)}, indent=2, sort_keys=True), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
