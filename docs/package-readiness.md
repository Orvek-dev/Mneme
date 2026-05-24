# Package Readiness

Mneme is pre-1.0 and the workspace crates are not published to a registry.
Each crate is marked `publish = false` to prevent accidental crates.io
publication while the project is still stabilizing its public license,
distribution, and API policy.

## Current Packages

- `mneme-core`: personal-memory engine and local JSON store abstractions.
- `mneme-cli`: local CLI binary exposed as `mneme`.
- `mneme-eval`: scenario replay harness and quality gates.

All package manifests include a public repository URL, a description, a README
reference, and the shared workspace version.

## Local Package Check

Run:

```sh
./scripts/package-check.sh
```

The package check assembles `mneme-core` and lists each workspace package's
included files. `mneme-cli` and `mneme-eval` depend on the unpublished
workspace-local `mneme-core` crate, so their file-list checks are the useful
pre-publication signal until registry publication is explicitly enabled.

The script blocks known private or generated paths such as local stores, eval
reports, private planning files, and local harness/template copies.

Cargo will warn that the manifests have no `license` or `license-file`. That
warning is expected until the project owner chooses a public license; do not add
a license field just to silence the warning.

The full quality gate also runs this check:

```sh
./scripts/quality-gate.sh full
```

## Publication Policy

Do not remove `publish = false` until all of the following are true:

- a public license has been selected and committed;
- the crate API surfaces have an explicit stability policy;
- package contents are reviewed against `scripts/package-check.sh`;
- `docs/release-checklist.md` includes registry publication steps;
- CI verifies the exact package or publish dry-run command intended for release.
