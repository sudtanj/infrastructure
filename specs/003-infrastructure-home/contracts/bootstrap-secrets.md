# Contract: bootstrap secrets in the scripts and the tick

Covers FR-010 to FR-016. Matrix of names and Environments: `../data-model.md`.

## `.github/scripts/factory-ci-secrets.sh`

| Function | Before | After |
| --- | --- | --- |
| `fci_log_creds` | Reads the three keys from `FCI_JSON` | Removed |
| `fci_log_env` | none | Reads `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN` from the process environment. Three non-empty: sets `FCI_LOG_URL`, `FCI_LOG_USER`, `FCI_LOG_TOKEN`, `FCI_HAVE_LOGS=1`, returns 0. All three empty or unset: leaves `FCI_HAVE_LOGS` empty, returns 0. Otherwise: `FCI_ERR="log credentials incomplete: FACTORY_CI_LOG_X, ..."` (names of the empty ones), returns 1. |

`fci_log_env` never prints a value. Its error text names variables, never values.

## `factory-ci-plan.sh`

1. After the blob-key check and before `plan ... --rows-out`, call `fci_log_env`. On error: `fail "$FCI_ERR"` (the tick fails before any GitHub read).
2. Inside the `total > 0` block: the `fci_list` calls stay (token check, 002 FR-045). Delete both `fci_log_creds` calls and the docker `have` juggling.
3. When `FCI_HAVE_LOGS` is set: refuse a short line, mask each value, export `FACTORY_CI_LOG_*` for phase C. This now happens for every tick that has rows, with or without a Bitwarden token.

## `factory-ci-job.sh`

1. `fci_log_env` runs first. On error: `refuse "$FCI_ERR"`.
2. `fci_list` runs only when `${#names[@]} -gt 0`. With names and no token, the existing "no secret store configured" refusal fires earlier.
3. Short-line check, the `fail_closed` rule ("private repository and no log store"), masking and `--quiet-logs` are unchanged.
4. The final `unset` list adds nothing: the runner needs the log variables.

## `.github/workflows/factory-ci-tick.yml`

| Job | Step | New `env` |
| --- | --- | --- |
| `plan` | `plan` | `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN` from `secrets.*` |
| `run` | `job` | same |
| `run-docker` | `job` | same |

No other step gets them. In the three jobs, `fetch runner` replaces its bare `docker create` with:

```sh
id=$(docker create "ghcr.io/$owner/rust-ci-runner@$RUNNER_IMAGE_DIGEST" 2> /dev/null) \
  || { echo "the runner image is not pullable; make the GHCR package public" >&2; exit 1; }
```

## Invariants

- I1. Log state is `on`, `off` or `partial`; `partial` always stops the tick or the item.
- I2. No value of the three log secrets, the tokens or the blob keys appears in any output of `plan.sh` or `job.sh`, except inside `::add-mask::` lines.
- I3. With the log variables empty and `BWS_ACCESS_TOKEN` set, `job.sh` for a no-secret pipeline calls `bws` zero times.
- I4. A pipeline secret has one source: Bitwarden.
