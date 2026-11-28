# Contract: step secrets in `ci`

The surfaces are two files, one lookup interface, one Bitwarden lookup rule and the public output. No flag, workflow input or runner setting changes.

## Watch entry (`.github/factory-ci-repos.toml`)

```toml
[[watch]]
repo = "sudtanj/factory-monorepo"
id = 1473152830
branches = ["main"]                  # explicit, no bare "*": required to receive secrets
secrets = ["SITE_API_KEY"]            # placeholder name; allowlist of step secret names for this repo
```

- `secrets`: names a pipeline of this repo may request through `ci`. Default none. At most 20, no duplicates, each a valid secret name. The list is per repo, not per Bitwarden project. Names are public in this file.
- Refused in this list: every reserved name (`FACTORY_CI*`, which covers the four runner settings, `BWS_`, `BW_`, `BITWARDEN`, `PATH` and so on). One such name refuses the whole file: `plan --check` reports it and `ci` prints `repository or ref not in the watch list`.
- Ref gate when a pipeline requests secrets: with `secret_branches` set, the ref must match it; without it, `branches` must be explicit and none of its patterns made only of `*` and `?` characters. Otherwise `[runner] repository or ref not in the watch list`.
- Only the entry of the repo being run counts. `sudtanj/infrastructure` never gets `secrets`. A name appears in one entry only.

## Pipeline step (`.factory-ci.toml`, untrusted, from the fetched branch)

```toml
[steps.e2e]
image = "<image>@sha256:<digest>"
commands = ["run-the-check"]
secrets = ["SITE_API_KEY"]
```

- `secrets`: names the step needs. Each must be in the entry's list. A step gets exactly its own names.
- Values arrive as environment variables of the container. Never put them in `env` or `commands`.
- A pipeline with a step that lists `secrets` and any step with `docker = true` is refused.
- Steps share `/workspace`: a step without secrets can leave code that a step with secrets runs. Pin images by digest.

## Lookup interface v1

```rust
// names in, name-to-value map out; the source is the caller's choice (feature 011 may swap it)
fn take(&self, names: &[String]) -> Result<StepSecrets, NamedError>   // on Held
```

- `StepSecrets`: wraps `BTreeMap<String, String>`; `Debug` prints names only.
- `NamedError`: `Count { index, found }` and `Unusable { index }`, keyed by position in `names`. It carries no name, item or field; `Display` and `Debug` hold indexes and counts, never a value. The caller maps an index to a name when it writes the Loki detail. Feature 011 may reuse the type; its logs must not map an index to a vault item or field name (011 FR-047).
- v1 source: `store::fetch_with`, which keeps the rows of the entry's allowlisted keys from the one `bws secret list` call. `take` validates lazily, only the names asked for, and the map holds exactly those names or the call fails as a whole.

## Lookup rule (v1 source)

- Store: Bitwarden, through `/usr/local/bin/bws` and the run's access token, the one call the run already makes (no filter, so the whole project comes back; 60 s per attempt, one retry without backoff, output capped at 16 MiB). There is no second call.
- Key: the shared key whose whole name equals the secret name, exact case. Exactly one match.
- Value: at most 32 KiB; not blank; no control character other than `\n` and `\r\n`; a single-line value has no leading or trailing whitespace (a trailing newline included); a multi-line value has every non-blank trimmed line at 4 or more characters, and blank inner lines and a trailing newline are allowed. Unicode and `=` pass. Passed unchanged.
- Delivery: `-e NAME` on the engine command and the value in the engine child's environment. Never argv.
- Masking: the value and each trimmed line of 4 or more characters always; its base64, the base64 of `x-access-token:<value>`, and (for forms of 8 to 48 KiB) base64 at any alignment, URL-safe and unpadded, percent-encoding, JSON escape and lowercase hex, all replaced with `***` in Loki. A transform beyond these is not caught, and the `echo "$V" | base64` form of a value under 6 bytes is not either.

## Public output (closed grammar)

Stdout is unchanged. Stderr gains two lines, each followed by exit code 1:

| Line | When |
|---|---|
| `[runner] step secret not allowed` | a step lists a name that is not in the entry's `secrets` |
| `[runner] step secret unavailable` | an allowed name has no, several or an unusable value, or the lookup call failed |

No name, count or value appears in a public line, and public lines are built from the fixed grammar, so a secret value cannot change them. Names and counts go to Loki as step `ci`, for example `secret SITE_API_KEY: expected 1, found 0`.

Existing lines that now apply: `pipeline file refused` (runner setting or malformed or reserved name in a step), `docker steps are refused` (a docker step in a pipeline with secrets) and `repository or ref not in the watch list` (the ref gate).
