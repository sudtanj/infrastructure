# Quickstart: Infrastructure Home

Checks the owner runs, in order. Each names the spec item it proves. Nothing here puts a value in the repo: names only. Results go in `research.md` without run URLs of private repos, Grafana hostnames or any token (FR-032).

## Part 1. On the pull request (no secrets)

1. Every selftest job is green: `check`, `pipeline`, `build-release`, `own-pipelines`, `list`, `workflow`, `shell`. (SC-001, FR-008, FR-028)
2. `rust-ci-runner/Cargo.lock` is in the diff, `Cargo.toml` holds `rust-version = "1.90"`, and `factory-ci-bootstrap.yml` is not in the tree. (FR-006, FR-007, FR-009)
3. `grep -rn 'ubuntu-latest' .github` prints nothing. (FR-003)
4. `grep -rnP '(?<!factory-)monorepo' README.md rust-ci-runner .github` prints nothing. (FR-001)
5. The `shell` job log shows `test-factory-ci-docs.sh` passing. (FR-023, FR-032)

## Part 2. Setup by hand (README "Setup")

Follow the root README top to bottom: repository rules, Environments, Bitwarden, Grafana, GitHub tokens, Environment secrets, merge, release tag, public package, variables, mirror images, scheduler, first manual tick. After Setup, `gh secret list` (repo and org) shows none of the bootstrap names. (FR-022)

## Part 3. After Setup (live checks, 002 T046, T054, T065)

Checks 1 to 5, 8 and 9 on factory-monorepo need its PR 2 merged (the root `.factory-ci.toml` on its `main`). Until then the plan skips that repo, which is itself check 12.

1. Dispatch `factory-ci-tick` by hand from `main`. All jobs pass and `verdict` is green. (SC-003)
2. `sudtanj/infrastructure` `main` has `factory-ci` and `factory-ci/rust-ci-runner` statuses, both `success`.
3. `sudtanj/factory-monorepo` has `factory-ci` statuses on its branch tips. (needs PR 2)
4. Loki holds the step output of a monorepo run, searchable in 30 seconds. The Actions log shows only `item N`. (SC-004, needs PR 2)
5. Signed out, open the public run log and search it for a branch name, a SHA, a project path, a secret name and the host of the Loki URL. None appears.
6. Unset `BWS_ACCESS_TOKEN` in `factory-ci`, dispatch a tick: a pipeline with no secrets still runs and ships logs. Restore the token. (SC-005)
7. Unset `FACTORY_CI_LOG_TOKEN` in `factory-ci-plan` only: the tick fails at the start and names it. Restore it.
8. Revoke the Bitwarden token: a pipeline that lists a secret ends in `error` with "secret store rejected the token". (002 T054)
9. A step container cannot reach `169.254.169.254`, and the DROP rules cover IPv6 too. (002 T065)
10. Dispatch from a branch other than `main`: the Environment refuses the job, `plan` does not run and `verdict` ends red. No secret is read. (002 T065)
11. Set `FACTORY_CI_BWS_SERVER_URL` to an `http://` value on a scratch run: the tick refuses it before any download. Unset it. (FR-031)
12. With a watched repo that has no `.factory-ci.toml`, a tick posts nothing for it and does not fail. (FR-017)
13. `gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility` prints `public`, and the release digest came from a run on `main`. (FR-004, FR-022)
14. Record results in `research.md` under "Live check results". Only after checks 1 to 5 and 8 pass and some days of green ticks, delete the old `FACTORY_CI_LOG_*` keys from Bitwarden.
