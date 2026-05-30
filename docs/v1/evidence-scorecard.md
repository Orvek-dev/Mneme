# Mneme v1 Evidence Scorecard

This scorecard uses GitHub-native Markdown so the public evidence summary stays
readable without generated images or renderer-specific SVG behavior.

Measured for `v0.70.0` on 2026-05-30.

## Summary

| Area | Data shape | Result |
| --- | --- | --- |
| Ontology fixture regression | 14 committed ontology cases, including one paraphrase canary | committed fixture passes; not an open-domain ontology claim |
| Hard dogfood | 100 normal records, 150 adversarial records, 30 agent handoff workflows with non-exact retrieval queries | `30/30` workflows passed |
| Outcome gate | Acceptance contract, external verifier report, first-class `gate_result`, CLI status, and non-zero failed gate path | MVP1 smoke passed locally |
| Public scenario suites | `core`, `runtime`, `agent`, `dogfood`, `model`, `team`, `mcp`, `mcp-agent-usability` | `52` public scenarios passed through quality gates |
| Safety guardrails | scope leak and synthetic secret leak checks | `0` scope leaks, `0` synthetic secret leaks |
| Product validation | P1-P6 scripted artifact adoption, privacy/cost, lifecycle, ranking-decision, migration, review-schema, dogfood-bundle, held-out-claim, and scale checks | local product loop passed; causal productivity, semantic search, open-domain extraction, and third-party value are not claimed |
| Seeded faults | dropped citation, scope leak, secret leak, stale reuse, handoff miss | `5/5` detected |
| Team v2 privacy | actor-scoped context, handoff, sync, ontology, firewall, run, and quality surfaces | `10/10` team scenarios passed; full-output leak checks passed |

## Metric Bars

| Metric | Score |
| --- | --- |
| Context Recall@K | committed fixture regression only |
| Precision@K | committed fixture regression only |
| Citation Coverage | required by quality gate |
| Entity F1 | committed ontology fixture only |
| Relation F1 | committed ontology fixture only |
| Attribute F1 | committed ontology fixture only |
| Scope Leak | `[----------] 0` |
| Secret Leak | `[----------] 0` |
| Product Validation Loop | P1-P6 plus dogfood-bundle, held-out-claim, review-summary, and scale checks required by quality gate |
| Outcome Gate | acceptance/verifier/gate_result smoke required by quality gate |
| Seeded Fault Detection | `[##########] 5/5` |
| V2 Team Readiness | `[##########] 10/10` |

## What This Means

These are public-safe local development metrics. They show that the committed
fixtures and deterministic gates can verify Mneme v1 behavior across local
memory persistence, scoped context retrieval, agent handoff, fixture-bound
ontology extraction, candidate promotion, trend comparison, v2 team policy, and
safety checks.

They are not external production benchmark claims. Generated run bundles remain
git-ignored; the fixtures, scripts, and summary docs are committed so anyone can
inspect and rerun the public path. P1 is scripted artifact adoption, not causal
productivity evidence. The product-validation review example only validates the
evidence format; it is not third-party validation.

Do not read the fixture scores as broad natural-language understanding,
semantic search quality, or external value proof. `scripts/eval-integrity-check.py`
guards against copying golden input strings into runtime source code, and
broader extractor quality should be measured with live-provider or independently
reviewed held-out baselines.

## Reproduce

```sh
./scripts/quality-gate.sh full
scripts/quickstart-smoke.sh
scripts/v1-hard-dogfood.py --check-dataset
scripts/v1-hard-dogfood.py --check-seeded-faults
scripts/v1-ontology-benchmark.py --check-fixture
scripts/v1-ontology-benchmark.py --check-scorer
scripts/outcome-gate-smoke.sh
cargo run -p mneme-eval -- v2-readiness
```
