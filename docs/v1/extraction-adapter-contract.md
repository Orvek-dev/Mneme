# Extraction Adapter Contract

Mneme v1 extracts memory through the `MnemeExtractor` trait in `mneme-core`.
The default adapter is `RuleBasedExtractor`, which preserves deterministic
`remember:` and `기억해줘:` behavior.

Model-backed experiments should start with `CommandExtractor`. It delegates
extraction to a local command over a strict JSON stdin/stdout protocol, so API
keys and provider SDKs stay outside the public core crate.

## Contract

An extractor receives an appended `EventRecord` and returns either:

- `Ok(Some(ExtractedClaim))` when one claim should be persisted;
- `Ok(None)` when the event should append without a claim;
- `Err(ExtractorError)` when extraction failed and the caller should surface
  the failure.

The engine, not the extractor, owns:

- stable claim IDs;
- source event citations;
- claim lifecycle state;
- secret-like data blocking;
- budget gates before extraction;
- audit records after extraction.

This keeps future model-backed adapters replaceable without letting them bypass
core safety and provenance rules.

Extractor adapters should still be conservative. Events that are small talk,
one-off task instructions, answer-local instructions, quoted sample data, test
fixtures, or third-party preferences should normally return `Ok(None)`.

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
engine.ingest_event(input)?;
```

This uses `RuleBasedExtractor`.

Adapter experiments can call:

```rust
engine.ingest_event_with_extractor(input, &extractor)?;
```

Agent integrations can also close a session with a custom extractor:

```rust
engine.end_session_with_extractor(input, &extractor, SessionMemoryInputMode::RawEvent)?;
```

The resulting claim still passes through the same engine-owned ID, provenance,
secret-blocking, audit, and persistence paths.

## Command Protocol

`CommandExtractor` starts the configured program without a shell, writes one
JSON request to stdin, and expects one JSON response on stdout.

Request:

```json
{
  "schema_version": "mneme.extractor.command.v1",
  "event": {
    "id": "event-001",
    "speaker_id": "user",
    "actor_agent_id": "codex",
    "text": "the user prefers local-first tools",
    "scope": "private",
    "trust_level": "trusted_user"
  }
}
```

Response with a claim:

```json
{
  "schema_version": "mneme.extractor.command.v1",
  "claim": {
    "subject": "user",
    "predicate": "prefers",
    "object": "local-first tools"
  }
}
```

Response without a claim:

```json
{
  "schema_version": "mneme.extractor.command.v1",
  "claim": null
}
```

The command response must use the same schema version and non-empty claim
fields. Shell parsing is intentionally outside this adapter; pass a program and
arguments explicitly from the CLI or wrapper code.
