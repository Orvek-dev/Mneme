#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

echo "distribution-policy: verifying license and registry publication guardrails"

POLICY_FILE="docs/project/distribution-policy.md"
if [ ! -f "$POLICY_FILE" ]; then
  echo "distribution-policy: missing $POLICY_FILE" >&2
  exit 1
fi

if [ ! -f LICENSE ]; then
  echo "distribution-policy: missing LICENSE" >&2
  exit 1
fi

grep -q '^MIT License$' LICENSE || {
  echo "distribution-policy: LICENSE must be MIT" >&2
  exit 1
}

grep -q '^license = "MIT"$' Cargo.toml || {
  echo "distribution-policy: workspace package license must be MIT" >&2
  exit 1
}

for manifest in crates/mneme-core/Cargo.toml crates/mneme-cli/Cargo.toml crates/mneme-mcp/Cargo.toml crates/mneme-eval/Cargo.toml; do
  if ! grep -q '^publish = false$' "$manifest"; then
    echo "distribution-policy: $manifest must keep publish = false until registry publication is intentionally enabled" >&2
    exit 1
  fi
  if ! grep -q '^license.workspace = true$' "$manifest"; then
    echo "distribution-policy: $manifest must inherit workspace MIT license metadata" >&2
    exit 1
  fi
done

grep -q 'license_status: MIT' "$POLICY_FILE" || {
  echo "distribution-policy: policy must record MIT license status" >&2
  exit 1
}

grep -q 'registry_publication: disabled' "$POLICY_FILE" || {
  echo "distribution-policy: policy must record disabled registry publication" >&2
  exit 1
}

echo "distribution-policy: ok"
