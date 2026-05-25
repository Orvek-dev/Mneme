# Mneme Roadmap and Repo Strategy

## Current Direction

Build Mneme in this order:

```text
spec -> eval harness v0 -> eval harness self-verification -> Mneme vertical slice -> eval verification -> next slice
```

The first implementation target is **Eval Harness v0**, not the full Mneme
runtime. Mneme should only grow feature by feature after the harness can replay
scenarios and verify core invariants.

## North Star

Mneme is a local-first memory control plane / memory gateway for individuals
and teams running multiple AI agents.

The product promise is not "more memory." The promise is:

```text
agents can remember without leaking, drifting, losing provenance, or exceeding budget
```

## Development Loop

Every meaningful slice should follow:

1. Define or update governing spec and REQ IDs.
2. Add or update eval scenarios before implementation.
3. Implement the smallest vertical slice.
4. Run deterministic evals with fake extractor / fake LLM.
5. Run real-model evals only after deterministic behavior passes.
6. Record evidence, gaps, and residual risk.
7. Move to the next slice.

## Phase 0: Eval Harness v0

Goal: make Mneme testable before Mneme becomes complex.

Status: shipped.

Required:

- Scenario format, likely YAML or JSON.
- Isolated SQLite test database reset.
- Deterministic fake extractor.
- Deterministic fake token estimator.
- Scenario replay runner.
- Expected outcome checker for:
  - raw events
  - claims
  - context packs
  - citations
  - budget ledger
  - audit events
  - omitted reasons
- JSON and Markdown reports.
- CI gate for core scenarios.

Example command shape:

```bash
mneme-eval run --suite core
mneme-eval replay evals/scenarios/same-turn-explicit-remember.yaml
mneme-eval report evals/reports/latest.json
```

Acceptance:

- P0 REQ scenario coverage is 100%.
- Fake replay is deterministic.
- Seeded critical bugs are caught.
- Failure reports point to scenario, expected result, actual result, and relevant
  DB/audit/context artifact.

## Phase 1: Personal Mneme v1

Goal: useful local-first memory gateway for one power user using multiple AI
agents.

Status: active. The current implementation is the v1 personal-memory profile.

Suggested slices:

1. SQLite event ledger and audit log.
2. `mneme init`, `status`, `doctor`.
3. Explicit remember vertical slice.
4. Same-turn recall.
5. Budget escrow gate.
6. FTS5 retrieval.
7. Cited context pack.
8. Secret detector and blocked-secret status.
9. MCP stdio server.
10. Local HTTP API.
11. Correction / forget with tombstone.
12. Basic memory doctor.
13. Codex / Claude Code / Hermes adapter spikes.

v1 success metrics:

- raw event loss under API/LLM failure: `0`
- budget hard-cap violation: `0`
- explicit remember same-turn recall: `100%`
- context pack citation coverage: `100%`
- synthetic attribution F1: `>= 0.98`
- p95 query latency at 5k memories: `< 100ms`
- startup context overhead: `< 1,500 tokens`
- secret leakage into active memory/context: `0`

## Phase 1.5: Hygiene and Control

Goal: prevent long-term memory drift and tool-switch churn.

Suggested slices:

- memory diff
- stale / duplicate / noisy memory doctor
- contradiction detection
- quarantine review CLI/TUI
- context quality scoring
- enforce mode
- import/export
- MEMORY.md bridge
- agent feedback loop

## Phase 2: Team Mneme

Goal: safe team memory control plane with personal/project/team boundaries.

Status: planned. Team/shared memory behavior is not implemented yet.

Suggested slices:

- workspace model
- user / project / team scopes
- ACL
- agent permissions
- team promotion workflow
- self-hosted sync server
- admin audit
- offboarding
- Docker deployment

v2 success metrics:

- personal memory team leakage: `0`
- team promotion audit coverage: `100%`
- ACL bypass: `0`
- team context pack citation coverage: `100%`

## Recommended Repository Strategy

The repo should expose Eval Harness as a first-class tool, but Mneme v1 and v2
should not be implemented as separate duplicated code folders.

Prefer:

```text
mneme/
  spec/
    00_constitution.md
    features/
  evals/
    scenarios/
    adversarial/
    seeded-faults/
    reports/
  packages/ or crates/
    mneme-core/
    mneme-storage-sqlite/
    mneme-policy/
    mneme-retrieval/
    mneme-context-pack/
    mneme-budget/
    mneme-audit/
    mneme-mcp/
    mneme-http/
    mneme-cli/
    mneme-eval/
  adapters/
    codex/
    claude-code/
    hermes/
  docs/
    roadmap.md
    architecture.md
    operations.md
  .github/
    workflows/
```

Why:

- `mneme-eval` can be used by outside users without copying Mneme internals.
- Personal v1 and team v2 can share core, storage, policy, audit, and retrieval
  code.
- v2 becomes an extension of scope/ACL/sync behavior, not a fork of v1.
- Release tags can describe product maturity: `v0.x` eval/core, `v1.x`
  personal, `v2.x` team.

Avoid:

```text
mneme/
  mneme-v1/
  mneme-v2/
  eval-harness/
```

This looks clear at first, but it encourages duplicated code, duplicated
bugfixes, duplicated migrations, and unclear ownership of shared contracts.

Better public positioning:

```text
Mneme repository
- Mneme Personal: v1 product profile
- Mneme Team: v2 product profile
- Mneme Eval Harness: reusable conformance/evaluation tool
```

In code, this should be one monorepo with shared packages and product profiles,
not separate implementation trees.

The current public documentation tree follows that positioning:

```text
docs/v1/             current personal-memory runtime and local CLI
docs/v2/             future team/shared-memory product scope
docs/eval-harness/   scenario, baseline, candidate, and provider eval workflow
docs/project/        roadmap, release, packaging, and policy material
```

## GitHub Packaging Recommendation

Make `mneme-eval` separately installable from the same repository.

Possible command names:

```bash
mneme eval run --suite core
mneme-eval run --suite core
```

Public users should be able to:

1. install the eval harness,
2. point it at a Mneme-compatible implementation,
3. run the same conformance scenarios,
4. compare reports across versions.

## Current Next Step

The next large product-supporting phase should close the v1 eval feedback loop:

1. Promote reviewed candidate artifacts into official eval scenarios.
2. Compare current and previous baseline reports for regressions and trend
   signals.
3. Use those reports to decide whether v1 behavior is improving before opening
   v2 team/shared-memory work.

Do not start team sync before the eval harness can preserve real v1 failures as
stable regression scenarios and compare quality across releases.
