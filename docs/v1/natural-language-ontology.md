# Natural-Language Ontology Extraction

Mneme v1 includes a deterministic schema-lite ontology extractor for the
default local `rule` extractor path. It turns selected durable natural-language
memory statements into multiple scoped claims while keeping the existing
explicit `remember:` marker behavior.

## What It Captures

The v1 extractor is intentionally conservative. It captures public-safe durable
signals such as:

- preferences and requirements;
- project-scoped release evidence decisions;
- artifact attributes such as length, language, style, location, and schedule;
- alias relationships;
- agent handoff requirements;
- superseded temporal or policy claims;
- team-handoff visibility requirements.

It continues to skip attribution traps and secret-like inputs. Explicit command
or model-backed extractors still use the adapter boundary and do not receive the
deterministic natural-language fallback.

## Retrieval

Context retrieval still returns claim text only, with citations to source event
IDs. Relevance scoring now also considers source event text, so a claim can be
retrieved for a task query when the supporting event contains the user's natural
language wording.

## Evidence

The v1 ontology benchmark is the acceptance gate for this surface. A complete
run should report:

```text
decision_status: ontology_benchmark_passed
readiness_status: v1_ontology_ready
entity_f1: 1.0
relation_f1: 1.0
attribute_f1: 1.0
context_recall_at_k: 1.0
scope_leak_count: 0
secret_leak_count: 0
```

Run it locally with:

```sh
scripts/v1-ontology-benchmark.py --run-label local-ontology-check
```
