# Mneme v2 Evaluation

v2 is evaluated as a team-agent memory boundary, not as a chatbot benchmark.
The question is:

```text
Can several agents share useful memory without leaking private, unsafe, or
unreviewed memory?
```

## Public Team Suite

Run:

```sh
cargo run -p mneme-eval -- validate --suite team
cargo run -p mneme-eval -- run --suite team --target mneme-v2
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
```

The team suite covers:

- private boundary checks;
- project member access;
- promotion review;
- unapproved promotion denial;
- revoked agent denial;
- secret blocking;
- memory firewall quarantine;
- sync privacy envelopes;
- handoff ontology projection;
- task-run handoff, quality reports, and sync checksum verification.

## Seeded Faults

The acceptance gate intentionally mutates target behavior and expects the suite
to fail. v2 currently checks these faults:

- `bypass-acl`;
- `leak-secrets`;
- `drop-citations`;
- `unapproved-promotion`;
- `ignore-revocation`;
- `leak-quarantined`.

Run:

```sh
cargo run -p mneme-eval -- acceptance --suite team --target mneme-v2
```

The gate is meaningful only if every seeded fault is detected.

## Dogfood Dataset Contract

The v2 dogfood script keeps a public-safe synthetic pressure test:

```sh
scripts/v2-team-dogfood.py --check-contract
scripts/v2-team-dogfood.py --check-dataset
scripts/v2-team-dogfood.py --check-seeded-faults
scripts/v2-team-dogfood.py
```

Current public-safe scale:

| Surface | Count |
| --- | ---: |
| Team records | 120 |
| Adversarial records | 80 |
| Handoff workflows | 25 |
| Team scenarios | 10 |
| Seeded v2 faults | 6 |

## Scorecard Signals

The v2 scorecard should be read as a boundary and workflow signal:

| Signal | Meaning |
| --- | --- |
| Team suite pass rate | Expected behavior still works |
| ACL leak count | Private/project memory did not cross actor scope |
| Secret leak count | Secret-like records did not enter context |
| Quarantine leak count | Instruction-override records stayed out |
| Promotion audit coverage | Team memory changes are reviewable |
| Revocation denial count | Revoked identities lose access |
| Run handoff coverage | Task runs can become handoff packages |
| Sync checksum coverage | Export/import detects payload integrity |
| Quality conflict detection | Handoff can surface memory disagreement |
| Seeded fault detection rate | The eval suite catches known bad behavior |

## What the Evaluation Proves

The current gates prove that the public v2 core can:

- enforce local scope policy before ranking;
- keep private and quarantined memory out of handoff and sync;
- preserve citations for context items;
- require review for team promotion;
- reject revoked users and agents;
- create run-anchored handoff packages;
- detect checksum mismatches on sync import;
- surface duplicate and conflicting memory before handoff.

## What It Does Not Prove Yet

The current gates do not prove:

- hosted multi-tenant auth correctness;
- production cloud sync conflict handling;
- vector-search quality at large scale;
- provider-specific extraction quality for live model calls;
- UI usability.

Those are future product layers. The public v2 claim stays focused on the
local team-agent memory boundary and the artifacts that make it testable.
