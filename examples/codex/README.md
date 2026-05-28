# Codex MCP Example

This directory shows how to connect Codex to Mneme's local stdio MCP server.
Mneme keeps memory in workspace-local JSON stores under `.mneme/`, which is
ignored by git.

## Config Snippet

Generate a fresh snippet from your local install:

```sh
mneme mcp config \
  --client codex \
  --mode all \
  --mcp-bin mneme-mcp \
  --v1-store .mneme/mneme-v1.json \
  --team-store .mneme/mneme-team-v2.json
```

Equivalent Codex TOML shape, also available as
[`mcp-config.example.toml`](mcp-config.example.toml):

```toml
[mcp_servers.mneme]
command = "mneme-mcp"
args = [
  "--mode", "all",
  "--v1-store", ".mneme/mneme-v1.json",
  "--team-store", ".mneme/mneme-team-v2.json",
]
```

Use an absolute `command` path if `mneme-mcp` is not on your `PATH`.

For task-start/task-end continuity instructions, see
[`AGENTS.example.md`](AGENTS.example.md).

## Smoke Test Shape

The public Codex smoke test verifies registration in an isolated `CODEX_HOME`.
Protocol tool calls are covered by the shared MCP client smoke and eval suite.

| Check | Expected behavior |
| --- | --- |
| `codex mcp add` | Mneme can be registered without mutating the user's real Codex config. |
| `codex mcp list` | The isolated server appears as enabled. |
| `codex mcp get mneme` | The server uses stdio with the expected command and args. |
| V1 continuity protocol | Writer memory survives server restart and is read by a reader agent. |
| Guardrails | Missing end, wrong scope, and secret-like memory are blocked from handoff context. |

The latest local run used isolated temporary stores and passed. The
public-safe summary shape is captured in
[`../mcp-client-smoke/summary.example.json`](../mcp-client-smoke/summary.example.json).
Raw logs are not committed because MCP client logs can include local paths or
environment details.
