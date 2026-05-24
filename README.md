# Mneme

Mneme is an early personal-memory runtime and eval harness for agent workflows.
The current repository focuses on deterministic v1 behavior before adding
production provider integrations, teams, UI, or production storage.

Mneme currently provides:

- `mneme-core`: the v1 personal-memory engine.
- `mneme-cli`: a local CLI over the v1 engine and JSON file store.
- `mneme-eval`: a scenario-based eval harness with acceptance gates.
- `scripts/quality-gate.sh`: the single local gate used before PRs and
  releases.

## Current Status

Mneme is pre-1.0. The useful surface today is local development and evaluation:

- raw events are the source of truth;
- claims preserve source event citations;
- budget checks happen before extraction;
- secret-like data is blocked from active context;
- corrections and forgets are auditable lifecycle transitions;
- local JSON stores include schema metadata, atomic writes, backups, repair,
  import/export, and compaction;
- agents can open and close task sessions with scoped context and post-task
  memory writes;
- extraction and storage are behind adapter boundaries;
- model-backed extraction experiments can use a provider-neutral command
  adapter without adding API keys to the repo;
- a public OpenAI wrapper example can run through the same command protocol,
  with CI using deterministic dry-run mode;
- workspace crates are package-checked locally but marked `publish = false`
  until the public license and distribution policy are finalized.

See [docs/v1-stability.md](docs/v1-stability.md) for the current stability
contract.

For a step-by-step first run, see [Getting Started](docs/getting-started.md).

## Quickstart

Install Rust, then run:

```sh
cargo run -p mneme-cli -- doctor
cargo run -p mneme-eval -- doctor
```

Try the local CLI with an isolated store:

```sh
STORE=/tmp/mneme.json
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store "$STORE"
cargo run -p mneme-cli -- context "local-first" --store "$STORE" --json
cargo run -p mneme-cli -- correct "user prefers local-first tools" "user prefers desktop IDE" --store "$STORE"
cargo run -p mneme-cli -- forget "user prefers desktop IDE" --store "$STORE"
cargo run -p mneme-cli -- snapshot --store "$STORE" --json
cargo run -p mneme-cli -- validate --store "$STORE"
cargo run -p mneme-cli -- compact --store "$STORE"
cargo run -p mneme-cli -- begin "Draft setup plan" --query "local-first" --agent codex --store "$STORE" --json
cargo run -p mneme-cli -- end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store "$STORE" --json
```

Without `--store`, the CLI writes to `.mneme/mneme-v1.json` in the current
directory. `.mneme/` is ignored by git.

For model-backed extraction experiments, use `ingest` with a local wrapper:

```sh
cargo run -p mneme-cli -- ingest "the user prefers local-first tools" \
  --extractor command \
  --extractor-command ./mneme-extractor-wrapper \
  --store "$STORE"
```

## Eval Harness

Validate and run the public core suite:

```sh
cargo run -p mneme-eval -- validate --suite core
cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
```

Run the runtime maintenance suite:

```sh
cargo run -p mneme-eval -- validate --suite runtime
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
```

Run the agent integration suite:

```sh
cargo run -p mneme-eval -- validate --suite agent
cargo run -p mneme-eval -- run --suite agent --target mneme-v1
```

Run the opt-in command extraction suite:

```sh
cargo run -p mneme-eval -- validate --suite model
cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command evals/fixtures/command-extractor.sh
```

Run the OpenAI wrapper example without provider credentials:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py
```

Build a repeated baseline report for provider-wrapper quality tracking:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --report evals/reports/openai-dry-run-baseline.json
```

Baseline JSON includes aggregate, category-level, and per-scenario pass rates.
Gate a saved baseline report before treating it as usable:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

Run the acceptance gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
```

Use `--json` for machine-readable reports.

## Development Checks

Before opening a PR, run:

```sh
./scripts/quality-gate.sh full
```

Check package assembly directly:

```sh
./scripts/package-check.sh
```

Generated eval reports and local stores are ignored. Public scenarios under
`evals/scenarios/` are tracked.

CI runs on pull requests and `main` pushes only. Branch pushes do not trigger
the full gate, which keeps action usage aligned with phase-sized work.

## Repository Layout

```text
crates/mneme-core   v1 engine, storage port, extraction port
crates/mneme-cli    local developer CLI
crates/mneme-eval   scenario replay, target adapters, acceptance gates
docs/               public contracts and usage docs
evals/              public scenario fixtures
scripts/            local quality, safety, and live-baseline helpers
spec/               feature specs and verification maps
```

## Documentation

- [Local CLI](docs/local-cli.md)
- [Getting Started](docs/getting-started.md)
- [Personal Runtime](docs/personal-runtime.md)
- [Agent Integration](docs/agent-integration.md)
- [Eval Scenario Format](docs/eval-scenario-format.md)
- [Eval Acceptance Gate](docs/eval-harness-acceptance.md)
- [Eval Target Adapter Contract](docs/eval-target-adapter-contract.md)
- [Extraction Adapter Contract](docs/extraction-adapter-contract.md)
- [Model Extraction Adapter](docs/model-extraction-adapter.md)
- [OpenAI Provider Wrapper](docs/openai-provider-wrapper.md)
- [Live Provider Baseline](docs/live-provider-baseline.md)
- [Live Provider Baseline Runbook](docs/live-provider-baseline-runbook.md)
- [Package Readiness](docs/package-readiness.md)
- [CI Cost Policy](docs/ci-cost-policy.md)
- [Phase 3 Readiness](docs/phase-3-readiness.md)
- [Mneme v1 Personal Core](docs/mneme-v1-personal-core.md)
- [Mneme v1 Stability](docs/v1-stability.md)
- [Release Checklist](docs/release-checklist.md)
