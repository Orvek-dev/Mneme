# CLI Help Developer UX MVP Spec

## Scope

This phase makes the local CLI and eval harness easier to use without private
context or prior knowledge of the command surface.

## Authority

- Help output is a public developer contract for local usage.
- Command help must stay consistent with the implemented CLI parser.
- Invalid command errors should guide users to discoverable usage instead of
  ending at a dead end.
- Release checks should smoke-test representative help paths.

## Requirements

- [REQ-CLI-HELP-001][Ubiquitous] `mneme help` shall list the public local CLI
  commands, common options, and representative examples.
- [REQ-CLI-HELP-002][Ubiquitous] `mneme help <command>` and
  `mneme <command> --help` shall print command-specific usage for supported
  commands.
- [REQ-CLI-HELP-003][Ubiquitous] `mneme-eval help` shall list the public eval
  harness commands, targets, and representative examples.
- [REQ-CLI-HELP-004][Ubiquitous] `mneme-eval help <command>` and
  `mneme-eval <command> --help` shall print command-specific usage for
  supported commands.
- [REQ-CLI-HELP-005][Ubiquitous] Invalid CLI command errors shall point to
  top-level and command-specific help.
- [REQ-CLI-HELP-006][Release] The local quality gate shall smoke-test
  top-level and command-specific help output for both binaries.
- [REQ-CLI-HELP-007][Ubiquitous] Public onboarding docs shall show where to
  discover command-specific usage.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-CLI-HELP-001 | `crates/mneme-cli/src/lib.rs` help tests | verified |
| REQ-CLI-HELP-002 | `crates/mneme-cli/src/lib.rs` help tests | verified |
| REQ-CLI-HELP-003 | `crates/mneme-eval/src/cli.rs` help tests | verified |
| REQ-CLI-HELP-004 | `crates/mneme-eval/src/cli.rs` help tests | verified |
| REQ-CLI-HELP-005 | CLI invalid command tests | verified |
| REQ-CLI-HELP-006 | `scripts/quality-gate.sh` | verified |
| REQ-CLI-HELP-007 | README and docs usage guidance | verified |
