#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

VERSION="${1:-1.0.0}"
TAG="v${VERSION}"

echo "v1-final-readiness: version=${VERSION}"

grep -q "version = \"${VERSION}\"" Cargo.toml || {
  echo "v1-final-readiness: workspace version mismatch" >&2
  exit 1
}

grep -q "mneme-core = { path = \"crates/mneme-core\", version = \"${VERSION}\" }" Cargo.toml || {
  echo "v1-final-readiness: workspace mneme-core dependency mismatch" >&2
  exit 1
}

grep -q "mneme-mcp = { path = \"../mneme-mcp\", version = \"${VERSION}\" }" crates/mneme-eval/Cargo.toml || {
  echo "v1-final-readiness: mneme-eval mneme-mcp dependency mismatch" >&2
  exit 1
}

grep -q "releases/tag/${TAG}" README.md || {
  echo "v1-final-readiness: README release badge link mismatch" >&2
  exit 1
}

grep -q "version-${VERSION}" README.md || {
  echo "v1-final-readiness: README version badge mismatch" >&2
  exit 1
}

grep -q 'Measured for `'"${TAG}"'`' docs/v1/evidence-scorecard.md || {
  echo "v1-final-readiness: evidence scorecard version mismatch" >&2
  exit 1
}

grep -q "## \\[${VERSION}\\] - 2026-05-30" CHANGELOG.md || {
  echo "v1-final-readiness: changelog missing ${VERSION}" >&2
  exit 1
}

grep -q "v1.0.0 is the first public source release" README.md || {
  echo "v1-final-readiness: README missing v1 source release boundary" >&2
  exit 1
}

grep -q "does not claim hosted SaaS readiness" README.md || {
  echo "v1-final-readiness: README missing non-claim boundary" >&2
  exit 1
}

grep -q "causal productivity" README.md || {
  echo "v1-final-readiness: README missing causal-productivity caveat" >&2
  exit 1
}

grep -q "third-party production validation" README.md || {
  echo "v1-final-readiness: README missing third-party caveat" >&2
  exit 1
}

grep -q "registry publication" docs/project/v1-final-readiness.md || {
  echo "v1-final-readiness: final readiness doc missing registry boundary" >&2
  exit 1
}

grep -q "scripts/v1-final-readiness-check.sh" docs/project/release-checklist.md || {
  echo "v1-final-readiness: release checklist missing final readiness command" >&2
  exit 1
}

grep -q "normal public source releases" docs/project/release-checklist.md || {
  echo "v1-final-readiness: release checklist missing v1 release policy" >&2
  exit 1
}

if grep -R "0\\.75\\.0\\|v0\\.75\\.0" \
  README.md Cargo.toml Cargo.lock crates docs examples .github scripts/mneme-mcp-stdio.py \
  >/tmp/mneme-v1-final-readiness-old-version.txt 2>/dev/null; then
  echo "v1-final-readiness: stale v0.75.0 reference outside changelog" >&2
  cat /tmp/mneme-v1-final-readiness-old-version.txt >&2
  exit 1
fi

if grep -R "pre-1\\.0" \
  README.md crates docs/project docs/v1 \
  >/tmp/mneme-v1-final-readiness-pre1.txt 2>/dev/null; then
  echo "v1-final-readiness: stale pre-1.0 language in active public docs" >&2
  cat /tmp/mneme-v1-final-readiness-pre1.txt >&2
  exit 1
fi

echo "v1-final-readiness: ok"
