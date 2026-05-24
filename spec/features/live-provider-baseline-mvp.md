# Live Provider Baseline MVP Spec

## Scope

This phase adds repeated baseline evaluation for provider wrappers without
adding live provider calls to public CI.

## Requirements

- [REQ-LIVE-BASE-001][Testability] The eval harness shall expose
  `mneme-eval baseline`.
- [REQ-LIVE-BASE-002][Testability] The baseline command shall repeat a suite
  for a bounded number of iterations.
- [REQ-LIVE-BASE-003][Observability] Baseline reports shall include iteration
  counts, aggregate scenario pass rate, category pass rates, per-scenario pass
  rates, and run-level errors.
- [REQ-LIVE-BASE-004][Privacy] Live provider baselines shall be local and
  opt-in; public CI shall use dry-run mode only.
- [REQ-LIVE-BASE-005][Documentation] Public docs shall explain dry-run and
  live local baseline usage.
- [REQ-LIVE-BASE-006][Release] CI and release verification shall exercise the
  baseline command without provider credentials.
- [REQ-LIVE-BASE-007][Observability] Baseline reports shall include opt-in
  provider, model, run label, and live-provider metadata.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-LIVE-BASE-001 | `mneme-eval baseline` command | verified |
| REQ-LIVE-BASE-002 | `--iterations` parser with max bound | verified |
| REQ-LIVE-BASE-003 | `BaselineReport` JSON contract and category summaries | verified |
| REQ-LIVE-BASE-004 | CI uses `MNEME_OPENAI_DRY_RUN=1` | verified |
| REQ-LIVE-BASE-005 | `docs/live-provider-baseline.md` | verified |
| REQ-LIVE-BASE-006 | CI and release workflow baseline steps | verified |
| REQ-LIVE-BASE-007 | `baseline_metadata` report fields | verified |
