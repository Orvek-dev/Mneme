# Context Ranking Budget MVP Spec

## Scope

This phase makes context retrieval stable as memory volume grows by ranking
eligible claims deterministically and capping the number of returned context
items.

## Authority

- Scope authorization still happens before relevance scoring.
- Retrieval ranking must be deterministic and explainable from the query and
  claim text.
- Context packs should expose enough metadata for evals and agents to inspect
  why an item was selected.
- Relevant items excluded by the context item cap should be reported as omitted
  without leaking extra claim text in the omission record.

## Requirements

- [REQ-RANK-001][Ubiquitous] `ContextQuery` shall carry a maximum returned item
  count with a default of 8.
- [REQ-RANK-002][Ubiquitous] Context items shall include deterministic `score`,
  `matched_terms`, and `match_reason` metadata.
- [REQ-RANK-003][Ubiquitous] Retrieval shall rank by descending score and use a
  stable claim-order tie-break.
- [REQ-RANK-004][Ubiquitous] Retrieval shall omit relevant items beyond the cap
  with `context_budget_exceeded:max_items=<n>`.
- [REQ-RANK-005][Release] `mneme context` and `mneme begin` shall accept
  `--max-items <n>`.
- [REQ-RANK-006][Release] The eval harness shall validate exact item count and
  relative expected ranking order.
- [REQ-RANK-007][Release] The quality gate shall smoke-test ranked,
  budget-capped retrieval.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-RANK-001 | `ContextQuery.max_items` and default constant | verified |
| REQ-RANK-002 | `ContextItem` metadata and CLI JSON smoke | verified |
| REQ-RANK-003 | core unit test and `context-ranking-budget` scenario | verified |
| REQ-RANK-004 | core unit test and eval omission checks | verified |
| REQ-RANK-005 | `mneme-cli` parser and CLI tests | verified |
| REQ-RANK-006 | `ContextPackExpected` checks in `mneme-eval` | verified |
| REQ-RANK-007 | `scripts/quality-gate.sh` | verified |
