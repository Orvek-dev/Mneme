# Guided Memory Curation MVP

## Scope

Phase 24 turns memory quality findings into an explicit cleanup plan while
keeping mutation opt-in. The MVP focuses on deterministic local-store cleanup
for personal use.

## Requirements

- [REQ-CURATE-001][Review] `mneme curate` shall produce a dry-run cleanup plan
  without mutating the store.
- [REQ-CURATE-002][Safety] Curation reports shall redact blocked-secret values
  by default.
- [REQ-CURATE-003][Lifecycle] `mneme curate --apply` shall forget redundant
  duplicate active claims by stable claim ID.
- [REQ-CURATE-004][Privacy] `mneme curate --apply --compact` shall remove
  non-active records, including blocked-secret, superseded, and forgotten
  claims, only after explicit `--compact`.
- [REQ-CURATE-005][Persistence] Applied curation shall persist through the
  normal JSON store path so an existing store backup is written before
  replacement.
- [REQ-CURATE-006][Eval] The eval harness shall support curation before/after
  quality checks.
- [REQ-CURATE-007][Release] The release quality gate shall smoke dry-run,
  apply, redaction, backup, and final quality health behavior.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-CURATE-001 | `mneme curate --json` | verified |
| REQ-CURATE-002 | CLI unit test and quality gate secret scan | verified |
| REQ-CURATE-003 | CLI unit test and curation eval scenario | verified |
| REQ-CURATE-004 | `mneme curate --apply --compact` checks | verified |
| REQ-CURATE-005 | CLI unit test and quality gate backup check | verified |
| REQ-CURATE-006 | `expected.curation` eval checks | verified |
| REQ-CURATE-007 | `scripts/quality-gate.sh` | verified |
