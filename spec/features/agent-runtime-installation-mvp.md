# Agent Runtime Installation MVP

## Intent

Agent automation needs a small, stable installation surface around the hook
contract. This phase adds hook runtime introspection and a repository-local
wrapper that agents can call with environment-based defaults.

## Requirements

- [REQ-RUNTIME-001][Ubiquitous] `mneme hook doctor` shall emit the stable
  `mneme.agent_hook.v1` JSON envelope without mutating the store.
- [REQ-RUNTIME-002][Ubiquitous] The doctor report shall include version, build
  stage, supported operations, selected store, default store, and store
  inspection metadata.
- [REQ-RUNTIME-003][Ports-and-adapters] A repository wrapper shall expose
  `doctor`, `begin`, and `end` commands for agent runtime configuration.
- [REQ-RUNTIME-004][Ports-and-adapters] The wrapper shall support `MNEME_BIN`,
  `MNEME_STORE`, `MNEME_AGENT_ID`, `MNEME_SCOPE`, and `MNEME_MAX_ITEMS` without
  requiring agents to know cargo command details.
- [REQ-RUNTIME-005][Safety] The wrapper `doctor` command shall run an isolated
  begin/end smoke test against a temporary store.
- [REQ-RUNTIME-006][Release] The quality gate shall verify hook doctor and the
  wrapper doctor/begin/end path.
- [REQ-RUNTIME-007][Documentation] Public docs shall describe the runtime
  wrapper, environment variables, and failure handling.

## Verification Map

| Requirement | Verification | Status |
| --- | --- | --- |
| REQ-RUNTIME-001 | `hook_doctor_emits_runtime_installation_report` | verified |
| REQ-RUNTIME-002 | CLI unit test and hook contract docs | verified |
| REQ-RUNTIME-003 | `scripts/mneme-agent-hook.sh` | verified |
| REQ-RUNTIME-004 | wrapper quality-gate smoke with env defaults | verified |
| REQ-RUNTIME-005 | wrapper `doctor` smoke in quality gate | verified |
| REQ-RUNTIME-006 | `scripts/quality-gate.sh` | verified |
| REQ-RUNTIME-007 | `docs/agent-integration.md` and hook contract docs | verified |
