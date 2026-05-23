# Eval Harness v0 Spec

## Scope

Eval Harness v0 makes Mneme behavior testable before the Mneme runtime grows.
It covers deterministic scenario replay, expected outcome checks, and actionable
reports.

## Authority

- Product invariant: raw events, claims, context packs, budget, and audit state
  must be verifiable without relying on manual inspection.
- Test invariant: fake extractors and fake token estimators must be
  deterministic.

## Domain Truth

| Term | Meaning | Source of truth |
| --- | --- | --- |
| scenario | A replayable fixture describing inputs and expected outcomes | `evals/scenarios/` |
| suite | A named group of scenarios | eval harness config or directory |
| replay | Execution of a scenario against an isolated implementation | `mneme-eval` |
| report | Machine-readable and human-readable eval result | `evals/reports/` |
| seeded fault | Intentional broken adapter or behavior used to test harness detection | `evals/seeded-faults/` |

## Requirements

- [REQ-EVAL-001][Ubiquitous] The eval harness shall load scenario fixtures from
  the repository eval directory.
- [REQ-EVAL-002][Event-driven] When a scenario runs, the harness shall isolate
  runtime state from other scenarios.
- [REQ-EVAL-003][Ubiquitous] The fake extractor and fake token estimator shall
  produce deterministic output for the same input.
- [REQ-EVAL-004][Event-driven] When actual output differs from expected output,
  the report shall include the scenario ID, failed check, expected value, actual
  value, and relevant artifact reference.
- [REQ-EVAL-005][Ubiquitous] Generated reports shall be ignored by default
  unless they are explicitly committed as fixtures.
- [REQ-EVAL-006][Event-driven] When a seeded critical fault is active, the
  harness shall fail the affected scenario instead of reporting success.
- [REQ-EVAL-007][Ubiquitous] The public scenario format shall be documented with
  required fields, optional fields, report conventions, and seeded fault modes.
- [REQ-EVAL-008][Ubiquitous] The eval harness shall validate scenario structure
  without replaying runtime behavior and shall fail malformed fixtures.
- [REQ-EVAL-009][Ports-and-adapters] The eval harness shall execute scenarios
  through a named target adapter boundary instead of coupling checks to one
  runtime implementation.
- [REQ-EVAL-010][Ubiquitous] Eval reports shall include the target name and a
  report schema version for future compatibility checks.
- [REQ-EVAL-011][Ubiquitous] The eval harness shall expose an acceptance gate
  that validates fixtures, runs the core suite, verifies report metadata, and
  proves critical seeded faults are detected.
- [REQ-EVAL-012][Ubiquitous] The repository shall document what the core suite
  does and does not cover before Mneme v1 implementation begins.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-EVAL-001 | scenario loader tests and `mneme-eval run --suite core` | verified |
| REQ-EVAL-002 | per-scenario fake runtime state in replay tests | verified |
| REQ-EVAL-003 | fake component determinism tests | verified |
| REQ-EVAL-004 | failure report fields in replay output | verified |
| REQ-EVAL-005 | `.gitignore` rules for `evals/reports/*` | verified |
| REQ-EVAL-006 | `--seeded-fault skip-claims` / `leak-secrets` tests | verified |
| REQ-EVAL-007 | `docs/eval-scenario-format.md` | verified |
| REQ-EVAL-008 | `mneme-eval validate --suite core` and invalid fixture CI check | verified |
| REQ-EVAL-009 | `--target fake`, `--target mneme-v1`, `EvalTarget`, and `docs/eval-target-adapter-contract.md` | verified |
| REQ-EVAL-010 | JSON report fields `target` and `report_schema_version` | verified |
| REQ-EVAL-011 | `mneme-eval acceptance --suite core --target fake` in CI | verified |
| REQ-EVAL-012 | `docs/eval-harness-acceptance.md` | verified |
