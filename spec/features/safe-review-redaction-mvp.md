# Safe Review Redaction MVP Spec

## Scope

This phase makes memory review artifacts safer to generate during local
development and release review. `mneme review` remains an inspection command,
but its default output should not expose blocked-secret claim values or obvious
secret-like text.

## Authority

- The store remains the source of truth. Redaction applies only to review
  artifacts and review stdout reports.
- Review export must not mutate store state.
- Raw sensitive export must require an explicit command-line opt-in.

## Requirements

- [REQ-REDACT-001][Privacy] `mneme review` shall redact `blocked_secret` claim
  object text by default.
- [REQ-REDACT-002][Privacy] `mneme review` shall redact obvious secret-like
  field text by default.
- [REQ-REDACT-003][Ubiquitous] Review artifacts shall include redaction
  metadata with policy, enabled state, and redacted counts.
- [REQ-REDACT-004][Permission] `mneme review --include-sensitive` shall export
  raw sensitive claim text for local private inspection.
- [REQ-REDACT-005][Release] The local quality gate shall fail if safe review
  artifacts include secret-like claim text.
- [REQ-REDACT-006][Release] Public docs shall describe the default safe policy
  and the local-only raw export option.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-REDACT-001 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-REDACT-002 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-REDACT-003 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-REDACT-004 | `review_exports_markdown_and_json_artifacts` | verified |
| REQ-REDACT-005 | `scripts/quality-gate.sh` review redaction smoke | verified |
| REQ-REDACT-006 | `docs/v1/memory-review-artifacts.md` | verified |
