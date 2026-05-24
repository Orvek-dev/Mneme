# Curation Recovery & Audit Trail MVP

## Scope

Phase 25 adds an explicit rollback path for guided curation. Applied curation
already writes through the normal JSON backup path; this phase makes the backup
usable through a dedicated CLI command and eval contract.

## Requirements

- [REQ-RESTORE-001][Safety] `mneme restore --check` shall inspect current and
  backup store files without mutating either file.
- [REQ-RESTORE-002][Ubiquitous] Restore check JSON shall include `command`,
  `mode`, `ok`, `action`, current and backup status, restore availability,
  inspection details, and recommendations.
- [REQ-RESTORE-003][Recovery] `mneme restore` shall replace the current store
  with a valid `<store>.bak` even when the current store is also valid.
- [REQ-RESTORE-004][Recovery] `mneme restore` shall preserve the pre-restore
  current file as the new backup so a second restore can swap back.
- [REQ-RESTORE-005][Safety] Restore shall fail without writing when no valid
  backup is available.
- [REQ-RESTORE-006][Review] Applied curation reports shall include restore
  follow-up commands when the store changed.
- [REQ-RESTORE-007][Eval] The eval harness shall support explicit restore
  actions and expected `store.restored` checks.
- [REQ-RESTORE-008][Release] The release quality gate shall smoke restore help,
  readiness, rollback after curation, and swap-back recovery.
- [REQ-RESTORE-009][Docs] Public docs shall describe when to use repair versus
  restore.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-RESTORE-001 | `parse_restore_args`, `run_restore`, and quality gate | verified |
| REQ-RESTORE-002 | `RestoreCliReport` and CLI restore test | verified |
| REQ-RESTORE-003 | `JsonFileStore::restore_from_backup` and core test | verified |
| REQ-RESTORE-004 | Core and CLI swap-back assertions | verified |
| REQ-RESTORE-005 | Restore availability checks and error path | verified |
| REQ-RESTORE-006 | Curation apply report test and quality gate | verified |
| REQ-RESTORE-007 | `maintenance.restore_from_backup`, `store.restored`, and restore scenario | verified |
| REQ-RESTORE-008 | `scripts/quality-gate.sh` restore smoke checks | verified |
| REQ-RESTORE-009 | README, local CLI, personal runtime, stability, and API docs | verified |
