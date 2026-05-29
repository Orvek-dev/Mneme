# mneme-mcp

`mneme-mcp` is Mneme's local stdio MCP server. It gives MCP-capable coding
agents access to the same local-first memory behavior as the CLI:

- V1 personal memory tools for one user's agent workflow;
- V2 team handoff tools with users, agents, projects, scopes, firewall,
  quality, sync, ontology, and validation;
- local JSON stores under `.mneme/` by default;
- no hosted service requirement.

## Build

```sh
cargo build -p mneme-cli -p mneme-mcp -p mneme-eval
cargo run -p mneme-mcp -- --self-test
```

The self-test verifies startup and tool registration. It does not write project
memory into the repository.

## Run

```sh
mneme-mcp \
  --mode all \
  --v1-store .mneme/mneme-v1.json \
  --team-store .mneme/mneme-team-v2.json
```

Modes:

| Mode | Tools |
| --- | --- |
| `personal` | V1 tools only |
| `team` | V2 tools only |
| `all` | V1 and V2 tools |

## Client Config

Generate snippets for supported clients:

```sh
mneme mcp config --client all
```

Client examples:

- [Codex](../../examples/codex/README.md)
- [Claude Code](../../examples/claude-code/README.md)
- [Cursor](../../examples/cursor/README.md)

The examples use explicit workspace-local stores so memory remains outside git.

## Tool Surface

V1 tools:

- `mneme_mcp_status`
- `mneme_v1_remember`
- `mneme_v1_ingest`
- `mneme_v1_context`
- `mneme_v1_begin`
- `mneme_v1_end`
- `mneme_v1_continuity_begin`
- `mneme_v1_continuity_end`
- `mneme_v1_continuity_handoff`
- `mneme_v1_forget`
- `mneme_v1_correct`
- `mneme_v1_quality`
- `mneme_v1_validate`
- `mneme_v1_snapshot`

V2 tools:

- team setup: `mneme_v2_team_init`, `mneme_v2_user_add`,
  `mneme_v2_agent_add`, `mneme_v2_project_add`, `mneme_v2_project_grant`;
- memory and handoff: `mneme_v2_team_remember`,
  `mneme_v2_team_context`, `mneme_v2_team_handoff`;
- run lifecycle: `mneme_v2_run_begin`, `mneme_v2_run_note`,
  `mneme_v2_run_end`, `mneme_v2_run_handoff`;
- review and sync: `mneme_v2_promote`, `mneme_v2_promotion_report`,
  `mneme_v2_review`, `mneme_v2_sync_export`, `mneme_v2_sync_import`;
- safety and inspection: `mneme_v2_firewall`, `mneme_v2_quality`,
  `mneme_v2_ontology`, `mneme_v2_revoke_user`, `mneme_v2_revoke_agent`,
  `mneme_v2_validate`, `mneme_v2_snapshot`.

## Evaluation

MCP is tested as a real stdio boundary, not only as a Rust library:

```sh
cargo run -p mneme-eval -- validate --suite mcp
cargo run -p mneme-eval -- run --suite mcp --target mneme-mcp
scripts/mcp-hard-dogfood.py --out-dir /tmp/mneme-mcp-hard-dogfood --force
```

Current public-safe MCP coverage includes installation/status checks, V1
write/read, V1 restart persistence, V1 cross-agent continuity, V2 handoff, V2
private-scope blocking, citation checks, scope leak checks, secret leak checks,
hard dogfood corpora, and seeded fault detection.

Local-only development dogfood has also exercised 30 scripted V2 MCP handoff
episodes across `protocol-stdio`, Codex, Claude Code, and Cursor smoke
surfaces. The reduced public-safe summary from the latest run was `30/30`
episodes passed with Recall@K `1.00`, Precision@K `1.00`, citation coverage
`1.00`, and zero scope/secret/quarantine leaks. The runner, raw stores, client
logs, and real-session ledger are intentionally not committed.

Local edge dogfood also covered 80 concurrent V1 writers, 24 concurrent V2
handoffs, 300 noisy records, prompt-injection context blocking, and MCP restart
guards. Those fixtures remain local-only; only the reduced summary is published.

## V1 Continuity Flow

Sequential agents should use the continuity tools rather than relying on
best-effort memory calls:

```text
Agent A: mneme_v1_continuity_begin(lineage, scope)
Agent A: work
Agent A: mneme_v1_continuity_end(session_id, remember, scope)
Agent B: mneme_v1_continuity_handoff(lineage, scope)
Agent B: mneme_v1_continuity_begin(lineage, scope)
```

The MCP eval suite verifies this with a cross-agent scenario: `codex` writes
back scoped memory, the server restarts, and `claude-code` retrieves the cited
context from the same lineage/scope.

## Real Client Smoke

Mneme MCP has also been smoke-tested through actual installed client CLIs with
isolated temporary config homes and stores:

| Client | Check | Result |
| --- | --- | --- |
| Direct MCP protocol | V1 cross-agent continuity after server restart | Passed |
| Direct MCP protocol | Missing end, wrong scope, and secret-context guards | Passed |
| Codex CLI | Isolated `codex mcp add/list/get` | Passed |
| Claude Code CLI | Isolated `claude mcp add/list/get`, health connected | Passed |
| Cursor Agent CLI | Workspace approval and `list-tools` with 38 tools | Passed |

This is a client integration smoke test, not an external production benchmark.
Raw client logs are intentionally not committed because they may include local
paths or environment details.

Reproduce locally:

```sh
scripts/mcp-client-continuity-smoke.py --require-clients
```
