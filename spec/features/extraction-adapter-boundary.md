# Extraction Adapter Boundary Spec

## Scope

The extraction adapter boundary lets Mneme v1 swap deterministic rule-based
claim extraction for future model-backed extraction without changing storage,
audit, lifecycle, budget, or eval contracts.

## Authority

- Extractors propose claim candidates only.
- The engine assigns claim IDs and source event citations.
- The engine applies secret-like data blocking after extraction.
- The default behavior remains deterministic through `RuleBasedExtractor`.

## Requirements

- [REQ-EXT-001][Ports-and-adapters] The core shall expose `MnemeExtractor` as
  the extraction port.
- [REQ-EXT-002][Ports-and-adapters] The core shall expose `ExtractedClaim` as
  the adapter output type.
- [REQ-EXT-003][Ports-and-adapters] The core shall expose `RuleBasedExtractor`
  as the default deterministic adapter.
- [REQ-EXT-004][Event-driven] `MnemeEngine::ingest_event` shall keep using the
  default rule-based adapter.
- [REQ-EXT-005][Ports-and-adapters] `MnemeEngine::ingest_event_with_extractor`
  shall accept custom adapters.
- [REQ-EXT-006][Ubiquitous] Custom extractor output shall not bypass secret
  blocking or provenance rules.
- [REQ-EXT-007][Testability] Custom extraction failures shall surface as typed
  errors instead of being silently ignored.

## Verification Map

| Requirement | Evidence target | Status |
| --- | --- | --- |
| REQ-EXT-001 | public `MnemeExtractor` export | verified |
| REQ-EXT-002 | public `ExtractedClaim` export | verified |
| REQ-EXT-003 | public `RuleBasedExtractor` export | verified |
| REQ-EXT-004 | existing core/eval scenarios through `ingest_event` | verified |
| REQ-EXT-005 | custom extractor unit test | verified |
| REQ-EXT-006 | extractor secret-blocking unit test | verified |
| REQ-EXT-007 | `ExtractorError` return path on `MnemeExtractor` | verified |
