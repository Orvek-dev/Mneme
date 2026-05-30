#!/usr/bin/env sh
set -eu

ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$ROOT"

MODE="${1:-full}"
TMP_ROOT="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"

echo "quality-gate: mode=${MODE}"

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
python3 -m py_compile wrappers/openai_extractor.py
python3 -m py_compile scripts/v1-manual-dogfood.py
python3 -m py_compile scripts/v1-hard-dogfood.py
python3 -m py_compile scripts/v2-team-dogfood.py
python3 -m py_compile scripts/mcp-hard-dogfood.py
python3 -m py_compile scripts/mcp-client-continuity-smoke.py
python3 -m py_compile scripts/mneme-mcp-stdio.py
python3 -m py_compile scripts/v1-real-use-pilot.py
python3 -m py_compile scripts/v1-ontology-benchmark.py
python3 -m py_compile scripts/eval-integrity-check.py
python3 -m py_compile scripts/product-validation-loop.py
python3 -m py_compile scripts/product-review-summary.py
python3 -m py_compile scripts/product-dogfood-experiment.py
python3 -m py_compile scripts/product-heldout-gates.py
python3 -m py_compile scripts/long-horizon-scale-check.py
python3 -m py_compile scripts/mneme-outcome-verifier.py
scripts/mneme-mcp-stdio.py --self-test | grep -q '"tool_count": 13'
cargo run -q -p mneme-mcp -- --self-test | grep -q '"tool_count":46'
scripts/outcome-gate-smoke.sh | grep -q 'outcome-gate-smoke: ok'
scripts/eval-integrity-check.py | grep -q '"ok": true'
PRODUCT_VALIDATION_CONTRACT="${TMP_ROOT}/mneme-quality-gate-product-validation-contract.json"
PRODUCT_VALIDATION_DATASET="${TMP_ROOT}/mneme-quality-gate-product-validation-dataset.json"
PRODUCT_VALIDATION_OUT="${TMP_ROOT}/mneme-quality-gate-product-validation"
PRODUCT_VALIDATION_STDOUT="${TMP_ROOT}/mneme-quality-gate-product-validation.stdout.json"
rm -rf "$PRODUCT_VALIDATION_OUT"
scripts/product-validation-loop.py --check-contract > "$PRODUCT_VALIDATION_CONTRACT"
grep -q '"command": "product-validation-loop-contract"' "$PRODUCT_VALIDATION_CONTRACT"
grep -q '"id": "P1"' "$PRODUCT_VALIDATION_CONTRACT"
grep -q '"id": "P6"' "$PRODUCT_VALIDATION_CONTRACT"
scripts/product-validation-loop.py --check-dataset --record-count 180 > "$PRODUCT_VALIDATION_DATASET"
grep -q '"scripted_adoption_task_count": 4' "$PRODUCT_VALIDATION_DATASET"
grep -q '"privacy_cost_event_count": 4' "$PRODUCT_VALIDATION_DATASET"
grep -q '"ranking_case_count": 4' "$PRODUCT_VALIDATION_DATASET"
grep -q '"external_review_case_count": 1' "$PRODUCT_VALIDATION_DATASET"
scripts/product-validation-loop.py \
  --out-dir "$PRODUCT_VALIDATION_OUT" \
  --run-label quality-gate \
  --record-count 180 \
  --force \
  --no-build > "$PRODUCT_VALIDATION_STDOUT"
grep -q '"ok": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"scripted_memory_adoption_rate": 1.0' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"scripted_decision_change_rate": 1.0' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"requires_blind_review_for_value_claim": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"harmful_memory_count": 0' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"provider_opt_in_required": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"live_provider_executed": false' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"provider_budget_within_limit": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"long_horizon_actual_lifecycle_operations": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"long_horizon_scope_leak_count": 0' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"requires_external_embedding_eval_before_shipping": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"migration_memory_preserved": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"external_review_schema_valid": true' "$PRODUCT_VALIDATION_STDOUT"
grep -q '"third_party_claim": false' "$PRODUCT_VALIDATION_STDOUT"
PRODUCT_REVIEW_CONTRACT="${TMP_ROOT}/mneme-quality-gate-product-review-contract.json"
PRODUCT_REVIEW_EXAMPLE="${TMP_ROOT}/mneme-quality-gate-product-review-example.json"
scripts/product-review-summary.py --check-contract > "$PRODUCT_REVIEW_CONTRACT"
grep -q '"command": "product-review-summary-contract"' "$PRODUCT_REVIEW_CONTRACT"
scripts/product-review-summary.py --review examples/product-validation-review.example.json > "$PRODUCT_REVIEW_EXAMPLE"
grep -q '"ok": true' "$PRODUCT_REVIEW_EXAMPLE"
grep -q '"external_value_claim_allowed": false' "$PRODUCT_REVIEW_EXAMPLE"
grep -q '"win_rate": 1.0' "$PRODUCT_REVIEW_EXAMPLE"
PRODUCT_DOGFOOD_CONTRACT="${TMP_ROOT}/mneme-quality-gate-product-dogfood-contract.json"
PRODUCT_DOGFOOD_OUT="${TMP_ROOT}/mneme-quality-gate-product-dogfood"
PRODUCT_DOGFOOD_STDOUT="${TMP_ROOT}/mneme-quality-gate-product-dogfood.stdout.json"
PRODUCT_DOGFOOD_CHECK="${TMP_ROOT}/mneme-quality-gate-product-dogfood-check.json"
rm -rf "$PRODUCT_DOGFOOD_OUT"
scripts/product-dogfood-experiment.py --check-contract > "$PRODUCT_DOGFOOD_CONTRACT"
grep -q '"command": "product-dogfood-experiment-contract"' "$PRODUCT_DOGFOOD_CONTRACT"
scripts/product-dogfood-experiment.py \
  --out-dir "$PRODUCT_DOGFOOD_OUT" \
  --run-label quality-gate \
  --task-count 2 \
  --force \
  --no-build > "$PRODUCT_DOGFOOD_STDOUT"
grep -q '"ok": true' "$PRODUCT_DOGFOOD_STDOUT"
grep -q '"actual_agent_execution": false' "$PRODUCT_DOGFOOD_STDOUT"
scripts/product-dogfood-experiment.py --check-bundle "$PRODUCT_DOGFOOD_OUT" > "$PRODUCT_DOGFOOD_CHECK"
grep -q '"ok": true' "$PRODUCT_DOGFOOD_CHECK"
grep -q '"external_value_claim_allowed": false' "$PRODUCT_DOGFOOD_CHECK"
PRODUCT_HELDOUT_CONTRACT="${TMP_ROOT}/mneme-quality-gate-product-heldout-contract.json"
PRODUCT_HELDOUT_DATASET="${TMP_ROOT}/mneme-quality-gate-product-heldout-dataset.json"
PRODUCT_HELDOUT_STDOUT="${TMP_ROOT}/mneme-quality-gate-product-heldout.stdout.json"
scripts/product-heldout-gates.py --check-contract > "$PRODUCT_HELDOUT_CONTRACT"
grep -q '"command": "product-heldout-gates-contract"' "$PRODUCT_HELDOUT_CONTRACT"
scripts/product-heldout-gates.py --check-dataset > "$PRODUCT_HELDOUT_DATASET"
grep -q '"extraction_heldout_case_count": 4' "$PRODUCT_HELDOUT_DATASET"
grep -q '"ranking_heldout_case_count": 3' "$PRODUCT_HELDOUT_DATASET"
scripts/product-heldout-gates.py > "$PRODUCT_HELDOUT_STDOUT"
grep -q '"open_domain_extraction_claim_allowed": false' "$PRODUCT_HELDOUT_STDOUT"
grep -q '"semantic_search_claim_allowed": false' "$PRODUCT_HELDOUT_STDOUT"
grep -q '"heldout_evidence_ready": false' "$PRODUCT_HELDOUT_STDOUT"
LONG_HORIZON_CONTRACT="${TMP_ROOT}/mneme-quality-gate-long-horizon-contract.json"
LONG_HORIZON_OUT="${TMP_ROOT}/mneme-quality-gate-long-horizon"
LONG_HORIZON_STDOUT="${TMP_ROOT}/mneme-quality-gate-long-horizon.stdout.json"
rm -rf "$LONG_HORIZON_OUT"
scripts/long-horizon-scale-check.py --check-contract > "$LONG_HORIZON_CONTRACT"
grep -q '"command": "long-horizon-scale-check-contract"' "$LONG_HORIZON_CONTRACT"
scripts/long-horizon-scale-check.py \
  --record-counts 1000 \
  --out-dir "$LONG_HORIZON_OUT" \
  --no-build > "$LONG_HORIZON_STDOUT"
grep -q '"ok": true' "$LONG_HORIZON_STDOUT"
grep -q '"record_count": 1000' "$LONG_HORIZON_STDOUT"
grep -q '"scope_leak_count": 0' "$LONG_HORIZON_STDOUT"
MCP_READINESS_REPORT="${TMP_ROOT}/mneme-quality-gate-mcp-readiness.json"
MCP_AGENT_USABILITY_REPORT="${TMP_ROOT}/mneme-quality-gate-mcp-agent-usability.json"
cargo run -q -p mneme-eval -- validate --suite mcp >/dev/null
cargo run -q -p mneme-eval -- run --suite mcp --target mneme-mcp --json --report "$MCP_READINESS_REPORT" >/dev/null
grep -q '"ok": true' "$MCP_READINESS_REPORT"
grep -q '"target": "mneme-mcp"' "$MCP_READINESS_REPORT"
cargo run -q -p mneme-eval -- validate --suite mcp-agent-usability >/dev/null
cargo run -q -p mneme-eval -- run --suite mcp-agent-usability --target mneme-mcp --json --report "$MCP_AGENT_USABILITY_REPORT" >/dev/null
grep -q '"ok": true' "$MCP_AGENT_USABILITY_REPORT"
grep -q '"target": "mneme-mcp"' "$MCP_AGENT_USABILITY_REPORT"
MCP_HARD_CONTRACT="${TMP_ROOT}/mneme-quality-gate-mcp-hard-contract.json"
MCP_HARD_DATASET="${TMP_ROOT}/mneme-quality-gate-mcp-hard-dataset.json"
MCP_HARD_FAULTS="${TMP_ROOT}/mneme-quality-gate-mcp-hard-seeded-faults.json"
scripts/mcp-hard-dogfood.py --check-contract > "$MCP_HARD_CONTRACT"
grep -q '"command": "mcp-hard-dogfood-contract"' "$MCP_HARD_CONTRACT"
grep -q '"v1_normal_record_count": 100' "$MCP_HARD_CONTRACT"
grep -q '"v2_team_record_count": 120' "$MCP_HARD_CONTRACT"
scripts/mcp-hard-dogfood.py --check-dataset > "$MCP_HARD_DATASET"
grep -q '"normal_record_count": 100' "$MCP_HARD_DATASET"
grep -q '"adversarial_record_count": 150' "$MCP_HARD_DATASET"
grep -q '"team_record_count": 120' "$MCP_HARD_DATASET"
scripts/mcp-hard-dogfood.py --check-seeded-faults > "$MCP_HARD_FAULTS"
grep -q '"detection_rate": 1.0' "$MCP_HARD_FAULTS"
MCP_CLIENT_CONTRACT="${TMP_ROOT}/mneme-quality-gate-mcp-client-contract.json"
MCP_CLIENT_PROTOCOL="${TMP_ROOT}/mneme-quality-gate-mcp-client-protocol.json"
scripts/mcp-client-continuity-smoke.py --check-contract > "$MCP_CLIENT_CONTRACT"
grep -q '"command": "mcp-client-continuity-smoke-contract"' "$MCP_CLIENT_CONTRACT"
grep -q '"expected_tool_count": 46' "$MCP_CLIENT_CONTRACT"
scripts/mcp-client-continuity-smoke.py --protocol-only --no-build > "$MCP_CLIENT_PROTOCOL"
grep -q '"ok": true' "$MCP_CLIENT_PROTOCOL"
grep -q '"cross_agent_continuity": "passed"' "$MCP_CLIENT_PROTOCOL"
grep -q '"wrong_scope_guard": "passed"' "$MCP_CLIENT_PROTOCOL"
grep -q '"secret_context_guard": "passed"' "$MCP_CLIENT_PROTOCOL"
MANUAL_DOGFOOD_DATASET="${TMP_ROOT}/mneme-quality-gate-manual-dogfood-dataset.json"
scripts/v1-manual-dogfood.py --check-dataset > "$MANUAL_DOGFOOD_DATASET"
grep -q '"mock_record_count": 100' "$MANUAL_DOGFOOD_DATASET"
grep -q '"workflow_count": 25' "$MANUAL_DOGFOOD_DATASET"
HARD_DOGFOOD_CONTRACT="${TMP_ROOT}/mneme-quality-gate-hard-dogfood-contract.json"
HARD_DOGFOOD_DATASET="${TMP_ROOT}/mneme-quality-gate-hard-dogfood-dataset.json"
HARD_DOGFOOD_FAULTS="${TMP_ROOT}/mneme-quality-gate-hard-dogfood-seeded-faults.json"
HARD_DOGFOOD_CANDIDATE_DIR="${TMP_ROOT}/mneme-quality-gate-hard-dogfood-candidate"
HARD_DOGFOOD_CANDIDATE_CHECK="${TMP_ROOT}/mneme-quality-gate-hard-dogfood-candidate-check.json"
HARD_DOGFOOD_TREND="${TMP_ROOT}/mneme-quality-gate-hard-dogfood-trend.json"
rm -rf "$HARD_DOGFOOD_CANDIDATE_DIR"
mkdir -p "$HARD_DOGFOOD_CANDIDATE_DIR"
scripts/v1-hard-dogfood.py --check-contract > "$HARD_DOGFOOD_CONTRACT"
grep -q '"command": "v1-hard-dogfood-contract"' "$HARD_DOGFOOD_CONTRACT"
scripts/v1-hard-dogfood.py --check-dataset > "$HARD_DOGFOOD_DATASET"
grep -q '"normal_record_count": 100' "$HARD_DOGFOOD_DATASET"
grep -q '"adversarial_record_count": 150' "$HARD_DOGFOOD_DATASET"
grep -q '"agent_workflow_count": 30' "$HARD_DOGFOOD_DATASET"
scripts/v1-hard-dogfood.py --check-seeded-faults > "$HARD_DOGFOOD_FAULTS"
grep -q '"detection_rate": 1.0' "$HARD_DOGFOOD_FAULTS"
scripts/v1-hard-dogfood.py --check-official-candidate > "$HARD_DOGFOOD_CANDIDATE_DIR/hard-sample.candidate.yaml"
grep -q 'schema_version: mneme.eval_candidate.v1' "$HARD_DOGFOOD_CANDIDATE_DIR/hard-sample.candidate.yaml"
cargo run -q -p mneme-eval -- candidate-check "$HARD_DOGFOOD_CANDIDATE_DIR" --report "$HARD_DOGFOOD_CANDIDATE_CHECK" --json >/dev/null
grep -q '"ok": true' "$HARD_DOGFOOD_CANDIDATE_CHECK"
scripts/v1-hard-dogfood.py --check-trend > "$HARD_DOGFOOD_TREND"
grep -q '"command": "v1-hard-dogfood-trend"' "$HARD_DOGFOOD_TREND"
grep -q '"status": "compared"' "$HARD_DOGFOOD_TREND"
V2_TEAM_CONTRACT="${TMP_ROOT}/mneme-quality-gate-v2-team-contract.json"
V2_TEAM_DATASET="${TMP_ROOT}/mneme-quality-gate-v2-team-dataset.json"
V2_TEAM_FAULTS="${TMP_ROOT}/mneme-quality-gate-v2-team-seeded-faults.json"
V2_TEAM_AGENT_OPS="${TMP_ROOT}/mneme-quality-gate-v2-team-agent-ops"
scripts/v2-team-dogfood.py --check-contract > "$V2_TEAM_CONTRACT"
grep -q '"command": "v2-team-dogfood-contract"' "$V2_TEAM_CONTRACT"
grep -q '"team_record_count": 120' "$V2_TEAM_CONTRACT"
scripts/v2-team-dogfood.py --check-dataset > "$V2_TEAM_DATASET"
grep -q '"adversarial_record_count": 80' "$V2_TEAM_DATASET"
grep -q '"handoff_workflow_count": 25' "$V2_TEAM_DATASET"
scripts/v2-team-dogfood.py --check-seeded-faults > "$V2_TEAM_FAULTS"
grep -q '"detection_rate": 1.0' "$V2_TEAM_FAULTS"
rm -rf "$V2_TEAM_AGENT_OPS"
cargo build -q -p mneme-cli
MNEME_BIN="$ROOT/target/debug/mneme" examples/v2-team-agent-ops/run-demo.sh --out-dir "$V2_TEAM_AGENT_OPS" >/dev/null
grep -q '"ok": true' "$V2_TEAM_AGENT_OPS/reports/handoff-summary.json"
grep -q '"private_memory_redacted": true' "$V2_TEAM_AGENT_OPS/reports/handoff-summary.json"
grep -q '"quarantined_memory_omitted": true' "$V2_TEAM_AGENT_OPS/reports/handoff-summary.json"
grep -q '"sync_checksum_verified": true' "$V2_TEAM_AGENT_OPS/reports/handoff-summary.json"
grep -q '"quality_conflict_group_count": 1' "$V2_TEAM_AGENT_OPS/reports/handoff-summary.json"
grep -q '"denied_context_item_count": 0' "$V2_TEAM_AGENT_OPS/reports/handoff-summary.json"
grep -q 'Status: `pass`' "$V2_TEAM_AGENT_OPS/reports/readiness.md"
REAL_USE_CONTRACT="${TMP_ROOT}/mneme-quality-gate-real-use-contract.json"
REAL_USE_FEEDBACK="${TMP_ROOT}/mneme-quality-gate-real-use-feedback.json"
scripts/v1-real-use-pilot.py --check-contract > "$REAL_USE_CONTRACT"
grep -q '"command": "v1-real-use-pilot-contract"' "$REAL_USE_CONTRACT"
scripts/v1-real-use-pilot.py --check-feedback examples/v1-real-use-feedback.example.json > "$REAL_USE_FEEDBACK"
grep -q '"decision_status": "pilot_feedback_triaged"' "$REAL_USE_FEEDBACK"
ONTOLOGY_CONTRACT="${TMP_ROOT}/mneme-quality-gate-ontology-contract.json"
ONTOLOGY_FIXTURE="${TMP_ROOT}/mneme-quality-gate-ontology-fixture.json"
ONTOLOGY_SCORER="${TMP_ROOT}/mneme-quality-gate-ontology-scorer.json"
ONTOLOGY_GAP_ANALYSIS="${TMP_ROOT}/mneme-quality-gate-ontology-gap-analysis.json"
scripts/v1-ontology-benchmark.py --check-contract > "$ONTOLOGY_CONTRACT"
grep -q '"command": "v1-ontology-benchmark-contract"' "$ONTOLOGY_CONTRACT"
scripts/v1-ontology-benchmark.py --check-fixture > "$ONTOLOGY_FIXTURE"
grep -q '"case_count": 14' "$ONTOLOGY_FIXTURE"
grep -q '"natural_language": 11' "$ONTOLOGY_FIXTURE"
grep -q '"relation_count": 19' "$ONTOLOGY_FIXTURE"
scripts/v1-ontology-benchmark.py --check-scorer > "$ONTOLOGY_SCORER"
grep -q '"ok": true' "$ONTOLOGY_SCORER"
grep -q '"dropped_relation"' "$ONTOLOGY_SCORER"
scripts/v1-ontology-benchmark.py --check-gap-analysis > "$ONTOLOGY_GAP_ANALYSIS"
grep -q '"command": "v1-ontology-benchmark-gap-analysis"' "$ONTOLOGY_GAP_ANALYSIS"
grep -q '"readiness_status": "v1_ontology_design_needed"' "$ONTOLOGY_GAP_ANALYSIS"
grep -q '"capability": "relation_mapping"' "$ONTOLOGY_GAP_ANALYSIS"

cargo run -p mneme-cli -- doctor
cargo run -p mneme-eval -- doctor

ONTOLOGY_RUN_DIR="${TMP_ROOT}/mneme-quality-gate-ontology-run"
ONTOLOGY_RUN_STDOUT="${TMP_ROOT}/mneme-quality-gate-ontology-run.txt"
rm -rf "$ONTOLOGY_RUN_DIR"
scripts/v1-ontology-benchmark.py --run-label quality-gate --out-dir "$ONTOLOGY_RUN_DIR" --force --no-build > "$ONTOLOGY_RUN_STDOUT"
grep -q 'decision ontology_benchmark_passed' "$ONTOLOGY_RUN_STDOUT"
grep -q '"decision_status": "ontology_benchmark_passed"' "$ONTOLOGY_RUN_DIR/summary.json"
grep -q '"readiness_status": "v1_ontology_ready"' "$ONTOLOGY_RUN_DIR/summary.json"
grep -q '"entity_f1": 1.0' "$ONTOLOGY_RUN_DIR/scorecard.json"
grep -q '"relation_f1": 1.0' "$ONTOLOGY_RUN_DIR/scorecard.json"
grep -q '"attribute_f1": 1.0' "$ONTOLOGY_RUN_DIR/scorecard.json"

INSTALL_ROOT="${TMP_ROOT}/mneme-quality-gate-install"
INSTALL_STDOUT="${TMP_ROOT}/mneme-quality-gate-install.txt"
INSTALL_DOCTOR="${TMP_ROOT}/mneme-quality-gate-installed-doctor.txt"
INSTALL_DOCTOR_PRE="${TMP_ROOT}/mneme-quality-gate-installed-doctor-pre.json"
INSTALL_DOCTOR_POST="${TMP_ROOT}/mneme-quality-gate-installed-doctor-post.json"
INSTALL_DOCTOR_BAD_PROFILE="${TMP_ROOT}/mneme-quality-gate-installed-doctor-bad-profile.json"
INSTALL_DOCTOR_BAD_STORE="${TMP_ROOT}/mneme-quality-gate-installed-doctor-bad-store.json"
INSTALL_REPAIR_CHECK_VALID="${TMP_ROOT}/mneme-quality-gate-installed-repair-check-valid.json"
INSTALL_REPAIR_CHECK_BAD="${TMP_ROOT}/mneme-quality-gate-installed-repair-check-bad.json"
INSTALL_HELP="${TMP_ROOT}/mneme-quality-gate-installed-help.txt"
INSTALL_MCP_SELF_TEST="${TMP_ROOT}/mneme-quality-gate-installed-mcp-self-test.json"
INSTALL_CONTEXT="${TMP_ROOT}/mneme-quality-gate-installed-context.json"
INSTALL_STORE="${TMP_ROOT}/mneme-quality-gate-installed-cli.json"
INSTALL_REVIEW="${TMP_ROOT}/mneme-quality-gate-installed-review.md"
INSTALL_WORKSPACE="${TMP_ROOT}/mneme-quality-gate-installed-workspace"
INSTALL_INIT="${TMP_ROOT}/mneme-quality-gate-installed-init.json"
INSTALL_PROFILE="${INSTALL_WORKSPACE}/.mneme/mneme-agent-hook.env"
INSTALL_WORKSPACE_STORE="${INSTALL_WORKSPACE}/.mneme/mneme-v1.json"
INSTALL_WRAPPER_DOCTOR="${TMP_ROOT}/mneme-quality-gate-installed-wrapper-doctor.txt"
INSTALL_WRAPPER_BEGIN="${TMP_ROOT}/mneme-quality-gate-installed-wrapper-begin.json"
INSTALL_WRAPPER_END="${TMP_ROOT}/mneme-quality-gate-installed-wrapper-end.json"
QUICKSTART_SMOKE="${TMP_ROOT}/mneme-quality-gate-quickstart-smoke.txt"
rm -rf "$INSTALL_ROOT" "$INSTALL_WORKSPACE"
rm -f "$INSTALL_STDOUT" "$INSTALL_DOCTOR" "$INSTALL_HELP" "$INSTALL_CONTEXT" "$INSTALL_STORE" "$INSTALL_REVIEW" \
  "$INSTALL_INIT" "$INSTALL_DOCTOR_PRE" "$INSTALL_DOCTOR_POST" "$INSTALL_DOCTOR_BAD_PROFILE" \
  "$INSTALL_DOCTOR_BAD_STORE" "$INSTALL_REPAIR_CHECK_VALID" "$INSTALL_REPAIR_CHECK_BAD" \
  "$INSTALL_MCP_SELF_TEST" "$INSTALL_WRAPPER_DOCTOR" "$INSTALL_WRAPPER_BEGIN" "$INSTALL_WRAPPER_END" "$QUICKSTART_SMOKE"
./scripts/install-local.sh --root "$INSTALL_ROOT" --debug > "$INSTALL_STDOUT"
grep -q 'mneme-install: ok' "$INSTALL_STDOUT"
INSTALL_BIN="${INSTALL_ROOT}/bin/mneme"
INSTALL_MCP_BIN="${INSTALL_ROOT}/bin/mneme-mcp"
test -x "$INSTALL_MCP_BIN"
"$INSTALL_MCP_BIN" --self-test > "$INSTALL_MCP_SELF_TEST"
grep -q '"tool_count":46' "$INSTALL_MCP_SELF_TEST"
"$INSTALL_BIN" doctor > "$INSTALL_DOCTOR"
grep -q 'Mneme local CLI' "$INSTALL_DOCTOR"
"$INSTALL_BIN" help > "$INSTALL_HELP"
grep -q 'mneme help begin' "$INSTALL_HELP"
"$INSTALL_BIN" remember "user prefers installed CLI workflows" --store "$INSTALL_STORE"
"$INSTALL_BIN" context "installed CLI" --store "$INSTALL_STORE" --json > "$INSTALL_CONTEXT"
grep -q 'installed CLI workflows' "$INSTALL_CONTEXT"
"$INSTALL_BIN" review "$INSTALL_REVIEW" --store "$INSTALL_STORE"
grep -q '# Mneme Memory Review' "$INSTALL_REVIEW"
mkdir -p "$INSTALL_WORKSPACE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" doctor --json > "$INSTALL_DOCTOR_PRE")
grep -q '"command": "doctor"' "$INSTALL_DOCTOR_PRE"
grep -q '"ok": false' "$INSTALL_DOCTOR_PRE"
grep -q '"status": "missing"' "$INSTALL_DOCTOR_PRE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" init --bin "$INSTALL_BIN" --json > "$INSTALL_INIT")
grep -q '"command": "init"' "$INSTALL_INIT"
grep -q '"store_created": true' "$INSTALL_INIT"
grep -q '"config_written": true' "$INSTALL_INIT"
test -f "$INSTALL_WORKSPACE_STORE"
test -f "$INSTALL_PROFILE"
grep -Fq "MNEME_BIN=$INSTALL_BIN" "$INSTALL_PROFILE"
grep -Fq "MNEME_STORE=" "$INSTALL_PROFILE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" doctor --json > "$INSTALL_DOCTOR_POST")
grep -q '"command": "doctor"' "$INSTALL_DOCTOR_POST"
grep -q '"ok": true' "$INSTALL_DOCTOR_POST"
grep -q '"name": "store.current"' "$INSTALL_DOCTOR_POST"
grep -q '"name": "profile.agent_hook"' "$INSTALL_DOCTOR_POST"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" repair --check --json > "$INSTALL_REPAIR_CHECK_VALID")
grep -q '"mode": "check"' "$INSTALL_REPAIR_CHECK_VALID"
grep -q '"action": "current_valid"' "$INSTALL_REPAIR_CHECK_VALID"
grep -q '"ok": true' "$INSTALL_REPAIR_CHECK_VALID"
printf '%s\n' "MNEME_STORE=$INSTALL_WORKSPACE_STORE" "UNKNOWN=value" > "$INSTALL_PROFILE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" doctor --json > "$INSTALL_DOCTOR_BAD_PROFILE")
grep -q '"ok": false' "$INSTALL_DOCTOR_BAD_PROFILE"
grep -q 'unknown profile key' "$INSTALL_DOCTOR_BAD_PROFILE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" init --force --bin "$INSTALL_BIN" --json > "$INSTALL_INIT")
printf '{not-json\n' > "$INSTALL_WORKSPACE_STORE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" doctor --json > "$INSTALL_DOCTOR_BAD_STORE")
grep -q '"ok": false' "$INSTALL_DOCTOR_BAD_STORE"
grep -q '"name": "store.current"' "$INSTALL_DOCTOR_BAD_STORE"
grep -q '"status": "fail"' "$INSTALL_DOCTOR_BAD_STORE"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" repair --check --json > "$INSTALL_REPAIR_CHECK_BAD")
grep -q '"mode": "check"' "$INSTALL_REPAIR_CHECK_BAD"
grep -q '"action": "repair_available"' "$INSTALL_REPAIR_CHECK_BAD"
grep -q '"ok": true' "$INSTALL_REPAIR_CHECK_BAD"
(cd "$INSTALL_WORKSPACE" && "$INSTALL_BIN" init --force --bin "$INSTALL_BIN" --json > "$INSTALL_INIT")
"$INSTALL_BIN" validate --store "$INSTALL_WORKSPACE_STORE"
MNEME_AGENT_HOOK_CONFIG="$INSTALL_PROFILE" ./scripts/mneme-agent-hook.sh doctor > "$INSTALL_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: config=' "$INSTALL_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: config_loaded=true' "$INSTALL_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: hook_smoke=ok' "$INSTALL_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: extractor_smoke=skipped:not-configured' "$INSTALL_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: ok' "$INSTALL_WRAPPER_DOCTOR"
"$INSTALL_BIN" remember "user prefers bootstrap workflows" --store "$INSTALL_WORKSPACE_STORE"
MNEME_AGENT_HOOK_CONFIG="$INSTALL_PROFILE" \
  ./scripts/mneme-agent-hook.sh begin "Draft bootstrap plan" --query "bootstrap workflows" > "$INSTALL_WRAPPER_BEGIN"
grep -q '"operation": "begin"' "$INSTALL_WRAPPER_BEGIN"
grep -q '"session_id": "session-001"' "$INSTALL_WRAPPER_BEGIN"
MNEME_AGENT_HOOK_CONFIG="$INSTALL_PROFILE" \
  ./scripts/mneme-agent-hook.sh end session-001 --summary "Prepared bootstrap plan" > "$INSTALL_WRAPPER_END"
grep -q '"operation": "end"' "$INSTALL_WRAPPER_END"
MNEME_BIN="$INSTALL_BIN" scripts/quickstart-smoke.sh > "$QUICKSTART_SMOKE"
grep -q 'quickstart-smoke: ok' "$QUICKSTART_SMOKE"

MNEME_HELP="${TMP_ROOT}/mneme-quality-gate-help.txt"
MNEME_EVAL_HELP="${TMP_ROOT}/mneme-quality-gate-eval-help.txt"
rm -f "$MNEME_HELP" "$MNEME_EVAL_HELP"
cargo run -p mneme-cli -- help > "$MNEME_HELP"
grep -q "Usage:" "$MNEME_HELP"
grep -q "mneme init" "$MNEME_HELP"
grep -q "mneme help begin" "$MNEME_HELP"
cargo run -p mneme-cli -- init --help > "$MNEME_HELP"
grep -q "Usage: mneme init" "$MNEME_HELP"
grep -q -- "--config <path>" "$MNEME_HELP"
grep -q -- "--force" "$MNEME_HELP"
cargo run -p mneme-cli -- doctor --help > "$MNEME_HELP"
grep -q "Usage: mneme doctor" "$MNEME_HELP"
grep -q -- "--config <path>" "$MNEME_HELP"
./scripts/mneme-agent-hook.sh help > "$MNEME_HELP"
grep -q "scripts/mneme-agent-hook.sh doctor \[--check-extractor\]" "$MNEME_HELP"
grep -q -- "--check-extractor" "$MNEME_HELP"
cargo run -p mneme-cli -- begin --help > "$MNEME_HELP"
grep -q "Usage: mneme begin" "$MNEME_HELP"
grep -q -- "--acceptance <path>" "$MNEME_HELP"
cargo run -p mneme-cli -- outcome --help > "$MNEME_HELP"
grep -q "mneme outcome status" "$MNEME_HELP"
grep -q "mneme outcome judge" "$MNEME_HELP"
cargo run -p mneme-cli -- hook --help > "$MNEME_HELP"
grep -q "mneme.agent_hook.v1" "$MNEME_HELP"
grep -q "mneme hook doctor" "$MNEME_HELP"
grep -q -- "--verifier-command <program>" "$MNEME_HELP"
cargo run -p mneme-cli -- mcp --help > "$MNEME_HELP"
grep -q "mneme mcp config" "$MNEME_HELP"
cargo run -p mneme-cli -- mcp config --client all --json > "$MNEME_HELP"
grep -q '"command": "mcp.config"' "$MNEME_HELP"
grep -q '"client": "codex"' "$MNEME_HELP"
grep -q '"client": "claude-code"' "$MNEME_HELP"
grep -q '"client": "cursor"' "$MNEME_HELP"
cargo run -p mneme-cli -- review --help > "$MNEME_HELP"
grep -q "Usage: mneme review" "$MNEME_HELP"
grep -q -- "--format markdown|json" "$MNEME_HELP"
grep -q -- "--include-sensitive" "$MNEME_HELP"
cargo run -p mneme-cli -- quality --help > "$MNEME_HELP"
grep -q "Usage: mneme quality" "$MNEME_HELP"
grep -q "duplicate active claims" "$MNEME_HELP"
cargo run -p mneme-cli -- curate --help > "$MNEME_HELP"
grep -q "Usage: mneme curate" "$MNEME_HELP"
grep -q -- "--apply" "$MNEME_HELP"
grep -q -- "--compact" "$MNEME_HELP"
cargo run -p mneme-cli -- restore --help > "$MNEME_HELP"
grep -q "mneme restore --check" "$MNEME_HELP"
grep -q "roll back" "$MNEME_HELP"
cargo run -p mneme-eval -- help > "$MNEME_EVAL_HELP"
grep -q "Usage:" "$MNEME_EVAL_HELP"
grep -q "mneme-eval help baseline" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- baseline-gate --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval baseline-gate" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- baseline-summary --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval baseline-summary" "$MNEME_EVAL_HELP"
grep -q "provider triage" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- baseline-compare --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval baseline-compare" "$MNEME_EVAL_HELP"
grep -q -- "--fail-on-regression" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- candidate --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval candidate" "$MNEME_EVAL_HELP"
grep -q "scenario candidate artifacts" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- candidate-check --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval candidate-check" "$MNEME_EVAL_HELP"
grep -q "Validate local scenario candidate" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- candidate-promote --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval candidate-promote" "$MNEME_EVAL_HELP"
grep -q -- "--scenario-root <dir>" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- v1-readiness --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval v1-readiness" "$MNEME_EVAL_HELP"
grep -q "dogfood" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- v2-readiness --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval v2-readiness" "$MNEME_EVAL_HELP"
grep -q "team-memory readiness" "$MNEME_EVAL_HELP"
cargo run -p mneme-eval -- dogfood-summary --help > "$MNEME_EVAL_HELP"
grep -q "Usage: mneme-eval dogfood-summary" "$MNEME_EVAL_HELP"
grep -q "ready_for_manual_dogfood" "$MNEME_EVAL_HELP"

STORE="${TMP_ROOT}/mneme-quality-gate-cli.json"
rm -f "$STORE"
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store "$STORE"
CLAIMS_REPORT="${TMP_ROOT}/mneme-quality-gate-claims.json"
rm -f "$CLAIMS_REPORT"
cargo run -p mneme-cli -- claims --status active --store "$STORE" --json > "$CLAIMS_REPORT"
grep -q '"claim_count": 1' "$CLAIMS_REPORT"
grep -q '"id": "claim-001"' "$CLAIMS_REPORT"
cargo run -p mneme-cli -- context "local-first" --store "$STORE" --json | grep -q "local-first tools"
TEAM_STORE="${TMP_ROOT}/mneme-quality-gate-team-v2.json"
TEAM_CONTEXT="${TMP_ROOT}/mneme-quality-gate-team-context.json"
TEAM_HANDOFF="${TMP_ROOT}/mneme-quality-gate-team-handoff.json"
TEAM_RUN_BEGIN="${TMP_ROOT}/mneme-quality-gate-team-run-begin.json"
TEAM_RUN_NOTE="${TMP_ROOT}/mneme-quality-gate-team-run-note.json"
TEAM_RUN_END="${TMP_ROOT}/mneme-quality-gate-team-run-end.json"
TEAM_RUN_HANDOFF="${TMP_ROOT}/mneme-quality-gate-team-run-handoff.json"
TEAM_SYNC="${TMP_ROOT}/mneme-quality-gate-team-sync.json"
TEAM_SYNC_EXPORT="${TMP_ROOT}/mneme-quality-gate-team-sync-export.json"
TEAM_FIREWALL="${TMP_ROOT}/mneme-quality-gate-team-firewall.json"
TEAM_QUALITY="${TMP_ROOT}/mneme-quality-gate-team-quality.json"
TEAM_ONTOLOGY="${TMP_ROOT}/mneme-quality-gate-team-ontology.json"
TEAM_ADAPTER="${TMP_ROOT}/mneme-quality-gate-team-adapter.json"
TEAM_VALIDATE="${TMP_ROOT}/mneme-quality-gate-team-validate.json"
rm -f "$TEAM_STORE" "$TEAM_CONTEXT" "$TEAM_HANDOFF" "$TEAM_SYNC" "$TEAM_SYNC_EXPORT" \
  "$TEAM_RUN_BEGIN" "$TEAM_RUN_NOTE" "$TEAM_RUN_END" "$TEAM_RUN_HANDOFF" \
  "$TEAM_FIREWALL" "$TEAM_QUALITY" "$TEAM_ONTOLOGY" "$TEAM_ADAPTER" "$TEAM_VALIDATE"
cargo run -p mneme-cli -- team init --admin alice --store "$TEAM_STORE" --json | grep -q '"command": "team.init"'
cargo run -p mneme-cli -- team user add bob --role member --store "$TEAM_STORE" --json | grep -q '"command": "team.user.add"'
cargo run -p mneme-cli -- team agent add codex-bob --owner bob --store "$TEAM_STORE" --json | grep -q '"command": "team.agent.add"'
cargo run -p mneme-cli -- team project add atlas --member bob --store "$TEAM_STORE" --json | grep -q '"command": "team.project.add"'
cargo run -p mneme-cli -- team remember "Atlas deploys require rollback notes" --actor bob --agent codex-bob --scope project:atlas --store "$TEAM_STORE" --json | grep -q '"id": "team-memory-001"'
cargo run -p mneme-cli -- team promote team-memory-001 --actor bob --agent codex-bob --store "$TEAM_STORE" --json | grep -q '"status": "pending"'
cargo run -p mneme-cli -- team review team-promotion-001 --actor alice --approve --store "$TEAM_STORE" --json | grep -q '"status": "approved"'
cargo run -p mneme-cli -- team context "rollback notes" --actor alice --store "$TEAM_STORE" --json > "$TEAM_CONTEXT"
grep -q '"item_count": 1' "$TEAM_CONTEXT"
grep -q 'rollback notes' "$TEAM_CONTEXT"
cargo run -p mneme-cli -- team handoff "rollback notes" --actor bob --agent codex-bob --store "$TEAM_STORE" --json > "$TEAM_HANDOFF"
grep -q '"command": "team.handoff"' "$TEAM_HANDOFF"
grep -q '"schema_version": "mneme.team_handoff.v1"' "$TEAM_HANDOFF"
cargo run -p mneme-cli -- team run begin "Atlas deploy handoff" --actor bob --agent codex-bob --query "rollback notes" --scope project:atlas --store "$TEAM_STORE" --json > "$TEAM_RUN_BEGIN"
grep -q '"command": "team.run.begin"' "$TEAM_RUN_BEGIN"
grep -q '"id": "team-run-001"' "$TEAM_RUN_BEGIN"
cargo run -p mneme-cli -- team run note team-run-001 "Atlas run requires smoke test" --actor bob --agent codex-bob --scope project:atlas --store "$TEAM_STORE" --json > "$TEAM_RUN_NOTE"
grep -q '"command": "team.run.note"' "$TEAM_RUN_NOTE"
cargo run -p mneme-cli -- team run end team-run-001 --actor bob --agent codex-bob --summary "Rollback notes reviewed" --next "Run smoke test" --store "$TEAM_STORE" --json > "$TEAM_RUN_END"
grep -q '"status": "closed"' "$TEAM_RUN_END"
cargo run -p mneme-cli -- team run handoff team-run-001 --actor bob --agent codex-bob --store "$TEAM_STORE" --json > "$TEAM_RUN_HANDOFF"
grep -q '"command": "team.run.handoff"' "$TEAM_RUN_HANDOFF"
grep -q '"run": {' "$TEAM_RUN_HANDOFF"
cargo run -p mneme-cli -- team sync export "$TEAM_SYNC" --actor bob --agent codex-bob --include-projects --store "$TEAM_STORE" --json > "$TEAM_SYNC_EXPORT"
grep -q '"command": "team.sync.export"' "$TEAM_SYNC_EXPORT"
grep -q '"schema_version": "mneme.team_sync.v1"' "$TEAM_SYNC"
cargo run -p mneme-cli -- team promotion report team-promotion-001 --store "$TEAM_STORE" --json | grep -q '"command": "team.promotion.report"'
cargo run -p mneme-cli -- team sync import "$TEAM_SYNC" --store "$TEAM_STORE" --json | grep -q '"mode": "dry_run"'
cargo run -p mneme-cli -- team firewall --store "$TEAM_STORE" --json > "$TEAM_FIREWALL"
grep -q '"command": "team.firewall"' "$TEAM_FIREWALL"
grep -q '"ok": true' "$TEAM_FIREWALL"
cargo run -p mneme-cli -- team quality --store "$TEAM_STORE" --json > "$TEAM_QUALITY"
grep -q '"command": "team.quality"' "$TEAM_QUALITY"
grep -q '"health"' "$TEAM_QUALITY"
cargo run -p mneme-cli -- team ontology --store "$TEAM_STORE" --json > "$TEAM_ONTOLOGY"
grep -q '"command": "team.ontology"' "$TEAM_ONTOLOGY"
grep -q '"relation_count"' "$TEAM_ONTOLOGY"
cargo run -p mneme-cli -- team adapter manifest --json > "$TEAM_ADAPTER"
grep -q '"command": "team.adapter.manifest"' "$TEAM_ADAPTER"
grep -q 'mneme.team.handoff' "$TEAM_ADAPTER"
cargo run -p mneme-cli -- team validate --store "$TEAM_STORE" --json > "$TEAM_VALIDATE"
grep -q '"ok": true' "$TEAM_VALIDATE"
REVIEW_STORE="${TMP_ROOT}/mneme-quality-gate-review.json"
REVIEW_MD="${TMP_ROOT}/mneme-quality-gate-review.md"
REVIEW_JSON="${TMP_ROOT}/mneme-quality-gate-review-artifact.json"
REVIEW_RAW_JSON="${TMP_ROOT}/mneme-quality-gate-review-raw-artifact.json"
REVIEW_STDOUT="${TMP_ROOT}/mneme-quality-gate-review-stdout.json"
rm -f "$REVIEW_STORE" "$REVIEW_MD" "$REVIEW_JSON" "$REVIEW_RAW_JSON" "$REVIEW_STDOUT"
cargo run -p mneme-cli -- remember "user prefers review artifacts" --store "$REVIEW_STORE"
cargo run -p mneme-cli -- remember "user note API_KEY=FAKE_TEST_VALUE" --store "$REVIEW_STORE"
cargo run -p mneme-cli -- review "$REVIEW_MD" --store "$REVIEW_STORE" --json > "$REVIEW_STDOUT"
grep -q '"command": "review"' "$REVIEW_STDOUT"
grep -q '"format": "markdown"' "$REVIEW_STDOUT"
grep -q '"policy": "default_safe"' "$REVIEW_STDOUT"
grep -q '# Mneme Memory Review' "$REVIEW_MD"
grep -q 'blocked_secret' "$REVIEW_MD"
grep -q '\[redacted:blocked_secret\]' "$REVIEW_MD"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$REVIEW_STDOUT" "$REVIEW_MD"; then
  echo "quality-gate: safe review artifact leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- review "$REVIEW_JSON" --format json --store "$REVIEW_STORE"
grep -q '"format": "json"' "$REVIEW_JSON"
grep -q '"policy": "default_safe"' "$REVIEW_JSON"
grep -q '"blocked_secret_claim_count": 1' "$REVIEW_JSON"
grep -q '"object": "\[redacted:blocked_secret\]"' "$REVIEW_JSON"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$REVIEW_JSON"; then
  echo "quality-gate: safe JSON review artifact leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- review "$REVIEW_RAW_JSON" --format json --include-sensitive --store "$REVIEW_STORE"
grep -q '"policy": "include_sensitive"' "$REVIEW_RAW_JSON"
grep -q 'API_KEY=FAKE_TEST_VALUE' "$REVIEW_RAW_JSON"
QUALITY_STORE="${TMP_ROOT}/mneme-quality-gate-quality.json"
QUALITY_REPORT="${TMP_ROOT}/mneme-quality-gate-quality-report.json"
QUALITY_REVIEW="${TMP_ROOT}/mneme-quality-gate-quality-review.md"
CURATE_REPORT="${TMP_ROOT}/mneme-quality-gate-curate-report.json"
CURATE_APPLY="${TMP_ROOT}/mneme-quality-gate-curate-apply.json"
CURATE_FINAL="${TMP_ROOT}/mneme-quality-gate-curate-final-quality.json"
RESTORE_CHECK="${TMP_ROOT}/mneme-quality-gate-restore-check.json"
RESTORE_APPLY="${TMP_ROOT}/mneme-quality-gate-restore-apply.json"
RESTORE_QUALITY="${TMP_ROOT}/mneme-quality-gate-restore-quality.json"
RESTORE_SWAPBACK="${TMP_ROOT}/mneme-quality-gate-restore-swapback-quality.json"
rm -f "$QUALITY_STORE" "$QUALITY_REPORT" "$QUALITY_REVIEW" "$CURATE_REPORT" "$CURATE_APPLY" "$CURATE_FINAL" \
  "$RESTORE_CHECK" "$RESTORE_APPLY" "$RESTORE_QUALITY" "$RESTORE_SWAPBACK" "${QUALITY_STORE}.bak"
cargo run -p mneme-cli -- remember "user prefers quality loops" --store "$QUALITY_STORE"
cargo run -p mneme-cli -- remember "user prefers quality loops" --store "$QUALITY_STORE"
cargo run -p mneme-cli -- remember "user token API_KEY=FAKE_TEST_VALUE" --store "$QUALITY_STORE"
cargo run -p mneme-cli -- remember "user prefers old review notes" --store "$QUALITY_STORE"
cargo run -p mneme-cli -- correct --claim-id claim-004 "user prefers current review notes" --store "$QUALITY_STORE"
cargo run -p mneme-cli -- quality --store "$QUALITY_STORE" --json > "$QUALITY_REPORT"
grep -q '"command": "quality"' "$QUALITY_REPORT"
grep -q '"health": "attention_required"' "$QUALITY_REPORT"
grep -q '"duplicate_active_group_count": 1' "$QUALITY_REPORT"
grep -q '"blocked_secret_claim_count": 1' "$QUALITY_REPORT"
grep -q '"inactive_claim_count": 1' "$QUALITY_REPORT"
grep -q '"kind": "duplicate_active"' "$QUALITY_REPORT"
grep -q '"kind": "blocked_secret"' "$QUALITY_REPORT"
grep -q '"kind": "inactive_history"' "$QUALITY_REPORT"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$QUALITY_REPORT"; then
  echo "quality-gate: quality report leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- review "$QUALITY_REVIEW" --store "$QUALITY_STORE"
grep -q '## Memory Quality' "$QUALITY_REVIEW"
grep -q 'duplicate_active' "$QUALITY_REVIEW"
grep -q 'inactive_history' "$QUALITY_REVIEW"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$QUALITY_REVIEW"; then
  echo "quality-gate: quality review artifact leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- curate --store "$QUALITY_STORE" --json > "$CURATE_REPORT"
grep -q '"command": "curate"' "$CURATE_REPORT"
grep -q '"mode": "dry_run"' "$CURATE_REPORT"
grep -q '"changed": false' "$CURATE_REPORT"
grep -q '"duplicate_forget_count": 1' "$CURATE_REPORT"
grep -q '"blocked_secret_review_count": 1' "$CURATE_REPORT"
grep -q '"compact_target_count": 3' "$CURATE_REPORT"
grep -q '"kind": "forget_duplicate_active"' "$CURATE_REPORT"
grep -q '"kind": "compact_non_active_records"' "$CURATE_REPORT"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$CURATE_REPORT"; then
  echo "quality-gate: curate dry-run leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- curate --apply --compact --store "$QUALITY_STORE" --json > "$CURATE_APPLY"
grep -q '"mode": "apply"' "$CURATE_APPLY"
grep -q '"changed": true' "$CURATE_APPLY"
grep -q '"forgotten_claim_count": 1' "$CURATE_APPLY"
grep -q '"compacted": true' "$CURATE_APPLY"
grep -q '"health": "ok"' "$CURATE_APPLY"
grep -q '"duplicate_active_group_count": 0' "$CURATE_APPLY"
grep -q '"blocked_secret_claim_count": 0' "$CURATE_APPLY"
grep -q '"inactive_claim_count": 0' "$CURATE_APPLY"
grep -q 'mneme restore --check' "$CURATE_APPLY"
test -f "${QUALITY_STORE}.bak"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$CURATE_APPLY"; then
  echo "quality-gate: curate apply leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- quality --store "$QUALITY_STORE" --json > "$CURATE_FINAL"
grep -q '"health": "ok"' "$CURATE_FINAL"
grep -q '"review_item_count": 0' "$CURATE_FINAL"
cargo run -p mneme-cli -- restore --check --store "$QUALITY_STORE" --json > "$RESTORE_CHECK"
grep -q '"command": "restore"' "$RESTORE_CHECK"
grep -q '"mode": "check"' "$RESTORE_CHECK"
grep -q '"action": "restore_available"' "$RESTORE_CHECK"
grep -q '"restore_available": true' "$RESTORE_CHECK"
cargo run -p mneme-cli -- restore --store "$QUALITY_STORE" --json > "$RESTORE_APPLY"
grep -q '"mode": "restore"' "$RESTORE_APPLY"
grep -q '"action": "restored_from_backup"' "$RESTORE_APPLY"
grep -q '"restored": true' "$RESTORE_APPLY"
grep -q '"current_preserved_as_backup": true' "$RESTORE_APPLY"
cargo run -p mneme-cli -- quality --store "$QUALITY_STORE" --json > "$RESTORE_QUALITY"
grep -q '"health": "attention_required"' "$RESTORE_QUALITY"
grep -q '"duplicate_active_group_count": 1' "$RESTORE_QUALITY"
grep -q '"blocked_secret_claim_count": 1' "$RESTORE_QUALITY"
grep -q '"inactive_claim_count": 1' "$RESTORE_QUALITY"
if grep -q 'API_KEY=FAKE_TEST_VALUE' "$RESTORE_QUALITY"; then
  echo "quality-gate: restored quality report leaked secret text" >&2
  exit 1
fi
cargo run -p mneme-cli -- restore --store "$QUALITY_STORE"
cargo run -p mneme-cli -- quality --store "$QUALITY_STORE" --json > "$RESTORE_SWAPBACK"
grep -q '"health": "ok"' "$RESTORE_SWAPBACK"
grep -q '"review_item_count": 0' "$RESTORE_SWAPBACK"
SCOPE_STORE="${TMP_ROOT}/mneme-quality-gate-scope.json"
SCOPE_DENIED="${TMP_ROOT}/mneme-quality-gate-scope-denied.json"
SCOPE_ALLOWED="${TMP_ROOT}/mneme-quality-gate-scope-allowed.json"
SCOPE_BEGIN="${TMP_ROOT}/mneme-quality-gate-scope-begin.json"
rm -f "$SCOPE_STORE" "$SCOPE_DENIED" "$SCOPE_ALLOWED" "$SCOPE_BEGIN"
cargo run -p mneme-cli -- remember "user prefers project launch reviews" --scope project-alpha --store "$SCOPE_STORE"
cargo run -p mneme-cli -- context "project launch" --store "$SCOPE_STORE" --json > "$SCOPE_DENIED"
grep -q '"item_count": 0' "$SCOPE_DENIED"
grep -q 'scope_denied:project-alpha' "$SCOPE_DENIED"
cargo run -p mneme-cli -- context "project launch" --scope project-alpha --store "$SCOPE_STORE" --json > "$SCOPE_ALLOWED"
grep -q 'project launch reviews' "$SCOPE_ALLOWED"
cargo run -p mneme-cli -- begin "Draft launch plan" --query "project launch" --scope project-alpha --agent codex --store "$SCOPE_STORE" --json > "$SCOPE_BEGIN"
grep -q 'project launch reviews' "$SCOPE_BEGIN"
RANK_STORE="${TMP_ROOT}/mneme-quality-gate-rank.json"
RANK_CONTEXT="${TMP_ROOT}/mneme-quality-gate-rank-context.json"
rm -f "$RANK_STORE" "$RANK_CONTEXT"
cargo run -p mneme-cli -- remember "user prefers launch templates" --store "$RANK_STORE"
cargo run -p mneme-cli -- remember "user prefers review summaries" --store "$RANK_STORE"
cargo run -p mneme-cli -- remember "user prefers launch review checklists" --store "$RANK_STORE"
cargo run -p mneme-cli -- context "launch review" --max-items 1 --store "$RANK_STORE" --json > "$RANK_CONTEXT"
grep -q '"item_count": 1' "$RANK_CONTEXT"
grep -q 'launch review checklists' "$RANK_CONTEXT"
grep -q '"score": 25' "$RANK_CONTEXT"
grep -q 'context_budget_exceeded:max_items=1' "$RANK_CONTEXT"
BEGIN_REPORT="${TMP_ROOT}/mneme-quality-gate-begin.json"
END_REPORT="${TMP_ROOT}/mneme-quality-gate-end.json"
rm -f "$BEGIN_REPORT" "$END_REPORT"
cargo run -p mneme-cli -- begin "Draft setup plan" --query "local-first" --agent codex --store "$STORE" --json > "$BEGIN_REPORT"
grep -q "session-001" "$BEGIN_REPORT"
cargo run -p mneme-cli -- end session-001 --summary "Prepared a concise setup plan" --remember "user prefers concise setup plans" --store "$STORE" --json > "$END_REPORT"
grep -q "claim-002" "$END_REPORT"
cargo run -p mneme-cli -- context "concise" --store "$STORE" --json | grep -q "concise setup plans"
ID_STORE="${TMP_ROOT}/mneme-quality-gate-id-lifecycle.json"
ID_ACTIVE="${TMP_ROOT}/mneme-quality-gate-id-lifecycle-active.json"
ID_CONTEXT="${TMP_ROOT}/mneme-quality-gate-id-lifecycle-context.json"
rm -f "$ID_STORE" "$ID_ACTIVE" "$ID_CONTEXT"
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store "$ID_STORE"
cargo run -p mneme-cli -- remember "user prefers local-first tools" --store "$ID_STORE"
cargo run -p mneme-cli -- forget --claim-id claim-001 --store "$ID_STORE"
cargo run -p mneme-cli -- claims --status active --store "$ID_STORE" --json > "$ID_ACTIVE"
grep -q '"claim_count": 1' "$ID_ACTIVE"
grep -q '"id": "claim-002"' "$ID_ACTIVE"
cargo run -p mneme-cli -- correct --claim-id claim-002 "user prefers terminal workflows" --store "$ID_STORE"
cargo run -p mneme-cli -- context "terminal workflows" --store "$ID_STORE" --json > "$ID_CONTEXT"
grep -q 'terminal workflows' "$ID_CONTEXT"
grep -q '"claim_id": "claim-003"' "$ID_CONTEXT"
HOOK_STORE="${TMP_ROOT}/mneme-quality-gate-hook.json"
HOOK_DOCTOR="${TMP_ROOT}/mneme-quality-gate-hook-doctor.json"
HOOK_BEGIN="${TMP_ROOT}/mneme-quality-gate-hook-begin.json"
HOOK_END="${TMP_ROOT}/mneme-quality-gate-hook-end.json"
HOOK_ERROR="${TMP_ROOT}/mneme-quality-gate-hook-error.json"
rm -f "$HOOK_STORE" "$HOOK_DOCTOR" "$HOOK_BEGIN" "$HOOK_END" "$HOOK_ERROR"
cargo run -p mneme-cli -- hook doctor --store "$HOOK_STORE" > "$HOOK_DOCTOR"
grep -q '"operation": "doctor"' "$HOOK_DOCTOR"
grep -q '"schema_version": "mneme.agent_hook.v1"' "$HOOK_DOCTOR"
cargo run -p mneme-cli -- remember "user prefers hook workflows" --store "$HOOK_STORE"
cargo run -p mneme-cli -- hook begin "Draft hook plan" --query "hook workflows" --agent codex --store "$HOOK_STORE" > "$HOOK_BEGIN"
grep -q '"schema_version": "mneme.agent_hook.v1"' "$HOOK_BEGIN"
grep -q '"operation": "begin"' "$HOOK_BEGIN"
grep -q '"session_id": "session-001"' "$HOOK_BEGIN"
cargo run -p mneme-cli -- hook end session-001 --summary "Prepared hook plan" --remember "user prefers stable hook contracts" --store "$HOOK_STORE" > "$HOOK_END"
grep -q '"operation": "end"' "$HOOK_END"
grep -q '"remembered_claim_count": 1' "$HOOK_END"
if cargo run -p mneme-cli -- hook end session-404 --summary "Missing session" --store "$HOOK_STORE" > "$HOOK_ERROR"; then
  echo "quality-gate: hook error unexpectedly passed" >&2
  exit 1
fi
grep -q '"ok": false' "$HOOK_ERROR"
grep -q '"kind": "session"' "$HOOK_ERROR"
LOCKED_STORE="${TMP_ROOT}/mneme-quality-gate-locked.json"
LOCKED_ERROR="${TMP_ROOT}/mneme-quality-gate-locked-error.json"
rm -f "$LOCKED_STORE" "$LOCKED_STORE.lock" "$LOCKED_ERROR"
printf '%s\n' "held by quality gate" > "$LOCKED_STORE.lock"
if cargo run -p mneme-cli -- hook begin "Draft locked plan" --store "$LOCKED_STORE" > "$LOCKED_ERROR"; then
  echo "quality-gate: locked hook unexpectedly passed" >&2
  exit 1
fi
grep -q '"kind": "store_lock"' "$LOCKED_ERROR"
grep -q '"recoverable": true' "$LOCKED_ERROR"
rm -f "$LOCKED_STORE.lock"
WRAPPER_DOCTOR="${TMP_ROOT}/mneme-quality-gate-wrapper-doctor.txt"
WRAPPER_STORE="${TMP_ROOT}/mneme-quality-gate-wrapper.json"
WRAPPER_BEGIN="${TMP_ROOT}/mneme-quality-gate-wrapper-begin.json"
WRAPPER_END="${TMP_ROOT}/mneme-quality-gate-wrapper-end.json"
WRAPPER_CONFIG="${TMP_ROOT}/mneme-quality-gate-wrapper.env"
WRAPPER_CONFIG_STORE="${TMP_ROOT}/mneme-quality-gate-wrapper-config.json"
WRAPPER_CONFIG_DOCTOR="${TMP_ROOT}/mneme-quality-gate-wrapper-config-doctor.txt"
WRAPPER_CONFIG_BEGIN="${TMP_ROOT}/mneme-quality-gate-wrapper-config-begin.json"
WRAPPER_CONFIG_END="${TMP_ROOT}/mneme-quality-gate-wrapper-config-end.json"
rm -f "$WRAPPER_DOCTOR" "$WRAPPER_STORE" "$WRAPPER_STORE.bak" "$WRAPPER_STORE.lock" "$WRAPPER_BEGIN" "$WRAPPER_END" \
  "$WRAPPER_CONFIG" "$WRAPPER_CONFIG_STORE" "$WRAPPER_CONFIG_STORE.bak" "$WRAPPER_CONFIG_STORE.lock" \
  "$WRAPPER_CONFIG_DOCTOR" "$WRAPPER_CONFIG_BEGIN" "$WRAPPER_CONFIG_END"
./scripts/mneme-agent-hook.sh doctor > "$WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: config_loaded=false' "$WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: hook_smoke=ok' "$WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: extractor_smoke=skipped:not-configured' "$WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: ok' "$WRAPPER_DOCTOR"
cargo run -p mneme-cli -- remember "user prefers wrapper workflows" --store "$WRAPPER_STORE"
MNEME_STORE="$WRAPPER_STORE" MNEME_AGENT_ID=codex MNEME_SCOPE=private MNEME_MAX_ITEMS=2 \
  ./scripts/mneme-agent-hook.sh begin "Draft wrapper plan" --query "wrapper workflows" > "$WRAPPER_BEGIN"
grep -q '"operation": "begin"' "$WRAPPER_BEGIN"
grep -q '"session_id": "session-001"' "$WRAPPER_BEGIN"
MNEME_STORE="$WRAPPER_STORE" MNEME_AGENT_ID=codex \
  ./scripts/mneme-agent-hook.sh end session-001 --summary "Prepared wrapper plan" > "$WRAPPER_END"
grep -q '"operation": "end"' "$WRAPPER_END"
cat > "$WRAPPER_CONFIG" <<EOF
MNEME_STORE=$WRAPPER_CONFIG_STORE
MNEME_AGENT_ID=codex
MNEME_SCOPE=private
MNEME_MAX_ITEMS=2
EOF
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_CONFIG" ./scripts/mneme-agent-hook.sh doctor > "$WRAPPER_CONFIG_DOCTOR"
grep -q 'mneme-agent-hook: config=' "$WRAPPER_CONFIG_DOCTOR"
grep -q 'mneme-agent-hook: config_loaded=true' "$WRAPPER_CONFIG_DOCTOR"
grep -q "mneme-agent-hook: store=$WRAPPER_CONFIG_STORE" "$WRAPPER_CONFIG_DOCTOR"
grep -q 'mneme-agent-hook: hook_smoke=ok' "$WRAPPER_CONFIG_DOCTOR"
grep -q 'mneme-agent-hook: extractor_smoke=skipped:not-configured' "$WRAPPER_CONFIG_DOCTOR"
grep -q 'mneme-agent-hook: ok' "$WRAPPER_CONFIG_DOCTOR"
cargo run -p mneme-cli -- remember "user prefers config profiles" --store "$WRAPPER_CONFIG_STORE"
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_CONFIG" \
  ./scripts/mneme-agent-hook.sh begin "Draft config profile plan" --query "config profiles" > "$WRAPPER_CONFIG_BEGIN"
grep -q '"operation": "begin"' "$WRAPPER_CONFIG_BEGIN"
grep -q '"session_id": "session-001"' "$WRAPPER_CONFIG_BEGIN"
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_CONFIG" \
  ./scripts/mneme-agent-hook.sh end session-001 --summary "Prepared config profile plan" > "$WRAPPER_CONFIG_END"
grep -q '"operation": "end"' "$WRAPPER_CONFIG_END"
cargo run -p mneme-cli -- validate --store "$STORE"
EXPORT_STORE="${TMP_ROOT}/mneme-quality-gate-export.json"
IMPORT_STORE="${TMP_ROOT}/mneme-quality-gate-import.json"
REPAIR_CHECK_VALID="${TMP_ROOT}/mneme-quality-gate-repair-check-valid.json"
REPAIR_CHECK_BAD="${TMP_ROOT}/mneme-quality-gate-repair-check-bad.json"
REPAIR_RUN="${TMP_ROOT}/mneme-quality-gate-repair-run.json"
rm -f "$EXPORT_STORE" "$IMPORT_STORE" "$IMPORT_STORE.bak" "$REPAIR_CHECK_VALID" "$REPAIR_CHECK_BAD" "$REPAIR_RUN"
cargo run -p mneme-cli -- export "$EXPORT_STORE" --store "$STORE"
cargo run -p mneme-cli -- import "$EXPORT_STORE" --store "$IMPORT_STORE"
cargo run -p mneme-cli -- compact --store "$IMPORT_STORE"
cargo run -p mneme-cli -- validate --store "$IMPORT_STORE"
cargo run -p mneme-cli -- repair --check --store "$IMPORT_STORE" --json > "$REPAIR_CHECK_VALID"
grep -q '"mode": "check"' "$REPAIR_CHECK_VALID"
grep -q '"action": "current_valid"' "$REPAIR_CHECK_VALID"
grep -q '"ok": true' "$REPAIR_CHECK_VALID"
cargo run -p mneme-cli -- remember "user prefers repairable stores" --store "$IMPORT_STORE"
printf '{not-json\n' > "$IMPORT_STORE"
cargo run -p mneme-cli -- repair --check --store "$IMPORT_STORE" --json > "$REPAIR_CHECK_BAD"
grep -q '"mode": "check"' "$REPAIR_CHECK_BAD"
grep -q '"action": "repair_available"' "$REPAIR_CHECK_BAD"
grep -q '"ok": true' "$REPAIR_CHECK_BAD"
cargo run -p mneme-cli -- repair --store "$IMPORT_STORE" --json > "$REPAIR_RUN"
grep -q '"mode": "repair"' "$REPAIR_RUN"
grep -q '"action": "restored_from_backup"' "$REPAIR_RUN"
grep -q '"repaired": true' "$REPAIR_RUN"
cargo run -p mneme-cli -- validate --store "$IMPORT_STORE"

COMMAND_STORE="${TMP_ROOT}/mneme-quality-gate-command.json"
rm -f "$COMMAND_STORE"
RESPONSE='{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"command-backed extraction"}}'
cargo run -p mneme-cli -- ingest "the user likes model-backed extraction" \
  --extractor command \
  --extractor-command /bin/sh \
  --extractor-arg -c \
  --extractor-arg "cat >/dev/null; printf '%s\n' '${RESPONSE}'" \
  --store "$COMMAND_STORE"
cargo run -p mneme-cli -- context "command-backed" --store "$COMMAND_STORE" --json | grep -q "command-backed extraction"

AGENT_COMMAND_STORE="${TMP_ROOT}/mneme-quality-gate-agent-command.json"
AGENT_COMMAND_END="${TMP_ROOT}/mneme-quality-gate-agent-command-end.json"
rm -f "$AGENT_COMMAND_STORE" "$AGENT_COMMAND_END"
cargo run -p mneme-cli -- hook begin "Draft planning docs" \
  --agent codex \
  --store "$AGENT_COMMAND_STORE" > /dev/null
cargo run -p mneme-cli -- hook end session-001 \
  --summary "Prepared planning docs" \
  --remember "For future planning docs, keep explanations direct and skip motivational language." \
  --extractor command \
  --extractor-command evals/fixtures/command-extractor.sh \
  --store "$AGENT_COMMAND_STORE" > "$AGENT_COMMAND_END"
grep -q '"extractor": "command"' "$AGENT_COMMAND_END"
grep -q '"remembered_claim_count": 1' "$AGENT_COMMAND_END"
cargo run -p mneme-cli -- context "planning docs" --store "$AGENT_COMMAND_STORE" --json | grep -q "direct explanations"

WRAPPER_COMMAND_STORE="${TMP_ROOT}/mneme-quality-gate-wrapper-command.json"
WRAPPER_COMMAND_CONFIG="${TMP_ROOT}/mneme-quality-gate-wrapper-command.env"
WRAPPER_COMMAND_DOCTOR="${TMP_ROOT}/mneme-quality-gate-wrapper-command-doctor.json"
WRAPPER_COMMAND_WRAPPER_DOCTOR="${TMP_ROOT}/mneme-quality-gate-wrapper-command-wrapper-doctor.txt"
WRAPPER_COMMAND_EXTRACTOR_DOCTOR="${TMP_ROOT}/mneme-quality-gate-wrapper-command-extractor-doctor.txt"
WRAPPER_COMMAND_END="${TMP_ROOT}/mneme-quality-gate-wrapper-command-end.json"
WRAPPER_FAILING_COMMAND_CONFIG="${TMP_ROOT}/mneme-quality-gate-wrapper-failing-command.env"
WRAPPER_FAILING_COMMAND_DOCTOR="${TMP_ROOT}/mneme-quality-gate-wrapper-failing-command-doctor.txt"
rm -f "$WRAPPER_COMMAND_STORE" "$WRAPPER_COMMAND_CONFIG" "$WRAPPER_COMMAND_DOCTOR" \
  "$WRAPPER_COMMAND_WRAPPER_DOCTOR" "$WRAPPER_COMMAND_EXTRACTOR_DOCTOR" "$WRAPPER_COMMAND_END" \
  "$WRAPPER_FAILING_COMMAND_CONFIG" "$WRAPPER_FAILING_COMMAND_DOCTOR"
cargo run -p mneme-cli -- init \
  --store "$WRAPPER_COMMAND_STORE" \
  --config "$WRAPPER_COMMAND_CONFIG" \
  --no-bin \
  --extractor-command evals/fixtures/command-extractor.sh \
  --force \
  --json > /dev/null
grep -q '^MNEME_EXTRACTOR_COMMAND=evals/fixtures/command-extractor.sh$' "$WRAPPER_COMMAND_CONFIG"
cargo run -p mneme-cli -- doctor \
  --store "$WRAPPER_COMMAND_STORE" \
  --config "$WRAPPER_COMMAND_CONFIG" \
  --json > "$WRAPPER_COMMAND_DOCTOR"
grep -q '"mneme_extractor_command": "evals/fixtures/command-extractor.sh"' "$WRAPPER_COMMAND_DOCTOR"
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_COMMAND_CONFIG" scripts/mneme-agent-hook.sh doctor > "$WRAPPER_COMMAND_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: extractor_command=evals/fixtures/command-extractor.sh' "$WRAPPER_COMMAND_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: hook_smoke=ok' "$WRAPPER_COMMAND_WRAPPER_DOCTOR"
grep -q 'mneme-agent-hook: extractor_smoke=skipped:requires --check-extractor' "$WRAPPER_COMMAND_WRAPPER_DOCTOR"
if grep -q 'mneme-agent-hook: extractor_smoke=ok' "$WRAPPER_COMMAND_WRAPPER_DOCTOR"; then
  echo "quality-gate: wrapper doctor ran extractor smoke without --check-extractor" >&2
  exit 1
fi
cat > "$WRAPPER_FAILING_COMMAND_CONFIG" <<EOF
MNEME_STORE=$WRAPPER_COMMAND_STORE
MNEME_AGENT_ID=codex
MNEME_SCOPE=private
MNEME_MAX_ITEMS=3
MNEME_EXTRACTOR_COMMAND=/bin/false
EOF
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_FAILING_COMMAND_CONFIG" scripts/mneme-agent-hook.sh doctor > "$WRAPPER_FAILING_COMMAND_DOCTOR"
grep -q 'mneme-agent-hook: extractor_command=/bin/false' "$WRAPPER_FAILING_COMMAND_DOCTOR"
grep -q 'mneme-agent-hook: extractor_smoke=skipped:requires --check-extractor' "$WRAPPER_FAILING_COMMAND_DOCTOR"
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_COMMAND_CONFIG" scripts/mneme-agent-hook.sh doctor --check-extractor > "$WRAPPER_COMMAND_EXTRACTOR_DOCTOR"
grep -q 'mneme-agent-hook: extractor_smoke=ok' "$WRAPPER_COMMAND_EXTRACTOR_DOCTOR"
grep -q 'mneme-agent-hook: ok' "$WRAPPER_COMMAND_EXTRACTOR_DOCTOR"
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_COMMAND_CONFIG" scripts/mneme-agent-hook.sh begin "Draft planning docs" \
  --query "planning docs" > /dev/null
MNEME_AGENT_HOOK_CONFIG="$WRAPPER_COMMAND_CONFIG" scripts/mneme-agent-hook.sh end session-001 \
  --summary "Prepared planning docs" \
  --remember "For future planning docs, keep explanations direct and skip motivational language." \
  > "$WRAPPER_COMMAND_END"
grep -q '"extractor": "command"' "$WRAPPER_COMMAND_END"
grep -q '"remembered_claim_count": 1' "$WRAPPER_COMMAND_END"
cargo run -p mneme-cli -- context "planning docs" --store "$WRAPPER_COMMAND_STORE" --json | grep -q "direct explanations"

cargo run -p mneme-eval -- validate --suite core
cargo run -p mneme-eval -- validate --suite model
cargo run -p mneme-eval -- validate --suite runtime
cargo run -p mneme-eval -- validate --suite agent
cargo run -p mneme-eval -- validate --suite dogfood
cargo run -p mneme-eval -- validate --suite team

for scenario in evals/fixtures/invalid/*.yaml; do
  if cargo run -p mneme-eval -- validate "$scenario"; then
    echo "quality-gate: invalid scenario unexpectedly passed validation: $scenario" >&2
    exit 1
  fi
done

cargo run -p mneme-eval -- run --suite core --target fake
cargo run -p mneme-eval -- run --suite core --target mneme-v1
cargo run -p mneme-eval -- run --suite runtime --target fake
cargo run -p mneme-eval -- run --suite runtime --target mneme-v1
cargo run -p mneme-eval -- run --suite agent --target fake
cargo run -p mneme-eval -- run --suite agent --target mneme-v1
cargo run -p mneme-eval -- run --suite dogfood --target fake
cargo run -p mneme-eval -- run --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- run --suite team --target mneme-v2
cargo run -p mneme-eval -- run --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh

MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py

if cargo run -p mneme-eval -- run --suite core --target fake --seeded-fault skip-claims; then
  echo "quality-gate: seeded fault unexpectedly passed" >&2
  exit 1
fi
if cargo run -p mneme-eval -- run --suite team --target mneme-v2 --seeded-fault bypass-acl; then
  echo "quality-gate: v2 seeded fault unexpectedly passed" >&2
  exit 1
fi

cargo run -p mneme-eval -- acceptance --suite core --target fake
cargo run -p mneme-eval -- acceptance --suite core --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite runtime --target fake
cargo run -p mneme-eval -- acceptance --suite runtime --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite agent --target fake
cargo run -p mneme-eval -- acceptance --suite agent --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite dogfood --target fake
cargo run -p mneme-eval -- acceptance --suite dogfood --target mneme-v1
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
cargo run -p mneme-eval -- acceptance --suite model --target mneme-v1-command --extractor-command evals/fixtures/command-extractor.sh

MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- acceptance --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py

BASELINE_REPORT="${TMP_ROOT}/mneme-openai-wrapper-baseline.json"
BASELINE_GATE_REPORT="${TMP_ROOT}/mneme-openai-wrapper-baseline-gate.json"
BASELINE_GATE_STDOUT="${TMP_ROOT}/mneme-openai-wrapper-baseline-gate.stdout.json"
BASELINE_SUMMARY_REPORT="${TMP_ROOT}/mneme-openai-wrapper-baseline-summary.json"
BASELINE_SUMMARY_STDOUT="${TMP_ROOT}/mneme-openai-wrapper-baseline-summary.stdout.json"
CORE_BASELINE_REPORT="${TMP_ROOT}/mneme-core-baseline.json"
CORE_BASELINE_STDOUT="${TMP_ROOT}/mneme-core-baseline.stdout.json"
BASELINE_COMPARE_REPORT="${TMP_ROOT}/mneme-baseline-compare.json"
BASELINE_COMPARE_STDOUT="${TMP_ROOT}/mneme-baseline-compare.stdout.json"
BASELINE_COMPARE_FAIL_STDOUT="${TMP_ROOT}/mneme-baseline-compare-fail.stdout.json"
FAILED_BASELINE_REPORT="${TMP_ROOT}/mneme-seeded-fault-baseline.json"
FAILED_BASELINE_STDOUT="${TMP_ROOT}/mneme-seeded-fault-baseline.stdout.json"
FAILED_BASELINE_SUMMARY="${TMP_ROOT}/mneme-seeded-fault-baseline-summary.json"
FAILED_BASELINE_SUMMARY_STDOUT="${TMP_ROOT}/mneme-seeded-fault-baseline-summary.stdout.json"
CANDIDATE_DIR="${TMP_ROOT}/mneme-quality-gate-candidates"
CANDIDATE_REPORT="${TMP_ROOT}/mneme-quality-gate-candidates.json"
CANDIDATE_STDOUT="${TMP_ROOT}/mneme-quality-gate-candidates.stdout.json"
CANDIDATE_CHECK_REPORT="${TMP_ROOT}/mneme-quality-gate-candidate-check.json"
CANDIDATE_CHECK_STDOUT="${TMP_ROOT}/mneme-quality-gate-candidate-check.stdout.json"
CANDIDATE_PROMOTE_ROOT="${TMP_ROOT}/mneme-quality-gate-promoted-scenarios"
CANDIDATE_PROMOTE_REPORT="${TMP_ROOT}/mneme-quality-gate-candidate-promote.json"
CANDIDATE_PROMOTE_STDOUT="${TMP_ROOT}/mneme-quality-gate-candidate-promote.stdout.json"
V1_READINESS_REPORT="${TMP_ROOT}/mneme-quality-gate-v1-readiness.json"
V1_READINESS_STDOUT="${TMP_ROOT}/mneme-quality-gate-v1-readiness.stdout.json"
V2_READINESS_REPORT="${TMP_ROOT}/mneme-quality-gate-v2-readiness.json"
V2_READINESS_STDOUT="${TMP_ROOT}/mneme-quality-gate-v2-readiness.stdout.json"
DOGFOOD_OUT_DIR="${TMP_ROOT}/mneme-quality-gate-v1-dogfood"
V2_DOGFOOD_OUT_DIR="${TMP_ROOT}/mneme-quality-gate-v2-dogfood"
PROMOTED_SCENARIO="${CANDIDATE_PROMOTE_ROOT}/dogfood/dogfood-curation-restore-from-backup.yaml"
rm -rf "$CANDIDATE_DIR" "$CANDIDATE_PROMOTE_ROOT" "$DOGFOOD_OUT_DIR" "$V2_DOGFOOD_OUT_DIR"
rm -f "$CANDIDATE_REPORT" "$CANDIDATE_STDOUT" "$CANDIDATE_CHECK_REPORT" "$CANDIDATE_CHECK_STDOUT" \
  "$CANDIDATE_PROMOTE_REPORT" "$CANDIDATE_PROMOTE_STDOUT" \
  "$CORE_BASELINE_REPORT" "$CORE_BASELINE_STDOUT" "$BASELINE_COMPARE_REPORT" \
  "$BASELINE_COMPARE_STDOUT" "$BASELINE_COMPARE_FAIL_STDOUT" \
  "$V1_READINESS_REPORT" "$V1_READINESS_STDOUT" "$V2_READINESS_REPORT" "$V2_READINESS_STDOUT"
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- baseline --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py \
  --iterations 2 \
  --provider-label openai \
  --model-label dry-run \
  --run-label "${MODE}-dry-run" \
  --json | tee "$BASELINE_REPORT"
grep -q '"provider_label": "openai"' "$BASELINE_REPORT"
grep -q '"model_label": "dry-run"' "$BASELINE_REPORT"
grep -q '"scenario_count": 14' "$BASELINE_REPORT"
grep -q '"category": "agent"' "$BASELINE_REPORT"
grep -q '"category": "communication"' "$BASELINE_REPORT"
grep -q '"category": "format"' "$BASELINE_REPORT"
grep -q '"category": "project"' "$BASELINE_REPORT"
grep -q '"category": "no-claim"' "$BASELINE_REPORT"
grep -q '"passed_iterations": 2' "$BASELINE_REPORT"
grep -q '"failed_scenario_runs": 0' "$BASELINE_REPORT"
grep -q '"failure_summary"' "$BASELINE_REPORT"

cargo run -p mneme-eval -- baseline-summary "$BASELINE_REPORT" \
  --report "$BASELINE_SUMMARY_REPORT" \
  --json > "$BASELINE_SUMMARY_STDOUT"
grep -q '"command": "baseline-summary"' "$BASELINE_SUMMARY_STDOUT"
grep -q '"triage_status": "passing"' "$BASELINE_SUMMARY_REPORT"
grep -q '"failed_category_count": 0' "$BASELINE_SUMMARY_REPORT"
grep -q '"redaction_findings": \[\]' "$BASELINE_SUMMARY_REPORT"
grep -q 'dry-run evidence' "$BASELINE_SUMMARY_REPORT"

cargo run -p mneme-eval -- baseline --suite core \
  --target fake \
  --iterations 1 \
  --report "$CORE_BASELINE_REPORT" \
  --json > "$CORE_BASELINE_STDOUT"
grep -q '"suite": "core"' "$CORE_BASELINE_REPORT"
grep -q '"ok": true' "$CORE_BASELINE_REPORT"

if cargo run -p mneme-eval -- baseline --suite core \
  --target fake \
  --seeded-fault skip-claims \
  --iterations 1 \
  --report "$FAILED_BASELINE_REPORT" \
  --json > "$FAILED_BASELINE_STDOUT"; then
  echo "quality-gate: seeded fault baseline unexpectedly passed" >&2
  exit 1
fi
cargo run -p mneme-eval -- baseline-summary "$FAILED_BASELINE_REPORT" \
  --report "$FAILED_BASELINE_SUMMARY" \
  --json > "$FAILED_BASELINE_SUMMARY_STDOUT"
grep -q '"triage_status": "failing_redaction_required"' "$FAILED_BASELINE_SUMMARY"
grep -q '"failed_scenario_count": 11' "$FAILED_BASELINE_SUMMARY"
grep -q 'API_KEY=' "$FAILED_BASELINE_SUMMARY"
grep -q 'redact or keep local before sharing' "$FAILED_BASELINE_SUMMARY"
grep -q '"top_failed_checks"' "$FAILED_BASELINE_SUMMARY"

cargo run -p mneme-eval -- baseline-compare "$CORE_BASELINE_REPORT" "$FAILED_BASELINE_REPORT" \
  --report "$BASELINE_COMPARE_REPORT" \
  --json > "$BASELINE_COMPARE_STDOUT"
grep -q '"command": "baseline-compare"' "$BASELINE_COMPARE_STDOUT"
grep -q '"regression_detected": true' "$BASELINE_COMPARE_REPORT"
grep -q '"new_failed_scenarios": \[' "$BASELINE_COMPARE_REPORT"
grep -q '"new_failed_checks": \[' "$BASELINE_COMPARE_REPORT"
if cargo run -p mneme-eval -- baseline-compare "$CORE_BASELINE_REPORT" "$FAILED_BASELINE_REPORT" \
  --fail-on-regression \
  --json > "$BASELINE_COMPARE_FAIL_STDOUT"; then
  echo "quality-gate: baseline-compare unexpectedly passed with --fail-on-regression" >&2
  exit 1
fi

cargo run -p mneme-eval -- candidate "$FAILED_BASELINE_REPORT" \
  --out-dir "$CANDIDATE_DIR" \
  --limit 3 \
  --prefix dogfood \
  --report "$CANDIDATE_REPORT" \
  --json > "$CANDIDATE_STDOUT"
grep -q '"command": "candidate"' "$CANDIDATE_STDOUT"
grep -q '"candidate_count": 3' "$CANDIDATE_REPORT"
grep -q '"redaction_finding_codes": \[' "$CANDIDATE_REPORT"
grep -q 'api_key_assignment' "$CANDIDATE_REPORT"
test -f "$CANDIDATE_DIR/dogfood-curation-restore-from-backup.candidate.yaml"
if rg -n 'API_KEY=FAKE_TEST_VALUE|OPENAI_API_KEY|sk-' "$CANDIDATE_DIR"; then
  echo "quality-gate: candidate artifact leaked redaction-sensitive text" >&2
  exit 1
fi
cargo run -p mneme-eval -- candidate-check "$CANDIDATE_DIR" \
  --report "$CANDIDATE_CHECK_REPORT" \
  --json > "$CANDIDATE_CHECK_STDOUT"
grep -q '"command": "candidate-check"' "$CANDIDATE_CHECK_STDOUT"
grep -q '"ok": true' "$CANDIDATE_CHECK_REPORT"
grep -q '"valid": 3' "$CANDIDATE_CHECK_REPORT"

cargo run -p mneme-eval -- candidate-promote "$CANDIDATE_DIR/dogfood-curation-restore-from-backup.candidate.yaml" \
  --suite dogfood \
  --filename dogfood-curation-restore-from-backup.yaml \
  --scenario-root "$CANDIDATE_PROMOTE_ROOT" \
  --apply \
  --report "$CANDIDATE_PROMOTE_REPORT" \
  --json > "$CANDIDATE_PROMOTE_STDOUT"
grep -q '"command": "candidate-promote"' "$CANDIDATE_PROMOTE_STDOUT"
grep -q '"applied": true' "$CANDIDATE_PROMOTE_REPORT"
grep -q '"ok": true' "$CANDIDATE_PROMOTE_REPORT"
test -f "$PROMOTED_SCENARIO"
cargo run -p mneme-eval -- validate "$PROMOTED_SCENARIO"
if rg -n 'API_KEY=FAKE_TEST_VALUE|OPENAI_API_KEY|sk-' "$CANDIDATE_PROMOTE_ROOT"; then
  echo "quality-gate: promoted scenario leaked redaction-sensitive text" >&2
  exit 1
fi

cargo run -p mneme-eval -- v1-readiness \
  --report "$V1_READINESS_REPORT" \
  --json > "$V1_READINESS_STDOUT"
grep -q '"command": "v1-readiness"' "$V1_READINESS_STDOUT"
grep -q '"readiness_status": "ready_for_v1_dogfood"' "$V1_READINESS_REPORT"
grep -q '"suite": "dogfood"' "$V1_READINESS_REPORT"
grep -q '"scenario_count": 22' "$V1_READINESS_REPORT"

cargo run -p mneme-eval -- v2-readiness \
  --report "$V2_READINESS_REPORT" \
  --json > "$V2_READINESS_STDOUT"
grep -q '"command": "v2-readiness"' "$V2_READINESS_STDOUT"
grep -q '"readiness_status": "ready_for_team_v2_dogfood"' "$V2_READINESS_REPORT"
grep -q '"suite": "team"' "$V2_READINESS_REPORT"
grep -q '"scenario_count": 10' "$V2_READINESS_REPORT"

MNEME_DOGFOOD_RUN_LABEL="quality-gate" \
MNEME_DOGFOOD_OUT_DIR="$DOGFOOD_OUT_DIR" \
  ./scripts/v1-dogfood.sh
grep -q '"command": "v1-dogfood"' "$DOGFOOD_OUT_DIR/summary.json"
grep -q '"status": "passed"' "$DOGFOOD_OUT_DIR/summary.json"
grep -q '"readiness_status": "ready_for_v1_dogfood"' "$DOGFOOD_OUT_DIR/v1-readiness.json"
grep -q '"ok": true' "$DOGFOOD_OUT_DIR/dogfood.run.mneme-v1.json"
grep -q '"command": "dogfood-summary"' "$DOGFOOD_OUT_DIR/dogfood-summary.json"
grep -q '"decision_status": "ready_for_manual_dogfood"' "$DOGFOOD_OUT_DIR/dogfood-summary.json"
cargo run -p mneme-eval -- dogfood-summary "$DOGFOOD_OUT_DIR" \
  --json > "${DOGFOOD_OUT_DIR}/dogfood-summary-rerun.stdout.json"
grep -q '"decision_status": "ready_for_manual_dogfood"' "${DOGFOOD_OUT_DIR}/dogfood-summary-rerun.stdout.json"

scripts/v2-team-dogfood.py --out-dir "$V2_DOGFOOD_OUT_DIR" --force
grep -q '"command": "v2-team-dogfood"' "$V2_DOGFOOD_OUT_DIR/summary.json"
grep -q '"status": "passed"' "$V2_DOGFOOD_OUT_DIR/summary.json"
grep -q '"command": "v2-team-dogfood-scorecard"' "$V2_DOGFOOD_OUT_DIR/scorecard.json"
grep -q '"seeded_fault_detection_rate": 1.0' "$V2_DOGFOOD_OUT_DIR/scorecard.json"

cargo run -p mneme-eval -- baseline-gate "$BASELINE_REPORT" \
  --report "$BASELINE_GATE_REPORT" \
  --json > "$BASELINE_GATE_STDOUT"
grep -q '"ok": true' "$BASELINE_GATE_STDOUT"
grep -q '"failure-summary.empty"' "$BASELINE_GATE_REPORT"

./scripts/public-safety-check.sh
./scripts/package-check.sh

echo "quality-gate: ok"
