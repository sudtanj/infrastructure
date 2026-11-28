# Contract: the release store

The store is a separate private repo with no code and no workflows. Its `owner/name` is the runner setting `FACTORY_CI_STATE_REPO` (Bitwarden), never a file in the public repo. All calls use `FACTORY_CI_STATE_TOKEN` (scoped to that repo, contents write) as `Authorization: Bearer <token>`, header `Accept: application/vnd.github+json`. Hosts are fixed: `api.github.com` and `uploads.github.com`; `GITHUB_API_URL` is never read. Redirects are off on every call. Before any ticket, `GET /repos/<store>` must answer `private: true`.

Each call has a time limit (30 s; 300 s for an upload) and one retry on no answer or 5xx. A 403, a 429 or `x-ratelimit-remaining: 0` pauses all store calls (polls and renewals too) until `retry-after`, `x-ratelimit-reset` or 60 s, whichever the answer gives, with the Loki line `rate limited`. Mutations are at least 1 s apart. The exit-time ticket delete has a 5 s limit and no retry. Every answer's `Date` header is parsed as server time.

## Names

`<repo>` is the pipeline repo as `<owner>_<name>`. `<project>` is `root` for `.`, other dots become `_`. A tag over 200 bytes is `pipeline file refused`. The tag never comes from the pipeline file.

| Thing | Release tag | Asset name | Label |
|---|---|---|---|
| Ticket | `ci-queue-<repo>-<project>-<state>` | `t-<runid>-<attempt>-<entry>` | `<runid>.<epoch>` |
| Slot marker | `ci-state-<repo>-<project>-<state>` | `slot-<epoch>-<entry>` | none |
| State | `ci-state-<repo>-<project>-<state>` | `state-<runid>-<attempt>-<entry>-<8 hex>.bin` | none |
| Artifact | `ci-artifacts-<repo>-<project>` | `artifact-<runid>-<attempt>-<entry>-<8 hex>.bin` | none |

`<runid>` is `GITHUB_RUN_ID` when it is a valid name, else the random id. `<attempt>` is `GITHUB_RUN_ATTEMPT` (`0` when unset). `<entry>` is the schedule index, `-` for a manual run. Releases are `prerelease: true`, `draft: false`, `make_latest: "false"`, at the default branch head. Immutable releases must be off.

## Calls

| Purpose | Call |
|---|---|
| Find release | `GET /repos/{o}/{r}/releases/tags/{tag}`; 404 means absent |
| Create release | `POST /repos/{o}/{r}/releases`; any 4xx means re-fetch by tag |
| List assets | `GET /repos/{o}/{r}/releases/{id}/assets?per_page=100&page=N`; follow pages; more than 300 assets fails closed |
| Upload | `POST https://uploads.github.com/repos/{o}/{r}/releases/{id}/assets?name=...&label=...`, `Content-Type: application/octet-stream`, explicit `Content-Length`; any 4xx means re-list and decide |
| Renew | `PATCH /repos/{o}/{r}/releases/assets/{asset_id}` `{"label":"<runid>.<epoch>"}` |
| Delete | `DELETE /repos/{o}/{r}/releases/assets/{asset_id}`; 404 counts as gone |
| Download | check name and `size` first (state at most 10 MB plus overhead, artifact at most 100 MB plus overhead); `GET .../releases/assets/{asset_id}` with `Accept: application/octet-stream`, redirects off; the `Location` must be https on `*.githubusercontent.com`; `GET <Location>` with no `Authorization`; a counting reader stops at the bound |

Only assets whose API `state` is `uploaded` count, with one exception: a ticket in state `starter` younger than 120 s by `created_at` counts as live. Asset names must match their pattern; others are ignored (tickets) or make the listing a store fault (state, artifacts) only when they would be chosen.

## Ticket protocol

1. List. A ticket counts only if its name matches `t-[0-9]+-[0-9]+-(-|[0-9]+)`. Delete every ticket whose label is missing, malformed, more than 120 s ahead of this run's latest `Date`, or 600 s or more behind it. Count live tickets; at 3 or more, end with `queue is full` and register nothing.
2. For a scheduled run, create the slot marker. A refusal means another run took the slot: Loki `slot taken`, exit 0.
3. Upload the ticket with label `<runid>.<epoch of the latest Date>`. On a 4xx, list again: an own ticket means it succeeded; a foreign one with the same name (a rerun) is deleted and the upload repeats once.
4. List again. If this ticket's position by id is above 3, delete it and end with `queue is full`.
5. Wait: poll every 20 s; the first poll that shows this ticket with the lowest id among live tickets starts a 3 s settle; list again; claim only if still lowest. Print `waiting for the queue` once on the first miss. The wait is capped at `min(120 min, FACTORY_CI_BUDGET_END - now - 45 min)`.
6. Renew every 60 s from a thread, waiting or running; each renewal lists again and logs `overlap` when a live ticket has a lower id. A failed renewal is a Loki line, never a stop.
7. A missing own ticket while waiting, after a 60 s grace for read lag, registers again.
8. At the cap: delete the ticket, `queue wait timed out`.
9. At the end, on every path, delete the ticket (5 s, no retry).

Fence: before a state upload and again after it, list and check the ticket still exists with a label starting with `<runid>.`. If it is gone after the upload, delete the new asset and end `state save failed`.

## Retention

State: after the new asset is `uploaded` and its size equals the ciphertext length, delete all but the 2 highest ids. Artifacts: after an upload, delete all but the 10 highest ids of `artifact-*`. Slot markers older than 7 days (by the epoch in the name) are deleted at the same time.

## Loki lines

See [loki-lines.md](loki-lines.md).
