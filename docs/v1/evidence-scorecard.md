# Mneme v1 Evidence Scorecard

This scorecard uses GitHub-native Markdown so the public evidence summary stays
readable without generated images or renderer-specific SVG behavior.

Measured for `v0.64.0` on 2026-05-25.

## Summary

| Area | Data shape | Result |
| --- | --- | --- |
| Ontology readiness | 13 golden ontology cases | `v1_ontology_ready` |
| Hard dogfood | 100 normal records, 150 adversarial records, 30 agent handoff workflows | `30/30` workflows passed |
| Public scenario suites | `core`, `runtime`, `agent`, `dogfood`, `model`, `team` | `46` public scenarios passed through quality gates |
| Safety guardrails | scope leak and secret leak checks | `0` scope leaks, `0` secret leaks |
| Seeded faults | dropped citation, scope leak, secret leak, stale reuse, handoff miss | `5/5` detected |
| Team v2 privacy | actor-scoped context, handoff, sync, ontology, firewall, run, and quality surfaces | `10/10` team scenarios passed; full-output leak checks passed |

## Metric Bars

| Metric | Score |
| --- | --- |
| Context Recall@K | `[##########] 1.00` |
| Precision@K | `[##########] 1.00` |
| Citation Coverage | `[##########] 1.00` |
| Entity F1 | `[##########] 1.00` |
| Relation F1 | `[##########] 1.00` |
| Attribute F1 | `[##########] 1.00` |
| Scope Leak | `[----------] 0` |
| Secret Leak | `[----------] 0` |
| Seeded Fault Detection | `[##########] 5/5` |
| V2 Team Readiness | `[##########] 10/10` |

## What This Means

These are public-safe local development metrics. They show that the committed
fixtures and deterministic gates can verify Mneme v1 behavior across local
memory persistence, scoped context retrieval, agent handoff, ontology extraction,
candidate promotion, trend comparison, v2 team policy, and safety checks.

They are not external production benchmark claims. Generated run bundles remain
git-ignored; the fixtures, scripts, and summary docs are committed so anyone can
inspect and rerun the public path.

## Reproduce

```sh
./scripts/quality-gate.sh full
scripts/quickstart-smoke.sh
scripts/v1-hard-dogfood.py --check-dataset
scripts/v1-hard-dogfood.py --check-seeded-faults
scripts/v1-ontology-benchmark.py --check-fixture
scripts/v1-ontology-benchmark.py --check-scorer
cargo run -p mneme-eval -- v2-readiness
```
