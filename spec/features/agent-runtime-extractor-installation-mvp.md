# Phase 28: Agent Runtime Extractor Installation MVP

## Intent

Phase 27 made agent session-end memory extraction possible. Phase 28 makes it
installable: a local user can generate an ignored agent hook profile that
already contains the command extractor used by wrapper `end` calls.

## Requirements

- [REQ-AGENT-INSTALL-001][Agent-runtime] `mneme init` shall accept
  `--extractor-command <program>`.
- [REQ-AGENT-INSTALL-002][Agent-runtime] Generated hook profiles shall write an
  active `MNEME_EXTRACTOR_COMMAND=<program>` line when the init option is set.
- [REQ-AGENT-INSTALL-003][Compatibility] Generated profiles shall keep the
  extractor command commented out when no extractor command is requested.
- [REQ-AGENT-INSTALL-004][Observability] Init JSON/plain reports and
  `mneme doctor` output shall expose configured extractor commands.
- [REQ-AGENT-INSTALL-005][Safety] Extractor command values shall be validated
  as single-line profile values and must remain outside tracked local secrets.
- [REQ-AGENT-INSTALL-006][Release] The quality gate shall verify that
  `scripts/mneme-agent-hook.sh end` can use only the generated profile to run
  command-extracted session-end memory.

## Verification

| Requirement | Verification | Status |
| --- | --- | --- |
| REQ-AGENT-INSTALL-001 | `parse_init_args` and init help | verified |
| REQ-AGENT-INSTALL-002 | `render_agent_hook_profile` and init CLI test | verified |
| REQ-AGENT-INSTALL-003 | existing init profile test | verified |
| REQ-AGENT-INSTALL-004 | init/doctor CLI tests | verified |
| REQ-AGENT-INSTALL-005 | `single_line_value` profile rendering path | verified |
| REQ-AGENT-INSTALL-006 | `scripts/quality-gate.sh full` wrapper-profile smoke | verified |
