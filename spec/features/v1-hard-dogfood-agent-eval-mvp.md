# V1 Hard Dogfood Agent Eval MVP

## Goal

Add a local hard-mode dogfood protocol that evaluates Mneme v1 under normal,
adversarial, and agent handoff memory workloads before treating v1 behavior as
high-confidence.

## Requirements

| ID | Requirement | Verification |
|---|---|---|
| REQ-HARD-001 | Provide a runner for 100 normal records, 150 adversarial records, and 30 agent handoff workflows. | `scripts/v1-hard-dogfood.py --check-dataset` |
| REQ-HARD-002 | Score recall, precision, scope leak, secret leak, citation coverage, handoff success, stale reuse, and agent attribution. | `summary.json`, `scorecard.json` |
| REQ-HARD-003 | Detect seeded faults for dropped citations, scope leaks, secret leaks, stale reuse, and handoff misses. | `scripts/v1-hard-dogfood.py --check-seeded-faults` |
| REQ-HARD-004 | Produce local candidate artifacts for seeded faults and failed hard workflows. | `candidates/candidate-index.json` |
| REQ-HARD-005 | Produce public-safe JSON, Markdown, and HTML reports. | `summary.json`, `report.md`, `report.html` |
| REQ-HARD-006 | Keep the full hard-mode run local-only and add only lightweight CI checks. | `scripts/quality-gate.sh` |

## Non-Goals

- Running the full hard-mode protocol in GitHub Actions.
- Promoting generated candidates automatically into public scenarios.
- Adding vector search or a new retrieval backend.

## Validation

```sh
python3 -m py_compile scripts/v1-hard-dogfood.py
scripts/v1-hard-dogfood.py --check-contract
scripts/v1-hard-dogfood.py --check-dataset
scripts/v1-hard-dogfood.py --check-seeded-faults
scripts/v1-hard-dogfood.py --run-label local-phase38 --out-dir /tmp/mneme-phase38-hard-full --force
```
