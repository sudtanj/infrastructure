# Data Model: Infrastructure Home

No stored data. These are the configuration objects the feature touches.

## Environment

| Field | Value |
| --- | --- |
| name | `factory-ci-plan`, `factory-ci`, `factory-ci-docker` |
| deployment branches | `main` only |
| jobs | `plan` and `verdict` use `factory-ci-plan`; `run` uses `factory-ci`; `run-docker` uses `factory-ci-docker` |

## Bootstrap secret (name, Environments)

| Name | Required | `factory-ci-plan` | `factory-ci` | `factory-ci-docker` |
| --- | --- | --- | --- | --- |
| `FACTORY_CI_TOKEN` | yes | yes | yes | no |
| `FACTORY_CI_TOKEN_DOCKER` | only with `allow_docker` rows | yes | no | yes |
| `BWS_ACCESS_TOKEN` | only for pipelines with secrets | yes | yes | no |
| `BWS_ACCESS_TOKEN_DOCKER` | only with `allow_docker` rows | yes | no | yes |
| `FACTORY_CI_BLOB_KEY` | yes, 32 characters or more | yes | yes | no |
| `FACTORY_CI_BLOB_KEY_DOCKER` | optional: only with `allow_docker` rows, 32 characters or more, a different value | yes | no | yes |
| `FACTORY_CI_HEARTBEAT_URL` | optional | yes | no | no |
| `FACTORY_CI_LOG_URL` | all three or none; required for a private watched repo | yes | yes | yes |
| `FACTORY_CI_LOG_USER` | same | yes | yes | yes |
| `FACTORY_CI_LOG_TOKEN` | same | yes | yes | yes |

`GITHUB_TOKEN` is supplied by GitHub.

## Repository variable

| Name | Required | Set when |
| --- | --- | --- |
| `RUNNER_IMAGE_DIGEST` | yes | after the release; `sha256:<64 hex>` |
| `FACTORY_CI_CLONE_IMAGE` | yes | after `factory-ci-mirror-images`; pinned `@sha256` |
| `FACTORY_CI_DIND_IMAGE` | yes | same |
| `FACTORY_CI_BWS_PROJECT` | for pipelines with secrets | after Bitwarden setup |
| `FACTORY_CI_BWS_PROJECT_DOCKER` | with `allow_docker` rows | same |
| `FACTORY_CI_BWS_SERVER_URL` | outside the US cloud | same |
| `FACTORY_CI_LOG_LINK` | optional | Explore URL with `{run_id}` |

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

`on` (three non-empty), `off` (three empty), `partial` (anything else). `partial` is an error everywhere. A value with a trimmed line under 4 characters is an error.
