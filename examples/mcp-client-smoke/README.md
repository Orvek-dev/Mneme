# MCP Client Smoke Example

This directory contains a public-safe summary shape for Mneme's client MCP
smoke test. The real smoke runner uses isolated temporary stores and temporary
client homes, then prints a reduced JSON report.

Run locally:

```sh
scripts/mcp-client-continuity-smoke.py --require-clients
```

For CI or environments without Codex, Claude Code, or Cursor installed, use the
protocol-only path:

```sh
scripts/mcp-client-continuity-smoke.py --protocol-only
```

Raw logs are intentionally not committed because client output can include
local paths and machine-specific config locations.
