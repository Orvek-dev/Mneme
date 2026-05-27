# API Contract

Mneme is pre-1.0, so Rust API names and module placement are not yet semantic
versioning commitments. This document defines the intended public API surface
for the current local-first MVP and the verification required before releases.

## Intended Public Crates

- `mneme-core` is the product runtime API.
- `mneme-cli` exposes the `mneme` binary and `run_cli` for CLI-bound local
  tooling.
- `mneme-mcp` exposes the local stdio MCP server for MCP-capable coding agents.
- `mneme-eval` exposes the `mneme-eval` binary and `run_cli` for harness-bound
  local tooling.
- `mneme hook doctor/begin/end` expose the `mneme.agent_hook.v1` JSON contract
  for local agent automation.

New integrations should prefer `mneme-core` unless they specifically need to
drive the command-line contracts.

## Core API Surface

The current `mneme-core` entry points are `MnemeEngine` for v1 personal memory
and `TeamMemoryEngine` for v2 team memory.

Primary runtime flow:

1. Create `MnemeEngine` with `MnemeConfig`.
2. Append raw user or agent events with `EventInput`.
3. Retrieve cited, ranked, budget-capped context with `build_context_pack` or
   an explicit `ContextQuery`.
4. Inspect claim IDs and lifecycle status with `snapshot`.
5. Persist and reload state through a `MnemeStore` implementation.
6. Use `begin_session` and `end_session` when an agent needs task-scoped
   context and explicit post-task memory writes.
7. Use `end_session_with_extractor` when an agent needs command-extracted
   post-task memory notes.

Supported extension points:

- `MnemeStore` for storage adapters.
- `MnemeExtractor` for extraction adapters.
- `ContextQuery` for scoped retrieval boundaries and context item caps.
- `StoreErrorKind` for stable store and lock conflict classification.
- `CommandExtractor` and the `mneme.extractor.command.v1` JSON protocol for
  provider-wrapper experiments.

CLI-bound integrations can use `mneme claims`, `mneme forget --claim-id`, and
`mneme correct --claim-id` when they need an inspectable user-control surface
without linking directly to Rust APIs.

CLI-bound review workflows can use `mneme quality` for read-only review queues,
`mneme curate` for dry-run or explicitly applied cleanup, and
`mneme review <path>` to write a Markdown or JSON artifact. Quality, curation,
and review artifacts redact sensitive claim text by default; raw review export
requires `--include-sensitive`. CLI-bound recovery workflows can use
`mneme repair --check` for invalid or legacy stores and `mneme restore --check`
for explicit rollback from a valid backup.

CLI-bound first-run workflows can use `mneme init` to create the default local
store and `.mneme/mneme-agent-hook.env` profile before wiring an agent runtime.
They can use `mneme doctor --json` to inspect store/profile health and
recommendations without mutating workspace files.

CLI-bound team workflows can use `mneme team init`, `mneme team user add`,
`mneme team agent add`, `mneme team project add`, `mneme team remember`,
`mneme team promote`, `mneme team review`, `mneme team context`,
`mneme team handoff`, `mneme team run begin/note/end/handoff`,
`mneme team sync export/import`, `mneme team firewall`, `mneme team quality`,
`mneme team promotion report`, `mneme team ontology`,
`mneme team adapter manifest`, and
`mneme team validate` against the local `.mneme/mneme-team-v2.json` store.
Direct integrations can use `TeamMemoryEngine`, `TeamMemoryStore`,
`JsonTeamFileStore`, team role/scope inputs, promotion review inputs,
task-run inputs, sync-envelope inputs, handoff packages, firewall reports,
quality reports, ontology reports, and `validate_team_state`.

MCP integrations should use `mneme-mcp`. The server exposes V1 personal-memory
tools and V2 team-memory tools over a local stdio JSON-RPC boundary while
delegating policy to `mneme-core`. CLI-bound integrations can use
`mneme mcp config` to print Codex, Claude Code, and Cursor snippets without
mutating local client config files. `scripts/mneme-mcp-stdio.py` remains as a
legacy thin bridge over `mneme team ... --json`.

Agent runtimes can use `scripts/mneme-agent-hook.sh` as the repository-local
installation wrapper. It delegates to an installed `MNEME_BIN`, otherwise runs
cargo from the repository, and uses a local debug binary only when cargo is
unavailable. The wrapper can load runtime profiles from
`MNEME_AGENT_HOOK_CONFIG`, `MNEME_CONFIG`, or `.mneme/mneme-agent-hook.env`.
Wrapper doctor diagnostics are no-cost by default for configured command
extractors; `doctor --check-extractor` is the explicit command-extractor smoke
path.

Local users can install the CLI and MCP server with `scripts/install-local.sh`.
It installs the repository-local `mneme-cli` package as the `mneme` binary, the
`mneme-mcp` package as the `mneme-mcp` binary, and runs small
doctor/help/review/MCP smoke checks.

Stable behavior remains defined by `docs/v1/v1-stability.md`, `docs/v2/`,
public feature specs, eval scenarios, `mneme-eval v1-readiness`,
`mneme-eval v2-readiness`, the `mcp` eval suite against `mneme-mcp`, and the
release quality gate.

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
baseline checks, baseline-summary triage checks, baseline comparison checks,
scenario candidate generation, candidate validation, candidate promotion
checks, public-safety checks, and package assembly checks.

## Change Rule

When changing public API shape, update the same PR with the relevant evidence:

- crate-level docs or item docs;
- `docs/project/api-contract.md` and `docs/v1/v1-stability.md` when the intended API
  surface changes;
- compile-checked examples or unit tests;
- feature specs and verification maps;
- eval scenarios when behavior changes.
