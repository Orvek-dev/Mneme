# Personal Runtime

Mneme v1.0.0 exposes a stable local personal-runtime maintenance surface for
single-user development.

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

Save, repair, and restore operations first create `<store>.lock` with exclusive
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
cargo run -p mneme-cli -- curate --store /tmp/mneme.json --json
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

`mneme curate` turns those findings into a dry-run cleanup plan. Add `--apply`
to forget redundant duplicate active claims by ID. Add `--compact` only when
non-active records, including blocked-secret, superseded, and forgotten claims,
should be removed after review. Applied curation reports include restore
commands so the previous backup can be checked and rolled back if needed.

Validate the current store:

```sh
cargo run -p mneme-cli -- validate --store /tmp/mneme.json --json
```

Export and import a store:

```sh
cargo run -p mneme-cli -- export /tmp/mneme-export.json --store /tmp/mneme.json
cargo run -p mneme-cli -- import /tmp/mneme-export.json --store /tmp/mneme-restored.json
```

Compact non-active records while preserving active recall:

```sh
cargo run -p mneme-cli -- compact --store /tmp/mneme.json
```

Compaction keeps active claims and removes blocked-secret, superseded, and
forgotten records plus events no active claim cites.

Check repair readiness without mutating files:

```sh
cargo run -p mneme-cli -- repair --check --store /tmp/mneme.json --json
```

Repair a corrupted current store from its backup, or normalize a compatible
legacy store schema:

```sh
cargo run -p mneme-cli -- repair --store /tmp/mneme.json --json
```

Check and run an explicit rollback from a valid backup:

```sh
cargo run -p mneme-cli -- restore --check --store /tmp/mneme.json --json
cargo run -p mneme-cli -- restore --store /tmp/mneme.json --json
```

`mneme restore` is intentionally separate from repair. Repair is for an invalid
or legacy-compatible current store. Restore is an explicit user rollback from a
valid `<store>.bak`; the pre-restore current file becomes the new backup.

## Eval Coverage

The core and runtime suites check:

- export/import round trips;
- compaction after correction;
- guided curation before/after quality;
- restore from backup after applied curation and compaction;
- repair from backup after current-store corruption;
- repair readiness checks in the release quality gate;
- secret blocking after persisted import/export.

Run it locally:

```sh
cargo run -p mneme-eval -- validate --suite runtime
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
```
