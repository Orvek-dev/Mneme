# Eval Harness

The eval harness is the reusable verification surface for Mneme-compatible
memory behavior. It is implemented by `crates/mneme-eval` and driven by public
scenario fixtures under `evals/scenarios/`.

## Core Workflow

- [Eval Scenario Format](eval-scenario-format.md)
- [Eval Acceptance Gate](eval-harness-acceptance.md)
- [Eval Target Adapter Contract](eval-target-adapter-contract.md)

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

Useful commands:

- `mneme-eval candidate <report.json>`
- `mneme-eval candidate-check <candidate.yaml|dir>`
- `mneme-eval candidate-promote <candidate.yaml>`
- `mneme-eval v1-readiness`
