# Contract: Docker Hub API calls of the publish step

Made by the runner process with `ureq` (already a dependency), from `rust-ci-runner/src/hub.rs`. No child process, no argv, no environment variable for the token: the login body is the only request that carries it, and the access token it returns is kept in memory for the next two calls. The base is a function argument: `ensure_public(base, user, token, namespace, name, limit)`, where `limit` is the time left of the shared `JOB_DEADLINE` (never `Duration::MAX`; an overflowing sum is clamped, not a panic); production passes `HUB_BASE` (`https://hub.docker.com`). The runner refuses a base that is not `https://` or `http://127.0.0.1`. Binary-level tests need to point the runner at a fake, so `job.rs` reads the override variable `FACTORY_CI_HUB_URL` only under `#[cfg(debug_assertions)]`; a release binary never reads it, and the workflow never sets it. Every call has a 30-second timeout (the test of that timeout stays and runs in its own `cargo test --lib hub_tests::` command, see plan S5a); each call gets `min(30 seconds, time left)`, and with no time left `ensure_public` refuses with `publish refused: time ran out` before it makes the call. The login endpoint (`/v2/auth/token`, or the fallback `/v2/users/login`) is pending T002.

`ensure_public(base, user, token, namespace, name, limit) -> Result<(), &'static str>`

## Calls

| Step | Request | Answer used |
|---|---|---|
| login | `POST /v2/auth/token`, JSON `{"identifier": <user>, "secret": <token>}` | status; on 200 only the `access_token` string |
| probe | `GET /v2/repositories/<namespace>/<name>/`, `Authorization: Bearer <access_token>` | status; on 200 only the boolean `is_private` |
| create | `POST /v2/repositories/`, same header, JSON `{"namespace": <namespace>, "name": <name>, "is_private": false}` | status only |

T002 decides the login endpoint: if `/v2/auth/token` does not accept a personal access token as `secret`, the fallback is `POST /v2/users/login` with `{"username": <user>, "password": <token>}` and the `token` field of the answer. T002 also records whether a Read & Write token may create a repository; if a broader scope is needed, that is an owner item (spec R13), not a workaround.

## Mapping (status codes only; no body ever reaches a log, status or error)

| Situation | Result |
|---|---|
| login 200 | continue |
| login 401 | `registry refused the token` |
| login any other status, network error, timeout, body without `access_token` | `docker hub api error` |
| probe 200, `is_private` false | `Ok` (nothing created, nothing edited) |
| probe 200, `is_private` true | `publish refused: repository is private` |
| probe 404 (after a valid login) | missing: create |
| probe 401 or 403 (after a valid login) | `docker hub api refused repository access` (a scope problem, never "bad token") |
| probe any other status | `docker hub api error` |
| create 201 | probe again; `is_private` false gives `Ok`; true gives `publish refused: repository is private`; 404 gives `docker hub api error` |
| create 400 or 409 | someone created it meanwhile: probe again as above |
| create 401 or 403 | `docker hub api refused repository creation` |
| create any other status | `docker hub api error` |

## Rules

- The runner never calls an endpoint that changes visibility, deletes, or edits a README.
- The login name is the value of `DOCKERHUB_USERNAME`, already checked equal to `NAMESPACE` by `publish`; it is not registered with a redactor.
- Neither the token nor the access token appears in a `format!` that is returned or logged. `ensure_public` returns only `Ok` or a fixed line, so the access token never leaves `hub.rs` and no redactor holds it; the redactor passed to the final post holds the token and its base64.
- A repeat run finds the repository and does nothing here (idempotent).
