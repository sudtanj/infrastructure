# Contract: the release store

All calls use `FACTORY_CI_STATE_TOKEN` as `Authorization: Bearer <token>`, header `Accept: application/vnd.github+json`, and the repo that owns the pipeline. Each call has a time limit (30 s; 300 s for an upload) and one retry on no answer, 429 or 5xx. Never `FACTORY_CI_TOKEN`; never a clone. Every answer's `Date` header is parsed as server time.

## Names

| Thing | Release tag | Asset name | Label |
|---|---|---|---|
| Ticket | `ci-queue-<state>` | `t-<runid>` | `<runid>.<epoch>` |
| State | `ci-state-<name>` | `state-<runid>-<8 hex>.bin` | none |
| Artifact | `ci-artifacts-<project>` | `artifact-<runid>-<8 hex>.bin` | none |

`<project>`: `.` becomes `root`; other dots become `_`. `<runid>` is `GITHUB_RUN_ID` when it is a valid name, else the random id. The tag is fixed by the repo and the validated name; the pipeline file never sets a tag.

Releases are created `prerelease: true`, `draft: false`, `make_latest: "false"`, at the default branch head. Immutable releases must be off.

## Calls

| Purpose | Call |
|---|---|
| Find release | `GET /repos/{o}/{r}/releases/tags/{tag}`; 404 means absent |
| Create release | `POST /repos/{o}/{r}/releases` `{tag_name, name, prerelease:true, make_latest:"false"}`; 422 `already_exists` means lost race, fetch again |
| List assets | `GET /repos/{o}/{r}/releases/{id}/assets?per_page=100` (one page; at most 13 assets exist) |
| Upload | `POST https://uploads.github.com/repos/{o}/{r}/releases/{id}/assets?name=...&label=...` with `Content-Type: application/octet-stream` and an explicit `Content-Length`; 422 `already_exists` is `Exists` |
| Renew | `PATCH /repos/{o}/{r}/releases/assets/{asset_id}` `{"label":"<runid>.<epoch>"}` |
| Delete | `DELETE /repos/{o}/{r}/releases/assets/{asset_id}`; 404 counts as gone |
| Download | `GET /repos/{o}/{r}/releases/assets/{asset_id}` with `Accept: application/octet-stream`, redirects off; then `GET <Location>` with no `Authorization` |

Only assets whose API `state` is `uploaded` count. A `starter` asset (a cut upload) is ignored and may be deleted by retention.

## Ticket protocol

1. List. Delete every ticket whose epoch is 600 s or more behind this run's latest `Date`. Count live tickets; at 3 or more, end with `queue is full` and register nothing.
2. Upload `t-<runid>` with label `<runid>.<epoch of the latest Date>`. On `Exists`, delete the old asset and upload again (a rerun of one run id).
3. List again. If this ticket's position by id is above 3, delete it and end with `queue is full`.
4. Wait: poll every 20 s; the first poll that shows this ticket with the lowest id among live tickets starts a 3 s settle; list again; claim only if still lowest. Print `waiting for the queue` once on the first miss.
5. Renew every 60 s from a thread, waiting or running. A failed renewal is a Loki line, never a stop.
6. A missing own ticket while waiting, after a 60 s grace for read lag, registers again.
7. Give up after 120 minutes: delete the ticket, `queue wait timed out`.
8. At the end, on every path, delete the ticket.

Fence: before a state upload and again after it, list and check the ticket still exists with a label starting with `<runid>.`. If it is gone after the upload, delete the new asset and end `state save failed`.

## Retention

State: after the new asset is `uploaded` and its size equals the ciphertext length, delete all but the 2 highest ids. Artifacts: after an upload, delete all but the 10 highest ids of `artifact-*`.

## Loki lines (step `ci`)

Registration, position, stale breaks, renewal failures, restore result, save result, release and asset names for an upload (FR-029), skipped paths, bundle size. Never a token, key, vault name or value.
