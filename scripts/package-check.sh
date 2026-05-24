#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

TMP_ROOT="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
LIST_DIR="$(mktemp -d "${TMP_ROOT}/mneme-package-check.XXXXXX")"
trap 'rm -rf "$LIST_DIR"' EXIT

echo "package-check: verifying package assembly and package file lists"

./scripts/distribution-policy-check.sh

cargo package -p mneme-core --allow-dirty --no-verify --locked

for package in mneme-core mneme-cli mneme-eval; do
  list_file="${LIST_DIR}/${package}.txt"
  cargo package -p "$package" --allow-dirty --list --locked > "$list_file"
  if grep -E '(^|/)(\.env($|/)|\.mneme($|/)|evals/reports/|benchmarks/results/|target/|Mneme_|AGENTS\.md$|CLAUDE\.md$|harness/|templates/|99_.*_template)' "$list_file"; then
    echo "package-check: blocked file pattern found in $package package" >&2
    exit 1
  fi
done

echo "package-check: ok"
