# V1 Dogfood Readiness

`mneme-eval v1-readiness` is the deterministic gate for deciding whether the
current v1 runtime is ready for structured dogfood testing.

It does not call live providers. It validates and replays the public `core`,
`runtime`, `agent`, and `dogfood` suites against the `mneme-v1` target and
emits one product-readiness report.

## Commands

```sh
cargo run -p mneme-eval -- validate --suite dogfood
cargo run -p mneme-eval -- run --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- v1-readiness --json --report evals/reports/v1-readiness.json
```

The readiness report includes:

- required suite discovery status;
- per-suite validation and replay status;
- aggregate scenario counts;
- failed scenario IDs and failed check names;
- criteria for `required-suites.present`, `suite.discovery`,
  `scenario.validation`, `dogfood.coverage`, and `target.mneme-v1`;
- `readiness_status`, which is `ready_for_v1_dogfood` only when all criteria
  pass.

## Dogfood Suite

The `dogfood` suite is intentionally small and workflow-shaped. It covers the
minimum loops a real v1 user depends on:

- preference correction and context recall;
- agent begin/end session memory;
- quality review, curation, backup restore, and secret blocking;
- project/private scope isolation.

Provider/model extraction remains a separate baseline workflow. Run model
baselines when extractor prompts, wrappers, or provider choices change.
