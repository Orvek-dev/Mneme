# Eval Harness Acceptance Gate

The acceptance gate answers one question: is the Eval Harness v0 strong enough
to start Mneme v1 implementation without changing the harness contract first?

Run it locally with:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
```

Use JSON output for automation:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake --json
```

## Required Gates

The acceptance command must pass every gate:

- `scenario.validation`: all scenarios in the selected suite validate.
- `invalid-fixtures.rejected`: malformed fixtures are rejected.
- `target.core-suite`: the selected target passes the core suite.
- `report.contract`: eval reports include schema version, target, counts, and
  results.
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
- read/write audit evidence;
- seeded fault detection for claims, secrets, and citations.

The current `core` suite does not yet cover:

- long-term persistence across process restarts;
- concurrent sessions or multi-agent conflict resolution;
- team/shared memory scopes;
- real LLM extraction quality;
- external storage adapters;
- UI behavior;
- performance benchmarks.

These gaps are acceptable before Mneme v1 starts because the v1 target can be
added behind the same adapter boundary and expanded against the same acceptance
gate.

## Phase 1 Entry Rule

Mneme v1 implementation can start when all are true:

- CI passes `cargo run -p mneme-eval -- acceptance --suite core --target fake`.
- `docs/eval-scenario-format.md` describes the public fixture contract.
- `docs/eval-target-adapter-contract.md` describes how a v1 target plugs into
  the harness.
- generated reports and local harness files remain ignored.
