#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

MODE="${1:-full}"
TMP_ROOT="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"

echo "quality-gate: mode=${MODE}"

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets

cargo run -p mneme-cli -- doctor
cargo run -p mneme-eval -- doctor

STORE="${TMP_ROOT}/mneme-quality-gate-cli.json"
rm -f "$STORE"
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store "$STORE"
cargo run -p mneme-cli -- context "local-first" --store "$STORE" --json | grep -q "local-first tools"
cargo run -p mneme-cli -- validate --store "$STORE"
EXPORT_STORE="${TMP_ROOT}/mneme-quality-gate-export.json"
IMPORT_STORE="${TMP_ROOT}/mneme-quality-gate-import.json"
rm -f "$EXPORT_STORE" "$IMPORT_STORE" "$IMPORT_STORE.bak"
cargo run -p mneme-cli -- export "$EXPORT_STORE" --store "$STORE"
cargo run -p mneme-cli -- import "$EXPORT_STORE" --store "$IMPORT_STORE"
cargo run -p mneme-cli -- compact --store "$IMPORT_STORE"
cargo run -p mneme-cli -- validate --store "$IMPORT_STORE"
cargo run -p mneme-cli -- remember "user prefers repairable stores" --store "$IMPORT_STORE"
printf '{not-json\n' > "$IMPORT_STORE"
cargo run -p mneme-cli -- repair --store "$IMPORT_STORE"
cargo run -p mneme-cli -- validate --store "$IMPORT_STORE"

COMMAND_STORE="${TMP_ROOT}/mneme-quality-gate-command.json"
rm -f "$COMMAND_STORE"
RESPONSE='{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"command-backed extraction"}}'
cargo run -p mneme-cli -- ingest "the user likes model-backed extraction" \
  --extractor command \
  --extractor-command /bin/sh \
  --extractor-arg -c \
  --extractor-arg "cat >/dev/null; printf '%s\n' '${RESPONSE}'" \
  --store "$COMMAND_STORE"
cargo run -p mneme-cli -- context "command-backed" --store "$COMMAND_STORE" --json | grep -q "command-backed extraction"

cargo run -p mneme-eval -- validate --suite core
cargo run -p mneme-eval -- validate --suite model
cargo run -p mneme-eval -- validate --suite runtime

for scenario in evals/fixtures/invalid/*.yaml; do
  if cargo run -p mneme-eval -- validate "$scenario"; then
    echo "quality-gate: invalid scenario unexpectedly passed validation: $scenario" >&2
    exit 1
  fi
done

cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
cargo run -p mneme-eval -- run --suite runtime --target fake
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
cargo run -p mneme-eval -- run --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh

MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py

if cargo run -p mneme-eval -- run --suite core --target fake --seeded-fault skip-claims; then
  echo "quality-gate: seeded fault unexpectedly passed" >&2
  exit 1
fi

cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target fake
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh

MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- acceptance --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py

BASELINE_REPORT="${TMP_ROOT}/mneme-openai-wrapper-baseline.json"
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --run-label "${MODE}-dry-run" \
  --json | tee "$BASELINE_REPORT"
grep -q '"provider_label": "openai"' "$BASELINE_REPORT"
grep -q '"model_label": "dry-run"' "$BASELINE_REPORT"
grep -q '"scenario_count": 8' "$BASELINE_REPORT"
grep -q '"category": "no-claim"' "$BASELINE_REPORT"
grep -q '"passed_iterations": 2' "$BASELINE_REPORT"
grep -q '"failed_scenario_runs": 0' "$BASELINE_REPORT"

./scripts/public-safety-check.sh

echo "quality-gate: ok"
