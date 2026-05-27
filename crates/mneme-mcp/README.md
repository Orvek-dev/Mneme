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

For Codex, see [examples/codex](../../examples/codex/README.md). The example
uses explicit workspace-local stores so memory remains outside git.

## Tool Surface

V1 tools:

- `mneme_v1_remember`
- `mneme_v1_context`
- `mneme_v1_begin`
- `mneme_v1_end`
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

Current public-safe MCP coverage includes V1 write/read, V1 restart
persistence, V2 handoff, V2 private-scope blocking, citation checks, scope leak
checks, secret leak checks, hard dogfood corpora, and seeded fault detection.

## Real Codex Smoke

Mneme MCP has also been smoke-tested through actual Codex CLI execution with
isolated temporary stores:

| Client | Check | Result |
| --- | --- | --- |
| Codex CLI | V1 MCP write/read | Passed |
| Codex CLI | V2 team handoff | Passed |
| Codex CLI | V2 wrong agent-owner denial | Passed |

This is a client integration smoke test, not an external production benchmark.
Raw client logs are intentionally not committed because they may include local
paths or environment details.
