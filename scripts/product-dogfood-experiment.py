#!/usr/bin/env python3
"""Prepare a public-safe Mneme ON/OFF dogfood experiment bundle.

The bundle is designed for real agent runs and blinded human review. It does
not execute or score an agent by itself, because doing so would recreate the
self-scoring failure mode this project is trying to avoid.
"""

from __future__ import annotations

import argparse
import json
import shutil
import subprocess
import sys
import time
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1


class DogfoodExperimentFailure(RuntimeError):
    """Raised when a dogfood experiment bundle cannot be prepared."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


TASKS: list[dict[str, Any]] = [
    {
        "id": "storage-migration-plan",
        "title": "Decide whether to ship SQLite migration now",
        "scope": "private",
        "query": "sqlite migration evidence",
        "memory": [
            "project should defer sqlite migration until migration evidence exists",
            "migration decision requires product-validation evidence",
        ],
        "task_prompt": "Write a short release decision note for the storage layer. Include the decision, risk, and next evidence needed.",
        "review_focus": "Does the artifact avoid premature SQLite migration and cite the needed evidence?",
    },
    {
        "id": "handoff-next-action",
        "title": "Prepare next-agent handoff instructions",
        "scope": "private",
        "query": "next agent mcp extractor",
        "memory": [
            "next agent should update MCP guide before extractor work",
            "handoff carries finish summary and next actions",
        ],
        "task_prompt": "Write the next-agent handoff note for MCP/extractor work. Keep it operational and cite any context used.",
        "review_focus": "Does the artifact prioritize the MCP guide before extractor work?",
    },
    {
        "id": "current-report-schedule",
        "title": "Choose current eval report schedule",
        "scope": "private",
        "query": "current eval report schedule",
        "memory": [
            "user prefers weekly eval reports on Monday mornings",
            "weekly eval reports schedule Monday mornings",
        ],
        "task_prompt": "Write the current reporting cadence note. Avoid obsolete schedules and explain the current cadence.",
        "review_focus": "Does the artifact use Monday mornings and avoid obsolete Friday language?",
    },
    {
        "id": "project-alpha-rollback",
        "title": "Draft release gate for Project Alpha",
        "scope": "project-alpha",
        "query": "rollback proof",
        "memory": [
            "project alpha deploy requires rollback proof before release",
        ],
        "task_prompt": "Write a Project Alpha release gate note. Include the minimum release-blocking evidence.",
        "review_focus": "Does the artifact require rollback proof and avoid unrelated project memory?",
    },
    {
        "id": "public-readme-claim",
        "title": "Edit public evidence claim wording",
        "scope": "private",
        "query": "external validation claim",
        "memory": [
            "Mneme public README must not claim third-party validation without blinded review data",
            "P6 review schema validates evidence format but does not prove external value",
        ],
        "task_prompt": "Draft a README evidence sentence that is accurate and not overclaimed.",
        "review_focus": "Does the artifact clearly avoid third-party value claims without review data?",
    },
]


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-dogfood-experiment-contract",
        "condition_count_per_task": 2,
        "task_count": len(TASKS),
        "outputs": [
            "assignment.private.json",
            "conditions/<task>/<condition>/prompt.md",
            "conditions/<task>/<condition>/mneme-store.json",
            "review-template.json",
            "summary.json",
        ],
        "claim_policy": "bundle preparation is not product-value evidence; agent execution plus blinded review is required",
    }


def prepare_bundle(args: argparse.Namespace) -> dict[str, Any]:
    out_dir = args.out_dir
    if out_dir.exists():
        if not args.force:
            raise DogfoodExperimentFailure(f"output directory exists: {out_dir}")
        shutil.rmtree(out_dir)
    out_dir.mkdir(parents=True)
    binary = ensure_cli(args.no_build)
    condition_root = out_dir / "conditions"
    assignment: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "warning": "Private local mapping. Do not send to reviewers.",
        "tasks": {},
    }
    review_tasks: list[dict[str, Any]] = []
    for index, task in enumerate(TASKS[: args.task_count]):
        on_label = "condition_a" if index % 2 == 0 else "condition_b"
        off_label = "condition_b" if on_label == "condition_a" else "condition_a"
        assignment["tasks"][task["id"]] = {
            on_label: "mneme_on",
            off_label: "mneme_off",
        }
        for label, mode in [(on_label, "mneme_on"), (off_label, "mneme_off")]:
            condition_dir = condition_root / task["id"] / label
            condition_dir.mkdir(parents=True)
            store = condition_dir / "mneme-store.json"
            if mode == "mneme_on":
                write_memory_store(binary, store, task)
            else:
                initialize_empty_store(binary, store)
            (condition_dir / "prompt.md").write_text(
                prompt_for_condition(task, label, mode, store),
                encoding="utf-8",
            )
            (condition_dir / "artifact.md").write_text(
                "# Agent Artifact Placeholder\n\nRun the assigned agent and replace this file with its output.\n",
                encoding="utf-8",
            )
        review_tasks.append(
            {
                "id": task["id"],
                "title": task["title"],
                "review_focus": task["review_focus"],
                "conditions": [
                    {
                        "label": "condition_a",
                        "artifact_path": f"conditions/{task['id']}/condition_a/artifact.md",
                        "score": None,
                        "citation_fidelity": None,
                        "correction_count": None,
                        "rework_count": None,
                    },
                    {
                        "label": "condition_b",
                        "artifact_path": f"conditions/{task['id']}/condition_b/artifact.md",
                        "score": None,
                        "citation_fidelity": None,
                        "correction_count": None,
                        "rework_count": None,
                    },
                ],
            }
        )
    review_template = {
        "schema_version": "mneme.product_dogfood_review.v1",
        "run_label": args.run_label,
        "condition_labels_blinded": True,
        "reviewer_id": "",
        "reviewer_is_project_author": None,
        "third_party_claim": False,
        "public_safe": True,
        "raw_transcript_included": False,
        "tasks": review_tasks,
    }
    summary = {
        "schema_version": SCHEMA_VERSION,
        "command": "product-dogfood-experiment",
        "ok": True,
        "status": "bundle_ready",
        "run_label": args.run_label,
        "task_count": len(review_tasks),
        "actual_agent_execution": False,
        "blind_review_completed": False,
        "external_value_claim_allowed": False,
        "requires": [
            "Run each prompt with the assigned coding agent.",
            "Replace each artifact.md placeholder with real agent output.",
            "Give artifacts, not assignment.private.json, to a reviewer.",
            "Convert review results into product-validation-review JSON.",
            "Run scripts/product-review-summary.py before any public value claim.",
        ],
    }
    write_json(out_dir / "assignment.private.json", assignment)
    write_json(out_dir / "review-template.json", review_template)
    write_json(out_dir / "summary.json", summary)
    return summary


def ensure_cli(no_build: bool) -> Path:
    binary = ROOT / "target" / "debug" / "mneme"
    if not no_build:
        run_command(["cargo", "build", "-q", "-p", "mneme-cli"])
    if not binary.exists():
        raise DogfoodExperimentFailure("target/debug/mneme is missing")
    return binary


def write_memory_store(binary: Path, store: Path, task: dict[str, Any]) -> None:
    for claim in task["memory"]:
        run_command(
            [
                str(binary),
                "remember",
                claim,
                "--scope",
                task["scope"],
                "--store",
                str(store),
            ]
        )


def initialize_empty_store(binary: Path, store: Path) -> None:
    run_command([str(binary), "init", "--store", str(store), "--no-bin", "--force", "--json"])


def prompt_for_condition(task: dict[str, Any], label: str, mode: str, store: Path) -> str:
    lines = [
        f"# {task['title']} ({label})",
        "",
        "You are running one condition of a Mneme product dogfood experiment.",
        "Do not mention the hidden condition mapping in the final artifact.",
        "",
        "## Task",
        "",
        task["task_prompt"],
        "",
        "## Output",
        "",
        "Write the final artifact to `artifact.md` in this condition directory.",
        "Keep the artifact public-safe: no private paths, tokens, raw transcripts, or unrelated local details.",
        "",
    ]
    if mode == "mneme_on":
        lines.extend(
            [
                "## Mneme Context",
                "",
                "Use Mneme before writing the artifact:",
                "",
                "```sh",
                f"target/debug/mneme context \"{task['query']}\" --scope {task['scope']} --store {store} --json",
                "```",
                "",
                "Use cited Mneme memory only when it directly supports the artifact.",
            ]
        )
    else:
        lines.extend(
            [
                "## Mneme Context",
                "",
                "Do not use Mneme for this condition. Work only from the task prompt.",
            ]
        )
    lines.append("")
    return "\n".join(lines)


def check_bundle(path: Path) -> dict[str, Any]:
    errors: list[str] = []
    for relative in ["assignment.private.json", "review-template.json", "summary.json"]:
        if not (path / relative).exists():
            errors.append(f"missing {relative}")
    summary_path = path / "summary.json"
    summary = json.loads(summary_path.read_text(encoding="utf-8")) if summary_path.exists() else {}
    for task in TASKS[: int(summary.get("task_count", 0))]:
        for condition in ["condition_a", "condition_b"]:
            prompt = path / "conditions" / task["id"] / condition / "prompt.md"
            artifact = path / "conditions" / task["id"] / condition / "artifact.md"
            store = path / "conditions" / task["id"] / condition / "mneme-store.json"
            for file_path in [prompt, artifact, store]:
                if not file_path.exists():
                    errors.append(f"missing {file_path.relative_to(path)}")
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-dogfood-experiment-check",
        "ok": not errors,
        "bundle": str(path),
        "errors": errors,
        "external_value_claim_allowed": False,
    }


def run_command(args: list[str]) -> None:
    result = subprocess.run(args, cwd=ROOT, text=True, capture_output=True)
    if result.returncode != 0:
        raise DogfoodExperimentFailure(
            f"command failed ({result.returncode}): {' '.join(args)}\n{result.stderr}"
        )


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true")
    parser.add_argument("--check-bundle", type=Path)
    parser.add_argument("--out-dir", type=Path)
    parser.add_argument("--run-label", default="local-product-dogfood")
    parser.add_argument("--task-count", type=int, default=len(TASKS))
    parser.add_argument("--force", action="store_true")
    parser.add_argument("--no-build", action="store_true")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    try:
        if args.check_contract:
            print(json.dumps(contract(), indent=2, sort_keys=True))
            return 0
        if args.check_bundle:
            report = check_bundle(args.check_bundle)
            print(json.dumps(report, indent=2, sort_keys=True))
            return 0 if report["ok"] else 1
        if args.out_dir is None:
            args.out_dir = ROOT / "evals" / "runs" / "product-dogfood-experiment" / args.run_label
        if args.task_count < 1 or args.task_count > len(TASKS):
            raise DogfoodExperimentFailure(f"task-count must be 1..{len(TASKS)}")
        summary = prepare_bundle(args)
        print(json.dumps(summary, indent=2, sort_keys=True))
        return 0
    except (DogfoodExperimentFailure, OSError, json.JSONDecodeError) as error:
        print(f"product-dogfood-experiment: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
