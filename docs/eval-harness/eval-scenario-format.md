# Eval Scenario Format

Mneme eval scenarios are public YAML fixtures that describe one isolated memory
behavior test. The eval harness validates these files before replaying them.

## Location

Scenario suites live under `evals/scenarios/<suite-name>/`.

Invalid example fixtures for harness tests live under `evals/fixtures/invalid/`
and must not be included in a runnable suite directory.

Generated JSON reports belong under `evals/reports/` or `evals/runs/`; both are
ignored by default.

Generated candidate artifacts belong under `evals/candidates/`, which is also
ignored by default. A candidate is a review artifact, not a runnable suite
fixture, until its nested `scenario` block is manually reviewed and promoted.

## Commands

Inspect available commands:

```sh
cargo run -p mneme-eval -- help
cargo run -p mneme-eval -- run --help
cargo run -p mneme-eval -- help baseline-gate
cargo run -p mneme-eval -- help baseline-summary
cargo run -p mneme-eval -- help baseline-compare
cargo run -p mneme-eval -- help candidate
cargo run -p mneme-eval -- help candidate-promote
cargo run -p mneme-eval -- help v1-readiness
```

Validate a suite without running the fake runtime:

```sh
cargo run -p mneme-eval -- validate --suite core
```

Validate one scenario:

```sh
cargo run -p mneme-eval -- validate evals/scenarios/core/same-turn-explicit-remember.yaml
```

Replay one scenario:

```sh
cargo run -p mneme-eval -- replay evals/scenarios/core/same-turn-explicit-remember.yaml --target fake
```

Run a suite:

```sh
cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
cargo run -p mneme-eval -- run --suite agent --target mneme-v1
cargo run -p mneme-eval -- run --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- run --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh
```

Run the full harness acceptance gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh
```

Use `--json` for machine-readable output and `--report <path>` to write a JSON
report.

Gate a saved provider baseline report:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

Summarize a saved provider baseline report for local triage:

```sh
cargo run -p mneme-eval -- baseline-summary evals/reports/openai-dry-run-baseline.json
```

Compare two saved provider baseline reports for regressions:

```sh
cargo run -p mneme-eval -- baseline-compare evals/reports/before.json evals/reports/after.json --fail-on-regression
```

Create and validate local scenario candidates from a failed report:

```sh
cargo run -p mneme-eval -- candidate evals/reports/openai-live-baseline.json --out-dir evals/candidates/openai --limit 3
cargo run -p mneme-eval -- candidate-check evals/candidates/openai
cargo run -p mneme-eval -- candidate-promote evals/candidates/openai/dogfood-example.candidate.yaml --suite model --filename dogfood-example.yaml
```

Run v1 product-readiness gates:

```sh
cargo run -p mneme-eval -- v1-readiness --json --report evals/reports/v1-readiness.json
```

`fake` is the default target. CI passes targets explicitly so future adapters
cannot silently change what is being tested.

The `model` suite is opt-in and is intended for command-backed/model-backed
extraction checks. Use `mneme-v1-command` with `--extractor-command`; the
tracked fixture at `evals/fixtures/command-extractor.sh` is deterministic and
does not require provider credentials.

The `dogfood` suite is deterministic and intended for v1 product-readiness
checks. It is included in `mneme-eval v1-readiness` with `core`, `runtime`, and
`agent`.

## Schema

```yaml
id: same-turn-explicit-remember
tags:
  - recall
budget:
  daily_cloud_tokens: 100
persistence:
  restart_after_event: 1
maintenance:
  export_import_roundtrip: false
  compact_after_events: false
  repair_from_backup: false
  restore_from_backup: false
  curation:
    apply: false
    compact: false
agent_flow:
  begin:
    task: "Draft setup plan"
    actor_agent_id: codex
    query: "local-first"
    allowed_scopes:
      - private
  end:
    summary: "Prepared a concise setup plan"
    extractor: rule
    remember:
      - "user prefers concise setup plans"
events:
  - speaker_id: user
    actor_agent_id: codex
    scope: private
    trust_level: trusted_user
    text: "remember: user prefers local-first tools"
expected:
  event_append:
    count: 1
  claims:
    - subject: user
      predicate: prefers
      object: local-first tools
      status: active
      scope: private
  context_pack:
    query: "user preferences"
    allowed_scopes:
      - private
    max_items: 8
    item_count: 1
    must_include:
      - local-first
    must_not_include: []
    expected_order:
      - local-first
    omitted_reason_contains: []
    citation_required: true
  budget:
    hard_cap_violations: 0
  audit:
    read_write_events_required: true
    claim_update_required: false
    session_events_required: false
  store:
    schema_version: 2
    valid: true
    backup_required: false
    repair_performed: false
    restored: false
    compacted: false
    imported: false
  session:
    status: closed
    task: "Draft setup plan"
    actor_agent_id: codex
    context_must_include:
      - local-first tools
    memory_event_count: 1
    summary_contains: concise setup
  quality:
    duplicate_active_groups: 0
    duplicate_active_claims: 0
    blocked_secret_count: 0
    inactive_claim_count: 0
    review_item_count: 0
    finding_kinds: []
  curation:
    duplicate_forget_count: 0
    blocked_secret_review_count: 0
    compact_recommended: false
    compacted: false
    changed: false
    before_quality:
      duplicate_active_groups: 0
      review_item_count: 0
    after_quality:
      duplicate_active_groups: 0
      review_item_count: 0
```

## Required Fields

- `id`: stable scenario ID. It must not be empty.
- `events`: ordered input events. The list must contain at least one event.
- `expected`: expected checks. At least one expected check must be present.

Each event requires:

- `speaker_id`: source speaker. It must not be empty.
- `text`: input text. It must not be empty.

Current `mneme-v1` lifecycle markers are deterministic:

- `remember: <claim>` or `기억해줘: <claim>` writes a claim.
- `correct: <old claim> -> <new claim>` or `수정: <old claim> -> <new claim>`
  supersedes an active claim and writes the replacement.
- `forget: <claim>` or `잊어줘: <claim>` marks an active claim as forgotten.
- `correct-id: <claim-id> -> <new claim>` and `forget-id: <claim-id>` target
  one active claim by ID.

Each expected claim requires:

- `subject`
- `predicate`
- `object`

## Optional Fields

- `tags`: labels used for filtering, reporting, and later suite curation.
  Tags prefixed with `category-` are aggregated by `mneme-eval baseline`; for
  example, `category-secret`, `category-no-claim`, `category-communication`,
  `category-format`, or `category-project`.
- `budget.daily_cloud_tokens`: deterministic fake token cap. Defaults to
  `100000` and must be greater than zero.
- `persistence.restart_after_event`: asks compatible targets to persist state
  and reload after the 1-based event index. It must be within the event count.
  Targets without real persistence may treat this as an in-process checkpoint,
  but product targets should use their storage adapter.
- `maintenance.export_import_roundtrip`: asks compatible targets to persist,
  export, import, and reload state before final checks.
- `maintenance.compact_after_events`: asks compatible targets to compact
  inactive claims before context and store checks.
- `maintenance.repair_from_backup`: asks compatible targets to corrupt the
  current store after backup creation, repair from backup, and reload.
- `maintenance.restore_from_backup`: asks compatible targets to create a valid
  backup, run any requested curation, restore from backup, and reload.
- `maintenance.curation.apply`: asks compatible targets to run guided memory
  curation after events and before final checks.
- `maintenance.curation.compact`: asks curation to compact non-active records
  after applied duplicate cleanup. It requires `apply: true`.
- `agent_flow.begin`: asks compatible targets to start an agent session with
  `task`, optional `actor_agent_id`, optional context `query`, and optional
  `allowed_scopes`.
- `agent_flow.end`: asks compatible targets to close that session with an
  optional `summary`, optional `extractor` (`rule` or `command`), and zero or
  more `remember` values. `rule` treats values as explicit claims; `command`
  passes them as raw memory notes to the target command extractor.
- `events[].actor_agent_id`: agent acting on behalf of the speaker.
- `events[].scope`: memory scope. Defaults to `private`.
- `events[].trust_level`: input trust level. Defaults to `trusted_user`.
- `claims[].status`: expected claim status, such as `active`,
  `blocked_secret`, `superseded`, or `forgotten`.
- `claims[].scope`: expected claim scope.
- `claims[].must_not_exist`: marks a claim as prohibited.
- `context_pack.query`: deterministic context retrieval query.
- `context_pack.allowed_scopes`: scopes allowed for context retrieval. Defaults
  to `private` when omitted.
- `context_pack.max_items`: maximum ranked context items to return. Defaults to
  the runtime cap of 8 when omitted.
- `context_pack.item_count`: exact expected number of returned context items.
- `context_pack.must_include`: strings that must appear in the context pack.
- `context_pack.must_not_include`: strings that must not appear in the context
  pack.
- `context_pack.expected_order`: strings that must appear in returned context
  items in this relative ranking order.
- `context_pack.omitted_reason_contains`: omission reason fragments that must
  appear, such as `scope_denied:project-alpha` or
  `context_budget_exceeded:max_items=2`.
- `context_pack.citation_required`: requires source event citations.
- `budget.hard_cap_violations`: expected budget hard-cap violation count.
- `audit.read_write_events_required`: requires read/write audit evidence.
- `audit.claim_update_required`: requires `claim.update` audit evidence for
  correction or forget scenarios.
- `audit.session_events_required`: requires `session.begin` and `session.end`
  audit evidence.
- `store.schema_version`: expected persisted state schema version.
- `store.valid`: requires the inspected current store to be valid.
- `store.backup_required`: requires a backup file to exist.
- `store.repair_performed`: requires repair from backup to have run.
- `store.restored`: requires explicit restore from backup to have run.
- `store.compacted`: requires compaction to have run.
- `store.imported`: requires an export/import round trip to have run.
- `session.status`: expected session status.
- `session.task`: expected session task.
- `session.actor_agent_id`: expected acting agent.
- `session.context_must_include`: strings that must appear in begin-session
  context claims.
- `session.memory_event_count`: expected remembered event count written by the
  session end.
- `session.summary_contains`: string that must appear in the session summary.
- `quality.duplicate_active_groups`: expected number of active duplicate
  claim groups by text and scope.
- `quality.duplicate_active_claims`: expected number of active claims that
  belong to duplicate groups.
- `quality.blocked_secret_count`: expected blocked-secret claim count.
- `quality.inactive_claim_count`: expected superseded plus forgotten claim
  count.
- `quality.review_item_count`: expected number of memory review queue items.
- `quality.finding_kinds`: quality finding kinds that must be present, such as
  `duplicate_active`, `blocked_secret`, or `inactive_history`.
- `curation.duplicate_forget_count`: expected number of duplicate active claims
  selected for ID-based forget during curation.
- `curation.blocked_secret_review_count`: expected number of blocked-secret
  records surfaced for manual privacy review before compaction.
- `curation.compact_recommended`: whether curation identified non-active
  records that can be compacted after review.
- `curation.compacted`: whether curation actually compacted the scenario state.
- `curation.changed`: whether curation changed the scenario state.
- `curation.before_quality`: quality expectations before curation.
- `curation.after_quality`: quality expectations after curation.

Unknown fields are rejected. This keeps public fixtures strict enough for long
term compatibility.

## Seeded Faults

Seeded faults intentionally break the fake runtime so the harness can prove that
critical regressions fail.

```sh
cargo run -p mneme-eval -- run --suite core --target fake --seeded-fault skip-claims
```

Current seeded fault modes:

- `skip-claims`: suppresses claim extraction.
- `leak-secrets`: marks blocked secrets as active claims.
- `drop-citations`: removes source event citations from context-pack items.
