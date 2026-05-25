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
mneme init
mneme init --extractor-command ./mneme-extractor-wrapper
mneme doctor
```

For manual profiles, `examples/mneme-agent-hook.env.example` remains a
copyable template.

Supported keys:

- `MNEME_BIN`
- `MNEME_STORE`
- `MNEME_AGENT_ID`
- `MNEME_SCOPE`
- `MNEME_MAX_ITEMS`
- `MNEME_EXTRACTOR_COMMAND`

`MNEME_EXTRACTOR_COMMAND` is optional. When set, wrapper `end` calls use
`--extractor command` for `--remember` notes unless an explicit `--extractor`
flag is already present.
`mneme init --extractor-command <program>` writes this key as an active profile
line; without that option, the generated profile keeps it as a commented
example.

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
mneme doctor --json
scripts/mneme-agent-hook.sh doctor
```

`mneme doctor` inspects the configured store/profile pair without mutating
files. `scripts/mneme-agent-hook.sh doctor` uses an isolated temporary store
for its smoke test and reports whether a profile was loaded without writing to
the configured project store.

Wrapper doctor output also reports the selected `mneme` source, configured
store, agent, scope, max item cap, and extractor command. It does not run the
configured command extractor by default, even when `MNEME_EXTRACTOR_COMMAND` is
set. This keeps routine diagnostics no-cost for provider-backed wrappers.

Run an extractor smoke only when you explicitly want to execute the configured
command:

```sh
MNEME_AGENT_HOOK_CONFIG=.mneme/mneme-agent-hook.env \
  scripts/mneme-agent-hook.sh doctor --check-extractor
```

Use `MNEME_OPENAI_DRY_RUN=1` or a fixture command when checking provider-backed
wrappers without spending live API budget.
