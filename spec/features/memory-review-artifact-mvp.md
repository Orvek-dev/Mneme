# Memory Review Artifact MVP Spec

## Scope

This phase adds a durable export surface for reviewing local memory state
without copying private stores into source control. It complements `mneme
claims` by creating Markdown or JSON artifacts that summarize stored memory,
claim lifecycle state, source citations, and agent sessions.

## Authority

- Review artifacts must be generated from the current `mneme-core` snapshot.
- Review export must not mutate store state.
- Artifacts can contain memory text and must be documented as private by
  default.

## Requirements

- [REQ-ARTIFACT-001][Ubiquitous] The CLI shall support `mneme review <path>`.
- [REQ-ARTIFACT-002][Ubiquitous] Review export shall support Markdown by
  default and JSON with `--format json`.
- [REQ-ARTIFACT-003][Event-driven] Artifacts shall include claim IDs, lifecycle
  status, scopes, claim text, and source event IDs.
- [REQ-ARTIFACT-004][Ubiquitous] Artifacts shall include claim status counts
  and scope counts.
- [REQ-ARTIFACT-005][Ubiquitous] Artifacts shall include session summaries.
- [REQ-ARTIFACT-006][Release] The local quality gate shall smoke-test Markdown
  and JSON review artifact export.
- [REQ-ARTIFACT-007][Release] Public docs shall warn that review artifacts may
  contain private memory text.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-ARTIFACT-001 | `mneme review --help` and CLI unit test | verified |
| REQ-ARTIFACT-002 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-ARTIFACT-003 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-ARTIFACT-004 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-ARTIFACT-005 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-ARTIFACT-006 | `scripts/quality-gate.sh` review smoke | verified |
| REQ-ARTIFACT-007 | `docs/memory-review-artifacts.md` | verified |
