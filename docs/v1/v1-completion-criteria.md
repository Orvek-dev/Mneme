# Mneme v1 Completion Criteria

Mneme v1 should be considered complete only when it is useful to a new public
user without private project context and when its quality can be checked with
the committed tools.

## Current Status

Mneme v1 is complete for the local-first personal-memory scope once the full
quality gate passes. The local CLI, store runtime, agent hook, review artifacts,
deterministic eval harness, manual dogfood protocol, hard dogfood protocol, and
ontology benchmark are implemented. Team/shared memory remains v2 scope, and
registry publication stays disabled until the public license policy changes.

## Complete V1 Gate

| Area | Completion requirement | Verification |
| --- | --- | --- |
| Installation | A user can install the local CLI and bootstrap a workspace without manual file edits. | `scripts/quality-gate.sh full` |
| Local runtime | `init`, `doctor`, `remember`, `context`, `begin`, `end`, `review`, `repair`, and `restore` work with local JSON stores. | `cargo test --workspace --all-targets` and installed CLI smoke checks |
| Safety | Secret-like memories are blocked from context and public reports remain sanitized. | `scripts/public-safety-check.sh` and v1 scenario suites |
| Agent integration | Hook begin/end flows emit stable JSON envelopes and preserve source citations. | `evals/scenarios/agent/` and wrapper smoke checks |
| Eval harness | Scenario, baseline, candidate, trend, manual dogfood, hard dogfood, and ontology checks are reproducible. | `scripts/quality-gate.sh full` |
| Hard dogfood | 100 normal records, 150 adversarial records, and 30 agent workflows pass without scope or secret leaks. | local `scripts/v1-hard-dogfood.py` run |
| Ontology | The committed public-safe ontology fixture meets the target contract for entities, relations, attributes, temporal state, provenance, scoped context, and safety. | local `scripts/v1-ontology-benchmark.py` run |
| Distribution | Package contents are public-safe and registry publication remains aligned with the documented license policy. | `scripts/package-check.sh` and `scripts/distribution-policy-check.sh` |

## Ontology Readiness Targets

The ontology benchmark reports `v1_ontology_ready` only when all target metrics
pass and no safety leaks are detected on the committed fixture. Treat this as a
release regression gate, not as broad open-domain ontology proof:

| Metric | Target |
| --- | --- |
| `entity_f1` | `0.8` |
| `relation_f1` | `0.8` |
| `attribute_f1` | `0.8` |
| `scope_accuracy` | `0.95` |
| `temporal_correctness` | `0.8` |
| `provenance_coverage` | `1.0` |
| `context_recall_at_k` | `0.8` |
| `scope_leak_count` | `0` |
| `secret_leak_count` | `0` |

## Development Rule

Every implementation phase after this document should end with one of two
outcomes:

- the complete v1 gate passes and v1 can be labeled complete;
- the latest eval evidence identifies the next highest-leverage gap, and that
  gap becomes the next implementation phase.
