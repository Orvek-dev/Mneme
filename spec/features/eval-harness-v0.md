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

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-EVAL-001 | scenario loader tests | mapped |
| REQ-EVAL-002 | isolated temp database tests | mapped |
| REQ-EVAL-003 | fake component determinism tests | mapped |
| REQ-EVAL-004 | failure report snapshot tests | mapped |
| REQ-EVAL-005 | gitignore and generated-output tests | mapped |
| REQ-EVAL-006 | seeded-fault tests | mapped |
