# Phase 29: Agent Runtime Diagnostics & Cost Guardrails MVP

## Summary

Phase 28 made command extractors installable through generated agent runtime
profiles. Phase 29 makes wrapper diagnostics explicit and keeps provider-backed
extractor smoke checks opt-in so routine doctor runs do not spend API or network
budget.

## Requirements

- [REQ-RUNTIME-DIAG-001][Ubiquitous] `scripts/mneme-agent-hook.sh doctor` shall
  print the loaded profile path, whether a profile was loaded, the selected
  `mneme` source, configured store, agent, scope, max item cap, and configured
  extractor command.
- [REQ-RUNTIME-DIAG-002][Safety] The default wrapper doctor command shall run
  only the existing isolated hook doctor/begin/end smoke path and shall not run
  the configured command extractor.
- [REQ-RUNTIME-DIAG-003][Safety] Wrapper command-extractor smoke checks shall
  require an explicit `--check-extractor` flag.
- [REQ-RUNTIME-DIAG-004][Ubiquitous] `doctor --check-extractor` shall fail fast
  when no `MNEME_EXTRACTOR_COMMAND` is configured.
- [REQ-RUNTIME-DIAG-005][Release] The release quality gate shall verify default
  no-cost doctor behavior and opt-in extractor doctor behavior.
- [REQ-RUNTIME-DIAG-006][Docs] Public docs shall explain that `--check-extractor`
  may run provider-backed commands and is not part of default doctor checks.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-RUNTIME-DIAG-001 | wrapper doctor diagnostics and quality-gate greps | verified |
| REQ-RUNTIME-DIAG-002 | default wrapper doctor smoke and skipped extractor output | verified |
| REQ-RUNTIME-DIAG-003 | `--check-extractor` parser and quality-gate opt-in smoke | verified |
| REQ-RUNTIME-DIAG-004 | `run_extractor_smoke` missing-command guard | verified |
| REQ-RUNTIME-DIAG-005 | `scripts/quality-gate.sh` wrapper doctor checks | verified |
| REQ-RUNTIME-DIAG-006 | runtime docs and stability docs | verified |
