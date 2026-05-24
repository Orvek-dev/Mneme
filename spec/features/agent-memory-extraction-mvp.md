# Phase 27: Agent Memory Extraction Integration MVP

## Intent

Agent session-end memories should use the same extraction boundary as normal
event ingestion. The default path remains deterministic and rule-based, while
opt-in command extraction lets `end` and `hook end` persist durable memories
from natural-language `--remember` notes.

## Requirements

- [REQ-AGENT-EXT-001][Ports-and-adapters] `mneme-core` shall expose a
  session-end API that can record remembered notes through any `MnemeExtractor`.
- [REQ-AGENT-EXT-002][Compatibility] Existing `end_session` behavior shall
  continue to treat `remember` values as explicit v1 claims through the
  rule-based extractor.
- [REQ-AGENT-EXT-003][CLI] `mneme end` and `mneme hook end` shall accept
  `--extractor rule|command`, `--extractor-command <program>`, and repeated
  `--extractor-arg <arg>` flags.
- [REQ-AGENT-EXT-004][Ubiquitous] Command-extracted session memory shall append
  an agent summary event, record produced claim IDs, and preserve source event
  citations.
- [REQ-AGENT-EXT-005][Observability] End reports and hook-end envelopes shall
  identify the extractor used for remembered notes.
- [REQ-AGENT-EXT-006][Agent-runtime] The repository hook wrapper and profile
  inspection shall support `MNEME_EXTRACTOR_COMMAND` without requiring tracked
  local secrets.
- [REQ-AGENT-EXT-007][Testability] The eval harness shall include a scenario
  proving command extraction works for agent session-end memories.
- [REQ-AGENT-EXT-008][Safety] Command-extracted session memories shall still
  pass through core budget, audit, secret-blocking, and lifecycle-owned
  persistence paths.

## Verification

| Requirement | Verification | Status |
| --- | --- | --- |
| REQ-AGENT-EXT-001 | `MnemeEngine::end_session_with_extractor` | verified |
| REQ-AGENT-EXT-002 | existing agent session tests and agent suite | verified |
| REQ-AGENT-EXT-003 | CLI unit tests for `end` and `hook end` command extraction | verified |
| REQ-AGENT-EXT-004 | `model-agent-end-command-extraction` eval scenario | verified |
| REQ-AGENT-EXT-005 | `extractor` field in end and hook-end reports | verified |
| REQ-AGENT-EXT-006 | profile parser, wrapper, and env example updates | verified |
| REQ-AGENT-EXT-007 | model suite run with deterministic command fixture | verified |
| REQ-AGENT-EXT-008 | core-owned extraction path and release quality gate | verified |
