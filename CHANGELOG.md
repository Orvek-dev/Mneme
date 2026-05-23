# Changelog

This project follows the spirit of Keep a Changelog.

## Unreleased

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

## [0.0.1] - 2026-05-24

### Added

- Initialized the public Mneme repository scaffold.
- Added a Rust workspace with `mneme-core` and `mneme-eval`.
- Added CI for format, Clippy, tests, and the eval harness doctor command.
- Added the initial public constitution and Eval Harness v0 spec.
- Added public-safe ignore rules for local harness files, private notes,
  generated eval reports, build output, and secrets.
