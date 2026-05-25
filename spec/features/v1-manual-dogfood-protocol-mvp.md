# V1 Manual Dogfood Protocol MVP

## Goal

Run Mneme v1 through a repeatable local dogfood protocol that is closer to real
personal use than unit tests, while keeping all committed data public-safe.

## Requirements

- Provide a local runner for the manual dogfood protocol.
- Generate exactly 100 synthetic memory records.
- Execute exactly 25 workflow checks over an isolated v1 store.
- Cover recall, scoped retrieval, ranking caps, correction, forgetting,
  quality review, redaction, curation, restore, export/import, and agent/hook
  begin/end flows.
- Run the deterministic v1 dogfood evidence preflight by default and require
  `ready_for_manual_dogfood` before executing manual workflows.
- Write ignored evidence under `evals/runs/v1-manual-dogfood/<run-label>/`.
- Keep full manual dogfood local-only; CI should verify dataset shape and
  script syntax without running the full protocol.
- Prevent compaction/restore from causing new event, claim, or session ID
  collisions with retained records.

## Verification

- `scripts/v1-manual-dogfood.py --check-dataset` reports 100 records and 25
  workflows.
- A full local run reports `decision_status=v1_manual_dogfood_passed`.
- Unit coverage proves new IDs remain collision-free after compaction.
- `scripts/quality-gate.sh full` compiles the runner and checks dataset shape.
- `scripts/public-safety-check.sh` passes with no private template files or
  local evidence tracked.
