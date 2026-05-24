# API Rustdoc Contract MVP Spec

## Scope

This phase makes Mneme's current Rust API surface discoverable and verifies API
documentation as part of the release gate.

## Authority

- `mneme-core` is the primary product API crate.
- `mneme-cli` and `mneme-eval` expose CLI-bound library entry points.
- Rustdoc must build with warnings denied before release.
- Public API examples must compile in the normal workspace test path.

## Requirements

- [REQ-API-DOC-001][Ubiquitous] Crate-level Rustdoc shall describe each public
  crate's intended role.
- [REQ-API-DOC-002][Ports-and-adapters] `mneme-core` Rustdoc shall identify
  `MnemeEngine`, `MnemeStore`, and `MnemeExtractor` as the current runtime and
  extension boundaries.
- [REQ-API-DOC-003][Ubiquitous] The repository shall document the current API
  contract and pre-1.0 stability policy.
- [REQ-API-DOC-004][Release] The local quality gate shall build workspace
  Rustdoc with warnings denied.
- [REQ-API-DOC-005][Ubiquitous] The repository shall include a compile-checked
  `mneme-core` personal-memory example.
- [REQ-API-DOC-006][Release] Public onboarding and package-readiness docs shall
  point developers to API docs and the Rustdoc verification command.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-API-DOC-001 | crate-level Rustdoc in workspace crates | verified |
| REQ-API-DOC-002 | `crates/mneme-core/src/lib.rs` | verified |
| REQ-API-DOC-003 | `docs/api-contract.md` and `docs/v1-stability.md` | verified |
| REQ-API-DOC-004 | `scripts/quality-gate.sh` | verified |
| REQ-API-DOC-005 | `crates/mneme-core/examples/personal_memory.rs` | verified |
| REQ-API-DOC-006 | README and package-readiness docs | verified |
