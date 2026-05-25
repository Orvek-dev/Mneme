# CI Cost Policy

Mneme uses phase-sized work. The repository should avoid running the full gate
twice for every branch.

## Current Policy

- Full CI runs on pull requests targeting `main`.
- Full CI runs on pushes to `main`.
- Full CI can be started manually with `workflow_dispatch`.
- Feature branch pushes do not run full CI.
- Superseded CI runs for the same PR are cancelled through workflow
  concurrency.
- Release tags run the release workflow, which reuses `scripts/quality-gate.sh`.

## Local Responsibility

Before opening a PR, run:

```sh
./scripts/quality-gate.sh full
```

This keeps GitHub Actions usage low without weakening release confidence.

## Auto Merge

The auto-merge workflow listens for successful pull-request CI runs. It only
acts when `AUTOMERGE_TOKEN` is configured with pull-request and contents write
permission. It does not create PRs from branch pushes. Without that secret, it
exits without changing the PR.
