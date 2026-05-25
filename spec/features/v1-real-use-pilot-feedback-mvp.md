# V1 Real-Use Pilot & Feedback Triage MVP

## Goal

Move from synthetic manual dogfood into private real-use pilot workflows without
letting private user data or local project text enter the public repository.

## Requirements

- Provide a local real-use pilot runner.
- Require the Phase 36 manual dogfood protocol by default before a real-use
  pilot workspace is prepared.
- Create an isolated local workspace, store, profile, runbook, and feedback
  template under ignored `evals/runs/v1-real-use-pilot/<run-label>/`.
- Define a stable feedback schema with category, severity, expected behavior,
  actual behavior, and next action fields.
- Sanitize derived feedback artifacts before writing issue drafts.
- Detect local paths and secret-like values and mark the triage decision as
  `blocked_private_feedback`.
- Provide a public-safe example feedback file.
- Keep full pilot execution local-only; CI verifies the contract, example
  feedback, and script syntax.

## Verification

- `scripts/v1-real-use-pilot.py --check-contract` reports the feedback schema.
- `scripts/v1-real-use-pilot.py --check-feedback examples/v1-real-use-feedback.example.json`
  reports `pilot_feedback_triaged`.
- A local pilot run can write an ignored `summary.json` with
  `ready_for_real_use_pilot` or `pilot_feedback_triaged`.
- `scripts/quality-gate.sh full` includes script syntax and contract checks.
- `scripts/public-safety-check.sh` passes with no generated pilot evidence
  tracked.
