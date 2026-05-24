#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

echo "distribution-policy: verifying license and registry publication guardrails"

POLICY_FILE="docs/distribution-policy.md"
if [ ! -f "$POLICY_FILE" ]; then
  echo "distribution-policy: missing $POLICY_FILE" >&2
  exit 1
fi

if [ ! -f LICENSE ] && [ ! -f LICENSE.md ]; then
  for manifest in crates/mneme-core/Cargo.toml crates/mneme-cli/Cargo.toml crates/mneme-eval/Cargo.toml; do
    if ! grep -q '^publish = false$' "$manifest"; then
      echo "distribution-policy: $manifest must keep publish = false until a LICENSE is committed" >&2
      exit 1
    fi
    if grep -Eq '^(license|license-file) = ' "$manifest"; then
      echo "distribution-policy: $manifest must not declare license metadata before a LICENSE is committed" >&2
      exit 1
    fi
  done

  grep -q 'license_status: pending-owner-decision' "$POLICY_FILE" || {
    echo "distribution-policy: policy must record pending owner license decision" >&2
    exit 1
  }
else
  echo "distribution-policy: LICENSE exists; update this script before enabling registry publication" >&2
  exit 1
fi

grep -q 'registry_publication: disabled' "$POLICY_FILE" || {
  echo "distribution-policy: policy must record disabled registry publication" >&2
  exit 1
}

echo "distribution-policy: ok"
