# Mneme Constitution

This file is the public starting point for Mneme's product invariants. It is
not an implementation-readiness report.

## Product Integrity

1. Raw events are the source of truth.
2. Memory claims must preserve provenance.
3. Context packs must explain why each memory was included.
4. Budget checks happen before model calls.
5. Secrets must not become active memory or context.
6. Scope and permission boundaries must be enforced before retrieval.
7. Corrections and deletion are auditable state transitions, not silent rewrites.

## Verification

Every accepted behavior requirement should map to eval scenarios, tests,
guardrails, benchmarks, or named manual evidence before implementation is called
complete.
