# Mneme

Mneme is a local-first memory runtime and eval harness for agent workflows. v1
is personal memory for one user. v2 adds a team-memory policy core for shared
workspaces where private, project, agent, and team memory must stay separated
until policy allows it.

The public repository is intentionally focused on deterministic local behavior:
JSON stores, CLI workflows, agent hooks, review and curation tools, team policy
checks, and public-safe eval suites. Hosted sync, UI, and production storage
belong to later tracks.

Mneme currently provides:

- `mneme-core`: the v1 personal-memory engine and v2 team-memory policy core.
- `mneme-cli`: a local CLI over v1 and v2 JSON stores.
- `mneme-eval`: a scenario-based eval harness with v1 and v2 acceptance gates.
- `scripts/install-local.sh`: a local installer for the `mneme` CLI.
- `scripts/quickstart-smoke.sh`: an isolated first-run smoke test for public
  onboarding.
- `scripts/quality-gate.sh`: the single local gate used before PRs and
  releases.

## 5-Minute Quickstart

Run this from a fresh clone with Rust stable installed:

```sh
./scripts/install-local.sh
scripts/quickstart-smoke.sh
```

That smoke test creates a temporary local store, initializes Mneme, records a
preference, retrieves cited context, opens and closes an agent session, exports
a review artifact, and validates the store. It does not write private data to
the repository.

For the same flow as manual commands, see
[Quickstart](docs/v1/quickstart.md). For the broader developer path, see
[Getting Started](docs/v1/getting-started.md).

## Why Mneme v1

Mneme is not a hosted memory service or a generic vector database. It is a
personal memory layer for local agent work where every returned memory should be
auditable before it affects a task.

- Local-first by default: core v1 behavior runs against an inspectable JSON
  store without requiring a cloud account or API key.
- Citation-first memory: extracted claims keep source-event evidence so context
  can explain why it was returned.
- Scope and safety before relevance: context retrieval filters allowed scopes
  and blocks secret-like data before ranking.
- Agent-native workflow: begin/end hooks turn task sessions into cited memory
  writes and bounded context packs.
- Eval-gated development: scenario suites, dogfood fixtures, hard adversarial
  checks, ontology benchmarks, candidate promotion, and regression comparison
  are part of the repo.

Broader memory platforms tend to optimize for managed APIs, production
application scale, and provider integrations. Mneme v1 optimizes for local
privacy, provenance, scope discipline, and repeatable agent-memory evaluation.

## Evidence at a Glance

| Evidence surface | Public-safe signal | Current result |
| --- | --- | --- |
| Ontology readiness | 13 golden ontology cases | `1.00` entity/relation/attribute F1, `v1_ontology_ready` |
| Hard dogfood | 100 normal records, 150 adversarial records, 30 agent handoffs | `30/30` workflows passed |
| Safety guardrails | scope leak and secret leak checks | `0` scope leaks, `0` secret leaks |
| Public eval surface | core, runtime, agent, dogfood, model, and team suites | `42` public scenarios |
| Regression detection | seeded dropped-citation, scope, secret, stale, and handoff faults | `5/5` detected |
| Team v2 readiness | ACL, promotion, revoke, secret, and citation checks | `6/6` team scenarios passed, `5/5` v2 seeded faults detected |

For a GitHub-native scorecard with metric bars and reproducibility notes, see
[Mneme v1 Evidence Scorecard](docs/v1/evidence-scorecard.md).

## Current Status

Mneme is pre-1.0. The useful surface today is local development and evaluation:

- raw events are the source of truth;
- claims preserve source event citations;
- context retrieval is filtered by allowed memory scopes before relevance;
- context packs are deterministically ranked and capped before agent use;
- budget checks happen before extraction;
- secret-like data is blocked from active context;
- corrections and forgets are auditable lifecycle transitions;
- stored claims can be reviewed and changed by stable claim ID;
- stored memory quality can be inspected as duplicate, blocked-secret, and
  inactive-history review queues;
- stored memory can be curated through dry-run plans, explicit duplicate cleanup,
  explicit compaction of non-active records, and backup-backed rollback;
- stored memory can be exported as Markdown or JSON review artifacts with
  quality findings and sensitive claim text redacted by default;
- local JSON stores include schema metadata, write locks, atomic writes,
  backups, repair readiness checks, schema normalization, explicit backup
  restore, import/export, and non-active record compaction;
- the local CLI can be installed as `mneme` for first-run personal workflows;
- `mneme init` creates a local store and agent hook profile for a new
  workspace;
- `mneme doctor` reports workspace health for the local store and agent hook
  profile;
- agents can open and close task sessions with scoped context and post-task
  memory writes;
- agent hooks expose a stable JSON envelope for doctor/begin/end automation;
- `scripts/mneme-agent-hook.sh` provides an environment-configurable local
  wrapper for agent runtimes;
- wrapper doctor diagnostics report loaded runtime settings without running
  provider-backed extractors by default;
- agent hook runtime profiles can keep local store, agent, scope, and item-cap
  settings out of each invocation;
- extraction and storage are behind adapter boundaries;
- model-backed extraction experiments can use a provider-neutral command
  adapter and expanded model eval suite without adding API keys to the repo;
- a public OpenAI wrapper example can run through the same command protocol,
  with CI using deterministic dry-run mode;
- failed eval or baseline reports can be converted into ignored, sanitized
  scenario candidate artifacts for dogfood feedback review;
- reviewed v1 behavior can be checked with a deterministic dogfood readiness
  gate that validates and replays core, runtime, agent, and dogfood suites;
- v1 manual dogfood can be run locally with 100 public-safe synthetic records
  and 25 workflow checks before product promotion;
- v1 hard dogfood can run 100 normal records, 150 adversarial records, and 30
  agent handoff workflows with scorecards, seeded-fault checks, regression
  gates, official candidate-check artifacts, trend history, and public-safe
  reports;
- real-use v1 pilots can use a local-only workspace and sanitized feedback
  triage before any public issue or eval candidate is created;
- natural-language ontology benchmarking can measure current v1 entity,
  relation, attribute, scope, temporal, provenance, context, and safety gaps
  before ontology implementation changes, then map those gaps to implementation
  priorities;
- the default v1 rule extractor now captures a conservative natural-language
  ontology layer for the public benchmark and is checked by the quality gate;
- v2 team memory supports local users, agents, projects, scoped memory,
  reviewed promotion into team memory, admin revoke, audit records, secret
  blocking, and team context packs;
- v2 readiness can be checked through the public `team` suite, `mneme-v2`
  target, seeded-fault acceptance, and `scripts/v2-team-dogfood.py`;
- Mneme is MIT licensed for source use, while workspace crates remain marked
  `publish = false` until a registry publication path is intentionally
  prepared.

See [Mneme v1 Stability](docs/v1/v1-stability.md) for the current stability
contract.
See [Mneme v1 Completion Criteria](docs/v1/v1-completion-criteria.md) for the
public v1 readiness gate.
See [API Contract](docs/project/api-contract.md) for the current Rust API
surface and documentation gate.
See [Distribution Policy](docs/project/distribution-policy.md) for the current
MIT license and registry publication policy.

For local CLI details, see [Local CLI](docs/v1/local-cli.md). Without
`--store`, the CLI writes to `.mneme/mneme-v1.json` in the current directory.
`.mneme/` is ignored by git.

For v2 team-memory details, see [Mneme v2](docs/v2/README.md). Without
`--store`, `mneme team ...` writes to `.mneme/mneme-team-v2.json`.

## Eval Harness

Validate and run the public core suite:

```sh
cargo run -p mneme-eval -- validate --suite core
cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
```

Run the runtime maintenance suite:

```sh
cargo run -p mneme-eval -- validate --suite runtime
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
```

Run the agent integration suite:

```sh
cargo run -p mneme-eval -- validate --suite agent
cargo run -p mneme-eval -- run --suite agent --target mneme-v1
```

Run the v1 dogfood readiness gate before treating a build as a v1 product
candidate:

```sh
cargo run -p mneme-eval -- validate --suite dogfood
cargo run -p mneme-eval -- run --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- v1-readiness --json --report evals/reports/v1-readiness.json
scripts/v1-dogfood.sh
cargo run -p mneme-eval -- dogfood-summary evals/runs/v1-dogfood/<run-label>
scripts/v1-manual-dogfood.py
scripts/v1-hard-dogfood.py
scripts/v1-real-use-pilot.py
scripts/v1-ontology-benchmark.py
```

Run the v2 team-memory readiness gate before treating a build as a v2 team
candidate:

```sh
cargo run -p mneme-eval -- validate --suite team
cargo run -p mneme-eval -- run --suite team --target mneme-v2
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
cargo run -p mneme-eval -- v2-readiness --json --report evals/reports/v2-readiness.json
scripts/v2-team-dogfood.py
```

Run the opt-in command extraction suite:

```sh
cargo run -p mneme-eval -- validate --suite model
cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command evals/fixtures/command-extractor.sh
```

Run the OpenAI wrapper example without provider credentials:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py
```

Build a repeated baseline report for provider-wrapper quality tracking:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --report evals/reports/openai-dry-run-baseline.json
```

Baseline JSON includes aggregate, category-level, and per-scenario pass rates.
Gate a saved baseline report before treating it as usable:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

Summarize a saved baseline report for local triage:

```sh
cargo run -p mneme-eval -- baseline-summary evals/reports/openai-dry-run-baseline.json
```

Compare two saved baseline reports before accepting a change:

```sh
cargo run -p mneme-eval -- baseline-compare \
  evals/reports/before.json \
  evals/reports/after.json \
  --fail-on-regression
```

Create local candidate artifacts from a failed report before promoting any new
public scenario:

```sh
cargo run -p mneme-eval -- candidate evals/reports/openai-dry-run-baseline.json \
  --out-dir evals/candidates/openai \
  --limit 3
cargo run -p mneme-eval -- candidate-check evals/candidates/openai
cargo run -p mneme-eval -- candidate-promote \
  evals/candidates/openai/dogfood-example.candidate.yaml \
  --suite model \
  --filename dogfood-example.yaml \
  --apply
```

Run the acceptance gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
```

Use `--json` for machine-readable reports.
Use `help` or `<command> --help` to inspect command-specific usage:

```sh
cargo run -p mneme-cli -- help begin
cargo run -p mneme-eval -- baseline-gate --help
cargo run -p mneme-eval -- baseline-summary --help
cargo run -p mneme-eval -- baseline-compare --help
cargo run -p mneme-eval -- candidate-promote --help
cargo run -p mneme-eval -- v1-readiness --help
cargo run -p mneme-eval -- v2-readiness --help
cargo run -p mneme-eval -- dogfood-summary --help
```

## Evaluation Evidence

The latest public-safe local evidence snapshot was measured for `v0.60.0` on
2026-05-25. These numbers are reproducible development evidence for Mneme,
not claims against external production workloads. Full run bundles are ignored
by git; the committed fixtures and scripts are safe to inspect and rerun.

The same evidence is summarized in the GitHub-native
[Mneme v1 Evidence Scorecard](docs/v1/evidence-scorecard.md).

| Surface | Public-safe data | Latest result |
| --- | --- | --- |
| Scenario suites | 42 public scenarios across `core`, `runtime`, `agent`, `dogfood`, `model`, and `team` | validation, replay, acceptance, baseline, regression, and candidate gates passed in `quality-gate` |
| Manual dogfood | 100 synthetic records and 25 workflow checks | fixture shape verified in CI; full evidence remains local-only |
| Hard dogfood | 100 normal records, 150 adversarial records, 30 agent handoff workflows | `30/30` workflows passed; `Recall@K=1.0`, `Precision@K=1.0`, `citation_coverage=1.0`, `handoff_success=1.0`, `scope_leak=0`, `secret_leak=0` |
| Seeded faults | dropped citation, scope leak, secret leak, stale reuse, handoff miss | `5/5` detected |
| Candidate bridge | hard-mode findings mirrored into official candidate YAML | `5/5` candidates valid with `mneme-eval candidate-check` |
| Ontology benchmark | 13 golden ontology cases: 10 natural-language, 3 explicit-marker anchors | current v1 reports `ontology_benchmark_passed` and `v1_ontology_ready`: `entity_f1=1.0`, `relation_f1=1.0`, `attribute_f1=1.0`, `scope_accuracy=1.0`, `temporal_correctness=1.0`, `provenance_coverage=1.0`, `context_recall_at_k=1.0`, `scope_leak=0`, `secret_leak=0` |
| v2 team readiness | 6 public team scenarios for ACL, project access, promotion, secret blocking, and revoked agents | `ready_for_team_v2_dogfood`; `6/6` scenarios passed; `5/5` seeded faults detected |
| v2 team dogfood shape | 120 synthetic team records, 80 adversarial records, 25 handoff workflows | fixture shape verified; generated bundles are public-safe and ignored by git |

The ontology benchmark remains the public regression gate for natural-language
memory behavior. It separates product capability buckets such as extraction,
relation mapping, entity resolution, attributes, temporal state, scoped
ownership, multi-hop context, provenance, and safety.

## Development Checks

Before opening a PR, run:

```sh
./scripts/quality-gate.sh full
```

Check package assembly directly:

```sh
./scripts/package-check.sh
```

Check distribution guardrails directly:

```sh
./scripts/distribution-policy-check.sh
```

Build API docs with warnings denied:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

Generated eval reports, candidate artifacts, and local stores are ignored.
Public scenarios under `evals/scenarios/` are tracked.

CI runs on pull requests and `main` pushes only. Branch pushes do not trigger
the full gate, which keeps action usage aligned with phase-sized work.

## Repository Layout

```text
README.md             main public entry point
crates/mneme-core     shared v1 personal-memory and v2 team-memory core
crates/mneme-cli      local v1/v2 CLI
crates/mneme-eval     reusable eval harness CLI
docs/v1/              Mneme v1 personal-memory docs
docs/v2/              Mneme v2 team-memory docs
docs/eval-harness/    scenario, baseline, candidate, and provider eval docs
docs/project/         roadmap, release, packaging, and policy docs
evals/                public scenario fixtures
scripts/              local quality, safety, and live-baseline helpers
spec/                 feature specs and verification maps
```

## Documentation

- [Documentation Map](docs/README.md)
- [Mneme v1](docs/v1/README.md)
- [Mneme v2](docs/v2/README.md)
- [Eval Harness](docs/eval-harness/README.md)
- [Project and Release](docs/project/README.md)
