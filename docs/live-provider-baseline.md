# Live Provider Baseline

`mneme-eval baseline` repeats a suite and summarizes stability across
iterations. It is meant for provider-wrapper quality tracking where one passing
run is not enough evidence.

## Dry-Run Baseline

CI and release verification use deterministic dry-run mode:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --json
```

The JSON report includes:

- `iterations`, `passed_iterations`, and `failed_iterations`
- `baseline_metadata`, including live-provider status and optional provider,
  model, and run labels
- `total_scenario_runs`, `passed_scenario_runs`, and `failed_scenario_runs`
- aggregate `pass_rate`
- category pass rates from scenario tags that start with `category-`
- per-scenario pass rates
- `failure_summary`, including failed categories, failed scenarios, and failed
  check counts
- run-level errors, when a provider wrapper fails before a scenario report can
  be produced

Gate a saved report before treating it as usable:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

The gate enforces strict default thresholds: aggregate pass rate `1.0`, every
category pass rate `1.0`, no failed iterations, no failed scenario runs, provider
and model labels present, command extractor target metadata present, and no
obvious secret or local-path redaction findings.

Summarize a saved report before deciding what to inspect next:

```sh
cargo run -p mneme-eval -- baseline-summary evals/reports/openai-dry-run-baseline.json
```

Summary output includes triage status, redaction findings, top failed
categories, top failed scenarios, top failed checks, and recommended next
actions. `baseline-summary` exits successfully for failed baseline reports so
the failure can be inspected locally; it is not a replacement for
`baseline-gate`.

## Live Local Baseline

Live provider calls are local and opt-in. Keep credentials in the shell or an
untracked `.env` file:

```sh
export OPENAI_API_KEY="YOUR_OPENAI_API_KEY"
export OPENAI_MODEL="gpt-5.4-mini"
```

Then run:

```sh
cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 3 \
  --provider-label openai \
  --model-label gpt-5.4-mini \
  --run-label local-YYYYMMDD \
  --live-provider \
  --report evals/reports/openai-live-baseline.json
```

`evals/reports/` is ignored by git. Do not commit live reports unless they have
been manually redacted and are intended as public benchmark artifacts.

For live runs, require the live-provider metadata and a safe run label:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-live-baseline.json \
  --require-live-provider \
  --require-run-label
```

Write a local triage summary next to the live report:

```sh
cargo run -p mneme-eval -- baseline-summary evals/reports/openai-live-baseline.json \
  --report evals/reports/openai-live-baseline.summary.json
```

## MVP Acceptance

For the current MVP, treat a provider baseline as acceptable only when:

- aggregate `pass_rate` is `1.0`
- every category pass rate is `1.0`
- `failed_iterations` is `0`
- `failed_scenario_runs` is `0`
- `failure_summary.failed_checks` is empty
- `baseline-gate` passes
- secret-blocking scenarios have no active secret leakage
- citation checks pass in every iteration
- no-claim categories pass for transient instructions and quoted sample data

Future phases can relax this into explicit thresholds once enough historical
live provider reports exist.

See [Live Provider Baseline Runbook](live-provider-baseline-runbook.md) for the
full live execution and redaction checklist.
