# V1 Ontology Benchmark

`scripts/v1-ontology-benchmark.py` measures Mneme v1 against a public-safe
natural-language and complex-ontology fixture. It is an eval-first baseline for
future ontology work, not a v1 pass/fail release gate.

## Purpose

The benchmark answers a narrow question: how far does current v1 get on
realistic memory inputs before adding a richer ontology model?

The expected result today is `ontology_design_needed`. That status means the
measurement completed and found gaps; it does not mean the local quality gate
failed.

## Fixture

The public fixture is:

```text
evals/ontology/v1-natural-language-ontology-v0.json
```

It contains:

- 13 cases;
- 10 natural-language cases;
- 3 explicit-marker anchor cases for current v1 behavior;
- 33 entities;
- 17 expected relations;
- 8 expected attributes;
- 16 context checks;
- 2 temporal checks;
- 3 prohibited relations.

The cases cover pronoun resolution, scoped project decisions, attribution traps,
natural corrections, multi-hop handoff context, scope collisions, privacy traps,
alias resolution, contradiction handling, team handoff visibility, and explicit
triple anchors.

## Metrics

The scorecard reports:

- `entity_f1`;
- `relation_f1`;
- `attribute_f1`;
- `scope_accuracy`;
- `temporal_correctness`;
- `provenance_coverage`;
- `context_recall_at_k`;
- `context_precision_at_k`;
- `scope_leak_count`;
- `secret_leak_count`;
- `prohibited_relation_count`.

The target thresholds in the contract are design targets for later ontology
work. The runner does not fail the process when current v1 falls below them.

## Commands

Check the public contract:

```sh
scripts/v1-ontology-benchmark.py --check-contract
```

Check the fixture shape:

```sh
scripts/v1-ontology-benchmark.py --check-fixture
```

Check scorer fault detection:

```sh
scripts/v1-ontology-benchmark.py --check-scorer
```

Check capability gap analysis:

```sh
scripts/v1-ontology-benchmark.py --check-gap-analysis
```

Run a local baseline:

```sh
scripts/v1-ontology-benchmark.py
```

The evidence bundle is ignored by git and written under:

```text
evals/runs/v1-ontology-benchmark/<run-label>/
```

It includes `summary.json`, `scorecard.json`, `gap-analysis.json`,
`gap-analysis.md`, per-case run artifacts, `report.md`, `report.html`, and CLI
command outputs.

## Interpretation

Current v1 is expected to score well on explicit-marker anchor cases and poorly
on natural-language ontology extraction, entity resolution, attributes, temporal
state, and multi-hop context. Those gaps are the input for later v1 ontology
design, not a reason to change v1 before measuring it.

`gap-analysis.json` maps low scores into implementation buckets such as
`natural_language_extraction`, `relation_mapping`, `entity_resolution`,
`attribute_capture`, `temporal_state`, `multi_hop_context`, `scope_ownership`,
`provenance`, and `safety`. A complete v1 ontology run should report
`v1_ontology_ready`; otherwise the gap analysis names the next development
phase.
