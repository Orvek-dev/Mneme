# Live Provider Runbook MVP Spec

## Scope

This phase makes live provider baseline runs repeatable and public-safe without
adding live credentials to CI.

## Requirements

- [REQ-LIVE-RUNBOOK-001][Privacy] Live provider runs shall remain local and
  opt-in.
- [REQ-LIVE-RUNBOOK-002][Observability] Baseline reports shall identify safe
  provider/model/run labels when supplied.
- [REQ-LIVE-RUNBOOK-003][Privacy] Public docs shall include a redaction
  checklist before any live report is shared.
- [REQ-LIVE-RUNBOOK-004][Release] CI and release verification shall cover
  metadata labels in dry-run mode only.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-LIVE-RUNBOOK-001 | `docs/eval-harness/live-provider-baseline-runbook.md` | verified |
| REQ-LIVE-RUNBOOK-002 | `baseline_metadata` report fields | verified |
| REQ-LIVE-RUNBOOK-003 | runbook redaction checklist | verified |
| REQ-LIVE-RUNBOOK-004 | CI and release dry-run metadata checks | verified |
