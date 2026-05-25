# Eval Candidate Workflow

Eval candidates are local review artifacts for turning real failures into
future public scenarios. They are not runnable suite fixtures until a developer
reviews and promotes the nested `scenario` block.

## Generate

Create candidates from a failed baseline or eval report:

```sh
cargo run -p mneme-eval -- candidate evals/reports/openai-live-baseline.json \
  --out-dir evals/candidates/openai \
  --limit 3 \
  --prefix dogfood
```

The command writes one `*.candidate.yaml` file per failed scenario, ordered by
failed attempts and failed check count. Candidate files include:

- source report kind, target, suite, and scenario ID;
- failed check counts;
- redaction finding codes when the source report contained sensitive-looking
  patterns;
- a promotion checklist;
- a nested `scenario` block when the source scenario can be found locally.

Generated candidates under `evals/candidates/` are ignored by git. This keeps
local dogfood failures out of the public repository unless a scenario is
intentionally reviewed and promoted.

## Validate

Check candidates before sharing or promoting:

```sh
cargo run -p mneme-eval -- candidate-check evals/candidates/openai
```

`candidate-check` verifies the candidate schema, failed-check metadata,
promotion checklist, nested scenario validity, and absence of obvious secret or
local-path patterns after sanitization.

## Promote

Promote a reviewed candidate with an explicit dry run first:

```sh
cargo run -p mneme-eval -- candidate-promote \
  evals/candidates/openai/dogfood-example.candidate.yaml \
  --suite model \
  --filename dogfood-example.yaml
```

The command validates the candidate schema, nested scenario, destination path,
duplicate scenario IDs, and redaction scan. It does not write by default.

Apply the promotion only after review:

```sh
cargo run -p mneme-eval -- candidate-promote \
  evals/candidates/openai/dogfood-example.candidate.yaml \
  --suite model \
  --filename dogfood-example.yaml \
  --apply
```

`candidate-promote` writes only the nested `scenario` block to
`evals/scenarios/<suite>/<filename>`. It removes review-only tags such as
`candidate` and `needs-review`, then adds `promoted-candidate` for traceability.

Promotion checklist:

1. Confirm the candidate contains no private user data, project paths, or
   provider secrets.
2. Minimize the nested `scenario` block to the smallest public behavior that
   reproduces the failure.
3. Run `candidate-promote` without `--apply` and inspect the destination.
4. Run `mneme-eval validate` on the new scenario.
5. Run the relevant suite, baseline gate, and full quality gate before release.
