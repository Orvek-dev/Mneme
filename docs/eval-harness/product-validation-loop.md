# Product Validation Loop

`scripts/product-validation-loop.py` is the P1-P6 gate for Mneme's product
validation risks. It is not another extraction benchmark and it is not a
third-party adoption claim. It asks whether Mneme's memory layer can influence
downstream artifacts in a cited, scoped, and reviewable way while preserving
local-first constraints.

## Phases

| Phase | Purpose | What must hold |
| --- | --- | --- |
| P1 | Causal memory usefulness | Retrieved memory is adopted into a generated task artifact with citations; the gate no longer treats an empty-store comparison as proof of value. |
| P2 | Privacy/cost extraction readiness | Provider extraction remains explicit opt-in, no live provider call runs in the gate, secret-like text is prefiltered locally, and sample events stay inside token/latency budgets. |
| P3 | Long-horizon lifecycle | Actual CLI `remember`, `correct`, and `forget` operations run before noisy accumulation; current memory is recalled without stale reuse, forgotten recall, or scope leaks. |
| P4 | Retrieval ranking decision | A semantic-ranking candidate must beat term ranking before embedding/vector complexity is allowed. The current candidate is an alias probe, not embedding proof. |
| P5 | Migration safety | Legacy stores normalize to the current schema with backup, migration history, and memory preservation. |
| P6 | External review gate | A public-safe review schema must validate before Mneme can claim real-world or third-party value evidence. The committed example is not itself third-party proof. |

## Commands

Inspect the contract:

```sh
scripts/product-validation-loop.py --check-contract
```

Inspect the public-safe dataset shape:

```sh
scripts/product-validation-loop.py --check-dataset
```

Run the local loop:

```sh
scripts/product-validation-loop.py \
  --run-label local-product-validation \
  --out-dir /tmp/mneme-product-validation \
  --force
```

The full run writes `summary.json`, `report.md`, and local task artifacts under
the output directory. Default repository output is ignored by git:

```text
evals/runs/product-validation-loop/<run-label>/
```

## Interpretation

Use the results this way:

- If P1 fails, do not add new extractor or retrieval features; fix memory
  adoption and harmful-memory handling first.
- If P2 fails, do not make provider-backed extraction easier to enable.
- If P3 fails, improve lifecycle, curation, or ranking before increasing memory
  volume.
- If P4 shows no ranking delta, do not add embedding/vector complexity.
- If P5 fails, do not change storage format or extractor protocol.
- If P6 fails, do not publish real-use or third-party value claims.

Public README summaries should use reduced metrics only. Raw full-run bundles,
local paths, and real work ledgers stay out of git.
