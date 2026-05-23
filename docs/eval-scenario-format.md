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
```

Run the full harness acceptance gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
```

Use `--json` for machine-readable output and `--report <path>` to write a JSON
report.

`fake` is the default target. CI passes targets explicitly so future adapters
cannot silently change what is being tested.

## Schema

```yaml
id: same-turn-explicit-remember
tags:
  - recall
budget:
  daily_cloud_tokens: 100
persistence:
  restart_after_event: 1
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
    must_include:
      - local-first
    must_not_include: []
    citation_required: true
  budget:
    hard_cap_violations: 0
  audit:
    read_write_events_required: true
    claim_update_required: false
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
- `budget.daily_cloud_tokens`: deterministic fake token cap. Defaults to
  `100000` and must be greater than zero.
- `persistence.restart_after_event`: asks compatible targets to persist state
  and reload after the 1-based event index. It must be within the event count.
  Targets without real persistence may treat this as an in-process checkpoint,
  but product targets should use their storage adapter.
- `events[].actor_agent_id`: agent acting on behalf of the speaker.
- `events[].scope`: memory scope. Defaults to `private`.
- `events[].trust_level`: input trust level. Defaults to `trusted_user`.
- `claims[].status`: expected claim status, such as `active`,
  `blocked_secret`, `superseded`, or `forgotten`.
- `claims[].scope`: expected claim scope.
- `claims[].must_not_exist`: marks a claim as prohibited.
- `context_pack.query`: deterministic context retrieval query.
- `context_pack.must_include`: strings that must appear in the context pack.
- `context_pack.must_not_include`: strings that must not appear in the context
  pack.
- `context_pack.citation_required`: requires source event citations.
- `budget.hard_cap_violations`: expected budget hard-cap violation count.
- `audit.read_write_events_required`: requires read/write audit evidence.
- `audit.claim_update_required`: requires `claim.update` audit evidence for
  correction or forget scenarios.

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
