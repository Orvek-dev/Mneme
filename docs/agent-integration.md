# Agent Integration

Mneme v1 exposes a small local protocol that agents can call around a task.
For automation, prefer `mneme hook doctor/begin/end`; the older
`begin/end --json` commands remain useful for direct CLI inspection.

## Start A Task

Check the local hook runtime first:

```sh
cargo run -p mneme-cli -- hook doctor --store /tmp/mneme.json
scripts/mneme-agent-hook.sh doctor
```

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

## Runtime Wrapper

Use `scripts/mneme-agent-hook.sh` when configuring an agent runtime that should
not know cargo details:

```sh
MNEME_STORE=/tmp/mneme.json \
MNEME_AGENT_ID=codex \
MNEME_SCOPE=private \
MNEME_MAX_ITEMS=3 \
  scripts/mneme-agent-hook.sh begin "Draft a setup plan" --query "local-first"

MNEME_STORE=/tmp/mneme.json \
MNEME_AGENT_ID=codex \
  scripts/mneme-agent-hook.sh end session-001 --summary "Prepared setup plan"
```

Supported environment variables:

- `MNEME_AGENT_HOOK_CONFIG`: explicit runtime profile path.
- `MNEME_CONFIG`: fallback runtime profile path.
- `MNEME_BIN`: path to an installed `mneme` binary.
- `MNEME_STORE`: store path appended when `--store` is absent.
- `MNEME_AGENT_ID`: agent ID appended when `--agent` is absent.
- `MNEME_SCOPE`: begin scope appended when `--scope` is absent.
- `MNEME_MAX_ITEMS`: begin item cap appended when `--max-items` is absent.

For persistent local configuration, copy
`examples/mneme-agent-hook.env.example` to `.mneme/mneme-agent-hook.env`.
Runtime values resolve as CLI flags, then environment variables, then profile
values, then command defaults.

`scripts/mneme-agent-hook.sh doctor` runs an isolated temporary-store smoke test
covering hook doctor, begin, and end.

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
