# V1 Hard Candidate Trend MVP

## Goal

Connect hard-mode dogfood findings to the existing candidate workflow and keep
public-safe trend history so repeated hard dogfood runs can be compared before
v1 changes are promoted.

## Requirements

| ID | Requirement | Verification |
|---|---|---|
| REQ-HARD-TREND-001 | Mirror hard-mode findings into official `mneme.eval_candidate.v1` YAML artifacts. | `scripts/v1-hard-dogfood.py --check-official-candidate` |
| REQ-HARD-TREND-002 | Validate generated official candidates with `mneme-eval candidate-check`. | `candidates/official-candidate-check.json` |
| REQ-HARD-TREND-003 | Write public-safe hard dogfood history entries. | `history/*.json` |
| REQ-HARD-TREND-004 | Compare the current hard scorecard with the latest passing history entry. | `trend.json`, `trend.md` |
| REQ-HARD-TREND-005 | Keep full hard-mode runs local-only while CI checks bridge and trend contracts. | `scripts/quality-gate.sh` |

## Non-Goals

- Automatically promoting hard candidates into public scenarios.
- Storing private real-use evidence in the repository.
- Running full hard dogfood in GitHub Actions.

## Validation

```sh
python3 -m py_compile scripts/v1-hard-dogfood.py
scripts/v1-hard-dogfood.py --check-official-candidate
scripts/v1-hard-dogfood.py --check-trend
scripts/v1-hard-dogfood.py --run-label local-phase39 --out-dir /tmp/mneme-phase39-hard-full --history-dir /tmp/mneme-phase39-hard-history --force
```
