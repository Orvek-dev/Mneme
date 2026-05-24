# Eval Harness Acceptance Gate

The acceptance gate answers one question: is the Eval Harness v0 strong enough
to start Mneme v1 implementation without changing the harness contract first?

Run it locally with:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh
```

Use JSON output for automation:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake --json
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1 --json
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1 --json
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1 --json
cargo run -p mneme-eval -- acceptance --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh --json
```

## Required Gates

The acceptance command must pass every gate:

- `scenario.validation`: all scenarios in the selected suite validate.
- `invalid-fixtures.rejected`: malformed fixtures are rejected.
- `target.core-suite`: the selected target passes the core suite.
- `report.contract`: eval reports include schema version, target, target
  metadata, counts, and results.
- `seeded-fault.skip-claims`: a missing-claim regression is detected.
- `seeded-fault.leak-secrets`: a secret-leak regression is detected.
- `seeded-fault.drop-citations`: a missing-citation regression is detected.

If any gate fails, the command exits non-zero.

## Core Suite Coverage

The current `core` suite covers:

- explicit same-turn memory capture;
- deterministic context-pack inclusion;
- source event citation checks;
- budget hard-cap blocking;
- blocked secret handling;
- memory quality review queues;
- guided curation before/after quality checks;
- explicit restore after applied curation and compaction;
- correction lifecycle from superseded claim to active replacement;
- forget lifecycle persisted across file-backed restart;
- file-backed restart persistence for `mneme-v1`;
- read/write audit evidence;
- seeded fault detection for claims, secrets, and citations.

The `runtime` suite covers:

- schema metadata on persisted stores;
- export/import round trips;
- compaction after correction lifecycle changes;
- backup repair after current-store corruption;
- persisted secret blocking.

The `agent` suite covers:

- begin-session context retrieval;
- end-session memory writes;
- session ledger expectations;
- session audit evidence;
- secret blocking from agent-recorded memory.

The current suites do not yet cover:

- concurrent writes;
- concurrent sessions or multi-agent conflict resolution;
- team/shared memory scopes;
- real LLM extraction quality;
- external storage adapters;
- UI behavior;
- performance benchmarks.

The opt-in `model` suite covers:

- implicit preference extraction through a command-backed adapter;
- no-claim handling for non-memory events;
- secret blocking after command extraction;
- explicit correction lifecycle after command extraction.

These gaps are acceptable before Mneme v1 starts because the v1 target can be
added behind the same adapter boundary and expanded against the same acceptance
gate.

## Active Targets

- `fake`: harness proof target.
- `mneme-v1`: personal-memory v1 core target.
- `mneme-v1-command`: opt-in command extractor target for model-suite checks.

## Phase 1 Entry Rule

Mneme v1 implementation can start when all are true:

- CI passes `cargo run -p mneme-eval -- acceptance --suite core --target fake`.
- `docs/eval-scenario-format.md` describes the public fixture contract.
- `docs/eval-target-adapter-contract.md` describes how a v1 target plugs into
  the harness.
- generated reports and local harness files remain ignored.

## Phase 1 Completion Rule

The first Mneme v1 core slice is complete when all are true:

- CI passes `cargo run -p mneme-eval -- run --suite core --target mneme-v1`.
- CI passes
  `cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1`.
- `mneme-core` owns the personal-memory domain model, engine, and persistence
  boundary.
