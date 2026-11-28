# Data model: ci-state-queue

## Schedule entry (`.github/factory-ci-schedule.toml`, `[[schedule]]`)

| Field | Type | Rule |
|---|---|---|
| `repo` | string | `owner/name`, as the watch list; required |
| `branch` | string | 1 to 200 characters of `[A-Za-z0-9._/-]`, no leading `-`, no `..`; required |
| `project` | string | `.` or a folder name by `folder_name_ok`; default `.` |
| `interval_minutes` | integer | multiple of 15, 15 to 10080; required |

File rules: at most 10 entries, an empty file is valid, unknown or repeated keys refuse the whole file, no two entries share `(repo case-folded, branch, project)`. An entry grants nothing: `ci` runs it only when the watch list allows its repo (numeric `id`) and branch.

Due: `minute_epoch_utc % interval_minutes < 15`.

## Watch entry additions

| Key | Type | Default | Rule |
|---|---|---|---|
| `state` | bool | false | needs a numeric `id`; the repo must be private at run time |
| `vault_items` | list of strings | empty | each 1 to 100 characters, no control character, no duplicate, at most 20; exact-byte match |

## Pipeline file additions

```toml
[state]                         # at most one table, pipeline-wide
name = "playwright-auth"        # 1 to 64 of [A-Za-z0-9_-]
path = ".auth/state.json"       # relative, no "..", no NUL or newline, a file

[artifacts]
paths = ["test-results", "playwright-report"]   # at most 20; a file or a folder
when = "failure"                # "failure" (default) or "always"

[steps.e2e]
state = true                    # the one step that sees the state file
vault = [{ item = "playwright-login", field = "username", env = "E2E_USER" }]
```

Rules: exactly one step has `state = true` when `[state]` exists, none otherwise; every other step is a transitive dependency or dependent of it; an artifact path never equals, contains or lies under the state path (compared by path component); names in `env`, `secrets` and vault `env` are refused when upper-cased they start with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI`, or equal a runner key; at most 20 vault entries per step and 50 per pipeline; a step with `vault` cannot set `docker = true`.

## Runner keys

Ten names. Read at start (`BOOT_KEYS`): `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. Read on need: `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY` (needed by state, by artifacts, or by both as the spec says), `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`. Value rule as 010: 4 characters or more, no CR, LF, NUL, tab, trimmed; the two keys 32 or more and different.

## Ticket

`release` `ci-queue-<state>`, asset `t-<runid>` (1 byte), label `<runid>.<epoch>`. State machine: `absent` -> `registered` -> `claimed` (lowest live id, settled) -> `released` (deleted). `registered` -> `withdrawn` (position over 3). Any live ticket whose label epoch is 600 s or more behind the reader's latest server time is `stale` and any reader deletes it.

## State asset

Release `ci-state-<name>`, asset `state-<runid>-<8 hex>.bin`, format `FCI1` kind 1. Newest 2 kept; the newest by id with API state `uploaded` is restored. Plaintext at most 10 MB.

## Artifact bundle

Release `ci-artifacts-<project>` (`.` becomes `root`, other dots become `_`), asset `artifact-<runid>-<8 hex>.bin`, format `FCI1` kind 2, plaintext a tar. Newest 10 kept. Plaintext at most 100 MB.

## Vault read

Input: `(item, field)` pairs, deduplicated. Output: a map from pair to value, held in memory, passed only to steps that declared the pair. No item or field name, and no value, reaches a log line or Loki.

## Public lines

stdout adds `[runner] waiting for the queue`. stderr adds `queue is full`, `queue wait timed out`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused` (25 lines in `FAULTS` after 010's 16). `due` prints one fixed stderr line of its own, `schedule file refused`.
