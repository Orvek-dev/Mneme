# Package Hardening MVP Spec

## Scope

Package hardening makes the public repository usable by a new developer without
private context. It documents the current surface, stability contract, release
checks, and verification commands.

## Authority

- README is the public entry point.
- Public docs must not reference private harness files or local template paths.
- Release checks must exercise the CLI and eval harness.
- Stability claims must map back to specs, tests, evals, or CI checks.

## Requirements

- [REQ-PKG-001][Ubiquitous] The repository shall provide a README with
  quickstart, eval, development checks, and layout guidance.
- [REQ-PKG-002][Ubiquitous] The repository shall document current v1 stability
  and unstable areas.
- [REQ-PKG-003][Ubiquitous] The repository shall document release preflight and
  public-safety checks.
- [REQ-PKG-004][Ports-and-adapters] Release verification shall include local CLI
  and eval harness checks.
- [REQ-PKG-005][Ubiquitous] Public docs shall link to the CLI, eval, extraction,
  and v1 core contracts.
- [REQ-PKG-006][Ubiquitous] Workspace package manifests shall include public
  descriptions, repository metadata, README references, and an explicit
  publication policy.
- [REQ-PKG-007][Privacy] Package assembly checks shall reject known private or
  generated file patterns from package contents.
- [REQ-PKG-008][Ubiquitous] The repository shall provide a public getting
  started path for new developers without private context.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-PKG-001 | `README.md` | verified |
| REQ-PKG-002 | `docs/v1/v1-stability.md` | verified |
| REQ-PKG-003 | `docs/project/release-checklist.md` | verified |
| REQ-PKG-004 | `.github/workflows/release.yml` | verified |
| REQ-PKG-005 | README documentation links | verified |
| REQ-PKG-006 | crate `Cargo.toml` package metadata and `publish = false` | verified |
| REQ-PKG-007 | `scripts/package-check.sh` | verified |
| REQ-PKG-008 | `docs/v1/getting-started.md` | verified |
