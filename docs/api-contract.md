# API Contract

Mneme is pre-1.0, so Rust API names and module placement are not yet semantic
versioning commitments. This document defines the intended public API surface
for the current local-first MVP and the verification required before releases.

## Intended Public Crates

- `mneme-core` is the product runtime API.
- `mneme-cli` exposes the `mneme` binary and `run_cli` for CLI-bound local
  tooling.
- `mneme-eval` exposes the `mneme-eval` binary and `run_cli` for harness-bound
  local tooling.
- `mneme hook begin/end` expose the `mneme.agent_hook.v1` JSON contract for
  local agent automation.

New integrations should prefer `mneme-core` unless they specifically need to
drive the command-line contracts.

## Core API Surface

The current `mneme-core` entry point is `MnemeEngine`.

Primary runtime flow:

1. Create `MnemeEngine` with `MnemeConfig`.
2. Append raw user or agent events with `EventInput`.
3. Retrieve cited, ranked, budget-capped context with `build_context_pack` or
   an explicit `ContextQuery`.
4. Persist and reload state through a `MnemeStore` implementation.
5. Use `begin_session` and `end_session` when an agent needs task-scoped
   context and explicit post-task memory writes.

Supported extension points:

- `MnemeStore` for storage adapters.
- `MnemeExtractor` for extraction adapters.
- `ContextQuery` for scoped retrieval boundaries and context item caps.
- `StoreErrorKind` for stable store and lock conflict classification.
- `CommandExtractor` and the `mneme.extractor.command.v1` JSON protocol for
  provider-wrapper experiments.

Stable behavior remains defined by `docs/v1-stability.md`, public feature
specs, eval scenarios, and the release quality gate.

## Example

The compile-checked example lives at
`crates/mneme-core/examples/personal_memory.rs`.

Run it with:

```sh
cargo run -p mneme-core --example personal_memory
```

Build local API docs with warnings denied:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
```

## Documentation Gate

Release verification must keep Rustdoc building cleanly with warnings denied.
This catches broken intra-doc links, undocumented public API drift, and example
snippets that no longer compile.

The full local gate runs:

```sh
./scripts/quality-gate.sh full
```

That gate includes the Rustdoc build, unit tests, CLI smoke checks, eval suites,
baseline checks, public-safety checks, and package assembly checks.

## Change Rule

When changing public API shape, update the same PR with the relevant evidence:

- crate-level docs or item docs;
- `docs/api-contract.md` and `docs/v1-stability.md` when the intended API
  surface changes;
- compile-checked examples or unit tests;
- feature specs and verification maps;
- eval scenarios when behavior changes.
