# Mneme v2 Team Memory Core

The v2 core is designed around a small control plane:

| Surface | Purpose |
| --- | --- |
| Users | Assign `admin`, `maintainer`, or `member` roles. |
| Agents | Bind an agent to the user it acts for. |
| Projects | Define project-scoped memory membership. |
| Memory | Store active, blocked-secret, or quarantined team-aware records. |
| Promotion | Move private/project memory into team memory only after review. |
| Sync | Move only connector-safe records through a public envelope. |
| Handoff | Package allowed context for the next agent. |
| Firewall | Flag active memory that should have been blocked or quarantined. |
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
to resolve exported records. Raw audit trails are not exported.

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
- the actor-scoped ontology projection.

This is the intended boundary for coding agents that hand work from one agent
or developer to another.

## Adapter Surface

`mneme team adapter manifest --json` exposes stable tool names for integrations.
`scripts/mneme-mcp-stdio.py` is a thin stdio bridge over the same CLI surface for
MCP-style runtimes. It does not implement separate policy logic.
