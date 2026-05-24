# Personal Runtime

Mneme v1 is still pre-1.0, but the local personal runtime now has a stable
maintenance surface for single-user development.

## Store Format

The default local store is `.mneme/mneme-v1.json`. The JSON state schema is
currently `2` and includes:

- `schema_version`
- `metadata.store_id`
- `metadata.generation`
- `metadata.created_at_unix_seconds`
- `metadata.updated_at_unix_seconds`
- `metadata.engine_version`
- `metadata.migration_history`
- `budget`, `events`, `claims`, `sessions`, and `audit`

Missing schema metadata from older stores is treated as legacy state.
Compatible older schemas remain readable, and `mneme repair` can normalize
schema metadata through the same atomic save path while keeping the
pre-normalized file as `<store>.bak`.

## Write Safety

`JsonFileStore` writes through a same-directory temporary file and then replaces
the current store. When a current store exists, it is copied to
`<store>.bak` before replacement.

Save and repair operations first create `<store>.lock` with exclusive
`create_new` semantics. If the lock file already exists, the write is not
attempted and callers receive a `store_lock` conflict. The lock is removed when
the write attempt finishes.

This is still a local single-user store, but concurrent hook or CLI writers are
explicitly rejected instead of racing the JSON file.

## Maintenance Commands

Review stored claims before changing them:

```sh
cargo run -p mneme-cli -- claims --status active --store /tmp/mneme.json --json
cargo run -p mneme-cli -- quality --store /tmp/mneme.json --json
cargo run -p mneme-cli -- forget --claim-id claim-001 --store /tmp/mneme.json
```

Export a review artifact when the store needs a durable audit surface:

```sh
cargo run -p mneme-cli -- review /tmp/mneme-review.md --store /tmp/mneme.json
cargo run -p mneme-cli -- review /tmp/mneme-review.json --format json --store /tmp/mneme.json
```

`mneme quality` reports duplicate active claims, blocked-secret claims,
inactive lifecycle history, and suggested follow-up commands without mutating
the store. Review artifacts include the same quality section and redact
sensitive claim text by default. Use `--include-sensitive` only for local
private inspection.

Validate the current store:

```sh
cargo run -p mneme-cli -- validate --store /tmp/mneme.json --json
```

Export and import a store:

```sh
cargo run -p mneme-cli -- export /tmp/mneme-export.json --store /tmp/mneme.json
cargo run -p mneme-cli -- import /tmp/mneme-export.json --store /tmp/mneme-restored.json
```

Compact inactive lifecycle records while preserving active recall:

```sh
cargo run -p mneme-cli -- compact --store /tmp/mneme.json
```

Check repair readiness without mutating files:

```sh
cargo run -p mneme-cli -- repair --check --store /tmp/mneme.json --json
```

Repair a corrupted current store from its backup, or normalize a compatible
legacy store schema:

```sh
cargo run -p mneme-cli -- repair --store /tmp/mneme.json --json
```

## Eval Coverage

The `runtime` suite checks:

- export/import round trips;
- compaction after correction;
- repair from backup after current-store corruption;
- repair readiness checks in the release quality gate;
- secret blocking after persisted import/export.

Run it locally:

```sh
cargo run -p mneme-eval -- validate --suite runtime
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
```
