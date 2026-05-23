# Mneme v1 Personal Core

Mneme v1 starts as a deterministic personal-memory core with a small persistence
boundary. The goal is to make the product runtime executable behind the eval
harness before adding teams, UI, or model-backed extraction.

## Current Scope

The v1 core supports:

- raw event append;
- deterministic budget hard-cap checks before extraction;
- explicit memory extraction from `remember:` and `기억해줘:` markers;
- claim status tracking for active and blocked-secret claims;
- context-pack retrieval over active claims;
- source event citations on context items;
- read/write audit records;
- serializable state snapshots;
- an in-memory store and local JSON file store for restart tests.

## Current Non-Goals

The v1 core does not yet provide:

- production-grade storage, migrations, compaction, or concurrent writes;
- concurrent sessions;
- team/shared memory;
- user correction or deletion workflows;
- real LLM extraction;
- vector search;
- external API or UI.

## Eval Target

The eval target is `mneme-v1`.

```sh
cargo run -p mneme-eval -- run --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
```

The target adapts `mneme-core` output into the eval harness normalized actual
state. The harness checker still owns pass/fail decisions.
