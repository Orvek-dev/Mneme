# v2 Connector Handoff Memory Firewall MVP

## Goal

Make Mneme v2 useful beyond a local team policy store by exposing safe
connection points for agent runtimes and future SaaS/storage integrations,
without committing to hosted infrastructure.

## Scope

- Export a `mneme.team_sync.v1` envelope that excludes private,
  agent-private, blocked-secret, and quarantined memory.
- Dry-run or apply sync envelopes with schema, workspace, conflict, firewall,
  and validation checks.
- Build `mneme.team_handoff.v1` packages with actor-scoped context, sync
  payload, firewall report, and ontology projection.
- Quarantine memory-poisoning-like text before context retrieval or sync.
- Expose a CLI adapter manifest and a thin stdio bridge for MCP-style agent
  runtimes.
- Add team eval scenarios for sync privacy, handoff, ontology, and quarantine.

## Non-Goals

- Hosted sync server.
- Web UI.
- Production auth provider integration.
- Real-time multi-device conflict resolution.

## Verification

- `cargo run -p mneme-eval -- validate --suite team`
- `cargo run -p mneme-eval -- run --suite team --target mneme-v2`
- `cargo run -p mneme-eval -- v2-readiness --json --report <path>`
- `scripts/mneme-mcp-stdio.py --self-test`
- `./scripts/quality-gate.sh full`
