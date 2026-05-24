# Agent Integration

Mneme v1 exposes a small local protocol that agents can call around a task.
For automation, prefer `mneme hook begin/end`; the older `begin/end --json`
commands remain useful for direct CLI inspection.

## Start A Task

Use `hook begin` to retrieve task-scoped context and create a session record:

```sh
cargo run -p mneme-cli -- hook begin "Draft a setup plan" \
  --query "local-first" \
  --scope private \
  --max-items 3 \
  --agent codex \
  --store /tmp/mneme.json
```

The JSON output includes:

- `schema_version: mneme.agent_hook.v1`
- `ok`
- `operation`
- `session_id`
- `report.session.id`
- `report.session.context_claim_ids`
- `report.context_pack.items`

The agent should keep the returned `session.id` for the end call.
`begin` defaults to the `private` scope. Pass repeated `--scope <scope>` values
when the agent is authorized to retrieve another scope for the task.
Returned context is deterministically ranked and capped to 8 items by default;
pass `--max-items <n>` when a task needs a tighter context budget.

## End A Task

Use `hook end` to close the session and optionally write explicit memory claims:

```sh
cargo run -p mneme-cli -- hook end session-001 \
  --summary "Prepared a concise setup plan" \
  --remember "user prefers concise setup plans" \
  --store /tmp/mneme.json
```

`--summary` is recorded on the session. Each `--remember` value is written as a
normal v1 memory event through the rule extractor, so secret blocking, citation,
budget, and audit behavior stay centralized in `mneme-core`.

Hook failures write a JSON envelope to stdout and exit non-zero. Agents should
read `ok`, `recoverable`, `error.kind`, `error.message`, and `error.exit_code`
instead of parsing stderr.
If `error.kind` is `store_lock`, another local writer is active; agents can
continue without memory or retry the hook later.

## Session Records

Sessions are persisted in the local store with:

- `id`
- `task`
- `actor_agent_id`
- `status`
- `started_at_unix_seconds`
- `ended_at_unix_seconds`
- `context_query`
- `context_claim_ids`
- `summary`
- `memory_event_ids`

Begin and end operations emit `session.begin` and `session.end` audit records.

## Eval Coverage

The `agent` suite checks:

- begin returns existing memory as session context;
- end closes the session and writes remembered claims;
- remembered claims are retrievable with citations;
- secret-like remembered claims remain blocked from active context;
- seeded faults still fail the suite.

Run it locally:

```sh
cargo run -p mneme-eval -- validate --suite agent
cargo run -p mneme-eval -- run --suite agent --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
```
