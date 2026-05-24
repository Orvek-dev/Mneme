# OpenAI Provider Wrapper

Mneme keeps provider calls outside `mneme-core`. The OpenAI wrapper is a public
example command that implements the `mneme.extractor.command.v1` protocol, so
the engine still sees only stdin/stdout JSON.

## Files

- `wrappers/openai_extractor.py`: executable command wrapper.
- `.env.example`: placeholder environment variables for local use.

The wrapper uses the OpenAI Responses API with Structured Outputs. It defaults
to `gpt-5.4-mini`, which is a lower-cost mini model that supports the
`/v1/responses` endpoint and structured outputs.

The wrapper keeps provider-specific quality guardrails outside `mneme-core`.
It prefilters obvious secret-like values locally, prompts the model to extract
only durable memory, and suppresses model claims for transient answer/task
instructions, quoted sample or test data, and rejected alternatives when the
user stated a preferred alternative.

## Dry-Run Verification

CI and release verification must not require provider credentials. Use dry-run
mode to exercise the wrapper protocol and the model eval suite without network
calls:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --json
```

Run the matching acceptance gate:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- acceptance --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py
```

When the wrapper is installed in an agent runtime profile, default diagnostics
remain no-cost:

```sh
MNEME_AGENT_HOOK_CONFIG=.mneme/mneme-agent-hook.env \
MNEME_OPENAI_DRY_RUN=1 \
  scripts/mneme-agent-hook.sh doctor --check-extractor
```

Without `--check-extractor`, wrapper doctor reports the configured extractor
command but does not execute it.

Build a repeated dry-run baseline:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --json
```

Gate the saved baseline report before treating it as usable:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

## Live Local Use

Keep credentials in the local shell or an untracked `.env` file:

```sh
export OPENAI_API_KEY="YOUR_OPENAI_API_KEY"
export OPENAI_MODEL="gpt-5.4-mini"
```

Then use the wrapper through the command extractor:

```sh
cargo run -p mneme-cli -- ingest "I work best with local-first tools." \
  --extractor command \
  --extractor-command wrappers/openai_extractor.py \
  --store /tmp/mneme.json
```

For repeated live evals, use `mneme-eval baseline` and write the report under
ignored `evals/reports/`. See [Live Provider Baseline](live-provider-baseline.md)
and [Live Provider Baseline Runbook](live-provider-baseline-runbook.md).

## Safety Rules

- Never commit real `.env` files, API keys, model transcripts, or private eval
  reports.
- Keep live provider evals local unless a CI secret policy is explicitly added.
- Obvious secret-like values are prefiltered locally by the wrapper before any
  provider request, then Mneme marks those claims as blocked from active
  context.
- Transient instructions and quoted sample/test data should return `claim:
  null`, not durable memory.
- Keep the default core suite provider-free; provider wrappers belong behind
  the opt-in `mneme-v1-command` target.
