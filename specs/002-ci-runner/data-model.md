# Data Model: Central Interval CI Runner

No database and no files between ticks. Commit statuses on the watched repo are the only
persistent state. The types below live in memory for one `plan` or one `job` process. Tasks cite
fields as `data-model.md Entity.field`.

## WatchEntry (watch.rs)

One `[[watch]]` table of the list file.

| Field | Type | Rule |
|---|---|---|
| `repo` | string | `owner/name`. owner: 1 to 39 chars of `[A-Za-z0-9-]`, no leading or trailing `-`. name: 1 to 100 chars of `[A-Za-z0-9._-]`, not `.` or `..`. Required. Also matches `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$` with no `..` anywhere, so `../x`, `*`, `[a-z]`, `a b` and `a/b/c` fail. Duplicates compare case-insensitively. |
| `id` | integer | Optional. The numeric GitHub repo id. `plan` checks it against `GET /repos/{o}/{n}`; a mismatch skips the repo. Required for the `owner/name<SEP>NAME` Bitwarden keys to apply. |
| `branches` | list of glob | Optional. Default: every branch. At most 20 globs. Each 1 to 100 chars of printable ASCII, no whitespace and none of `\ [ ] { } ! ^ ~ : ` and no `..`. `*` matches any run of characters including `/`; `?` matches one. |
| `secrets` | list of name | Optional. Default: none. At most 20. Each passes `pipeline::valid_secret_name`, so `FACTORY_CI*`, `BWS_*`, `GITHUB_*`, `DOCKER_*` and the other reserved names fail. No duplicates. Grants the unprefixed key `NAME` of the default Bitwarden project, and nothing else. A repo's own prefixed keys need no listing (they need the entry's `id`). |
| `secret_branches` | list of glob | Optional. Default: the repo's default branch (from `GET /repos/{r}`). Same glob rules as `branches`. An item on a branch that matches none gets no secrets (FR-060). |
| `allow_docker` | bool | Optional. Default false. |

List file: at most 64 KB and 100 entries. Unknown keys at any level are errors. Zero entries is
valid (the tick does nothing and succeeds).

## Branch, Commit, StatusEntry, ChangedFile (github.rs)

| Entity | Fields |
|---|---|
| `Branch` | `name`, `sha` (40 lowercase hex, checked on receipt) |
| `Commit` | `sha`, `parents: Vec<sha>` (first is the first parent), `date` (RFC 3339 `Z`, committer date) |
| `StatusEntry` | `context`, `state` (`pending`, `success`, `failure`, `error`), `updated_at` (RFC 3339 `Z`), `description`, `creator_login` |
| `TreeEntry` | `path`, `kind` (`blob`, `tree`, other), plus a `truncated` flag on the answer |
| `ChangedFile` | `filename`, `previous_filename: Option` (renames count for both paths) |

## Project (projects.rs)

| Field | Rule |
|---|---|
| `path` | `.` for the root, else a top-level folder name: 1 to 100 chars of `[A-Za-z0-9._-]`, not `.` or `..`, not starting with `-`. |
| `context` | `factory-ci` for the root, else `factory-ci/<path>`. |

A project exists at a tip when the tree has `.factory-ci.toml` as a blob of any mode at the root,
or at `<folder>/.factory-ci.toml` for a top-level tree entry. Deeper files, and entries that are
submodules or symlinked folders, are not projects. A folder whose name fails the `path` rule
and holds the file gets an `error` status ("invalid project folder name") from the plan and no job.

`owns(project, file_path, project_set)`: a folder project owns paths that start with
`<folder>/`. The root project owns every path that no folder project of `project_set` owns. The
match is on the whole first segment plus `/`, so `foo/` does not own `foobar/x`.

## Item (plan.rs), the matrix row

Contract in `contracts/plan-output.md`. Fields: `repo`, `sha`, `branch`, `project`,
`context`, `prev_sha` (empty string when unknown), `repo_id` (verified id or empty), `fail_closed` (B public and repo private), `retry`, `secrets_ok`, `allow_docker`, `secrets` (the entry's shared key names, space-separated),
`run_id` (16 hex), `n` (item number, unique across `run` and `run-docker`), `date` (sort key, not emitted).

Order: within a repo `date` ascending (missing last), then `branch`, `project`; across repos round-robin starting with the repo whose oldest row is oldest (FR-055). The workflow sees only `{n, blob}` (FR-059).

## PlanResult (plan.rs)

`items` (capped), `unchanged` (posted), `waiting` (count over the cap), `problems`
(repo name or `list`, reason), `unchecked` (repos not reached after a rate limit). The tick fails
when `problems` or `unchecked` is non-empty.

## Decision per project (walk.rs)

For one (repo, tip, project) the planner returns one of:

| Decision | When |
|---|---|
| `Done` | the tip has a status for the context, unless it is `pending` older than 7200 s or `error` older than 1800 s, and the description does not end with " (retry)" (then the retry budget is unused: `Run` with `retry` true). A `pending` older than 7200 s that ends with " (retry)" is closed with an `error` post (counts as a write). |
| `Unchanged { since }` | nearest first-parent ancestor within 50 commits with a status for the context is `success`, and no changed file between it and the tip belongs to the project |
| `Run { prev }` | every other case: no ancestor, ancestor state not `success`, ancestor stale or fresh `pending`, a file of the project changed, any walk failure. `prev` is that ancestor when one was found. |

## LogConfig, LogLine (ship.rs)

| Entity | Field | Rule |
|---|---|---|
| `LogConfig` | `url` | `FACTORY_CI_LOG_URL`. `http://` or `https://`, no trailing `/` after trimming, no userinfo. |
| | `user`, `token` | `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. Non-empty. Both are redaction values. |
| | `link` | Optional `FACTORY_CI_LOG_LINK`. Must contain `{run_id}` and start with `http://` or `https://`. |
| | set rule | all three set means on, none set means off, a partial set is an error naming each missing name. |
| `LogLine` | `ts_ns` | runner clock, nanoseconds since the epoch |
| | `labels` | `repo`, `project`, `step`, `stream` (`stdout` or `stderr`), and nothing else |
| | `text` | redacted, at most 16 KB, NUL replaced by U+FFFD, prefix `run=<id> sha=<8> branch=<=40 chars> ` added at ship time |

Limits: queue 32 MiB, batch 1 s or 1 MB, per-run output 10 MB (`MAX_LOG_BYTES`, the old
`log_max_mb` default), retry window 60 s, flush window 60 s, request timeout 10 s.

## JobResult (job.rs)

`state` (`success`, `failure`, `error`), `reason` (one line, redacted), `delivered` (bool from
the shipper). Description sent to GitHub: `reason`, shortened first so that reason plus the
suffix `" (logs not delivered)"` fits 140 characters, then the suffix when `delivered` is false.

## Status posted

`POST /repos/{repo}/statuses/{sha}` with `state`, `context`, `description` (at most 140 chars),
`target_url` (optional). Contexts: see Project. Descriptions: `queued` (pending, no `target_url`),
`unchanged since <8-char sha>`, `all steps passed`, `step <name> failed: <why>`, `run timed out`,
or the one-line error reason. At most 3 updates per commit and context in practice.

## State transitions of a context on a commit

```
(none) --plan, changed/no ancestor--> pending --job--> success | failure | error
(none) --plan, unchanged-----------> success
pending older than 7200 s: see the retry lines below
success | failure -------------> final
error, older than 1800 s, description not ending " (retry)" -> treated as (none): one retry (A6c)
pending older than 7200 s, description not ending " (retry)" -> same: replanned with "queued (retry)"
error ending " (retry)" -> final; pending older than 7200 s ending " (retry)" -> closed by the tick with error
```

## Secret names and sources (job prepare, scripts)

- `names`: the union of every step's `secrets` list in the project's validated pipeline, sorted,
  deduplicated. Each name passes `pipeline::valid_secret_name` (environment-variable shape, not
  reserved). Written by `job ... --names-file`. A pipeline with a bad or reserved name is invalid and
  yields a reason, not a names file.
- Source order per name (FR-040), first match wins, always in the default project (the docker project for
  `allow_docker` repos): (1) key `owner/name<SEP>NAME`, only when `WatchEntry.id` is recorded; (2) key `NAME`, only
  when `WatchEntry.secrets` lists it. `SEP` is the constant `FCI_KEY_SEP`: `/` first, `:` if Bitwarden refuses `/`,
  never `__` (it can collide). There are no per-repo projects and no GitHub secret source.
- No secret store (`BWS_ACCESS_TOKEN` unset): a pipeline that lists a name gets the reason "no secret store
  configured". A row with `secrets_ok` false gets "secrets not allowed on this branch" (`WatchEntry.secret_branches`).
- Log values: default project keys `FACTORY_CI_LOG_URL`, `_USER`, `_TOKEN`, no prefix. All three or none.

## Limits and text rules added by the review (`research.md` D10)

- `WRITE_CAP` = 60 status writes per tick; `PACE_MS` = 750 between writes; `RATE_RESERVE` = 60 reads left for writes.
- Cap order: interleaved across repos (round-robin), oldest first inside a repo.
- Sanitized text (reasons, descriptions, summary lines): only `[A-Za-z0-9 _.:/-]`, other characters become `_`, a
  leading `::` gets a prefix, at most 140 characters for a description.
- Reserved secret names, matched on the upper-cased name: `BWS_*`, `FACTORY_CI*`, `GITHUB_*`, `RUNNER_*`, `ACTIONS_*`,
  `LD_*`, `BASH_*`, `GIT_*`, `SSL_*`, `DOCKER_*`, `CONTAINER*`, `XDG_*`, `REGISTRY_*`, `FCI_*`, `ITEM_*`, `*_PROXY`,
  `ENV`, `NODE_OPTIONS`, `PATH`, `HOME`, `USER`, `HOSTNAME`, `TMPDIR`, `SHELL`, `ALL_SECRETS`.
- A secret value must have no trimmed line under 4 characters.
- A status counts only if `creator.login` equals the token's user.
- Heartbeat channels (each only when configured): a log line with labels `repo="factory-ci"`, `project="."`, `step="heartbeat"`, `stream="stdout"`, sent from `plan` phase C only on ticks that fetched the log credentials; and one `GET` to `FACTORY_CI_HEARTBEAT_URL` (an `https://` capability URL held as a secret, redacted, never printed), sent by the `verdict` job on every tick.

## Additions from the critic review (`research.md` D12)

- Matrix blob: each final row, serialised as JSON, is encrypted by the plan wrapper
  (`openssl enc -aes-256-cbc -pbkdf2 -salt -a -A`, key `FACTORY_CI_BLOB_KEY`, or `FACTORY_CI_BLOB_KEY_DOCKER` for `allow_docker` rows; `-iter 600000`; keys at least 32 characters) and carried as `{n, blob}`.
- `fail_tick`: `1` iff the plan found problems or unchecked repos and the tick starts in the first 10 minutes of a UTC
  hour (A12); otherwise `0`.
- Retry marker: the suffix `" (retry)"` on a status description. Reasons compose as `reason`, then `" (retry)"`, then
  `" (logs not delivered)"`; the 140-character cut shortens the reason first and keeps both suffixes.
- `Branch`/default branch: `GET /repos/{r}` supplies `default_branch`, `id` and `private` once per entry.
- Idle ticks: no secret-store call. A job makes one list call, for its project (default or docker).
