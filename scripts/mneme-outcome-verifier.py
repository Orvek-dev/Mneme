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
from pathlib import Path
from typing import Any

REPORT_SCHEMA = "mneme.verifier.v1"


def main() -> int:
    request = json.load(sys.stdin)
    acceptance = request.get("acceptance") or {}
    baseline = acceptance.get("baseline") or {}
    workspace = Path(baseline.get("worktree") or request.get("workspace") or os.getcwd())
    task_id = acceptance.get("task_id")
    results: list[dict[str, Any]] = []
    changed_files = get_changed_files(workspace, baseline.get("diff_base") or baseline.get("git_head"))

    for criterion in acceptance.get("criteria") or []:
        criterion_id = str(criterion.get("id") or "")
        kind = criterion.get("kind")
        config = criterion.get("config") or {}
        if kind == "judgment":
            continue
        try:
            if kind == "command":
                results.append(check_command(criterion_id, config, workspace))
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


def check_command(criterion_id: str, config: dict[str, Any], workspace: Path) -> dict[str, Any]:
    expect_exit = int(config.get("expect_exit", 0))
    cwd = Path(config.get("cwd") or workspace)
    if config.get("shell") is True:
        command = config.get("run")
        if not isinstance(command, str) or not command.strip():
            return result(criterion_id, "error", "shell command requires non-empty run")
        completed = subprocess.run(command, cwd=cwd, shell=True, capture_output=True, text=True, timeout=120)
        label = command
    else:
        argv = config.get("argv")
        if not isinstance(argv, list) or not argv or not all(isinstance(part, str) for part in argv):
            return result(criterion_id, "error", "command criterion requires argv string array")
        completed = subprocess.run(argv, cwd=cwd, shell=False, capture_output=True, text=True, timeout=120)
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


def get_changed_files(workspace: Path, diff_base: str | None) -> set[str]:
    changed: set[str] = set()
    if diff_base and diff_base != "unknown":
        diff = run_git(workspace, ["diff", "--name-only", diff_base, "--"])
        changed.update(line.strip() for line in diff.splitlines() if line.strip())
    unstaged = run_git(workspace, ["diff", "--name-only", "--"])
    changed.update(line.strip() for line in unstaged.splitlines() if line.strip())
    staged = run_git(workspace, ["diff", "--cached", "--name-only", "--"])
    changed.update(line.strip() for line in staged.splitlines() if line.strip())
    untracked = run_git(workspace, ["ls-files", "--others", "--exclude-standard"])
    changed.update(line.strip() for line in untracked.splitlines() if line.strip())
    return changed


def run_git(workspace: Path, args: list[str]) -> str:
    completed = subprocess.run(["git", *args], cwd=workspace, capture_output=True, text=True, timeout=30)
    if completed.returncode != 0:
        return ""
    return completed.stdout


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
