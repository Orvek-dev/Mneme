# Agent Hook Contract

`mneme hook begin` and `mneme hook end` are the automation-facing session
commands. They use the same `mneme-core` session behavior as `begin` and `end`,
but always write a stable JSON envelope to stdout.

## Schema

The current hook envelope schema is `mneme.agent_hook.v1`.

Successful `hook begin` output includes:

- `schema_version`
- `ok: true`
- `operation: begin`
- `recoverable: false`
- `store`
- `session_id`
- `context_item_count`
- `omitted_count`
- `context_claim_ids`
- `report`

Successful `hook end` output includes:

- `schema_version`
- `ok: true`
- `operation: end`
- `recoverable: false`
- `store`
- `session_id`
- `remembered_event_count`
- `remembered_claim_count`
- `remembered_event_ids`
- `remembered_claim_ids`
- `report`

Failure output includes:

```json
{
  "schema_version": "mneme.agent_hook.v1",
  "ok": false,
  "operation": "end",
  "recoverable": false,
  "error": {
    "kind": "session",
    "message": "agent session: unknown session: session-404",
    "exit_code": 1
  }
}
```

Hook failures exit non-zero after writing JSON. The CLI suppresses duplicate
stderr for hook failures that were already reported in the JSON envelope.

## Error Kinds

- `invalid_cli`: the command shape or argument values are invalid.
- `io`: filesystem or stdout I/O failed.
- `store`: loading, saving, validating, or repairing the store failed.
- `store_lock`: another writer holds the local store lock.
- `json`: JSON parsing or serialization failed.
- `extractor`: extraction failed.
- `session`: session lifecycle operation failed.

`recoverable` is `true` for store, `store_lock`, I/O, and extractor failures
where an agent may continue without memory or retry later. It is `false` for
invalid CLI, JSON, and session failures.

## Usage

```sh
cargo run -p mneme-cli -- hook begin "Draft setup plan" \
  --query "local-first" \
  --scope private \
  --max-items 3 \
  --agent codex \
  --store /tmp/mneme.json

cargo run -p mneme-cli -- hook end session-001 \
  --summary "Prepared a concise setup plan" \
  --remember "user prefers concise setup plans" \
  --store /tmp/mneme.json
```

Agents should preserve `session_id` from begin and pass it to end. They should
use `report.context_pack.items` as task context and treat `context_claim_ids` as
the compact citation list for the started session.
