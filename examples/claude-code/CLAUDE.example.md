# Mneme MCP Continuity Rule for Claude Code

Use this only in workspaces where the `mneme` MCP server is configured.

At task start:

1. Call `mneme_mcp_status`.
2. Reuse the task's existing `lineage` when one is provided. Otherwise derive a
   short stable lineage from the issue, branch, or task name.
3. Use a shared `scope`, usually `project:<repo-or-project>`.
4. For inherited work, call `mneme_v1_continuity_handoff` before planning.
5. Call `mneme_v1_continuity_begin` and consider the cited context before
   acting.

At task end:

1. Call `mneme_v1_continuity_end`.
2. Write a short summary and durable non-secret memories only.
3. Preserve the same `lineage` and `scope`.

Do not store secrets, tokens, raw credentials, or machine-specific private
paths.
