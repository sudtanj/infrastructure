# Data model: ci-state-queue

## Cron entry (in `factory-monorepo-ci.yml`)

| Field | Rule |
|---|---|
| `cron` line | at most 3 in the workflow; one `case` branch each |
| `repo` | `owner/name`, a plain word with one `/` |
| `branch` | 1 to 200 characters of `[A-Za-z0-9._/-]`, no leading `-`, no `..` |
| `project` | `.` or a folder name by `folder_name_ok` |
| `entry` | unique digit 1 to 3; a manual run is 0 |

An entry grants nothing: `ci` runs it only when the watch list allows its repo (numeric `id`) and branch.

## Watch entry additions

| Key | Type | Default | Rule |
|---|---|---|---|
| `state` | bool | false | needs a numeric `id`, an explicit `secret_branches = ["main"]`, and the repo must be private at run time |
| `vault_items` | list of strings | empty | needs explicit `secret_branches = ["main"]`; each 1 to 100 characters, no control character, no duplicate, at most 20; exact-byte match; names are opaque (`ci-login-1`) |

## Pipeline file additions

```toml
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

Rules: exactly one step has `state = true` when `[state]` exists, none otherwise; every other step is a transitive dependency or dependent of it; an artifact path never equals, contains or lies under the state path (by path component); a project with `..`, a trailing `.` or `.lock` cannot use `[state]` or `[artifacts]`; names in `env`, `secrets` and vault `env` are refused when upper-cased they start with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` or equal a runner key; a name present in both a `secrets` list and a vault `env` anywhere in the pipeline is refused; at most 20 vault entries per step and 50 per pipeline; a step with `vault` cannot set `docker = true`; a pipeline with `[state]` has no step with `docker = true`.

## Runner keys

Thirteen names in `store::RUNNER_KEYS`. The first four are `REQUIRED_KEYS`, read at start: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. The others are held from the same single `bws secret list` call and taken on need: `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID`, `FACTORY_CI_VAULT_COLLECTIONID`. Value rule as 010 (`Held::take`). When the pipeline has `[state]` or `[artifacts]`, `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_REPO` and both keys must exist; each key is base64 or hex text decoding to 32 bytes or more with 8 distinct byte values; the two keys differ.

## Ticket

Release `ci-queue+<repo>+<project>+<state>` in the store repo, asset `t-<runid>-<attempt>-<entry>` (1 byte), label `<runid>.<epoch>`. State machine: `absent` -> `registered` -> `claimed` (lowest live id, settled) -> `released` (deleted). No cap. Live means an `uploaded` asset, or a non-`uploaded` asset that matches the ticket regex and is under 120 s old; any other non-`uploaded` asset is deleted only once it is over 360 s old. Stale means the label is missing, malformed, more than 120 s ahead of the reader's latest `Date`, or 600 s or more behind it. A run that cannot claim before its budget (`FACTORY_CI_BUDGET_END` minus 45 minutes) deletes the ticket and ends `queue wait expired`.

## State asset

Release `ci-state+<repo>+<project>+<state>`, asset `state-<runid>-<attempt>-<entry>-<8 hex>.bin`, format `FCI1` kind 1. Newest 2 `uploaded` kept; the newest by id among `uploaded` is restored. Plaintext at most 10 MB.

## Artifact bundle

Release `ci-artifacts+<repo>+<project>`, asset `artifact-<runid>-<attempt>-<entry>-<8 hex>.bin`, format `FCI1` kind 2, plaintext a tar. Newest 10 `uploaded` kept. Plaintext at most 100 MB, written under the run's `--work` folder and removed with it.

## Vault read (0.4.1)

Input: `(item, field)` pairs, deduplicated. Output: a map from pair to value, held in memory, passed only to steps that declared the pair. No item or field name and no value reaches a log line or Loki.

## Public lines

stdout adds `[runner] waiting for the queue`. stderr adds `queue wait expired`, `queue renewal failed`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused` (nine lines on top of 010's list, whatever its length).

## Field names cited by tasks

`Cron entry.entry`, `Cron entry.branch`, `Cron entry.project`; `Watch entry additions.state`, `Watch entry additions.vault_items`; `Runner keys.FACTORY_CI_STATE_KEY`, `Runner keys.FACTORY_CI_ARTIFACT_KEY`; `Ticket.name` (the asset regex) and `Ticket.label` (`<runid>.<epoch>`); `State asset.asset` (name regex) and `State asset.plaintext` (at most 10 MB); `Artifact bundle.asset` and `Artifact bundle.plaintext` (at most 100 MB); `Vault read.map` (keyed by `(item, field)`).
