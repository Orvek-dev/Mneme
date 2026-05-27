# MCP Hard Dogfood

`scripts/mcp-hard-dogfood.py` verifies Mneme through the public `mneme-mcp`
stdio boundary. It exists because MCP integration should prove more than server
startup and tool discovery: the same memory, safety, handoff, and ontology
pressure used for V1 and V2 must still pass when an agent client calls tools
over JSON-RPC.

## What It Runs

| Surface | Dataset or check | Expected signal |
| --- | --- | --- |
| V1 hard corpus | 100 normal records, 150 adversarial records, 30 handoff workflows | Recall, precision, citation coverage, handoff success, zero leaks |
| V1 ontology | 13 natural-language ontology cases | Entity/relation/attribute F1 and context safety |
| V2 team corpus | 120 team records, 80 adversarial records, 25 handoff workflows | Handoff success, citations, secret blocking, quarantine, sync checksum |
| MCP scenario suite | `evals/scenarios/mcp` against `mneme-mcp` | MCP protocol and V1/V2 tool readiness |
| Team scenario suite | `evals/scenarios/team` against `mneme-mcp` | V2 ACL, promotion, revoke, sync, firewall, ontology, quality over MCP |
| Seeded faults | 3 V1 faults and 6 V2 faults | Faults must be detected, not pass silently |

## Commands

Fast contract checks:

```sh
scripts/mcp-hard-dogfood.py --check-contract
scripts/mcp-hard-dogfood.py --check-dataset
scripts/mcp-hard-dogfood.py --check-seeded-faults
```

Full local run:

```sh
scripts/mcp-hard-dogfood.py --out-dir /tmp/mneme-mcp-hard-dogfood --force
```

The full run writes:

- `summary.json`
- `scorecard.json`
- `dataset.json`
- `v1-mcp-hard.json`
- `v1-mcp-ontology.json`
- `v2-mcp-hard.json`
- `suite-results.json`
- `seeded-faults.json`
- `equivalence.json`
- `report.md`

## Passing Criteria

The run is considered passed only when:

- V1 hard corpus has `1.0` recall, precision, citation coverage, and handoff
  success with zero scope or secret leaks;
- V1 ontology has at least `0.8` entity, relation, and attribute F1 with zero
  leaks;
- V2 team MCP corpus has at least `0.95` handoff success, full citation
  coverage, blocked-secret evidence, quarantined-memory evidence, sync checksum
  verification, and zero leaks;
- the MCP and team suites pass through the `mneme-mcp` eval target;
- all 9 seeded faults are detected.

The generated bundle is synthetic and public-safe. It should stay out of git
unless a specific reduced artifact is intentionally promoted into an eval
scenario.
