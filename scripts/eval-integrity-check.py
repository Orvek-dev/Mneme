#!/usr/bin/env python3
"""Check that public eval evidence is not coupled to answer-key strings.

This is intentionally conservative. It does not prove broad model quality; it
only prevents the most damaging failure mode for a public benchmark: copying
golden input text into product code or making every retrieval query an exact
substring of its expected answer.
"""

from __future__ import annotations

import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
NGRAM_SIZE = 6
SOURCE_GLOBS = ("crates/mneme-core/src/*.rs", "crates/mneme-mcp/src/*.rs")
PROHIBITED_README_PATTERNS = [
    re.compile(r"V1 ontology readiness\s*\|[^\n]*`1\.00`", re.IGNORECASE),
    re.compile(r"Entity F1\s*\|\s*`\[##########\]\s*1\.00`", re.IGNORECASE),
    re.compile(r"Relation F1\s*\|\s*`\[##########\]\s*1\.00`", re.IGNORECASE),
    re.compile(r"Attribute F1\s*\|\s*`\[##########\]\s*1\.00`", re.IGNORECASE),
]


class IntegrityFailure(RuntimeError):
    """Raised when a benchmark integrity check fails."""


def repo_root() -> Path:
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        text=True,
        capture_output=True,
        check=True,
    )
    return Path(result.stdout.strip())


ROOT = repo_root()


def normalize(text: str) -> list[str]:
    return re.findall(r"[a-z0-9][a-z0-9_-]*", text.lower())


def normalized_text(text: str) -> str:
    return " ".join(normalize(text))


def ngrams(tokens: list[str], size: int) -> set[str]:
    if len(tokens) < size:
        return set()
    return {" ".join(tokens[index : index + size]) for index in range(len(tokens) - size + 1)}


def source_text() -> str:
    parts: list[str] = []
    for pattern in SOURCE_GLOBS:
        for path in sorted(ROOT.glob(pattern)):
            parts.append(normalized_text(strip_rust_test_modules(path.read_text())))
    return "\n".join(parts)


def strip_rust_test_modules(text: str) -> str:
    marker = "#[cfg(test)]"
    index = text.find(marker)
    return text if index < 0 else text[:index]


def check_ontology_input_contamination() -> dict[str, Any]:
    fixture_path = ROOT / "evals" / "ontology" / "v1-natural-language-ontology-v0.json"
    fixture = json.loads(fixture_path.read_text())
    source = source_text()
    matches: list[dict[str, str]] = []
    for case in fixture.get("cases", []):
        for event in case.get("events", []):
            for phrase in sorted(ngrams(normalize(event.get("text", "")), NGRAM_SIZE)):
                if phrase in source:
                    matches.append({"case_id": case.get("id", ""), "ngram": phrase})
    return {
        "name": "ontology_input_contamination",
        "ok": not matches,
        "ngram_size": NGRAM_SIZE,
        "match_count": len(matches),
        "matches": matches[:20],
    }


def load_module(name: str, path: Path) -> Any:
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise IntegrityFailure(f"cannot load module: {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)  # type: ignore[union-attr]
    return module


def check_hard_dogfood_query_coupling() -> dict[str, Any]:
    hard = load_module("mneme_v1_hard_dogfood", ROOT / "scripts" / "v1-hard-dogfood.py")
    coupled: list[dict[str, str]] = []
    for workflow in hard.build_agent_workflows():
        query = normalized_text(workflow.get("query", ""))
        if not query:
            continue
        for expected in workflow.get("must_include", []):
            if query in normalized_text(expected):
                coupled.append({"workflow_id": workflow["id"], "query": workflow["query"]})
    return {
        "name": "hard_dogfood_query_coupling",
        "ok": not coupled,
        "coupled_workflow_count": len(coupled),
        "coupled_workflows": coupled[:20],
    }


def check_public_evidence_language() -> dict[str, Any]:
    checked = [ROOT / "README.md", ROOT / "docs" / "v1" / "evidence-scorecard.md"]
    findings: list[dict[str, str]] = []
    for path in checked:
        if not path.exists():
            continue
        text = path.read_text()
        for pattern in PROHIBITED_README_PATTERNS:
            if pattern.search(text):
                findings.append({"path": str(path.relative_to(ROOT)), "pattern": pattern.pattern})
    return {
        "name": "public_evidence_language",
        "ok": not findings,
        "finding_count": len(findings),
        "findings": findings,
    }


def main() -> int:
    checks = [
        check_ontology_input_contamination(),
        check_hard_dogfood_query_coupling(),
        check_public_evidence_language(),
    ]
    report = {
        "schema_version": SCHEMA_VERSION,
        "command": "eval-integrity-check",
        "ok": all(check["ok"] for check in checks),
        "checks": checks,
    }
    print(json.dumps(report, indent=2, sort_keys=True))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
