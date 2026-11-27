# Quickstart: Infrastructure Home

Checks the owner runs, in order. Each names the spec item it proves. Nothing here puts a value in the repo: names only.

## Part 1. On the pull request (no secrets)

1. The `factory-ci-selftest` check is green. (SC-001, FR-008)
2. `rust-ci-runner/Cargo.lock` is in the diff, and `grep -rn -- '--locked' rust-ci-runner/Makefile .github/workflows` shows every cargo call. (FR-006)
3. `grep -rn 'ubuntu-latest' .github` prints nothing. (FR-003)
4. `grep -rnP '(?<!factory-)monorepo' README.md rust-ci-runner .github` prints nothing. (FR-001)
5. The `shell` job log shows `test-factory-ci-docs.sh` passing. (FR-023)

## Part 2. Setup by hand (README "Setup")

Follow the root README top to bottom: Environments, Bitwarden, Grafana, GitHub tokens, Environment secrets, merge, release tag, public package, variables, mirror images, scheduler.

## Part 3. After Setup (live checks, 002 T046, T054, T065)

1. Dispatch `factory-ci-tick` by hand from `main`. All jobs pass and `verdict` is green. (SC-003)
2. `sudtanj/infrastructure` `main` has `factory-ci` and `factory-ci/rust-ci-runner` statuses, both `success`.
3. `sudtanj/factory-monorepo` has `factory-ci` statuses on its branch tips.
4. Loki holds the step output of a monorepo run, searchable in 30 seconds. The Actions log shows only `item N`. (SC-004)
5. Signed out, open the public run log and search it for a branch name, a SHA, a project path, a secret name and the host of the Loki URL. None appears.
6. Unset `BWS_ACCESS_TOKEN` in `factory-ci`, dispatch a tick: a pipeline with no secrets still runs and ships logs. Restore the token. (SC-005)
7. Unset `FACTORY_CI_LOG_TOKEN` in `factory-ci-plan` only: the tick fails at the start and names it. Restore it.
8. Revoke the Bitwarden token: a pipeline that lists a secret ends in `error` with "secret store rejected the token". (002 T054)
9. A step container cannot reach `169.254.169.254`. (002 T065)
10. Dispatch from a branch other than `main`: `plan` is skipped and no secret is read. (002 T065)
11. Record results in `research.md` under "Live check results".
