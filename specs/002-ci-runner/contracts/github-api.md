# Contract: GitHub REST calls

Base `GITHUB_API_URL` (default `https://api.github.com`). Headers on every call:
`Authorization: Bearer <FACTORY_CI_TOKEN>`, `Accept: application/vnd.github+json`,
`X-GitHub-Api-Version: 2022-11-28`, `User-Agent: rust-ci-runner`. Timeout 20 s. A 5xx or a
network error is retried twice, after 1 s and 2 s. Lists use `per_page=100` and `page=N` until a
page has fewer than 100 items, at most 5 pages.

| Purpose | Request | Used fields |
|---|---|---|
| Branches | `GET /repos/{r}/branches?per_page=100` | `name`, `commit.sha` |
| Tree | `GET /repos/{r}/git/trees/{sha}?recursive=1` | `tree[].path`, `tree[].type`, `truncated`. When `truncated` is true: `GET .../git/trees/{sha}` (root, non-recursive), then one `GET .../git/trees/{folder sha}` per top-level tree entry, to find `<folder>/.factory-ci.toml` |
| Statuses of a commit | `GET /repos/{r}/commits/{sha}/status?per_page=100&page=N` | `statuses[].context`, `.state`, `.updated_at`, `.description`, `.creator.login` (latest per context); paged until a page is short or `total_count` is reached, at most 5 pages |
| Commit window | `GET /repos/{r}/commits?sha={tip}&per_page=100` | `sha`, `parents[].sha`, `commit.committer.date` |
| Changed files | `GET /repos/{r}/compare/{base}...{head}?per_page=100&page=N` | `files[].filename`, `.previous_filename`; 3 pages at most, then "unknown" |
| Post status | `POST /repos/{r}/statuses/{sha}` body `{state, context, description, target_url?}` | 201 expected |

Error classes returned by `HttpApi`:

- `RateLimited(msg)`: 429; 403 with `x-ratelimit-remaining: 0`, a `retry-after` header, or a body
  containing "rate limit"; or the local reserve rule (`research.md` D3).
- `Http(code, msg)`: any other 4xx or exhausted 5xx. 404 and 403 mean "repo unreachable".
- `Net(msg)`: transport failure after retries.

100+ matching branches, a sha that is not 40 lowercase hex, and a
compare answer that is still full after 3 pages are handled by the caller as described in
`data-model.md`. Error text never contains the token.


## Additions from the review (`research.md` D10, D12)

| Purpose | Request | Used fields |
|---|---|---|
| Token's user (M1) | `GET /user` once per token per plan | `login` |
| Repo identity, visibility, default branch | `GET /repos/{r}` once per entry per plan | `id`, `private`, `default_branch` |

- Statuses: `statuses[].creator.login` must equal the token's `login`; any other status is ignored, as if absent.
  If `GET /user` fails, the repo is a problem (fail closed).
- Token choice: calls for an `allow_docker` repo use `FACTORY_CI_TOKEN_DOCKER`; all others use `FACTORY_CI_TOKEN`.
- Writes: spaced at least 750 ms apart, at most 60 per tick. A 403 whose body or headers name a secondary or abuse
  limit, or that has `retry-after`, is `RateLimited`; on a rate-limited write the writer waits 30 s once, then defers the remaining writes to the next tick.
- The retry of an old `error` (A6c) needs no extra call: the description of the latest status carries the marker
  " (retry)" (`data-model.md` Decision per project).
- The idle call count for the reference fleet is 120 (110 plus one repo call per entry). A truncated tree adds one
  call per top-level folder for that tip only.

Review round 1 (R9): the branch list is paged like the others, but a list still full after 5 pages (more than 500
branches) is an error, shown as a repo problem, not a silent cut. A failed final status post reports only "final
status not delivered" and the HTTP code, never the request path (R4).
