# infrastructure

The factory CI runner (`rust-ci-runner`) and every GitHub Actions workflow for sudtanj's repositories. The runner is a Rust binary. A scheduled tick finds new commits in the repos listed in `.github/factory-ci-repos.toml`, runs the steps each repo declares in its `.factory-ci.toml` inside containers, and posts commit statuses. Logs go to Grafana Loki.

Docs:

- `rust-ci-runner/README.md`: how the runner works, setup, release, limits.
- `specs/002-ci-runner/`: the spec, plan, tasks and quickstart.
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

## Run factory-monorepo CI by hand

`.github/workflows/factory-monorepo-ci.yml` runs the `.factory-ci.toml` pipeline of one repo with the released `rust-ci-runner` image. The workflow checks out this repo and starts one container, `docker run` of `RUNNER_IMAGE` (the env line at the top, pinned by tag and digest) with the host docker socket and a work directory mounted. The runner's `ci` command does the rest: it checks the repo and branch against `.github/factory-ci-repos.toml`, reads four settings from Bitwarden, probes Grafana Loki, checks the repo id against the GitHub API, fetches the branch, refuses docker steps and runs the pipeline. After each release, set the `RUNNER_IMAGE` line to the new digest in a reviewed commit.

To run it, open Actions, pick `factory-monorepo-ci`, choose `main` as the workflow branch and fill in the inputs. Runs started from any other branch do nothing.

| Input | Default | Meaning |
| --- | --- | --- |
| `repo` | `sudtanj/factory-monorepo` | `owner/name`; must be in `.github/factory-ci-repos.toml` with its numeric `id` |
| `ref` | `main` | a branch that matches the `branches` of its watch entry; tags never match |
| `project` | `.` | a top-level folder with its own `.factory-ci.toml`, or `.` for the root |

The workflow uses the Environment `factory-ci`, which holds the secret `BWS_ACCESS_TOKEN` and may use the variable `FACTORY_CI_BWS_SERVER_URL` (leave it unset unless you run your own Bitwarden server). The runner reads these Bitwarden secrets: `FACTORY_CI_TOKEN` (a read-only fine-grained token that fetches the repo), `FACTORY_CI_LOG_URL` (host only; a trailing `/loki/api/v1/push` is stripped), `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` (a `logs:write` token). Steps get no pipeline secrets here; a step that lists `secrets` fails the run.

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

Owner exceptions, 2026-11-27. The constitution (1.1.0) lets this manual workflow hold `BWS_ACCESS_TOKEN`. Only `.github/workflows/rust-ci-runner-release.yml` builds the released runner; `.github/workflows/factory-ci-selftest.yml` also compiles it and builds the image without pushing, so breakage shows up before the direct push to `main`. The selftest holds no secret, and a step in it fails if that changes. Feature 008 supersedes parts of `specs/005-plain-actions-ci/`, `specs/006-runner-unit-tests/` and `specs/007-runner-image/`. To run the runner's own unit tests, start the `rust-ci-runner` pipeline of `sudtanj/infrastructure` with `repo` set to `sudtanj/infrastructure`, `ref` `main` and `project` `rust-ci-runner`; the runner reads `rust-ci-runner/.factory-ci.toml` from the fetched branch.

## License

MIT, see `LICENSE`.
