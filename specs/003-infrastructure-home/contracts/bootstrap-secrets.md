# Contract: Bitwarden bootstrap values in the scripts and the tick (round 4)

Covers FR-010 to FR-016 and FR-047 to FR-059. Names, forms and load lists: `../data-model.md`. Round 4 replaced the Environment-secret contract of round 2.

## `.github/scripts/factory-ci-secrets.sh`

| Function | Change |
| --- | --- |
| `fci_list TOKEN` | Was `fci_list PROJECT TOKEN`. One `bws secret list -o json` with no project ID, `timeout 30`, retry with jittered backoff until `FACTORY_CI_FETCH_WINDOW` (60 s; `verdict` sets 20), no retry on a rejected token (400, 401, 403, `invalid_client`, and the older words). JSON stays in `FCI_JSON`. The "no secret project configured" error goes. |
| `fci_key_value`, `fci_lookup`, `fci_mask`, `fci_short_line`, `fci_clean` | Unchanged. |
| `fci_log_env` | Unchanged. It reads the three `FACTORY_CI_LOG_*` variables from the process environment, which now arrive through `$GITHUB_ENV`. |

## `.github/scripts/factory-ci-load.sh NAME...` (new)

Env in: `BWS_ACCESS_TOKEN`, `GITHUB_ENV`, `RUNNER_TEMP` (for `fci_list`'s stderr file), optional `FACTORY_CI_FETCH_WINDOW`. `bws` must be on `PATH` (the `install bws` step puts `$RUNNER_TEMP/bin` on `$GITHUB_PATH`, FR-063). Names come as arguments.

Allowlist inside the script (name, required or optional): the keys of the data model. A name outside it: exit 1, "unknown secret name".

Order of work:

1. `BWS_ACCESS_TOKEN` empty or unset: exit 1 with "BWS_ACCESS_TOKEN is not set in the factory-ci Environment". No `bws` call.
2. `fci_list`. Backoff is `1, 2, 4, 8` seconds plus 0 to 2 seconds of jitter, inside the window. A `bws` stderr that matches `(^|[^0-9])(400|401|403)([^0-9]|$)|invalid_client|unauthori[sz]ed|forbidden|invalid access token` is a rejected token: no retry, `FCI_ERR="secret store rejected the token"`. Everything else is retried until the window ends: "secret store unreachable". T066 records the real text of a revoked token.
3. For each requested name, `fci_key_value` (its `jq` stderr to `/dev/null`): 0 found, 1 missing or empty, 2 ambiguous.
   - Missing and required: exit 1 with "`KEY` is missing in Bitwarden". Missing and optional: skip.
   - Ambiguous: exit 1 with "`KEY` is ambiguous in Bitwarden".
4. Mask every value found (`fci_mask`), then the derived forms: the `jq @uri` form when it differs from the value, and the base64 of `LOG_USER:LOG_TOKEN` when both are found. Nothing else touches a value before this.
5. Refuse a value, by key, in these cases: a line under 4 characters; a CR, LF or NUL anywhere; leading or trailing whitespace; a trailing newline. The whitespace and newline checks run with `jq` on the JSON value, because `$(...)` strips a trailing newline. A value that starts with `=` is legal and written as is.
6. Build every `NAME=value` line in a variable, then append them with one `printf '%s'` to `$GITHUB_ENV`. `RUNNER_IMAGE_DIGEST` is written as `FCI_RUNNER_IMAGE_DIGEST`.

Never prints a value, `bws` stderr or `jq` stderr; never `set -x`; `umask 077`; `LC_ALL=C`. The `GITHUB_ENV` write happens once, after all checks, so a refused run leaves it untouched. Malformed `bws` output (not a JSON array) is a failed attempt and retried; its text is never printed.

## `factory-ci-plan.sh`

1. Blob-key check (64 lower-case hex), `fci_log_env` (all or none), as before. The `https://` check is gone (the install step owns it).
2. No `bws` call, no `fci_list`, no `FACTORY_CI_BWS_PROJECT`, no docker counts.
3. `run()` strips `BWS_ACCESS_TOKEN` and `FACTORY_CI_BLOB_KEY` from the runner's environment. Phase A also strips the three log variables (FR-037).
4. When `FCI_HAVE_LOGS` is set: refuse a short line and mask (kept; harmless after the load step).
5. Every row is sealed with `FACTORY_CI_BLOB_KEY`. Outputs: `matrix`, `count`, `fail_tick`.

## `factory-ci-job.sh`

1. `fci_log_env` first (unchanged). The `https://` check is gone.
2. `fci_list "$BWS_ACCESS_TOKEN"` runs only when the pipeline lists names. With names and no token: "no secret store configured" (unchanged). `FACTORY_CI_BWS_PROJECT` and `HAVE_BWS` are gone from the script and its final `unset`.
3. The rest is unchanged: reserved names are the runner's job, short-line refusal, masking, `fail_closed`, `--quiet-logs`.

## `.github/workflows/factory-ci-tick.yml`

Jobs `plan`, `run`, `verdict`, all `environment: factory-ci` and all with an end-anchored main-only `if:` (`verdict`: `if: github.ref == 'refs/heads/main' && always()`). Step order per job: `actions/checkout`, `install bws`, `load secrets`, then repo steps. No `uses:` after `load secrets`.

| Job | `load secrets` arguments | `secrets.BWS_ACCESS_TOKEN` mapped in |
| --- | --- | --- |
| `plan` | `RUNNER_IMAGE_DIGEST FACTORY_CI_BLOB_KEY FACTORY_CI_TOKEN FACTORY_CI_LOG_URL FACTORY_CI_LOG_USER FACTORY_CI_LOG_TOKEN FACTORY_CI_LOG_LINK` | `load secrets` |
| `run` | the `plan` list plus `FACTORY_CI_CLONE_IMAGE FACTORY_CI_DIND_IMAGE` | `load secrets` and `job` |
| `verdict` | `FACTORY_CI_HEARTBEAT_URL` | `load secrets` |

- `install bws`: the `run:` body is byte-identical in all three jobs. It checks the `https://` guard on `FACTORY_CI_BWS_SERVER_URL`, downloads the archive only with `curl --proto '=https' --tlsv1.2 --max-time 120`, compares its SHA-256 with `BWS_SHA256`, checks `unzip -Z1` lists exactly `bws`, installs to `$RUNNER_TEMP/bin`, appends `$RUNNER_TEMP/bin` to `$GITHUB_PATH` (FR-063), and runs `bws config server-base` when the variable is set. No `HAVE_BWS`, no checksum-file download.
- `verdict`: steps `checkout`, `install bws`, `load secrets` (the last two `continue-on-error: true` and `timeout-minutes: 2`, the load step with `FACTORY_CI_FETCH_WINDOW: 20`), then `fail on a bad plan`, then `heartbeat`, the existing step, unchanged (`if: always()`, `RUN_RESULTS: ${{ needs.run.result }}`). Job `timeout-minutes: 10`, `permissions: contents: read`, checkout with `persist-credentials: false`. The heartbeat is optional and the README says to leave the key unset for manual runs (R2-38).
- The `guard, pinned images` step reads `FCI_RUNNER_IMAGE_DIGEST` and, in `run`, `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE` from the environment. In `run` it also refuses a reference that does not start with `ghcr.io/$owner/` (owner from `github.repository_owner`, lower-cased) before the `@sha256` check (FR-068). `plan` checks the digest and the blob key only.
- No step maps any other `secrets.` or `vars.` except `vars.FACTORY_CI_BWS_SERVER_URL` in the three `install bws` steps.
- `fetch runner` uses `FCI_RUNNER_IMAGE_DIGEST` and keeps the fixed "not pullable" message.
- `status fallback` maps `ITEM_BLOB` only; the blob key and token come from `$GITHUB_ENV`.
- Every step after `load secrets`, and the checkout post step, inherits `$GITHUB_ENV` (accepted, R-22).

## Invariants

- I1. Log state is `on`, `off` or `partial`; `partial` always stops the tick or the item.
- I2. No value of any Bitwarden name appears in any output of `load.sh`, `plan.sh` or `job.sh`, except inside `::add-mask::` lines.
- I3. `plan.sh` makes zero `bws` calls. `job.sh` makes zero for a no-secret pipeline.
- I4. A pipeline secret has one source: Bitwarden.
- I5. In `load.sh`, every mask line precedes the first `$GITHUB_ENV` write, and the file holds only requested names.
