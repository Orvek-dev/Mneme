# V1 Hard Dogfood

`scripts/v1-hard-dogfood.py` runs the hard-mode v1 dogfood protocol. It is
local-only evidence for stressing Mneme v1 as an agent memory gateway, not a
routine CI job.

## Dataset

The runner combines:

- 100 normal records from the manual dogfood dataset;
- 150 adversarial records across scope confusion, needle-in-noise, stale
  correction, agent handoff, attribution traps, poisoning traps, and synthetic
  secret-like claims;
- 30 agent handoff workflows using `begin`, `end`, and follow-up context recall.

Check the shape without running the full CLI workload:

```sh
scripts/v1-hard-dogfood.py --check-dataset
```

## Scorecard

The hard-mode scorecard reports:

- `recall_at_k`;
- `precision_at_k`;
- `scope_leak_count`;
- `secret_leak_count`;
- `citation_coverage`;
- `handoff_success_rate`;
- `agent_attribution_error_count`;
- `stale_reuse_count`;
- `agent_memory_score`.

The default gates require no scope leak, no secret leak, no attribution error,
no stale reuse, full citation coverage, and at least 95% recall, precision, and
handoff success.

## Seeded Faults

The runner includes static seeded faults for detector coverage:

- dropped citation;
- scope leak;
- secret leak;
- stale memory reuse;
- handoff miss.

These faults generate local candidate artifacts even when the real run passes,
so the promotion loop can be inspected without private data.

```sh
scripts/v1-hard-dogfood.py --check-seeded-faults
```

## Run

Run the full protocol from the repository root:

```sh
scripts/v1-hard-dogfood.py
```

By default, the runner first executes `scripts/v1-dogfood.sh` and requires its
dogfood summary decision to be `ready_for_manual_dogfood`. The evidence bundle
is ignored by git and written under:

```text
evals/runs/v1-hard-dogfood/<run-label>/
```

The bundle includes `summary.json`, `scorecard.json`, `regression.json`,
`trend.json`, `trend.md`, `seeded-faults.json`, `report.md`, `report.html`,
command artifacts, local candidate artifacts, official candidate YAML files,
and a `candidate-check` report for the official candidates.

## Candidate Bridge

Each seeded fault and failed hard workflow is written twice:

- a local JSON triage artifact under `candidates/`;
- an official `mneme.eval_candidate.v1` YAML artifact under
  `candidates/official/`.

The runner validates the official directory with `mneme-eval candidate-check`
and writes the result to:

```text
candidates/official-candidate-check.json
```

The official candidates intentionally omit the final `scenario` block. A user
must still minimize the hard-mode finding into a public scenario before running
`mneme-eval candidate-promote`.

## Trend History

The runner writes one public-safe history entry per run. By default this is
inside the ignored run bundle. Use `--history-dir <dir>` to keep a longer local
history across runs:

```sh
scripts/v1-hard-dogfood.py --history-dir evals/runs/v1-hard-dogfood-history
```

`trend.json` compares the current scorecard with the latest passing history
entry and reports metric deltas plus hard-mode regressions.

## Decision

Use `decision_status` as the hard-mode product signal:

- `v1_hard_dogfood_passed`: all 30 workflows passed and all regression gates
  passed.
- `blocked`: at least one workflow, detector, or regression gate failed.

## Cost Policy

The full runner executes hundreds of local CLI commands and writes a large
evidence bundle. CI checks only Python syntax, the hard-mode contract, dataset
shape, seeded-fault coverage, official candidate bridge, and trend contract to
avoid unnecessary GitHub Actions cost.
