# Mneme

Mneme is an early personal-memory runtime and eval harness for agent workflows.
The current repository focuses on deterministic v1 behavior before adding
production provider integrations, teams, UI, or production storage.

Mneme currently provides:

- `mneme-core`: the v1 personal-memory engine.
- `mneme-cli`: a local CLI over the v1 engine and JSON file store.
- `mneme-eval`: a scenario-based eval harness with acceptance gates.
- `scripts/install-local.sh`: a local installer for the `mneme` CLI.
- `scripts/quality-gate.sh`: the single local gate used before PRs and
  releases.

## Current Status

Mneme is pre-1.0. The useful surface today is local development and evaluation:

- raw events are the source of truth;
- claims preserve source event citations;
- context retrieval is filtered by allowed memory scopes before relevance;
- context packs are deterministically ranked and capped before agent use;
- budget checks happen before extraction;
- secret-like data is blocked from active context;
- corrections and forgets are auditable lifecycle transitions;
- stored claims can be reviewed and changed by stable claim ID;
- stored memory quality can be inspected as duplicate, blocked-secret, and
  inactive-history review queues;
- stored memory can be curated through dry-run plans, explicit duplicate cleanup,
  explicit compaction of non-active records, and backup-backed rollback;
- stored memory can be exported as Markdown or JSON review artifacts with
  quality findings and sensitive claim text redacted by default;
- local JSON stores include schema metadata, write locks, atomic writes,
  backups, repair readiness checks, schema normalization, explicit backup
  restore, import/export, and non-active record compaction;
- the local CLI can be installed as `mneme` for first-run personal workflows;
- `mneme init` creates a local store and agent hook profile for a new
  workspace;
- `mneme doctor` reports workspace health for the local store and agent hook
  profile;
- agents can open and close task sessions with scoped context and post-task
  memory writes;
- agent hooks expose a stable JSON envelope for doctor/begin/end automation;
- `scripts/mneme-agent-hook.sh` provides an environment-configurable local
  wrapper for agent runtimes;
- wrapper doctor diagnostics report loaded runtime settings without running
  provider-backed extractors by default;
- agent hook runtime profiles can keep local store, agent, scope, and item-cap
  settings out of each invocation;
- extraction and storage are behind adapter boundaries;
- model-backed extraction experiments can use a provider-neutral command
  adapter and expanded model eval suite without adding API keys to the repo;
- a public OpenAI wrapper example can run through the same command protocol,
  with CI using deterministic dry-run mode;
- failed eval or baseline reports can be converted into ignored, sanitized
  scenario candidate artifacts for dogfood feedback review;
- workspace crates are package-checked locally but marked `publish = false`
  until the public license and distribution policy are finalized.

See [Mneme v1 Stability](docs/v1/v1-stability.md) for the current stability
contract.
See [API Contract](docs/project/api-contract.md) for the current Rust API
surface and documentation gate.
See [Distribution Policy](docs/project/distribution-policy.md) for the current
license and registry publication policy.

For a step-by-step first run, see [Getting Started](docs/v1/getting-started.md).

## Quickstart

Install Rust, then install the local CLI:

```sh
./scripts/install-local.sh
mneme doctor
mneme init
mneme doctor --json
mneme help
cargo run -p mneme-eval -- doctor
cargo run -p mneme-eval -- help
```

Try the local CLI with an isolated store:

```sh
STORE=/tmp/mneme.json
mneme remember "user prefers local-first tools" --store "$STORE"
mneme claims --status active --store "$STORE" --json
mneme context "local-first" --store "$STORE" --json
mneme remember "user prefers project launch reviews" --scope project-alpha --store "$STORE"
mneme context "project launch" --scope project-alpha --max-items 3 --store "$STORE" --json
mneme correct "user prefers local-first tools" "user prefers desktop IDE" --store "$STORE"
mneme forget "user prefers desktop IDE" --store "$STORE"
mneme quality --store "$STORE" --json
mneme curate --store "$STORE" --json
mneme review /tmp/mneme-review.md --store "$STORE"
mneme snapshot --store "$STORE" --json
mneme validate --store "$STORE"
mneme repair --check --store "$STORE" --json
mneme restore --check --store "$STORE" --json
mneme compact --store "$STORE"
mneme begin "Draft setup plan" --query "local-first" --agent codex --store "$STORE" --json
mneme end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store "$STORE" --json
mneme hook doctor --store "$STORE"
mneme hook begin "Draft setup plan" --query "local-first" --agent codex --store "$STORE"
mneme hook end session-002 --summary "Prepared setup plan" --store "$STORE"
scripts/mneme-agent-hook.sh doctor
```

Use a local ignored runtime profile when wiring an agent:

```sh
mneme init
mneme init --extractor-command ./mneme-extractor-wrapper
mneme doctor
scripts/mneme-agent-hook.sh doctor
```

`scripts/mneme-agent-hook.sh doctor` does not run configured command extractors
by default. Use `scripts/mneme-agent-hook.sh doctor --check-extractor` only
when you intentionally want to smoke the configured extractor command.

Without `--store`, the CLI writes to `.mneme/mneme-v1.json` in the current
directory. `.mneme/` is ignored by git.

For model-backed extraction experiments, use `ingest` with a local wrapper:

```sh
cargo run -p mneme-cli -- ingest "the user prefers local-first tools" \
  --extractor command \
  --extractor-command ./mneme-extractor-wrapper \
  --store "$STORE"
```

The same command extractor can be used for session-end memory notes:

```sh
cargo run -p mneme-cli -- hook end session-001 \
  --remember "For future planning docs, keep explanations direct and skip motivational language." \
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

Summarize a saved baseline report for local triage:

```sh
cargo run -p mneme-eval -- baseline-summary evals/reports/openai-dry-run-baseline.json
```

Compare two saved baseline reports before accepting a change:

```sh
cargo run -p mneme-eval -- baseline-compare \
  evals/reports/before.json \
  evals/reports/after.json \
  --fail-on-regression
```

Create local candidate artifacts from a failed report before promoting any new
public scenario:

```sh
cargo run -p mneme-eval -- candidate evals/reports/openai-dry-run-baseline.json \
  --out-dir evals/candidates/openai \
  --limit 3
cargo run -p mneme-eval -- candidate-check evals/candidates/openai
cargo run -p mneme-eval -- candidate-promote \
  evals/candidates/openai/dogfood-example.candidate.yaml \
  --suite model \
  --filename dogfood-example.yaml \
  --apply
```

Run the acceptance gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
```

Use `--json` for machine-readable reports.
Use `help` or `<command> --help` to inspect command-specific usage:

```sh
cargo run -p mneme-cli -- help begin
cargo run -p mneme-eval -- baseline-gate --help
cargo run -p mneme-eval -- baseline-summary --help
cargo run -p mneme-eval -- baseline-compare --help
cargo run -p mneme-eval -- candidate-promote --help
```

## Development Checks

Before opening a PR, run:

```sh
./scripts/quality-gate.sh full
```

Check package assembly directly:

```sh
./scripts/package-check.sh
```

Check distribution guardrails directly:

```sh
./scripts/distribution-policy-check.sh
```

Build API docs with warnings denied:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Generated eval reports, candidate artifacts, and local stores are ignored.
Public scenarios under `evals/scenarios/` are tracked.

CI runs on pull requests and `main` pushes only. Branch pushes do not trigger
the full gate, which keeps action usage aligned with phase-sized work.

## Repository Layout

```text
README.md             main public entry point
crates/mneme-core     shared v1 personal-memory engine
crates/mneme-cli      local v1 CLI
crates/mneme-eval     reusable eval harness CLI
docs/v1/              Mneme v1 personal-memory docs
docs/v2/              future Mneme v2 team-memory scope
docs/eval-harness/    scenario, baseline, candidate, and provider eval docs
docs/project/         roadmap, release, packaging, and policy docs
evals/                public scenario fixtures
scripts/              local quality, safety, and live-baseline helpers
spec/                 feature specs and verification maps
```

## Documentation

- [Documentation Map](docs/README.md)
- [Mneme v1](docs/v1/README.md)
- [Mneme v2](docs/v2/README.md)
- [Eval Harness](docs/eval-harness/README.md)
- [Project and Release](docs/project/README.md)
