# Mneme v1 Stability

Mneme is pre-1.0, so Rust API names may still change. The contracts below are
the current v1 behavior that changes should preserve unless a spec update and
eval update land in the same PR.

## Stable Behavior

- Raw events are appended in input order and remain the source of truth.
- Budget hard caps are checked before extraction.
- Claims preserve source event IDs.
- Secret-like claims are marked `blocked_secret` and omitted from context.
- Corrections mark active claims as `superseded` and write replacement claims.
- Forgets mark active claims as `forgotten`.
- Context packs include active claims only.
- Read/write operations emit audit records.
- JSON file persistence round-trips events, claims, budget, and audit state.
- JSON stores include `schema_version`, metadata, generation, engine version,
  timestamps, and migration history.
- JSON store saves are atomic and create `<store>.bak` before replacing an
  existing store.
- Store validation detects unsupported schema versions, duplicate IDs, missing
  required fields, missing claim source events, invalid budgets, and empty
  audit targets.
- Store repair can restore an invalid current store from a valid backup.
- Compaction removes inactive claims while preserving active claim recall and
  citations.
- Extraction adapters propose claims; the engine owns IDs, provenance, safety,
  audit, and lifecycle state.

## Stable Tooling Contracts

- `mneme-eval` supports `doctor`, `validate`, `run`, `replay`, `acceptance`,
  and `baseline`.
- Eval reports include `report_schema_version`, target metadata, counts, and
  per-scenario results.
- The `core` and `runtime` suites must pass for `fake` and `mneme-v1`.
- `mneme-cli` supports `doctor`, `remember`, `correct`, `forget`, `context`,
  `snapshot`, `validate`, `export`, `import`, `compact`, `repair`, and
  `version`.
- `mneme-cli --store <path>` isolates local state.
- `scripts/quality-gate.sh` is the local and release verification entry point.
- `scripts/public-safety-check.sh` guards against known private/public-safety
  file and text patterns before release.

## Unstable Areas

- Rust type names and module placement are not semver-stable before 1.0.
- JSON store schema migration is minimal and limited to v1 local state
  normalization.
- Extraction is deterministic by default; production LLM extraction is not
  implemented.
- Storage is local JSON only and does not handle concurrent writers.
- Team/shared memory, UI, external API, vector search, and performance
  benchmarks are not part of v1 yet.

## Change Rule

Any change to stable behavior should update all relevant evidence in the same
PR:

- feature spec requirements and verification maps;
- public docs;
- eval scenarios or invalid fixtures;
- unit tests or CLI smoke checks;
- changelog entry.
