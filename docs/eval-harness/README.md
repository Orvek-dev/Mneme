# Eval Harness

The eval harness is the reusable verification surface for Mneme-compatible
memory behavior. It is implemented by `crates/mneme-eval` and driven by public
scenario fixtures under `evals/scenarios/`.

## Core Workflow

- [Eval Scenario Format](eval-scenario-format.md)
- [Eval Acceptance Gate](eval-harness-acceptance.md)
- [Eval Target Adapter Contract](eval-target-adapter-contract.md)

Useful commands:

- `mneme-eval validate --suite mcp`
- `mneme-eval run --suite mcp --target mneme-mcp`

## Provider and Baseline Workflow

- [Model Extraction Adapter](model-extraction-adapter.md)
- [OpenAI Provider Wrapper](openai-provider-wrapper.md)
- [Live Provider Baseline](live-provider-baseline.md)
- [Live Provider Baseline Runbook](live-provider-baseline-runbook.md)

Useful commands:

- `mneme-eval baseline-summary <report.json>`
- `mneme-eval baseline-compare <before.json> <after.json>`

## Dogfood Workflow

- [Eval Candidate Workflow](eval-candidate-workflow.md)
- [V1 Dogfood Readiness](v1-dogfood-readiness.md)
- [V1 Dogfood Execution](v1-dogfood-execution.md)
- [V1 Dogfood Triage](v1-dogfood-triage.md)
- [V1 Manual Dogfood](v1-manual-dogfood.md)
- [V1 Hard Dogfood](v1-hard-dogfood.md)
- [V1 Real-Use Pilot](v1-real-use-pilot.md)
- [V1 Ontology Benchmark](v1-ontology-benchmark.md)
- [MCP Hard Dogfood](mcp-hard-dogfood.md)
- [Product Validation Loop](product-validation-loop.md)

Useful commands:

- `mneme-eval candidate <report.json>`
- `mneme-eval candidate-check <candidate.yaml|dir>`
- `mneme-eval candidate-promote <candidate.yaml>`
- `mneme-eval v1-readiness`
- `mneme-eval dogfood-summary <bundle-dir>`
- `scripts/v1-dogfood.sh`
- `scripts/v1-manual-dogfood.py`
- `scripts/v1-hard-dogfood.py`
- `scripts/v1-real-use-pilot.py`
- `scripts/v1-ontology-benchmark.py`
- `scripts/mcp-hard-dogfood.py`
- `scripts/mcp-client-continuity-smoke.py`
- `scripts/product-validation-loop.py`

## MCP Workflow

The MCP suite verifies the local `mneme-mcp` stdio server through the eval
harness. It covers initialization, tool listing, V1 personal-memory writes and
retrieval, V1 session restart persistence, V2 team handoff, sync checksum,
firewall access, citation coverage, and private-scope denial.

Useful commands:

- `mneme-mcp --self-test`
- `mneme mcp config --client all`
- `mneme-eval validate --suite mcp`
- `mneme-eval run --suite mcp --target mneme-mcp --json`
- `scripts/mcp-hard-dogfood.py --check-seeded-faults`
- `scripts/mcp-hard-dogfood.py --out-dir /tmp/mneme-mcp-hard-dogfood --force`
