#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

if [ -z "${OPENAI_API_KEY:-}" ]; then
  echo "live-baseline: OPENAI_API_KEY is required for a live provider run" >&2
  exit 1
fi

MODEL="${OPENAI_MODEL:-gpt-5.4-mini}"
ITERATIONS="${MNEME_LIVE_BASELINE_ITERATIONS:-3}"
RUN_LABEL="${MNEME_LIVE_BASELINE_RUN_LABEL:-local-$(date +%Y%m%d)}"
REPORT="${MNEME_LIVE_BASELINE_REPORT:-evals/reports/openai-live-baseline.json}"

case "$RUN_LABEL" in
  *[!A-Za-z0-9._/-]*)
    echo "live-baseline: run label may contain only letters, digits, '-', '_', '.', or '/'" >&2
    exit 1
    ;;
esac

mkdir -p "$(dirname "$REPORT")"

cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations "$ITERATIONS" \
  --provider-label openai \
  --model-label "$MODEL" \
  --run-label "$RUN_LABEL" \
  --live-provider \
  --report "$REPORT" \
  --json

echo "live-baseline: wrote $REPORT"
echo "live-baseline: run scripts/public-safety-check.sh and the redaction checklist before sharing the report"
