#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  scripts/mneme-agent-hook.sh doctor [--check-extractor]
  scripts/mneme-agent-hook.sh doctor [--check-stop-hook]
  scripts/mneme-agent-hook.sh begin <task> [mneme hook begin options]
  scripts/mneme-agent-hook.sh end <session-id> [mneme hook end options]
  scripts/mneme-agent-hook.sh stop [--session-id <id>] [--max-attempts <n>] [--state <path>]

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
  MNEME_VERIFIER_COMMAND
                  Optional outcome verifier for gated hook end sessions.
  MNEME_LOOP_MAX_ATTEMPTS
                  Maximum Stop-hook re-entry attempts for the same gate failure.
  MNEME_LOOP_STATE
                  Optional local loop state path. Defaults next to MNEME_STORE.
  MNEME_LOOP_SESSION_ID
                  Optional explicit Mneme session id for Stop-hook checks.

Options:
  --check-extractor
                  Also run an isolated command-extractor smoke check. This is
                  never enabled by default because provider-backed extractors
                  can spend network/API budget.
  --check-stop-hook
                  Run an isolated Stop-hook smoke check. This simulates Claude
                  Code Stop JSON locally and does not touch user client config.
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
    MNEME_VERIFIER_COMMAND)
      if [ -z "${MNEME_VERIFIER_COMMAND:-}" ]; then MNEME_VERIFIER_COMMAND="$value"; fi
      export MNEME_VERIFIER_COMMAND
      ;;
    MNEME_VERIFIER_POLICY)
      if [ -z "${MNEME_VERIFIER_POLICY:-}" ]; then MNEME_VERIFIER_POLICY="$value"; fi
      export MNEME_VERIFIER_POLICY
      ;;
    MNEME_VERIFIER_MANIFEST)
      if [ -z "${MNEME_VERIFIER_MANIFEST:-}" ]; then MNEME_VERIFIER_MANIFEST="$value"; fi
      export MNEME_VERIFIER_MANIFEST
      ;;
    MNEME_LOOP_MAX_ATTEMPTS)
      if [ -z "${MNEME_LOOP_MAX_ATTEMPTS:-}" ]; then MNEME_LOOP_MAX_ATTEMPTS="$value"; fi
      export MNEME_LOOP_MAX_ATTEMPTS
      ;;
    MNEME_LOOP_STATE)
      if [ -z "${MNEME_LOOP_STATE:-}" ]; then MNEME_LOOP_STATE="$value"; fi
      export MNEME_LOOP_STATE
      ;;
    MNEME_LOOP_SESSION_ID)
      if [ -z "${MNEME_LOOP_SESSION_ID:-}" ]; then MNEME_LOOP_SESSION_ID="$value"; fi
      export MNEME_LOOP_SESSION_ID
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

mneme_source() {
  if [ -n "${MNEME_BIN:-}" ]; then
    printf '%s' "MNEME_BIN"
  elif [ -f "${ROOT}/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    printf '%s' "cargo-workspace"
  elif [ -x "${ROOT}/target/debug/mneme" ]; then
    printf '%s' "target-debug"
  else
    printf '%s' "unavailable"
  fi
}

runtime_value() {
  local value="$1"
  if [ -n "$value" ]; then
    printf '%s' "$value"
  else
    printf '%s' "<mneme-default>"
  fi
}

print_runtime_diagnostics() {
  if [ "${CONFIG_LOADED:-false}" = true ]; then
    printf 'mneme-agent-hook: config=%s\n' "$CONFIG_PATH"
  else
    printf 'mneme-agent-hook: config=absent\n'
  fi
  printf 'mneme-agent-hook: config_path=%s\n' "$CONFIG_PATH"
  printf 'mneme-agent-hook: config_loaded=%s\n' "${CONFIG_LOADED:-false}"
  printf 'mneme-agent-hook: mneme_source=%s\n' "$(mneme_source)"
  printf 'mneme-agent-hook: store=%s\n' "$(runtime_value "${MNEME_STORE:-}")"
  printf 'mneme-agent-hook: agent_id=%s\n' "$(runtime_value "${MNEME_AGENT_ID:-}")"
  printf 'mneme-agent-hook: scope=%s\n' "$(runtime_value "${MNEME_SCOPE:-}")"
  printf 'mneme-agent-hook: max_items=%s\n' "$(runtime_value "${MNEME_MAX_ITEMS:-}")"
  if [ -n "${MNEME_EXTRACTOR_COMMAND:-}" ]; then
    printf 'mneme-agent-hook: extractor_command=%s\n' "$MNEME_EXTRACTOR_COMMAND"
  else
    printf '%s\n' "mneme-agent-hook: extractor_command=<unset>"
  fi
  if [ -n "${MNEME_VERIFIER_COMMAND:-}" ]; then
    printf 'mneme-agent-hook: verifier_command=%s\n' "$MNEME_VERIFIER_COMMAND"
  else
    printf '%s\n' "mneme-agent-hook: verifier_command=<unset>"
  fi
  printf 'mneme-agent-hook: loop_max_attempts=%s\n' "$(runtime_value "${MNEME_LOOP_MAX_ATTEMPTS:-}")"
  printf 'mneme-agent-hook: loop_state=%s\n' "$(runtime_value "${MNEME_LOOP_STATE:-}")"
  printf 'mneme-agent-hook: loop_session_id=%s\n' "$(runtime_value "${MNEME_LOOP_SESSION_ID:-}")"
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
  if [ -n "${MNEME_VERIFIER_COMMAND:-}" ] \
    && ! has_option "--verifier-report" "${runtime_args[@]}" \
    && ! has_option "--verifier-command" "${runtime_args[@]}"; then
    runtime_args+=("--verifier-command" "$MNEME_VERIFIER_COMMAND")
  fi
}

run_extractor_smoke() {
  local store="$1"
  local begin_report="$2"
  local end_report="$3"

  if [ -z "${MNEME_EXTRACTOR_COMMAND:-}" ]; then
    printf '%s\n' "mneme-agent-hook: extractor_smoke=error:not-configured"
    printf '%s\n' "mneme-agent-hook: set MNEME_EXTRACTOR_COMMAND or run mneme init --extractor-command <program>" >&2
    return 2
  fi

  mneme_cmd hook begin "Verify command extractor runtime" \
    --agent "${MNEME_AGENT_ID:-mneme-agent-hook}" \
    --store "$store" > "$begin_report"
  grep -q '"operation": "begin"' "$begin_report"
  grep -q '"ok": true' "$begin_report"

  mneme_cmd hook end session-001 \
    --summary "Verified command extractor runtime" \
    --remember "For future planning docs, keep explanations direct and skip motivational language." \
    --extractor command \
    --extractor-command "$MNEME_EXTRACTOR_COMMAND" \
    --store "$store" > "$end_report"
  grep -q '"operation": "end"' "$end_report"
  grep -q '"ok": true' "$end_report"
  grep -q '"extractor": "command"' "$end_report"
  grep -q '"remembered_claim_count": 1' "$end_report"
  printf '%s\n' "mneme-agent-hook: extractor_smoke=ok"
}

run_stop_hook_smoke() {
  local tmp_root workspace store acceptance begin_report end_report stop_report active_report state
  tmp_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  workspace="$(mktemp -d "${tmp_root}/mneme-stop-hook-smoke.XXXXXX")"
  store="${workspace}/mneme-stop-hook.json"
  acceptance="${workspace}/acceptance.json"
  begin_report="${workspace}/begin.json"
  end_report="${workspace}/end.json"
  stop_report="${workspace}/stop.json"
  active_report="${workspace}/stop-active.json"
  state="${workspace}/loop-state.json"
  (
    cd "$workspace"
    cat > "$acceptance" <<'JSON'
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "stop-hook-smoke",
  "criteria": [
    {
      "id": "must-fail-on-first-pass",
      "kind": "command",
      "command": {"argv": ["sh", "-c", "exit 1"], "expect_exit": 0}
    }
  ]
}
JSON
    mneme_cmd hook begin "Verify Stop hook loop" \
      --acceptance "$acceptance" \
      --agent "${MNEME_AGENT_ID:-mneme-agent-hook}" \
      --store "$store" > "$begin_report"
    grep -q '"operation": "begin"' "$begin_report"
    if mneme_cmd hook end session-001 \
      --summary "First pass intentionally fails" \
      --verifier-command "$ROOT/scripts/mneme-outcome-verifier.py" \
      --store "$store" > "$end_report"; then
      printf '%s\n' "mneme-agent-hook: stop_hook_smoke=error:gate unexpectedly passed" >&2
      exit 1
    fi
    grep -q '"gate_status": "failed"' "$end_report"
    printf '%s\n' '{"hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"Done"}' \
      | MNEME_STORE="$store" MNEME_LOOP_STATE="$state" MNEME_LOOP_MAX_ATTEMPTS=1 \
        "$ROOT/scripts/mneme-agent-hook.sh" stop > "$stop_report"
    grep -q '"decision": "block"' "$stop_report"
    grep -q 'must-fail-on-first-pass' "$stop_report"
    printf '%s\n' '{"hook_event_name":"Stop","stop_hook_active":true,"last_assistant_message":"Retry"}' \
      | MNEME_STORE="$store" MNEME_LOOP_STATE="$state" MNEME_LOOP_MAX_ATTEMPTS=1 \
        "$ROOT/scripts/mneme-agent-hook.sh" stop > "$active_report"
    if [ -s "$active_report" ]; then
      printf '%s\n' "mneme-agent-hook: stop_hook_smoke=error:active stop hook should not block" >&2
      exit 1
    fi
  )
  rm -rf "$workspace"
  printf '%s\n' "mneme-agent-hook: stop_hook_smoke=ok"
}

parse_doctor_args() {
  CHECK_EXTRACTOR=false
  CHECK_STOP_HOOK=false
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --check-extractor)
        CHECK_EXTRACTOR=true
        ;;
      --check-stop-hook)
        CHECK_STOP_HOOK=true
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage >&2
        exit 2
        ;;
    esac
    shift
  done
}

parse_stop_args() {
  STOP_SESSION_ID="${MNEME_LOOP_SESSION_ID:-}"
  STOP_MAX_ATTEMPTS="${MNEME_LOOP_MAX_ATTEMPTS:-3}"
  STOP_STATE="${MNEME_LOOP_STATE:-}"
  while [ "$#" -gt 0 ]; do
    case "$1" in
      --session-id)
        shift
        [ "$#" -gt 0 ] || { printf '%s\n' "mneme-agent-hook: --session-id requires a value" >&2; exit 2; }
        STOP_SESSION_ID="$1"
        ;;
      --max-attempts)
        shift
        [ "$#" -gt 0 ] || { printf '%s\n' "mneme-agent-hook: --max-attempts requires a value" >&2; exit 2; }
        STOP_MAX_ATTEMPTS="$1"
        ;;
      --state)
        shift
        [ "$#" -gt 0 ] || { printf '%s\n' "mneme-agent-hook: --state requires a value" >&2; exit 2; }
        STOP_STATE="$1"
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        usage >&2
        exit 2
        ;;
    esac
    shift
  done
}

default_loop_state_path() {
  if [ -n "${STOP_STATE:-}" ]; then
    printf '%s' "$STOP_STATE"
    return 0
  fi
  if [ -n "${MNEME_STORE:-}" ]; then
    local dir
    dir="$(dirname "$MNEME_STORE")"
    printf '%s' "${dir}/mneme-loop-state.json"
    return 0
  fi
  printf '%s' "${ROOT}/.mneme/mneme-loop-state.json"
}

parse_stop_hook_active() {
  local input_file="$1"
  python3 - "$input_file" <<'PY'
import json
import sys
from pathlib import Path

try:
    text = Path(sys.argv[1]).read_text()
    data = json.loads(text) if text.strip() else {}
except Exception:
    data = {}
print("true" if data.get("stop_hook_active") is True else "false")
PY
}

extract_loop_field() {
  local report_file="$1"
  local field="$2"
  python3 - "$report_file" "$field" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
value = data.get(sys.argv[2])
if value is None:
    print("")
elif isinstance(value, bool):
    print("true" if value else "false")
else:
    print(value)
PY
}

next_loop_attempt() {
  local state_file="$1"
  local failure_id="$2"
  python3 - "$state_file" "$failure_id" <<'PY'
import json
import time
import sys
from pathlib import Path

path = Path(sys.argv[1])
failure_id = sys.argv[2]
state = {}
if path.exists():
    try:
        state = json.loads(path.read_text())
    except Exception:
        state = {}
if state.get("last_gate_failure_id") == failure_id:
    retry_count = int(state.get("retry_count") or 0) + 1
else:
    retry_count = 1
path.parent.mkdir(parents=True, exist_ok=True)
path.write_text(json.dumps({
    "schema_version": "mneme.loop_state.v1",
    "last_gate_failure_id": failure_id,
    "retry_count": retry_count,
    "updated_at_unix_seconds": int(time.time()),
}, indent=2) + "\n")
print(retry_count)
PY
}

emit_claude_stop_decision() {
  local report_file="$1"
  python3 - "$report_file" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text())
prompt = report.get("continuation_prompt") or report.get("reason") or "Mneme outcome gate requires more work."
reason = f"{prompt}\n\nMneme retry {report.get('retry_count')}/{report.get('max_attempts')}."
print(json.dumps({"decision": "block", "reason": reason}, ensure_ascii=False))
PY
}

run_stop_hook() {
  local input_file advice_probe advice_final state_path stop_active failure_id attempt block_stop
  local store_args=()
  if [ -n "${MNEME_STORE:-}" ]; then
    store_args=("--store" "$MNEME_STORE")
  fi
  input_file="$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/mneme-stop-input.XXXXXX")"
  advice_probe="$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/mneme-stop-advice-probe.XXXXXX")"
  advice_final="$(mktemp "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/mneme-stop-advice-final.XXXXXX")"
  trap "rm -f \"$input_file\" \"$advice_probe\" \"$advice_final\"" RETURN
  cat > "$input_file"
  stop_active="$(parse_stop_hook_active "$input_file")"
  if [ "$stop_active" = "true" ]; then
    return 0
  fi
  state_path="$(default_loop_state_path)"
  if [ -n "${STOP_SESSION_ID:-}" ]; then
    mneme_cmd outcome next "$STOP_SESSION_ID" \
      --attempt 0 \
      --max-attempts "$STOP_MAX_ATTEMPTS" \
      --stop-hook-active false \
      "${store_args[@]}" \
      --json > "$advice_probe" || return 0
  else
    mneme_cmd outcome next --latest \
      --attempt 0 \
      --max-attempts "$STOP_MAX_ATTEMPTS" \
      --stop-hook-active false \
      "${store_args[@]}" \
      --json > "$advice_probe" || return 0
  fi
  failure_id="$(extract_loop_field "$advice_probe" "last_gate_failure_id")"
  if [ -z "$failure_id" ]; then
    return 0
  fi
  attempt="$(next_loop_attempt "$state_path" "$failure_id")"
  if [ -n "${STOP_SESSION_ID:-}" ]; then
    mneme_cmd outcome next "$STOP_SESSION_ID" \
      --attempt "$attempt" \
      --max-attempts "$STOP_MAX_ATTEMPTS" \
      --stop-hook-active false \
      "${store_args[@]}" \
      --json > "$advice_final" || return 0
  else
    mneme_cmd outcome next --latest \
      --attempt "$attempt" \
      --max-attempts "$STOP_MAX_ATTEMPTS" \
      --stop-hook-active false \
      "${store_args[@]}" \
      --json > "$advice_final" || return 0
  fi
  block_stop="$(extract_loop_field "$advice_final" "block_stop")"
  if [ "$block_stop" = "true" ]; then
    emit_claude_stop_decision "$advice_final"
  fi
}

run_doctor() {
  local tmp_root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  local store="${tmp_root}/mneme-agent-hook-smoke-$$.json"
  local doctor_report="${tmp_root}/mneme-agent-hook-doctor-$$.json"
  local begin_report="${tmp_root}/mneme-agent-hook-begin-$$.json"
  local end_report="${tmp_root}/mneme-agent-hook-end-$$.json"
  local extractor_store="${tmp_root}/mneme-agent-hook-extractor-smoke-$$.json"
  local extractor_begin_report="${tmp_root}/mneme-agent-hook-extractor-begin-$$.json"
  local extractor_end_report="${tmp_root}/mneme-agent-hook-extractor-end-$$.json"
  rm -f "$store" "$store.bak" "$store.lock" "$doctor_report" "$begin_report" "$end_report" \
    "$extractor_store" "$extractor_store.bak" "$extractor_store.lock" \
    "$extractor_begin_report" "$extractor_end_report"
  trap "rm -f \"$store\" \"$store.bak\" \"$store.lock\" \"$doctor_report\" \"$begin_report\" \"$end_report\" \"$extractor_store\" \"$extractor_store.bak\" \"$extractor_store.lock\" \"$extractor_begin_report\" \"$extractor_end_report\"" EXIT

  print_runtime_diagnostics

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
  printf '%s\n' "mneme-agent-hook: hook_smoke=ok"

  if [ "${CHECK_EXTRACTOR:-false}" = true ]; then
    run_extractor_smoke "$extractor_store" "$extractor_begin_report" "$extractor_end_report"
  elif [ -n "${MNEME_EXTRACTOR_COMMAND:-}" ]; then
    printf '%s\n' "mneme-agent-hook: extractor_smoke=skipped:requires --check-extractor"
  else
    printf '%s\n' "mneme-agent-hook: extractor_smoke=skipped:not-configured"
  fi
  if [ "${CHECK_STOP_HOOK:-false}" = true ]; then
    run_stop_hook_smoke
  else
    printf '%s\n' "mneme-agent-hook: stop_hook_smoke=skipped:requires --check-stop-hook"
  fi
  printf '%s\n' "mneme-agent-hook: ok"
}

load_runtime_config

command="${1:-}"
case "$command" in
  doctor)
    shift
    parse_doctor_args "$@"
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
  stop)
    shift
    parse_stop_args "$@"
    run_stop_hook
    ;;
  help|-h|--help)
    usage
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
