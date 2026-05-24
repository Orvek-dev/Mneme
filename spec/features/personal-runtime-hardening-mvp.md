# Personal Runtime Hardening MVP

## Intent

Long-lived personal stores need a predictable maintenance path before Mneme
moves into more memory-quality features. Users should be able to inspect repair
readiness without mutation, recover from a corrupt current store when a backup
is valid, and normalize compatible older schema metadata safely.

## Requirements

- [REQ-HARDEN-001][Safety] `mneme repair --check` shall inspect current and
  backup store files without mutating either file.
- [REQ-HARDEN-002][Ubiquitous] Repair check JSON shall include `command`,
  `mode`, `ok`, `action`, current and backup status, repair availability,
  inspection details, and recommendations.
- [REQ-HARDEN-003][Safety] Repair check shall report `repair_available` when
  the current store is invalid and the backup is valid.
- [REQ-HARDEN-004][Migration] Repair check shall report
  `normalization_available` when the current store is valid but carries
  legacy-compatible schema or metadata warnings.
- [REQ-HARDEN-005][Migration] `mneme repair` shall normalize a valid
  legacy-compatible current store through the atomic write path and preserve
  the pre-normalized file as `<store>.bak`.
- [REQ-HARDEN-006][Safety] `mneme repair` shall continue to restore an invalid
  current store from a valid backup and fail when no valid backup is available.
- [REQ-HARDEN-007][Release] The quality gate shall verify repair check reports
  for valid stores and repairable corrupted stores using both cargo-run and
  installed-binary paths.
- [REQ-HARDEN-008][Docs] Public docs shall describe when to use repair check,
  backup repair, and legacy schema normalization.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-HARDEN-001 | `parse_repair_args`, `run_repair`, and CLI tests | verified |
| REQ-HARDEN-002 | `RepairCliReport` and `repair_command_restores_corrupted_store_from_backup` | verified |
| REQ-HARDEN-003 | CLI test and quality-gate repairable-store smoke | verified |
| REQ-HARDEN-004 | `repair_check_action` and `store_file_needs_normalization` | verified |
| REQ-HARDEN-005 | `repair_normalizes_current_legacy_schema_store` | verified |
| REQ-HARDEN-006 | `JsonFileStore::repair_from_backup` and existing repair tests | verified |
| REQ-HARDEN-007 | `scripts/quality-gate.sh` repair check smoke | verified |
| REQ-HARDEN-008 | README, local CLI, personal runtime, and stability docs | verified |
