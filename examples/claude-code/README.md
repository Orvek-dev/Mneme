# Claude Code MCP Example

This directory shows the public-safe Claude Code setup shape for Mneme's local
stdio MCP server.

## Add Mneme

Generate a command from your local install:

```sh
mneme mcp config \
  --client claude-code \
  --mode all \
  --mcp-bin mneme-mcp \
  --v1-store .mneme/mneme-v1.json \
  --team-store .mneme/mneme-team-v2.json
```

Equivalent command:

```sh
claude mcp add --transport stdio --scope user mneme -- \
  mneme-mcp \
  --mode all \
  --v1-store .mneme/mneme-v1.json \
  --team-store .mneme/mneme-team-v2.json
```

Use an absolute `mneme-mcp` path if it is not on your `PATH`.

For task-start/task-end continuity instructions, see
[`CLAUDE.example.md`](CLAUDE.example.md).

## Verify

```sh
claude mcp list
claude mcp get mneme
```

The expected health signal is `Connected`.

## Continuity Prompt Shape

At the start of a task, ask Claude Code to call:

```text
mneme_v1_continuity_begin(lineage, scope, task, query)
```

At the end, ask it to call:

```text
mneme_v1_continuity_end(session_id, lineage, scope, summary, remember)
```

Before handing work to another agent, ask the next agent to call:

```text
mneme_v1_continuity_handoff(lineage, scope, query)
mneme_v1_continuity_begin(lineage, scope, task, query)
```

The important invariant is that both agents use the same store and the same
`lineage` or `scope`.
