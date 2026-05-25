# Mneme v2 Use Cases

Mneme v2 is useful when a team uses agents across projects and needs memory to
move only when policy allows it.

## Project Onboarding

A maintainer stores project rules as `project:<project>` or reviewed `team`
memory. A new teammate or agent asks for a handoff package and receives only the
rules they can read, with citations.

```sh
mneme team handoff "atlas onboarding deploy rules" \
  --actor bob \
  --agent codex-bob \
  --json
```

## Agent Handoff

One agent leaves project-scoped findings. The next agent receives a context
pack plus a connector-safe sync envelope. Private scratchpad notes and
agent-private memory stay out of the package.

```sh
mneme team remember "Atlas handoff notes require test command" \
  --actor bob \
  --agent codex-bob \
  --scope project:atlas
mneme team handoff "handoff test command" --actor bob --agent codex-bob --json
```

## Promotion Review

Members can propose a memory for team-wide reuse. Admins or maintainers approve
it after review. Until approval, the memory remains in its original scope.

```sh
mneme team promote team-memory-001 --actor bob --agent codex-bob
mneme team review team-promotion-001 --actor alice --approve
```

## Connector Sync

External tools can pull a sync envelope without receiving private,
agent-private, blocked-secret, or quarantined records.

```sh
mneme team sync export /tmp/mneme-team-sync.json \
  --actor bob \
  --agent codex-bob \
  --include-projects \
  --json
mneme team sync import /tmp/mneme-team-sync.json --json
mneme team sync import /tmp/mneme-team-sync.json --apply --actor alice --json
```

## Memory Firewall

The firewall scan is intended for release gates and adapter smoke tests. It
fails only when active memory still contains secret-like or instruction
override-like text.

```sh
mneme team firewall --json
```

## Ontology Projection

The ontology report gives external tools a simple actor-scoped entity,
relation, and attribute map without requiring a graph database. Without
`--actor`, memory labels are redacted for public-safe inspection.

```sh
mneme team ontology --actor bob --agent codex-bob --json
```
