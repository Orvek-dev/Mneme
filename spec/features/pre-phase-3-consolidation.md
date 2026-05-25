# Pre-Phase-3 Consolidation Spec

## Scope

This phase consolidates development operations before Phase 3 so future work can
ship in larger, lower-overhead phases.

## Requirements

- [REQ-P3-PREP-001][Release] A single local quality gate shall cover format,
  lint, tests, CLI smoke checks, eval suites, dry-run baseline, and public
  safety checks.
- [REQ-P3-PREP-002][Privacy] A public safety script shall reject known private
  paths, internal doc names, real-looking API key patterns, and tracked secret
  files.
- [REQ-P3-PREP-003][Cost] CI shall avoid duplicate full runs for feature branch
  pushes and pull requests.
- [REQ-P3-PREP-004][Release] Release verification shall reuse the local quality
  gate.
- [REQ-P3-PREP-005][Operations] Live provider baseline execution shall be
  available as a local helper script that writes ignored reports.
- [REQ-P3-PREP-006][Planning] Phase 3 readiness and next large phase direction
  shall be documented publicly.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-P3-PREP-001 | `scripts/quality-gate.sh` | verified |
| REQ-P3-PREP-002 | `scripts/public-safety-check.sh` | verified |
| REQ-P3-PREP-003 | `.github/workflows/ci.yml` | verified |
| REQ-P3-PREP-004 | `.github/workflows/release.yml` | verified |
| REQ-P3-PREP-005 | `scripts/live-baseline.sh` | verified |
| REQ-P3-PREP-006 | `docs/project/phase-3-readiness.md` | verified |
