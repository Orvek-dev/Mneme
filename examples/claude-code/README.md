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

## Optional Stop Hook

If you use Mneme outcome gates, wire the local wrapper as a Claude Code Stop
hook so failed gates can block premature completion and feed the failed
criteria back into the same session:

```sh
scripts/mneme-agent-hook.sh stop
```

The wrapper reads Claude Code Stop-hook JSON on stdin. It exits without
blocking when `stop_hook_active=true` or when `MNEME_LOOP_MAX_ATTEMPTS` has
already been reached for the same `last_gate_failure_id`.

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
