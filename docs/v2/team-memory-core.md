# Mneme v2 Team Memory Core

The v2 core is designed around a small control plane:

| Surface | Purpose |
| --- | --- |
| Users | Assign `admin`, `maintainer`, or `member` roles. |
| Agents | Bind an agent to the user it acts for. |
| Projects | Define project-scoped memory membership. |
| Memory | Store active, blocked-secret, or quarantined team-aware records. |
| Run | Anchor task sessions, notes, summaries, next steps, and handoff. |
| Promotion | Move private/project memory into team memory only after review. |
| Sync | Move only connector-safe records through a public envelope. |
| Handoff | Package allowed context for the next agent. |
| Firewall | Flag active memory that should have been blocked or quarantined. |
| Quality | Flag duplicates, conflicts, pending review, promoted-source cleanup, and run state. |
| Ontology | Project state into entities, relations, and attributes. |
| Audit | Preserve policy decisions for inspection and regression tests. |

## Scope Rules

| Scope | Read/write rule |
| --- | --- |
| `team` | Any active actor can read. Only admin/maintainer can directly write. |
| `private:<user>` | Only that user can read/write. |
| `project:<project>` | Only active project members can read/write. |
| `agent-private:<agent>` | Only that active agent can read/write. |

Secret-like text is stored as `blocked_secret` for audit but omitted from
context packs.

Memory-poisoning-like text, such as instruction override attempts, is stored as
`quarantined` and is also omitted from context packs and sync envelopes.

## Promotion Rule

Members can propose promotion for a memory they are allowed to read. The memory
does not become team-visible until an admin or maintainer approves it.

Approval creates a new `team` memory that keeps:

- source event IDs;
- source memory IDs;
- reviewer attribution;
- promotion audit records.

`mneme team promotion report <promotion-id>` gives reviewers a small risk report
before approval. It flags missing source memory, unsafe source text, already
team-visible memory, and duplicate team memory.

## Task Runs

Runs make v2 useful for actual team/agent work rather than only loose memory
records.

```sh
mneme team run begin "Atlas deploy handoff" --actor bob --agent codex-bob \
  --query "rollback notes" --scope project:atlas --json
mneme team run note team-run-001 "remember: Atlas deploy uses smoke test" \
  --actor bob --agent codex-bob --scope project:atlas --json
mneme team run end team-run-001 --actor bob --agent codex-bob \
  --summary "Deploy checklist reviewed" --next "Run smoke test" --json
mneme team run handoff team-run-001 --actor bob --agent codex-bob --json
```

The run handoff package includes the run record, actor-scoped context,
connector-safe sync envelope, firewall report, quality report, and ontology
projection.

## Quality Rule

`mneme team quality` is the local release/handoff guard for team memory. It
reports:

- duplicate active memory by normalized text and scope;
- conflicting active memory using deterministic polarity heuristics;
- source memories that have already produced team memory;
- pending promotions awaiting review;
- open and closed run counts.

## Evaluation Contract

The committed v2 suite checks:

- private memory does not leak to a peer;
- project members can recall project memory;
- approved promotion becomes team context;
- pending promotion does not become team context;
- secret-like text is blocked from context;
- revoked agents cannot retrieve context.
- connector sync excludes private/quarantined memory and scans full JSON output
  for privacy leaks;
- handoff packages include only cited, policy-allowed memory;
- run lifecycle opens context, records notes, closes with next steps, and
  builds run-anchored handoff;
- quality checks catch duplicate/conflicting active memory;
- sync checksums are verified during dry-run/apply inspection;
- ontology projection exposes actor-readable team entities, relations, and
  attributes.

`mneme-eval v2-readiness` also verifies seeded faults are detected:

| Fault | Expected detection |
| --- | --- |
| `bypass-acl` | scope leak and forbidden context inclusion |
| `leak-secrets` | secret leak and forbidden context inclusion |
| `drop-citations` | missing source citations |
| `unapproved-promotion` | pending memory promoted without review |
| `ignore-revocation` | revoked actor receives context |
| `leak-quarantined` | quarantined memory appears in context |

## Connector Boundary

`mneme team sync export` writes a `mneme.team_sync.v1` envelope. It includes
active team memory and actor-readable project memory, sanitized supporting
events, and only the minimal user, agent, project, and promotion metadata needed
to resolve exported records. Raw audit trails are not exported. Each envelope
also carries an ID and checksum so imports can expose a deterministic dry-run
diff and reject tampered payloads.

The envelope deliberately excludes:

- `private:<user>` memory;
- `agent-private:<agent>` memory;
- `blocked_secret` memory;
- `quarantined` memory.

`mneme team sync import` defaults to dry-run. `--apply` requires
`--actor <admin-or-maintainer>` and mutates the local store only when the
envelope schema, workspace, metadata conflicts, firewall checks, and state
validation all pass.

## Agent Handoff

`mneme team handoff` returns a `mneme.team_handoff.v1` package with:

- the actor-scoped context pack;
- a connector-safe sync envelope for downstream tools;
- the current firewall report;
- the current quality report;
- the actor-scoped ontology projection.

This is the intended boundary for coding agents that hand work from one agent
or developer to another.

## Adapter Surface

`mneme team adapter manifest --json` exposes stable tool names for integrations.
`scripts/mneme-mcp-stdio.py` is a thin stdio bridge over the same CLI surface for
MCP-style runtimes. It does not implement separate policy logic.
