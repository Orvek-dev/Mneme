# Eval Target Adapter Contract

The eval harness runs scenarios against an eval target. A target is the boundary
between public fixtures and a concrete Mneme implementation.

## Current Target

- `fake`: deterministic in-process target used to prove the harness, scenario
  checks, report shape, budget behavior, audit checks, and seeded fault
  detection.
- `mneme-v1`: deterministic in-process adapter over the Mneme v1 personal core
  in `mneme-core`.

`mneme-v1` uses `mneme-core`'s default `RuleBasedExtractor`. Model-backed
experiments can use `CommandExtractor` through product code or a future opt-in
eval target, but the default CI target stays deterministic so public checks do
not require provider credentials.

The fake target is the default. CI still passes `--target fake` explicitly so a
future Mneme implementation cannot silently change what is being tested.

Before adding a production target, keep the fake target passing:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target fake
```

Mneme v1 must also pass the same gate:

```sh
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
```

## Target Responsibilities

Every target must:

- start each scenario with isolated state;
- append input events in scenario order;
- expose extracted claims with subject, predicate, object, status, scope, and
  source event IDs;
- build context-pack items with source event citations when requested;
- expose budget hard-cap violations;
- expose audit events for relevant reads and writes;
- return normalized actual state to the harness checker.

The checker owns pass/fail decisions. Targets should not decide whether a
scenario succeeded; they only run the scenario and return actual state.

## Normalized Actual State

Targets return the same normalized categories regardless of their internal
storage engine:

- `events`: appended event records.
- `claims`: extracted or blocked memory claims.
- `context_pack`: retrieved context items and omitted items.
- `budget`: deterministic budget counters.
- `audit`: read/write evidence.

This keeps `mneme-eval run --suite core --target fake` and a future
`mneme-eval run --suite core --target mneme-v1` comparable.

## Seeded Faults

Seeded faults are target-level options. The fake target currently supports:

- `skip-claims`
- `leak-secrets`
- `drop-citations`

Future targets may reject unsupported seeded faults, but the harness should keep
using the same report and exit-code behavior.

## Report Contract

Eval reports include:

- `report_schema_version`
- `target`
- `ok`
- `scenario_count`
- `passed`
- `failed`
- `results`

`report_schema_version` starts at `1`. Additive fields can keep the same schema
version; incompatible report changes should increment it.
