# Model Extraction Adapter

Mneme does not call a model provider directly in `mneme-core`. The public MVP
uses `CommandExtractor`: a provider-neutral adapter that lets a local wrapper
own model SDKs, API keys, retries, and provider-specific prompts.

## CLI Usage

```sh
cargo run -p mneme-cli -- ingest "the user prefers local-first tools" \
  --extractor command \
  --extractor-command ./mneme-extractor-wrapper \
  --store /tmp/mneme.json

cargo run -p mneme-cli -- hook end session-001 \
  --remember "For future planning docs, keep explanations direct and skip motivational language." \
  --extractor command \
  --extractor-command ./mneme-extractor-wrapper \
  --store /tmp/mneme.json
```

The wrapper receives one JSON request on stdin and writes one JSON response to
stdout. The response can contain a claim or `null` when no durable memory should
be stored.

## Request

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

## Response

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

Use `{"schema_version":"mneme.extractor.command.v1","claim":null}` when the
event should be retained without a claim.

## Safety Rules

- Keep provider API keys in environment variables consumed by the wrapper.
- Do not write prompts, API responses, or secrets into tracked repo files.
- Let Mneme own claim IDs, source event citations, lifecycle state, audit
  records, and secret blocking.
- Keep CI on `RuleBasedExtractor` unless a model wrapper is explicitly
  configured for an opt-in eval suite.

## Eval Suite

The `model` suite checks behavior that rule markers cannot cover, while still
using a deterministic fixture command:

```sh
cargo run -p mneme-eval -- validate --suite model
cargo run -p mneme-eval -- run --suite model \
  --target mneme-v1-command \
  --extractor-command evals/fixtures/command-extractor.sh \
  --json
```

Provider-backed wrappers should use the same `mneme-v1-command` target. Keep
provider credentials in the wrapper environment and pass the wrapper program
with `--extractor-command`; extra wrapper arguments can be repeated with
`--extractor-arg <arg>`.

The public suite covers stable preferences, communication style, negative
format preferences, project-scoped preferences, agent session-end extraction,
no-claim events, quoted sample data, third-party attribution, secret blocking,
and explicit correction lifecycle behavior.

## OpenAI Wrapper Example

This repo includes `wrappers/openai_extractor.py` as a public example provider
wrapper. CI runs it in deterministic dry-run mode so the command protocol stays
covered without provider credentials:

```sh
MNEME_OPENAI_DRY_RUN=1 cargo run -p mneme-eval -- acceptance --suite model \
  --target mneme-v1-command \
  --extractor-command wrappers/openai_extractor.py
```

For live local use, set `OPENAI_API_KEY` and optionally `OPENAI_MODEL` in the
environment. See [OpenAI Provider Wrapper](openai-provider-wrapper.md).

For repeated provider-wrapper quality tracking, use
[`mneme-eval baseline`](live-provider-baseline.md) instead of relying on a
single suite run.
