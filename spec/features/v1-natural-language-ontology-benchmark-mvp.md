# V1 Natural-Language Ontology Benchmark MVP

## Goal

Measure current Mneme v1 against realistic natural-language and
complex-ontology memory cases before implementing ontology changes.

## Requirements

| ID | Requirement | Verification |
| --- | --- | --- |
| REQ-ONTO-001 | Provide a public-safe golden ontology fixture with natural-language cases and explicit v1 anchor cases. | `scripts/v1-ontology-benchmark.py --check-fixture` |
| REQ-ONTO-002 | Validate entities, relations, attributes, source event references, context checks, and temporal checks in the fixture. | `scripts/v1-ontology-benchmark.py --check-fixture` |
| REQ-ONTO-003 | Score current v1 without requiring ontology scores to pass release thresholds. | full local `scripts/v1-ontology-benchmark.py` run |
| REQ-ONTO-004 | Report entity, relation, attribute, scope, temporal, provenance, context, and safety metrics. | `scorecard.json` |
| REQ-ONTO-005 | Include scorer fault detection for dropped relations, context recall misses, secret leaks, and missing provenance. | `scripts/v1-ontology-benchmark.py --check-scorer` |
| REQ-ONTO-006 | Emit capability-level gap analysis that maps score misses to the next implementation phase. | `gap-analysis.json` |
| REQ-ONTO-007 | Keep full benchmark evidence local-only while CI checks the contract, fixture, scorer, and gap-analysis contract. | `scripts/quality-gate.sh` |

## Non-Goals

- Implementing richer v1 ontology extraction.
- Changing current v1 memory storage semantics.
- Requiring natural-language ontology scores to pass before the baseline is
  measured.

## Verification

```sh
python3 -m py_compile scripts/v1-ontology-benchmark.py
scripts/v1-ontology-benchmark.py --check-contract
scripts/v1-ontology-benchmark.py --check-fixture
scripts/v1-ontology-benchmark.py --check-scorer
scripts/v1-ontology-benchmark.py --check-gap-analysis
scripts/v1-ontology-benchmark.py --run-label local-phase41 --out-dir /tmp/mneme-phase41-ontology --force
```
