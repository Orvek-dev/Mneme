# Mneme Local CLI

The local CLI is a thin developer interface over the Mneme v1 personal-memory
core. It uses the same `mneme-core` engine as the eval target and persists state
through the JSON file store.

## Commands

```sh
cargo run -p mneme-cli -- doctor
cargo run -p mneme-cli -- remember "user prefers local-first tools"
cargo run -p mneme-cli -- correct "user prefers local-first tools" "user prefers desktop IDE"
cargo run -p mneme-cli -- forget "user prefers desktop IDE"
cargo run -p mneme-cli -- context "desktop IDE"
cargo run -p mneme-cli -- snapshot --json
```

The default store is `.mneme/mneme-v1.json` under the current working
directory. `.mneme/` is ignored by git.

Use `--store <path>` to isolate experiments:

```sh
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store /tmp/mneme.json
cargo run -p mneme-cli -- context "local-first" --store /tmp/mneme.json --json
```

## Event Options

`remember`, `correct`, and `forget` accept:

- `--speaker <id>`: defaults to `user`.
- `--agent <id>`: optional acting agent.
- `--scope <scope>`: defaults to `private`.
- `--trust <trust>`: defaults to `trusted_user`.
- `--json`: prints machine-readable command output.

The CLI intentionally keeps the v1 deterministic lifecycle markers visible:

- `remember <claim>` writes `remember: <claim>`.
- `correct <old-claim> <new-claim>` writes
  `correct: <old-claim> -> <new-claim>`.
- `forget <claim>` writes `forget: <claim>`.
