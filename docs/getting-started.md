# Getting Started

This guide is for a new developer using the public repository without private
planning notes or local templates.

## Prerequisites

- Rust stable with `cargo`, `rustfmt`, and `clippy`.
- Git.
- Optional: GitHub CLI `gh` for release inspection and PR work.

Mneme is pre-1.0 and currently optimized for local development, deterministic
evals, and provider-wrapper experiments. The crates are intentionally marked
`publish = false` until the project has a final license and public distribution
policy.
The current distribution state is documented in
`docs/distribution-policy.md`.

## First Run

From the repository root, install the local CLI and inspect the command
surface:

```sh
./scripts/install-local.sh
mneme doctor
mneme init
mneme doctor --json
mneme help
cargo run -p mneme-eval -- doctor
cargo run -p mneme-eval -- help
```

Inspect command-specific usage, then use an isolated store for local
experiments:

```sh
mneme help begin
cargo run -p mneme-eval -- run --help
```

```sh
STORE=/tmp/mneme-getting-started.json
rm -f "$STORE"
mneme remember "user prefers local-first tools" --store "$STORE"
mneme claims --status active --store "$STORE" --json
mneme context "local-first" --store "$STORE" --json
mneme remember "user prefers project launch reviews" --scope project-alpha --store "$STORE"
mneme context "project launch" --scope project-alpha --max-items 3 --store "$STORE" --json
mneme validate --store "$STORE"
```

`mneme init` creates the default `.mneme/mneme-v1.json` store and
`.mneme/mneme-agent-hook.env` runtime profile in the current directory.
`mneme doctor` is the canonical health check for that store/profile pair.
`.mneme/` is ignored by git.

## Agent Session Flow

Agents can retrieve task-scoped context and then write explicit post-task memory:

```sh
STORE=/tmp/mneme-agent-session.json
rm -f "$STORE"
mneme remember "user prefers local-first tools" --store "$STORE"
mneme begin "Draft setup plan" \
  --query "local-first" \
  --scope private \
  --max-items 3 \
  --agent codex \
  --store "$STORE" \
  --json
mneme hook begin "Draft setup plan" \
  --query "local-first" \
  --agent codex \
  --store "$STORE"
scripts/mneme-agent-hook.sh doctor
mneme end session-001 \
  --summary "Prepared a concise setup plan" \
  --remember "user prefers concise setup plans" \
  --store "$STORE" \
  --json
```

For repeated local use, initialize the ignored runtime directory and let the
wrapper load the generated profile:

```sh
mneme init
mneme doctor
scripts/mneme-agent-hook.sh doctor
```

## Eval Harness

Run the deterministic suites before changing behavior:

```sh
cargo run -p mneme-eval -- validate --suite core
cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
cargo run -p mneme-eval -- run --suite agent --target mneme-v1
```

Run the model suite with the deterministic command fixture:

```sh
cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command evals/fixtures/command-extractor.sh
```

## Provider Wrapper Dry Run

The OpenAI wrapper can be exercised without credentials:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --run-label local-dry-run \
  --report evals/reports/openai-dry-run-baseline.json

cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

`evals/reports/` is ignored by git.

## Full Local Gate

Before opening a phase-sized PR:

```sh
./scripts/quality-gate.sh full
```

That gate runs formatting, clippy, tests, Rustdoc with warnings denied, CLI
smoke checks, eval suites, dry-run provider baseline checks, public-safety
checks, and package assembly checks.

For API-level work, inspect the current contract and docs locally:

```sh
cargo run -p mneme-core --example personal_memory
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Public Repository Rules

- Do not commit local stores, generated reports, credentials, or private
  planning notes.
- Keep provider credentials in the shell or an untracked `.env` file.
- Do not remove `publish = false` or add license metadata until the owner has
  committed a license file and updated the distribution policy.
- Add behavior changes through public specs, evals, tests, or docs.
- Keep live provider reports local unless they are intentionally redacted public
  artifacts.
