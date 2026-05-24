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

## First Store

Use an isolated store while testing:

```sh
STORE=/tmp/mneme-installed.json
mneme remember "user prefers installed CLI workflows" --store "$STORE"
mneme context "installed CLI" --store "$STORE" --json
mneme review /tmp/mneme-installed-review.md --store "$STORE"
```

Without `--store`, `mneme` writes to `.mneme/mneme-v1.json` in the current
directory. `.mneme/` is ignored by git.
