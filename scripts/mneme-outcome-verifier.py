#!/usr/bin/env python3
"""External deterministic outcome verifier for Mneme MVP1.

The verifier reads ``mneme.verifier_request.v1`` JSON from stdin and emits a
``mneme.verifier.v1`` report to stdout. Mneme core owns the final gate result;
this script only proposes criterion outcomes.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

REPORT_SCHEMA = "mneme.verifier.v1"
DEFAULT_COMMAND_TIMEOUT_SECONDS = 120
DEFAULT_GIT_TIMEOUT_SECONDS = 30
DEFAULT_GLOBAL_TIMEOUT_SECONDS = 900
MAX_TIMEOUT_SECONDS = 3600


def main() -> int:
    request = json.load(sys.stdin)
    acceptance = request.get("acceptance") or {}
    baseline = acceptance.get("baseline") or {}
    workspace = Path(baseline.get("worktree") or request.get("workspace") or os.getcwd())
    task_id = acceptance.get("task_id")
    results: list[dict[str, Any]] = []
    deadline = time.monotonic() + bounded_timeout(
        os.environ.get("MNEME_VERIFIER_GLOBAL_TIMEOUT_SECONDS"),
        DEFAULT_GLOBAL_TIMEOUT_SECONDS,
    )
    changed_files = get_changed_files(workspace, baseline.get("diff_base") or baseline.get("git_head"), deadline)

    for criterion in acceptance.get("criteria") or []:
        criterion_id = str(criterion.get("id") or "")
        kind = criterion.get("kind")
        config = criterion.get("config") or {}
        if time.monotonic() >= deadline:
            results.append(result(criterion_id, "error", "global verifier timeout exceeded"))
            continue
        if kind == "judgment":
            continue
        try:
            if kind == "command":
                results.append(check_command(criterion_id, config, workspace, deadline))
            elif kind == "diff_touches":
                results.append(check_diff_touches(criterion_id, config, changed_files))
            elif kind == "diff_scope":
                results.append(check_diff_scope(criterion_id, config, changed_files))
            elif kind == "symbol_present":
                results.append(check_symbol_present(criterion_id, config, workspace))
            else:
                results.append(result(criterion_id, "error", f"unknown criterion kind: {kind}"))
        except Exception as exc:  # noqa: BLE001 - verifier must report per-criterion errors.
            results.append(result(criterion_id, "error", f"{type(exc).__name__}: {exc}"))

    json.dump(
        {
            "schema_version": REPORT_SCHEMA,
            "task_id": task_id,
            "verifier": "mneme-outcome-verifier.py",
            "results": results,
        },
        sys.stdout,
        sort_keys=True,
    )
    sys.stdout.write("\n")
    return 0


def check_command(criterion_id: str, config: dict[str, Any], workspace: Path, deadline: float) -> dict[str, Any]:
    expect_exit = int(config.get("expect_exit", 0))
    cwd = Path(config.get("cwd") or workspace)
    timeout = min(
        bounded_timeout(config.get("timeout_seconds"), DEFAULT_COMMAND_TIMEOUT_SECONDS),
        remaining_timeout(deadline),
    )
    if config.get("shell") is True:
        allow_shell = config.get("allow_shell") is True or os.environ.get("MNEME_VERIFIER_ALLOW_SHELL") == "1"
        if not allow_shell:
            return result(criterion_id, "error", "shell command requires allow_shell=true or MNEME_VERIFIER_ALLOW_SHELL=1")
        command = config.get("run")
        if not isinstance(command, str) or not command.strip():
            return result(criterion_id, "error", "shell command requires non-empty run")
        completed = subprocess.run(command, cwd=cwd, shell=True, capture_output=True, text=True, timeout=timeout)
        label = command
    else:
        argv = config.get("argv")
        if not isinstance(argv, list) or not argv or not all(isinstance(part, str) for part in argv):
            return result(criterion_id, "error", "command criterion requires argv string array")
        completed = subprocess.run(argv, cwd=cwd, shell=False, capture_output=True, text=True, timeout=timeout)
        label = " ".join(argv)
    status = "pass" if completed.returncode == expect_exit else "fail"
    evidence = f"{label} exited {completed.returncode}, expected {expect_exit}"
    if completed.stderr.strip():
        evidence = f"{evidence}; stderr={completed.stderr.strip()[:500]}"
    return result(criterion_id, status, evidence)


def check_diff_touches(criterion_id: str, config: dict[str, Any], changed_files: set[str]) -> dict[str, Any]:
    paths = config.get("paths") or config.get("path")
    expected = normalize_paths(paths)
    if not expected:
        return result(criterion_id, "error", "diff_touches requires paths")
    missing = [path for path in expected if path not in changed_files]
    status = "pass" if not missing else "fail"
    return result(
        criterion_id,
        status,
        f"changed={sorted(changed_files)} expected={expected} missing={missing}",
    )


def check_diff_scope(criterion_id: str, config: dict[str, Any], changed_files: set[str]) -> dict[str, Any]:
    allowed = normalize_paths(config.get("allowed_paths") or config.get("paths"))
    if not allowed:
        return result(criterion_id, "error", "diff_scope requires allowed_paths")
    out_of_scope = [
        path
        for path in changed_files
        if not any(path == allowed_path or path.startswith(f"{allowed_path.rstrip('/')}/") for allowed_path in allowed)
    ]
    status = "pass" if not out_of_scope else "fail"
    return result(
        criterion_id,
        status,
        f"changed={sorted(changed_files)} allowed={allowed} out_of_scope={out_of_scope}",
    )


def check_symbol_present(criterion_id: str, config: dict[str, Any], workspace: Path) -> dict[str, Any]:
    path_value = config.get("path")
    symbol = config.get("symbol")
    if not isinstance(path_value, str) or not path_value.strip() or not isinstance(symbol, str) or not symbol.strip():
        return result(criterion_id, "error", "symbol_present requires path and symbol")
    path = workspace / path_value
    text = path.read_text(encoding="utf-8")
    status = "pass" if symbol in text else "fail"
    return result(criterion_id, status, f"{symbol!r} {'found' if status == 'pass' else 'missing'} in {path_value}")


def get_changed_files(workspace: Path, diff_base: str | None, deadline: float) -> set[str]:
    changed: set[str] = set()
    if diff_base and diff_base != "unknown":
        diff = run_git(workspace, ["diff", "--name-only", diff_base, "--"], deadline)
        changed.update(line.strip() for line in diff.splitlines() if line.strip())
    unstaged = run_git(workspace, ["diff", "--name-only", "--"], deadline)
    changed.update(line.strip() for line in unstaged.splitlines() if line.strip())
    staged = run_git(workspace, ["diff", "--cached", "--name-only", "--"], deadline)
    changed.update(line.strip() for line in staged.splitlines() if line.strip())
    untracked = run_git(workspace, ["ls-files", "--others", "--exclude-standard"], deadline)
    changed.update(line.strip() for line in untracked.splitlines() if line.strip())
    return changed


def run_git(workspace: Path, args: list[str], deadline: float) -> str:
    timeout = min(
        bounded_timeout(os.environ.get("MNEME_VERIFIER_GIT_TIMEOUT_SECONDS"), DEFAULT_GIT_TIMEOUT_SECONDS),
        remaining_timeout(deadline),
    )
    completed = subprocess.run(["git", *args], cwd=workspace, capture_output=True, text=True, timeout=timeout)
    if completed.returncode != 0:
        return ""
    return completed.stdout


def bounded_timeout(value: Any, default: int) -> int:
    if value is None or value == "":
        return default
    try:
        timeout = int(value)
    except (TypeError, ValueError):
        return default
    return max(1, min(timeout, MAX_TIMEOUT_SECONDS))


def remaining_timeout(deadline: float) -> int:
    remaining = int(deadline - time.monotonic())
    return max(1, min(remaining, MAX_TIMEOUT_SECONDS))


def normalize_paths(value: Any) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, list):
        return [item for item in value if isinstance(item, str) and item.strip()]
    return []


def result(criterion_id: str, status: str, evidence: str) -> dict[str, Any]:
    return {"id": criterion_id, "status": status, "evidence": evidence}


if __name__ == "__main__":
    raise SystemExit(main())
