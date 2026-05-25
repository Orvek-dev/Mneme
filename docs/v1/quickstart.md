# Quickstart

This is the shortest public path for trying Mneme v1 from a fresh clone. It
uses only local files and public-safe sample text.

## Prerequisites

- Rust stable with `cargo`.
- Git.

## One-Command Smoke

From the repository root:

```sh
./scripts/install-local.sh
scripts/quickstart-smoke.sh
```

The smoke test runs an isolated first memory workflow in a temporary directory:

| Step | What it verifies |
| --- | --- |
| `init` | creates a valid local JSON store and agent hook profile |
| `doctor` | reports the store/profile as healthy |
| `remember` | records a cited memory claim |
| `context` | retrieves relevant memory from the local store |
| `begin` | opens an agent session with bounded context |
| `end` | closes the session and writes a post-task memory |
| `review` | exports a public-safe Markdown review artifact |
| `validate` | confirms the store remains valid |

## Manual Flow

Use an isolated store so the first run is repeatable:

```sh
STORE="${TMPDIR:-/tmp}/mneme-quickstart.json"
CONFIG="${TMPDIR:-/tmp}/mneme-quickstart.env"
REVIEW="${TMPDIR:-/tmp}/mneme-quickstart-review.md"
rm -f "$STORE" "$STORE.bak" "$STORE.lock" "$CONFIG" "$REVIEW"

mneme init --store "$STORE" --config "$CONFIG" --no-bin
mneme doctor --store "$STORE" --config "$CONFIG"
mneme remember "user prefers local-first tools" --store "$STORE"
mneme context "local-first" --store "$STORE"
mneme begin "Draft setup plan" --query "local-first" --agent codex --store "$STORE" --json
mneme end session-001 \
  --summary "Prepared a setup plan" \
  --remember "user prefers concise setup plans" \
  --store "$STORE" \
  --json
mneme context "concise setup" --store "$STORE"
mneme review "$REVIEW" --store "$STORE"
mneme validate --store "$STORE"
```

Expected signal:

- `doctor` reports a valid store and profile.
- `context "local-first"` returns the remembered preference.
- `begin` returns `session-001` and a context pack.
- `context "concise setup"` returns the post-task memory written by `end`.
- `review` writes a Markdown artifact without needing external services.

## Agent Hook Smoke

After `mneme init`, the wrapper can read the generated runtime profile:

```sh
MNEME_AGENT_HOOK_CONFIG="$CONFIG" scripts/mneme-agent-hook.sh doctor
```

By default, wrapper doctor diagnostics do not run configured command extractors.
Use `scripts/mneme-agent-hook.sh doctor --check-extractor` only when you
intentionally want to test an extractor command.

## Next Steps

- [Local CLI](local-cli.md) for the full command surface.
- [Agent Integration](agent-integration.md) for hook contracts and automation.
- [Evidence Scorecard](evidence-scorecard.md) for the current public eval
  evidence.
- [Getting Started](getting-started.md) for the broader developer path.
