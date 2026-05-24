# Eval Scenario Format

Mneme eval scenarios are public YAML fixtures that describe one isolated memory
behavior test. The eval harness validates these files before replaying them.

## Location

Scenario suites live under `evals/scenarios/<suite-name>/`.

Invalid example fixtures for harness tests live under `evals/fixtures/invalid/`
and must not be included in a runnable suite directory.

Generated JSON reports belong under `evals/reports/` or `evals/runs/`; both are
ignored by default.

## Commands

Inspect available commands:

```sh
cargo run -p mneme-eval -- help
cargo run -p mneme-eval -- run --help
cargo run -p mneme-eval -- help baseline-gate
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
cargo run -p mneme-eval -- run --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh
```

Run the full harness acceptance gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh
```

Use `--json` for machine-readable output and `--report <path>` to write a JSON
report.

Gate a saved provider baseline report:

```sh
cargo run -p mneme-eval -- baseline-gate evals/reports/openai-dry-run-baseline.json
```

`fake` is the default target. CI passes targets explicitly so future adapters
cannot silently change what is being tested.

The `model` suite is opt-in and is intended for command-backed/model-backed
extraction checks. Use `mneme-v1-command` with `--extractor-command`; the
tracked fixture at `evals/fixtures/command-extractor.sh` is deterministic and
does not require provider credentials.

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
agent_flow:
  begin:
    task: "Draft setup plan"
    actor_agent_id: codex
    query: "local-first"
    allowed_scopes:
      - private
  end:
    summary: "Prepared a concise setup plan"
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
    must_include:
      - local-first
    must_not_include: []
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

Each expected claim requires:

- `subject`
- `predicate`
- `object`

## Optional Fields

- `tags`: labels used for filtering, reporting, and later suite curation.
  Tags prefixed with `category-` are aggregated by `mneme-eval baseline`; for
  example, `category-secret` or `category-no-claim`.
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
- `agent_flow.begin`: asks compatible targets to start an agent session with
  `task`, optional `actor_agent_id`, optional context `query`, and optional
  `allowed_scopes`.
- `agent_flow.end`: asks compatible targets to close that session with an
  optional `summary` and zero or more explicit `remember` claims.
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
- `context_pack.must_include`: strings that must appear in the context pack.
- `context_pack.must_not_include`: strings that must not appear in the context
  pack.
- `context_pack.omitted_reason_contains`: omission reason fragments that must
  appear, such as `scope_denied:project-alpha`.
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
