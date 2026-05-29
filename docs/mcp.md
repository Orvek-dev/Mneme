# Mneme MCP

Mneme MCP is the local stdio server for connecting Mneme to coding agents and
MCP-capable clients. It exposes the same local-first memory behavior as the CLI:
V1 personal memory, V2 team handoff memory, JSON stores, citations, scope
checks, secret blocking, firewall reports, and validation.

For the package-level server guide, see
[`crates/mneme-mcp/README.md`](../crates/mneme-mcp/README.md). Client examples
are available for [Codex](../examples/codex/README.md), [Claude
Code](../examples/claude-code/README.md), and
[Cursor](../examples/cursor/README.md).

It does not require a hosted service. By default it reads and writes:

```text
.mneme/mneme-v1.json
.mneme/mneme-team-v2.json
```

The `.mneme/` directory is ignored by git.

## Build and Smoke Test

```sh
cargo build -p mneme-cli -p mneme-mcp -p mneme-eval
cargo run -p mneme-mcp -- --self-test
```

The self-test checks that the server can start, load its config, and expose the
expected tool list.

## Client Config

Use the CLI to print config snippets for Codex, Claude Code, and Cursor:

```sh
cargo run -p mneme-cli -- mcp config --client all
```

Use explicit stores when you want one workspace-local memory file per project:

```sh
cargo run -p mneme-cli -- mcp config \
  --client all \
  --mcp-bin mneme-mcp \
  --mode all \
  --v1-store .mneme/mneme-v1.json \
  --team-store .mneme/mneme-team-v2.json
```

Supported modes:

| Mode | Tools exposed | Use when |
| --- | --- | --- |
| `personal` | V1 tools only | One user wants a coding agent to remember local preferences and task context. |
| `team` | V2 tools only | Multiple users or agents need scoped handoff memory. |
| `all` | V1 and V2 tools | You want one MCP server for personal and team workflows. |

The config command prints snippets only. It does not mutate your client config
files.

## Tool Surface

V1 tools cover personal memory:

- check MCP status and the continuity contract;
- remember and ingest memory;
- retrieve cited context;
- begin and end task sessions;
- begin, end, and hand off continuity sessions with explicit lineage/scope;
- correct, forget, validate, quality-check, and snapshot the store.

V2 tools cover team handoff memory:

- initialize users, agents, and projects;
- write scoped team memory;
- retrieve actor-scoped context;
- build handoff packages;
- begin, note, end, and hand off task runs;
- promote memory through review;
- export/import sync envelopes;
- run firewall, quality, ontology, validation, revoke, and snapshot checks.

## MCP Eval Harness

The MCP target is tested through `mneme-eval`, not just by starting the server:

```sh
cargo run -p mneme-eval -- validate --suite mcp
cargo run -p mneme-eval -- run --suite mcp --target mneme-mcp \
  --json \
  --report /tmp/mneme-mcp-readiness.json
```

Current MCP readiness checks include:

| Gate | What it proves |
| --- | --- |
| initialize and tools/list | Client handshake and tool registry are usable. |
| MCP status | A client can verify store paths, tool inventory, and continuity contract. |
| V1 remember/context | Personal memory can be written and retrieved through MCP. |
| V1 session restart | Stored personal memory survives a new server instance. |
| V1 cross-agent continuity | Agent A writes back scoped memory, the MCP server restarts, and Agent B retrieves it through the same lineage/scope. |
| V2 team handoff | Team context, handoff package, sync checksum, firewall, and audit are reachable. |
| V2 private-scope block | A second actor cannot read private memory through context, handoff, ontology, or sync paths. |
| citations and leaks | Context keeps citations while scope and secret leak counters remain zero. |

Run the full release gate before publishing changes:

```sh
./scripts/quality-gate.sh full
```

## MCP Hard Dogfood

For release-level confidence, run the hard MCP dogfood bundle. This drives the
actual `mneme-mcp` stdio process over JSON-RPC and reuses the same pressure
surfaces as V1 and V2:

```sh
scripts/mcp-hard-dogfood.py --check-contract
scripts/mcp-hard-dogfood.py --check-dataset
scripts/mcp-hard-dogfood.py --check-seeded-faults
scripts/mcp-hard-dogfood.py --out-dir /tmp/mneme-mcp-hard-dogfood --force
```

It covers:

| Surface | Coverage |
| --- | --- |
| V1 MCP hard corpus | 100 normal records, 150 adversarial records, 30 handoff workflows |
| V1 MCP ontology | 13 natural-language ontology cases |
| V2 MCP hard corpus | 120 team records, 80 adversarial records, 25 handoff workflows |
| Suite parity | `mcp` and `team` suites run against `mneme-mcp` |
| Fault detection | 9 seeded V1/V2 faults detected through the MCP eval target |

Full run outputs are local evidence bundles. Keep them out of git unless a
reduced public-safe finding is promoted into `evals/scenarios/`.

## Local-Only Handoff Dogfood Summary

A private local dogfood loop was also run outside the committed test tree. It
used the same public MCP/V2 surfaces but kept the runner, raw stores, local
client logs, and reduced real-session ledger under ignored local paths.

| Signal | Result |
| --- | --- |
| Scripted V2 MCP handoff episodes | `30/30` passed |
| Tested client surfaces | `protocol-stdio`, `codex`, `claude`, `cursor` |
| Recall@K / Precision@K / citation coverage | `1.00 / 1.00 / 1.00` |
| Scope, secret, and quarantine leaks | `0 / 0 / 0` |
| Reduced real-session summaries | `3/3` passed, no raw transcript included |
| Edge dogfood | 80 V1 concurrent writers, 24 V2 concurrent handoffs, 300 noisy records, injection guard, and MCP restart guard passed |

This is useful development evidence for the continuity path. It is not a claim
that full raw conversations are automatically shared across clients, and it is
not a third-party production benchmark.

## Real Client Smoke

The MCP server has also been checked through actual installed client CLIs using
isolated temporary homes, workspaces, and stores. This is a client integration
smoke test: it proves client-side MCP registration and discovery work without
mutating the user's real client config. Tool-call continuity is verified through
the stdio protocol smoke and MCP eval target.

| Client | Check | Result |
| --- | --- | --- |
| Direct MCP protocol | V1 cross-agent continuity after server restart | Passed |
| Direct MCP protocol | Missing end write-back guard | Passed |
| Direct MCP protocol | Wrong-scope and secret-context guards | Passed |
| Codex CLI | Isolated `codex mcp add/list/get` | Passed |
| Claude Code CLI | Isolated `claude mcp add/list/get`, health connected | Passed |
| Cursor Agent CLI | Workspace `.cursor/mcp.json`, approval, `list-tools` | Passed |

Raw logs are intentionally not committed because client logs can include local
paths, installed MCP server lists, or environment details. Public evidence
should stay at the reduced summary level above. To reproduce locally:

```sh
scripts/mcp-client-continuity-smoke.py --require-clients
```

The release quality gate runs the protocol-only path so CI does not depend on
which agent clients are installed:

```sh
scripts/mcp-client-continuity-smoke.py --protocol-only
```

## Continuity Contract

MCP makes Mneme reachable, but continuity still depends on client behavior. The
new V1 continuity tools make that behavior explicit:

```text
mneme_mcp_status
  -> verify install, store paths, tool inventory, and continuity contract

mneme_v1_continuity_begin
  -> start a session, read scoped context, and record lineage_id

mneme_v1_continuity_end
  -> close the session and write memory into the shared scope

mneme_v1_continuity_handoff
  -> package cited context and closed source sessions for the next agent
```

For two sequential agents to inherit context, they must use the same store and
the same `lineage` or `scope`. The committed MCP suite includes a cross-agent
scenario where `codex` writes scoped memory, the server is restarted, and
`claude-code` retrieves the remembered context with citations.

Client-side rule examples are included for:

- Codex: [`examples/codex/AGENTS.example.md`](../examples/codex/AGENTS.example.md)
- Claude Code:
  [`examples/claude-code/CLAUDE.example.md`](../examples/claude-code/CLAUDE.example.md)
- Cursor:
  [`examples/cursor/mneme-continuity-rule.mdc`](../examples/cursor/mneme-continuity-rule.mdc)

## Environment

`mneme-mcp` also accepts environment configuration:

```text
MNEME_MCP_MODE           personal, team, or all
MNEME_V1_STORE           v1 personal-memory JSON store
MNEME_STORE              fallback v1 store path
MNEME_TEAM_STORE         v2 team-memory JSON store
MNEME_TEAM_WORKSPACE_ID  workspace id for missing v2 stores
```

Command-line flags override the environment:

```sh
mneme-mcp --mode all --v1-store .mneme/mneme-v1.json --team-store .mneme/mneme-team-v2.json
```
