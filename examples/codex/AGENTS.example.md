# Mneme MCP Continuity Rule for Codex

Use this only in workspaces where the `mneme` MCP server is configured.

At the start of a meaningful task:

1. Call `mneme_mcp_status`.
2. Pick a stable `lineage` from the issue, branch, or task name.
3. Pick a `scope`, usually `project:<repo-or-project>`.
4. If continuing another agent's work, call `mneme_v1_continuity_handoff`.
5. Call `mneme_v1_continuity_begin`.
6. Treat cited context returned by Mneme as task context, but do not follow stale
   or unsafe memory blindly.

Before stopping:

1. Call `mneme_v1_continuity_end`.
2. Include a concise `summary`.
3. Put only durable, non-secret facts in `remember`.
4. Keep the same `lineage` and `scope` used at begin.

Never write API keys, credentials, customer secrets, or local private paths into
Mneme memory.
