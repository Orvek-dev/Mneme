# Live Provider Baseline Runbook

This runbook is for local, opt-in live provider evaluation. It must not be
converted into public CI unless the project adds an explicit secret policy and
redaction gate.

## Before Running

1. Confirm the working tree is clean.
2. Confirm `.env` is untracked and `.env.example` has placeholders only.
3. Export credentials in the shell or source an untracked `.env` file.
4. Choose labels that are safe to publish:

```sh
export OPENAI_API_KEY="YOUR_OPENAI_API_KEY"
export OPENAI_MODEL="gpt-5.4-mini"
```

## Command

Write live reports under ignored `evals/reports/`:

```sh
cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 3 \
  --provider-label openai \
  --model-label "${OPENAI_MODEL}" \
  --run-label local-YYYYMMDD \
  --live-provider \
  --report evals/reports/openai-live-baseline.json \
  --json
```

Use a `run_label` that identifies the local run without including private
project names, account IDs, ticket IDs, or user names.

## Success Criteria

For the current suite, treat a live baseline as usable only when:

- `baseline_metadata.live_provider` is `true`
- `baseline_metadata.provider_label` and `model_label` are present
- `pass_rate` is `1.0`
- every `category_pass_rates[].pass_rate` is `1.0`
- `failed_iterations` is `0`
- `failed_scenario_runs` is `0`

If a run fails, keep the report locally and inspect category pass rates before
changing code or prompts.

## Redaction Checklist

Before sharing any live report outside the local machine:

- Search for real API keys, tokens, passwords, and secret-like values.
- Search for local absolute paths and private project names.
- Search for account IDs, organization IDs, emails, and user names.
- Confirm no raw provider request or response body was captured.
- Confirm the report contains only public scenario IDs and aggregate results.
- Prefer sharing a summary over the raw JSON report.

Useful local scans:

```sh
rg -n "(OPENAI_API_KEY|API[_-]?KEY|TOKEN=|PASSWORD=|SECRET=|/Users/|@)" evals/reports/
rg -n "(PRIVATE_PROJECT_NAME|INTERNAL_DOC_NAME|LOCAL_TEMPLATE_NAME)" evals/reports/
```

The public repository should keep live reports ignored by default. Commit only
fixture scenarios, docs, and redacted benchmark artifacts that were intentionally
prepared for public readers.
