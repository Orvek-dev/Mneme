#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

TMP_ROOT="${TMPDIR:-/tmp}/mneme-quickstart-smoke-$$"
MNEME_BIN="${MNEME_BIN:-}"
KEEP=0

usage() {
  cat <<'EOF'
Usage: scripts/quickstart-smoke.sh [--bin <path>] [--keep]

Run the public first-memory workflow against an isolated temporary store.

Options:
  --bin <path>  Use a specific mneme binary instead of cargo run.
  --keep        Keep the temporary quickstart directory for inspection.
  --help        Show this help.

Environment:
  MNEME_BIN     Alternative way to provide the mneme binary path.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin)
      shift
      if [ "$#" -eq 0 ] || [ -z "$1" ]; then
        echo "quickstart-smoke: --bin requires a value" >&2
        exit 2
      fi
      MNEME_BIN="$1"
      ;;
    --keep)
      KEEP=1
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "quickstart-smoke: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if [ -n "$MNEME_BIN" ] && [ ! -x "$MNEME_BIN" ]; then
  echo "quickstart-smoke: mneme binary is not executable: $MNEME_BIN" >&2
  exit 1
fi

if [ -z "$MNEME_BIN" ] && command -v mneme >/dev/null 2>&1; then
  MNEME_BIN="$(command -v mneme)"
fi

mneme_cmd() {
  if [ -n "$MNEME_BIN" ]; then
    "$MNEME_BIN" "$@"
  else
    cargo run -q -p mneme-cli -- "$@"
  fi
}

cleanup() {
  if [ "$KEEP" -eq 0 ]; then
    rm -rf "$TMP_ROOT"
  fi
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$TMP_ROOT"
STORE="${TMP_ROOT}/mneme-quickstart.json"
CONFIG="${TMP_ROOT}/mneme-agent-hook.env"
REVIEW="${TMP_ROOT}/mneme-review.md"
INIT_JSON="${TMP_ROOT}/init.json"
DOCTOR_JSON="${TMP_ROOT}/doctor.json"
CONTEXT_JSON="${TMP_ROOT}/context.json"
BEGIN_JSON="${TMP_ROOT}/begin.json"
END_JSON="${TMP_ROOT}/end.json"
FOLLOWUP_CONTEXT_JSON="${TMP_ROOT}/followup-context.json"

echo "quickstart-smoke: root=${ROOT}"
echo "quickstart-smoke: workspace=${TMP_ROOT}"
if [ -n "$MNEME_BIN" ]; then
  echo "quickstart-smoke: mneme=${MNEME_BIN}"
else
  echo "quickstart-smoke: mneme=cargo-run"
fi

mneme_cmd init --store "$STORE" --config "$CONFIG" --no-bin --json > "$INIT_JSON"
grep -q '"command": "init"' "$INIT_JSON"
grep -q '"store_created": true' "$INIT_JSON"
grep -q '"config_written": true' "$INIT_JSON"

mneme_cmd doctor --store "$STORE" --config "$CONFIG" --json > "$DOCTOR_JSON"
grep -q '"command": "doctor"' "$DOCTOR_JSON"
grep -q '"ok": true' "$DOCTOR_JSON"

mneme_cmd remember "user prefers local-first tools" --store "$STORE" >/dev/null
mneme_cmd context "local-first" --store "$STORE" --json > "$CONTEXT_JSON"
grep -q 'local-first tools' "$CONTEXT_JSON"

mneme_cmd begin "Draft setup plan" \
  --query "local-first" \
  --agent codex \
  --store "$STORE" \
  --json > "$BEGIN_JSON"
grep -q '"id": "session-001"' "$BEGIN_JSON"
grep -q 'local-first tools' "$BEGIN_JSON"

mneme_cmd end session-001 \
  --summary "Prepared a setup plan" \
  --remember "user prefers concise setup plans" \
  --store "$STORE" \
  --json > "$END_JSON"
grep -q '"id": "session-001"' "$END_JSON"

mneme_cmd context "concise setup" --store "$STORE" --json > "$FOLLOWUP_CONTEXT_JSON"
grep -q 'concise setup plans' "$FOLLOWUP_CONTEXT_JSON"

mneme_cmd review "$REVIEW" --store "$STORE" >/dev/null
grep -q '# Mneme Memory Review' "$REVIEW"
grep -q 'local-first tools' "$REVIEW"

mneme_cmd validate --store "$STORE" >/dev/null

if [ "$KEEP" -eq 1 ]; then
  echo "quickstart-smoke: kept=${TMP_ROOT}"
fi
echo "quickstart-smoke: ok"
