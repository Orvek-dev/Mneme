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
- `MNEME_VERIFIER_COMMAND`
- `MNEME_VERIFIER_POLICY`
- `MNEME_VERIFIER_MANIFEST`
- `MNEME_LOOP_MAX_ATTEMPTS`
- `MNEME_LOOP_STATE`
- `MNEME_LOOP_SESSION_ID`

`MNEME_EXTRACTOR_COMMAND` is optional. When set, wrapper `end` calls use
`--extractor command` for `--remember` notes unless an explicit `--extractor`
flag is already present.
`mneme init --extractor-command <program>` writes this key as an active profile
line; without that option, the generated profile keeps it as a commented
example.

`MNEME_VERIFIER_COMMAND` is optional. When set, wrapper `end` calls add
`--verifier-command` unless an explicit verifier report or verifier command is
already present. Use it only for sessions started with `--acceptance`; ungated
sessions do not need a verifier.

`MNEME_VERIFIER_POLICY` and `MNEME_VERIFIER_MANIFEST` are optional verifier
trust settings. They are passed through the environment to `mneme end` and
`mneme hook end`, so strict verifier pinning can be configured once in the
profile.

`MNEME_LOOP_MAX_ATTEMPTS`, `MNEME_LOOP_STATE`, and `MNEME_LOOP_SESSION_ID`
control Stop-hook loop behavior. The wrapper stores only the last
`last_gate_failure_id` and retry count in `MNEME_LOOP_STATE`; it does not store
transcripts. `MNEME_LOOP_SESSION_ID` is optional and is only needed when a
client cannot infer the latest incomplete gated session from the store.

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
store, agent, scope, max item cap, extractor command, and verifier command. It does not run the
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

Run a local Stop-hook smoke when you want to verify the automatic failed-gate
loop without touching real client settings:

```sh
MNEME_AGENT_HOOK_CONFIG=.mneme/mneme-agent-hook.env \
  scripts/mneme-agent-hook.sh doctor --check-stop-hook
```

For Claude Code Stop hooks, wire the wrapper command as the Stop hook and pipe
the hook JSON to stdin. If the current gate is incomplete, the wrapper emits
`{"decision":"block","reason":"..."}`. If `stop_hook_active` is already true or
the retry cap is exceeded, it exits zero without blocking.
