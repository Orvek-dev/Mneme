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
GATE_REPORT="${MNEME_LIVE_BASELINE_GATE_REPORT:-${REPORT}.gate.json}"
SUMMARY_REPORT="${MNEME_LIVE_BASELINE_SUMMARY_REPORT:-${REPORT}.summary.json}"

case "$RUN_LABEL" in
  *[!A-Za-z0-9._/-]*)
    echo "live-baseline: run label may contain only letters, digits, '-', '_', '.', or '/'" >&2
    exit 1
    ;;
esac

mkdir -p "$(dirname "$REPORT")" "$(dirname "$GATE_REPORT")" "$(dirname "$SUMMARY_REPORT")"

set +e
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
BASELINE_STATUS=$?
set -e

set +e
cargo run -p mneme-eval -- baseline-gate "$REPORT" \
  --require-live-provider \
  --require-run-label \
  --report "$GATE_REPORT" \
  --json
GATE_STATUS=$?
set -e

set +e
if [ -f "$REPORT" ]; then
  cargo run -p mneme-eval -- baseline-summary "$REPORT" \
    --report "$SUMMARY_REPORT" \
    --json
  SUMMARY_STATUS=$?
else
  echo "live-baseline: baseline report is missing; cannot write summary" >&2
  SUMMARY_STATUS=1
fi
set -e

echo "live-baseline: wrote $REPORT"
echo "live-baseline: wrote $GATE_REPORT"
if [ -f "$SUMMARY_REPORT" ]; then
  echo "live-baseline: wrote $SUMMARY_REPORT"
fi
echo "live-baseline: run scripts/public-safety-check.sh and the redaction checklist before sharing the report"

if [ "$BASELINE_STATUS" -ne 0 ]; then
  echo "live-baseline: baseline command reported failing scenario runs" >&2
fi
if [ "$GATE_STATUS" -ne 0 ]; then
  echo "live-baseline: baseline quality gate failed" >&2
fi
if [ "$SUMMARY_STATUS" -ne 0 ]; then
  echo "live-baseline: baseline summary failed" >&2
fi
if [ "$BASELINE_STATUS" -ne 0 ] || [ "$GATE_STATUS" -ne 0 ] || [ "$SUMMARY_STATUS" -ne 0 ]; then
  exit 1
fi
