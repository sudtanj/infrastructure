# Data Model: Infrastructure Home

No stored data. These are the configuration objects the feature touches. Round 4 (one Bitwarden token) replaced the Environment and secret tables.

## Environment

| Field | Value |
| --- | --- |
| name | `factory-ci`, `factory-ci-release` |
| deployment branches | `main` only; `factory-ci-release` also allows the tag pattern `rust-ci-runner-v*` |
| admin bypass | off |
| jobs | `plan`, `run` and `verdict` use `factory-ci`; release `publish` and the image mirror use `factory-ci-release` (no secrets) |
| selftest | names no Environment |

## GitHub Actions items (all of them)

| Name | Type | Scope | Required |
| --- | --- | --- | --- |
| `BWS_ACCESS_TOKEN` | Environment secret | `factory-ci` | yes |
| `FACTORY_CI_BWS_SERVER_URL` | repository variable | repository, must start with `https://` | only outside the US cloud |

`GITHUB_TOKEN` is supplied by GitHub. `gh secret list` shows no secret at repo or org level, and `gh secret list --env factory-ci` shows exactly `BWS_ACCESS_TOKEN`.

## Bitwarden secret (key, load list)

One project; one machine account with Can read. Keys are plain, with no prefix. `factory-ci-load.sh` writes the value to `$GITHUB_ENV` under the key, except `RUNNER_IMAGE_DIGEST`, written as `FCI_RUNNER_IMAGE_DIGEST`.

| Key | Required | Form | `plan` | `run` | `verdict` |
| --- | --- | --- | --- | --- | --- |
| `FACTORY_CI_TOKEN` | yes | fine-grained PAT (FR-016) | yes | yes | no |
| `FACTORY_CI_BLOB_KEY` | yes | 64 lower-case hex, `openssl rand -hex 32` | yes | yes | no |
| `RUNNER_IMAGE_DIGEST` | yes | `sha256:<64 hex>` | yes | yes | no |
| `FACTORY_CI_CLONE_IMAGE` | yes | `...@sha256:<64 hex>` | no | yes | no |
| `FACTORY_CI_DIND_IMAGE` | yes | `...@sha256:<64 hex>` | no | yes | no |
| `FACTORY_CI_LOG_URL` | all three or none; required for a private watched repo | Loki base URL | yes | yes | no |
| `FACTORY_CI_LOG_USER` | same | Loki instance user ID | yes | yes | no |
| `FACTORY_CI_LOG_TOKEN` | same | `logs:write` token | yes | yes | no |
| `FACTORY_CI_LOG_LINK` | optional | Explore URL with `{run_id}` | yes | yes | no |
| `FACTORY_CI_HEARTBEAT_URL` | optional | healthchecks.io ping URL | no | no | yes |
| pipeline secrets | per pipeline | names in `.factory-ci.toml` steps; shared names or `<owner>/<repo>/<NAME>` keys | no | second list call, only with names | no |

Every key above starts with `FACTORY_CI` or `RUNNER_`, which the runner reserves, so no pipeline can ask for one (FR-053).

## Removed

`FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY_DOCKER`, `FACTORY_CI_BWS_PROJECT`, `FACTORY_CI_BWS_PROJECT_DOCKER`, the Environments `factory-ci-docker` and `factory-ci-plan`, the job `run-docker` and its outputs. The old repository variables for the images, the digest and the log link move into Bitwarden.

## Watch entry

Unchanged schema (`contracts/watch-list.md` in 002). 003 adds two entries:

| repo | branches | secret_branches | secrets | allow_docker |
| --- | --- | --- | --- | --- |
| `sudtanj/factory-monorepo` | default (all) | default | none | false |
| `sudtanj/infrastructure` | default (all) | default | none | false |

## Pipeline file

| Path | Project context | Steps |
| --- | --- | --- |
| `.factory-ci.toml` | `factory-ci` | `shell`, `lint-workflows` |
| `rust-ci-runner/.factory-ci.toml` | `factory-ci/rust-ci-runner` | `lint`, `test` |

## Log credential state (derived)

`on` (three non-empty), `off` (three absent), `partial` (anything else). `partial` is an error everywhere. A value with a trimmed line under 4 characters is an error.
