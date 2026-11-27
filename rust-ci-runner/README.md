# rust-ci-runner

A small CI runner in Rust. A repository describes its pipeline in `.factory-ci.toml`: steps, each in its own
container image, forming a dependency graph. Central mode runs on GitHub Actions in this repo (`sudtanj/infrastructure`),
which watches your other repos and posts a `factory-ci` commit status on each. The same binary runs a pipeline on your
machine.

- Toolchain pinned in `rust-toolchain.toml`. One static binary for Linux, x86_64 and aarch64.
- Local use needs Docker or Podman and nothing else.
- Commands: `make setup | dev | test | lint | build`. `make build TARGET=aarch64-unknown-linux-musl` makes the arm64
  binary; it needs `musl-tools` and a C compiler for that target (the release workflow uses `gcc-aarch64-linux-gnu`).

## How a tick works

An outside scheduler (cron-job.org, see the root README) dispatches `.github/workflows/factory-ci-tick.yml` in this repo
every 5 minutes. This repo is public, so its Actions minutes are free. A private copy of this repo is unsupported: an
idle tick would bill about 17,000 minutes a month.

1. `plan` reads `.github/factory-ci-repos.toml`, lists each repo's branch tips and posts nothing for commits that
   already have a status. It writes the work as a matrix with an encrypted blob per item.
2. `run` (repos without `allow_docker`) and `run-docker` (repos with it) start one job per item. Each job clones the
   commit, runs the pipeline and posts the final status. Job names are `item N`.
3. `verdict` fails the tick if the plan had a problem, then pings the heartbeat URL, even on idle ticks.

A project folder whose files did not change since the last status gets `success` "unchanged since <sha>" without a
run. An item that ends in `error` (a runner fault such as an image pull failure) gets one retry after 30 minutes,
marked " (retry)". A second `error` stays until a new commit. A failing step is `failure` and is never retried.

The ping is the dead-man signal. Point a ping service at the heartbeat URL with a period of 5 minutes and an alert
when it goes quiet. A tick that has work also writes one line to Loki, so that line marks ticks with work only; an
idle tick leaves no trace there.

An idle tick makes no Bitwarden call. A tick plans at most 20 items. The API budget (150 calls for an idle tick)
holds for about 13 repos with 5 branches each; more repos still work with more calls.

## Setup

The setup steps, every secret and variable with its Environment, and the checks are in the root `README.md`, section
Setup. Follow it once, in order, before the first tick. `FACTORY_CI_BLOB_KEY_DOCKER` is optional: it is needed only
when `allow_docker` rows exist, it must be at least 32 characters, and it must differ from `FACTORY_CI_BLOB_KEY`.

## Release the runner

Push a tag `rust-ci-runner-vX.Y.Z` on a commit on `main`. The tag must match `version` in `Cargo.toml`. The
`rust-ci-runner-release` workflow builds static x86_64 and aarch64 binaries, attaches them to a release, and pushes a
multi-arch image holding only the binary to `ghcr.io/<owner>/rust-ci-runner`. The last step prints the image digest in
the run summary: "Set the repository variable RUNNER_IMAGE_DIGEST to ...". Set that variable to the digest alone, `sha256:<64 hex>`, without the image name.

Run the manual workflow `factory-ci-mirror-images` once for the clone and docker-in-docker images, and make the
GHCR packages public. A tick pulls the runner by digest. When `RUNNER_IMAGE_DIGEST` is unset, the tick fails before
it touches any repo.

## The watch list

`.github/factory-ci-repos.toml` is public. One `[[watch]]` table per repo; unknown keys are errors.

```toml
[[watch]]
repo = "you/invoices"              # owner/name
id = 123456789                     # numeric repo id; needed for owner/name keys in Bitwarden
branches = ["main", "release/*"]   # default: every branch
secret_branches = ["main"]         # default: the repo's default branch
secrets = ["NPM_TOKEN"]            # shared keys, default none
allow_docker = false               # default false
```

Check a file with `rust-ci-runner plan --repos .github/factory-ci-repos.toml --check`. It reports every problem and
needs no token or network. The selftest workflow runs it on pull requests. Removing an entry stops all contact with
that repo; its old statuses stay.

A watched repo without `.factory-ci.toml` is skipped: no status, no job and no error.

## Secrets

Bitwarden Secrets Manager is the only store for pipeline secrets. There is no GitHub-secret fallback. With `BWS_ACCESS_TOKEN`
unset, a pipeline that lists a secret ends in `error` ("no secret store configured"); a pipeline with none still runs.

A step gets only the names it lists in `secrets`. A job looks them up in the default project, or in the docker
project for `allow_docker` repos:

- `owner/name/NAME`: a repo's own secret. The entry must record `id`. If Bitwarden refuses `/` in key names, change
  the default of `FCI_KEY_SEP` in `.github/scripts/factory-ci-secrets.sh` to `:` and name the keys `owner/name:NAME`.
  The separator is `/` or `:`, never `__`.
- `NAME`: a shared key. A repo can read it only when its entry lists the name in `secrets`.

The free plan allows 3 projects and 3 machine accounts. This setup uses 2 of each: a default project and a docker
project, each with its own account. Give each account "Can read" on its project only. A key under another repo's
prefix is never read, but the account can read the whole project, so anyone who can push a pipeline to a listed repo
can read that repo's keys and the shared names its entry lists.

The Bitwarden project IDs go in the variables `FACTORY_CI_BWS_PROJECT` and `FACTORY_CI_BWS_PROJECT_DOCKER`, and the
server URL in `FACTORY_CI_BWS_SERVER_URL` outside the US cloud. Secrets reach only branches that match `secret_branches`.
On another branch a step that lists a secret ends in `error` ("secrets not allowed on this branch"); steps with none run.

A value with a line shorter than 4 characters is refused: masking so short a line would hide ordinary text. Change the
secret.

Rollback: remove `BWS_ACCESS_TOKEN` (and the docker token) from the Environments. Pipelines that list no secret keep
running; the others end in `error` until you restore it. Log shipping keeps running, because its credentials do not
come from Bitwarden. Removing the `FACTORY_CI_LOG_*` secrets stops log shipping, and a private watched repo is then refused.

## Log shipping

This repo is public, so step output must not appear in its Actions log. The three log credentials, `FACTORY_CI_LOG_URL`,
`FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` (Grafana Cloud Loki: the base URL, the instance user ID and a
`logs:write` token), are Environment secrets of `factory-ci-plan`, `factory-ci` and `factory-ci-docker`. The runner reads
them from its environment; no Bitwarden call fetches them. `factory-ci-docker` holds its own token, so you can cut it
off alone. The root README, step 4, has the table. Jobs ship every output line to Loki, labelled by repo, project, step
and stream, with secrets replaced. The Actions log shows only `item N`.

Set the variable `FACTORY_CI_LOG_LINK` to an Explore URL containing `{run_id}`; it becomes the status link. Without it
the link is the Actions run. If Loki rejects the credentials or stays down, the result is unchanged and the status
ends " (logs not delivered)".

A private watched repo needs log shipping and fails closed: without it the plan refuses the repo. `job --private` and
`--quiet-logs` also refuse to start without a log store.

## Commands

```sh
rust-ci-runner run [--dir PATH] [--quiet-logs] [STEP]
rust-ci-runner plan --repos FILE --check
rust-ci-runner plan --repos FILE --rows-out CAND
rust-ci-runner plan --rows-in CAND --rows-out ROWS --summary FILE [--cap N]
rust-ci-runner job --repo OWNER/NAME --sha SHA --project PATH --branch NAME --run-id ID [--prev SHA] [--allow-docker] [--quiet-logs] [--private] [--retry]
rust-ci-runner status --repo OWNER/NAME --sha SHA --project PATH --run-id ID --state error|failure|success --description TEXT [--retry]
```

The workflow calls `plan`, `job` and `status`. You use `run` and `plan --check`.

## Pipeline file `.factory-ci.toml`

At the repository root. Unknown keys are errors. The whole file is checked before anything
starts, and every problem is reported: a cycle, a duplicate name, a dependency on an unknown
step, a missing image or commands, a bad name. At most 100 steps, 256 KB.

```toml
[steps.lint]
image = "rust:1.90"
commands = ["rustup component add clippy rustfmt", "cargo fmt --check", "cargo clippy -- -D warnings"]

[steps.test]
image = "rust:1.90"
commands = ["cargo test"]
timeout = 600                 # seconds; default: the run timeout
env = { RUST_BACKTRACE = "1" }

[steps.image]
image = "docker:27.5.1-cli"
docker = true                 # needs allow_docker = true in the watch entry
depends_on = ["lint", "test"]
secrets = ["REGISTRY_TOKEN"]  # names, looked up in Bitwarden
commands = [
  "docker build -t app .",
  "echo \"$REGISTRY_TOKEN\" | docker login -u ci --password-stdin registry.example.com",
  "docker push registry.example.com/app",
]
```

| Key | Meaning |
|---|---|
| `image` | Container image of the step. |
| `commands` | Run in order with `sh -c` in one shell with `set -e`: the first failing command ends the step. |
| `depends_on` | Steps that must succeed first. Others run in parallel, up to four at a time. A step whose dependency failed or was skipped is skipped. |
| `env` | Plain variables. Never put a secret here: the value is an argument of the engine command and shows in `ps` and in the committed file. Use `secrets`. |
| `secrets` | Names of secrets the step needs. Only these reach the container. A name with no value ends the item in `error` before any container starts. |
| `timeout` | Seconds, 1 to 86400. |
| `docker` | `true` gives the step a Docker daemon. See below. |

All steps of a run share one checkout, mounted at `/workspace`, which is the working directory.
A file one step writes is there for the steps that depend on it. Every output line is
prefixed with the step name. Each step also gets `CI=true`, `FACTORY_CI=true`,
`FACTORY_CI_RUN_ID`, `FACTORY_CI_SHA`, `FACTORY_CI_PREV_SHA` (when known), `FACTORY_CI_BRANCH`
and `FACTORY_CI_EVENT` (`push`, or `local` for `run`), so a pipeline
can pick what to run, for example only the projects that changed.

### Docker steps

A step with `docker = true` can run `docker build` and `docker run`. The runner starts a
`docker:dind` container for the run, on a private network named after the run, and the step
joins that network with `DOCKER_HOST=tcp://<sidecar>:2375`. The step image needs the docker
CLI (for example `docker:cli`). Two runs never share a daemon, so they cannot see each
other's images or containers. The daemon, its network and its volume are removed when the run
ends, and found by label and removed after a restart. The host's Docker socket is never mounted
anywhere, and steps without `docker = true` get no `DOCKER_HOST`.

The daemon lives in another container, so `docker run -v /workspace:/x` does not see the
checkout; use `docker build` (the context is sent to the daemon) or `docker cp`.

**Security.** The sidecar is privileged, and a privileged container can escape to the host.
Treat a `docker = true` step as root on the host. Turn on `allow_docker` only for repositories
whose pipelines you trust.  Steps without
`docker = true` run on a separate per-run network, with `--pids-limit 512` and
`no-new-privileges`, and cannot reach the daemon.

## Run a pipeline locally

```sh
rust-ci-runner run                  # the pipeline in the current directory
rust-ci-runner run --dir ../app     # another directory
rust-ci-runner run test             # one step and the steps it depends on
```

It uses the same graph and executor as a tick and exits non-zero if the pipeline failed. It needs no token and makes
no network call of its own. Secrets come from environment variables of the same name; a missing one fails the run
before any container starts and names the secret. If `FACTORY_CI_SECRETS` is set (comma-separated names), only those
names are looked up. `FACTORY_CI_ENGINE=podman` selects Podman. `docker = true` steps work without any opt-in.
Output goes to stdout, or to the log store when the `FACTORY_CI_LOG_*` variables are set (`--quiet-logs` then prints
nothing). Steps run as root in most images, so files they create in the checkout may be owned by root.

## Risks you accept

- Repo names are public. This repo is public, so the watch list and the tick summary show repo names. Job names, matrix
  values and logs show no branch, SHA, project path, secret name or run ID.
- `allow_docker` gives a repo's committers root on the runner and the docker secrets of this repo. Turn it on only for
  repos whose committers you trust. Anyone who can push to a watched branch runs code on this repo's runners and can read
  the secrets their pipeline lists on `secret_branches`.
- Docker Hub pull limits apply to anonymous pulls from shared runners. A limit hit ends the item in `error`, which
  gets one retry; a second one stays red until a new commit.
- A secret is hidden in a log line only when its whole value appears in it. A step that transforms it (base64, URL
  encoding) or prints it across two lines reaches Loki unmasked. Loki holds the encoded output as printed, so
  restrict who can read the store.
- Status links to Grafana stop working after its 14-day retention.

## Limits

- A push is picked up at the next tick, up to 5 minutes later, or later if the tick is busy.
- Secret names must not start with `GH_`, `GITHUB_`, `ACTIONS_`, `RUNNER_`, `DOCKER_`, `FACTORY_CI` and a few other
  prefixes. Step names `clone` and `dind` are reserved.
- No per-step statuses, artifacts, caches, service containers or matrices.

## Tests

`make test` runs unit tests, and integration tests that need a running Docker with network access. `make lint` is
`cargo fmt --check` and `cargo clippy -- -D warnings`. The workflow `factory-ci-selftest.yml` builds the binary,
runs `tests/fixtures/selftest/`, checks the watch list, and lints the tick and release workflows.
