# V1 Natural-Language Ontology Extraction MVP

## Goal

Make Mneme v1 complete for local-first personal memory by implementing the
schema-lite natural-language ontology layer measured in the public benchmark.

## Requirements

| ID | Requirement | Verification |
| --- | --- | --- |
| REQ-NLO-001 | The default `rule` extraction path can emit multiple claims from one durable natural-language event. | `cargo test -p mneme-core rule_extractor` |
| REQ-NLO-002 | Natural-language extraction captures entities, relations, attributes, temporal state, scope, provenance, and agent handoff facts in the public ontology fixture. | `scripts/v1-ontology-benchmark.py --run-label local-phase42 --out-dir /tmp/mneme-phase42-ontology --force` |
| REQ-NLO-003 | Attribution traps and secret-like inputs do not become active context. | ontology benchmark safety metrics and existing secret-blocking tests |
| REQ-NLO-004 | Context retrieval can use source event text for relevance while returning cited claim text only. | ontology benchmark context checks |
| REQ-NLO-005 | The full compact ontology benchmark is part of the quality gate and must report `v1_ontology_ready`. | `scripts/quality-gate.sh full` |

## Non-Goals

- General-purpose open-domain NLP.
- Team/shared memory beyond the existing v1 scoped claims.
- Registry publication while the license policy remains pending.

## Verification

```sh
cargo test -p mneme-core rule_extractor
scripts/v1-ontology-benchmark.py --run-label local-phase42 --out-dir /tmp/mneme-phase42-ontology --force
scripts/quality-gate.sh full
```
