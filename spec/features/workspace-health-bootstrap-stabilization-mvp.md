# Workspace Health & Bootstrap Stabilization MVP

## Intent

After installation and `mneme init`, users need one stable command that tells
them whether the current workspace store and agent hook profile are usable.

## Requirements

- [REQ-HEALTH-001][Ubiquitous] `mneme doctor` shall report workspace, version,
  build stage, default store, active store, backup, agent hook profile, health,
  and recommendations without mutating files.
- [REQ-HEALTH-002][Ubiquitous] `mneme doctor --json` shall emit a structured
  report with `command: doctor`, `ok`, store inspection, profile inspection,
  checks, and recommendations.
- [REQ-HEALTH-003][Safety] Doctor shall report missing pre-init store/profile
  state without exiting non-zero.
- [REQ-HEALTH-004][Safety] Doctor shall report invalid store state and repair
  availability without mutating the store.
- [REQ-HEALTH-005][Safety] Doctor shall validate agent hook profiles for
  required keys, duplicate keys, unknown keys, `MNEME_STORE` alignment,
  positive `MNEME_MAX_ITEMS`, and configured `MNEME_BIN` file existence.
- [REQ-HEALTH-006][Ports-and-adapters] Doctor shall support `--store <path>`
  and `--config <path>` so custom runtime profiles can be inspected.
- [REQ-HEALTH-007][Release] The quality gate shall verify installed-binary
  doctor reports for pre-init, post-init, invalid-profile, and invalid-store
  states.
- [REQ-HEALTH-008][Docs] Public first-run docs shall use `mneme doctor` as the
  canonical workspace health check after `mneme init`.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-HEALTH-001 | `emit_doctor_report` and CLI smoke checks | verified |
| REQ-HEALTH-002 | `doctor_reports_workspace_health_before_and_after_init` | verified |
| REQ-HEALTH-003 | `doctor_reports_workspace_health_before_and_after_init` | verified |
| REQ-HEALTH-004 | `doctor_reports_workspace_health_before_and_after_init` | verified |
| REQ-HEALTH-005 | `inspect_agent_hook_profile` and CLI test | verified |
| REQ-HEALTH-006 | `parse_doctor_args` and CLI test | verified |
| REQ-HEALTH-007 | `scripts/quality-gate.sh` installed doctor smoke | verified |
| REQ-HEALTH-008 | README and local install docs | verified |
