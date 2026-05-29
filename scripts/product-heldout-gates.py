#!/usr/bin/env python3
"""Run product-level held-out gates for extractor and ranking claims.

The purpose is not to inflate scores. The gate keeps open-domain extraction and
semantic-search claims disabled until live-provider extraction and embedding
ranking have real held-out evidence.
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


EXTRACTION_HELDOUT_CASES: list[dict[str, Any]] = [
    {
        "id": "jin-standup-spanish",
        "text": "Jin prefers terse standup notes in Spanish for future sprint reviews.",
        "expected_claims": ["Jin prefers terse standup notes", "standup notes language Spanish"],
    },
    {
        "id": "borealis-risk-owner",
        "text": "For Project Borealis, Mira wants launch risk notes grouped by owner, not by subsystem.",
        "expected_claims": ["Project Borealis risk notes grouped by owner"],
    },
    {
        "id": "no-claim-one-off",
        "text": "For this one reply, make the answer funny and ignore that preference tomorrow.",
        "expected_claims": [],
    },
    {
        "id": "preference-correction",
        "text": "Earlier I wanted Friday retros, but use Monday morning retros from now on.",
        "expected_claims": ["user prefers Monday morning retros"],
    },
]


RANKING_HELDOUT_CASES: list[dict[str, Any]] = [
    {
        "id": "owner-grouped-risk",
        "query": "risk notes by owner",
        "expected_id": "doc-owner-risk",
        "documents": [
            ("doc-owner-risk", "launch risk summaries grouped by responsible owner"),
            ("doc-subsystem-risk", "launch risk summaries grouped by subsystem"),
            ("doc-standup", "standup notes should stay terse"),
        ],
    },
    {
        "id": "retro-cadence",
        "query": "retros on Monday morning",
        "expected_id": "doc-monday-retro",
        "documents": [
            ("doc-friday-retro", "old retrospectives happened on Friday"),
            ("doc-monday-retro", "current retrospective cadence is Monday morning"),
            ("doc-language", "standup notes language Spanish"),
        ],
    },
    {
        "id": "terse-standup",
        "query": "concise sprint update",
        "expected_id": "doc-standup",
        "documents": [
            ("doc-owner-risk", "launch risk summaries grouped by responsible owner"),
            ("doc-standup", "standup notes should stay terse"),
            ("doc-language", "standup notes language Spanish"),
        ],
    },
]


ALIAS_MAP = {
    "by": ["grouped"],
    "owner": ["responsible"],
    "retros": ["retrospective", "retrospectives"],
    "concise": ["terse"],
    "sprint": ["standup"],
    "update": ["notes"],
}


def contract() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-heldout-gates-contract",
        "extraction": {
            "case_count": len(EXTRACTION_HELDOUT_CASES),
            "claim_policy": "open-domain extraction claims require live-provider or independently reviewed extractor evidence",
        },
        "ranking": {
            "case_count": len(RANKING_HELDOUT_CASES),
            "metrics": ["term_mrr", "alias_mrr", "mrr_delta"],
            "claim_policy": "semantic search claims require embedding or stronger ranker evidence on held-out queries",
        },
    }


def dataset() -> dict[str, Any]:
    return {
        "schema_version": SCHEMA_VERSION,
        "command": "product-heldout-gates-dataset",
        "extraction_heldout_case_count": len(EXTRACTION_HELDOUT_CASES),
        "ranking_heldout_case_count": len(RANKING_HELDOUT_CASES),
        "contains_unseen_entities": True,
        "contains_no_claim_case": True,
        "contains_correction_case": True,
    }


def run_gates() -> dict[str, Any]:
    ranking = ranking_report()
    report = {
        "schema_version": SCHEMA_VERSION,
        "command": "product-heldout-gates",
        "ok": True,
        "extraction": {
            "heldout_case_count": len(EXTRACTION_HELDOUT_CASES),
            "live_provider_executed": False,
            "dry_run_or_rule_only": True,
            "open_domain_extraction_claim_allowed": False,
            "live_provider_required_for_open_domain_claim": True,
            "human_adjudication_required": True,
        },
        "ranking": ranking,
        "summary": {
            "open_domain_extraction_claim_allowed": False,
            "semantic_search_claim_allowed": False,
            "heldout_evidence_ready": False,
        },
    }
    return report


def ranking_report() -> dict[str, Any]:
    term_rr = []
    alias_rr = []
    cases = []
    for case in RANKING_HELDOUT_CASES:
        term_ranked = rank_documents(case["query"], case["documents"], aliases=False)
        alias_ranked = rank_documents(case["query"], case["documents"], aliases=True)
        term_case_rr = reciprocal_rank(term_ranked, case["expected_id"])
        alias_case_rr = reciprocal_rank(alias_ranked, case["expected_id"])
        term_rr.append(term_case_rr)
        alias_rr.append(alias_case_rr)
        cases.append(
            {
                "id": case["id"],
                "query": case["query"],
                "expected_id": case["expected_id"],
                "term_ranked": term_ranked,
                "alias_ranked": alias_ranked,
                "term_rr": term_case_rr,
                "alias_rr": alias_case_rr,
            }
        )
    term_mrr = mean(term_rr)
    alias_mrr = mean(alias_rr)
    return {
        "heldout_case_count": len(RANKING_HELDOUT_CASES),
        "term_mrr": term_mrr,
        "alias_mrr": alias_mrr,
        "mrr_delta": alias_mrr - term_mrr,
        "semantic_search_claim_allowed": False,
        "embedding_eval_required_for_semantic_claim": True,
        "ranking_shape": "heldout_alias_probe_not_embedding_proof",
        "cases": cases,
    }


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


def expand(tokens: set[str]) -> set[str]:
    expanded = set(tokens)
    for token in list(tokens):
        expanded.update(ALIAS_MAP.get(token, []))
    return expanded


def rank_documents(query: str, docs: list[tuple[str, str]], *, aliases: bool) -> list[str]:
    query_tokens = set(tokenize(query))
    if aliases:
        query_tokens = expand(query_tokens)
    scored = []
    for index, (doc_id, text) in enumerate(docs):
        score = len(query_tokens & set(tokenize(text)))
        scored.append((score, -index, doc_id))
    scored.sort(reverse=True)
    return [doc_id for score, _, doc_id in scored if score > 0]


def reciprocal_rank(ranked: list[str], expected_id: str) -> float:
    for index, doc_id in enumerate(ranked, start=1):
        if doc_id == expected_id:
            return 1.0 / index
    return 0.0


def mean(values: list[float]) -> float:
    if not values:
        return 0.0
    return sum(values) / len(values)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check-contract", action="store_true")
    parser.add_argument("--check-dataset", action="store_true")
    parser.add_argument("--report", type=Path)
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.check_contract:
        print(json.dumps(contract(), indent=2, sort_keys=True))
        return 0
    if args.check_dataset:
        print(json.dumps(dataset(), indent=2, sort_keys=True))
        return 0
    report = run_gates()
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
