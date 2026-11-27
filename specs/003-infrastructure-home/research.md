# Research: Infrastructure Home

Decisions for `spec.md`. Each has the choice, why, and what was rejected. Live results of the 002 checks (T046, T054, T065) are added under "Live check results" when they run.

## D1. Log credentials move to Environment secrets; the store check stays

- **Decision**: `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` are secrets of the three Environments. `.github/scripts/factory-ci-secrets.sh` replaces `fci_log_creds` (reads Bitwarden JSON) with `fci_log_env` (reads the process environment). The plan checks all-or-none at its start. The plan still calls `fci_list` when it has rows and a Bitwarden token, to prove the token works (002 FR-045); it no longer reads log values from the result.
- **Why**: The Rust code already reads `FACTORY_CI_LOG_*` from its environment (`src/main.rs`, `src/job.rs`, `src/ship.rs`). Only the scripts change. The smallest diff keeps 002's reviewed store check.
- **Rejected**: (a) Drop the plan's store check. It saves a call but turns a revoked token from one failed tick into one `error` status per item. (b) Keep Bitwarden as the source and add Actions secrets as a fallback. Two sources make a partial set ambiguous and contradict Principle II.

## D2. A job calls Bitwarden only when the pipeline lists a secret

- **Decision**: `factory-ci-job.sh` skips `fci_list` when `names` is empty.
- **Why**: The list call existed to also fetch the log credentials. Without that, a log-only job needs no store, so a Bitwarden outage no longer blocks it.
- **Rejected**: Keep the call and ignore the result. It adds a failure path for no data.

## D3. Environment copies of the log credentials

- **Decision**: All three Environments hold the same three secrets. The plan needs them for its own log lines and the private-repo rule; each job needs them to ship.
- **Why**: Each job reads only its own Environment, so a run from another branch gets none. Environment secrets are the only per-job scope Actions offers.
- **Rejected**: Repository-level secrets for the log credentials. They would reach jobs on any branch.

## D4. First compile through CI, with a lock artifact

- **Decision**: A temporary selftest job `bootstrap` runs `cargo generate-lockfile` and `cargo fmt`, writes `fmt.patch` (a `git diff`), and uploads `Cargo.lock` and `fmt.patch` with `actions/upload-artifact` pinned by SHA. The engineer downloads it with `gh run download`, commits the lock, applies the patch, then deletes the job.
- **Why**: The VM has 1 GB of RAM and no rustfmt. CI is the only compiler.
- **Rejected**: (a) `docker run rust:1 cargo generate-lockfile` on the VM: Docker is forbidden here. (b) Commit a hand-written lock: invalid. (c) Leave out `--locked`: the supply chain stays open (002 R16).
- **Note**: `rustup` 1.28 and later does not install the toolchain named in `rust-toolchain.toml` by itself. The selftest runs `rustup toolchain install` (no argument reads the file) before cargo. The engineer confirms in the first run.

## D5. `rust-version = "1.82"` is a declaration

- **Decision**: Add it as 002 T081 asks. The pinned toolchain stays 1.90.0.
- **Why**: Cargo does not check dependency versions against the crate's own `rust-version` when the running compiler is newer. If a dependency needs a newer rustc than 1.90 the build fails in the compile loop and the engineer pins that dependency in `Cargo.lock`.

## D6. This repo watches itself, without `allow_docker`

- **Decision**: Entries for `sudtanj/factory-monorepo` and `sudtanj/infrastructure`, branches default (all), no `id`, no `secrets`, no `allow_docker`.
- **Why**: Neither repo needs a pipeline secret today, so `id` is unneeded. `allow_docker` gives committers root on the runner (002 README "Risks you accept"); a public repo gains nothing from it here.
- **Consequence**: Tests that need real Docker (`tests/executor.rs`, probably `tests/argv.rs`) cannot run inside a step container. The selftest workflow runs the full `cargo test --locked` on the Actions host (bootstrap). The pipeline file runs `--lib` and the integration test files that use a stub `docker`. The engineer classifies each `tests/*.rs` file by reading it (T008) and records the list in this file under "Test classification".
- **Rejected**: A `docker = true` test step. No image holds both `cargo` and the Docker CLI, and it needs `allow_docker`.

## D7. Pipeline files for this repo

- **rust-ci-runner/.factory-ci.toml**: steps `lint` (`rustup component add clippy rustfmt`, `cargo fmt --check`, `cargo clippy --locked -- -D warnings`) and `test` (`cargo test --locked --lib` plus the Docker-free test files), image `rust:1.90`, `test` after nothing (parallel). 
- **.factory-ci.toml** (root): steps `shell` (the two shell test scripts) and `lint-workflows` (the lint and its self-test), image `ubuntu:24.04`, installing `jq`, `curl`, `unzip` and `openssl` with apt first (the script list decides; the engineer reads the scripts).
- **Why**: Root owns files outside top-level project folders (`.github/`, `specs/`, `README.md`). `rust-ci-runner/` is one project (002 US3).
- **Rejected**: One root file for everything. A README edit would rebuild the crate.

## D8. The selftest runs the pipeline files before merge

- **Decision**: A selftest job `own-pipelines` runs `rust-ci-runner run --dir rust-ci-runner` and `run --dir .` with the built binary on the Actions host. It needs Docker, which the host has.
- **Why**: The tick cannot run these files until after merge, so a typo would fail the first tick. 002 already validates the whole file before any step starts; running it is the full check.
- **Cost**: A second compile inside the container, about 5 to 10 minutes of free public-repo time. Accepted.

## D9. Lint rules for workflows

- **Decision**: Extend `.github/scripts/factory-ci-lint-workflow.sh` with (a) `runs-on-pinned` over every `*.yml` in the workflow directory, (b) `no-cargo` over every workflow except `factory-ci-selftest.yml` and `rust-ci-runner-release.yml`, (c) `log-secrets-scope`: `secrets.FACTORY_CI_LOG_*` appear only in the `plan` step `plan` and the `job` steps, never in `status fallback`, `fetch runner` or `install bws`. Each gets a self-test mutation row.
- **Why**: These are the rules the spec makes testable. The lint is already the place for workflow rules (line-based, see its `shortcut` note).
- **Rejected**: `yq` or `actionlint`: a new dependency for three rules.

## D10. README and workflow names are checked by a shell test

- **Decision**: A new case file `.github/scripts/test-factory-ci-docs.sh` extracts `secrets.NAME` and `vars.NAME` from the workflows with `grep -o`, and the names in the README Setup tables, and compares both ways. It also runs the stale-wording grep (FR-001), the watch-list entry check (FR-002) and the `ubuntu-latest` grep (FR-003, belt and braces next to the lint).
- **Why**: The constitution says the README lists every name the workflows read. A test keeps it true.
- **Rejected**: Adding the cases to `test-factory-ci-hardening.sh`: it is 309 lines already and mixes topics.

## D11. Runner image visibility

- **Decision**: The README tells the owner to set the package to public after the first release. The tick's `fetch runner` step wraps `docker create` and prints `the runner image is not pullable; make the GHCR package public` on failure, in the three jobs.
- **Why**: The tick job has `contents: read` and no `packages` scope and does not log in. A private package fails with a registry error that does not name the fix.
- **Rejected**: Log in with `GITHUB_TOKEN` and `packages: read`. It works for a private package but widens permissions in a job that runs next to private tokens (Principle: minimum permissions); public is simpler.

## D12. Scheduler

- **Decision**: External HTTP scheduler, 5-minute period, `POST /repos/sudtanj/infrastructure/actions/workflows/factory-ci-tick.yml/dispatches` with `{"ref":"main"}` and a fine-grained token (Actions: write, this repo only, 90 days). Default service: cron-job.org (OQ-1).
- **Why**: GitHub's `schedule` trigger is delayed and is disabled after 60 days without repo activity. 002 chose an external caller.
- **Rejected**: `schedule:` in the tick. The lint forbids it (rule `no-schedule`).

## D13. Constitution PATCH

- **Decision**: 1.0.0 to 1.0.1: "GitHub Actions secrets and variables of this repository, repository-level or in its Environments". No principle changes meaning.
- **Why**: The README has used Environments since 002, and Environment secrets are how Principle II's "nowhere else" holds for branches other than `main`.

## Test classification

Filled by T008.

## Live check results

Filled by T029 to T033.
