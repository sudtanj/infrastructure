# Quickstart: Infrastructure Home

Checks the owner runs, in order. Each names the spec item it proves. Nothing here puts a value in the repo: names only. Results go in `research.md` without run URLs of private repos, Grafana hostnames or any token (FR-032).

## Part 1. On the pull request (no secrets)

1. Every selftest job is green: `check`, `pipeline`, `build-release`, `own-pipelines`, `list`, `workflow`, `shell`. (SC-001, FR-008, FR-028)
2. `rust-ci-runner/Cargo.lock` is in the diff, `Cargo.toml` holds `rust-version = "1.90"`, and `factory-ci-bootstrap.yml` is not in the tree. (FR-006, FR-007, FR-009)
3. `grep -rn 'ubuntu-latest' .github` prints nothing. (FR-003)
4. `grep -rnP '(?<!factory-)monorepo' README.md rust-ci-runner .github` prints nothing. (FR-001)
5. The `shell` job log shows `test-factory-ci-docs.sh` passing. (FR-023, FR-032)

## Part 2. Setup by hand (README "Setup")

Follow the root README top to bottom: repository rules, the Environment, the Bitwarden project and machine account, the tokens, the first Bitwarden secrets, merge, release tag, public package, mirror images (their names go into Bitwarden), scheduler, first manual tick. After Setup, `gh secret list` (repo and org) shows nothing and `gh secret list --env factory-ci` shows exactly `BWS_ACCESS_TOKEN`. (FR-022, SC-007)

## Part 3. After Setup (live checks, 002 T046, T054, T065)

Checks 1 to 5, 8 and 9 on factory-monorepo need its PR 2 merged (the root `.factory-ci.toml` on its `main`). Until then the plan skips that repo, which is itself check 12.

1. Dispatch `factory-ci-tick` by hand from `main`. All jobs pass and `verdict` is green. (SC-003)
2. `sudtanj/infrastructure` `main` has `factory-ci` and `factory-ci/rust-ci-runner` statuses, both `success`.
3. `sudtanj/factory-monorepo` has `factory-ci` statuses on its branch tips. (needs PR 2)
4. Loki holds the step output of a monorepo run, searchable in 30 seconds. The Actions log shows only `item N`. (SC-004, needs PR 2)
5. Signed out, open the public run log and search it for a branch name, a SHA, a project path, a secret name and the host of the Loki URL. None appears.
6. Unset `BWS_ACCESS_TOKEN` in `factory-ci`, dispatch a tick: `plan` stops at its load step with the fixed "BWS_ACCESS_TOKEN is not set" line and `verdict` is red. Restore the token. (SC-005)
7. Delete `FACTORY_CI_LOG_TOKEN` from Bitwarden, leaving the other two log keys: the tick fails at the start and names it. Restore it.
8. Revoke the Bitwarden token: `plan` stops with "secret store rejected the token". With a valid token and a pipeline that lists a secret Bitwarden lacks, the item ends in `error`. (002 T054)
9. A step container cannot reach `169.254.169.254`, and the DROP rules cover IPv6 too. (002 T065)
10. Dispatch from a branch other than `main`: every tick job is skipped by its main-only `if:`, no Environment is entered and no secret is read. (002 T065, FR-058)
11. Set `FACTORY_CI_BWS_SERVER_URL` to an `http://` value on a scratch run: the tick refuses it before any download. Unset it. (FR-031)
12. With a watched repo that has no `.factory-ci.toml`, a tick posts nothing for it and does not fail. (FR-017)
13. `gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility` prints `public`, and the release digest came from a run on `main`. (FR-004, FR-022)
14. Rate: record how many `bws` calls a manual tick makes (two, plus one per item whose pipeline lists a secret) and whether any run saw a 429 or a retry. (R-14, T066)
15. `FCI_RUNNER_IMAGE_DIGEST` reaches the `fetch runner` step: the first tick pulls the image. Revoke a scratch token and record the real `bws` stderr. If you set a heartbeat URL for a test, a green tick sends the plain ping from `verdict`; leave it unset for manual runs. (D27, D30, T066)
16. `gh api repos/sudtanj/infrastructure/environments/factory-ci` shows `custom_branch_policies` true, `protected_branches` false, `can_admins_bypass` false; its branch policies are exactly `main`; `gh secret list` and `gh secret list --org <org>` print nothing; the `main` ruleset matches the README. Run before the first tick and again after checks 6 to 8. (FR-066, T070)
17. Record results in `research.md` under "Live check results". Checks 1 to 5 and 8 pass and some days of ticks are green before the owner deletes the old Environments and secrets (migration note in the README).
