# Release Checklist

Mneme releases should be cut only from `main` after CI has passed.

## Preflight

Run the same checks locally before creating a tag:

```sh
./scripts/quality-gate.sh release
```

For package-specific inspection, run:

```sh
./scripts/distribution-policy-check.sh
./scripts/package-check.sh
```

For API documentation inspection, run:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Run a local CLI smoke check with an isolated store:

```sh
STORE=/tmp/mneme-release-smoke.json
rm -f "$STORE" "$STORE.bak"
./scripts/install-local.sh
mneme init --force
mneme doctor --json
mneme remember "user prefers local-first tools" --store "$STORE"
mneme context "local-first" --store "$STORE" --json
mneme quality --store "$STORE" --json
mneme curate --store "$STORE" --json
mneme repair --check --store "$STORE" --json
mneme restore --check --store "$STORE" --json
mneme hook doctor --store "$STORE"
scripts/mneme-agent-hook.sh doctor
MNEME_AGENT_HOOK_CONFIG=examples/mneme-agent-hook.env.example scripts/mneme-agent-hook.sh doctor
rm -f "$STORE" "$STORE.bak"
```

Run `scripts/mneme-agent-hook.sh doctor --check-extractor` only for an
explicit extractor smoke check with a fixture or dry-run provider wrapper.

## Public Safety

Before pushing a release tag, confirm:

- no private planning documents are tracked;
- no local `.mneme/` store is tracked;
- no generated eval report is tracked;
- no token, key, or secret file is tracked;
- package manifests still have `publish = false` unless a public registry
  release is intentionally being prepared;
- MIT license metadata still matches the committed `LICENSE` file and
  distribution policy;
- `scripts/install-local.sh` still installs and smokes the local `mneme` and
  `mneme-mcp` binaries;
- `CHANGELOG.md` describes the release-relevant changes;
- README commands still match the actual CLI and eval behavior.
- Rustdoc builds cleanly when public API docs or examples changed.

The quality gate runs `scripts/public-safety-check.sh`, but release owners
should still inspect unusual new files before tagging.

## Tagging

Create annotated tags from `main`:

```sh
git switch main
git pull --ff-only origin main
git tag -a vX.Y.Z -m "vX.Y.Z"
git push origin vX.Y.Z
```

The release workflow verifies the workspace again. Release publication requires
the workflow-scoped `GITHUB_TOKEN` to have `contents: write` permission. The
workflow requests that permission explicitly. It marks `v0.x` tags as GitHub
prereleases; `v1.x` tags are normal public source releases.

CI runs on pull requests and `main` pushes only. Feature branch pushes are
intentionally not full CI triggers; run `scripts/quality-gate.sh` locally before
opening one phase-sized PR.

After the workflow completes, verify the public release:

```sh
gh release view vX.Y.Z --json tagName,isPrerelease,url
```

For the v1.0.0 readiness boundary, also run:

```sh
scripts/v1-final-readiness-check.sh
```
