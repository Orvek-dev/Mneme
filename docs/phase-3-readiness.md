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
