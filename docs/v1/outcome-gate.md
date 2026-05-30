# V1 Outcome Gate

Mneme outcome gate turns "the agent says the task is done" into a stored,
checkable session result. It does not make Mneme a test runner or an LLM judge:
external verifiers propose results, and `mneme-core` owns the final
`gate_result`.

## Boundary

The gate keeps the boundary strict:

- the core stores `mneme.acceptance.v1` on session begin;
- the CLI captures git/worktree baseline before work starts;
- an external verifier emits `mneme.verifier.v1` on session end;
- the core validates the verifier contract and stores `gate_result`;
- `hook end` exits non-zero when gated work is not passed;
- an external reviewer/model can later submit `mneme.judgment.v1` for
  `judgment` criteria that ended as `pending_judgment`.

The core never runs subprocesses. Command execution, diff inspection, and symbol
checks live outside the core in `scripts/mneme-outcome-verifier.py` or another
compatible verifier. Subjective judgments also live outside the core; Mneme only
validates and records the verdict.

## Acceptance Contract

Start from a template when possible:

```sh
mneme outcome template \
  --kind rust \
  --include-judgment \
  --output acceptance.json

mneme outcome validate acceptance.json --json
```

`mneme begin --acceptance` also validates the same contract before storing a
session. Invalid criteria are rejected up front, so malformed gates cannot be
silently recorded as active work.

Pass an acceptance contract at begin:

```json
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "parser-task",
  "criteria": [
    {
      "id": "tests-pass",
      "kind": "command",
      "command": {
        "argv": ["cargo", "test", "-p", "mneme-core"],
        "expect_exit": 0
      }
    },
    {
      "id": "core-changed",
      "kind": "diff_touches",
      "diff_touches": {
        "paths": ["crates/mneme-core/src/v1.rs"]
      }
    },
    {
      "id": "scope-core-only",
      "kind": "diff_scope",
      "diff_scope": {
        "allowed_paths": ["crates/mneme-core", "docs/v1"]
      }
    },
    {
      "id": "gate-symbol-exists",
      "kind": "symbol_present",
      "symbol_present": {
        "path": "crates/mneme-core/src/v1.rs",
        "symbol": "OutcomeGateResult"
      }
    }
  ]
}
```

`command` uses `argv` by default. `shell: true` with `run` is supported by the
reference verifier but should stay opt-in.

## CLI Flow

```sh
mneme begin "Implement parser" \
  --acceptance acceptance.json \
  --store .mneme/mneme-v1.json \
  --json

# agent edits files here

mneme end session-001 \
  --summary "Implemented parser" \
  --verifier-command scripts/mneme-outcome-verifier.py \
  --store .mneme/mneme-v1.json \
  --json

mneme outcome status session-001 \
  --store .mneme/mneme-v1.json \
  --json
```

If `gate_result.completed` is false, `mneme end` and `mneme hook end` exit
non-zero after writing JSON. The failed session is still stored, including
public-safe evidence, so a later agent can continue from the failed criterion
instead of receiving a fake completed handoff.

## External Judgment Intake

Use `judgment` for criteria that cannot be proven by a deterministic command,
for example UX acceptability or first-implementation quality. Session end will
store `pending_judgment` until an external verdict is submitted:

```json
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "ui-polish",
  "criteria": [
    {
      "id": "reviewer-accepts-ux",
      "kind": "judgment",
      "judgment": {
        "rubric": "external reviewer accepts the UI outcome"
      }
    }
  ]
}
```

Submit a verdict after review:

```sh
mneme outcome judge session-001 \
  --id reviewer-accepts-ux \
  --verdict pass \
  --evidence "Reviewer accepted the UI outcome" \
  --reviewer lee \
  --task-id ui-polish \
  --store .mneme/mneme-v1.json \
  --json
```

The equivalent report-file contract is `mneme.judgment.v1`:

```json
{
  "schema_version": "mneme.judgment.v1",
  "task_id": "ui-polish",
  "reviewer": "lee",
  "results": [
    {
      "id": "reviewer-accepts-ux",
      "verdict": "pass",
      "evidence": "Reviewer accepted the UI outcome"
    }
  ]
}
```

`mneme outcome judge` exits non-zero when the verdict fails, remains pending, or
produces an invalid gate. The updated `gate_result` is still stored so the next
agent can continue from the failed or pending evidence.

## Statuses

| Status | Meaning |
| --- | --- |
| `passed` | Every deterministic criterion passed. The session can be treated as completed work evidence. |
| `failed` | At least one deterministic criterion failed. Continue from the failed evidence. |
| `error` | The verifier contract was invalid, missing, duplicated, unknown, or returned an error result. |
| `pending_judgment` | A `judgment` criterion still requires external human/model verdict input. |

Existing session lifecycle status stays `active` / `closed`. Completion trust is
owned by `session.gate_result`, not by the lifecycle enum.

## Local Smoke

Run the dedicated outcome gate smoke:

```sh
cargo build -p mneme-cli
scripts/outcome-gate-smoke.sh
```

The smoke creates an isolated git repo, checks template generation and
validation, rejects a malformed acceptance contract before begin, checks a
passing gated session, checks `mneme outcome status`, verifies that an
out-of-scope diff produces a non-zero failed gate, and checks both passing and
failing external judgment verdicts.
