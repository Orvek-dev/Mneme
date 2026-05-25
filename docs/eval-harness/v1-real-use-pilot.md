# V1 Real-Use Pilot

`scripts/v1-real-use-pilot.py` prepares a local, private pilot workspace after
the structured v1 dogfood gates have passed. It also sanitizes and triages
pilot feedback so only public-safe findings become issues, fixes, docs, or
future eval candidates.

## Contract

Inspect the feedback schema without creating a pilot bundle:

```sh
scripts/v1-real-use-pilot.py --check-contract
```

Feedback uses this shape:

```json
{
  "schema_version": 1,
  "pilot_label": "local-v1-real-use",
  "findings": [
    {
      "id": "pilot-001",
      "title": "Context recall missed release evidence preference",
      "category": "recall_miss",
      "severity": "medium",
      "summary": "Public-safe behavior summary.",
      "expected": "The expected behavior.",
      "actual": "The observed behavior.",
      "next_action": "candidate"
    }
  ]
}
```

Categories are `recall_miss`, `wrong_memory`, `irrelevant_context`,
`privacy_redaction`, `scope_leak`, `workflow_friction`, `docs_gap`,
`performance`, `cli_ux`, and `other`.

## Pilot Setup

Run the pilot setup locally:

```sh
scripts/v1-real-use-pilot.py
```

By default, this runs the Phase 36 manual dogfood protocol first and requires
`v1_manual_dogfood_passed`. It then creates an isolated workspace under:

```text
evals/runs/v1-real-use-pilot/<run-label>/
```

That directory is ignored by git and contains:

- `summary.json`;
- `pilot-runbook.md`;
- `feedback-template.json`;
- command artifacts under `commands/`;
- sanitized feedback and issue drafts under `reports/` when feedback is
  supplied.

## Feedback Triage

Check a feedback file without creating a pilot bundle:

```sh
scripts/v1-real-use-pilot.py --check-feedback examples/v1-real-use-feedback.example.json
```

Attach feedback to a pilot run:

```sh
scripts/v1-real-use-pilot.py \
  --feedback /path/to/local-feedback.json
```

The script writes only sanitized derived artifacts. If it finds local paths or
secret-like values, the decision becomes `blocked_private_feedback`; edit the
source feedback before opening public issues or creating eval candidates.

## Decision

Use `decision_status` in `summary.json`:

- `ready_for_real_use_pilot`: workspace is ready, no feedback supplied yet.
- `pilot_feedback_triaged`: supplied feedback is schema-valid and public-safe.
- `blocked_private_feedback`: supplied feedback required redaction.
- `blocked`: pilot setup failed.

Full pilot evidence is local-only. CI only checks the contract, example
feedback, and script syntax to avoid unnecessary GitHub Actions cost.
