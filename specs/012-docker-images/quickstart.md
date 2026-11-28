# Quickstart: validate feature 012

Run these after the tasks are done. Contracts: `contracts/publish-step.md`, `contracts/image-project.md`. Rules: `data-model.md`.

## 1. Owner setup (once, before the runner digest moves; spec A14)

No agent touches a credential. The owner:

1. Creates a Docker Hub personal access token for account `sudtanj`: scope Read & Write (no delete, no admin), with an expiry.
2. Creates the public repository `sudtanj/hello-static` on Docker Hub by hand (this also settles the default visibility). A push to a missing repository ends in `registry refused the token`.
3. Creates a Bitwarden project that holds only this key, reachable only by the machine account behind `BWS_ACCESS_TOKEN_DOCKER`, and adds the secret with key `sudtanj/infrastructure/DOCKERHUB_TOKEN` (key `sudtanj/infrastructure:DOCKERHUB_TOKEN` if Bitwarden refuses `/`, see `FCI_KEY_SEP`). The repository variable `FACTORY_CI_BWS_PROJECT_DOCKER` names that project. Never paste the value anywhere else.
4. Confirms `FACTORY_CI_TOKEN_DOCKER` covers `sudtanj/infrastructure` with Commit statuses read and write and Contents and Metadata read, and that it and `FACTORY_CI_TOKEN` belong to the same GitHub user (the plan reads statuses by user).
5. Confirms the Environment `factory-ci-docker` is limited to `main` and has no required reviewers, and that `FACTORY_CI_BLOB_KEY_DOCKER` is set.
6. Writes down the current value of `gh variable get RUNNER_IMAGE_DIGEST` (the rollback value).

## 2. Unit and integration tests (selftest)

The pull request (or the push to the feature branch) changes `rust-ci-runner/**`, so `factory-ci-selftest` runs `cargo test --locked --lib` and `cargo test --locked --test '*'`, which includes `repo_files` and the static check of every `docker/` folder. The `factory-ci-selftest` result on the head commit is the proof. A push that changes only `docker/` does not start the selftest (known limit, plan K4): the tick on `main` runs the same check and blocks a publish, and the owner can include any `rust-ci-runner/` change in the push to get the selftest earlier.

## 3. Path check (FR-022, SC-004)

```
git diff --name-only origin/main...HEAD | grep -E '^\.github/(workflows|scripts)/' && echo FAIL || echo ok
```

Expected: `ok`.

## 4. Release and canary

1. Merge the PR. The tick still runs the 0.3.0 runner and plans the repo as before; check one tick.
2. Tag `rust-ci-runner-v0.4.0` on the merge commit and push the tag. The release workflow ends with the digest line; record it.
3. `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`.
4. Canary, two ticks: `factory-ci/rust-ci-runner` and `factory-ci/ci-canary` post once and are not planned again on the second tick. A re-plan means the two tokens are different users: stop and roll back.

## 5. End-to-end (SC-001, SC-003, SC-006, SC-007)

1. Wait for a tick. It posts `factory-ci/docker/hello-static` on the `main` tip: built, checked, pushed.
2. From a machine with no Docker Hub login:

```
docker pull sudtanj/hello-static:latest
docker buildx imagetools inspect sudtanj/hello-static:latest
docker buildx imagetools inspect sudtanj/hello-static:sha-<first 12 hex of the tip>
```

Expected: both digests equal, and each lists exactly `linux/amd64` and `linux/arm64`.
3. Pull and run on an amd64 host and on an arm64 host. Each prints the fixed line.
4. Search the public run log of the item and the commit status text for the token value and for `dckr_pat_`: zero hits.
5. Break the Dockerfile on a branch (remove a digest). The selftest fails on `repo_files::docker_folders_pass_the_static_check`. Merged by mistake, the tick posts `failure` `check failed: from-unpinned` and pushes nothing. Remove `--platform=$BUILDPLATFORM` from the builder: `check failed: emulation`.
6. Before the first release, check each pinned base: `docker buildx imagetools inspect <ref>@sha256:<digest>` for the Go builder, distroless and `docker:cli` lists both platforms (FR-028).

## 6. Post-publish check (SC-007), after every tick that ran a `docker/` change

`docker buildx imagetools inspect sudtanj/<image>:sha-<first 12 hex of the main tip>` shows a list with both platforms. No answer (rate limit, network) counts as unknown: ask again, do not assume it passed.

## 7. Rollback

1. `gh variable set RUNNER_IMAGE_DIGEST --body <value from step 1.6>`. The next tick runs 0.3.0: `docker/` is not discovered, nothing publishes.
2. Delete the published tags on Docker Hub by hand if they must go; the runner never deletes any.
3. Revoke the Docker Hub token and delete the Bitwarden key.
4. Revert the `allow_docker` line of `.github/factory-ci-repos.toml` in a separate reviewed PR.
