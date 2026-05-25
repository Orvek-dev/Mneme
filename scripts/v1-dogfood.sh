#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

RUN_LABEL="${MNEME_DOGFOOD_RUN_LABEL:-local-$(date +%Y%m%d-%H%M%S)}"

case "$RUN_LABEL" in
  *[!A-Za-z0-9._/-]*)
    echo "v1-dogfood: run label may contain only letters, digits, '-', '_', '.', or '/'" >&2
    exit 1
    ;;
esac

OUT_DIR="${MNEME_DOGFOOD_OUT_DIR:-evals/runs/v1-dogfood/${RUN_LABEL}}"
WORKSPACE_DIR="${OUT_DIR}/workspace"
STORE="${WORKSPACE_DIR}/.mneme/mneme-v1.json"
CONFIG="${WORKSPACE_DIR}/.mneme/mneme-agent-hook.env"

mkdir -p "$OUT_DIR" "$WORKSPACE_DIR"

VALIDATE_DOGFOOD="${OUT_DIR}/dogfood.validate.json"
RUN_DOGFOOD_FAKE="${OUT_DIR}/dogfood.run.fake.json"
RUN_DOGFOOD_V1="${OUT_DIR}/dogfood.run.mneme-v1.json"
ACCEPTANCE_DOGFOOD_V1="${OUT_DIR}/dogfood.acceptance.mneme-v1.json"
V1_READINESS="${OUT_DIR}/v1-readiness.json"
CLI_DOCTOR_PRE="${OUT_DIR}/cli.doctor.pre.json"
CLI_INIT="${OUT_DIR}/cli.init.json"
CLI_DOCTOR_POST="${OUT_DIR}/cli.doctor.post.json"
CLI_REMEMBER="${OUT_DIR}/cli.remember.json"
CLI_BEGIN="${OUT_DIR}/cli.begin.json"
CLI_END="${OUT_DIR}/cli.end.json"
CLI_CONTEXT="${OUT_DIR}/cli.context.json"
CLI_QUALITY="${OUT_DIR}/cli.quality.json"
CLI_VALIDATE="${OUT_DIR}/cli.validate.txt"
SUMMARY="${OUT_DIR}/summary.json"
DOGFOOD_SUMMARY="${OUT_DIR}/dogfood-summary.json"

mneme_cli() {
  cargo run -q -p mneme-cli -- "$@"
}

mneme_eval() {
  cargo run -q -p mneme-eval -- "$@"
}

mneme_eval validate --suite dogfood --report "$VALIDATE_DOGFOOD" --json > "${VALIDATE_DOGFOOD}.stdout"
mneme_eval run --suite dogfood --target fake --report "$RUN_DOGFOOD_FAKE" --json > "${RUN_DOGFOOD_FAKE}.stdout"
mneme_eval run --suite dogfood --target mneme-v1 --report "$RUN_DOGFOOD_V1" --json > "${RUN_DOGFOOD_V1}.stdout"
mneme_eval acceptance --suite dogfood --target mneme-v1 --report "$ACCEPTANCE_DOGFOOD_V1" --json > "${ACCEPTANCE_DOGFOOD_V1}.stdout"
mneme_eval v1-readiness --report "$V1_READINESS" --json > "${V1_READINESS}.stdout"

mneme_cli doctor --store "$STORE" --config "$CONFIG" --json > "$CLI_DOCTOR_PRE"

mneme_cli init --store "$STORE" --config "$CONFIG" --no-bin --force --json > "$CLI_INIT"
mneme_cli doctor --store "$STORE" --config "$CONFIG" --json > "$CLI_DOCTOR_POST"
mneme_cli remember "user prefers dogfood evidence bundles" --store "$STORE" --json > "$CLI_REMEMBER"
mneme_cli begin "Review v1 dogfood readiness" \
  --query "dogfood evidence" \
  --agent codex \
  --store "$STORE" \
  --json > "$CLI_BEGIN"
mneme_cli end session-001 \
  --summary "Captured v1 dogfood evidence" \
  --remember "user prefers release evidence before phase promotion" \
  --store "$STORE" \
  --json > "$CLI_END"
mneme_cli context "release evidence" --store "$STORE" --json > "$CLI_CONTEXT"
mneme_cli quality --store "$STORE" --json > "$CLI_QUALITY"
mneme_cli validate --store "$STORE" > "$CLI_VALIDATE"

cat > "$SUMMARY" <<EOF
{
  "schema_version": 1,
  "command": "v1-dogfood",
  "run_label": "$RUN_LABEL",
  "status": "passed",
  "out_dir": "$OUT_DIR",
  "reports": {
    "dogfood_validate": "$VALIDATE_DOGFOOD",
    "dogfood_run_fake": "$RUN_DOGFOOD_FAKE",
    "dogfood_run_mneme_v1": "$RUN_DOGFOOD_V1",
    "dogfood_acceptance_mneme_v1": "$ACCEPTANCE_DOGFOOD_V1",
    "v1_readiness": "$V1_READINESS",
    "cli_doctor_pre": "$CLI_DOCTOR_PRE",
    "cli_init": "$CLI_INIT",
    "cli_doctor_post": "$CLI_DOCTOR_POST",
    "cli_remember": "$CLI_REMEMBER",
    "cli_begin": "$CLI_BEGIN",
    "cli_end": "$CLI_END",
    "cli_context": "$CLI_CONTEXT",
    "cli_quality": "$CLI_QUALITY",
    "cli_validate": "$CLI_VALIDATE",
    "dogfood_summary": "$DOGFOOD_SUMMARY"
  }
}
EOF

mneme_eval dogfood-summary "$OUT_DIR" --report "$DOGFOOD_SUMMARY" --json > "${DOGFOOD_SUMMARY}.stdout"

echo "v1-dogfood: wrote $OUT_DIR"
echo "v1-dogfood: summary $SUMMARY"
echo "v1-dogfood: dogfood summary $DOGFOOD_SUMMARY"
