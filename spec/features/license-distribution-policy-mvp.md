# License Distribution Policy MVP Spec

## Scope

This phase makes Mneme's current public distribution state explicit and
machine-checked after the owner selects the MIT License.

## Authority

- The project owner has selected the MIT License.
- Registry publication must remain disabled until explicitly prepared.
- Package manifests must match the committed license in the repo.
- Release checks must catch accidental publication guard removal.

## Requirements

- [REQ-DIST-001][Ubiquitous] The repository shall document the current license
  status and registry publication status.
- [REQ-DIST-002][Privacy] The repository shall keep crates unpublished with
  `publish = false` while registry publication is disabled.
- [REQ-DIST-003][Release] Package checks shall run a distribution policy guard
  before assembling package contents.
- [REQ-DIST-004][Ubiquitous] Public docs shall explain the selected license and
  list the release work required before registry publication.
- [REQ-DIST-005][Release] Release and package readiness docs shall include the
  distribution policy check.
- [REQ-DIST-006][Release] The policy guard shall fail if license metadata and
  the committed license file disagree.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-DIST-001 | `docs/project/distribution-policy.md` | verified |
| REQ-DIST-002 | crate `Cargo.toml` files and policy check | verified |
| REQ-DIST-003 | `scripts/package-check.sh` | verified |
| REQ-DIST-004 | `docs/project/distribution-policy.md` | verified |
| REQ-DIST-005 | package readiness and release checklist docs | verified |
| REQ-DIST-006 | `scripts/distribution-policy-check.sh` | verified |
