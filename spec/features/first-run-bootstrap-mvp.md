# First-Run Bootstrap & Installed Agent Hook MVP

## Intent

After local installation, a user should be able to initialize a workspace and
wire the agent hook wrapper without hand-authoring `.mneme` runtime files.

## Requirements

- [REQ-BOOT-001][Ubiquitous] `mneme init` shall create a valid v1 JSON store at
  the default `.mneme/mneme-v1.json` path unless `--store <path>` is provided.
- [REQ-BOOT-002][Ports-and-adapters] `mneme init` shall create an agent hook
  runtime profile at `.mneme/mneme-agent-hook.env` unless `--config <path>` is
  provided.
- [REQ-BOOT-003][Ubiquitous] The generated profile shall include `MNEME_STORE`,
  `MNEME_AGENT_ID`, `MNEME_SCOPE`, `MNEME_MAX_ITEMS`, and, by default, a
  `MNEME_BIN` value.
- [REQ-BOOT-004][Safety] `mneme init` shall be idempotent and shall not
  overwrite an existing valid store or profile unless `--force` is passed.
- [REQ-BOOT-005][Release] The quality gate shall verify installed-binary
  bootstrap in a temporary workspace.
- [REQ-BOOT-006][Release] The quality gate shall verify the generated profile
  can drive `scripts/mneme-agent-hook.sh doctor/begin/end`.
- [REQ-BOOT-007][Docs] Public first-run docs shall prefer `mneme init` over
  manual profile copying.

## Verification Map

| Requirement | Evidence | Status |
| --- | --- | --- |
| REQ-BOOT-001 | `init_creates_store_and_agent_hook_profile` | verified |
| REQ-BOOT-002 | `init_creates_store_and_agent_hook_profile` | verified |
| REQ-BOOT-003 | `init_creates_store_and_agent_hook_profile` | verified |
| REQ-BOOT-004 | `init_creates_store_and_agent_hook_profile` | verified |
| REQ-BOOT-005 | `scripts/quality-gate.sh` installed workspace init smoke | verified |
| REQ-BOOT-006 | `scripts/quality-gate.sh` generated profile wrapper smoke | verified |
| REQ-BOOT-007 | README and `docs/local-install.md` | verified |
