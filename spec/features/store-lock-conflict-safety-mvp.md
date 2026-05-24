# Store Lock Conflict Safety MVP Spec

## Scope

This phase rejects concurrent local JSON store writers before they can race a
save, repair, or agent hook write.

## Authority

- `JsonFileStore` remains the local single-user persistence adapter.
- Atomic write and backup behavior remain unchanged after a lock is acquired.
- Lock conflicts must be surfaced as stable, machine-readable errors.
- Agents should treat lock conflicts as recoverable and may retry later.

## Requirements

- [REQ-LOCK-001][Storage] `JsonFileStore::save` shall acquire an exclusive
  `<store>.lock` before writing backup or current state files.
- [REQ-LOCK-002][Storage] `JsonFileStore::repair_from_backup` shall acquire the
  same exclusive lock before restoring the current store.
- [REQ-LOCK-003][Storage] Lock acquisition shall fail when the lock file already
  exists and shall not modify the store.
- [REQ-LOCK-004][Ubiquitous] Lock conflicts shall use stable
  `StoreErrorKind::LockConflict` and string identifier `store_lock`.
- [REQ-LOCK-005][Ubiquitous] Hook lock failures shall emit
  `error.kind: store_lock` and `recoverable: true`.
- [REQ-LOCK-006][Release] Lock behavior shall be covered by core and CLI tests.
- [REQ-LOCK-007][Release] The local quality gate shall smoke-test a locked hook
  failure path.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-LOCK-001 | `JsonFileStore::save` lock guard | verified |
| REQ-LOCK-002 | `JsonFileStore::repair_from_backup` lock guard | verified |
| REQ-LOCK-003 | `json_file_store_save_requires_exclusive_lock` | verified |
| REQ-LOCK-004 | `StoreErrorKind` | verified |
| REQ-LOCK-005 | hook lock CLI test | verified |
| REQ-LOCK-006 | core and CLI tests | verified |
| REQ-LOCK-007 | `scripts/quality-gate.sh` | verified |
