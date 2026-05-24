# Provider Extraction Quality MVP

## Scope

Phase 26 raises the quality bar for command-backed and provider-backed
extraction without changing the stable `mneme.extractor.command.v1` protocol.
The work focuses on a stronger public model suite and wrapper-side guardrails.

## Requirements

- [REQ-PROVIDER-QUAL-001][Testability] The model suite shall include durable
  communication preference extraction.
- [REQ-PROVIDER-QUAL-002][Testability] The model suite shall include negative
  format preference extraction where the accepted alternative is persisted and
  the rejected option is not.
- [REQ-PROVIDER-QUAL-003][Testability] The model suite shall include
  project-scoped preference extraction.
- [REQ-PROVIDER-QUAL-004][Safety] The model suite shall reject quoted sample or
  test fixture text as durable memory.
- [REQ-PROVIDER-QUAL-005][Safety] The model suite shall reject answer-local or
  task-local instructions as durable memory.
- [REQ-PROVIDER-QUAL-006][Testability] The deterministic command fixture shall
  pass the expanded model suite without provider credentials.
- [REQ-PROVIDER-QUAL-007][Quality] The OpenAI wrapper dry-run shall pass the
  expanded model suite without provider credentials.
- [REQ-PROVIDER-QUAL-008][Quality] The OpenAI wrapper shall apply conservative
  post-processing guardrails for transient instructions, sample/test text, and
  rejected alternatives.
- [REQ-PROVIDER-QUAL-009][Release] The release quality gate shall require the
  expanded model baseline and category coverage.
- [REQ-PROVIDER-QUAL-010][Docs] Public docs shall describe the broader model
  suite and wrapper guardrails.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-PROVIDER-QUAL-001 | `model-implicit-communication-style` | verified |
| REQ-PROVIDER-QUAL-002 | `model-implicit-negative-format-preference` | verified |
| REQ-PROVIDER-QUAL-003 | `model-implicit-project-format-preference` | verified |
| REQ-PROVIDER-QUAL-004 | `model-no-claim-quoted-sample` | verified |
| REQ-PROVIDER-QUAL-005 | `model-no-claim-answer-local-instruction` | verified |
| REQ-PROVIDER-QUAL-006 | `evals/fixtures/command-extractor.sh` model suite run | verified |
| REQ-PROVIDER-QUAL-007 | `MNEME_OPENAI_DRY_RUN=1` model suite run | verified |
| REQ-PROVIDER-QUAL-008 | `should_suppress_model_claim` in `wrappers/openai_extractor.py` | verified |
| REQ-PROVIDER-QUAL-009 | `scripts/quality-gate.sh` baseline checks | verified |
| REQ-PROVIDER-QUAL-010 | README, model adapter, OpenAI wrapper, and baseline docs | verified |
