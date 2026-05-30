# V1 Outcome Gate

Mneme outcome gate turns "the agent says the task is done" into a stored,
checkable session result. It does not make Mneme a test runner or an LLM judge:
external verifiers propose results, and `mneme-core` owns the final
`gate_result`.

## Boundary

MVP1 keeps the boundary strict:

- the core stores `mneme.acceptance.v1` on session begin;
- the CLI captures git/worktree baseline before work starts;
- an external verifier emits `mneme.verifier.v1` on session end;
- the core validates the verifier contract and stores `gate_result`;
- `hook end` exits non-zero when gated work is not passed.

The core never runs subprocesses. Command execution, diff inspection, and symbol
checks live outside the core in `scripts/mneme-outcome-verifier.py` or another
compatible verifier.

## Acceptance Contract

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

## Statuses

| Status | Meaning |
| --- | --- |
| `passed` | Every deterministic criterion passed. The session can be treated as completed work evidence. |
| `failed` | At least one deterministic criterion failed. Continue from the failed evidence. |
| `error` | The verifier contract was invalid, missing, duplicated, unknown, or returned an error result. |
| `pending_judgment` | A `judgment` criterion requires external human/model verdict input. MVP1 records this as pending; MVP2 adds verdict intake. |

Existing session lifecycle status stays `active` / `closed`. Completion trust is
owned by `session.gate_result`, not by the lifecycle enum.

## Local Smoke

Run the dedicated MVP1 smoke:

```sh
cargo build -p mneme-cli
scripts/outcome-gate-smoke.sh
```

The smoke creates an isolated git repo, checks a passing gated session, checks
`mneme outcome status`, and verifies that an out-of-scope diff produces a
non-zero failed gate.
