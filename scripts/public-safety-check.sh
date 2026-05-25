#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

echo "public-safety: scanning tracked and unignored working-tree content"

FILES_FILE="$(mktemp "${TMPDIR:-/tmp}/mneme-public-safety-files.XXXXXX")"
BLOCKED_FILE="$(mktemp "${TMPDIR:-/tmp}/mneme-public-safety-blocked.XXXXXX")"
trap 'rm -f "$FILES_FILE" "$BLOCKED_FILE"' EXIT

git ls-files --cached --others --exclude-standard >"$FILES_FILE"

if grep -E '(^|/)(\.env|\.env\..*|Mneme_.*|AGENTS\.md|CLAUDE\.md|harness/|templates/|\.codex/|\.agents/)' "$FILES_FILE" |
  grep -v -E '(^|/)\.env\.example$' >"$BLOCKED_FILE"; then
  echo "public-safety: blocked public file pattern(s):" >&2
  cat "$BLOCKED_FILE" >&2
  exit 1
fi

PRIVATE_USER_PATH="${HOME:-__mneme_no_home__}"
PRIVATE_TEMPLATE_ROOT="99_""vibecoding_""template"
PRIVATE_TEMPLATE_NAME="vibecoding_""template"
OPENAI_KEY_PREFIX="OPENAI_API_KEY=""sk"
KEY_LIKE_PATTERN="sk-[A-Za-z0-9_-]{16,}"
INTERNAL_PRD="Mneme_""PRD"
INTERNAL_ROADMAP="Mneme_""Roadmap"
CONTENT_PATTERN="(${PRIVATE_USER_PATH}|${PRIVATE_TEMPLATE_ROOT}|${PRIVATE_TEMPLATE_NAME}|${OPENAI_KEY_PREFIX}|${KEY_LIKE_PATTERN}|${INTERNAL_PRD}|${INTERNAL_ROADMAP})"

FOUND=0
while IFS= read -r file; do
  case "$file" in
    target/* | Cargo.lock | evals/reports/* | benchmarks/results/*)
      continue
      ;;
  esac

  if [ -f "$file" ] && rg -n "$CONTENT_PATTERN" "$file"; then
    FOUND=1
  fi
done <"$FILES_FILE"

if [ "$FOUND" -ne 0 ]; then
  echo "public-safety: blocked private path, internal doc, or key-like pattern found" >&2
  exit 1
fi

echo "public-safety: ok"
