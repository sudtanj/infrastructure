# Quickstart: validate feature 012

Run these after the tasks are done. Contracts: `contracts/publish-step.md`, `contracts/image-project.md`. Rules: `data-model.md`.

## 1. Owner setup (once, before the runner digest moves)

1. Create a Docker Hub access token, read and write, for account `sudtanj`. Confirm that a repository created by a push is public (spec A5).
2. In the Bitwarden project that `FACTORY_CI_BWS_PROJECT_DOCKER` names, add a secret with key `sudtanj/infrastructure/DOCKERHUB_TOKEN` (key `sudtanj/infrastructure:DOCKERHUB_TOKEN` if Bitwarden refuses `/`, see `FCI_KEY_SEP`). Value is the token. Never paste it anywhere else.
3. Confirm the Actions secrets `FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY_DOCKER` exist in Environment `factory-ci-docker`, and that `FACTORY_CI_TOKEN_DOCKER` can write commit statuses on `sudtanj/infrastructure`.
4. Write down the current value of the repository variable `RUNNER_IMAGE_DIGEST` (the rollback value).

## 2. Unit and integration tests (CI runner pipeline, not on a laptop)

The `rust-ci-runner` project of `.factory-ci.toml` runs `unit` and `integration`. New test files are added to those commands. A green tick status `factory-ci/rust-ci-runner` on the PR head is the proof.

## 3. Path check (FR-022, SC-004)

```
git diff --name-only origin/main...HEAD | grep -E '^\.github/(workflows|scripts)/' && echo FAIL || echo ok
```

Expected: `ok`.

## 4. Release

1. Record the current digest: `gh variable get RUNNER_IMAGE_DIGEST` (rollback value).
2. Merge the PR.
3. Tag `rust-ci-runner-v0.4.0` on the merge commit and push the tag. The release workflow ends with the digest line; record it.
4. `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`.
5. Canary: the next tick must plan and post `factory-ci/rust-ci-runner` and `factory-ci/ci-canary` as before. Only then trust an image push.

## 5. End-to-end (SC-001, SC-003, SC-006)

1. Wait for a tick. The first tick after the digest change posts `factory-ci/docker/hello-static` on the current `main` tip, built, checked and pushed.
2. From a machine with no Docker Hub login:

```
docker pull sudtanj/hello-static:latest
docker buildx imagetools inspect sudtanj/hello-static:latest
docker buildx imagetools inspect sudtanj/hello-static:sha-<short>
```

Expected: both digests equal, and each lists exactly `linux/amd64` and `linux/arm64`.
3. Pull and run on an amd64 host and on an arm64 host. Each prints the fixed line.
4. Search the public run log of the item and the commit status text for the token value and for `dckr_pat_`: zero hits.
5. Push a branch that changes `docker/hello-static/README.md`. Expected: status `built and checked, not published`; the Docker Hub tags do not move; the Bitwarden audit log shows no read of `DOCKERHUB_TOKEN` for that run.
6. Fast-forward `main` to that branch tip. Expected: the next tick runs the image again and publishes.
7. Break the Dockerfile on a branch (remove a digest). Expected: status `failure` `check failed: from-unpinned`. Remove `--platform=$BUILDPLATFORM` from the builder: `check failed: emulation`.
8. Before the first release, check each pinned base: `docker buildx imagetools inspect golang@sha256:<digest>` and the distroless one list both platforms (FR-028).

## 6. Rollback check

`gh variable set RUNNER_IMAGE_DIGEST --body <value from step 4.1>`. Next tick runs the 0.3.0 runner: `docker/` is not discovered, nothing publishes.
