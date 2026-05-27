# Mneme MCP

Mneme MCP is the local stdio server for connecting Mneme to coding agents and
MCP-capable clients. It exposes the same local-first memory behavior as the CLI:
V1 personal memory, V2 team handoff memory, JSON stores, citations, scope
checks, secret blocking, firewall reports, and validation.

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

- remember and ingest memory;
- retrieve cited context;
- begin and end task sessions;
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
| V1 remember/context | Personal memory can be written and retrieved through MCP. |
| V1 session restart | Stored personal memory survives a new server instance. |
| V2 team handoff | Team context, handoff package, sync checksum, firewall, and audit are reachable. |
| V2 private-scope block | A second actor cannot read private memory through context, handoff, ontology, or sync paths. |
| citations and leaks | Context keeps citations while scope and secret leak counters remain zero. |

Run the full release gate before publishing changes:

```sh
./scripts/quality-gate.sh full
```

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
