# Product Held-Out Gates

`scripts/product-heldout-gates.py` records the evidence required before Mneme
can claim open-domain extraction or semantic search quality.

## Purpose

The script prevents two premature claims:

- Open-domain extraction is not claimed until live-provider or independently
  reviewed extractor evidence exists on held-out entities and paraphrases.
- Semantic search is not claimed until a stronger ranker beats the term/alias
  baseline on held-out ranking tasks.

## Commands

```sh
scripts/product-heldout-gates.py --check-contract
scripts/product-heldout-gates.py --check-dataset
scripts/product-heldout-gates.py
```

## Interpretation

The expected current state is intentionally conservative:

- `open_domain_extraction_claim_allowed=false`
- `semantic_search_claim_allowed=false`
- `heldout_evidence_ready=false`

Those fields should only become true after real extractor/ranking evidence is
added, not because fixture rules were expanded.
