# Cursor MCP Example

This directory shows the public-safe Cursor Agent setup shape for Mneme's local
stdio MCP server.

## Workspace Config

Create `.cursor/mcp.json` in your project using this shape:

```json
{
  "mcpServers": {
    "mneme": {
      "command": "mneme-mcp",
      "args": [
        "--mode",
        "all",
        "--v1-store",
        ".mneme/mneme-v1.json",
        "--team-store",
        ".mneme/mneme-team-v2.json"
      ]
    }
  }
}
```

The same JSON is available as
[`mcp-config.example.json`](mcp-config.example.json).

For task-start/task-end continuity instructions, see
[`mneme-continuity-rule.mdc`](mneme-continuity-rule.mdc).

## Verify

From the workspace root:

```sh
cursor agent mcp enable mneme
cursor agent mcp list
cursor agent mcp list-tools mneme
```

The expected signals are `mneme: ready` and a tool list that includes:

```text
mneme_mcp_status
mneme_v1_continuity_begin
mneme_v1_continuity_end
mneme_v1_continuity_handoff
mneme_v2_team_handoff
```

## Continuity Prompt Shape

For sequential agent work, keep the same `lineage` and `scope` across agents:

```text
Agent A: mneme_v1_continuity_begin(...)
Agent A: mneme_v1_continuity_end(...)
Agent B: mneme_v1_continuity_handoff(...)
Agent B: mneme_v1_continuity_begin(...)
```

If `mneme_v1_continuity_end` is skipped, the next agent should not receive a
closed-session handoff.
