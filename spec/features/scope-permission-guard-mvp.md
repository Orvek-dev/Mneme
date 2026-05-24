# Scope Permission Guard MVP Spec

## Scope

This phase enforces memory scope authorization before context retrieval for the
local v1 runtime, CLI, and eval harness.

## Authority

- Raw events and claims may carry different memory scopes.
- Context retrieval must not return claims outside the caller's allowed scopes.
- The default local retrieval scope is `private`.
- Agent session begin uses the same retrieval guard as direct context queries.
- Omitted context may report stable omission reasons without leaking claim text.

## Requirements

- [REQ-SCOPE-001][Permission] `mneme-core` shall provide a scoped context query
  API for explicit allowed retrieval scopes.
- [REQ-SCOPE-002][Permission] Context retrieval shall omit active claims whose
  scope is not allowed before relevance matching.
- [REQ-SCOPE-003][Permission] Scope-denied omissions shall use a stable
  `scope_denied:<scope>` reason.
- [REQ-SCOPE-004][Permission] `MnemeEngine::build_context_pack` shall default
  to the `private` scope for compatibility with local personal use.
- [REQ-SCOPE-005][Permission] Agent session begin shall apply the same allowed
  scope guard as direct context retrieval.
- [REQ-SCOPE-006][Ubiquitous] `mneme context` and `mneme begin` shall accept
  repeated `--scope <scope>` values for allowed retrieval scopes.
- [REQ-SCOPE-007][Release] The eval harness shall validate allowed-scope
  context scenarios and omitted scope-denial reasons.
- [REQ-SCOPE-008][Release] The local quality gate shall smoke-test denied and
  allowed scoped retrieval paths.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-SCOPE-001 | `ContextQuery` in `mneme-core` | verified |
| REQ-SCOPE-002 | `MnemeEngine::build_context_pack_with` tests | verified |
| REQ-SCOPE-003 | scope-denied unit and eval checks | verified |
| REQ-SCOPE-004 | `MnemeEngine::build_context_pack` implementation | verified |
| REQ-SCOPE-005 | agent session unit and eval checks | verified |
| REQ-SCOPE-006 | `mneme-cli` parser and CLI tests | verified |
| REQ-SCOPE-007 | `evals/scenarios/core/scope-permission-guard.yaml` | verified |
| REQ-SCOPE-008 | `scripts/quality-gate.sh` | verified |
