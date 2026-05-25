# Runtime Config Install Profile MVP

## Intent

Agent runtime installation should not require repeating the same store, agent,
scope, and context-budget flags on every hook call. This phase adds a
public-safe profile format and wrapper loading rules while keeping local config
ignored by default.

## Requirements

- [REQ-PROFILE-001][Ubiquitous] `scripts/mneme-agent-hook.sh` shall load a
  runtime profile from `MNEME_AGENT_HOOK_CONFIG`, `MNEME_CONFIG`, or the default
  `.mneme/mneme-agent-hook.env` path.
- [REQ-PROFILE-002][Safety] The wrapper shall parse `KEY=VALUE` profile lines
  directly and shall not execute the profile file.
- [REQ-PROFILE-003][Ubiquitous] Profile-supported keys shall include
  `MNEME_BIN`, `MNEME_STORE`, `MNEME_AGENT_ID`, `MNEME_SCOPE`, and
  `MNEME_MAX_ITEMS`.
- [REQ-PROFILE-004][Ubiquitous] Runtime precedence shall be explicit CLI flags,
  then environment variables, then profile values, then command defaults.
- [REQ-PROFILE-005][Release] The repository shall include a public-safe profile
  example that does not expose private local paths or secrets.
- [REQ-PROFILE-006][Release] The quality gate shall smoke-test config-driven
  wrapper doctor, begin, and end flows.
- [REQ-PROFILE-007][Documentation] Public docs shall describe profile paths,
  format, supported keys, and precedence.

## Verification Map

| Requirement | Verification | Status |
| --- | --- | --- |
| REQ-PROFILE-001 | wrapper implementation and quality-gate config smoke | verified |
| REQ-PROFILE-002 | direct parser in `scripts/mneme-agent-hook.sh` | verified |
| REQ-PROFILE-003 | `examples/mneme-agent-hook.env.example` | verified |
| REQ-PROFILE-004 | docs and quality-gate config smoke | verified |
| REQ-PROFILE-005 | public-safety check and profile example | verified |
| REQ-PROFILE-006 | `scripts/quality-gate.sh` | verified |
| REQ-PROFILE-007 | `docs/v1/agent-runtime-config.md` | verified |
