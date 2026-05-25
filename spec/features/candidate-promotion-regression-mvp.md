# Candidate Promotion & Regression Intelligence MVP

## Summary

Phase 32 closes the v1 eval feedback loop. Failed reports can already generate
local candidate artifacts; this phase promotes reviewed candidates into public
scenario suites and compares baseline reports before release.

## Requirements

- [REQ-PROMOTE-001][Reviewability] The eval harness shall validate and promote
  a reviewed candidate with `mneme-eval candidate-promote <candidate.yaml>`.
- [REQ-PROMOTE-002][Safety] Candidate promotion shall require candidate schema
  validation, redaction checks, nested scenario validation, duplicate scenario
  ID checks, and a safe destination path.
- [REQ-PROMOTE-003][Explicit Mutation] Candidate promotion shall be a dry run
  by default and shall write a scenario only when `--apply` is provided.
- [REQ-PROMOTE-004][Public Repository Safety] Candidate promotion shall write
  only the nested `scenario` block, not local candidate metadata.
- [REQ-REGRESS-001][Regression Analysis] The eval harness shall compare two
  baseline reports with `mneme-eval baseline-compare <before> <after>`.
- [REQ-REGRESS-002][Triage] Baseline comparison shall report aggregate,
  category, scenario, and failed-check deltas.
- [REQ-REGRESS-003][Release Gate] Baseline comparison shall support
  `--fail-on-regression` so release candidates can fail on detected
  regressions.
- [REQ-REGRESS-004][Automation] The release quality gate shall exercise
  candidate promotion and baseline regression detection.
- [REQ-REGRESS-005][Documentation] Public docs shall describe candidate
  promotion and baseline comparison workflows.

## Verification Map

| Requirement | Evidence | Status |
|---|---|---|
| REQ-PROMOTE-001 | `candidate-promote` CLI and candidate unit test | verified |
| REQ-PROMOTE-002 | promotion gates and quality-gate promotion smoke | verified |
| REQ-PROMOTE-003 | `--apply` option and dry-run help text | verified |
| REQ-PROMOTE-004 | promoted scenario write path and redaction scan | verified |
| REQ-REGRESS-001 | `baseline-compare` CLI and trend unit tests | verified |
| REQ-REGRESS-002 | baseline compare JSON report contract | verified |
| REQ-REGRESS-003 | `--fail-on-regression` CLI path | verified |
| REQ-REGRESS-004 | `scripts/quality-gate.sh` checks | verified |
| REQ-REGRESS-005 | README and eval-harness docs | verified |
