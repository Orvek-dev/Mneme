# Agent Runtime Config

`scripts/mneme-agent-hook.sh` can read a local runtime profile before running
`doctor`, `begin`, or `end`. This keeps agent installation settings out of
every command invocation.

## Profile Path

The wrapper checks profile paths in this order:

1. `MNEME_AGENT_HOOK_CONFIG`
2. `MNEME_CONFIG`
3. `.mneme/mneme-agent-hook.env`

The default `.mneme/` path is ignored by git.

## Format

Profiles use simple `KEY=VALUE` lines. Blank lines and `#` comments are
ignored. The wrapper parses these lines directly and does not execute the file.

Start from the public example:

```sh
mkdir -p .mneme
cp examples/mneme-agent-hook.env.example .mneme/mneme-agent-hook.env
```

Supported keys:

- `MNEME_BIN`
- `MNEME_STORE`
- `MNEME_AGENT_ID`
- `MNEME_SCOPE`
- `MNEME_MAX_ITEMS`

## Precedence

Runtime values resolve in this order:

1. Explicit CLI flags passed to `scripts/mneme-agent-hook.sh`
2. Environment variables
3. Runtime profile values
4. Command defaults

Example:

```sh
MNEME_AGENT_HOOK_CONFIG=.mneme/mneme-agent-hook.env \
  scripts/mneme-agent-hook.sh begin "Draft setup plan" --query "local-first"
```

Run an installation smoke test:

```sh
scripts/mneme-agent-hook.sh doctor
```

`doctor` uses an isolated temporary store for its smoke test. It reports whether
a profile was loaded without writing to the configured project store.
