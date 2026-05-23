# Extraction Adapter Contract

Mneme v1 extracts memory through the `MnemeExtractor` trait in `mneme-core`.
The default adapter is `RuleBasedExtractor`, which preserves deterministic
`remember:` and `기억해줘:` behavior.

## Contract

An extractor receives an appended `EventRecord` and may return one
`ExtractedClaim`.

The engine, not the extractor, owns:

- stable claim IDs;
- source event citations;
- claim lifecycle state;
- secret-like data blocking;
- budget gates before extraction;
- audit records after extraction.

This keeps future model-backed adapters replaceable without letting them bypass
core safety and provenance rules.

## Default Adapter

`RuleBasedExtractor` parses explicit markers:

```text
remember: user prefers local-first tools
기억해줘: user prefers local-first tools
```

The first two tokens become subject and predicate. The remaining text becomes
the object. If that shape is not present, the extractor falls back to:

```text
<speaker_id> note <marker text>
```

## Integration

Product code can keep using:

```rust
engine.ingest_event(input);
```

This uses `RuleBasedExtractor`.

Adapter experiments can call:

```rust
engine.ingest_event_with_extractor(input, &extractor);
```

The resulting claim still passes through the same engine-owned ID, provenance,
secret-blocking, audit, and persistence paths.
