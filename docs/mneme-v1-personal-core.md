# Mneme v1 Personal Core

Mneme v1 starts as a deterministic personal-memory core with a small persistence
boundary. The goal is to make the product runtime executable behind the eval
harness before adding teams, UI, or production storage.

## Current Scope

The v1 core supports:

- raw event append;
- deterministic budget hard-cap checks before extraction;
- extraction through the `MnemeExtractor` adapter boundary;
- explicit memory extraction from `remember:` and `기억해줘:` markers;
- explicit memory correction from `correct:` and `수정:` markers;
- explicit memory removal from `forget:` and `잊어줘:` markers;
- claim status tracking for active, blocked-secret, superseded, and forgotten
  claims;
- context-pack retrieval over active claims;
- source event citations on context items;
- read/write audit records;
- serializable state snapshots;
- an in-memory store and local JSON file store for restart tests.
- a rule-based extractor used by default for deterministic v1 behavior;
- a command extractor protocol for opt-in model-backed experiments.

## Current Non-Goals

The v1 core does not yet provide:

- production-grade storage, migrations, compaction, or concurrent writes;
- concurrent sessions;
- team/shared memory;
- provider-specific production LLM extraction adapters;
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
