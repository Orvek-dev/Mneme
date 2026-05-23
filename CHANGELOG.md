# Changelog

This project follows the spirit of Keep a Changelog.

## Unreleased

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
