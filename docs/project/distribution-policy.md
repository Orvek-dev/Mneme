# Distribution Policy

```text
license_status: MIT
registry_publication: disabled
```

Mneme is distributed under the MIT License for source use, local development,
and evaluation. It is not yet published as a registry crate.

This document is a project policy, not legal advice. Do not publish the crates
to a registry or remove package publication guards until the owner
intentionally completes the registry publication decision.

## Current Policy

- `LICENSE` is committed with the MIT License.
- Workspace package metadata declares `license = "MIT"`.
- Crate package metadata inherits the workspace license.
- Workspace crates must keep `publish = false`.
- GitHub `v0.x` releases are prerelease source snapshots for development
  tracking, not registry publication events.
- Public package checks may assemble package contents locally, but they must
  not publish anything.

## Publication Readiness

Do not remove `publish = false` until all of the following are true:

- `docs/project/package-readiness.md` and `docs/project/release-checklist.md` describe the
  intended registry publication path;
- `scripts/distribution-policy-check.sh` has been updated for the selected
  license and publication target;
- `scripts/package-check.sh` verifies the exact package contents intended for
  publication;
- release CI verifies the intended package or publish dry-run command;
- `CHANGELOG.md` records the policy change.

## Verification

Run:

```sh
./scripts/distribution-policy-check.sh
./scripts/package-check.sh
./scripts/quality-gate.sh full
```

The distribution policy check currently enforces the MIT source-distribution
state: `LICENSE` must be MIT, package metadata must declare the MIT license,
`publish = false` must remain in all package manifests, and registry
publication must stay disabled.
