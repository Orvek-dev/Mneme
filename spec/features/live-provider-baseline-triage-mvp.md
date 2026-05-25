# Phase 30: Live Provider Baseline Triage MVP

## Summary

Phase 30 makes provider baseline results easier to inspect before spending more
live API budget. It adds a baseline summary command that reads saved baseline
JSON reports and produces stable triage output for passing and failing runs.

## Requirements

- [REQ-LIVE-TRIAGE-001][Ubiquitous] `mneme-eval baseline-summary
  <baseline-report.json>` shall read an existing baseline report and summarize
  pass rate, failed iterations, failed scenario runs, failed categories,
  failed scenarios, and failed checks.
- [REQ-LIVE-TRIAGE-002][Safety] Baseline summary shall include the same
  redaction scan findings used by `baseline-gate` so local reports are not
  mistaken for public-safe artifacts.
- [REQ-LIVE-TRIAGE-003][Ubiquitous] Baseline summary shall support `--json` and
  `--report <path>` for machine-readable local artifacts.
- [REQ-LIVE-TRIAGE-004][Ubiquitous] Baseline summary shall exit successfully for
  failed baseline reports so users can inspect failures without bypassing
  `baseline-gate`.
- [REQ-LIVE-TRIAGE-005][Release] The release quality gate shall verify both a
  passing dry-run baseline summary and a failing seeded-fault baseline summary.
- [REQ-LIVE-TRIAGE-006][Docs] Live provider docs and runbooks shall describe
  the summary artifact as local triage output, not a public benchmark by
  default.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-LIVE-TRIAGE-001 | `build_baseline_summary_report` and CLI tests | verified |
| REQ-LIVE-TRIAGE-002 | `redaction_findings` reuse and summary tests | verified |
| REQ-LIVE-TRIAGE-003 | parser tests and quality-gate summary report writes | verified |
| REQ-LIVE-TRIAGE-004 | `run_baseline_summary` returns after emitting failed summaries | verified |
| REQ-LIVE-TRIAGE-005 | `scripts/quality-gate.sh` passing and seeded-fault summaries | verified |
| REQ-LIVE-TRIAGE-006 | live provider docs and runbook updates | verified |
