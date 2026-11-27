# infrastructure

sudtanj's open-source infrastructure and DevOps tools, MIT licensed. Today the repo holds the factory CI runner (`rust-ci-runner`) and every GitHub Actions workflow for sudtanj's repositories. The runner is a Rust binary. A scheduled tick finds new commits in the repos listed in `.github/factory-ci-repos.toml`, runs the steps each repo declares in its `.factory-ci.toml` inside containers, and posts commit statuses. Logs go to Grafana Loki.

Docs:

- `specs/003-infrastructure-home/`: the spec, plan, tasks, data model and quickstart for this setup.
- `rust-ci-runner/README.md`: how the runner works, its commands, the pipeline format and its limits.
- `specs/002-ci-runner/`: the spec, plan, tasks and quickstart of the runner.
- `.specify/memory/constitution.md`: the rules for changing this repo. Every change goes through Spec Kit.

## Setup

Do these steps once, in order. Each step needs the ones before it. You enter every value in the GitHub or vendor web UI. Never commit a value.

### Workflow rules

Only `factory-ci-selftest` and `rust-ci-runner-release` run cargo directly. This is the bootstrap exception: no runner exists yet to build the first one, so no other workflow may run cargo. The header of each of those two workflows says the same.

### 1. Repository rules

In **Settings** for this repo:

1. Create a `main` ruleset. Require a pull request with 0 approvals, and require the jobs of `factory-ci-selftest` as the only required status checks. Block force pushes and deletion. Leave bypass empty. Do not add a code-owner review rule: GitHub never lets an author approve their own pull request, and only the owner has write access, so only the owner can merge. The selftest is the gate.
2. Create a tag ruleset that limits who can create `rust-ci-runner-v*` tags to you.
3. Under **Actions > General**, require approval for all outside collaborators, set the default workflow token to read-only, and set run retention to 7 to 14 days.

### 2. Environments

Create four Environments under **Settings > Environments**. For each one, set **Deployment branches** to *Selected branches* and add the pattern in the table. Do not let administrators bypass the environment rules.

| Environment | Deployment branches | Used by |
| --- | --- | --- |
| `factory-ci-plan` | `main` | the `plan` and `verdict` jobs of `factory-ci-tick` |
| `factory-ci` | `main` | the `run` job |
| `factory-ci-docker` | `main` | the `run-docker` job, for repos with `allow_docker` |
| `factory-ci-release` | `main` and `rust-ci-runner-v*` | the release `publish` job and the image mirror; holds no secret |

Create the Environments before you add secrets. If a tick runs first, GitHub creates empty Environments with no branch rule. The plan then stops at its check for the blob key. Restrict each Environment as above, then add its secrets.

### 3. Accounts and tokens

Create these before step 4.

- **Bitwarden Secrets Manager.** Create two machine accounts: one for the default project and one for the docker project. Give each account *Can read* on its project only, and set a token expiry. The free plan allows 3 projects and 3 machine accounts. This setup uses 2 of each.
- **Grafana Cloud Loki.** Create two access policies that hold `logs:write` only, with one token each. Use the first token for `factory-ci-plan` and `factory-ci`. Use the second for `factory-ci-docker`, so you can cut off the docker Environment alone. Add a usage alert on ingest.
- **`FACTORY_CI_TOKEN`:** one fine-grained token for `sudtanj/factory-monorepo` and `sudtanj/infrastructure` only. Permissions: Contents read, Commit statuses read and write, Metadata read. Expiry 90 days; rotate before day 80. Create `FACTORY_CI_TOKEN_DOCKER` the same way only if a watched repo sets `allow_docker = true`.
- **Scheduler token.** One fine-grained token for `sudtanj/infrastructure` only, with Actions write. It goes into the scheduler (step 10), not into GitHub.
- **Blob keys.** Generate each key with `openssl rand -hex 32`. Each key must have 32 characters or more. `FACTORY_CI_BLOB_KEY_DOCKER` must differ from `FACTORY_CI_BLOB_KEY`.

Rotate the tokens and blob keys together, before day 80.

### 4. Environment secrets

Add each secret to the Environments marked `yes`, under **Settings > Environments > (name) > Environment secrets**. Use these names exactly.

| Name | `factory-ci-plan` | `factory-ci` | `factory-ci-docker` | Notes |
| --- | --- | --- | --- | --- |
| `FACTORY_CI_TOKEN` | yes | yes | no | GitHub token, step 3 |
| `FACTORY_CI_TOKEN_DOCKER` | yes | no | yes | Only with `allow_docker` repos |
| `BWS_ACCESS_TOKEN` | yes | yes | no | Default Bitwarden project; needed only for pipelines with secrets |
| `BWS_ACCESS_TOKEN_DOCKER` | yes | no | yes | Docker Bitwarden project; only with `allow_docker` repos |
| `FACTORY_CI_BLOB_KEY` | yes | yes | no | `openssl rand -hex 32`, 32 characters or more |
| `FACTORY_CI_BLOB_KEY_DOCKER` | yes | no | yes | Optional: only with `allow_docker` repos; a different value from `FACTORY_CI_BLOB_KEY` |
| `FACTORY_CI_HEARTBEAT_URL` | yes | no | no | Optional: the heartbeat ping URL, step 11 |
| `FACTORY_CI_LOG_URL` | yes | yes | yes | Grafana Cloud Loki base URL |
| `FACTORY_CI_LOG_USER` | yes | yes | yes | Loki instance user ID |
| `FACTORY_CI_LOG_TOKEN` | yes | yes | yes | Loki token; `factory-ci-docker` holds its own, separate token |

Pipeline secrets never go into GitHub. They stay in Bitwarden. `GITHUB_TOKEN` comes from GitHub, so do not set it.

### 5. Merge

Merge the pull request into `main` once every job of `factory-ci-selftest` is green.

### 6. Release the runner

Push a tag `rust-ci-runner-vX.Y.Z` on a commit on `main`. The tag must match `version` in `rust-ci-runner/Cargo.toml`. The release workflow pushes the image to `ghcr.io/sudtanj/rust-ci-runner` and prints its digest in the run summary. Check that the run ran on `main`.

### 7. Make the GHCR packages public

The first push creates a private package, and the tick cannot pull a private package. Open the package `rust-ci-runner` in GHCR, change its visibility to public, then check it:

```sh
gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility
```

The output must be `public`. Repeat this for the two mirror packages in step 9.

### 8. Set the repository variables

Under **Settings > Secrets and variables > Actions > Variables**, add these variables.

| Name | Scope | Set when |
| --- | --- | --- |
| `RUNNER_IMAGE_DIGEST` | repository | After step 6: `sha256:<64 hex>`, the digest alone, without the image name |
| `FACTORY_CI_CLONE_IMAGE` | repository | After step 9 |
| `FACTORY_CI_DIND_IMAGE` | repository | After step 9 |
| `FACTORY_CI_BWS_PROJECT` | repository | After Bitwarden setup; for pipelines with secrets |
| `FACTORY_CI_BWS_PROJECT_DOCKER` | repository | Only with `allow_docker` repos |
| `FACTORY_CI_BWS_SERVER_URL` | repository | Only outside the US cloud; must start with `https://` |
| `FACTORY_CI_LOG_LINK` | repository | Optional: an Explore URL with `{run_id}`. Without it, the status links to the Actions run |

### 9. Mirror the clone and docker images

Run the workflow `factory-ci-mirror-images` from the Actions tab, on the branch `main`. Its run summary has two lines in the form `FACTORY_CI_CLONE_IMAGE = ghcr.io/<owner>/alpine-git@sha256:<64 hex>`. Copy the value after `=` into the variable named before it, for both lines. Make the `alpine-git` and `docker-dind` packages public, as in step 7. If the mirror stops on a Docker Hub rate limit, run it again later.

### 10. Set up the scheduler

Create a cron-job.org job that runs every 5 minutes and sends a POST request to this URL:

```text
https://api.github.com/repos/sudtanj/infrastructure/actions/workflows/factory-ci-tick.yml/dispatches
```

Send these headers:

```text
Accept: application/vnd.github+json
Authorization: Bearer <scheduler token>
X-GitHub-Api-Version: 2022-11-28
```

Send this body:

```json
{"ref":"main"}
```

The request returns 204. Turn on failure notifications. Set a reminder for day 75 to renew the scheduler token.

### 11. Set up the heartbeat

Create a healthchecks.io check with a period of 5 minutes and a grace of 60 minutes. Store its ping URL as `FACTORY_CI_HEARTBEAT_URL` in `factory-ci-plan`. Without it, nothing alerts you when ticks stop.

The `verdict` job pings this URL after every tick on `main`. A red tick or a failed plan pings the heartbeat `/fail` URL and alerts at once. A late check means ticks stopped.

### 12. Run the first tick by hand

Run `factory-ci-tick` from the Actions tab, on the branch `main`. Every job passes and `verdict` is green. The tip of `main` in `sudtanj/infrastructure` then shows the statuses `factory-ci` and `factory-ci/rust-ci-runner`, both `success`.

If a step fails, read the `plan` step first. It names the problem.

| Symptom | Fix |
| --- | --- |
| `the runner image is not pullable; make the GHCR package public` | Step 7 |
| `log credentials incomplete` | Set all three `FACTORY_CI_LOG_*` secrets in the Environment, or remove all three. A partial set stops the tick. |
| `no secret store configured` | A pipeline lists a secret, but `BWS_ACCESS_TOKEN` is not set in that job's Environment. Set it, or remove the secret from the pipeline. |
| `secrets not allowed on this branch` | The branch is not in the watch entry's `secret_branches`. Add it, or remove the secret from the step. |
| `FACTORY_CI_BWS_SERVER_URL must start with https://` | Use an `https://` URL, or unset the variable inside the US cloud. |
| The plan stops at its check for the blob key | `FACTORY_CI_BLOB_KEY` is missing or shorter than 32 characters in `factory-ci-plan`. |

### 13. Check that no bootstrap secret sits at repo level

Run `gh secret list`. It must show none of the bootstrap names in step 4. If one shows, delete it: bootstrap secrets belong in Environments only. Add `--org <org>` for any organisation that owns this repo.

### Watched repos without `.factory-ci.toml`

A watched repo without `.factory-ci.toml` gets nothing: no status, no job and no error. `sudtanj/factory-monorepo` gets nothing until its root `.factory-ci.toml` is on `main`.

### Delete the old log keys last

The runner no longer reads `FACTORY_CI_LOG_*` from Bitwarden. Keep the three old keys there until quickstart Part 3 checks 1 to 5 and 8 pass (`specs/003-infrastructure-home/quickstart.md`) and several days of ticks have been green. Then delete `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` from Bitwarden.

### Rollback

- Pause the scheduler first: disable the cron-job.org job, so no tick starts.
- Revert the lock file and `--locked` together, in one commit. Reverting one without the other breaks the build.
- Write down the current `RUNNER_IMAGE_DIGEST` before you change it, so you can put it back.
- Rollback does not restore Environment secrets, the public package, Bitwarden contents, the scheduler token or the Grafana policy.

### Notes

- GitHub retires runner images. Watch for `ubuntu-24.04` deprecation notices, and move the pins when one arrives.
- Docker Hub limits anonymous pulls. Run the mirror again after a rate limit.
- This repo is public, so its ticks cost no Actions minutes. A private repo would bill about 17,000 minutes a month for idle ticks. If you make it private, lengthen the tick period.

## License

MIT, see `LICENSE`.
