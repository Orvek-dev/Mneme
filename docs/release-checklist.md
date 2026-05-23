# Release Checklist

Mneme releases should be cut only from `main` after CI has passed.

## Preflight

Run the same checks locally before creating a tag:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo run -p mneme-cli -- doctor
cargo run -p mneme-eval -- doctor
cargo run -p mneme-eval -- validate --suite core
cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
```

Run a local CLI smoke check with an isolated store:

```sh
STORE=/tmp/mneme-release-smoke.json
rm -f "$STORE"
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store "$STORE"
cargo run -p mneme-cli -- context "local-first" --store "$STORE" --json
rm -f "$STORE"
```

## Public Safety

Before pushing a release tag, confirm:

- no private planning documents are tracked;
- no local `.mneme/` store is tracked;
- no generated eval report is tracked;
- no token, key, or secret file is tracked;
- `CHANGELOG.md` describes the release-relevant changes;
- README commands still match the actual CLI and eval behavior.

## Tagging

Create annotated tags from `main`:

```sh
git switch main
git pull --ff-only origin main
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
```

The release workflow verifies the workspace again. Release publication requires
`MNEME_RELEASE_TOKEN` to be configured with contents write permission.
