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
- Claim-ID lifecycle markers target one active claim and avoid broad text-match
  updates when duplicate claim text exists.
- Context packs include active claims only when their scope is allowed by the
  retrieval query.
- Context items are deterministically ranked, carry score and match metadata,
  and are capped by the retrieval query's item budget.
- Read/write operations emit audit records.
- JSON file persistence round-trips events, claims, budget, and audit state.
- JSON stores include `schema_version`, metadata, generation, engine version,
  timestamps, and migration history.
- JSON store schema version `2` includes agent session records.
- JSON store saves are atomic and create `<store>.bak` before replacing an
  existing store.
- JSON store save and repair writes require an exclusive `<store>.lock`; lock
  conflicts are reported as `store_lock`.
- Store validation detects unsupported schema versions, duplicate IDs, missing
  required fields, missing claim source events, invalid budgets, and empty
  audit targets.
- Store repair can restore an invalid current store from a valid backup.
- Store repair can normalize compatible legacy schema metadata while preserving
  the previous file as backup.
- Store restore can explicitly roll back from a valid backup while preserving
  the pre-restore current file as the new backup.
- Compaction removes inactive claims while preserving active claim recall and
  citations.
- Agent sessions can begin with scoped context and end with explicit remembered
  claims.
- Review artifacts summarize store metadata, claim status counts, scope counts,
  memory quality findings, source event IDs, and session summaries without
  mutating store state.
- Review artifacts redact blocked-secret and obvious secret-like claim text by
  default; raw sensitive review export requires `--include-sensitive`.
- Extraction adapters propose claims; the engine owns IDs, provenance, safety,
  audit, and lifecycle state.

## Stable Tooling Contracts

- `mneme-eval` supports `doctor`, `validate`, `run`, `replay`, `acceptance`,
  `baseline`, `baseline-gate`, and command-specific help.
- Eval reports include `report_schema_version`, target metadata, counts, and
  per-scenario results.
- The opt-in `model` suite covers command-backed extraction quality for durable
  preferences, no-claim cases, secret blocking, and lifecycle correction.
- The `core`, `runtime`, and `agent` suites must pass for `fake` and
  `mneme-v1`.
- `mneme-cli` supports `init`, `doctor`, `remember`, `correct`, `forget`,
  `claims`, `quality`, `curate`, `context`, `snapshot`, `begin`, `end`, `validate`,
  `export`, `review`, `import`, `compact`, `repair`, `restore`, `version`, and
  command-specific help.
- `mneme init` creates a valid local v1 store and an agent hook runtime profile
  without tracking `.mneme/` in git.
- `mneme doctor` reports local workspace health for the store, backup, and
  agent hook profile without mutating files; `--json` exposes the same report
  for scripts.
- `mneme quality` reports duplicate active claims, blocked-secret claims,
  inactive lifecycle history, review queue items, and suggested next commands
  without mutating files.
- `mneme curate` reports a dry-run cleanup plan by default; applied curation
  requires `--apply`, and non-active record removal requires `--compact`.
- `mneme repair --check` reports repair or normalization readiness without
  mutating files.
- `mneme restore --check` reports explicit backup rollback readiness without
  mutating files; `mneme restore` swaps current and backup store roles.
- `mneme hook doctor/begin/end` emit the `mneme.agent_hook.v1` JSON envelope for
  success and failure, with non-zero process exits on failure.
- `scripts/mneme-agent-hook.sh` provides the repository-local wrapper for agent
  runtime installation smoke checks and env-based hook defaults.
- The hook wrapper reads runtime profiles from `MNEME_AGENT_HOOK_CONFIG`,
  `MNEME_CONFIG`, or `.mneme/mneme-agent-hook.env`; CLI flags override env,
  and env overrides profile values.
- `mneme-cli --store <path>` isolates local state.
- `scripts/install-local.sh` installs the local `mneme` CLI with
  `cargo install --path crates/mneme-cli --locked` and smokes doctor/help/review
  commands plus installed first-workspace bootstrap.
- `scripts/quality-gate.sh` is the local and release verification entry point.
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` must pass before
  release.
- `scripts/public-safety-check.sh` guards against known private/public-safety
  file and text patterns before release.
- `scripts/package-check.sh` verifies workspace package assembly and blocks
  known private or generated files from package contents.
- `scripts/distribution-policy-check.sh` guards the current pending-license
  state and disabled registry publication policy.

## Unstable Areas

- Rust type names and module placement are not semver-stable before 1.0; the
  intended current API surface is documented in `docs/api-contract.md`.
- JSON store schema migration is minimal and limited to v1 local state
  normalization.
- Extraction is deterministic by default; production LLM extraction is not
  implemented.
- Storage is local JSON only; concurrent writers are rejected by the local lock
  rather than coordinated.
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
