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

`.github/workflows/factory-monorepo-ci.yml` runs the `.factory-ci.toml` pipeline of the private repo `sudtanj/factory-monorepo` with the released `rust-ci-runner` image. The job copies the binary out of `RUNNER_IMAGE` (the env line at the top of the workflow, pinned by tag and digest) and runs it on the runner VM. After each release, set that line to the new digest. Only `.github/workflows/rust-ci-runner-release.yml` builds the runner. Before the first digest is filled in, the line holds `DIGEST_PLACEHOLDER` and the pull fails.

To run it, open Actions, pick `factory-monorepo-ci`, choose `main` as the workflow branch, and set `ref` to the factory-monorepo branch (default `main`). Runs started from any other branch do nothing. The job uses the Environment `factory-ci`, which needs the secret `BWS_ACCESS_TOKEN` and may use the variable `FACTORY_CI_BWS_SERVER_URL`. It reads the Bitwarden secrets `FACTORY_CI_TOKEN` (to clone factory-monorepo) and `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN` (Grafana Cloud Loki: base URL, instance user ID, a `logs:write` token). Steps get no pipeline secrets here.

To run the unit tests of `rust-ci-runner` itself, open Actions, pick `factory-monorepo-ci`, choose Run workflow on `main` and set `pipeline` to `rust-ci-runner`. The runner then reads `rust-ci-runner/.factory-ci.toml` from this repo's `main` checkout and runs that pipeline in a `rust:1.90` container. `ref` is ignored and factory-monorepo is not cloned. Only the `--lib` unit tests run: the integration tests in `rust-ci-runner/tests/` call the real `docker` binary, which a step container does not have. The tests of `ship` and `job` run in their own processes because a step container allows 512 processes. Spec: `specs/006-runner-unit-tests/`.

The log is public, so it shows only each step's name, result and duration, plus `<pipeline> <ref>: passed` or `failed` (`rust-ci-runner main: passed` for the runner's tests). The full step output goes to Grafana Loki: in Grafana Explore query `{service_name="factory-ci", repo="sudtanj/infrastructure"}` (Logs Drilldown lists it as service `factory-ci`); the summary prints this query; each line starts with `run=<id> sha=<short> branch=<name>`. It is never uploaded as an artifact. The job exits with the runner's exit code and posts no commit status.

Rules for this workflow:

- Logs are minimal on purpose. The repo is public, factory-monorepo is private. If the runner fails before any step, you see one fixed line and nothing else.
- Docker steps are refused. The job fails if the root `.factory-ci.toml` has `docker = true`, because a docker-in-docker step would give root on the VM and access to the job's secrets. Use the factory CI runner with `allow_docker` for such repos.
- Before the runner starts, the job posts one probe line (`project="probe"`) to Grafana Loki and prints `log store answered HTTP <code>`. A non-2xx answer fails the job with a hint to check `FACTORY_CI_LOG_URL` (host only; a trailing `/loki/api/v1/push` is stripped), `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`. The runner itself drops failed pushes silently.
- Keep `FACTORY_CI_BWS_SERVER_URL` unset unless you run your own Bitwarden server.
- Give the Bitwarden machine account behind `factory-ci` read access only to a project that holds `FACTORY_CI_TOKEN` and the three `FACTORY_CI_LOG_*` keys. The workflow lists every secret that account can read.

Owner exception, 2026-11-27: only `.github/workflows/rust-ci-runner-release.yml` builds the runner (rule: compile, test and lint run only through the CI runner). The pipeline workflow relies on the released image and compiles nothing itself; the runner's unit tests compile inside their own step container, which counts as the pipeline. This replaces the build exception of `specs/005-plain-actions-ci/`. Spec: `specs/007-runner-image/`.

## License

MIT, see `LICENSE`.
