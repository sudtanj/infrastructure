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

`.github/workflows/factory-monorepo-ci.yml` runs the `.factory-ci.toml` pipeline of the private repo `sudtanj/factory-monorepo` with `rust-ci-runner` built from source. It is a stopgap. Delete it once the runner image is released and the tick runs (`specs/003-infrastructure-home/`).

To run it, open Actions, pick `factory-monorepo-ci`, choose `main` as the workflow branch, and set `ref` to the factory-monorepo branch (default `main`). Runs started from any other branch do nothing. The job uses the Environment `factory-ci`, which needs the secret `BWS_ACCESS_TOKEN` and may use the variable `FACTORY_CI_BWS_SERVER_URL`. It reads the Bitwarden secret `FACTORY_CI_TOKEN` to clone factory-monorepo. Steps get no pipeline secrets here.

The log is public, so it shows only each step's name, result and duration, plus `factory-monorepo <ref>: passed` or `failed`. The runner's full output goes to a file on the hosted runner and is never uploaded. The job exits with the runner's exit code and posts no commit status.

Owner exception, 2026-11-27: building the runner with cargo in a plain workflow breaks the rule that compile, test and lint run only through the CI runner. The runner cannot build itself before it exists, so only its own build is exempt. factory-monorepo's steps still run through `rust-ci-runner`. Spec: `specs/005-plain-actions-ci/`.

## License

MIT, see `LICENSE`.
