#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
TMP_ROOT="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
WORKSPACE="$(mktemp -d "${TMP_ROOT}/mneme-outcome-gate.XXXXXX")"

if [ "${MNEME_BIN:-}" = "" ]; then
  MNEME_BIN="$ROOT/target/debug/mneme"
  (cd "$ROOT" && cargo build -q -p mneme-cli)
elif [ ! -x "$MNEME_BIN" ]; then
  (cd "$ROOT" && cargo build -q -p mneme-cli)
fi

cd "$WORKSPACE"
git init -q
git config user.email "mneme@example.local"
git config user.name "Mneme Test"
mkdir -p src
printf '%s\n' 'pub fn existing() {}' > src/main.rs
printf '%s\n' '.mneme-test/' > .gitignore
git add src/main.rs .gitignore
git commit -q -m "initial"
mkdir -p .mneme-test

STORE="$WORKSPACE/.mneme-test/mneme.json"
TEMPLATE_REPORT="$WORKSPACE/.mneme-test/template-report.json"
TEMPLATE_ACCEPTANCE="$WORKSPACE/.mneme-test/template-acceptance.json"
TEMPLATE_VALIDATE="$WORKSPACE/.mneme-test/template-validate.json"
BAD_ACCEPTANCE="$WORKSPACE/.mneme-test/bad-acceptance.json"
BAD_VALIDATE="$WORKSPACE/.mneme-test/bad-validate.json"
BAD_BEGIN="$WORKSPACE/.mneme-test/bad-begin.json"
ACCEPTANCE_PASS="$WORKSPACE/.mneme-test/acceptance-pass.json"
BEGIN_PASS="$WORKSPACE/.mneme-test/begin-pass.json"
END_PASS="$WORKSPACE/.mneme-test/end-pass.json"
STATUS_PASS="$WORKSPACE/.mneme-test/status-pass.json"

"$MNEME_BIN" outcome template --kind rust --include-judgment --task-id template-task --output "$TEMPLATE_ACCEPTANCE" --json > "$TEMPLATE_REPORT"
grep -q '"command": "outcome.template"' "$TEMPLATE_REPORT"
grep -q '"kind": "rust"' "$TEMPLATE_REPORT"
grep -q '"ok": true' "$TEMPLATE_REPORT"
"$MNEME_BIN" outcome validate "$TEMPLATE_ACCEPTANCE" --json > "$TEMPLATE_VALIDATE"
grep -q '"command": "outcome.validate"' "$TEMPLATE_VALIDATE"
grep -q '"ok": true' "$TEMPLATE_VALIDATE"

cat > "$BAD_ACCEPTANCE" <<'JSON'
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "bad-task",
  "criteria": [
    {
      "id": "bad-command",
      "kind": "command",
      "command": {}
    }
  ]
}
JSON

set +e
"$MNEME_BIN" outcome validate "$BAD_ACCEPTANCE" --json > "$BAD_VALIDATE"
BAD_VALIDATE_EXIT="$?"
set -e
if [ "$BAD_VALIDATE_EXIT" -eq 0 ]; then
  echo "outcome-gate-smoke: expected invalid acceptance validation to exit non-zero" >&2
  exit 1
fi
grep -q '"ok": false' "$BAD_VALIDATE"
grep -q 'must define argv' "$BAD_VALIDATE"

set +e
"$MNEME_BIN" begin "Reject bad gate" --acceptance "$BAD_ACCEPTANCE" --store "$STORE" --json > "$BAD_BEGIN" 2>&1
BAD_BEGIN_EXIT="$?"
set -e
if [ "$BAD_BEGIN_EXIT" -eq 0 ]; then
  echo "outcome-gate-smoke: expected begin with invalid acceptance to exit non-zero" >&2
  exit 1
fi

cat > "$ACCEPTANCE_PASS" <<'JSON'
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "pass-task",
  "criteria": [
    {
      "id": "command-main-exists",
      "kind": "command",
      "command": {"argv": ["sh", "-c", "test -f src/main.rs"], "expect_exit": 0}
    },
    {
      "id": "diff-touches-main",
      "kind": "diff_touches",
      "diff_touches": {"paths": ["src/main.rs"]}
    },
    {
      "id": "diff-scope-src",
      "kind": "diff_scope",
      "diff_scope": {"allowed_paths": ["src"]}
    },
    {
      "id": "symbol-marker",
      "kind": "symbol_present",
      "symbol_present": {"path": "src/main.rs", "symbol": "outcome_marker"}
    }
  ]
}
JSON

"$MNEME_BIN" begin "Implement outcome marker" --acceptance "$ACCEPTANCE_PASS" --store "$STORE" --json > "$BEGIN_PASS"
grep -q '"acceptance"' "$BEGIN_PASS"
printf '%s\n' 'pub fn outcome_marker() {}' >> src/main.rs
"$MNEME_BIN" end session-001 --summary "Implemented outcome marker" --verifier-command "$ROOT/scripts/mneme-outcome-verifier.py" --store "$STORE" --json > "$END_PASS"
grep -q '"status": "passed"' "$END_PASS"
grep -q '"completed": true' "$END_PASS"
"$MNEME_BIN" outcome status session-001 --store "$STORE" --json > "$STATUS_PASS"
grep -q '"command": "outcome.status"' "$STATUS_PASS"
grep -q '"status": "passed"' "$STATUS_PASS"

git add src/main.rs
git commit -q -m "outcome marker"

ACCEPTANCE_FAIL="$WORKSPACE/.mneme-test/acceptance-fail.json"
END_FAIL="$WORKSPACE/.mneme-test/end-fail.json"
cat > "$ACCEPTANCE_FAIL" <<'JSON'
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "fail-task",
  "criteria": [
    {
      "id": "diff-scope-src-only",
      "kind": "diff_scope",
      "diff_scope": {"allowed_paths": ["src"]}
    }
  ]
}
JSON

"$MNEME_BIN" begin "Touch out of scope file" --acceptance "$ACCEPTANCE_FAIL" --store "$STORE" --json > /dev/null
printf '%s\n' 'out of scope' > README.md
set +e
"$MNEME_BIN" end session-002 --summary "Touched readme" --verifier-command "$ROOT/scripts/mneme-outcome-verifier.py" --store "$STORE" --json > "$END_FAIL"
FAIL_EXIT="$?"
set -e
if [ "$FAIL_EXIT" -eq 0 ]; then
  echo "outcome-gate-smoke: expected failing gate to exit non-zero" >&2
  exit 1
fi
grep -q '"status": "failed"' "$END_FAIL"
grep -q '"completed": false' "$END_FAIL"

git add README.md
git commit -q -m "out of scope readme"

ACCEPTANCE_JUDGMENT_PASS="$WORKSPACE/.mneme-test/acceptance-judgment-pass.json"
END_JUDGMENT_PASS="$WORKSPACE/.mneme-test/end-judgment-pass.json"
JUDGE_PASS="$WORKSPACE/.mneme-test/judge-pass.json"
cat > "$ACCEPTANCE_JUDGMENT_PASS" <<'JSON'
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "judgment-pass-task",
  "criteria": [
    {
      "id": "external-ux-review",
      "kind": "judgment",
      "judgment": {"rubric": "external reviewer accepts the task outcome"}
    }
  ]
}
JSON

"$MNEME_BIN" begin "Need external review pass" --acceptance "$ACCEPTANCE_JUDGMENT_PASS" --store "$STORE" --json > /dev/null
set +e
"$MNEME_BIN" end session-003 --summary "Prepared reviewable outcome" --store "$STORE" --json > "$END_JUDGMENT_PASS"
PENDING_EXIT="$?"
set -e
if [ "$PENDING_EXIT" -eq 0 ]; then
  echo "outcome-gate-smoke: expected pending judgment to exit non-zero" >&2
  exit 1
fi
grep -q '"status": "pending_judgment"' "$END_JUDGMENT_PASS"
"$MNEME_BIN" outcome judge session-003 --id external-ux-review --verdict pass --evidence "external reviewer accepted outcome" --reviewer smoke-reviewer --task-id judgment-pass-task --store "$STORE" --json > "$JUDGE_PASS"
grep -q '"command": "outcome.judge"' "$JUDGE_PASS"
grep -q '"status": "passed"' "$JUDGE_PASS"
grep -q '"completed": true' "$JUDGE_PASS"

ACCEPTANCE_JUDGMENT_FAIL="$WORKSPACE/.mneme-test/acceptance-judgment-fail.json"
END_JUDGMENT_FAIL="$WORKSPACE/.mneme-test/end-judgment-fail.json"
JUDGE_FAIL="$WORKSPACE/.mneme-test/judge-fail.json"
cat > "$ACCEPTANCE_JUDGMENT_FAIL" <<'JSON'
{
  "schema_version": "mneme.acceptance.v1",
  "task_id": "judgment-fail-task",
  "criteria": [
    {
      "id": "external-doc-review",
      "kind": "judgment",
      "judgment": {"rubric": "external reviewer accepts documentation clarity"}
    }
  ]
}
JSON

"$MNEME_BIN" begin "Need external review fail" --acceptance "$ACCEPTANCE_JUDGMENT_FAIL" --store "$STORE" --json > /dev/null
set +e
"$MNEME_BIN" end session-004 --summary "Prepared unclear docs" --store "$STORE" --json > "$END_JUDGMENT_FAIL"
PENDING_FAIL_EXIT="$?"
set -e
if [ "$PENDING_FAIL_EXIT" -eq 0 ]; then
  echo "outcome-gate-smoke: expected pending judgment fail fixture to exit non-zero" >&2
  exit 1
fi
set +e
"$MNEME_BIN" outcome judge session-004 --id external-doc-review --verdict fail --evidence "reviewer rejected docs clarity" --reviewer smoke-reviewer --task-id judgment-fail-task --store "$STORE" --json > "$JUDGE_FAIL"
JUDGE_FAIL_EXIT="$?"
set -e
if [ "$JUDGE_FAIL_EXIT" -eq 0 ]; then
  echo "outcome-gate-smoke: expected failing judgment to exit non-zero" >&2
  exit 1
fi
grep -q '"status": "failed"' "$JUDGE_FAIL"
grep -q '"completed": false' "$JUDGE_FAIL"

echo "outcome-gate-smoke: ok"
