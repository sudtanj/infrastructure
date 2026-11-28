# infrastructure

The factory CI runner (`rust-ci-runner`) and every GitHub Actions workflow for sudtanj's repositories. The runner is a Rust binary. A scheduled tick finds new commits in the repos listed in `.github/factory-ci-repos.toml`, runs the steps each repo declares in its `.factory-ci.toml` inside containers, and posts commit statuses. Logs go to Grafana Loki. Folders under `docker/` become public multi-arch Docker Hub images.

Docs:

- `rust-ci-runner/README.md`: how the runner works, setup, release, limits.
- `docker/README.md`: how to add a public Docker image and refresh its pins.
- `specs/002-ci-runner/`: the spec, plan, tasks and quickstart.
- `specs/012-docker-images/`: the spec, plan, tasks and pins of the Docker image publishing.
- `.specify/memory/constitution.md`: the rules for changing this repo. Every change goes through Spec Kit.

## Setup

Bootstrap secrets and variables live in this repo's GitHub Actions settings. Pipeline secrets live in Bitwarden, never in GitHub. Do not commit a value. `rust-ci-runner/README.md` ("Set up B") says which Environment holds which secret: `factory-ci-plan`, `factory-ci` and `factory-ci-docker`, all limited to `main`.

Secrets:

| Name | Use |
| --- | --- |
| `BWS_ACCESS_TOKEN` | Bitwarden Secrets Manager access token for pipeline secrets |
| `BWS_ACCESS_TOKEN_DOCKER` | Same, for repos with `allow_docker` |
| `FACTORY_CI_TOKEN` | Fine-grained GitHub token: read code, write commit statuses |
| `FACTORY_CI_TOKEN_DOCKER` | Same, for `allow_docker` repos |
| `FACTORY_CI_BLOB_KEY` | Encrypts job state, 32 characters or more |
| `FACTORY_CI_BLOB_KEY_DOCKER` | Same, for `allow_docker`, a different value |
| `FACTORY_CI_HEARTBEAT_URL` | Optional dead-man ping after a good tick |

`GITHUB_TOKEN` is supplied by GitHub; do not set it.

Variables:

| Name | Use |
| --- | --- |
| `RUNNER_IMAGE_DIGEST` | Runner image, `sha256:<64 hex>`; printed by the release workflow |
| `FACTORY_CI_CLONE_IMAGE` | Clone image, from `factory-ci-mirror-images` |
| `FACTORY_CI_DIND_IMAGE` | Docker-in-docker image, from `factory-ci-mirror-images` |
| `FACTORY_CI_BWS_SERVER_URL` | Bitwarden server URL |
| `FACTORY_CI_BWS_PROJECT` | Bitwarden project holding pipeline secrets |
| `FACTORY_CI_BWS_PROJECT_DOCKER` | Same, for `allow_docker` |
| `FACTORY_CI_LOG_LINK` | Base link that commit statuses point to |

The Grafana Loki credentials (`FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`) are not read from GitHub. They sit in Bitwarden and the runner fetches them with the Bitwarden token.

## Docker images

Each folder under `docker/` is an image project. When a commit to `main` changes one, the runner builds it for `linux/amd64` and `linux/arm64` and pushes `sudtanj/<folder>:latest` and `sudtanj/<folder>:sha-<12 hex>` to Docker Hub, each a manifest list with both platforms. The runner creates the public repository when it is missing. `docker/hello-static` is the first image. To add one, follow `docker/README.md`.

The runner runs a static check on the folder before and again inside the publish step, and refuses to push when it fails. The status then reads `check failed: <ids>`. The ids are `layout`, `directive`, `instruction`, `heredoc`, `external-source`, `from-unpinned`, `from-arg`, `emulation`, `final-base`, `final-run`, `final-shell`, `final-user`, `secret-name`, `secret-value`, `secret-flag`, `secret-file`, `step-image`, `step-template` and `pipeline-secrets`; `specs/012-docker-images/data-model.md` defines each one. An older commit pushes only its `sha-` tag, so it never moves `latest`.

Setup: the Bitwarden project `FACTORY_CI_BWS_PROJECT_DOCKER` holds the keys `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`, once each. The `sudtanj/infrastructure` entry in `.github/factory-ci-repos.toml` lists both names in `secrets`, sets `allow_docker = true` and sets no `secret_branches`. The token needs Read and Write scope, with no delete and no admin right.

After the first publish of an image, run the post-publish check: pull `sudtanj/<folder>:latest` without a login, then confirm that both tags list exactly `linux/amd64` and `linux/arm64` with equal digests. Repeat it after any later tick that published.

Rollback: set `RUNNER_IMAGE_DIGEST` back to the previous value, revert the watch-entry change in its own pull request, delete the tags and any repository the runner created on Docker Hub by hand, and revoke the Docker Hub token.

Residual risk: whoever controls a build step on `main` can reach the host through the privileged docker sidecar, so that person is effectively a token holder. Only the owner merges to `main`. The static check helps a reviewer and does not stop a determined author. Rootless BuildKit would remove the sidecar and is out of scope.

## Run factory-monorepo CI by hand

`.github/workflows/factory-monorepo-ci.yml` runs the `.factory-ci.toml` pipeline of one repo with the released `rust-ci-runner` image. The workflow checks out this repo and starts one container, `docker run` of `RUNNER_IMAGE` (the env line at the top, pinned by tag and digest) with the host docker socket and a work directory mounted. The runner's `ci` command does the rest: it checks the repo and branch against `.github/factory-ci-repos.toml`, reads four settings from Bitwarden, probes Grafana Loki, checks the repo id against the GitHub API, fetches the branch, refuses docker steps and runs the pipeline. After each release, set the `RUNNER_IMAGE` line to the new digest in a reviewed commit.

To run it, open Actions, pick `factory-monorepo-ci`, choose `main` as the workflow branch and fill in the inputs. Runs started from any other branch do nothing. The inputs, and `FACTORY_CI_BWS_SERVER_URL` when set, show on the public run page and in the step header, so never put anything private in them. The by-hand tick (`factory-ci-tick.yml`) is the only other workflow that holds `BWS_ACCESS_TOKEN`; it is not in use and is due to be retired.

| Input | Default | Meaning |
| --- | --- | --- |
| `repo` | `sudtanj/factory-monorepo` | `owner/name`; must be in `.github/factory-ci-repos.toml` with its numeric `id` |
| `ref` | `main` | a branch that matches the `branches` of its watch entry; tags never match |
| `project` | `.` | a top-level folder with its own `.factory-ci.toml`, or `.` for the root |

The workflow uses the Environment `factory-ci`, which holds the secret `BWS_ACCESS_TOKEN` and may use the variable `FACTORY_CI_BWS_SERVER_URL` (leave it unset unless you run your own Bitwarden server). The runner reads these Bitwarden secrets: `FACTORY_CI_TOKEN` (a read-only fine-grained token that fetches the repo), `FACTORY_CI_LOG_URL` (host only; a trailing `/loki/api/v1/push` is stripped), `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` (a `logs:write` token). No step gets a pipeline secret in this workflow: a step that lists `secrets` fails the run. The tick hands no secret to a user step either. Its one token, the Docker Hub token of an image publish, goes only to the Docker Hub login request and, on standard input, to the two containers of the built-in publish step.

The Actions log is public, so it shows only fixed lines: `[<step>] started`, `success in <N>s`, `failed in <N>s`, `skipped`, then `[runner] pipeline passed` or `failed`. A failure before any step prints one fixed line on stderr, such as `[runner] clone failed`. The full step output, the failure reasons and the error details go to Grafana Loki: in Grafana Explore query `{service_name="factory-ci", repo="sudtanj/factory-monorepo"}`. Each line starts with `run=<id> sha=<short> branch=<name>`. The job exits 0 only when the pipeline passed and Loki took every line.

Rules for this workflow:

- Docker steps are refused. The workflow never passes `--allow-docker`; a docker-in-docker step would give root on the VM.
- Before anything runs, the runner posts one probe line (`project="probe"`) to Loki and prints `[runner] log store answered HTTP <code>`. A 401 or 403 prints a hint to check the three `FACTORY_CI_LOG_*` values; any other failure prints `log store check failed`. Both fail the run.
- A single log line larger than Loki's per-line limit makes Loki reject its whole batch. The runner drops that batch, ends with `[runner] log lines were dropped` and exits 1, even when the pipeline passed.
- Give the Bitwarden machine account read access to one project that holds only the four keys above. A second project with the same keys makes the run fail as ambiguous.
- The job must never run on a self-hosted runner. The runner container holds the docker socket and the access token; the throwaway GitHub-hosted VM contains that.

Owner settings (GitHub, not code). Set them before the first run:

- Environment `factory-ci`: deployment branches limited to `main`, the owner as required reviewer, and `BWS_ACCESS_TOKEN` stored only there.
- A tag ruleset for `rust-ci-runner-v*`.
- After this landing, a ruleset on `main`: pull request required, code-owner review for `.github/**` and `rust-ci-runner/**`, no force-push, no deletion.
- Optional: make the GHCR package public and drop the login step.

Owner exceptions, 2026-11-27. The constitution (1.1.0) lets this manual workflow hold `BWS_ACCESS_TOKEN`. Only `.github/workflows/rust-ci-runner-release.yml` builds the released runner; `.github/workflows/factory-ci-selftest.yml` also compiles it and builds the image without pushing, so breakage shows up before the direct push to `main`. The selftest holds no secret, and a step in it fails if that changes. Feature 008 supersedes parts of `specs/005-plain-actions-ci/`, `specs/006-runner-unit-tests/` and `specs/007-runner-image/`. To run the runner's own unit and integration tests (steps `unit` and `integration`), start the `rust-ci-runner` pipeline of `sudtanj/infrastructure` with `repo` set to `sudtanj/infrastructure`, `ref` `main` and `project` `rust-ci-runner`; the runner reads `rust-ci-runner/.factory-ci.toml` from the fetched branch.

## License

MIT, see `LICENSE`.
