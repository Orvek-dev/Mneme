# Agent Hook Contract MVP Spec

## Scope

This phase adds an automation-facing CLI contract for agents that call Mneme
around task execution.

## Authority

- The product runtime remains `mneme-core`.
- Existing `begin` and `end` commands keep their current CLI behavior.
- Agent automation should use `mneme hook doctor/begin/end` for stable JSON
  envelopes.
- Hook success and failure must both be machine-readable from stdout.

## Requirements

- [REQ-HOOK-001][Ubiquitous] `mneme hook begin` shall start a session using the
  same engine path as `mneme begin`.
- [REQ-HOOK-002][Ubiquitous] `mneme hook end` shall end a session using the
  same engine path as `mneme end`.
- [REQ-HOOK-008][Ubiquitous] `mneme hook doctor` shall report hook runtime
  readiness without mutating memory state.
- [REQ-HOOK-003][Ubiquitous] Hook success output shall include
  `schema_version: mneme.agent_hook.v1`, `ok`, `operation`, `store`, and
  session identifiers.
- [REQ-HOOK-004][Ubiquitous] Hook failure output shall include
  `schema_version: mneme.agent_hook.v1`, `ok: false`, `recoverable`, and an
  `error` object with kind, message, and exit code.
- [REQ-HOOK-005][Ubiquitous] Hook failures shall exit non-zero after writing the
  JSON error envelope.
- [REQ-HOOK-006][Release] Hook help, success, and failure paths shall be covered
  by CLI tests and the local quality gate.
- [REQ-HOOK-007][Release] The public docs shall define the hook envelope and
  error kinds.
- [REQ-HOOK-009][Ports-and-adapters] `mneme hook end` shall expose opt-in
  command extraction for session-end remembered notes.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-HOOK-001 | `run_agent_hook_begin` and CLI tests | verified |
| REQ-HOOK-002 | `run_agent_hook_end` and CLI tests | verified |
| REQ-HOOK-003 | `AgentHookBeginReport` and `AgentHookEndReport` | verified |
| REQ-HOOK-004 | `AgentHookErrorReport` | verified |
| REQ-HOOK-005 | `CliError::reported` hook path | verified |
| REQ-HOOK-006 | `scripts/quality-gate.sh` | verified |
| REQ-HOOK-007 | `docs/agent-hook-contract.md` | verified |
| REQ-HOOK-008 | `run_agent_hook_doctor` and CLI tests | verified |
| REQ-HOOK-009 | `hook_end_accepts_command_extractor` CLI test | verified |
