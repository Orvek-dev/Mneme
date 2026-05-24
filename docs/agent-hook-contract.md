# Agent Hook Contract

`mneme hook doctor`, `mneme hook begin`, and `mneme hook end` are the
automation-facing runtime commands. `begin` and `end` use the same `mneme-core`
session behavior as the direct CLI commands, but hook commands always write a
stable JSON envelope to stdout.

## Schema

The current hook envelope schema is `mneme.agent_hook.v1`.

Successful `hook doctor` output includes:

- `schema_version`
- `ok: true`
- `operation: doctor`
- `recoverable: false`
- `store`
- `default_store`
- `version`
- `build_stage`
- `operations`
- `inspection`

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
- `extractor`
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
cargo run -p mneme-cli -- hook doctor \
  --store /tmp/mneme.json

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

cargo run -p mneme-cli -- hook end session-001 \
  --summary "Prepared a direct planning doc" \
  --remember "For future planning docs, keep explanations direct and skip motivational language." \
  --extractor command \
  --extractor-command ./mneme-extractor-wrapper \
  --store /tmp/mneme.json
```

Agents should preserve `session_id` from begin and pass it to end. They should
use `report.context_pack.items` as task context and treat `context_claim_ids` as
the compact citation list for the started session.
By default, hook end uses the rule extractor and treats `--remember` values as
explicit claims. With `--extractor command`, hook end passes `--remember` values
as raw memory notes to the configured command extractor.

## Runtime Wrapper

Repository-local automation can call `scripts/mneme-agent-hook.sh` instead of
hard-coding cargo commands:

```sh
scripts/mneme-agent-hook.sh doctor
MNEME_STORE=/tmp/mneme.json MNEME_AGENT_ID=codex \
  scripts/mneme-agent-hook.sh begin "Draft setup plan" --query "local-first"
MNEME_STORE=/tmp/mneme.json \
  scripts/mneme-agent-hook.sh end session-001 --summary "Prepared setup plan"
```

The wrapper uses `MNEME_BIN` when set, otherwise runs
`cargo run -q -p mneme-cli --` from the repository, and falls back to
`target/debug/mneme` only when cargo is unavailable. The wrapper applies
`MNEME_STORE`, `MNEME_AGENT_ID`, `MNEME_SCOPE`, `MNEME_MAX_ITEMS`, and
`MNEME_EXTRACTOR_COMMAND` when the same CLI options are not already present.

Profiles can be loaded with `MNEME_AGENT_HOOK_CONFIG`, `MNEME_CONFIG`, or the
default ignored `.mneme/mneme-agent-hook.env` path. The file format is
documented in [Agent Runtime Config](agent-runtime-config.md).
