#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/mneme-agent-hook.sh doctor
  scripts/mneme-agent-hook.sh begin <task> [mneme hook begin options]
  scripts/mneme-agent-hook.sh end <session-id> [mneme hook end options]

Environment:
  MNEME_BIN        Optional path to an installed mneme binary.
  MNEME_STORE      Optional store path appended when --store is absent.
  MNEME_AGENT_ID   Optional agent id appended when --agent is absent.
  MNEME_SCOPE      Optional begin scope appended when --scope is absent.
  MNEME_MAX_ITEMS  Optional begin max item count appended when --max-items is absent.
EOF
}

mneme_cmd() {
  if [ -n "${MNEME_BIN:-}" ]; then
    "$MNEME_BIN" "$@"
  elif [ -f "${ROOT}/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    (cd "$ROOT" && cargo run -q -p mneme-cli -- "$@")
  elif [ -x "${ROOT}/target/debug/mneme" ]; then
    "${ROOT}/target/debug/mneme" "$@"
  else
    printf '%s\n' "mneme-agent-hook: set MNEME_BIN or run from a Cargo workspace" >&2
    return 127
  fi
}

has_option() {
  local expected="$1"
  shift
  local value
  for value in "$@"; do
    if [ "$value" = "$expected" ]; then
      return 0
    fi
  done
  return 1
}

with_common_runtime_args() {
  runtime_args=("$@")
  if [ -n "${MNEME_STORE:-}" ] && ! has_option "--store" "${runtime_args[@]}"; then
    runtime_args+=("--store" "$MNEME_STORE")
  fi
  if [ -n "${MNEME_AGENT_ID:-}" ] && ! has_option "--agent" "${runtime_args[@]}"; then
    runtime_args+=("--agent" "$MNEME_AGENT_ID")
  fi
}

with_begin_runtime_args() {
  with_common_runtime_args "$@"
  if [ -n "${MNEME_SCOPE:-}" ] && ! has_option "--scope" "${runtime_args[@]}"; then
    runtime_args+=("--scope" "$MNEME_SCOPE")
  fi
  if [ -n "${MNEME_MAX_ITEMS:-}" ] && ! has_option "--max-items" "${runtime_args[@]}"; then
    runtime_args+=("--max-items" "$MNEME_MAX_ITEMS")
  fi
}

run_doctor() {
  local tmp_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  local store="${tmp_root}/mneme-agent-hook-smoke-$$.json"
  local doctor_report="${tmp_root}/mneme-agent-hook-doctor-$$.json"
  local begin_report="${tmp_root}/mneme-agent-hook-begin-$$.json"
  local end_report="${tmp_root}/mneme-agent-hook-end-$$.json"
  rm -f "$store" "$store.bak" "$store.lock" "$doctor_report" "$begin_report" "$end_report"
  trap "rm -f \"$store\" \"$store.bak\" \"$store.lock\" \"$doctor_report\" \"$begin_report\" \"$end_report\"" EXIT

  mneme_cmd hook doctor --store "$store" > "$doctor_report"
  grep -q '"operation": "doctor"' "$doctor_report"
  grep -q '"schema_version": "mneme.agent_hook.v1"' "$doctor_report"

  mneme_cmd remember "user prefers hook smoke checks" --store "$store" > /dev/null
  mneme_cmd hook begin "Verify agent hook runtime" \
    --query "hook smoke" \
    --agent "${MNEME_AGENT_ID:-mneme-agent-hook}" \
    --store "$store" > "$begin_report"
  grep -q '"operation": "begin"' "$begin_report"
  grep -q '"ok": true' "$begin_report"

  mneme_cmd hook end session-001 \
    --summary "Verified agent hook runtime" \
    --remember "user prefers verified hook runtime" \
    --store "$store" > "$end_report"
  grep -q '"operation": "end"' "$end_report"
  grep -q '"ok": true' "$end_report"

  printf '%s\n' "mneme-agent-hook: ok"
}

command="${1:-}"
case "$command" in
  doctor)
    shift
    if [ "$#" -ne 0 ]; then
      usage >&2
      exit 2
    fi
    run_doctor
    ;;
  begin)
    shift
    with_begin_runtime_args "$@"
    mneme_cmd hook begin "${runtime_args[@]}"
    ;;
  end)
    shift
    with_common_runtime_args "$@"
    mneme_cmd hook end "${runtime_args[@]}"
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
