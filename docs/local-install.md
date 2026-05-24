# Local Install

Mneme can be installed locally as the `mneme` CLI binary from the repository
root:

```sh
./scripts/install-local.sh
```

The installer wraps:

```sh
cargo install --path crates/mneme-cli --locked --root "$HOME/.cargo" --force
```

By default, the binary is written to `$HOME/.cargo/bin/mneme`. Ensure that
directory is on `PATH`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

Then verify:

```sh
mneme --version
mneme doctor
mneme init
mneme doctor --json
mneme help
mneme review --help
```

## Options

Install to a temporary or custom root:

```sh
./scripts/install-local.sh --root /tmp/mneme-install
/tmp/mneme-install/bin/mneme doctor
```

Use a debug build for faster smoke checks:

```sh
./scripts/install-local.sh --root /tmp/mneme-install --debug
```

Skip automatic reinstall or smoke checks:

```sh
./scripts/install-local.sh --no-force
./scripts/install-local.sh --skip-smoke
```

## First Workspace

Initialize the current directory before wiring an agent:

```sh
mneme init
mneme doctor
```

This creates:

- `.mneme/mneme-v1.json`: a valid empty v1 store.
- `.mneme/mneme-agent-hook.env`: a runtime profile for
  `scripts/mneme-agent-hook.sh`.

The profile includes the current `mneme` binary path by default, plus
`MNEME_STORE`, `MNEME_AGENT_ID`, `MNEME_SCOPE`, and `MNEME_MAX_ITEMS`.

Refresh the generated files intentionally:

```sh
mneme init --force
mneme doctor --json
```

Use explicit paths for automation tests or custom workspaces:

```sh
mneme init \
  --store /tmp/mneme.json \
  --config /tmp/mneme-agent-hook.env \
  --bin "$(command -v mneme)" \
  --json
```

## Isolated Store

Use an isolated store while testing:

```sh
STORE=/tmp/mneme-installed.json
mneme remember "user prefers installed CLI workflows" --store "$STORE"
mneme context "installed CLI" --store "$STORE" --json
mneme review /tmp/mneme-installed-review.md --store "$STORE"
```

Without `--store`, `mneme` writes to `.mneme/mneme-v1.json` in the current
directory. `.mneme/` is ignored by git.
