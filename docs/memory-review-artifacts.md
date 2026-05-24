# Memory Review Artifacts

`mneme review` exports a store summary for human review or scripted checks. It
does not mutate the store and redacts sensitive claim text by default.

## Commands

Export Markdown:

```sh
cargo run -p mneme-cli -- review /tmp/mneme-review.md --store /tmp/mneme.json
```

Export JSON:

```sh
cargo run -p mneme-cli -- review /tmp/mneme-review.json \
  --format json \
  --store /tmp/mneme.json
```

Use `--json` when the CLI stdout report also needs to be machine-readable:

```sh
cargo run -p mneme-cli -- review /tmp/mneme-review.md \
  --store /tmp/mneme.json \
  --json
```

Inspect the same review queue without writing an artifact:

```sh
cargo run -p mneme-cli -- quality --store /tmp/mneme.json --json
```

Export raw sensitive text only for local private inspection:

```sh
cargo run -p mneme-cli -- review /tmp/mneme-review.raw.json \
  --format json \
  --include-sensitive \
  --store /tmp/mneme.json
```

## Contents

Artifacts include:

- store path, schema version, generation, event count, claim count, and session
  count;
- claim lifecycle counts for `active`, `blocked_secret`, `superseded`, and
  `forgotten`;
- memory quality findings for duplicate active claims, blocked-secret claims,
  and inactive lifecycle history;
- review queue items with claim IDs and suggested follow-up commands;
- scope counts across all stored claims;
- claim IDs, status, scope, text, and source event IDs;
- session IDs, status, task, context query, context claim IDs, and memory event
  IDs.
- redaction policy metadata, including whether redaction was enabled and how
  many claims or fields were redacted.

Markdown artifacts are intended for direct reading and release review notes.
JSON artifacts carry the same fields for automation.

## Quality Findings

`mneme quality` and `mneme review` share the same read-only quality model:

- `duplicate_active`: multiple active claims have the same normalized text and
  scope. Review the IDs and forget redundant active claims by ID.
- `blocked_secret`: secret-like claims are retained but excluded from active
  context. Confirm they should stay blocked, or forget them by ID.
- `inactive_history`: superseded or forgotten claims remain for audit. Export a
  review artifact, then run `mneme compact` when that history is no longer
  needed.

Quality reports redact blocked-secret text by default and include exact claim
IDs so follow-up commands can be run without exposing sensitive values.

## Redaction

Default review export uses the `default_safe` policy. It redacts:

- `blocked_secret` claim object text;
- obvious secret-like field text such as API key, token, password, or secret
  assignments;
- key-like values with common secret prefixes.

The artifact still keeps claim IDs, lifecycle status, scope, and source event
IDs so a user can decide whether to forget or correct the memory without seeing
the sensitive value.

## Safety

Review artifacts can still contain non-secret user memory text. Keep them
outside git unless the store content is already safe to publish. The local
`.mneme/` directory is ignored by the repository and is the preferred place for
private runtime files.

Use `--include-sensitive` only when the artifact will remain local and private.
