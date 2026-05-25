# Mneme v2

Mneme v2 is the team-memory profile. It builds on the same local-first,
inspectable idea as v1, but the core question changes:

> Which memories can safely move from one person or agent to a team?

The current v2 implementation is a local team policy core and CLI preview. It
is ready for deterministic dogfood and public inspection, while hosted sync,
server deployment, and UI are still future work.

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
- A v2 eval suite and readiness gate for ACL leaks, secret leaks, promotion
  review, citation coverage, and revoked-agent denial.

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

The readiness gate currently requires six public-safe team scenarios and
seeded-fault detection for ACL bypass, secret leak, dropped citations,
unapproved promotion, and ignored revocation.

## Current Boundary

Implemented:

- local JSON team store;
- Rust team-memory policy API in `mneme-core`;
- `mneme team ...` CLI;
- `mneme-v2` eval target;
- team scenario suite and v2 readiness gate;
- v2 team dogfood evidence script.

Not implemented yet:

- hosted sync or server-backed storage;
- team UI;
- multi-device conflict resolution;
- provider-specific extraction for v2 team notes;
- production auth integration.

See [Team Memory Core](team-memory-core.md) for the API and policy surface.
