# Contract: step secrets in `ci`

The surfaces are two files, one lookup interface, one Bitwarden lookup rule and the public output. No flag, workflow input or runner setting changes.

## Watch entry (`.github/factory-ci-repos.toml`)

```toml
[[watch]]
repo = "sudtanj/factory-monorepo"
id = 1473152830
branches = ["main"]                  # explicit, no bare "*": required to receive secrets
secrets = ["BROWSER_TEST_URL"]       # example only; allowlist of step secret names for this repo
```

- `secrets`: names a pipeline of this repo may request through `ci`. Default none. At most 20, no duplicates, each a valid secret name. The list is per repo, not per Bitwarden project. Names are public in this file.
- Refused in this list: the four runner settings (`RUNNER_KEYS`) and every reserved name (`FACTORY_CI*`, `BWS_`, `BW_`, `BITWARDEN`, `PATH` and so on). One such name refuses the whole file: `plan --check` reports it and `ci` prints `repository or ref not in the watch list`.
- Ref gate when a pipeline requests secrets: with `secret_branches` set, the ref must match it; without it, `branches` must be explicit and none of its patterns made only of `*`. Otherwise `[runner] repository or ref not in the watch list`.
- Only the entry of the repo being run counts. `sudtanj/infrastructure` never gets `secrets`. A name appears in one entry only.

## Pipeline step (`.factory-ci.toml`, untrusted, from the fetched branch)

```toml
[steps.e2e]
image = "mcr.microsoft.com/playwright@sha256:<digest>"
commands = ["npx playwright test"]
secrets = ["BROWSER_TEST_URL"]
```

- `secrets`: names the step needs. Each must be in the entry's list. A step gets exactly its own names.
- Values arrive as environment variables of the container. Never put them in `env` or `commands`.
- A pipeline with a step that lists `secrets` and any step with `docker = true` is refused.
- Steps share `/workspace`: a step without secrets can leave code that a step with secrets runs. Pin images by digest.

## Lookup interface v1

```rust
// names in, name-to-value map out; the source is the caller's choice (feature 011 may swap it)
Fn(&[String]) -> Result<StepSecrets, NamedError>
```

- `StepSecrets`: wraps `BTreeMap<String, String>`; `Debug` prints names only.
- `NamedError`: `Store(StoreError)`, `Count(name, n)`, `Unusable(name)`. `Display` and `Debug` hold names and counts, never a value.
- v1 source: `store::fetch_named`, closure built in `job()` with the Bitwarden access token. The map holds exactly the requested names or the call fails as a whole.

## Lookup rule (v1 source)

- Store: Bitwarden, through `/usr/local/bin/bws` and the run's access token (never the GitHub clone token). One `secret list` call (no filter, so the whole project comes back; 60 s per attempt, one retry without backoff, output capped at 16 MiB) when a step lists a name.
- Key: the shared key whose whole name equals the secret name, exact case. Exactly one match. Names in `RUNNER_KEYS` are never returned.
- Value: not blank; no control character other than `\n` and `\r\n`; every non-blank trimmed line 4 or more characters; blank inner lines allowed. Unicode and `=` pass. Passed unchanged.
- Delivery: `-e NAME` on the engine command and the value in the engine child's environment. Never argv.
- Masking: the value, base64 (any alignment, URL-safe, unpadded), percent-encoding, JSON escape, lowercase hex and each long line are replaced with `***` in Loki; a transform beyond these is not caught.

## Public output (closed grammar)

Stdout is unchanged. Stderr gains two lines, each followed by exit code 1:

| Line | When |
|---|---|
| `[runner] step secret not allowed` | a step lists a name that is not in the entry's `secrets` |
| `[runner] step secret unavailable` | an allowed name has no, several or an unusable value, or the lookup call failed |

No name, count or value appears in a public line, and public lines are built from the fixed grammar, so a secret value cannot change them. Names and counts go to Loki as step `ci`, for example `secret BROWSER_TEST_URL: expected 1, found 0`.

Existing lines that now apply: `pipeline file refused` (runner setting or malformed or reserved name in a step), `docker steps are refused` (a docker step in a pipeline with secrets) and `repository or ref not in the watch list` (the ref gate).
