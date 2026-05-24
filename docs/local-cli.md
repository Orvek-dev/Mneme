# Mneme Local CLI

The local CLI is a thin developer interface over the Mneme v1 personal-memory
core. It uses the same `mneme-core` engine as the eval target and persists state
through the JSON file store.

## Commands

```sh
cargo run -p mneme-cli -- doctor
cargo run -p mneme-cli -- ingest "remember: user prefers local-first tools"
cargo run -p mneme-cli -- remember "user prefers local-first tools"
cargo run -p mneme-cli -- correct "user prefers local-first tools" "user prefers desktop IDE"
cargo run -p mneme-cli -- forget "user prefers desktop IDE"
cargo run -p mneme-cli -- context "desktop IDE"
cargo run -p mneme-cli -- snapshot --json
cargo run -p mneme-cli -- validate --json
cargo run -p mneme-cli -- compact
```

The default store is `.mneme/mneme-v1.json` under the current working
directory. `.mneme/` is ignored by git.

Use `--store <path>` to isolate experiments:

```sh
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store /tmp/mneme.json
cargo run -p mneme-cli -- context "local-first" --store /tmp/mneme.json --json
```

## Store Maintenance

The local JSON store includes schema metadata and generation tracking. Writes
are atomic, and replacing an existing store creates `<store>.bak`.

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
