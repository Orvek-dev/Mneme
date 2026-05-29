# Product Validation Loop

`scripts/product-validation-loop.py` is the P0-P5 gate for the red-team
findings that came after Mneme `v0.66.0`. It is not another extraction
benchmark. It asks whether Mneme's memory layer can produce useful downstream
behavior while preserving the product's local-first constraints.

## Phases

| Phase | Purpose | What must hold |
| --- | --- | --- |
| P0 | Real-use value dogfood | Memory changes scripted downstream decisions compared with an empty-store control. |
| P1 | Downstream usefulness | The scorecard measures outcome delta and wrong-memory count, not only extraction F1. |
| P2 | Privacy-preserving extraction | Provider-backed extraction remains explicit opt-in; dry-run works without keys; secret prefilter runs before provider calls. |
| P3 | Long-horizon memory | Current memory is recalled under noisy accumulation without stale reuse or scope leaks. |
| P4 | Retrieval ranking decision | A semantic candidate must beat term ranking before semantic search is worth shipping. |
| P5 | Migration safety | Legacy stores normalize to the current schema with backup and migration history preserved. |

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

The full run writes `summary.json` and `report.md` under the output directory.
Default repository output is ignored by git:

```text
evals/runs/product-validation-loop/<run-label>/
```

## Interpretation

This loop is a product-validation signal, not a market-adoption claim. It proves
that the local Mneme runtime can preserve scoped, cited memory across scripted
downstream tasks and that the next risky features are gated before being added.

Use the results this way:

- If P0/P1 fail, do not add new extractor or retrieval features; fix memory
  usefulness first.
- If P2 fails, do not make provider-backed extraction easier to enable.
- If P3 fails, improve lifecycle, curation, or ranking before increasing memory
  volume.
- If P4 shows no ranking delta, do not add embedding/vector complexity.
- If P5 fails, do not change storage format or extractor protocol.

Public README summaries should use reduced metrics only. Raw full-run bundles,
local paths, and real work ledgers stay out of git.
