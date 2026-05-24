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
  MNEME_AGENT_HOOK_CONFIG  Optional path to a KEY=VALUE runtime profile.
  MNEME_CONFIG             Fallback config path when MNEME_AGENT_HOOK_CONFIG is absent.
  MNEME_BIN        Optional path to an installed mneme binary.
  MNEME_STORE      Optional store path appended when --store is absent.
  MNEME_AGENT_ID   Optional agent id appended when --agent is absent.
  MNEME_SCOPE      Optional begin scope appended when --scope is absent.
  MNEME_MAX_ITEMS  Optional begin max item count appended when --max-items is absent.
  MNEME_EXTRACTOR_COMMAND
                  Optional command extractor for end --remember notes.
EOF
}

strip_optional_quotes() {
  local value="$1"
  case "$value" in
    \"*\")
      value="${value#\"}"
      value="${value%\"}"
      ;;
    \'*\')
      value="${value#\'}"
      value="${value%\'}"
      ;;
  esac
  printf '%s' "$value"
}

apply_config_value() {
  local key="$1"
  local value="$2"
  case "$key" in
    MNEME_BIN)
      if [ -z "${MNEME_BIN:-}" ]; then MNEME_BIN="$value"; fi
      ;;
    MNEME_STORE)
      if [ -z "${MNEME_STORE:-}" ]; then MNEME_STORE="$value"; fi
      ;;
    MNEME_AGENT_ID)
      if [ -z "${MNEME_AGENT_ID:-}" ]; then MNEME_AGENT_ID="$value"; fi
      ;;
    MNEME_SCOPE)
      if [ -z "${MNEME_SCOPE:-}" ]; then MNEME_SCOPE="$value"; fi
      ;;
    MNEME_MAX_ITEMS)
      if [ -z "${MNEME_MAX_ITEMS:-}" ]; then MNEME_MAX_ITEMS="$value"; fi
      ;;
    MNEME_EXTRACTOR_COMMAND)
      if [ -z "${MNEME_EXTRACTOR_COMMAND:-}" ]; then MNEME_EXTRACTOR_COMMAND="$value"; fi
      export MNEME_EXTRACTOR_COMMAND
      ;;
    *)
      printf '%s\n' "mneme-agent-hook: unknown config key: $key" >&2
      exit 2
      ;;
  esac
}

load_runtime_config() {
  CONFIG_PATH="${MNEME_AGENT_HOOK_CONFIG:-${MNEME_CONFIG:-${ROOT}/.mneme/mneme-agent-hook.env}}"
  CONFIG_LOADED=false
  if [ ! -f "$CONFIG_PATH" ]; then
    return 0
  fi
  CONFIG_LOADED=true
  local line key value
  while IFS= read -r line || [ -n "$line" ]; do
    line="${line%$'\r'}"
    case "$line" in
      ""|\#*) continue ;;
    esac
    if [[ "$line" != *=* ]]; then
      printf '%s\n' "mneme-agent-hook: invalid config line: $line" >&2
      exit 2
    fi
    key="${line%%=*}"
    value="${line#*=}"
    value="$(strip_optional_quotes "$value")"
    apply_config_value "$key" "$value"
  done < "$CONFIG_PATH"
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

option_value_equals() {
  local expected="$1"
  local desired="$2"
  shift 2
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "$expected" ]; then
      shift
      [ "$#" -gt 0 ] && [ "$1" = "$desired" ]
      return
    fi
    shift
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

with_end_runtime_args() {
  with_common_runtime_args "$@"
  if [ -n "${MNEME_EXTRACTOR_COMMAND:-}" ]; then
    if ! has_option "--extractor" "${runtime_args[@]}"; then
      runtime_args+=("--extractor" "command")
      if ! has_option "--extractor-command" "${runtime_args[@]}"; then
        runtime_args+=("--extractor-command" "$MNEME_EXTRACTOR_COMMAND")
      fi
    elif option_value_equals "--extractor" "command" "${runtime_args[@]}" \
      && ! has_option "--extractor-command" "${runtime_args[@]}"; then
      runtime_args+=("--extractor-command" "$MNEME_EXTRACTOR_COMMAND")
    fi
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

  if [ "${CONFIG_LOADED:-false}" = true ]; then
    printf 'mneme-agent-hook: config=%s\n' "$CONFIG_PATH"
  else
    printf 'mneme-agent-hook: config=absent\n'
  fi
  printf '%s\n' "mneme-agent-hook: ok"
}

load_runtime_config

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
    with_end_runtime_args "$@"
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
