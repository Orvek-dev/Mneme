# V1 Dogfood Execution

`scripts/v1-dogfood.sh` runs the deterministic v1 dogfood loop and writes an
ignored evidence bundle. Use it when a build should be treated as a concrete v1
dogfood candidate, not just as a passing unit-test run.

## Command

```sh
scripts/v1-dogfood.sh
```

By default, reports are written to:

```text
evals/runs/v1-dogfood/<run-label>/
```

`evals/runs/` is ignored by git, so local evidence can include generated JSON
reports and temporary stores without becoming public repository content.

## Configuration

- `MNEME_DOGFOOD_RUN_LABEL`: optional label. Defaults to
  `local-YYYYMMDD-HHMMSS`.
- `MNEME_DOGFOOD_OUT_DIR`: optional output directory. Defaults to
  `evals/runs/v1-dogfood/<run-label>`.

Run labels may contain only letters, digits, `-`, `_`, `.`, and `/`.

## Evidence

The bundle includes:

- `dogfood.validate.json`;
- `dogfood.run.fake.json`;
- `dogfood.run.mneme-v1.json`;
- `dogfood.acceptance.mneme-v1.json`;
- `v1-readiness.json`;
- CLI smoke reports for `doctor`, `init`, `remember`, `begin`, `end`,
  `context`, `quality`, and `validate`;
- `summary.json` with the command, run label, status, output directory, and
  report paths.

The script exits non-zero if any dogfood, readiness, or CLI smoke step fails.
Provider/model extraction is intentionally excluded; use the baseline workflow
for provider experiments.
