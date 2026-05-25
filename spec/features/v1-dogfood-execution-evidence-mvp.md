# V1 Dogfood Execution Evidence MVP

## Summary

Phase 34 turns v1 dogfood readiness into a repeatable execution artifact. A
single local command should run deterministic dogfood checks, smoke the local
CLI flow, and write ignored evidence for later triage.

## Requirements

- [REQ-DOGFOOD-EXEC-001][One Command] The repository shall provide
  `scripts/v1-dogfood.sh` as the local v1 dogfood execution entry point.
- [REQ-DOGFOOD-EXEC-002][Ignored Evidence] Dogfood evidence shall be written
  under ignored output by default.
- [REQ-DOGFOOD-EXEC-003][Eval Evidence] The evidence bundle shall include
  dogfood validation, fake run, `mneme-v1` run, dogfood acceptance, and
  `v1-readiness` reports.
- [REQ-DOGFOOD-EXEC-004][CLI Evidence] The evidence bundle shall include a
  local CLI smoke path over an isolated store.
- [REQ-DOGFOOD-EXEC-005][Automation] The release quality gate shall execute
  the dogfood script and verify the summary and readiness outputs.
- [REQ-DOGFOOD-EXEC-006][Documentation] Public docs shall explain how to run
  the script and where evidence is written.

## Verification Map

| Requirement | Evidence | Status |
|---|---|---|
| REQ-DOGFOOD-EXEC-001 | `scripts/v1-dogfood.sh` | verified |
| REQ-DOGFOOD-EXEC-002 | default output under `evals/runs/` | verified |
| REQ-DOGFOOD-EXEC-003 | script report list and quality-gate checks | verified |
| REQ-DOGFOOD-EXEC-004 | CLI doctor/init/session/context/quality/validate smoke | verified |
| REQ-DOGFOOD-EXEC-005 | `scripts/quality-gate.sh` script execution | verified |
| REQ-DOGFOOD-EXEC-006 | README and eval-harness docs | verified |
