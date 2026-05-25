# Changelog

This project follows the spirit of Keep a Changelog.

## Unreleased

## [0.45.0] - 2026-05-25

### Added

- Extended `scripts/v1-hard-dogfood.py` to mirror hard-mode findings into
  official `mneme.eval_candidate.v1` YAML candidates and validate them with
  `mneme-eval candidate-check`.
- Added public-safe hard dogfood history entries and trend reports for
  comparing hard-mode scorecards across repeated local runs.
- Added lightweight quality-gate checks for the hard candidate bridge and
  synthetic trend report contract.
- Added Phase 39 hard candidate/trend docs and spec coverage.

## [0.44.0] - 2026-05-25

### Added

- Added `scripts/v1-hard-dogfood.py` for hard-mode v1 validation with 100
  normal records, 150 adversarial records, and 30 agent handoff workflows.
- Added a hard-mode scorecard covering recall, precision, scope leak, secret
  leak, citation coverage, handoff success, stale reuse, and agent attribution.
- Added seeded-fault detection, local candidate artifact generation, regression
  gates, and public-safe JSON/Markdown/HTML report outputs for hard dogfood.
- Added Phase 38 hard-mode dogfood docs/spec and lightweight quality-gate
  checks for the hard-mode contract, dataset shape, and seeded-fault coverage.

## [0.43.0] - 2026-05-25

### Added

- Added `scripts/v1-real-use-pilot.py` to prepare ignored local v1 pilot
  workspaces after manual dogfood preflight.
- Added structured feedback contract checks, public-safe feedback triage, and
  sanitized issue draft generation for real-use pilot findings.
- Added a public-safe feedback example and Phase 37 real-use pilot docs/spec.
- Expanded the quality gate to compile the pilot runner and verify its feedback
  contract plus example feedback without running the full pilot in CI.

## [0.42.0] - 2026-05-25

### Added

- Added `scripts/v1-manual-dogfood.py` to run local v1 manual dogfood with 100
  public-safe synthetic records and 25 workflow checks.
- Added manual dogfood evidence docs and a Phase 36 feature spec.
- Expanded the quality gate to compile the manual dogfood runner and verify the
  dataset shape without running the full local-only dogfood protocol in CI.

### Fixed

- Hardened v1 event, claim, and session ID allocation after compaction/restore
  so new records do not collide with retained source references.

## [0.41.0] - 2026-05-25

### Added

- Added `mneme-eval dogfood-summary <bundle-dir>` to triage v1 dogfood
  evidence bundles and report `ready_for_manual_dogfood`.
- Added dogfood summary checks for required readiness, eval, acceptance, and
  CLI smoke artifacts.
- Updated `scripts/v1-dogfood.sh` to write `dogfood-summary.json` into each
  bundle.
- Expanded the release quality gate to verify dogfood summary help, generated
  bundle decisions, and command reruns.
- Added the Phase 35 v1 dogfood evidence triage feature spec and docs.

## [0.40.0] - 2026-05-25

### Added

- Added `scripts/v1-dogfood.sh` to run deterministic v1 dogfood evals,
  readiness checks, and isolated CLI smoke flows into an ignored evidence
  bundle.
- Added public docs for v1 dogfood execution and evidence bundle layout.
- Expanded the release quality gate to execute the v1 dogfood script and verify
  its summary, readiness, and dogfood run reports.
- Added the Phase 34 v1 dogfood execution evidence feature spec.

## [0.39.0] - 2026-05-25

### Added

- Added a public `dogfood` eval suite for realistic v1 readiness workflows:
  preference correction, agent session memory, curation/restore, and scope
  isolation.
- Added `mneme-eval v1-readiness` to validate and replay `core`, `runtime`,
  `agent`, and `dogfood` against `mneme-v1` and report
  `ready_for_v1_dogfood`.
- Expanded the release quality gate to validate, run, and acceptance-test the
  dogfood suite and require the v1 readiness report.
- Added the Phase 33 v1 dogfood readiness feature spec and public docs.

## [0.38.0] - 2026-05-25

### Added

- Added `mneme-eval candidate-promote <candidate.yaml>` to validate and
  promote reviewed candidate scenarios into public eval suites.
- Added `mneme-eval baseline-compare <before.json> <after.json>` to compare
  aggregate, category, scenario, and failed-check baseline deltas.
- Added `--fail-on-regression` support for baseline comparison release gates.
- Expanded the quality gate to validate promoted candidate scenarios and
  seeded-fault baseline regression detection.
- Added the Phase 32 candidate promotion and regression intelligence feature
  spec.

### Changed

- Reorganized public docs into `docs/v1`, `docs/v2`, `docs/eval-harness`,
  and `docs/project` so GitHub shows the product structure directly.

## [0.37.0] - 2026-05-25

### Added

- Added `mneme-eval candidate <report.json>` to create local, sanitized
  scenario candidate artifacts from failed eval or baseline reports.
- Added `mneme-eval candidate-check <candidate.yaml|dir>` to validate candidate
  artifacts before sharing or promotion.
- Added redaction sanitization utilities for generated candidate YAML so
  test-only key strings and local user paths do not survive in artifacts.
- Ignored generated `evals/candidates/` content by default while preserving a
  tracked `.gitkeep`.
- Expanded the release quality gate to generate, sanitize, and validate
  candidate artifacts from a seeded-fault baseline.
- Added the Phase 31 dogfood scenario candidate feature spec.

## [0.36.0] - 2026-05-25

### Added

- Added `mneme-eval baseline-summary <baseline-report.json>` for local provider
  baseline triage with human and JSON output.
- Added summary fields for triage status, redaction findings, top failed
  categories, scenarios, checks, and recommended next actions.
- Updated `scripts/live-baseline.sh` to write a local summary artifact next to
  the baseline and gate reports.
- Expanded the release quality gate to verify passing dry-run and failing
  seeded-fault baseline summaries.
- Added the Phase 30 live provider baseline triage feature spec.

## [0.35.0] - 2026-05-25

### Added

- Added explicit runtime diagnostics to `scripts/mneme-agent-hook.sh doctor`,
  including profile loading status, selected `mneme` source, runtime defaults,
  and configured extractor command.
- Added `scripts/mneme-agent-hook.sh doctor --check-extractor` for opt-in
  command-extractor smoke checks.
- Added release quality-gate coverage proving default wrapper doctor runs do
  not execute configured command extractors, while `--check-extractor` does.
- Added the Phase 29 agent runtime diagnostics and cost guardrails feature
  spec.

## [0.34.0] - 2026-05-25

### Added

- Added `mneme init --extractor-command <program>` to install an active
  session-end command extractor in the generated agent hook profile.
- Added init and doctor output coverage for configured
  `MNEME_EXTRACTOR_COMMAND` values.
- Expanded the release quality gate to verify that the repository hook wrapper
  can run command-extracted `hook end` memories using only the generated
  runtime profile.
- Added the Phase 28 agent runtime extractor installation feature spec.

## [0.33.0] - 2026-05-24

### Added

- Added opt-in command extraction for `mneme end` and `mneme hook end`, allowing
  session-end `--remember` notes to flow through the same provider-neutral
  extractor contract as `ingest`.
- Added command-extractor metadata to end and hook-end reports.
- Added agent hook profile and wrapper support for `MNEME_EXTRACTOR_COMMAND`.
- Added a model-suite scenario covering agent session-end command extraction.
- Added release quality-gate coverage for command-extracted hook-end memories.
- Added the Phase 27 agent memory extraction feature spec.

## [0.32.0] - 2026-05-24

### Added

- Expanded the opt-in model eval suite from 8 to 13 scenarios covering durable
  communication preferences, negative format preferences, project-scoped
  format preferences, quoted sample no-claim handling, and answer-local
  instruction no-claim handling.
- Expanded the deterministic command extractor fixture for the new model suite
  coverage.
- Strengthened the OpenAI wrapper prompt and dry-run behavior so provider
  experiments reject transient instructions, quoted sample data, and rejected
  format/tool preferences more consistently.
- Added wrapper post-processing guardrails that suppress model claims for
  transient instructions, sample/test text, and rejected alternatives.
- Updated the release quality gate to require the expanded 13-scenario model
  baseline.
- Added the Phase 26 provider extraction quality feature spec.

## [0.31.0] - 2026-05-24

### Added

- Added `mneme restore --check` for non-mutating backup rollback readiness
  reports.
- Added `mneme restore` to swap a valid `<store>.bak` into the current store
  while preserving the pre-restore current file as the new backup.
- Added restore follow-up commands to applied curation reports so cleanup can
  be rolled back through the CLI.
- Added eval harness restore expectations and a core curation rollback
  scenario for `fake` and `mneme-v1` targets.
- Added release quality-gate checks for restore help, readiness reports,
  rollback after curation, and swap-back recovery.
- Added the Phase 25 curation recovery and audit trail feature spec.

## [0.30.0] - 2026-05-24

### Added

- Added `mneme curate` for guided memory cleanup plans with safe dry-run output
  by default.
- Added `mneme curate --apply` to forget redundant duplicate active claims by
  deterministic claim ID, and `--compact` to remove non-active records after
  explicit review.
- Added curation before/after quality checks to the eval harness and a core
  guided-memory-curation scenario.
- Added release quality-gate checks for curation help, dry-run redaction,
  applied cleanup, backup creation, and post-curation quality health.
- Added the Phase 24 guided memory curation feature spec.

## [0.29.0] - 2026-05-24

### Added

- Added `mneme quality` for read-only memory quality reports with duplicate
  active claim, blocked-secret, inactive-history, review queue, and suggested
  command output.
- Added memory quality sections to Markdown and JSON review artifacts.
- Added safe quality redaction so blocked-secret values are not exposed in
  default quality reports or review artifacts.
- Added eval harness `quality` expectations and a core memory-quality review
  loop scenario.
- Added release quality-gate checks for `mneme quality` and safe quality review
  artifacts.
- Added the Phase 23 memory quality and review loop feature spec.

## [0.28.0] - 2026-05-24

### Added

- Added `mneme repair --check` for non-mutating repair and normalization
  readiness reports.
- Expanded repair JSON/plain reports with command, mode, action, health status,
  current/backup status, repair availability, and recommendations.
- Added repair-driven normalization for compatible legacy schema metadata while
  preserving the pre-normalized store as backup.
- Added core, CLI, and quality-gate coverage for valid, repairable, and
  legacy-normalization store lifecycle paths.
- Added the Phase 22 personal runtime hardening feature spec.

## [0.27.0] - 2026-05-24

### Added

- Added `mneme doctor --json` workspace health reports covering store and
  agent hook profile status.
- Expanded plain `mneme doctor` with workspace, store, backup, profile, health,
  and recommendation output.
- Added agent hook profile validation for required keys, store alignment,
  max-item parsing, duplicate keys, unknown keys, and configured binary paths.
- Added installed-binary quality-gate checks for pre-init, post-init,
  invalid-profile, and invalid-store doctor reports.
- Added the Phase 21 workspace health and bootstrap stabilization feature spec.

## [0.26.0] - 2026-05-24

### Added

- Added `mneme init` to create a valid local v1 store and agent hook runtime
  profile for a new workspace.
- Added generated profile support for `MNEME_BIN`, `MNEME_STORE`,
  `MNEME_AGENT_ID`, `MNEME_SCOPE`, and `MNEME_MAX_ITEMS` values.
- Added installed-binary quality-gate checks for workspace bootstrap and
  generated-profile wrapper doctor/begin/end flows.
- Added first-run bootstrap documentation and the Phase 20 feature spec.

## [0.25.0] - 2026-05-24

### Added

- Added `scripts/install-local.sh` for local `mneme` CLI installation through
  `cargo install`.
- Added install smoke checks for installed `mneme doctor`, `help`, `remember`,
  `context`, and `review` flows.
- Added local install documentation and first-run guidance centered on the
  installed `mneme` command.
- Added the Phase 19 local install and first-run UX feature spec.

## [0.24.0] - 2026-05-24

### Added

- Added default safe redaction for `mneme review` artifacts and JSON stdout
  reports.
- Added `mneme review --include-sensitive` for explicit local-only raw review
  export.
- Added redaction metadata to review artifacts with policy and redacted field
  counts.
- Added CLI tests and quality-gate smoke checks that safe review artifacts do
  not include secret-like claim text.
- Added the Phase 18 safe review redaction feature spec and docs.

## [0.23.0] - 2026-05-24

### Added

- Added `mneme review` for exporting Markdown or JSON memory review artifacts.
- Added review artifacts with claim lifecycle counts, scope counts, source
  event citations, session summaries, and store metadata.
- Added CLI tests and quality-gate smoke checks for Markdown and JSON review
  artifact export.
- Added public docs and the Phase 17 memory review artifact feature spec.

## [0.22.0] - 2026-05-24

### Added

- Added agent hook runtime profile loading to `scripts/mneme-agent-hook.sh`.
- Added `MNEME_AGENT_HOOK_CONFIG` and `MNEME_CONFIG` support for wrapper
  configuration files.
- Added a public-safe `examples/mneme-agent-hook.env.example` profile template.
- Added quality-gate smoke checks for config-driven wrapper doctor/begin/end
  flows.
- Added the Phase 16 runtime config and install profile feature spec and docs.

## [0.21.0] - 2026-05-24

### Added

- Added `mneme hook doctor` for hook runtime introspection with the stable
  `mneme.agent_hook.v1` JSON envelope.
- Added `scripts/mneme-agent-hook.sh` as an agent runtime wrapper for
  `doctor`, `begin`, and `end`.
- Added wrapper environment defaults for `MNEME_BIN`, `MNEME_STORE`,
  `MNEME_AGENT_ID`, `MNEME_SCOPE`, and `MNEME_MAX_ITEMS`.
- Added quality-gate smoke checks for hook doctor and wrapper doctor/begin/end
  flows.
- Added the Phase 15 agent runtime installation feature spec and docs.

## [0.20.0] - 2026-05-24

### Added

- Added `mneme claims` for reviewing stored claims by ID, status, scope, and
  source event IDs.
- Added `mneme forget --claim-id <id>` and
  `mneme correct --claim-id <id> <new-claim>` for precise lifecycle updates.
- Added `forget-id:` and `correct-id:` lifecycle markers in the v1 core and eval
  fake target.
- Added core, CLI, eval, and quality-gate coverage for ID-based lifecycle
  controls over duplicate claim text.
- Added the Phase 14 memory review and policy-controls feature spec.

## [0.19.0] - 2026-05-24

### Added

- Added exclusive `.lock` files around `JsonFileStore` save and repair writes.
- Added `StoreErrorKind` with `store_lock` classification for lock conflicts.
- Added hook error output for recoverable `store_lock` failures.
- Added core and CLI tests plus quality-gate smoke coverage for locked-store
  conflict handling.
- Added store lock documentation and a Phase 13 feature spec.

## [0.18.0] - 2026-05-24

### Added

- Added `mneme hook begin` and `mneme hook end` for agent automation.
- Added the stable `mneme.agent_hook.v1` JSON envelope for hook success and
  failure output.
- Added hook error classification with non-zero exits and suppressed duplicate
  stderr for JSON-reported hook failures.
- Added CLI tests and quality-gate smoke checks for hook begin/end and hook
  error output.
- Added an agent hook contract document and Phase 12 feature spec.

## [0.17.0] - 2026-05-24

### Added

- Added deterministic context ranking metadata (`score`, `matched_terms`, and
  `match_reason`) to context-pack items.
- Added a default context item cap and explicit `--max-items` controls for
  `mneme context` and `mneme begin`.
- Added context budget omission reasons with
  `context_budget_exceeded:max_items=<n>`.
- Added eval harness checks for context item count and expected ranked order.
- Added a core eval scenario and quality-gate smoke checks for ranked,
  budget-capped retrieval.

## [0.16.0] - 2026-05-24

### Added

- Added scoped `ContextQuery` retrieval in `mneme-core`.
- Added default private-scope context retrieval and explicit allowed-scope
  filtering for `mneme context` and `mneme begin`.
- Added a core eval scenario covering scope-denied context and scoped agent
  begin retrieval.
- Added CLI scope smoke checks to the local quality gate.
- Added scope guard documentation and a Phase 10 feature spec.

## [0.15.0] - 2026-05-24

### Added

- Added a public distribution policy documenting the pending owner license
  decision and disabled registry publication status.
- Added `scripts/distribution-policy-check.sh` to enforce `publish = false`
  while no license file is committed.
- Added distribution policy checks to package assembly verification.
- Added release, package-readiness, README, and PR-template guidance for
  license and registry publication guardrails.

## [0.14.0] - 2026-05-24

### Added

- Added crate-level API documentation for `mneme-core`, `mneme-cli`, and
  `mneme-eval`.
- Added a public API contract document for the current pre-1.0 Rust API
  surface.
- Added a compile-checked `mneme-core` personal-memory example.
- Added Rustdoc verification with warnings denied to the local quality gate.
- Added public onboarding and package-readiness guidance for API docs.

## [0.13.0] - 2026-05-24

### Added

- Added top-level and command-specific help for `mneme`.
- Added top-level and command-specific help for `mneme-eval`.
- Added CLI help smoke checks to the local quality gate.
- Added public CLI help usage guidance to getting-started, local CLI, eval,
  and stability documentation.

### Changed

- Invalid CLI command errors now point users to `help` and command-specific
  help topics.

## [0.12.0] - 2026-05-24

### Added

- Added public getting-started and package-readiness documentation.
- Added `scripts/package-check.sh` to verify Cargo package assembly and block
  private or generated file patterns from package contents.

### Changed

- Added public package metadata to workspace crates and marked them
  `publish = false` until license and distribution policy are finalized.
- Extended the release quality gate with package assembly checks.

## [0.11.0] - 2026-05-24

### Added

- Added `mneme-eval baseline-gate` for strict provider baseline quality checks.
- Added baseline failure summaries for failed categories, failed scenarios, and
  failed check counts.
- Added live-baseline helper gate output so local provider runs produce both a
  raw baseline report and a quality gate report.
- Added Live Provider Quality Gate MVP documentation and spec.

### Changed

- Extended the local quality gate to validate the dry-run OpenAI wrapper
  baseline report with `baseline-gate`.

## [0.10.0] - 2026-05-24

### Added

- Added agent session records to v1 state and bumped the local store schema to
  version 2.
- Added `mneme begin` and `mneme end` for task-scoped context retrieval and
  post-task memory writes.
- Added session begin/end audit events and session validation coverage.
- Added the public `agent` eval suite covering begin/end recall, session
  context, remembered claims, and secret blocking from agent summaries.
- Added Agent Integration MVP documentation and spec.

### Changed

- Extended the local quality gate with CLI begin/end smoke checks and agent
  suite validation, replay, and acceptance checks.

## [0.9.0] - 2026-05-24

### Added

- Added schema metadata, generation tracking, and migration records to local
  v1 JSON stores.
- Added atomic JSON store writes, automatic `.bak` backup creation, store
  inspection, and backup repair support.
- Added `mneme validate`, `export`, `import`, `compact`, and `repair` CLI
  commands for personal runtime maintenance.
- Added the public `runtime` eval suite covering import/export, compaction,
  backup repair, and persisted secret blocking.
- Added personal runtime documentation and a Phase 3 MVP spec.

### Changed

- Extended eval scenarios with optional `maintenance` actions and `store`
  expectations.
- Extended the release quality gate with runtime suite validation, replay, and
  acceptance checks.

## [0.8.0] - 2026-05-24

### Added

- Added a single local quality gate script for format, lint, tests, evals,
  dry-run baseline, and public safety checks.
- Added public safety and live baseline helper scripts.
- Added Phase 3 readiness documentation and a pre-Phase-3 consolidation spec.

### Changed

- Reduced GitHub Actions duplication by running CI on pull requests and `main`
  pushes only, with concurrency cancellation for superseded runs.
- Reworked auto PR merge to act on successful pull-request CI runs instead of
  branch-push CI runs.
- Made release verification reuse the same local quality gate script.

## [0.7.0] - 2026-05-24

### Added

- Added opt-in baseline metadata labels for provider, model, run label, and
  live-provider status.
- Added a live provider baseline runbook and redaction checklist.
- Added CI and release checks for baseline metadata in dry-run reports.

## [0.6.0] - 2026-05-24

### Added

- Expanded the public `model` eval suite with transient-task, third-party
  attribution, output-format preference, and token secret scenarios.
- Added `category-*` model scenario tags for baseline failure analysis.
- Added baseline category pass-rate aggregation to the JSON report contract.
- Updated deterministic command and OpenAI dry-run fixtures for the expanded
  model suite.

## [0.5.0] - 2026-05-24

### Added

- Added `mneme-eval baseline` for repeated suite runs and aggregate pass-rate
  reporting.
- Added baseline report schema fields for iteration results, scenario pass
  rates, and run-level provider errors.
- Added live-provider baseline documentation for local OpenAI wrapper evals.
- Added CI and release dry-run coverage for the baseline command.

## [0.4.0] - 2026-05-24

### Added

- Added a public OpenAI command-extractor wrapper example using the Responses
  API and Structured Outputs.
- Added deterministic wrapper dry-run mode for CI and release verification
  without provider credentials.
- Added local secret prefiltering in the wrapper before provider calls.
- Added public wrapper documentation and `.env.example` placeholders.

## [0.3.0] - 2026-05-24

### Added

- Added the opt-in `mneme-v1-command` eval target for command-backed
  extraction suites.
- Added the public `model` eval suite for implicit preference, no-claim,
  secret-blocking, and correction scenarios.
- Added eval report target metadata for extractor and protocol visibility.
- Added deterministic command-extractor fixture coverage to CI and release
  verification.

## [0.2.0] - 2026-05-24

### Added

- Added a provider-neutral command extraction adapter for model-backed memory
  extraction experiments.
- Added a stable JSON stdin/stdout protocol for external extraction commands.
- Added `mneme ingest` with opt-in `--extractor command` support.
- Added public documentation for command-backed extraction wrappers.

## [0.1.1] - 2026-05-24

### Fixed

- Made the tag release workflow publish GitHub releases with the
  workflow-scoped token instead of silently skipping publication when a custom
  secret is missing.
- Marked `v0.x` tag releases as GitHub prereleases.

## [0.1.0] - 2026-05-24

### Added

- Added the first Eval Harness v0 scenario replay implementation.
- Added deterministic fake claim extraction, budget checks, context-pack
  checks, citation checks, audit checks, and seeded fault modes.
- Added initial core scenarios for explicit remember, budget hard-cap, and
  secret blocking.
- Added `mneme-eval validate` for scenario contract checks before replay.
- Added public eval scenario format documentation and invalid fixture coverage.
- Added eval target adapter boundary with explicit `--target fake` execution.
- Added report schema version and target metadata to eval JSON reports.
- Added `mneme-eval acceptance` to gate Phase 1 readiness in CI.
- Added public acceptance coverage and Phase 1 entry documentation.
- Added Mneme v1 personal-memory core with in-memory events, claims, context,
  budget, and audit state.
- Added the `mneme-v1` eval target and CI coverage for the core suite and
  acceptance gate.
- Added the v1 persistence boundary with in-memory and JSON file stores.
- Added a `restart-persistence` core scenario that verifies `mneme-v1` recall
  after file-backed reload.
- Added v1 correction and forget lifecycle handling with core eval coverage.
- Added the local `mneme` CLI for remember, correct, forget, context, and
  snapshot workflows over the JSON file store.
- Added the v1 extraction adapter boundary with a default rule-based extractor.
- Added public package documentation, v1 stability notes, and a release
  checklist.

## [0.0.1] - 2026-05-24

### Added

- Initialized the public Mneme repository scaffold.
- Added a Rust workspace with `mneme-core` and `mneme-eval`.
- Added CI for format, Clippy, tests, and the eval harness doctor command.
- Added the initial public constitution and Eval Harness v0 spec.
- Added public-safe ignore rules for local harness files, private notes,
  generated eval reports, build output, and secrets.
