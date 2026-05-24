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
- run-level errors, when a provider wrapper fails before a scenario report can
  be produced

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

## MVP Acceptance

For the current MVP, treat a provider baseline as acceptable only when:

- aggregate `pass_rate` is `1.0`
- every category pass rate is `1.0`
- `failed_iterations` is `0`
- secret-blocking scenarios have no active secret leakage
- citation checks pass in every iteration

Later phases can relax this into explicit thresholds once the model suite has
more scenarios and enough historical reports.

See [Live Provider Baseline Runbook](live-provider-baseline-runbook.md) for the
full live execution and redaction checklist.
