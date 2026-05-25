# Mneme v2 Team Memory Core

The v2 core is designed around a small control plane:

| Surface | Purpose |
| --- | --- |
| Users | Assign `admin`, `maintainer`, or `member` roles. |
| Agents | Bind an agent to the user it acts for. |
| Projects | Define project-scoped memory membership. |
| Memory | Store active or blocked-secret team-aware records. |
| Promotion | Move private/project memory into team memory only after review. |
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

`mneme-eval v2-readiness` also verifies seeded faults are detected:

| Fault | Expected detection |
| --- | --- |
| `bypass-acl` | scope leak and forbidden context inclusion |
| `leak-secrets` | secret leak and forbidden context inclusion |
| `drop-citations` | missing source citations |
| `unapproved-promotion` | pending memory promoted without review |
| `ignore-revocation` | revoked actor receives context |
