# Mneme v1 Personal Core Spec

## Scope

Mneme v1 personal core provides the first product runtime that can be exercised
by the eval harness. It is deterministic and in-memory.

## Authority

- Raw events remain the source of truth.
- Budget checks happen before extraction.
- Secret-like data must not become active context.
- Every context item must preserve source event provenance.

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
