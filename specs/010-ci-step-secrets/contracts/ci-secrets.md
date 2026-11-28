# Contract: step secrets in `ci`

The surfaces are two files, one Bitwarden lookup rule and the public output. No flag, workflow input or runner setting changes.

## Watch entry (`.github/factory-ci-repos.toml`)

```toml
[[watch]]
repo = "sudtanj/factory-monorepo"
id = 1473152830
branches = ["main"]
secrets = ["E2E_BASE_URL", "E2E_USER", "E2E_PASSWORD"]   # allowlist of step secret names
```

- `secrets`: names a pipeline of this repo may request through `ci`. Default none. Format and limits unchanged: at most 20, no duplicates, each a valid secret name.
- Refused in this list, as before: the four runner settings and every other reserved name (`FACTORY_CI*`, `BWS_`, `PATH` and so on). One such name refuses the whole file: `plan --check` reports it and `ci` prints `repository or ref not in the watch list`.
- `secret_branches` stays ignored by `ci`; `branches` is its only ref gate.
- Only the entry of the repo being run counts.

## Pipeline step (`.factory-ci.toml`, untrusted, from the fetched branch)

```toml
[steps.e2e]
image = "mcr.microsoft.com/playwright:v1.50.0-noble"
commands = ["npx playwright test"]
secrets = ["E2E_BASE_URL", "E2E_USER", "E2E_PASSWORD"]
```

- `secrets`: names the step needs. Each must be in the entry's list. A step gets exactly its own names.
- Values arrive as environment variables of the container. Never put them in `env` or `commands`.
- A step with `docker = true` and `secrets` is refused.

## Lookup rule

- Store: Bitwarden, through `/usr/local/bin/bws` and the run's access token. One `secret list` call (60 s per attempt, one retry) when a step lists a name.
- Key: the shared key whose whole name equals the secret name, exact case, in the project the token reads. Exactly one match.
- Value: not empty, no NUL, every trimmed line 4 or more characters. Multi-line, Unicode and `=` pass as they are.
- Delivery: `-e NAME` on the engine command and the value in the engine child's environment. Never argv.

## Public output (closed grammar)

Stdout is unchanged. Stderr gains two lines, each followed by exit code 1:

| Line | When |
|---|---|
| `[runner] step secret not allowed` | a step lists a name that is not in the entry's `secrets` |
| `[runner] step secret unavailable` | an allowed name has no, several or an unusable value, or the lookup call failed |

No name, count or value appears in a public line. Names and counts go to Loki as step `ci`, for example `secret E2E_PASSWORD: expected 1, found 0`.

Existing lines that now apply to secrets: `pipeline file refused` (a step lists a runner setting or a malformed name) and `docker steps are refused` (docker step with secrets).
