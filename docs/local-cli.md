# Mneme Local CLI

The local CLI is a thin developer interface over the Mneme v1 personal-memory
core. It uses the same `mneme-core` engine as the eval target and persists state
through the JSON file store.

## Commands

Inspect available commands:

```sh
cargo run -p mneme-cli -- help
cargo run -p mneme-cli -- help begin
cargo run -p mneme-cli -- begin --help
```

```sh
cargo run -p mneme-cli -- doctor
cargo run -p mneme-cli -- ingest "remember: user prefers local-first tools"
cargo run -p mneme-cli -- remember "user prefers local-first tools"
cargo run -p mneme-cli -- remember "user prefers project launch reviews" --scope project-alpha
cargo run -p mneme-cli -- correct "user prefers local-first tools" "user prefers desktop IDE"
cargo run -p mneme-cli -- forget "user prefers desktop IDE"
cargo run -p mneme-cli -- claims --status active --json
cargo run -p mneme-cli -- review /tmp/mneme-review.md
cargo run -p mneme-cli -- context "desktop IDE"
cargo run -p mneme-cli -- context "project launch" --scope project-alpha --max-items 3
cargo run -p mneme-cli -- snapshot --json
cargo run -p mneme-cli -- begin "Draft setup plan" --query "local-first" --scope private --max-items 3 --agent codex --json
cargo run -p mneme-cli -- end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --json
cargo run -p mneme-cli -- hook doctor --json
cargo run -p mneme-cli -- hook begin "Draft setup plan" --query "local-first" --agent codex
cargo run -p mneme-cli -- validate --json
cargo run -p mneme-cli -- compact
```

The default store is `.mneme/mneme-v1.json` under the current working
directory. `.mneme/` is ignored by git.

Use `--store <path>` to isolate experiments:

```sh
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store /tmp/mneme.json
cargo run -p mneme-cli -- claims --status active --store /tmp/mneme.json --json
cargo run -p mneme-cli -- context "local-first" --store /tmp/mneme.json --json
```

`context` defaults to the `private` scope. Pass one or more `--scope <scope>`
values to retrieve claims from other authorized scopes. Results are ranked by
deterministic term/phrase matches and capped to 8 items unless `--max-items
<n>` is provided:

```sh
cargo run -p mneme-cli -- remember "user prefers project launch reviews" \
  --scope project-alpha \
  --store /tmp/mneme.json
cargo run -p mneme-cli -- context "project launch" \
  --scope project-alpha \
  --max-items 3 \
  --store /tmp/mneme.json \
  --json
```

## Claim Review

Use `claims` to inspect stored memory before changing it:

```sh
cargo run -p mneme-cli -- claims --store /tmp/mneme.json --json
cargo run -p mneme-cli -- claims --status active --scope private --store /tmp/mneme.json --json
```

The report includes claim IDs, lifecycle status, scope, and source event IDs.
When duplicate claim text exists, prefer ID-based lifecycle commands:

```sh
cargo run -p mneme-cli -- forget --claim-id claim-001 --store /tmp/mneme.json
cargo run -p mneme-cli -- correct --claim-id claim-002 "user prefers terminal workflows" --store /tmp/mneme.json
```

Unknown or inactive claim IDs fail before writing a lifecycle event.

Use `review` when the inspection output should become a file that can be read
or attached outside the CLI:

```sh
cargo run -p mneme-cli -- review /tmp/mneme-review.md --store /tmp/mneme.json
cargo run -p mneme-cli -- review /tmp/mneme-review.json --format json --store /tmp/mneme.json --json
```

Markdown artifacts are optimized for human review. JSON artifacts carry the
same counts and summaries for scripts.

## Store Maintenance

The local JSON store includes schema metadata and generation tracking. Writes
create `<store>.lock`, write atomically, and replacing an existing store creates
`<store>.bak`. If another writer holds the lock, write commands fail without
modifying the store.

Validate the current store:

```sh
cargo run -p mneme-cli -- validate --store /tmp/mneme.json --json
```

Export and import a validated store:

```sh
cargo run -p mneme-cli -- export /tmp/mneme-export.json --store /tmp/mneme.json
cargo run -p mneme-cli -- import /tmp/mneme-export.json --store /tmp/mneme-restored.json
```

Compact inactive lifecycle records:

```sh
cargo run -p mneme-cli -- compact --store /tmp/mneme.json --json
```

Repair a corrupted current store from `<store>.bak`:

```sh
cargo run -p mneme-cli -- repair --store /tmp/mneme.json --json
```

## Agent Sessions

`begin` retrieves context and records a session:

```sh
cargo run -p mneme-cli -- begin "Draft setup plan" \
  --query "local-first" \
  --scope private \
  --max-items 3 \
  --agent codex \
  --store /tmp/mneme.json \
  --json
```

`begin` uses the same allowed-scope guard, deterministic ranking, and item cap
as `context`.

`hook begin` and `hook end` run the same session operations with the
`mneme.agent_hook.v1` JSON envelope. They always write JSON to stdout and are
intended for agents and local automations:

```sh
cargo run -p mneme-cli -- hook doctor \
  --store /tmp/mneme.json
cargo run -p mneme-cli -- hook begin "Draft setup plan" \
  --query "local-first" \
  --agent codex \
  --store /tmp/mneme.json

cargo run -p mneme-cli -- hook end session-001 \
  --summary "Prepared a concise setup plan" \
  --remember "user prefers concise setup plans" \
  --store /tmp/mneme.json
```

`end` closes the session and can write explicit memory claims:

```sh
cargo run -p mneme-cli -- end session-001 \
  --summary "Prepared a concise setup plan" \
  --remember "user prefers concise setup plans" \
  --store /tmp/mneme.json \
  --json
```

## Event Options

`ingest`, `remember`, `correct`, and `forget` accept:

- `--speaker <id>`: defaults to `user`.
- `--agent <id>`: optional acting agent.
- `--scope <scope>`: defaults to `private`.
- `--trust <trust>`: defaults to `trusted_user`.
- `--json`: prints machine-readable command output.

The CLI intentionally keeps the v1 deterministic lifecycle markers visible:

- `ingest <text>` writes the event exactly as provided.
- `remember <claim>` writes `remember: <claim>`.
- `correct <old-claim> <new-claim>` writes
  `correct: <old-claim> -> <new-claim>`.
- `forget <claim>` writes `forget: <claim>`.
- `correct --claim-id <id> <new-claim>` writes
  `correct-id: <id> -> <new-claim>` after checking the claim is active.
- `forget --claim-id <id>` writes `forget-id: <id>` after checking the claim is
  active.

## Command Extractor

`ingest` can delegate extraction to a local command:

```sh
cargo run -p mneme-cli -- ingest "the user prefers local-first tools" \
  --extractor command \
  --extractor-command ./mneme-extractor-wrapper \
  --store /tmp/mneme.json
```

The wrapper receives the command extraction JSON request on stdin and must write
the response JSON to stdout. `MNEME_EXTRACTOR_COMMAND` can provide the command
program when `--extractor-command` is omitted; pass command arguments with
repeated `--extractor-arg <arg>` flags. API keys should stay in the wrapper's
environment, not in the Mneme store or tracked repo files.
