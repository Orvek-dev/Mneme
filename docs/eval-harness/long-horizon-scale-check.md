# Long-Horizon Scale Check

`scripts/long-horizon-scale-check.py` is a local scale smoke for Mneme context
retrieval. It is not a database benchmark.

## Purpose

The check builds larger local JSON stores and verifies that Mneme still:

- recalls current active memory
- avoids superseded stale memory
- avoids cross-scope leakage
- stays within a simple context latency budget

## Commands

Inspect the contract:

```sh
scripts/long-horizon-scale-check.py --check-contract
```

Run the default local scale smoke:

```sh
scripts/long-horizon-scale-check.py --record-counts 1000,5000,10000
```

## Interpretation

This check is useful before changing ranking, lifecycle, or storage code. It
does not prove production scalability. JSON storage remains a local-first v1
choice, and larger deployments should use separate storage evidence before
making performance claims.
