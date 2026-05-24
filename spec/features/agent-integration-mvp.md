# Agent Integration MVP Spec

## Scope

Phase 4 gives agents a stable local begin/end workflow over the v1 personal
runtime.

## Requirements

- [REQ-P4-AGENT-001][Session] The core shall persist agent session records with
  task, agent, status, context claim IDs, summary, and memory event IDs.
- [REQ-P4-AGENT-002][Context] `begin_session` shall retrieve task-scoped
  context and record a `session.begin` audit event.
- [REQ-P4-AGENT-003][Memory] `end_session` shall close the session and write
  explicit remembered claims through normal v1 extraction and safety checks.
- [REQ-P4-AGENT-004][Audit] Ending a session shall record `session.end` audit
  evidence.
- [REQ-P4-AGENT-005][CLI] The CLI shall expose `mneme begin` and `mneme end`.
- [REQ-P4-AGENT-006][Eval] The eval harness shall include an `agent` suite for
  begin/end recall, session context, memory writes, and secret blocking.
- [REQ-P4-AGENT-007][Schema] The local store schema shall advance to version 2
  and validate session integrity.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-P4-AGENT-001 | `SessionRecord` in `mneme-core` | verified |
| REQ-P4-AGENT-002 | `MnemeEngine::begin_session` | verified |
| REQ-P4-AGENT-003 | `MnemeEngine::end_session` | verified |
| REQ-P4-AGENT-004 | `AuditKind::SessionBegin` and `SessionEnd` | verified |
| REQ-P4-AGENT-005 | `mneme begin` and `mneme end` | verified |
| REQ-P4-AGENT-006 | `evals/scenarios/agent/` and quality gate | verified |
| REQ-P4-AGENT-007 | `MNEME_STATE_SCHEMA_VERSION = 2` and `validate_state` | verified |
