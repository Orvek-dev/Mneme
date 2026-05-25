# Mneme v2 Security Model

Mneme v2 is a local team-memory policy core. It does not try to be a hosted
identity provider or a production sync server. Its job is narrower: keep team
agent memory from crossing boundaries unless the local policy allows it.

## Assets

The protected assets are:

- private user memory;
- agent-private memory;
- project memory outside a user's membership;
- blocked secret-like memory;
- quarantined instruction-override memory;
- audit history;
- sync envelopes and handoff packages.

## Boundary Rules

Mneme applies policy before relevance ranking:

1. inactive users and agents cannot act;
2. private memory is readable only by its owner;
3. agent-private memory is readable only by that agent;
4. project memory is readable only by project members;
5. blocked secret-like memory cannot enter active context;
6. quarantined memory cannot enter context or sync;
7. promotion to team memory requires review;
8. sync import verifies checksum before apply.

## Public Handoff Contract

A v2 handoff package can be shared with another agent only if it is built
through:

```sh
mneme team handoff ...
mneme team run handoff ...
```

The package must keep these surfaces separate:

- `context_pack.items`: readable memory with citations;
- `context_pack.omitted`: redacted denied records and reasons;
- `sync_envelope`: connector-safe export payload;
- `firewall`: quarantine and high-risk findings;
- `quality`: duplicate, conflict, promotion, and run-state findings;
- `ontology`: actor-scoped graph projection.

## Threat Cases Covered by the Public Eval Suite

The current v2 suite checks:

- ACL bypass;
- private scope leakage;
- project membership boundary;
- secret leakage;
- unapproved promotion;
- revoked agent access;
- quarantined memory leakage;
- handoff citation coverage;
- sync privacy envelope behavior;
- sync checksum verification;
- task-run handoff state;
- duplicate and conflict detection.

## What v2 Does Not Claim

v2 is not yet:

- a hosted auth system;
- a cloud sync service;
- a web dashboard;
- a multi-device conflict resolver;
- a cryptographic zero-trust storage layer.

Those can be built around the local core later. The current claim is stronger
because it is narrower: local team-agent memory boundaries are explicit,
auditable, and regression-tested.

## Release Gate

Before treating a change as v2-ready, run:

```sh
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
cargo run -p mneme-eval -- v2-readiness --json --report /tmp/v2-readiness.json
scripts/v2-team-dogfood.py
examples/v2-team-agent-ops/run-demo.sh --out-dir /tmp/mneme-v2-demo
```

For repository releases, run the full gate:

```sh
scripts/quality-gate.sh full
```
