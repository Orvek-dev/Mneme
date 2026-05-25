# Local Install & First-Run UX MVP Spec

## Scope

This phase makes the personal CLI usable as `mneme` without requiring every
first-run command to go through `cargo run -p mneme-cli --`. It adds a local
installer and release smoke checks for the installed binary.

## Authority

- Local install must build from the repository. It is not registry
  publication.
- Crates remain `publish = false` until the distribution policy changes.
- Install smoke checks must use an isolated store and avoid writing private
  runtime files into the repository.

## Requirements

- [REQ-INSTALL-001][Ubiquitous] The repository shall provide
  `scripts/install-local.sh`.
- [REQ-INSTALL-002][Ports-and-adapters] The installer shall install the
  `mneme-cli` package as the `mneme` binary through `cargo install --path`.
- [REQ-INSTALL-003][Ubiquitous] The installer shall support a custom install
  root for temporary and CI smoke installs.
- [REQ-INSTALL-004][Release] The installer shall smoke-test the installed
  binary with doctor/help/review commands by default.
- [REQ-INSTALL-005][Release] The quality gate shall run a temporary installed
  binary through remember/context/review first-run commands.
- [REQ-INSTALL-006][Release] Getting started docs shall prefer installed
  `mneme` commands for local CLI workflows.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-INSTALL-001 | `scripts/install-local.sh` | verified |
| REQ-INSTALL-002 | `scripts/install-local.sh` | verified |
| REQ-INSTALL-003 | `scripts/install-local.sh --root` and quality gate | verified |
| REQ-INSTALL-004 | installer smoke block | verified |
| REQ-INSTALL-005 | `scripts/quality-gate.sh` install smoke | verified |
| REQ-INSTALL-006 | `docs/v1/getting-started.md` and `README.md` | verified |
