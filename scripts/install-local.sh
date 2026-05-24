#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

INSTALL_ROOT="${MNEME_INSTALL_ROOT:-${CARGO_INSTALL_ROOT:-$HOME/.cargo}}"
BUILD_MODE="release"
FORCE=1
SMOKE=1

usage() {
  cat <<'EOF'
Usage: scripts/install-local.sh [--root <path>] [--debug] [--no-force] [--skip-smoke]

Install the local mneme CLI binary with cargo install.

Options:
  --root <path>    Cargo install root. Binary is written to <path>/bin/mneme.
  --debug          Use cargo install --debug for faster local smoke installs.
  --no-force       Do not pass --force to cargo install.
  --skip-smoke     Skip doctor/help/review command smoke checks.
  --help           Show this help.

Environment:
  MNEME_INSTALL_ROOT overrides the install root.
  CARGO_INSTALL_ROOT is used when MNEME_INSTALL_ROOT is unset.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --root)
      shift
      if [ "$#" -eq 0 ] || [ -z "$1" ]; then
        echo "mneme-install: --root requires a value" >&2
        exit 2
      fi
      INSTALL_ROOT="$1"
      ;;
    --debug)
      BUILD_MODE="debug"
      ;;
    --no-force)
      FORCE=0
      ;;
    --skip-smoke)
      SMOKE=0
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "mneme-install: unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
  shift
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "mneme-install: cargo is required" >&2
  exit 1
fi

mkdir -p "$INSTALL_ROOT"

set -- install --path crates/mneme-cli --locked --root "$INSTALL_ROOT"
if [ "$FORCE" -eq 1 ]; then
  set -- "$@" --force
fi
if [ "$BUILD_MODE" = "debug" ]; then
  set -- "$@" --debug
fi

echo "mneme-install: installing mneme to ${INSTALL_ROOT}/bin"
cargo "$@"

BIN="${INSTALL_ROOT}/bin/mneme"
if [ ! -x "$BIN" ]; then
  echo "mneme-install: expected executable not found: $BIN" >&2
  exit 1
fi

if [ "$SMOKE" -eq 1 ]; then
  "$BIN" --version >/dev/null
  "$BIN" doctor >/dev/null
  "$BIN" help >/dev/null
  "$BIN" review --help >/dev/null
fi

echo "mneme-install: binary=${BIN}"
echo "mneme-install: add to PATH with: export PATH=\"${INSTALL_ROOT}/bin:\$PATH\""
echo "mneme-install: ok"
