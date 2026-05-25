# V1 Completion Gap Analysis MVP

## Goal

Turn the ontology benchmark from a raw score report into a public v1 completion
decision tool that identifies the next implementation phase.

## Requirements

| ID | Requirement | Verification |
| --- | --- | --- |
| REQ-COMPLETE-001 | Document public v1 completion criteria without relying on private project context. | `docs/v1/v1-completion-criteria.md` |
| REQ-COMPLETE-002 | Emit ontology gap analysis grouped by product capability, not only by metric. | full local `scripts/v1-ontology-benchmark.py` run |
| REQ-COMPLETE-003 | Include a lightweight contract check for the gap-analysis scorer. | `scripts/v1-ontology-benchmark.py --check-gap-analysis` |
| REQ-COMPLETE-004 | Add the gap-analysis contract check to the quality gate without running the full local benchmark in CI. | `scripts/quality-gate.sh` |
| REQ-COMPLETE-005 | Keep all full benchmark outputs local-only and public-safe. | `scripts/public-safety-check.sh` |

## Non-Goals

- Implementing the richer ontology extraction layer.
- Declaring v1 complete before the ontology readiness gate passes.
- Uploading local benchmark run bundles to git.

## Verification

```sh
python3 -m py_compile scripts/v1-ontology-benchmark.py
scripts/v1-ontology-benchmark.py --check-gap-analysis
scripts/v1-ontology-benchmark.py --run-label local-phase41 --out-dir /tmp/mneme-phase41-ontology --force
scripts/quality-gate.sh full
```
