# Contract: command line

```
rust-ci-runner plan   --repos FILE --rows-out FILE                       (phase A: candidates, reads only)
rust-ci-runner plan   --rows-in FILE --rows-out FILE --summary FILE
                      [--cap N] [--now EPOCH]                            (phase C: refusals, cap, posts)
rust-ci-runner plan   --repos FILE --check                               (validate the list and log config)
rust-ci-runner job    --repo OWNER/NAME --sha SHA --project PATH --branch NAME --run-id ID
                      [--prev SHA] [--allow-docker] [--quiet-logs] [--private] [--retry]
                      [--names-file FILE --reason-file FILE]
rust-ci-runner status --repo OWNER/NAME --sha SHA --project PATH --run-id ID
                      --state error|failure|success --description TEXT [--retry]
rust-ci-runner run    [--dir PATH] [--quiet-logs] [STEP]
rust-ci-runner help
```

Removed: `serve`, `plan --list-projects`, `--secrets`, `--require-logs`.

Environment:

| Variable | Used by | Meaning |
|---|---|---|
| `FACTORY_CI_TOKEN` | plan, job, status | token with Contents read and Commit statuses read and write on the listed repos |
| `FACTORY_CI_TOKEN_DOCKER` | plan | same, for `allow_docker` entries only |
| `GITHUB_API_URL` | plan, job, status | API base, default `https://api.github.com` |
| `FACTORY_CI_LOG_URL`, `_USER`, `_TOKEN` | plan (phase C), job, run | Loki destination; all or none; resolved from the secret store by the wrapper |
| `FACTORY_CI_LOG_LINK` | job, status | link template with `{run_id}` |
| `B_PRIVATE` | plan | `true` or `false`: whether B is private (from the event payload) |
| `GITHUB_SERVER_URL`, `GITHUB_REPOSITORY`, `GITHUB_RUN_ID` | plan, job, status | Actions run link when shipping is off |
| `FACTORY_CI_CLONE_IMAGE`, `FACTORY_CI_DIND_IMAGE` | job | pinned images, `...@sha256:<64 hex>` |
| names the pipeline lists | job | read by name from the environment; the wrapper exports only those it resolved |
| `FACTORY_CI_ENGINE` | job, run | `docker` (default) or `podman` |
| `FACTORY_CI_SECRETS`, `FACTORY_CI_SHA`, `_PREV_SHA`, `_BRANCH`, `_EVENT` | run | as today |

## `plan`

See `plan-output.md` for both phases. Phase A needs `FACTORY_CI_TOKEN` (and `_DOCKER` when an entry has
`allow_docker`). `--check` needs no token and no network. Every message passes the sanitizer and the redactor
(FR-051, FR-024). The runner never calls the secret store, and never sends the heartbeat ping (the `verdict` job
does); it sends the Loki heartbeat line from phase C when the log credentials are present (FR-056).

## `job`

1. Validate flags and log config. `--quiet-logs` or `--private` without a complete log config exits 1 before
   anything runs, with the reason (FR-030, FR-049, FR-059).
2. Require `FACTORY_CI_CLONE_IMAGE` (and `FACTORY_CI_DIND_IMAGE` when a step needs docker) to be pinned by digest;
   otherwise exit 1 before cloning, "image is not pinned" (FR-057).
3. Clone `--sha` of `--repo` into a temp workspace with `FACTORY_CI_TOKEN` (git container, no credentials left on
   disk). The clone, the pipeline run and the sidecar all log through `Log`.
4. Use `workspace/<project>` as the pipeline directory (`.` is the workspace itself).
5. Read and parse `.factory-ci.toml` (symlink, size, step-count and graph rules unchanged), then check docker steps
   against `--allow-docker` and every listed secret name against `valid_secret_name`, all before any container starts.
6. Run the graph. A step whose container engine call exits 125 (image cannot be pulled, engine fault) and a failed
   clone end the item in `error`, not `failure` (FR-017). Flush the shipper. Post the final status with the link rule
   of FR-031; with `--retry` every description ends with " (retry)".
7. Exit 0 when the final status was delivered, whatever its state. Exit 1 when it was not.

Prepare mode (`--names-file F --reason-file R`): runs steps 1 to 5, starts no container, ships nothing and prints
nothing. Success: writes the sorted union of the steps' `secrets` names to `F`, one per line, exit 0. Failure: writes
one reason line to `R` (for example `invalid .factory-ci.toml: step build depends on unknown step compile`, or
`clone failed`), exit 1. A name that is not valid as a variable name, or is reserved, makes the pipeline invalid.

`--private` (rows for a private repo on a public B): implies `--quiet-logs`, requires a log store, and sends step
names, durations and results only to the log store; stdout stays empty (FR-059). `--quiet-logs` alone prints the
fixed events (`started`, `success in 12s`, ...) for public repos.

Reasons, descriptions and summary lines pass one sanitizer: characters outside `[A-Za-z0-9 _.:/-]` become `_`, a
leading `::` gets a prefix, length limited (FR-051). Step containers get `--cap-drop NET_RAW --cap-drop MKNOD
--cap-drop SETFCAP` (FR-054). The redactor covers each fetched value, its base64, and the base64 of
`x-access-token:<token>`. A value with a trimmed line under 4 characters is refused by the fetch script (name only,
FR-053).

## `status`

Posts one status without running anything. The workflow's fallback step calls it, through `factory-ci-status.sh`,
when `job` exited non-zero or was cancelled.

## `run`

Unchanged apart from `--quiet-logs` and the log shipping of story 11. It needs no token and no network unless log
shipping is configured. It never calls a secret store (FR-046).

Review round 1 (research D12): `job` prints "final status not delivered" plus the HTTP code when the last post fails,
never the request path (R4). A failure to spawn the container engine ends the item in `error` (R10). The Loki URL must
be `https`, except plain `http` to `127.0.0.1`, `localhost` or `[::1]`; another scheme or host is refused with "log URL must be https" (R8). `Log` replaces `\r` and other control
characters except tab before printing or shipping (R7).
