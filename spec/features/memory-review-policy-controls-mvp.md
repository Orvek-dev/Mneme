# Memory Review Policy Controls MVP

## Intent

Personal memory must remain inspectable and correctable as the local store grows.
This phase adds a review surface for stored claims and precise claim-ID
lifecycle controls so users and agents can avoid broad text-match updates.

## Requirements

- [REQ-REVIEW-001][Ubiquitous] `mneme claims` shall list stored memory claims
  with IDs, status, scope, and source event citations.
- [REQ-REVIEW-002][Permission] `mneme claims` shall support status and scope
  filters without mutating the store.
- [REQ-REVIEW-003][Event-driven] `mneme forget --claim-id <id>` shall mark
  exactly one active claim as forgotten.
- [REQ-REVIEW-004][Event-driven] `mneme correct --claim-id <id> <new-claim>`
  shall supersede exactly one active claim and write one replacement claim.
- [REQ-REVIEW-005][Safety] ID-based lifecycle commands shall reject unknown or
  inactive claim IDs before writing a lifecycle event.
- [REQ-REVIEW-006][Evaluation] The eval harness shall cover duplicate claim
  text where ID-based lifecycle updates only the selected claim.
- [REQ-REVIEW-007][Release] The quality gate shall smoke test claim review and
  ID-based forget/correct behavior.

## Verification Map

| Requirement | Verification | Status |
| --- | --- | --- |
| REQ-REVIEW-001 | CLI unit test and quality-gate `mneme claims` smoke | verified |
| REQ-REVIEW-002 | CLI parser/report tests and docs | verified |
| REQ-REVIEW-003 | core, CLI, and eval scenario checks | verified |
| REQ-REVIEW-004 | core, CLI, and eval scenario checks | verified |
| REQ-REVIEW-005 | CLI unit test for inactive claim ID rejection | verified |
| REQ-REVIEW-006 | `id-lifecycle-targets-one-claim` core scenario | verified |
| REQ-REVIEW-007 | `scripts/quality-gate.sh` smoke checks | verified |
