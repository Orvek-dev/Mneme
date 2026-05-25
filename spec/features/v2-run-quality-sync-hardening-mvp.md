# v2 Run Quality Sync Hardening MVP

## Goal

Make Mneme v2 useful as an agent-team operating layer, not only a scoped memory
store, by adding task runs, run handoff, memory-quality analysis, promotion
review reports, and connector-grade sync inspection.

## Requirements

- `REQ-V2-HARD-001`: A team store MUST persist task runs without breaking older
  stores that do not yet contain run records.
- `REQ-V2-HARD-002`: A run MUST support begin, note, end, and handoff actions
  under the same actor and scope policy as team memory.
- `REQ-V2-HARD-003`: Run handoff MUST include cited actor-scoped context,
  connector-safe sync, firewall, quality, ontology, and the run anchor.
- `REQ-V2-HARD-004`: Quality reports MUST identify duplicate active memory,
  conflicting active memory, pending promotion review, promoted-source cleanup,
  and open/closed run counts.
- `REQ-V2-HARD-005`: Promotion reports MUST expose review risks before a
  candidate becomes team-visible.
- `REQ-V2-HARD-006`: Sync exports MUST include an envelope ID and checksum, and
  sync imports MUST expose a deterministic dry-run/apply diff.
- `REQ-V2-HARD-007`: Public evals MUST cover run lifecycle, quality conflict
  detection, sync checksum verification, and full-output leak checks.

## Verification

- `cargo test -p mneme-core --lib`
- `cargo test -p mneme-cli --lib`
- `cargo test -p mneme-eval --lib`
- `cargo run -p mneme-eval -- validate --suite team`
- `cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2`
- `cargo run -p mneme-eval -- v2-readiness`
- `scripts/mneme-mcp-stdio.py --self-test`
- `./scripts/quality-gate.sh full`

## Public Safety

The feature uses only synthetic fixtures. Generated run bundles and local team
stores remain ignored by git.
