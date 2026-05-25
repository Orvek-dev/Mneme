# Mneme v2

Mneme v2 is the team-memory profile. It builds on the same local-first,
inspectable idea as v1, but the core question changes:

> Which memories can safely move from one person or agent to a team?

The current v2 implementation is a local team policy core plus connector-ready
boundary. It is ready for deterministic dogfood and public inspection, while
hosted sync, server deployment, and UI are still future work.

## What v2 Adds

- Team users with `admin`, `maintainer`, and `member` roles.
- Team-aware agents owned by users.
- Scopes for `team`, `private:<user>`, `project:<project>`, and
  `agent-private:<agent>`.
- Project membership checks before project memory is read or written.
- Reviewed promotion from private/project memory into team memory.
- Admin revocation for users and agents.
- Audit records for writes, reads, denials, promotion, and revoke actions.
- Secret-like memory blocking before context retrieval.
- Memory-poisoning-like text quarantine before context retrieval or sync.
- Connector-safe sync envelopes that omit private, agent-private, blocked, and
  quarantined memory.
- Agent handoff packages with context, sync payload, firewall report, and
- quality report, and ontology projection.
- Task-run lifecycle for `begin`, `note`, `end`, and run-anchored handoff.
- Quality reports for duplicate memory, active conflicts, pending promotions,
  promoted-source cleanup, and run state.
- Sync envelope IDs, stable checksums, and import diff summaries.
- Entity/relation/attribute ontology reports for team state.
- A thin stdio bridge for MCP-style agent runtime integration.
- A v2 eval suite and readiness gate for ACL leaks, secret leaks, promotion
  review, citation coverage, revoked-agent denial, sync privacy, handoff, run
  lifecycle, quality checks, checksum verification, and quarantine behavior.

## Quick Start

```sh
cargo run -p mneme-cli -- team init --admin alice
cargo run -p mneme-cli -- team user add bob --role member
cargo run -p mneme-cli -- team agent add codex-bob --owner bob
cargo run -p mneme-cli -- team project add atlas --member bob
cargo run -p mneme-cli -- team remember "Atlas deploys require rollback notes" \
  --actor bob \
  --agent codex-bob \
  --scope project:atlas
cargo run -p mneme-cli -- team promote team-memory-001 --actor bob --agent codex-bob
cargo run -p mneme-cli -- team review team-promotion-001 --actor alice --approve
cargo run -p mneme-cli -- team context "rollback notes" --actor alice --json
cargo run -p mneme-cli -- team handoff "rollback notes" --actor bob --agent codex-bob --json
cargo run -p mneme-cli -- team run begin "Atlas deploy handoff" \
  --actor bob \
  --agent codex-bob \
  --query "rollback notes" \
  --scope project:atlas \
  --json
cargo run -p mneme-cli -- team run end team-run-001 \
  --actor bob \
  --agent codex-bob \
  --summary "Rollback owner confirmed" \
  --next "Run smoke test" \
  --json
cargo run -p mneme-cli -- team run handoff team-run-001 \
  --actor bob \
  --agent codex-bob \
  --json
cargo run -p mneme-cli -- team sync export /tmp/mneme-team-sync.json \
  --actor bob \
  --agent codex-bob \
  --include-projects \
  --json
cargo run -p mneme-cli -- team firewall --json
cargo run -p mneme-cli -- team quality --json
cargo run -p mneme-cli -- team ontology --actor bob --agent codex-bob --json
```

Without `--store`, v2 writes to `.mneme/mneme-team-v2.json` in the current
directory. `.mneme/` is ignored by git.

## Validation

```sh
cargo run -p mneme-eval -- validate --suite team
cargo run -p mneme-eval -- run --suite team --target mneme-v2
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
cargo run -p mneme-eval -- v2-readiness --json --report evals/reports/v2-readiness.json
scripts/v2-team-dogfood.py --check-contract
scripts/v2-team-dogfood.py --check-dataset
scripts/v2-team-dogfood.py --check-seeded-faults
```

The readiness gate currently requires ten public-safe team scenarios and
seeded-fault detection for ACL bypass, secret leak, dropped citations,
unapproved promotion, ignored revocation, and quarantined-memory leakage.

## Current Boundary

Implemented:

- local JSON team store;
- Rust team-memory policy API in `mneme-core`;
- `mneme team ...` CLI;
- connector-safe sync export/import;
- agent handoff packages;
- task-run lifecycle and run-anchored handoff;
- team memory quality and promotion review reports;
- sync checksums and import diff summaries;
- firewall and ontology reports;
- CLI adapter manifest and `scripts/mneme-mcp-stdio.py`;
- `mneme-v2` eval target;
- team scenario suite and v2 readiness gate;
- v2 team dogfood evidence script.

Not implemented yet:

- hosted sync or server-backed storage beyond the local sync-envelope contract;
- team UI;
- multi-device conflict resolution;
- provider-specific extraction for v2 team notes;
- production auth integration.

See [Team Memory Core](team-memory-core.md) for the API and policy surface.
See [Use Cases](use-cases.md) for onboarding, handoff, promotion, sync,
firewall, and ontology recipes.
