# Quickstart: Central Interval CI Runner

Setup and the checks the owner runs once Actions are enabled. The checks need a live GitHub and are the recorded gap
of the constitution check; the tests cover the logic behind each.

## B is a separate public copy

B is a public repository and the scheduler fires every 5 minutes (A25). Use a separate public copy of this repo,
never the monorepo itself (A1). An idle tick bills about 2 minutes, which a public repo does not pay for. A private B
would bill about 17,000 minutes a month and is not supported.

- Everything B prints is public. Job names show only `item N`, the matrix carries an encrypted blob, and the tick
  summary lists repo names only (FR-058, FR-059). The watch list and the workflow files are public too: keep nothing
  secret in them. Repo names in the watch list and the summary are public by decision.
- Step output of private watched repos goes only to your log store. A private repo needs log shipping; the plan
  refuses it otherwise.
- The first compile (the crate has never been built) needs this public copy with Actions enabled, or a scratch public
  repo. This guide does not create either.

## Setup in B

1. Enable Actions (Settings, Actions, General). Set run retention to 7 to 14 days.
2. Protect `main` with a ruleset: pull request required, no force push, no deletion.
3. Create three Environments, each with deployment branches limited to `main`:
   `factory-ci-plan`, `factory-ci`, `factory-ci-docker`.
4. Tag `rust-ci-runner-v0.2.0`. The release workflow prints the runner image digest and its previous digest. Make the
   GHCR packages public. Run the manual workflow `factory-ci-mirror-images` once. Set the repository variables
   `RUNNER_IMAGE_DIGEST` (`sha256:<64 hex>`, no `@`), `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE`
   (`...@sha256:<64 hex>`) from the printed digests.
5. Tokens (all fine-grained, never classic):
   - `FACTORY_CI_TOKEN`: the repos without `allow_docker`; Contents read, Commit statuses read and write, Metadata
     read; 90-day expiry. `FACTORY_CI_TOKEN_DOCKER`: the same for `allow_docker` repos only.
   - Caller token: B only, Actions write, 90-day expiry. It lives in the scheduler, not in B.
6. Bootstrap secrets only (Environment secrets, no pipeline secret goes into GitHub):
   - Generate two blob keys, each at least 32 characters: `openssl rand -base64 33`, once for plain rows and once for
     docker rows. The guard step refuses a shorter key.
   - `factory-ci-plan`: both repo tokens, `BWS_ACCESS_TOKEN`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY`,
     `FACTORY_CI_BLOB_KEY_DOCKER`, `FACTORY_CI_HEARTBEAT_URL` (optional).
   - `factory-ci`: `FACTORY_CI_TOKEN`, `BWS_ACCESS_TOKEN`, `FACTORY_CI_BLOB_KEY` (never the docker key).
   - `factory-ci-docker`: `FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY_DOCKER` (never the
     plain key).
7. Bitwarden Secrets Manager (free plan: cloud only, 3 projects, 3 machine accounts; this uses 2 and 2):
   - Projects: the default project, and the docker project if any repo has `allow_docker`.
   - Machine accounts, read-only ("Can read"): one on the default project; one on the docker project only. Create an
     access token for each.
   - Keys in the default project: `owner/name<SEP>NAME` for a repo's own secrets (the entry must record `id`), `NAME`
     for shared keys an entry lists, and the three log keys `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`,
     `FACTORY_CI_LOG_TOKEN`. The docker project holds the docker repos' prefixed and shared keys and a copy of the
     three log keys. `SEP` is `/`; check that Bitwarden accepts it, else set `FCI_KEY_SEP` to `:` (never `__`).
   - Variables: `FACTORY_CI_BWS_PROJECT`, `FACTORY_CI_BWS_PROJECT_DOCKER`; for a non-US account
     `FACTORY_CI_BWS_SERVER_URL`.
8. Grafana Cloud (free: 50 GB a month, 14-day retention): create an access policy with the scope `logs:write` only and
   a token for it. The user name is the Loki instance's user ID (a number). The URL is the base, such as
   `https://logs-prod-XXX.grafana.net`, with no path. Put the URL, user and token into the Bitwarden keys of step 7.
   Set `FACTORY_CI_LOG_LINK` (a variable) to an Explore URL containing `{run_id}`. Status links stop working after
   14 days. Add a usage alert at 70% of 50 GB.
9. Edit `.github/factory-ci-repos.toml`. Start with the default branch only:

   ```toml
   [[watch]]
   repo = "you/scratch"
   id = 123456789
   branches = ["main"]
   ```

   `secret_branches` defaults to the repo's default branch, so secrets are available only there. Check the file with
   `rust-ci-runner plan --repos .github/factory-ci-repos.toml --check`. The self-test runs the same check on pull
   requests.

## Alerts

- Ping service (the dead-man signal): create a check at a service such as healthchecks.io with a period of 5 minutes
  and a grace longer than your longest tick (20 slow items can take about 3 hours; use 3 hours, or accept a late
  alert on long ticks). Store its HTTPS ping URL as the secret `FACTORY_CI_HEARTBEAT_URL` in `factory-ci-plan`. The
  `verdict` job pings it at the end of every tick, even idle ones.
- The Loki heartbeat line is sent only on ticks that have work (idle ticks make no Bitwarden call), so do not alert on
  its absence.
- cron-job.org: turn on failure notifications for the job.
- Raspberry Pi: put the token in a 0600 file containing `Authorization: Bearer <token>` and use
  `*/5 * * * * flock -n /var/lock/fci.lock curl -fsS -X POST -H @/home/pi/.fci-header -H "Accept: application/vnd.github+json" -H "X-GitHub-Api-Version: 2022-11-28" -H "User-Agent: factory-ci-scheduler" https://api.github.com/repos/OWNER/B/actions/workflows/factory-ci-tick.yml/dispatches -d '{"ref":"main"}' || logger -t factory-ci "dispatch failed"`.
- A persistent repo error fails a tick only when it starts in the first 10 minutes of a UTC hour (A12), so you get at
  most about two failure mails an hour. The tick summary lists the error on every tick.

## Token expiry and rotation

Fine-grained tokens expire (90 days here) and are bound to one owner. A tick with an expired token fails with a clear
reason and posts nothing. Rotate before day 80: create the new token, replace the Environment secret (and the
scheduler's header file), dispatch one tick by hand, then delete the old token. Rotate Bitwarden access tokens, the
blob key and the Grafana token the same way, with the same one-tick test.

## Run a tick

Use the `curl` of the contracts (`contracts/tick-workflow.md`), method POST, body `{"ref":"main"}`.

## Checks

1. **First status (US1, SC-001)**: scratch repo with `.factory-ci.toml` at the root. After one tick the tip has
   `factory-ci` pending, then success. With shipping on, the public Actions log shows only `item N` and no step
   output; for a private repo it shows nothing else.
2. **Unchanged (US2, SC-002)**: add `other/.factory-ci.toml`, change only `other/`. The root context shows "unchanged
   since <sha>" and only `factory-ci/other` ran.
3. **Failure (US1.2)**: push a failing step. The status is `failure` and names the step. A step whose image cannot be
   pulled ends `error`, not `failure`.
4. **Idle tick (SC-005)**: tick again with no push. No status is written and no Bitwarden call is made. Compare
   `gh api rate_limit` before and after: the difference is the plan's reads, 120 for the reference fleet.
5. **Overlap (US5, SC-003)**: dispatch twice 10 seconds apart on a slow project. One run; the second tick waits.
6. **Bad repo (US6, SC-004)**: add `you/does-not-exist`. The other repo still gets its statuses, the summary names the
   bad repo, and the tick ends red only if it started in the first 10 minutes of the hour. Change an entry's `id` to a
   wrong number: that repo is skipped, both ids named.
7. **No secret store (US12, FR-043)**: with `BWS_ACCESS_TOKEN` unset, a pipeline that lists a secret ends `error` "no
   secret store configured"; a pipeline with no secret runs. No GitHub secret is ever read.
8. **Bitwarden (US12)**: set `BWS_ACCESS_TOKEN` and `FACTORY_CI_BWS_PROJECT`. In the default project create
   `you/scratch<SEP>DEPLOY_KEY`; list `DEPLOY_KEY` in one step: that step sees it, a sibling does not. A key only under
   `you/other<SEP>DEPLOY_KEY` ends the item `error`. Push to a non-default branch: the pipeline that lists a secret ends
   `error` "secrets not allowed on this branch". Break the pipeline: `error` with that reason and no store call.
   Revoke the token: the next tick with work fails before any status is posted ("secret store rejected the token").
   Create a key whose value has a line under 4 characters: the item ends `error` naming the secret only. Check the
   limits: 2 projects and 2 machine accounts in use. Rollback: remove the token; pipelines without secrets still run.
9. **Docker (H1)**: set `allow_docker = true` on a second scratch repo with a docker step. It runs in `run-docker` with
   the docker account, cannot read the default project, and a plain-repo job has no docker token.
10. **Logs (US11, SC-010, SC-011)**: with shipping on, a printed line is searchable within 30 seconds. With a wrong
    password, statuses end " (logs not delivered)" and results are unchanged. On a public B with a private repo and
    shipping off, the repo is refused and the tick reports a problem.
11. **Ping (D6, FR-056)**: stop dispatching until the grace passes. The ping service alerts.
12. **Retry of an old error (A6c)**: cause a runner fault (a wrong clone image variable, then fix it). The commit shows
    `error`; after 30 minutes the next tick runs it once more, marked " (retry)". A second `error` stays red.
13. **Public log grep (FR-059, blocker check)**: open a real run's log in a signed-out browser, and grep the text of the
    plan, run and verdict jobs for a branch name, a commit SHA, a project path, a secret name and a run ID. None may
    appear. Also check the matrix output in the run summary shows only `n` and ciphertext.
14. **Kill, branch guard and metadata block (SC-008, H2, FR-054)**: cancel a run mid-job: the context ends `error` "job
    did not finish" within 5 minutes. Dispatch with `"ref":"some-branch"`: no job receives a secret and the tick ends
    red. From a step, `curl -m 3 http://169.254.169.254/` fails. Also check Environments with `workflow_dispatch` behave
    as expected (this is unverified).

Local run (US8): `rust-ci-runner run --dir path/to/project [STEP]` with Docker or Podman.
