# V1 Ontology Benchmark

`scripts/v1-ontology-benchmark.py` measures Mneme v1 against a public-safe
natural-language and complex-ontology fixture. It is an eval-first baseline for
future ontology work, not a v1 pass/fail release gate.

## Purpose

The benchmark answers a narrow question: how far does current v1 get on
realistic memory inputs before adding a richer ontology model?

The current committed fixture is expected to report
`ontology_benchmark_passed` and `v1_ontology_ready`. The gap-analysis command
still exists as a scorer and planning check: when a future implementation or
fixture falls below target it reports `ontology_design_needed` instead of
failing with an opaque process error.

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

The target thresholds in the contract are the public regression target for the
current fixture. The scorer check also verifies that intentionally faulted
outputs still fall back to `ontology_design_needed`.

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

Current v1 passes the committed public fixture with a deterministic,
schema-lite ontology extractor. Treat that as a bounded public regression claim,
not as proof of broad open-domain ontology understanding.

When v1 is below target, `gap-analysis.json` maps low scores into implementation
buckets such as
`natural_language_extraction`, `relation_mapping`, `entity_resolution`,
`attribute_capture`, `temporal_state`, `multi_hop_context`, `scope_ownership`,
`provenance`, and `safety`. A complete v1 ontology run should report
`v1_ontology_ready`; otherwise the gap analysis names the next development
phase.

As of the natural-language ontology extraction phase, the full public fixture is
also run in `scripts/quality-gate.sh` against a temporary output directory and
must report `ontology_benchmark_passed`.
