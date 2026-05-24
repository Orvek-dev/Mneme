# Memory Quality & Review Loop MVP

## Intent

After store hardening, the next product risk is low-quality memory accumulation.
Users need a read-only way to see which memories deserve review before they
forget, correct, compact, or export artifacts.

## Requirements

- [REQ-QUALITY-001][Ubiquitous] `mneme quality` shall inspect the configured
  store without mutating files.
- [REQ-QUALITY-002][Ubiquitous] Quality JSON shall include `command`, `ok`,
  `health`, claim counts, duplicate active claim counts, review item count,
  findings, review queue items, and next commands.
- [REQ-QUALITY-003][Review] Quality reports shall flag duplicate active claims
  by normalized claim text and scope.
- [REQ-QUALITY-004][Privacy] Quality reports shall flag `blocked_secret` claims
  without exposing blocked secret values by default.
- [REQ-QUALITY-005][Review] Quality reports shall flag inactive lifecycle
  history for superseded and forgotten claims and suggest review-before-compact
  commands.
- [REQ-QUALITY-006][Docs] `mneme review` artifacts shall include the same
  memory quality findings and review queue.
- [REQ-QUALITY-007][Evaluation] The eval harness shall support `quality`
  expectations and include a core scenario for duplicate/blocked/inactive
  review queue behavior.
- [REQ-QUALITY-008][Release] The quality gate shall verify `mneme quality`,
  review artifact quality sections, and default secret redaction.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-QUALITY-001 | `run_quality` and CLI quality test | verified |
| REQ-QUALITY-002 | `MemoryQualityReport` and CLI quality test | verified |
| REQ-QUALITY-003 | `build_memory_quality_report` duplicate grouping | verified |
| REQ-QUALITY-004 | `quality_claim_text` and redaction checks | verified |
| REQ-QUALITY-005 | inactive history queue in `build_memory_quality_report` | verified |
| REQ-QUALITY-006 | `render_review_markdown` and review JSON embedding | verified |
| REQ-QUALITY-007 | `quality` expected checks and `memory-quality-review-loop.yaml` | verified |
| REQ-QUALITY-008 | `scripts/quality-gate.sh` quality smoke | verified |
