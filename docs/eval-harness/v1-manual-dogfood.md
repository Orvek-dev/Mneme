# V1 Manual Dogfood

`scripts/v1-manual-dogfood.py` runs the structured v1 dogfood protocol with
public-safe synthetic data. It is intended for local product evidence, not for
routine CI.

## Dataset

The runner generates:

- 100 synthetic memory records;
- 25 workflow checks;
- private, `project-alpha`, and `project-beta` scopes;
- normal active memories, correction seeds, forget seeds, duplicate memories,
  and fake secret-like values.

The data is intentionally synthetic so evidence can be discussed publicly
without copying private memory into the repository.

```sh
scripts/v1-manual-dogfood.py --check-dataset
```

## Run

Run the full protocol from the repository root:

```sh
scripts/v1-manual-dogfood.py
```

By default, the runner first executes `scripts/v1-dogfood.sh` and requires its
`dogfood-summary.json` decision to be `ready_for_manual_dogfood`. It then
creates an isolated workspace and store under:

```text
evals/runs/v1-manual-dogfood/<run-label>/
```

That directory is ignored by git. The summary is written to:

```text
evals/runs/v1-manual-dogfood/<run-label>/summary.json
```

## Decision

Use `decision_status` as the product signal:

- `v1_manual_dogfood_passed`: the 100 records were ingested and all 25
  workflows passed.
- `blocked`: at least one workflow or command artifact failed.

The workflow set covers recall, scoped retrieval, max-item ranking, correction,
forgetting, quality review, redaction, curation, restore, export/import, and
agent/hook begin/end usage.

## Cost Policy

The full runner is local-only because it executes many CLI commands and writes
large evidence bundles. CI verifies only that the dataset shape remains 100
records and 25 workflows, plus Python syntax, to avoid unnecessary GitHub
Actions cost.
