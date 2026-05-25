# V1 Dogfood Triage

`mneme-eval dogfood-summary <bundle-dir>` checks whether a bundle produced by
`scripts/v1-dogfood.sh` is usable as deterministic evidence for manual v1
dogfood review.

## Command

```sh
scripts/v1-dogfood.sh
cargo run -p mneme-eval -- dogfood-summary evals/runs/v1-dogfood/<run-label> \
  --json \
  --report evals/runs/v1-dogfood/<run-label>/dogfood-summary.json
```

The command returns `ready_for_manual_dogfood` only when all required artifacts
are present and passing:

- `summary.json`;
- `v1-readiness.json`;
- `dogfood.validate.json`;
- `dogfood.run.fake.json`;
- `dogfood.run.mneme-v1.json`;
- `dogfood.acceptance.mneme-v1.json`;
- `cli.doctor.post.json`;
- `cli.context.json`;
- `cli.quality.json`;
- `cli.validate.txt`.

## Decision

Use `decision_status` as the release/product signal:

- `ready_for_manual_dogfood`: deterministic dogfood evidence is complete.
- `blocked`: at least one required artifact is missing, malformed, or failing.

This triage is still deterministic. It does not replace live provider baselines
or human product judgment; it prevents manual dogfood from starting with a
known-bad evidence bundle.
