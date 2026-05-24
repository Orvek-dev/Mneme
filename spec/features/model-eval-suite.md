# Model Eval Suite Spec

## Scope

The model eval suite gives Mneme a repeatable way to evaluate command-backed or
model-backed extraction without making the default core suite depend on provider
credentials.

## Requirements

- [REQ-MODEL-EVAL-001][Testability] The eval harness shall expose an opt-in
  `mneme-v1-command` target.
- [REQ-MODEL-EVAL-002][Testability] The command target shall require an explicit
  extractor command.
- [REQ-MODEL-EVAL-003][Ubiquitous] Eval reports shall identify the extractor and
  protocol in `target_metadata`.
- [REQ-MODEL-EVAL-004][Testability] A tracked deterministic command fixture
  shall cover the command protocol without provider credentials.
- [REQ-MODEL-EVAL-005][Testability] The `model` suite shall cover implicit
  preference extraction, communication preferences, negative format
  preferences, project-scoped preferences, agent session-end extraction,
  no-claim events, quoted sample data, third-party attribution,
  over-extraction avoidance, secret blocking, and lifecycle correction.
- [REQ-MODEL-EVAL-006][Release] CI and release verification shall run the model
  suite through the deterministic command fixture.
- [REQ-MODEL-EVAL-007][Observability] Model scenarios shall include
  `category-*` tags that baseline reports can aggregate.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-MODEL-EVAL-001 | `TargetKind::MnemeV1Command` | verified |
| REQ-MODEL-EVAL-002 | missing command error path | verified |
| REQ-MODEL-EVAL-003 | eval report JSON metadata test and model suite report | verified |
| REQ-MODEL-EVAL-004 | `evals/fixtures/command-extractor.sh` | verified |
| REQ-MODEL-EVAL-005 | `evals/scenarios/model/*.yaml` | verified |
| REQ-MODEL-EVAL-006 | CI and release workflow model-suite steps | verified |
| REQ-MODEL-EVAL-007 | model scenario `category-*` tags and baseline category summaries | verified |
