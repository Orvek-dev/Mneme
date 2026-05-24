# Mneme v1 Personal Core Spec

## Scope

Mneme v1 personal core provides the first product runtime that can be exercised
by the eval harness. It is deterministic and exposes a small persistence
boundary for restart verification.

## Authority

- Raw events remain the source of truth.
- Budget checks happen before extraction.
- Extraction adapters propose claims; the engine still owns IDs, provenance,
  audit, lifecycle state, and safety checks.
- Secret-like data must not become active context.
- Every context item must preserve source event provenance.
- Persisted state must round-trip without changing claim, event, budget, or
  audit semantics.
- Corrections and forgets are new events; old claims remain auditable but stop
  contributing to context.

## Requirements

- [REQ-V1-001][Event-driven] The v1 core shall append raw events in input order.
- [REQ-V1-002][Ubiquitous] The v1 core shall enforce deterministic token budget
  hard caps before extracting claims.
- [REQ-V1-003][Ubiquitous] The v1 core shall extract explicit memory claims from
  supported remember markers.
- [REQ-V1-004][Ubiquitous] The v1 core shall mark secret-like claims as blocked
  instead of active.
- [REQ-V1-005][Event-driven] The v1 core shall build context packs from active
  claims only.
- [REQ-V1-006][Ubiquitous] Context items shall preserve source event citations.
- [REQ-V1-007][Event-driven] The v1 core shall emit audit records for append,
  claim write, context read, and budget block operations.
- [REQ-V1-008][Ports-and-adapters] The eval harness shall expose `mneme-v1` as a
  target adapter over `mneme-core`.
- [REQ-V1-009][Ports-and-adapters] The v1 core shall expose a storage port for
  loading and saving complete state snapshots.
- [REQ-V1-010][Event-driven] The `mneme-v1` eval target shall prove recall still
  works after file-backed persistence and reload.
- [REQ-V1-011][Event-driven] The v1 core shall support explicit correction
  events that supersede active claims and write replacement claims.
- [REQ-V1-012][Event-driven] The v1 core shall support explicit forget events
  that mark active claims as forgotten.
- [REQ-V1-013][Ubiquitous] Superseded and forgotten claims shall be omitted from
  context packs.
- [REQ-V1-014][Ports-and-adapters] The v1 core shall expose an extraction
  adapter boundary for claim extraction.
- [REQ-V1-015][Ubiquitous] Extractor output shall still pass through engine
  safety, provenance, and audit rules.
- [REQ-V1-016][Event-driven] Claim-ID lifecycle events shall target exactly one
  active claim.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-V1-001 | `mneme-core` unit tests and `mneme-eval run --suite core --target mneme-v1` | verified |
| REQ-V1-002 | budget hard-cap unit test and core scenario | verified |
| REQ-V1-003 | explicit memory unit test and core scenario | verified |
| REQ-V1-004 | blocked secret unit test and core scenario | verified |
| REQ-V1-005 | context-pack checks in core suite | verified |
| REQ-V1-006 | citation checks in core suite | verified |
| REQ-V1-007 | audit checks in core suite | verified |
| REQ-V1-008 | `mneme-eval acceptance --suite core --target mneme-v1` | verified |
| REQ-V1-009 | JSON file store round-trip unit test | verified |
| REQ-V1-010 | `restart-persistence` core scenario through `mneme-v1` target | verified |
| REQ-V1-011 | `correct-memory` core scenario | verified |
| REQ-V1-012 | `forget-persists` core scenario | verified |
| REQ-V1-013 | lifecycle context-pack checks in core suite | verified |
| REQ-V1-014 | custom extractor unit test and `RuleBasedExtractor` default path | verified |
| REQ-V1-015 | extractor secret-blocking unit test | verified |
| REQ-V1-016 | `id-lifecycle-targets-one-claim` core scenario | verified |
