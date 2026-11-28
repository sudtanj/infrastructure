# Quickstart: validate feature 012

Run these after the tasks are done. Contracts: `contracts/publish-step.md`, `contracts/hub-api.md`, `contracts/image-project.md`. Rules: `data-model.md`.

## 0. Owner spikes with the real token (T002), status codes only

The owner runs these with the Docker Hub token in a shell variable that is never echoed, and reports only the status codes. Use a throwaway repository name such as `spike-012`; delete it afterwards by hand.

```
# login: expect 200
curl -s -o /tmp/hub.json -w '%{http_code}\n' -X POST https://hub.docker.com/v2/auth/token \
  -H 'Content-Type: application/json' \
  -d "{\"identifier\":\"sudtanj\",\"secret\":\"$HUB_TOKEN\"}"
JWT=$(jq -r .access_token /tmp/hub.json)
# probe a missing repository: expect 404 (a 401 here means the token cannot read repositories)
curl -s -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $JWT" https://hub.docker.com/v2/repositories/sudtanj/spike-012/
# create it public: expect 201 (a 401 or 403 means a Read & Write token cannot create: owner decision)
curl -s -o /dev/null -w '%{http_code}\n' -X POST https://hub.docker.com/v2/repositories/ \
  -H "Authorization: Bearer $JWT" -H 'Content-Type: application/json' \
  -d '{"namespace":"sudtanj","name":"spike-012","is_private":false}'
# probe again: expect 200 and is_private false
curl -s -H "Authorization: Bearer $JWT" https://hub.docker.com/v2/repositories/sudtanj/spike-012/ | jq .is_private
```

If the login answers 401 with a valid token, retry against `POST /v2/users/login` with `{"username":"sudtanj","password":"<token>"}` and report which endpoint works. The crane and mirror spikes of research.md use the same throwaway repository.

## 1. Owner setup (once, before the runner digest moves; spec A14), names only

The two keys already exist. The owner confirms, without printing a value:

1. The project `FACTORY_CI_BWS_PROJECT_DOCKER` names holds exactly the keys `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`, readable by the machine account behind `BWS_ACCESS_TOKEN_DOCKER`. No agent can check this: `bws secret list` returns values, so a names-only read does not exist. The owner runs:

```
bws secret list "$PROJECT" -o json | jq -r '.[].key' | grep -Ex 'DOCKERHUB_(USERNAME|TOKEN)'
```

Expected: two lines, once each.
2. The Docker Hub token has Read & Write scope, no delete and no admin, with an expiry, plus whatever the T002 result says repository creation needs.
3. `FACTORY_CI_TOKEN_DOCKER` covers `sudtanj/infrastructure` with Commit statuses read and write and Contents and Metadata read, and it and `FACTORY_CI_TOKEN` belong to the same GitHub user (the plan reads statuses by user).
4. The Environment `factory-ci-docker` is limited to `main` and has no required reviewers, and `FACTORY_CI_BLOB_KEY_DOCKER` is set.
5. The current value of `gh variable get RUNNER_IMAGE_DIGEST` is written down (the rollback value).

The owner no longer creates the repository: the runner does.

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

1. Wait for a tick. It posts `factory-ci/docker/hello-static` on the `main` tip: built, checked, repository created public if missing, pushed.
2. From a machine with no Docker Hub login:

```
docker pull sudtanj/hello-static:latest
docker buildx imagetools inspect sudtanj/hello-static:latest
docker buildx imagetools inspect sudtanj/hello-static:sha-<first 12 hex of the tip>
```

Expected: both digests equal, and each lists exactly `linux/amd64` and `linux/arm64`. The pull works without login only if the repository is public: that is what the runner's creation and probe guarantee.
3. Pull and run on an amd64 host and on an arm64 host. Each prints the fixed line.
4. Search the public run log of the item and the commit status text for the token value and for `dckr_pat_`: zero hits. `sudtanj` shows as `***` in later lines of the job log because the script masks the username; that is expected.
5. Break the Dockerfile on a branch (remove a digest). The selftest fails on `repo_files::docker_folders_pass_the_static_check`. Merged by mistake, the tick posts `failure` `check failed: from-unpinned` and pushes nothing. Remove `--platform=$BUILDPLATFORM` from the builder: `check failed: emulation`.
6. Before the first release, check each pinned base: `docker buildx imagetools inspect <ref>@sha256:<digest>` for the Go builder, distroless and `docker:cli` lists both platforms (FR-028).
7. Set a repository private on Docker Hub by hand and run the next publish of it: the status is `publish refused: repository is private` and nothing changed. Set it back.

## 6. Post-publish check (SC-007), after every tick that ran a `docker/` change

`docker buildx imagetools inspect sudtanj/<image>:sha-<first 12 hex of the main tip>` shows a list with both platforms. No answer (rate limit, network) counts as unknown: ask again, do not assume it passed.

## 7. Rollback

1. `gh variable set RUNNER_IMAGE_DIGEST --body <value from step 1.5>`. The next tick runs 0.3.0: `docker/` is not discovered, nothing publishes.
2. Delete the published tags, and any repository the runner created, on Docker Hub by hand if they must go; the runner never deletes any.
3. Revoke the Docker Hub token.
4. Revert the `allow_docker` and `secrets` lines of `.github/factory-ci-repos.toml` in a separate reviewed PR (with the `secrets` lines gone, the repo-files test returns to "no secrets").
