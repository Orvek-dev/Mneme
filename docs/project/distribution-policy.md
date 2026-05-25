# Distribution Policy

```text
license_status: pending-owner-decision
registry_publication: disabled
```

Mneme is public for inspection, local development, and evaluation. It is not
yet published as an open source package or registry crate because the project
owner has not selected and committed a public license.

This document is a project policy, not legal advice. Do not represent Mneme as
open source, publish the crates to a registry, or remove package publication
guards until the owner intentionally completes the license decision.

## Current Policy

- No `LICENSE` or `LICENSE.md` file is committed.
- Workspace crates must keep `publish = false`.
- Cargo manifests must not declare `license` or `license-file` until the
  matching license file is committed.
- GitHub `v0.x` releases are prerelease source snapshots for development
  tracking, not registry publication events.
- Public package checks may assemble package contents locally, but they must
  not publish anything.

## Owner Decision Needed

Before enabling public redistribution or registry publication, the owner should
choose and commit a license strategy. Common paths are:

- permissive, minimal license;
- permissive license with an explicit patent grant;
- dual permissive license;
- copyleft license for intentionally reciprocal distribution.

The project should not pick one implicitly. The selected license must be
committed as `LICENSE` or `LICENSE.md`, and package metadata should be updated
in the same PR.

## Publication Readiness

Do not remove `publish = false` until all of the following are true:

- a license file is committed;
- every package manifest declares matching license metadata;
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

The distribution policy check currently enforces the pending-license state:
`publish = false` must remain in all package manifests and registry
publication must stay disabled.
