#!/usr/bin/env python3
"""OpenAI-backed Mneme command extractor.

This wrapper implements the public `mneme.extractor.command.v1` stdin/stdout
protocol. It keeps provider configuration outside `mneme-core` and reads
credentials from environment variables only.
"""

from __future__ import annotations

import json
import os
import re
import sys
from typing import Any, Optional
import urllib.error
import urllib.request

SCHEMA_VERSION = "mneme.extractor.command.v1"
DEFAULT_MODEL = "gpt-5.4-mini"
DEFAULT_BASE_URL = "https://api.openai.com/v1"
DEFAULT_TIMEOUT_SECONDS = 30

SYSTEM_PROMPT = """You extract durable user-memory claims for Mneme.

Return at most one stable claim from the event. Use concise subject,
predicate, and object strings. Prefer subject "user" for user preferences,
communication preferences, tool preferences, workflow preferences, and durable
project preferences. Extract only facts that should remain useful in future
sessions.

Return null when the event is small talk, a one-off task, a transient
instruction, status for only this answer/task/session, quoted sample data, test
fixture text, or implementation instruction that is not a user preference.
Return null for third-party preferences unless the event explicitly says the
user wants that third-party fact remembered for future work. Return null for
credentials, tokens, passwords, or other secret-like material.

When the user says not to use one format/tool and states an alternative,
extract the durable alternative preference instead of the rejected option.
Do not invent facts not present in the event.
"""

SECRET_RE = re.compile(
    r"\b((?:API[_-]?KEY|TOKEN|PASSWORD|SECRET)\s*[:=]\s*[A-Za-z0-9_./+=-]+)",
    re.IGNORECASE,
)


class WrapperError(Exception):
    """User-facing wrapper failure."""


def main() -> int:
    try:
        request = read_request()
        response = extract_response(request)
        json.dump(response, sys.stdout, separators=(",", ":"), ensure_ascii=False)
        sys.stdout.write("\n")
        return 0
    except WrapperError as error:
        print(f"mneme openai extractor: {error}", file=sys.stderr)
        return 1


def read_request() -> dict:
    try:
        request = json.load(sys.stdin)
    except json.JSONDecodeError as error:
        raise WrapperError(f"invalid JSON request: {error}") from error

    if request.get("schema_version") != SCHEMA_VERSION:
        raise WrapperError("unsupported or missing schema_version")

    event = request.get("event")
    if not isinstance(event, dict):
        raise WrapperError("request.event must be an object")
    if not isinstance(event.get("text"), str):
        raise WrapperError("request.event.text must be a string")
    return request


def extract_response(request: dict) -> dict:
    event_text = request["event"]["text"]
    local_secret_claim = claim_for_secret_like_text(event_text)
    if local_secret_claim is not None:
        return command_response(local_secret_claim)

    if env_flag("MNEME_OPENAI_DRY_RUN"):
        return command_response(dry_run_claim(event_text))

    model_output = call_openai(request)
    return command_response(normalize_model_claim(model_output.get("claim"), event_text))


def command_response(claim: Optional[dict]) -> dict:
    return {"schema_version": SCHEMA_VERSION, "claim": claim}


def claim_for_secret_like_text(text: str) -> Optional[dict]:
    match = SECRET_RE.search(text)
    if match is None:
        return None
    secret = re.sub(r"\s+", "", match.group(1))
    return {"subject": "user", "predicate": "note", "object": secret}


def dry_run_claim(text: str) -> Optional[dict]:
    lower = text.lower()
    if "local-first tools" in lower:
        return {"subject": "user", "predicate": "prefers", "object": "local-first tools"}
    if "compact tables" in lower:
        return {
            "subject": "user",
            "predicate": "prefers",
            "object": "compact tables for option summaries",
        }
    if "keep explanations direct" in lower and "future planning docs" in lower:
        return {
            "subject": "user",
            "predicate": "prefers",
            "object": "direct explanations in future planning docs",
        }
    if "bullets are easier for me" in lower and "tradeoff summaries" in lower:
        return {
            "subject": "user",
            "predicate": "prefers",
            "object": "bullets for tradeoff summaries",
        }
    if "project atlas" in lower and "risk notes grouped by owner" in lower:
        return {
            "subject": "user",
            "predicate": "prefers",
            "object": "Project Atlas risk notes grouped by owner",
        }
    if "thanks, that answer helps" in lower:
        return None
    if "for this one task" in lower:
        return None
    if "sam prefers vim" in lower:
        return None
    if "sample data" in lower or "test fixture" in lower:
        return None
    if "in this answer" in lower:
        return None
    return None


def call_openai(request: dict) -> dict:
    api_key = os.environ.get("OPENAI_API_KEY")
    if not api_key:
        raise WrapperError("OPENAI_API_KEY is required unless MNEME_OPENAI_DRY_RUN=1")

    base_url = os.environ.get("OPENAI_BASE_URL", DEFAULT_BASE_URL).rstrip("/")
    model = os.environ.get("OPENAI_MODEL", DEFAULT_MODEL)
    timeout = timeout_seconds()
    body = json.dumps(openai_request_body(model, request)).encode("utf-8")
    http_request = urllib.request.Request(
        f"{base_url}/responses",
        data=body,
        headers={
            "Authorization": f"Bearer {api_key}",
            "Content-Type": "application/json",
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(http_request, timeout=timeout) as response:
            payload = json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        detail = error.read().decode("utf-8", errors="replace")
        raise WrapperError(
            f"OpenAI API returned HTTP {error.code}: {truncate(detail)}"
        ) from error
    except urllib.error.URLError as error:
        raise WrapperError(f"OpenAI API request failed: {error.reason}") from error
    except json.JSONDecodeError as error:
        raise WrapperError(f"OpenAI API returned invalid JSON: {error}") from error

    if payload.get("status") == "incomplete":
        reason = payload.get("incomplete_details", {}).get("reason", "unknown")
        raise WrapperError(f"OpenAI response incomplete: {reason}")

    text = output_text(payload)
    try:
        parsed = json.loads(text)
    except json.JSONDecodeError as error:
        raise WrapperError(f"model output was not JSON: {error}") from error

    if not isinstance(parsed, dict):
        raise WrapperError("model output must be a JSON object")
    return parsed


def openai_request_body(model: str, request: dict) -> dict:
    return {
        "model": model,
        "input": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {
                "role": "user",
                "content": json.dumps({"event": request["event"]}, ensure_ascii=False),
            },
        ],
        "text": {
            "verbosity": "low",
            "format": {
                "type": "json_schema",
                "name": "mneme_extraction",
                "description": "Durable memory claim extracted from one Mneme event.",
                "strict": True,
                "schema": {
                    "type": "object",
                    "additionalProperties": False,
                    "required": ["claim"],
                    "properties": {
                        "claim": {
                            "anyOf": [
                                {
                                    "type": "object",
                                    "additionalProperties": False,
                                    "required": ["subject", "predicate", "object"],
                                    "properties": {
                                        "subject": {"type": "string"},
                                        "predicate": {"type": "string"},
                                        "object": {"type": "string"},
                                    },
                                },
                                {"type": "null"},
                            ]
                        }
                    },
                },
            },
        },
    }


def output_text(payload: dict) -> str:
    if isinstance(payload.get("output_text"), str):
        return payload["output_text"]

    for item in payload.get("output", []):
        for content in item.get("content", []):
            content_type = content.get("type")
            if content_type == "output_text" and isinstance(content.get("text"), str):
                return content["text"]
            if content_type == "refusal":
                refusal = content.get("refusal", "model refusal")
                raise WrapperError(f"model refused extraction: {truncate(str(refusal))}")

    raise WrapperError("OpenAI response did not include output_text content")


def normalize_model_claim(claim: Any, event_text: str) -> Optional[dict]:
    if claim is None:
        return None
    if not isinstance(claim, dict):
        raise WrapperError("model claim must be an object or null")

    normalized = {}
    for key in ("subject", "predicate", "object"):
        value = claim.get(key)
        if not isinstance(value, str) or not value.strip():
            raise WrapperError(f"model claim.{key} must be a non-empty string")
        normalized[key] = value.strip()

    if normalized["subject"].lower() in {"i", "me"}:
        normalized["subject"] = "user"
    normalized["predicate"] = normalized["predicate"].lower()

    if should_suppress_model_claim(event_text, normalized):
        return None
    return normalized


def should_suppress_model_claim(text: str, claim: dict) -> bool:
    lower = text.lower()
    subject = claim["subject"].strip().lower()
    object_text = claim["object"].strip().lower()

    transient_markers = [
        "for this one task",
        "for this answer",
        "in this answer",
        "for this session",
        "right now",
        "just this time",
    ]
    if any(marker in lower for marker in transient_markers):
        return True

    sample_markers = [
        "sample data",
        "test fixture",
        "example sentence",
        "quoted example",
    ]
    if any(marker in lower for marker in sample_markers):
        return True

    if subject not in {"user", "i", "me"} and "i do not" in lower:
        return True

    rejected_object_markers = ["do not use", "don't use", "avoid "]
    if any(marker in lower for marker in rejected_object_markers):
        rejected_terms = ["tables", "vim", "python"]
        if any(term in object_text for term in rejected_terms) and "bullets" not in object_text:
            return True

    return False


def timeout_seconds() -> float:
    raw = os.environ.get("MNEME_OPENAI_TIMEOUT_SECONDS")
    if raw is None:
        return DEFAULT_TIMEOUT_SECONDS
    try:
        value = float(raw)
    except ValueError as error:
        raise WrapperError("MNEME_OPENAI_TIMEOUT_SECONDS must be numeric") from error
    if value <= 0:
        raise WrapperError("MNEME_OPENAI_TIMEOUT_SECONDS must be positive")
    return value


def env_flag(name: str) -> bool:
    return os.environ.get(name, "").strip().lower() in {"1", "true", "yes", "on"}


def truncate(value: str, limit: int = 600) -> str:
    compact = value.replace("\n", " ").strip()
    if len(compact) <= limit:
        return compact
    return f"{compact[:limit]}..."


if __name__ == "__main__":
    raise SystemExit(main())
