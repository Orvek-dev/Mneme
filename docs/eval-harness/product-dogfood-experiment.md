# Product Dogfood Experiment

`scripts/product-dogfood-experiment.py` prepares a public-safe, blindable
Mneme ON/OFF dogfood bundle. It does not execute an agent and it does not score
productivity by itself.

## Purpose

The bundle exists to stop product-value checks from becoming scripted
self-scoring. It creates paired task conditions:

- `mneme_on`: a temporary local store contains relevant memory.
- `mneme_off`: the prompt must solve the same task without Mneme memory.

The private assignment file maps blinded condition labels to ON/OFF so a
reviewer can score artifacts without knowing which condition used Mneme.

## Commands

Inspect the contract:

```sh
scripts/product-dogfood-experiment.py --check-contract
```

Prepare a local bundle:

```sh
scripts/product-dogfood-experiment.py \
  --out-dir /tmp/mneme-product-dogfood \
  --run-label local-dogfood \
  --force
```

Validate a prepared bundle:

```sh
scripts/product-dogfood-experiment.py --check-bundle /tmp/mneme-product-dogfood
```

## Interpretation

The script intentionally reports `actual_agent_execution=false` and
`external_value_claim_allowed=false` until real artifacts are created and
reviewed. Use `scripts/product-review-summary.py` after artifacts have been
scored with the generated `review-template.json`.
