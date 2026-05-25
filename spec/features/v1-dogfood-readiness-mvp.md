# V1 Dogfood Readiness MVP

## Summary

Phase 33 creates a deterministic product-readiness gate for the current v1
runtime. The goal is to know whether `mneme-v1` is ready for structured
dogfood testing without running live providers or doing manual spot checks.

## Requirements

- [REQ-READY-001][Dogfood Suite] The repository shall include a public
  `evals/scenarios/dogfood/` suite covering realistic v1 user workflows.
- [REQ-READY-002][Product Gate] The eval harness shall expose
  `mneme-eval v1-readiness`.
- [REQ-READY-003][Deterministic Target] The readiness gate shall use the
  deterministic `mneme-v1` target and shall not require provider credentials.
- [REQ-READY-004][Coverage Criteria] The readiness gate shall validate and
  replay `core`, `runtime`, `agent`, and `dogfood` suites.
- [REQ-READY-005][Machine Report] The readiness gate shall emit JSON with
  suite-level status, failed scenario IDs, failed check names, criteria, and a
  `readiness_status`.
- [REQ-READY-006][Release Gate] The release quality gate shall validate and
  run the dogfood suite and shall require `ready_for_v1_dogfood`.
- [REQ-READY-007][Documentation] Public docs shall explain when to use
  dogfood readiness versus provider/model baselines.

## Verification Map

| Requirement | Evidence | Status |
|---|---|---|
| REQ-READY-001 | `evals/scenarios/dogfood/*.yaml` | verified |
| REQ-READY-002 | `mneme-eval v1-readiness` CLI and help text | verified |
| REQ-READY-003 | readiness implementation uses `TargetKind::MnemeV1` | verified |
| REQ-READY-004 | readiness suite list covers `core`, `runtime`, `agent`, `dogfood` | verified |
| REQ-READY-005 | `V1ReadinessReport` JSON contract | verified |
| REQ-READY-006 | `scripts/quality-gate.sh` readiness checks | verified |
| REQ-READY-007 | README, v1, and eval-harness docs | verified |
