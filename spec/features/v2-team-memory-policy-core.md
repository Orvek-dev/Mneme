# v2 Team Memory Policy Core

## Goal

Ship a public-safe local preview of Mneme Team: users, agents, projects, scoped
memory, reviewed promotion, revocation, audit, and deterministic eval gates.

## Requirements

- `REQ-V2-001`: A team store MUST distinguish users, agents, projects, memory,
  promotions, and audit records.
- `REQ-V2-002`: `team`, `private:<user>`, `project:<project>`, and
  `agent-private:<agent>` scopes MUST be checked before read/write.
- `REQ-V2-003`: Team memory promotion MUST require a pending candidate and an
  admin/maintainer review before team context can include it.
- `REQ-V2-004`: Secret-like text MUST be blocked from active context.
- `REQ-V2-005`: Revoked users and agents MUST be denied future context access.
- `REQ-V2-006`: Policy decisions MUST emit audit records.
- `REQ-V2-007`: Public evals MUST detect seeded ACL, secret, citation,
  promotion, and revocation faults.

## Verification

- `cargo test -p mneme-core --lib`
- `cargo test -p mneme-cli --lib`
- `cargo test -p mneme-eval --lib`
- `cargo run -p mneme-eval -- validate --suite team`
- `cargo run -p mneme-eval -- run --suite team --target mneme-v2`
- `cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2`
- `cargo run -p mneme-eval -- v2-readiness`
- `scripts/v2-team-dogfood.py --check-seeded-faults`
- `scripts/quality-gate.sh full`

## Public Safety

The feature uses only synthetic public fixtures. Local team stores live under
`.mneme/`, which is ignored by git.
