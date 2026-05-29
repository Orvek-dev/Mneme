#!/usr/bin/env python3
"""Validate and summarize public-safe Mneme product review artifacts.

This script is the bridge between local scripted checks and real product-value
evidence. It does not create value claims from synthetic fixtures. It only
allows a value claim when at least one public-safe, blinded, non-author review
explicitly marks itself as a third-party claim.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
REVIEW_SCHEMA_VERSION = "mneme.product_validation_review.v1"

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
LOCAL_PATH_RE = re.compile(r"/Users/|/home/|[A-Za-z]:\\\\")


class ReviewSummaryFailure(RuntimeError):
    """Raised when review summary generation fails."""


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-review-summary-contract",
        "review_schema_version": REVIEW_SCHEMA_VERSION,
        "required_review_fields": [
            "reviewer_id",
            "reviewer_is_project_author",
            "third_party_claim",
            "public_safe",
            "raw_transcript_included",
            "tasks",
        ],
        "required_task_fields": [
            "id",
            "condition_labels_blinded",
            "score_without_memory",
            "score_with_memory",
            "memory_helped",
            "memory_harmed",
            "citation_fidelity",
            "correction_count_without_memory",
            "correction_count_with_memory",
            "rework_count_without_memory",
            "rework_count_with_memory",
            "public_safe",
            "evidence_summary",
        ],
        "metrics": [
            "win_rate",
            "mean_score_delta",
            "memory_helped_rate",
            "memory_harmed_rate",
            "mean_citation_fidelity",
            "mean_correction_delta",
            "mean_rework_delta",
        ],
        "value_claim_policy": "requires valid public-safe blind review, reviewer_is_project_author=false, third_party_claim=true, and no raw transcript",
    }


def validate_review(review: dict[str, Any], source: str) -> tuple[list[dict[str, Any]], list[str]]:
    errors: list[str] = []
    if review.get("schema_version") != REVIEW_SCHEMA_VERSION:
        errors.append(f"{source}: schema_version must be {REVIEW_SCHEMA_VERSION}")
    if not isinstance(review.get("reviewer_id"), str) or not review["reviewer_id"].strip():
        errors.append(f"{source}: reviewer_id is required")
    for field in ["reviewer_is_project_author", "third_party_claim", "public_safe", "raw_transcript_included"]:
        if not isinstance(review.get(field), bool):
            errors.append(f"{source}: {field} must be boolean")
    if review.get("public_safe") is not True:
        errors.append(f"{source}: public_safe must be true")
    if review.get("raw_transcript_included") is not False:
        errors.append(f"{source}: raw_transcript_included must be false")
    serialized = json.dumps(review, ensure_ascii=False)
    if SECRET_RE.search(serialized):
        errors.append(f"{source}: secret-like text detected")
    if LOCAL_PATH_RE.search(serialized):
        errors.append(f"{source}: local filesystem path detected")

    tasks = review.get("tasks")
    if not isinstance(tasks, list) or not tasks:
        errors.append(f"{source}: tasks must be a non-empty list")
        return [], errors

    normalized_tasks: list[dict[str, Any]] = []
    for index, task in enumerate(tasks, start=1):
        prefix = f"{source}: tasks[{index}]"
        if not isinstance(task, dict):
            errors.append(f"{prefix} must be an object")
            continue
        normalized_tasks.append(normalize_task(review, task, prefix, errors))
    return normalized_tasks, errors


def normalize_task(
    review: dict[str, Any],
    task: dict[str, Any],
    prefix: str,
    errors: list[str],
) -> dict[str, Any]:
    for field in ["id", "evidence_summary"]:
        if not isinstance(task.get(field), str) or not task[field].strip():
            errors.append(f"{prefix}.{field} is required")
    for field in ["condition_labels_blinded", "memory_helped", "memory_harmed", "public_safe"]:
        if not isinstance(task.get(field), bool):
            errors.append(f"{prefix}.{field} must be boolean")
    for field in ["score_without_memory", "score_with_memory", "citation_fidelity"]:
        value = task.get(field)
        if not isinstance(value, int) or value < 0 or value > 3:
            errors.append(f"{prefix}.{field} must be integer 0..3")
    for field in [
        "correction_count_without_memory",
        "correction_count_with_memory",
        "rework_count_without_memory",
        "rework_count_with_memory",
    ]:
        value = task.get(field)
        if not isinstance(value, int) or value < 0:
            errors.append(f"{prefix}.{field} must be a non-negative integer")
    if task.get("memory_harmed") and task.get("memory_helped"):
        errors.append(f"{prefix} cannot mark memory_helped and memory_harmed together")
    if task.get("public_safe") is not True:
        errors.append(f"{prefix}.public_safe must be true")

    without_score = int(task.get("score_without_memory", 0))
    with_score = int(task.get("score_with_memory", 0))
    without_corrections = int(task.get("correction_count_without_memory", 0))
    with_corrections = int(task.get("correction_count_with_memory", 0))
    without_rework = int(task.get("rework_count_without_memory", 0))
    with_rework = int(task.get("rework_count_with_memory", 0))
    return {
        "reviewer_id": review.get("reviewer_id"),
        "third_party_claim": bool(review.get("third_party_claim")),
        "reviewer_is_project_author": bool(review.get("reviewer_is_project_author")),
        "task_id": task.get("id"),
        "condition_labels_blinded": bool(task.get("condition_labels_blinded")),
        "score_without_memory": without_score,
        "score_with_memory": with_score,
        "score_delta": with_score - without_score,
        "memory_won": with_score > without_score,
        "memory_helped": bool(task.get("memory_helped")),
        "memory_harmed": bool(task.get("memory_harmed")),
        "citation_fidelity": int(task.get("citation_fidelity", 0)),
        "correction_delta": without_corrections - with_corrections,
        "rework_delta": without_rework - with_rework,
    }


def summarize_reviews(paths: list[Path], min_reviews: int) -> dict[str, Any]:
    all_tasks: list[dict[str, Any]] = []
    errors: list[str] = []
    reviewer_ids: set[str] = set()
    third_party_reviewers: set[str] = set()
    author_reviewers: set[str] = set()
    for path in paths:
        review = json.loads(path.read_text(encoding="utf-8"))
        tasks, review_errors = validate_review(review, str(path))
        errors.extend(review_errors)
        if isinstance(review.get("reviewer_id"), str):
            reviewer_ids.add(review["reviewer_id"])
            if review.get("third_party_claim") and not review.get("reviewer_is_project_author"):
                third_party_reviewers.add(review["reviewer_id"])
            if review.get("reviewer_is_project_author"):
                author_reviewers.add(review["reviewer_id"])
        all_tasks.extend(tasks)

    task_count = len(all_tasks)
    valid = not errors and task_count > 0
    blind_task_count = sum(1 for task in all_tasks if task["condition_labels_blinded"])
    helped_count = sum(1 for task in all_tasks if task["memory_helped"])
    harmed_count = sum(1 for task in all_tasks if task["memory_harmed"])
    won_count = sum(1 for task in all_tasks if task["memory_won"])
    third_party_task_count = sum(1 for task in all_tasks if task["third_party_claim"])
    value_claim_allowed = (
        valid
        and len(reviewer_ids) >= min_reviews
        and len(third_party_reviewers) >= min_reviews
        and not author_reviewers
        and blind_task_count == task_count
        and third_party_task_count == task_count
    )
    summary = {
        "schema_version": SCHEMA_VERSION,
        "command": "product-review-summary",
        "ok": valid,
        "review_file_count": len(paths),
        "reviewer_count": len(reviewer_ids),
        "third_party_reviewer_count": len(third_party_reviewers),
        "author_reviewer_count": len(author_reviewers),
        "task_review_count": task_count,
        "blind_task_rate": ratio(blind_task_count, task_count),
        "win_rate": ratio(won_count, task_count),
        "mean_score_delta": mean([task["score_delta"] for task in all_tasks]),
        "memory_helped_rate": ratio(helped_count, task_count),
        "memory_harmed_rate": ratio(harmed_count, task_count),
        "mean_citation_fidelity": mean([task["citation_fidelity"] for task in all_tasks]),
        "mean_correction_delta": mean([task["correction_delta"] for task in all_tasks]),
        "mean_rework_delta": mean([task["rework_delta"] for task in all_tasks]),
        "external_value_claim_allowed": value_claim_allowed,
        "value_claim_policy": contract()["value_claim_policy"],
        "errors": errors,
    }
    return summary


def ratio(numerator: int, denominator: int) -> float:
    if denominator == 0:
        return 0.0
    return numerator / denominator


def mean(values: list[int]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true")
    parser.add_argument("--review", action="append", default=[], help="Review JSON file. Repeat for multiple reviewers.")
    parser.add_argument("--min-reviews", type=int, default=1)
    parser.add_argument("--report", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.check_contract:
        print(json.dumps(contract(), indent=2, sort_keys=True))
        return 0
    if not args.review:
        raise SystemExit("product-review-summary: provide at least one --review file")
    try:
        summary = summarize_reviews([Path(path) for path in args.review], args.min_reviews)
    except (OSError, json.JSONDecodeError, ReviewSummaryFailure) as error:
        print(f"product-review-summary: {error}", file=sys.stderr)
        return 1
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 0 if summary["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
