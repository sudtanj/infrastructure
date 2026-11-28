# Contract: the release store

The store is a separate private repo with no code, no workflows and no tag rulesets that block creation; it starts with one README commit. Its `owner/name` is the runner setting `FACTORY_CI_STATE_REPO` (Bitwarden), never a file in the public repo. All calls use `FACTORY_CI_STATE_TOKEN` (scoped to that repo, contents write) as `Authorization: Bearer <token>`, header `Accept: application/vnd.github+json`. Hosts are fixed: `api.github.com` and `uploads.github.com`; `GITHUB_API_URL` is never read. Redirects are off on every call. Before any ticket, `GET /repos/<store>` must answer `private: true`.

Each call has a time limit (30 s; 300 s for an upload) and one retry on no answer or 5xx. A 403, a 429 or `x-ratelimit-remaining: 0` pauses all calls except renewals until `retry-after`, `x-ratelimit-reset` or 60 s, never longer than 120 s, with the Loki line `rate limited`. A renewal never pauses. Mutations are at least 1 s apart. The exit-time ticket delete has a 5 s limit and no retry. Every answer's `Date` header is parsed as server time.

## Tags

`<repo>` is the pipeline repo as `<owner>_<name>` (owner has no `_`, so the first `_` splits). `<project>` is the folder name, or `-` for the root project `.` (a folder name cannot be `-`). The delimiter `+` is outside every alphabet (owner `[A-Za-z0-9-]`, name `[A-Za-z0-9._-]`, project `[A-Za-z0-9._-]`, state `[A-Za-z0-9_-]`), so the mapping is injective; a test walks a table of colliding-looking inputs. A project that contains `..`, ends with `.` or `.lock`, or a tag over 200 bytes is `pipeline file refused` when `[state]` or `[artifacts]` is declared. The tag never comes from the pipeline file.

| Thing | Release tag |
|---|---|
| Ticket | `ci-queue+<repo>+<project>+<state>` |
| State | `ci-state+<repo>+<project>+<state>` |
| Artifact | `ci-artifacts+<repo>+<project>` |

Releases are `prerelease: true`, `draft: false`, `make_latest: "false"`, at the default branch head. Immutable releases must be off.

## Asset names (one regex per kind)

`<runid>` is `GITHUB_RUN_ID` when it is all digits, else a random decimal number. `<attempt>` is read from the runner's own container name `factory-ci-runner-<runid>-<attempt>`, else `GITHUB_RUN_ATTEMPT`, else `1` (plan D19). `<entry>` is always `0`. `<8hex>` is random.

| Kind | Regex | Label |
|---|---|---|
| Ticket | `^t-[0-9]+-[0-9]+-[0-9]+$` | `<runid>.<epoch>` matching `^[0-9]+\.[0-9]+$` |
| State | `^state-[0-9]+-[0-9]+-[0-9]+-[0-9a-f]{8}\.bin$` | none |
| Artifact | `^artifact-[0-9]+-[0-9]+-[0-9]+-[0-9a-f]{8}\.bin$` | none |

Only assets that match a regex of the kind being read count. A test shows that an asset written by a manual run (entry 0) restores.

## Calls

| Purpose | Call |
|---|---|
| Find release | `GET /repos/{o}/{r}/releases/tags/{tag}`; 404 means absent |
| Create release | `POST /repos/{o}/{r}/releases`; any 4xx means re-fetch by tag |
| List assets | `GET /repos/{o}/{r}/releases/{id}/assets?per_page=100&page=N`; follow pages; more than 300 assets fails closed |
| Upload | `POST https://uploads.github.com/repos/{o}/{r}/releases/{id}/assets?name=...&label=...`, `Content-Type: application/octet-stream`, explicit `Content-Length`; any 4xx means re-list and decide; after a timeout, delete a half-created asset of the own name and retry once |
| Renew | `PATCH /repos/{o}/{r}/releases/assets/{asset_id}` `{"label":"<runid>.<epoch>"}` |
| Delete | `DELETE /repos/{o}/{r}/releases/assets/{asset_id}`; 404 counts as gone |
| Download | check name and `size` first (state at most 10 MB plus overhead, artifact at most 100 MB plus overhead); `GET .../releases/assets/{asset_id}` with `Accept: application/octet-stream`, redirects off; the `Location` must be https on `*.githubusercontent.com`; `GET <Location>` with no `Authorization`; a counting reader stops at the bound; one retry on a 404 |

## What counts

- Only assets whose API `state` is `uploaded` count as stored (state, artifacts, tickets).
- A non-`uploaded` asset that matches the ticket regex and is younger than 120 s by `created_at` is a live ticket. A non-`uploaded` asset of any other kind is left alone until it is older than 360 s (the 300 s upload limit plus margin), then deleted.
- A non-`uploaded` ticket-pattern asset older than 120 s is stale and deleted; a non-`uploaded` asset of another kind older than 360 s is deleted, at the next listing.
- Retention counts only `uploaded` assets, so two failed uploads never evict the last good state.

## Ticket protocol

1. List. Delete every ticket whose label is missing, does not match the label regex, is more than 120 s ahead of this run's latest `Date`, or is 600 s or more behind it.
2. Upload the ticket `t-<runid>-<attempt>-<entry>` with label `<runid>.<epoch of the latest Date>`. On a 4xx, list again: an own live ticket means it succeeded; a foreign asset of the same name (a rerun) is deleted and the upload repeats once.
3. List again and note the position by id (Loki `queue registered`). There is no cap on tickets. A 5xx on registration is retried inside the wait budget.
4. Wait: poll every 20 s; the first poll that shows this ticket with the lowest id among live tickets starts a 3 s settle; list again; claim only if still lowest. Print `waiting for the queue` once on the first miss. The wait ends at the budget end minus 30 minutes (the budget end is the runner start plus 35 minutes when `FACTORY_CI_BUDGET_END` is unset), 5 minutes after the start: delete the ticket, `queue wait expired`.
5. Renew every 60 s from a thread, waiting or running. Each renewal lists again and logs `overlap` when a live ticket has a lower id. A failed renewal is a Loki line. When renewals have failed for 600 s the run sets the cancel flag in `RunExtras`, kills its step containers by their name prefix (`docker kill`), waits for them to exit, and only then deletes its ticket, prints `queue renewal failed` and exits 1.
6. A missing own ticket while waiting, after a 60 s grace for read lag, registers again at the back (accepted residual).
7. At the end, on every path, delete the ticket (5 s, no retry). A SIGINT or SIGTERM handler deletes the ticket first, within 5 s, skips the Loki flush and exits 1.

Fence: before a state upload and again after it, list and check the ticket still exists with a label starting with `<runid>.`. If it is gone after the upload, delete the new asset and end `state save failed`.

## Retention

State: after the new asset is `uploaded` and its size equals the ciphertext length, delete all but the 2 highest ids of `uploaded` state assets. Artifacts: after an upload, delete all but the 10 highest ids of `uploaded` artifact assets.

## Loki lines

See [loki-lines.md](loki-lines.md).
