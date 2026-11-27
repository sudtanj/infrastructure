# Research: Infrastructure Home

Decisions for `spec.md`. Each has the choice, why, and what was rejected. Live results of the 002 checks (T046, T054, T065) are added under "Live check results" when they run. Review round 1 (critic, devops, security; lead decisions D1 to D9 in the spec) changed D4 to D9 and added D14 to D18.

## D1. Log credentials move to Environment secrets; the store check stays

- **Decision**: `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` are secrets of the three Environments. `.github/scripts/factory-ci-secrets.sh` replaces `fci_log_creds` (reads Bitwarden JSON) with `fci_log_env` (reads the process environment). The plan checks all-or-none at its start. The plan still calls `fci_list` when it has rows and a Bitwarden token, to prove the token works (002 FR-045); it no longer reads log values from the result.
- **Why**: The Rust code already reads `FACTORY_CI_LOG_*` from its environment (`src/main.rs`, `src/job.rs`, `src/ship.rs`). Only the scripts change. The smallest diff keeps 002's reviewed store check.
- **Rejected**: (a) Drop the plan's store check. It saves a call but turns a revoked token from one failed tick into one `error` status per item. (b) Keep Bitwarden as the source and add Actions secrets as a fallback. Two sources make a partial set ambiguous and contradict Principle II.

## D2. A job calls Bitwarden only when the pipeline lists a secret

- **Decision**: `factory-ci-job.sh` skips `fci_list` when `names` is empty.
- **Why**: The list call existed to also fetch the log credentials. Without that, a log-only job needs no store, so a Bitwarden outage no longer blocks it.
- **Rejected**: Keep the call and ignore the result. It adds a failure path for no data.

## D3. Environment copies of the log credentials, two Loki tokens

- **Decision**: All three Environments hold the three secrets. `factory-ci-plan` and `factory-ci` hold one `FACTORY_CI_LOG_TOKEN`; `factory-ci-docker` holds a second token from its own Grafana access policy. URL and user are shared. The plan needs the values for its own log lines and the private-repo rule; each job needs them to ship.
- **Why**: Each job reads only its own Environment, so a run from another branch gets none. The docker Environment is the one whose pipelines get root on the host (002 README "Risks you accept"); a separate token lets the owner cut it off alone. The lead closed OQ-3 this way.
- **Rejected**: Repository-level secrets for the log credentials (they would reach jobs on any branch); one shared token (one escape exposes every log write).

## D4. First compile through CI: a separate bootstrap workflow

- **Decision**: A temporary workflow `.github/workflows/factory-ci-bootstrap.yml` (`pull_request`, same-repo pull requests only through `if: github.event.pull_request.head.repo.full_name == github.repository`, `permissions: contents: read`, `ubuntu-24.04`). `rust-version` is already set in `Cargo.toml`. Steps: `rustup toolchain install`; `generate-lockfile` only when `rust-ci-runner/Cargo.lock` is absent, with `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`; `cargo fmt || true`; `git diff -- 'rust-ci-runner/*.rs' > fmt.patch`; upload with `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02` (the pin the release workflow already uses), `if: always()`, paths `rust-ci-runner/Cargo.lock` and `fmt.patch` only, `include-hidden-files: false`, `retention-days: 3`. The engineer downloads from the run of the same pull request with `gh run download <run-id>`, applies with `git apply --include='rust-ci-runner/*'`, reads the diff, commits. The workflow file is deleted before merge.
- **Why**: The VM has 1 GB of RAM and no rustfmt. CI is the only compiler. A separate file keeps the selftest free of artifacts, secrets and Environments (the existing lint rule `selftest-shell` forbids `upload-artifact` in the selftest), and deleting one file removes the whole risk. Limiting the patch to `.rs` files and the artifact to two paths stops a poisoned diff from touching workflows. A same-repo guard stops a fork from producing what the engineer commits.
- **Rejected**: (a) A `bootstrap` job inside the selftest: it mixes an upload into the gate and conflicts with the lint rule. (b) `docker run rust:1 cargo generate-lockfile` on the VM: Docker is forbidden. (c) A hand-written lock: invalid. (d) Leave out `--locked`: the supply chain stays open (002 R16).
- **Note**: `rustup` 1.28 and later does not install the toolchain named in `rust-toolchain.toml` by itself. Every job that runs cargo first runs `rustup toolchain install` (no argument reads the file). The engineer confirms in the first run.

## D5. `rust-version = "1.90"` follows the toolchain pin

- **Decision**: Add `rust-version = "1.90"` to `Cargo.toml` before the lock is generated. The pinned toolchain stays 1.90.0.
- **Why**: The owner asked for the field in 002 T081 but the value 1.82 was never checked against the dependencies. A value equal to the pin cannot disagree with them. With the field set and `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`, the resolver prefers dependency versions that fit.
- **Rejected**: 1.82: a claim nobody tested. Leaving the field out: the owner asked for it.

## D6. This repo watches itself, without `allow_docker`; engine tests are ignored

- **Decision**: Entries for `sudtanj/factory-monorepo` and `sudtanj/infrastructure`, branches default (all), no `id`, no shared names, no `allow_docker`. Tests that need a real container engine (the critic found only `tests/executor.rs`) get `#[ignore]`. The pipeline runs `cargo test --locked`; the selftest host job runs `cargo test --locked -- --include-ignored`. T010 reads every file in `tests/` and `src/*_tests.rs` to confirm.
- **Why**: Neither repo needs a pipeline secret, so `id` is unneeded. `allow_docker` gives committers root on the runner. The ignore attribute removes the need for a Docker-free allowlist that rots when someone adds a test.
- **Rejected**: A `docker = true` test step (no image holds `cargo` and the Docker CLI; it needs `allow_docker`); an allowlist of test files (a new test file would silently never run in the pipeline).

## D7. Pipeline files for this repo

- **rust-ci-runner/.factory-ci.toml**: steps `lint` (`rustup component add clippy rustfmt`, `cargo fmt --check`, `cargo clippy --locked -- -D warnings`) and `test` (`cargo test --locked`), image `rust:1.90`, in parallel.
- **.factory-ci.toml** (root): steps `shell` (the shell test scripts) and `lint-workflows` (the lint and its self-test), image `ubuntu:24.04`, installing `git`, `jq`, `curl`, `unzip`, `openssl` and `ca-certificates` with apt first, then `git config --global --add safe.directory '*'` because the checkout is mounted from another owner. `own-pipelines` proves the package list.
- **Why**: Root owns files outside top-level project folders (`.github/`, `specs/`, `README.md`). `rust-ci-runner/` is one project (002 US3).
- **Rejected**: One root file for everything: a README edit would rebuild the crate. A fixed file list in the docs test instead of `git`: allowed as the fallback only if `own-pipelines` shows `git` cannot work.

## D8. The selftest runs the pipeline files before merge, in its own job

- **Decision**: A selftest job `own-pipelines` builds the runner (or takes it as an artifact from the build job), sets `FACTORY_CI_SHA`, `FACTORY_CI_EVENT=push` and `FACTORY_CI_BRANCH=selftest` as the fixture step does, sets a separate `CARGO_TARGET_DIR` so the container build does not reuse or corrupt the host build, and runs `run --dir rust-ci-runner` and `run --dir .`.
- **Why**: The tick cannot run these files until after merge, so a typo would fail the first tick. 002 validates the whole file before any step starts; running it is the full check. A job of its own keeps its minutes and failures separate from the build.
- **Cost**: A second compile inside the container, about 5 to 10 minutes of free public-repo time. Accepted.

## D9. Lint rules for workflows

- **Decision**: Extend `.github/scripts/factory-ci-lint-workflow.sh` with: (a) `runs-on-pinned`, over every `*.yml` in the workflow directory, allowing exactly `ubuntu-24.04`; (b) `no-cargo`, over every workflow except `factory-ci-selftest.yml`, `rust-ci-runner-release.yml` and, while it exists, `factory-ci-bootstrap.yml`; (c) `no-pull-request-target` over every workflow, with mutation rows for the block, list (`on: [pull_request_target]`) and map (`on: {pull_request_target: ...}`) forms; (d) selftest rules: it must not contain `workflow_run`, `secrets.`, `environment:` or `id-token`, its `pull_request` trigger has no `paths:`, every checkout has `persist-credentials: false`. The selftest `workflow` job lints the whole directory with `FACTORY_CI_LINT_STRICT=1`. Each rule gets a self-test mutation row.
- **Dropped by the lead**: `log-secrets-scope`. The three-step placement is a grep count in the hardening test (T008), and "step containers never get them" points at the existing `job.rs` test on `LOG_VARS`, or one new case in `tests/job_rules.rs`.
- **Why**: The lint is already the place for workflow rules. It stays line-based (see its `shortcut` note); the flow-style rows cover the known gap.
- **Rejected**: `yq` or `actionlint`: a new dependency for a few rules. A YAML parser in the lint: deferred.

## D10. README and workflow names are checked by a shell test

- **Decision**: A new `.github/scripts/test-factory-ci-docs.sh` extracts `secrets.NAME`, `vars.NAME` and `environment:` names from the workflows with `grep -o`, and the names in the README Setup tables, and compares both ways. It also runs the stale-wording grep (FR-001), the watch-list entry check (FR-002), the `--locked` check (FR-006), README keyword rows (rulesets, CODEOWNERS, `factory-ci-release`, scheduler headers, the GHCR visibility command, the no-`.factory-ci.toml` sentence, the bootstrap-exception sentence, rollback), and the private-identifier scan of committed docs (FR-032).
- **Dropped by the lead**: the docs test's own `ubuntu-latest` grep, no-cargo check and "no workflow names factory-monorepo" check, as duplicates of lint rules. FR-017 rests on `no-cargo`, the directory lint and the README statement.
- **Why**: The constitution says the README lists every name the workflows read. A test keeps it true.
- **Rejected**: Putting the cases in `test-factory-ci-hardening.sh`: it mixes topics.

## D11. Runner image visibility

- **Decision**: The README tells the owner to set the package to public after the first release and gives the check `gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility`. The tick's `fetch runner` step wraps `docker create` and prints `the runner image is not pullable; make the GHCR package public` on failure, in the three jobs.
- **Why**: The tick job has `contents: read`, no `packages` scope and does not log in. A private package fails with a registry error that does not name the fix. The first push of a package is private.
- **Rejected**: Log in with `GITHUB_TOKEN` and `packages: read`: it widens permissions in a job that runs next to the clone token.

## D12. Scheduler and heartbeat

- **Decision**: cron-job.org, 5-minute period, `POST /repos/sudtanj/infrastructure/actions/workflows/factory-ci-tick.yml/dispatches` with headers `Accept: application/vnd.github+json`, `Authorization: Bearer <token>`, `X-GitHub-Api-Version: 2022-11-28`, body `{"ref":"main"}`, expect 204, failure notification on. Fine-grained token: Actions write, this repo only, 90 days, a reminder at day 75. Heartbeat on healthchecks.io, grace 60 minutes or more, `/fail` ping when the plan failed or the tick is red.
- **Why**: GitHub's `schedule` trigger is delayed and is disabled after 60 days without repo activity. 002 chose an external caller.
- **Rejected**: `schedule:` in the tick. The lint forbids it (rule `no-schedule`).

## D13. Constitution PATCH

- **Decision**: 1.0.0 to 1.0.1: "GitHub Actions secrets and variables of this repository, repository-level or in its Environments". No principle changes meaning.
- **Why**: Principle II lists the Bitwarden token and the Grafana credentials as bootstrap secrets and does not name Bitwarden as the only bootstrap store, so the move of the log credentials needs no MINOR bump (lead decision 17). The README has used Environments since 002.

## D14. Release dry run in the selftest

- **Decision**: A selftest job `build-release` runs the release workflow's matrix (both musl targets, same `CC_*` and `CARGO_TARGET_*_LINKER` environment, same packages) with `make build`, stages the binaries as the release workflow does, and runs `docker buildx build` with `push: false` (no login, no `packages` permission). The release workflow gets a `rustup toolchain install` step before `make build`. No `workflow_dispatch` is added to the release workflow.
- **Why**: A release problem should show before the tag, not after it. The first release is also the first image. A dry run needs no secret.
- **Rejected**: `workflow_dispatch` on the release workflow: a dispatch could publish from a non-tag ref unless guarded; the selftest dry run gives the same signal without a publish path.

## D15. Selftest hygiene

- **Decision**: No `paths:` filter, plus `workflow_dispatch`; `concurrency: {group: selftest-${{ github.ref }}, cancel-in-progress: true}`; `persist-credentials: false` on every checkout; no `secrets.`, `environment:`, `workflow_run` or `id-token`. All enforced by the lint (D9).
- **Why**: A path filter lets a change skip the gate it should run (a required check that never runs blocks or lets through). Cancelling older runs saves minutes on the compile loop.

## D16. Release and mirror Environment

- **Decision**: The release `publish` job and the mirror job run in `factory-ci-release`: no secrets, deployment branches limited to `main` and the tag pattern `rust-ci-runner-v*`. The mirror job adds `if: github.ref == 'refs/heads/main'`.
- **Why**: Publishing to GHCR with `packages: write` is the one write path to a public artifact the tick later runs. An Environment gives it a protection point (restricted refs, reviewers if the owner adds them).

## D17. Compile loop budget

- **Decision**: 5 selftest runs, then the engineer stops and reports so the plan can be revisited. A `check` job (`cargo check --locked --all-targets`) gives compile errors in about a minute before tests start.
- **Why**: No local build exists; an open-ended loop hides a design problem.

## D18. Deferred (no task in 003)

Recorded in `plan.md` Open risks: replace the third-party `alpine/git` clone image with one built in-repo; container escape reads the runner environment (narrow tokens and `allow_docker` off now); `cargo audit` or `cargo deny`; build provenance attestation; a YAML-parsing lint; splitting the clone token from the status token; a test that fetched file content never reaches the summary `problems` text (security L1), if cheap.

## Test classification

Read by the lead for T010. "Engine" means the test starts real containers through the `docker` CLI. Only `tests/executor.rs` does; its 12 tests carry `#[ignore = "needs a container engine"]`. The runner pipeline (`cargo test --locked`, no engine in the step container) passed every other test in selftest run 45961039214, which is the evidence for the "no engine" rows.

| File | Needs an engine | Why |
| --- | --- | --- |
| `tests/executor.rs` | yes, ignored | Runs step containers, networks and volumes for real |
| `tests/argv.rs` | no | Builds `docker run` argument lists and asserts on them |
| `tests/cli.rs` | no | Stub `docker` first on `PATH` (`tests/common/mod.rs`) |
| `tests/cli_logs.rs` | no | Stub `docker` and a fake log server |
| `tests/job_prepare.rs` | no | Pipeline parsing and validation only |
| `tests/job_rules.rs` | no | Stub `docker` on `PATH` |
| `tests/no_vendor.rs` | no | Reads source files |
| `tests/ship_job.rs` | no | Fake log server |
| `src/github_tests.rs` | no | Fake GitHub server |
| `src/job_tests.rs` | no | In-process |
| `src/plan_cap_tests.rs` | no | Fake GitHub API |
| `src/plan_error_tests.rs` | no | Fake GitHub API |
| `src/plan_hardening_tests.rs` | no | Fake GitHub API |
| `src/plan_race_tests.rs` | no | Fake GitHub API |
| `src/plan_tests.rs` | no | Fake GitHub API |
| `src/projects_tests.rs` | no | In-process |
| `src/secret_names_tests.rs` | no | In-process |
| `src/ship_tests.rs` | no | Fake log server |
| `src/text_tests.rs` | no | In-process |
| `src/walk_tests.rs` | no | Temporary directories |
| `src/watch_tests.rs` | no | TOML parsing |

## Live check results

Filled by T031 to T035. Write results as pass or fail with a one-line cause. Never paste a run URL of a private repo, a Grafana hostname, a token or a key (the docs test scans this file, FR-032).
