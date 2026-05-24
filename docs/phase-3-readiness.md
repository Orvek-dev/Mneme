# Phase 3 Readiness

This document freezes the Phase 3 entry point so future work can move in larger
implementation phases instead of small feature slices.

## Phase 3 Entry Criteria

Phase 3 can start when the following are true:

- `mneme-core` has deterministic v1 memory behavior behind eval gates.
- `mneme-cli` can exercise local personal-memory workflows over a JSON store.
- `mneme-eval` has core, model, acceptance, and repeated baseline commands.
- Model-backed extraction remains opt-in through the command adapter.
- OpenAI provider usage is isolated in `wrappers/openai_extractor.py`.
- Public CI uses dry-run provider checks only.
- Live provider baselines are local-only and write ignored reports.
- One local quality gate covers format, lint, tests, evals, baseline, and
  public safety checks.
- GitHub Actions run full CI only for PRs and `main` pushes.

## Pre-Phase-3 Feature List

Completed before Phase 3:

- Eval Harness v0 with scenario validation, seeded faults, and reports.
- `mneme-v1` personal core with events, claims, budget, lifecycle, context,
  audit, and JSON persistence.
- Local CLI for remember, correct, forget, context, snapshot, and command
  ingestion.
- Provider-neutral command extractor protocol.
- Opt-in model suite and command-backed eval target.
- OpenAI wrapper example with deterministic dry-run mode.
- Repeated baseline reports with aggregate, category, and per-scenario pass
  rates.
- Live provider baseline runbook and redaction checklist.
- Public safety and quality-gate scripts.
- Cost-aware CI policy: no full branch-push CI for phase branches.

## Phase 3 Direction

Phase 3 should be a product MVP phase, not more harness scaffolding. The next
large implementation phase should focus on one of these product surfaces:

1. **Personal Memory Runtime MVP**
   Add import/export, compaction, migration metadata, and safer local storage
   semantics for real personal use.

2. **Agent Integration MVP**
   Provide a stable local API/CLI contract that agents can call before and after
   tasks, with eval coverage for end-to-end agent memory flows.

3. **Live Provider Quality MVP**
   Run the first local live baseline, inspect category failures, and improve the
   provider wrapper prompt or parsing as one grouped phase.

The recommended next phase is **Personal Memory Runtime MVP**, because Phase 2
already made the model/eval side strong enough to guard provider experiments.

## Phase 3 Execution

Phase 3 shipped the **Personal Memory Runtime MVP** in `v0.9.0`. The next large
phase should build on the new runtime maintenance surface instead of adding more
storage scaffolding.

Phase 4 shipped the **Agent Integration MVP** in `v0.10.0`, adding begin/end
sessions, session audit, and an agent eval suite.

Phase 5 shipped the **Live Provider Quality Gate MVP** in `v0.11.0`, adding a
strict baseline-gate command and failure summaries for local provider quality
analysis.

Phase 6 shipped the **Public Package & Onboarding MVP** in `v0.12.0`, adding
package metadata, package assembly checks, and public getting-started guidance
while keeping crates unpublished with `publish = false`.

Phase 7 shipped the **CLI Help & Developer UX MVP** in `v0.13.0`, adding
top-level and command-specific help for `mneme` and `mneme-eval`, plus help
smoke checks in the quality gate.

Phase 8 shipped the **API/Rustdoc Contract MVP** in `v0.14.0`, documenting the
current Rust API surface, adding a compile-checked `mneme-core` example, and
building Rustdoc with warnings denied in the quality gate.

Phase 9 shipped the **License & Distribution Policy MVP** in `v0.15.0`,
documenting the pending owner license decision, keeping registry publication
disabled, and adding a distribution policy check to package verification.

Phase 10 shipped the **Scope & Permission Guard MVP** in `v0.16.0`, adding
scoped `ContextQuery` retrieval, CLI allowed-scope options for `context` and
`begin`, and eval/quality-gate checks for denied and allowed retrieval paths.

Phase 11 shipped the **Deterministic Context Ranking & Budget MVP** in
`v0.17.0`, adding ranked context item metadata, retrieval item caps, budget
omission reasons, and eval/quality-gate checks for ranked capped retrieval.

Phase 12 shipped the **Agent Hook Contract MVP** in `v0.18.0`, adding
`mneme hook begin/end`, the `mneme.agent_hook.v1` JSON envelope, hook error
classification, and quality-gate checks for hook success and failure paths.

Phase 13 shipped the **Local Store Lock & Conflict Safety MVP** in `v0.19.0`,
adding exclusive JSON store lock files, stable `store_lock` error
classification, and hook/quality-gate checks for recoverable lock conflicts.

Phase 14 shipped the **Memory Review & Policy Controls MVP** in `v0.20.0`,
adding `mneme claims`, claim-ID based forget/correct controls, and eval plus
quality-gate checks that duplicate claim text is only updated by selected ID.

Phase 15 shipped the **Agent Runtime Installation MVP** in `v0.21.0`, adding
`mneme hook doctor`, the `scripts/mneme-agent-hook.sh` runtime wrapper, and
quality-gate checks for wrapper doctor/begin/end installation smoke.

Phase 16 shipped the **Runtime Config & Install Profile MVP** in `v0.22.0`,
adding wrapper runtime profile loading, a public-safe profile example, and
quality-gate checks for config-driven wrapper doctor/begin/end flows.

Phase 17 shipped the **Memory Review Artifact MVP** in `v0.23.0`, adding
`mneme review`, Markdown and JSON review artifact export, and quality-gate
checks for persisted human-review output.

Phase 18 shipped the **Safe Review Redaction MVP** in `v0.24.0`, adding default
redaction for review artifacts, explicit `--include-sensitive` raw export, and
quality-gate checks that safe artifacts do not expose secret-like claim text.

Phase 19 shipped the **Local Install & First-Run UX MVP** in `v0.25.0`, adding
`scripts/install-local.sh`, installed-binary first-run smoke checks, and docs
centered on `mneme` as the local CLI entry point.

Phase 20 shipped the **First-Run Bootstrap & Installed Agent Hook MVP** in
`v0.26.0`, adding `mneme init`, generated local store/profile bootstrap, and
quality-gate checks that an installed `mneme` binary can initialize a temporary
workspace and drive the agent hook wrapper through that profile.

Phase 21 shipped the **Workspace Health & Bootstrap Stabilization MVP** in
`v0.27.0`, adding `mneme doctor --json`, richer plain doctor output, profile
validation, and quality-gate checks for pre-init, post-init, invalid-profile,
and invalid-store workspace health reports.

Phase 22 shipped the **Personal Runtime Hardening MVP** in `v0.28.0`, adding
`mneme repair --check`, repair JSON reports with mode/action/health status,
legacy-compatible schema normalization through `mneme repair`, and
quality-gate checks for valid and repairable store lifecycle paths.

Phase 23 shipped the **Memory Quality & Review Loop MVP** in `v0.29.0`, adding
`mneme quality`, quality findings inside review artifacts, eval checks for
duplicate/blocked/inactive review queues, and quality-gate checks that safe
quality reports do not leak blocked-secret text.

Phase 24 shipped the **Guided Memory Curation MVP** in `v0.30.0`, adding
`mneme curate`, dry-run cleanup plans, explicit duplicate cleanup and
compaction, curation before/after eval checks, and quality-gate checks for
redaction, backup creation, and post-curation quality health.

Phase 25 shipped the **Curation Recovery & Audit Trail MVP** in `v0.31.0`,
adding `mneme restore`, backup rollback readiness reports, curation rollback
commands, restore eval checks, and quality-gate checks for rollback and
swap-back recovery.

Phase 26 shipped the **Provider Extraction Quality MVP** in `v0.32.0`, expanding
the model suite to cover communication, format, project-scoped, quoted-sample,
and answer-local instruction cases, and strengthening the OpenAI wrapper
dry-run and post-processing guardrails.

Phase 27 shipped the **Agent Memory Extraction Integration MVP** in `v0.33.0`,
connecting `mneme end` and `mneme hook end` to the opt-in command extractor so
session-end `--remember` notes can be evaluated and released through the same
provider-neutral boundary as event ingestion.

Phase 28 shipped the **Agent Runtime Extractor Installation MVP** in `v0.34.0`,
adding `mneme init --extractor-command`, doctor/init visibility for installed
extractor commands, and quality-gate coverage proving the hook wrapper can use
only the generated profile for command-extracted session-end memory.

Phase 29 shipped the **Agent Runtime Diagnostics & Cost Guardrails MVP** in
`v0.35.0`, adding wrapper doctor runtime diagnostics and an explicit
`--check-extractor` flag so routine diagnostics never run provider-backed
extractors by default.
