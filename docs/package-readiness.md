# Package Readiness

Mneme is pre-1.0 and the workspace crates are not published to a registry.
Each crate is marked `publish = false` to prevent accidental crates.io
publication while the project is still stabilizing its public license,
distribution, and API policy.
The current distribution policy is documented in `docs/distribution-policy.md`.

## Current Packages

- `mneme-core`: personal-memory engine and local JSON store abstractions.
- `mneme-cli`: local CLI binary exposed as `mneme`.
- `mneme-eval`: scenario replay harness and quality gates.

All package manifests include a public repository URL, a description, a README
reference, and the shared workspace version.

The current Rust API policy is documented in `docs/api-contract.md`. API docs
must build with warnings denied before release:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Local Package Check

Run:

```sh
./scripts/distribution-policy-check.sh
./scripts/package-check.sh
```

The distribution policy check verifies that crates remain unpublished while no
license file exists. The package check runs that guard before assembling
`mneme-core` and listing each workspace package's included files. `mneme-cli`
and `mneme-eval` depend on the unpublished workspace-local `mneme-core` crate,
so their file-list checks are the useful pre-publication signal until registry
publication is explicitly enabled.

The script blocks known private or generated paths such as local stores, eval
reports, private planning files, and local harness/template copies.

Cargo will warn that the manifests have no `license` or `license-file`. That
warning is expected until the project owner chooses a public license; do not add
a license field just to silence the warning.

The full quality gate also runs this check:

```sh
./scripts/quality-gate.sh full
```

## Local Install Check

The local CLI installer is intentionally separate from registry publication.
It builds from this repository and installs only the `mneme` binary:

```sh
./scripts/install-local.sh
mneme doctor
mneme init
```

The full quality gate installs into a temporary root with `--debug` and smokes
the installed binary before release. It also initializes a temporary workspace
and verifies the generated agent hook profile through `scripts/mneme-agent-hook.sh`.

## Publication Policy

Do not remove `publish = false` until all of the following are true:

- a public license has been selected and committed;
- the crate API surfaces have an explicit stability policy;
- Rustdoc verification is part of CI or the release quality gate;
- `scripts/distribution-policy-check.sh` has been updated for the selected
  license and publication target;
- package contents are reviewed against `scripts/package-check.sh`;
- `docs/release-checklist.md` includes registry publication steps;
- CI verifies the exact package or publish dry-run command intended for release.
