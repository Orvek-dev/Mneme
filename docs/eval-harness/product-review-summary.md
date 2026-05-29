# Product Review Summary

`scripts/product-review-summary.py` validates and summarizes public-safe blind
review artifacts for Mneme ON/OFF dogfood runs.

## Required Evidence

Each review file must use `mneme.product_validation_review.v1` and include:

- a non-author reviewer identity
- blinded condition labels
- score deltas with and without Mneme
- whether memory helped or harmed
- citation fidelity
- correction and rework counts
- no raw transcript, no local paths, and no secret-like text

## Commands

Inspect the contract:

```sh
scripts/product-review-summary.py --check-contract
```

Summarize one or more reviews:

```sh
scripts/product-review-summary.py \
  --review examples/product-validation-review.example.json \
  --min-reviews 2
```

## Interpretation

`external_value_claim_allowed` remains `false` unless there is enough valid,
public-safe, third-party, blinded review evidence. The committed example is a
schema example only, not third-party validation.
