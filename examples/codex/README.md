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

## Smoke Test Shape

The public smoke test used for Codex verifies:

| Check | Expected behavior |
| --- | --- |
| V1 write/read | Codex calls Mneme MCP, stores one memory, and retrieves cited context. |
| V2 handoff | Codex calls Mneme MCP, writes project-scoped team memory, and retrieves a handoff package. |
| V2 wrong owner denial | A user cannot operate another user's owned agent. |

The latest local run used isolated temporary stores and passed all three
checks. The public-safe summary shape is captured in
[`smoke-summary.example.json`](smoke-summary.example.json). Raw logs are not
committed because MCP client logs can include local paths or environment
details.
