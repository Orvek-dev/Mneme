# OpenAI Provider Wrapper MVP Spec

## Scope

This phase adds a public provider wrapper example without moving provider SDKs,
credentials, or prompts into `mneme-core`.

## Requirements

- [REQ-OPENAI-WRAP-001][Architecture] The wrapper shall implement the
  `mneme.extractor.command.v1` stdin/stdout protocol.
- [REQ-OPENAI-WRAP-002][Privacy] Provider credentials shall be read only from
  environment variables and no real credentials shall be tracked.
- [REQ-OPENAI-WRAP-003][Testability] The wrapper shall support a deterministic
  dry-run mode that requires no network or provider credentials.
- [REQ-OPENAI-WRAP-004][Privacy] Obvious secret-like events shall be handled
  locally before a provider request is made.
- [REQ-OPENAI-WRAP-005][Documentation] Public usage docs shall explain dry-run,
  live local use, and safety rules.
- [REQ-OPENAI-WRAP-006][Release] CI and release verification shall exercise the
  wrapper dry-run through the opt-in model eval suite.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-OPENAI-WRAP-001 | `wrappers/openai_extractor.py` protocol IO | verified |
| REQ-OPENAI-WRAP-002 | `.env.example` placeholders and env-only wrapper config | verified |
| REQ-OPENAI-WRAP-003 | `MNEME_OPENAI_DRY_RUN=1` model suite run | verified |
| REQ-OPENAI-WRAP-004 | local secret prefilter in wrapper | verified |
| REQ-OPENAI-WRAP-005 | `docs/openai-provider-wrapper.md` | verified |
| REQ-OPENAI-WRAP-006 | CI and release workflow dry-run steps | verified |
