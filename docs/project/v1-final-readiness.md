# V1 Final Readiness

This document defines the public boundary for the Mneme v1.0.0 source release.
It is intentionally stricter than a feature checklist: it states what is ready,
what is only local evidence, and what Mneme must not claim yet.

## Ready Surface

v1.0.0 is ready as a local-first agent-memory source release:

- V1 personal memory with local JSON stores, citations, scope filtering,
  secret-like data blocking, review artifacts, curation, repair, restore, and
  agent begin/end hooks.
- V2 team handoff memory with local users, agents, projects, private/project
  scopes, promotion review, quarantine, firewall, quality, sync checksum,
  ontology projection, run handoff, and validation.
- Local stdio MCP through `mneme-mcp` for Codex, Claude Code, Cursor, and other
  MCP-capable agents.
- Outcome gates that require external verifier or reviewer evidence before a
  gated session is treated as complete.
- Stop-hook loop advice that can block completion, cap retries, avoid
  recursion through `stop_hook_active`, and resume from stored failed criteria.
- Public-safe eval suites, dogfood contracts, seeded faults, package checks,
  install checks, and public safety checks.

## Not Claimed

v1.0.0 does not claim:

- hosted SaaS readiness;
- crates.io or other registry publication;
- broad semantic search quality;
- open-domain natural-language extraction quality;
- causal productivity improvement against real users;
- third-party production validation;
- replacement of project instructions, specs, tests, or human review.

The product validation loop keeps those claims blocked until their evidence
exists. Scripted artifact adoption is a local shape check, not a market claim.

## Required Release Gate

Before tagging v1.0.0, run:

```sh
./scripts/quality-gate.sh full
```

The full gate includes:

- Rust formatting, clippy, unit tests, and rustdoc;
- CLI, MCP, install, package, and distribution checks;
- public safety scans;
- outcome gate smokes, verifier trust checks, and Stop-hook loop smoke;
- MCP eval, MCP hard dogfood contracts, and MCP client protocol smoke;
- V1/V2 dogfood contracts, seeded-fault checks, candidate checks, and
  readiness reports;
- product validation guardrails for privacy/cost, lifecycle, ranking-decision,
  migration, external review schema, held-out claim gating, and scale checks;
- `scripts/v1-final-readiness-check.sh`.

## Public Evidence Boundary

Public docs may summarize local evidence only when the raw data is public-safe.
Raw local stores, client logs, private transcripts, generated eval reports, and
real-session ledgers stay out of git.

README and scorecard language must keep these limits explicit:

- ontology scores are fixture regression only;
- retrieval scores are local regression signals, not semantic-search
  benchmarks;
- scripted outcome adoption is not causal productivity evidence;
- external value claims require public-safe blind review evidence.

## Release Policy

`v0.x` tags are prerelease source snapshots. `v1.x` tags are public source
releases. Registry publication remains disabled because all workspace crates
keep `publish = false`.
