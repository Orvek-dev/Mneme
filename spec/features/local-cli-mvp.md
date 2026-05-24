# Local CLI MVP Spec

## Scope

The local CLI provides the first human-usable interface for Mneme v1 without
introducing a server, UI, or model dependency. It is a thin adapter over
`mneme-core` and uses the JSON file store.

## Authority

- The CLI must call `mneme-core`; it must not reimplement memory behavior.
- The default store must be local and ignored by git.
- CLI smoke tests must prove persistence across separate invocations.

## Requirements

- [REQ-CLI-001][Ports-and-adapters] The workspace shall expose a `mneme` binary
  through the `mneme-cli` package.
- [REQ-CLI-002][Event-driven] The CLI shall support `remember`, `correct`, and
  `forget` commands by appending v1 events.
- [REQ-CLI-003][Event-driven] The CLI shall support `context` and `snapshot`
  read commands over persisted state.
- [REQ-CLI-004][Ports-and-adapters] The CLI shall support `--store <path>` for
  isolated local state.
- [REQ-CLI-005][Ubiquitous] The CLI shall support JSON output for machine
  checks.
- [REQ-CLI-006][Ubiquitous] CI shall run a CLI smoke check against a temporary
  store.
- [REQ-CLI-007][Ubiquitous] The CLI shall expose claim review and claim-ID
  lifecycle controls for precise user edits.
- [REQ-CLI-008][Ubiquitous] The CLI shall export memory review artifacts for
  human or scripted inspection.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-CLI-001 | `cargo run -p mneme-cli -- doctor` | verified |
| REQ-CLI-002 | `mneme-cli` unit tests and CI smoke command | verified |
| REQ-CLI-003 | `mneme-cli` unit tests and CI smoke command | verified |
| REQ-CLI-004 | `--store` unit tests and docs | verified |
| REQ-CLI-005 | JSON output unit tests | verified |
| REQ-CLI-006 | `.github/workflows/ci.yml` CLI smoke step | verified |
| REQ-CLI-007 | `claims_review_and_id_lifecycle_controls` and quality gate | verified |
| REQ-CLI-008 | `review_exports_markdown_and_json_artifacts` and quality gate | verified |
