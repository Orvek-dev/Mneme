# Model Extraction Adapter MVP Spec

## Scope

This phase introduces a provider-neutral model extraction path without making
CI, eval acceptance, or the public core crate depend on a specific model API.

## Authority

- Mneme owns event append, budget gates, claim IDs, source citations, lifecycle
  state, audit records, and secret blocking.
- The external command proposes at most one claim candidate per event.
- Provider SDKs, model prompts, API keys, retries, and network timeouts belong
  in the wrapper command, not in `mneme-core`.

## Requirements

- [REQ-MODEL-001][Ports-and-adapters] The core shall expose a command-backed
  extractor that implements `MnemeExtractor`.
- [REQ-MODEL-002][Ubiquitous] The command protocol shall include a stable schema
  version.
- [REQ-MODEL-003][Ubiquitous] Command responses shall support both one claim and
  no claim.
- [REQ-MODEL-004][Security] Empty claim fields shall be rejected before
  persistence.
- [REQ-MODEL-005][Security] Command execution shall not invoke a shell
  implicitly.
- [REQ-MODEL-006][Testability] The local CLI shall expose an opt-in raw `ingest`
  command with command extractor support.
- [REQ-MODEL-007][Testability] Default CI and eval targets shall remain
  deterministic without model credentials.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-MODEL-001 | `CommandExtractor` export and unit test | verified |
| REQ-MODEL-002 | `EXTRACTOR_COMMAND_SCHEMA_VERSION` | verified |
| REQ-MODEL-003 | `ExtractorCommandResponse::{from_claim,no_claim}` | verified |
| REQ-MODEL-004 | empty claim field unit test | verified |
| REQ-MODEL-005 | `Command::new(program).args(args)` implementation | verified |
| REQ-MODEL-006 | `mneme ingest --extractor command` CLI test | verified |
| REQ-MODEL-007 | existing `fake` and `mneme-v1` eval acceptance gates | verified |
