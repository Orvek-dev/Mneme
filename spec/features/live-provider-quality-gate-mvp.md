# Live Provider Quality Gate MVP Spec

## Scope

Phase 5 turns provider-wrapper baseline reports into an explicit quality gate.
Live provider calls remain local and opt-in; public CI continues to use dry-run
provider checks only.

## Requirements

- [REQ-LIVE-GATE-001][Testability] The eval harness shall expose
  `mneme-eval baseline-gate <report.json>`.
- [REQ-LIVE-GATE-002][Quality] The gate shall enforce aggregate pass-rate,
  category pass-rate, failed-iteration, and failed-scenario-run thresholds.
- [REQ-LIVE-GATE-003][Observability] Baseline reports shall include failed
  category, failed scenario, and failed check-count summaries.
- [REQ-LIVE-GATE-004][Privacy] The gate shall scan reports for obvious secret
  and local-path patterns before reports are shared.
- [REQ-LIVE-GATE-005][Privacy] Live provider metadata shall remain opt-in, with
  an explicit `--require-live-provider` gate option for local live runs.
- [REQ-LIVE-GATE-006][Release] CI and release verification shall exercise the
  gate with dry-run provider output only.
- [REQ-LIVE-GATE-007][Documentation] Public docs shall explain how to gate dry
  and live baseline reports.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-LIVE-GATE-001 | `mneme-eval baseline-gate` command | verified |
| REQ-LIVE-GATE-002 | baseline gate thresholds and tests | verified |
| REQ-LIVE-GATE-003 | `failure_summary` baseline report field | verified |
| REQ-LIVE-GATE-004 | `redaction.scan` gate | verified |
| REQ-LIVE-GATE-005 | `--require-live-provider` gate option | verified |
| REQ-LIVE-GATE-006 | `scripts/quality-gate.sh` dry-run baseline gate | verified |
| REQ-LIVE-GATE-007 | live provider baseline docs and runbook | verified |
