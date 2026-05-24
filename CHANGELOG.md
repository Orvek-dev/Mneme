# Changelog

This project follows the spirit of Keep a Changelog.

## Unreleased

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
