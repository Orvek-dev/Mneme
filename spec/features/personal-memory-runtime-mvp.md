# Personal Memory Runtime MVP Spec

## Scope

Phase 3 turns Mneme v1 from a basic JSON-backed prototype into a safer local
personal-memory runtime for single-user development.

## Requirements

- [REQ-P3-RUNTIME-001][Storage] The JSON file store shall write atomically
  through a same-directory temporary file.
- [REQ-P3-RUNTIME-002][Storage] The JSON file store shall create a `.bak`
  backup before replacing an existing store.
- [REQ-P3-RUNTIME-003][Schema] Persisted state shall include schema version,
  store metadata, generation, engine version, timestamps, and migration history.
- [REQ-P3-RUNTIME-004][Validation] The runtime shall expose state validation
  for schema, duplicate IDs, required fields, source citations, budget, and
  audit target integrity.
- [REQ-P3-RUNTIME-005][Repair] The runtime shall repair an invalid current
  store from a valid backup.
- [REQ-P3-RUNTIME-006][Portability] The CLI shall support export and import
  of validated local stores.
- [REQ-P3-RUNTIME-007][Compaction] The CLI shall compact inactive claims while
  preserving active recall and citations.
- [REQ-P3-RUNTIME-008][Eval] The eval harness shall include a runtime suite for
  import/export, compaction, repair, and persisted secret blocking.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-P3-RUNTIME-001 | `JsonFileStore::save` | verified |
| REQ-P3-RUNTIME-002 | `JsonFileStore::backup_path` and store tests | verified |
| REQ-P3-RUNTIME-003 | `MnemeState` and `StateMetadata` | verified |
| REQ-P3-RUNTIME-004 | `validate_state` and `mneme validate` | verified |
| REQ-P3-RUNTIME-005 | `JsonFileStore::repair_from_backup` and `mneme repair` | verified |
| REQ-P3-RUNTIME-006 | `mneme export` and `mneme import` | verified |
| REQ-P3-RUNTIME-007 | `MnemeEngine::compact` and `mneme compact` | verified |
| REQ-P3-RUNTIME-008 | `evals/scenarios/runtime/` and quality gate | verified |
