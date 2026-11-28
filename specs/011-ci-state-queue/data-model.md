# Data model: ci-state-queue

## Schedule

A table of a project's `.factory-ci.toml`, evaluated by `plan` and recomputed by the run job.

| Key | Type | Rule |
|---|---|---|
| `every` | string, required | `^[1-9][0-9]{0,4}[mhd]$`, from 900 s (`15m`) to 2,678,400 s (`31d`) |
| `offset` | string, optional | `^(0|[1-9][0-9]{0,4}[mhd])$`, smaller than `every`, default 0 |

No other key. Time is UTC. `due(now) = now - ((now - offset) mod every)`, in seconds since the Unix epoch. Honoured only for `main` of a private repo whose entry has `allow_docker = false` and `secret_branches` exactly `["main"]`, for a project that is not `docker/<name>`.

## Claim

One 1-byte asset per claim in release `ci-sched+<owner>_<name>` of `<owner>/factory-ci-state`.

| Field | Rule |
|---|---|
| name | `claim-<h16>-<slot>-<n>`, `^claim-[0-9a-f]{16}-[0-9]{1,12}-[01]$`; `<h16>` is the first 16 hex characters of SHA-256 over `project`, a NUL byte and `branch`; `<slot>` is the decimal epoch of the slot; `<n>` is 0, or 1 for the retry of a stale claim |
| label | `<o>.<epoch>.<runid>`, `^[rpfe]\.[0-9]+\.[0-9a-f]{16}$`; `r` running, `p` passed, `f` failed, `e` closed with an error; `<runid>` is the row's run id |
| writer | its creator only (create, then `PATCH` of the label); `plan` only lists |
| stale | label missing or malformed, epoch more than 120 s ahead of the reader's `Date`, or `r` with the epoch 3600 s or more behind |
| count | only `uploaded` assets; at most 2 per `<h16>` after a claim ends; any claim older than 90 days is swept; more than 300 assets in the release fails closed |

State machine: absent -> `r` -> `p` or `f` (the creator's label write) -> swept. A stale `r` with `n = 0` is followed by a new claim `n = 1`; a stale `n = 1` closes the slot with an `error` status.

## Watch entry

No new key. `secret_branches`, written as exactly `["main"]` (one literal item), grants the tick the store, state, queue, artifacts, vault values and declared step secrets for rows on `main` of a private repo (`store_allowed`). `state` and `vault_items` do not exist: the pinned 0.3.0 refuses unknown watch keys. Reviewed on `main` of B.

## Pipeline file additions

```toml
[schedule]                      # at most one table
every  = "6h"
offset = "3h"                   # optional

[state]                         # at most one table, pipeline-wide
name = "playwright-auth"        # 1 to 64 of [A-Za-z0-9_-]
path = ".auth/state.json"       # relative, no "..", no NUL or newline, a file

[artifacts]
paths = ["test-results"]        # at most 20; a file or a folder
when = "failure"                # "failure" (default) or "always"

[steps.e2e]
state = true                    # the one step that sees the state file
vault = [{ item = "ci-login-1", field = "username", env = "E2E_USER" }]   # read from 0.4.1
```

Rules: exactly one step has `state = true` when `[state]` exists, none otherwise; every other step is a transitive dependency or dependent of it; an artifact path never equals, contains or lies under the state path (by path component); a project with `..`, a trailing `.` or `.lock` cannot use `[state]` or `[artifacts]`; names in `env`, `secrets` and vault `env` are refused when upper-cased they start with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` or equal a runner key; a name present in both a `secrets` list and a vault `env` anywhere in the pipeline is refused; at most 20 vault entries per step and 50 per pipeline; a step with `vault` cannot set `docker = true`; a pipeline with `[state]` has no step with `docker = true`. The 0.3.0 runner refuses every key above (`deny_unknown_fields`).

## Runner keys

Thirteen names in `store::RUNNER_KEYS`. The first four are `REQUIRED_KEYS`, read at start: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. The others: `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID`, `FACTORY_CI_VAULT_COLLECTIONID`. On the tick they arrive as process variables from the frozen script's names-file route (Bitwarden keys `<repo><SEP><NAME>`), only for `main` rows with `secrets_ok`, and `take_runner_env` takes them once and removes them. Value rule as 010 (`Held::take`). A pipeline with `[state]` needs the token and the state key, `[artifacts]` the token and the artifact key, a scheduled row the token; each key is base64 or hex text decoding to 32 bytes or more with 8 distinct byte values; the two keys differ; the store repo is the derived `<owner>/factory-ci-state` and a `FACTORY_CI_STATE_REPO` value must equal it.

## Ticket

Release `ci-queue+<repo>+<project>+<state>` in the store repo, asset `t-<runid>-<attempt>-<entry>` (1 byte), label `<runid>.<epoch>`. `<runid>` is the row's run id (16 hex), `<attempt>` is `GITHUB_RUN_ATTEMPT`, `<entry>` is always `0`. State machine: `absent` -> `registered` -> `claimed` (lowest live id, settled) -> `released` (deleted). No cap. Live means an `uploaded` asset, or a non-`uploaded` asset that matches the ticket regex and is under 120 s old; any other non-`uploaded` asset is deleted only once it is over 360 s old. Stale means the label is missing, malformed, more than 120 s ahead of the reader's latest `Date`, or 600 s or more behind it. A run that cannot claim before its budget (the budget end minus 30 minutes, 5 minutes after the runner start) deletes the ticket and ends `queue wait expired`; for a scheduled row that leaves no claim, so the slot stays due.

## State asset

Release `ci-state+<repo>+<project>+<state>`, asset `state-<runid>-<attempt>-<entry>-<8 hex>.bin`, format `FCI1` kind 1. Newest 2 `uploaded` kept; the newest by id among `uploaded` is restored. Plaintext at most 10 MB.

## Artifact bundle

Release `ci-artifacts+<repo>+<project>`, asset `artifact-<runid>-<attempt>-<entry>-<8 hex>.bin`, format `FCI1` kind 2, plaintext a tar. Newest 10 `uploaded` kept. Plaintext at most 100 MB, written under the run's work folder and removed with it.

## Vault read (0.4.1)

Input: `(item, field)` pairs, deduplicated. Output: a map from pair to value, held in memory, passed only to steps that declared the pair. No item or field name and no value reaches a log line or Loki.

## Scheduled row

A plan row with `prev_sha == sha`, `secrets_ok = true`, `retry = false`, no `pending` status. Its status context is `factory-ci-schedule` (root project) or `factory-ci-schedule/<project>`.

## Public texts

On the tick the fixed texts go in the commit status description: `queue wait expired`, `queue renewal failed`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`, plus the existing 010 reasons. The manual `ci` prints `[runner] state not allowed` for the refused declarations. No repo, branch, project, state, release, asset, item, field or secret name appears.

## Field names cited by tasks

`Schedule.every`, `Schedule.offset`; `Claim.name`, `Claim.label`; `Runner keys.FACTORY_CI_STATE_KEY`, `Runner keys.FACTORY_CI_ARTIFACT_KEY`; `Ticket.name` (the asset regex) and `Ticket.label` (`<runid>.<epoch>`); `State asset.asset` and `State asset.plaintext` (at most 10 MB); `Artifact bundle.asset` and `Artifact bundle.plaintext` (at most 100 MB); `Vault read.map`; `Watch entry` (no key).
