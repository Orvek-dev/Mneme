# Mneme v2

Mneme v2 is the planned team/shared-memory product profile. It is not
implemented yet.

The intended direction is to extend the shared workspace with team-aware
scope, ACL, promotion, audit, and sync behavior instead of forking v1 into a
separate implementation tree.

Expected v2 work includes:

- personal, project, and team memory boundaries;
- ACL and agent permission checks;
- team memory promotion workflow;
- admin audit and offboarding behavior;
- self-hosted sync or server-backed deployment;
- v2-specific eval suites for leakage, ACL bypass, and promotion audit
  coverage.

The repository will add v2 implementation crates only when the v1 eval loop is
stable enough to protect shared behavior from regressions.
