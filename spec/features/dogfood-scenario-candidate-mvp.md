# Dogfood Scenario Candidate MVP

## Status

Verified in `v0.37.0`.

## Intent

Mneme needs a stable loop from real eval failures to future public scenarios.
The loop must keep dogfood artifacts local by default, redact obvious sensitive
patterns, and require explicit review before a candidate becomes a tracked
suite fixture.

## Requirements

- [REQ-CAND-001][Testability] The eval harness shall generate local candidate
  artifacts from failed eval or baseline JSON reports.
- [REQ-CAND-002][Safety] Generated candidate artifacts shall be sanitized so
  obvious key-like strings and local user paths do not remain in candidate YAML.
- [REQ-CAND-003][Traceability] Each candidate shall record source report kind,
  target, suite when known, source scenario ID, failed attempts, and failed
  check counts.
- [REQ-CAND-004][Reviewability] Each candidate shall include a promotion
  checklist and, when available, a nested scenario block that can be manually
  minimized and promoted.
- [REQ-CAND-005][Validation] The eval harness shall validate candidate schema,
  failed-check metadata, nested scenario validity, and redaction status before
  sharing or promotion.
- [REQ-CAND-006][Public Repository Safety] Generated candidates shall be ignored
  by default, while public scenario fixtures remain explicitly tracked under
  `evals/scenarios/`.
- [REQ-CAND-007][Automation] The release quality gate shall generate and check
  sanitized candidates from a seeded-fault baseline.

## Verification

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-CAND-001 | `mneme-eval candidate` CLI and unit tests | verified |
| REQ-CAND-002 | redaction sanitizer tests and quality-gate leak grep | verified |
| REQ-CAND-003 | candidate YAML/report fields | verified |
| REQ-CAND-004 | `promotion_checklist` and nested scenario generation | verified |
| REQ-CAND-005 | `mneme-eval candidate-check` CLI and quality gate | verified |
| REQ-CAND-006 | `.gitignore` and `evals/candidates/.gitkeep` | verified |
| REQ-CAND-007 | `scripts/quality-gate.sh full` | verified |
