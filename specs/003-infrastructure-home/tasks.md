# Tasks: Infrastructure Home

**Input**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: required by the constitution (Principle V). Test tasks come first in their phase and fail until the matching impl task lands.

**Format**: `- [ ] T### [P?] [US#] description with path`. `[P]` means the task touches different files from every other open task of its group and can run in parallel with them. Paths are from the repo root. Review round 1 (critic, devops, security; lead decisions in `spec.md`) renumbered every task; no ID from the first draft survives. Review round 2 adds T036 to T044 in Phase 7b.

## Common constraints (every task and brief)

- C1. No local `cargo`, `docker`, `make` or test run: the machine has 1 GB of RAM. Read, write, `grep`, `bash -n`, `git` and `gh` only. Re-read each file you change. List in the report what you could not run. CI is the compiler.
- C2. No new dependency, no new action without a SHA pin (reuse the pins already in the repo), no new file outside the plan's list.
- C3. Follow `feiyi-anti-slop-code`: comments only for a reason the code cannot show; match the style of the file you edit.
- C4. Never print a secret, token or log credential. Never use `set -x` in a script that holds one. Docs and specs name a secret by key, never by value, and never hold a private repo's run URL or a Grafana hostname.
- C5. Prose in docs and comments follows `stop-slop`: no em dashes, no filler.
- C6. Use explicit `git add` paths, never `-a`. Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- C7. Implement with `/speckit-implement`, only the tasks named in your brief. Work not listed goes back through spec, plan and tasks.

## Phases and order

Phase 1 is the gate: nothing else can be checked until the crate compiles. Phase 2 (tests) runs in parallel with Phase 1 after T001, except T010, which edits Rust test files and waits for T005. Phases 3 to 7 are impl; files shared between tasks run in the order of the task IDs. Phase 7b (review round 2) comes before Phase 8 closes the pull request. Phase 9 is blocked on the owner's Setup (and, for the monorepo checks, on factory-monorepo PR 2) and stays open after merge.

---

## Phase 1: First compile (US2)

**Goal**: a green selftest with a committed lock file. **Independent test**: the selftest checks on the PR.

- [X] T001 [US2] Selftest preparation (brief S1) in `.github/workflows/factory-ci-selftest.yml` and `rust-ci-runner/Cargo.toml` (FR-007, FR-008, FR-020, FR-029; D5, D15, D17): add `rust-version = "1.90"` to `Cargo.toml` first; in the selftest drop the `paths:` filter, add `workflow_dispatch`, add `concurrency: {group: selftest-${{ github.ref }}, cancel-in-progress: true}`, set `persist-credentials: false` on every checkout; add `rustup toolchain install` before cargo in every job that runs it; add a `check` job (`cargo check --all-targets`; T004 adds `--locked`); add `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test -- --include-ignored` to the `pipeline` job; no `secrets.`, `environment:`, `workflow_run`, `id-token` or `upload-artifact` in the file; put the bootstrap-exception comment in the header. Proof: the YAML reads back clean; the run in T003 starts.
- [X] T002 [US2] New `.github/workflows/factory-ci-bootstrap.yml` (temporary; FR-009; research D4): `pull_request`, one `ubuntu-24.04` job guarded by `if: github.event.pull_request.head.repo.full_name == github.repository`, `permissions: contents: read`, no Environment, no secret; `rustup toolchain install`; `generate-lockfile` only when `rust-ci-runner/Cargo.lock` is absent, with `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`; `cargo fmt || true`; `git diff -- 'rust-ci-runner/*.rs' > fmt.patch`; upload with `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02` (the pin the release workflow uses), `if: always()`, paths `rust-ci-runner/Cargo.lock` and `fmt.patch` only, `include-hidden-files: false`, `retention-days: 3`. Header comment says it is the bootstrap exception and is deleted by T028. Proof: the YAML reads back clean.
- [X] T003 [US2] Push the branch and open the pull request as a draft. Wait for the run of this pull request. Download the artifact with `gh run download <run-id>` from that run only (FR-009). Read `fmt.patch` and the lock diff before applying anything. If the bootstrap job fails, fix it and rerun. Proof: `Cargo.lock` and `fmt.patch` are on the engineer's disk and the diff was read; the task notes name the run ID. Notes: first bootstrap run 45952679858 (artifact read, lock and fmt.patch applied; later bootstrap runs gave an empty patch).
- [X] T004 [US2] Land the lock (FR-006, FR-009; 002 T081): commit `rust-ci-runner/Cargo.lock`; apply the patch with `git apply --include='rust-ci-runner/*' fmt.patch`; add `--locked` to every cargo call that accepts it in `rust-ci-runner/Makefile` (`fetch`, `test`, `clippy`, `build`), the selftest (including the `check` job) and `.github/workflows/rust-ci-runner-release.yml`. Lock and `--locked` land in one commit. Proof: `grep -n cargo` on the three files shows `--locked` on every build, test, clippy, check and fetch line (`fmt` stays without it).
- [X] T005 [US2] Compile-fix loop (FR-008; 002 T045): push, read `gh run view --log-failed`, fix every error in the log at its source, never by loosening or deleting a test, push again. Budget: 5 selftest runs; after the fifth red run, stop and report so the plan is revisited. After each Rust fix, take the fresh `fmt.patch` from the bootstrap run and apply it the same way. Record each class of failure in the task notes. Proof: every selftest job green, `cargo fmt --check` included. Notes (open, budget spent after 5 red selftest runs, 45952679834, 45952999801, 45953515782, 45953881768, 45954341009): E0308 array size in `tests/executor.rs`; unused `std::io::Read` in 4 test files; clippy `type_complexity` in `job.rs`; `job_rules` test expected the unsanitized comma (FR-051 sanitizer turns it into `_`); aarch64 `build-release` needed `libc6-dev-arm64-cross`. Still red: last step "no containers, networks or volumes are left" after `cargo test` ran in the same job. Budget extended by 3 selftest runs (coordinator): cause of the leak is a runner bug in `engine.rs` `run_logged`, where a timeout during container creation killed the client before the daemon finished the create and left a `Created` container; fix is `stop_and_remove`: `rm -f` the container repeatedly while the client is still alive (it exits only when the daemon is done), client killed after 30 s. A first version that killed the client first and polled `inspect` for 5 s still leaked (run 45956119997). Result: run 45956696628 green for `pipeline`, `check`, `list`, `workflow` and both `build-release` jobs (fmt patch empty); `shell` red only on the test-first scripts of T007 to T009. Deviation (R2-8): the compile-loop budget of D9 was extended to 8 selftest runs in total (5, then 3 by the coordinator).
- [X] T006 [US2] Release dry run (FR-028; D14): add a selftest job `build-release` with the release workflow's matrix (both musl targets, same packages, same `CC_*` and `CARGO_TARGET_*_LINKER` environment), `make build`, stage the binaries as the release workflow does, `docker buildx build` with `push: false` using the pinned `docker/setup-buildx-action` and no login; add `rustup toolchain install` before `make build` in `.github/workflows/rust-ci-runner-release.yml`; add no `workflow_dispatch` to the release workflow. Proof: green `build-release` job.

**Checkpoint**: the selftest is green with `--locked`.

## Phase 2: Tests first (parallel, after T001)

- [X] T007 [P] [US3] Tests in `.github/scripts/test-factory-ci-secrets.sh` for `fci_log_env` (FR-010 to FR-012; I1, P1, P2): three set gives `on` and the three values; none gives `off`; every other subset returns 1 and `FCI_ERR` names exactly the empty variables; a 3-character line is refused by the callers' short-line check; `FCI_ERR` never contains a value (sentinels). Remove the cases for `fci_log_creds`. Fails until T011.
- [X] T008 [P] [US3] Tests in `.github/scripts/test-factory-ci-hardening.sh` (a fake `rust-ci-runner`, fake `bws`, `GITHUB_OUTPUT` and summary files) for FR-004, FR-010 to FR-015, FR-031 (P2, P3, P7, I2 to I4): the plan fails before the first runner call when one or two log variables are set, and names them; the plan with all three set and no Bitwarden token exports them to phase C; `job` with a no-secret pipeline never calls `bws`, even with `BWS_ACCESS_TOKEN` set; `job` with a partial set refuses with "log credentials incomplete"; with the Bitwarden token unset and a GitHub secret of the pipeline's secret name set, the item ends in `error` (no fallback); sentinel values in every bootstrap variable (with `%`, a newline and a 3-character line among them) appear in no stdout, stderr, summary or output file except in `::add-mask::` lines; `grep -c 'secrets.FACTORY_CI_LOG_'` on the tick workflow is 9, in the `plan` step `plan` and the two `job` steps; step containers never get the log variables (name the existing `job.rs` or `tests/job_rules.rs` test that covers `LOG_VARS`, or add one case in `tests/job_rules.rs`); the three `fetch runner` steps carry the fixed "make the GHCR package public" message; the three `install bws` steps and both scripts refuse a `FACTORY_CI_BWS_SERVER_URL` that does not start with `https://`. Fails until T012 to T014.
- [X] T009 [P] [US1] [US6] New `.github/scripts/test-factory-ci-docs.sh`, same `eq` style as the hardening test (FR-001, FR-002, FR-006, FR-007, FR-016, FR-022 to FR-024, FR-032; P6): (a) no `(?<!factory-)monorepo` and no "separate public copy" in tracked files outside `specs/`, `.specify/`, `.claude/`, and no standalone `B` as the central repo's name in `README.md`, `rust-ci-runner/README.md` and `.github/` only (Rust test data uses "B" legitimately); build the patterns in the script so it does not match itself; (b) `.github/factory-ci-repos.toml` lists both repos; (c) every `cargo build|test|clippy|check|fetch` line of the Makefile, the selftest, the release workflow and both pipeline files has `--locked`, and `Cargo.toml` holds `rust-version = "1.90"`; (d) the set of `secrets.X`, `vars.X` names (minus `GITHUB_TOKEN`) and `environment:` names in `.github/workflows/*.yml` equals the set in the root README Setup tables, both directions, each secret or variable in a row with an Environment or scope; (e) README keyword rows: the bootstrap-exception sentence, "public" next to the GHCR package and the visibility command, both repo names next to `FACTORY_CI_TOKEN`, `factory-ci-release`, the main ruleset, CODEOWNERS, the tag ruleset, the scheduler headers and body, the heartbeat grace, the no-`.factory-ci.toml` sentence, the rollback list, the `gh secret list` check; (f) `rust-ci-runner/README.md` has no "Set up B"; (g) a scan of the README files, `research.md` and `quickstart.md` for private identifiers: no `github.com/sudtanj/factory-monorepo/actions` URL, no `grafana.net`, no token or key pattern (`ghp_`, `github_pat_`, `glc_`, `BEGIN .*PRIVATE KEY`). Mutation self-check (copy a fixture tree, break one rule, expect a named failure) for (a), (d) and (g). Fails until T016 to T025.
- [X] T010 [P] [US5] Test classification (FR-030; research D6): read every file in `rust-ci-runner/tests/*.rs` and every `src/*_tests.rs` and find the tests that need a real container engine (the critic found `tests/executor.rs`; check `argv.rs`, `job_prepare.rs`, `job_rules.rs`, `cli.rs` for a stub `docker` on `PATH` versus the real one). Add `#[ignore]` with a reason string to each such test. Write the table under "Test classification" in `research.md`. Needs T005 (it edits Rust test files after the compile loop). Proof: every file appears once; the selftest `pipeline` job (with `--include-ignored`) stays green; `cargo test --locked` in the root of a container without an engine is exercised by T022.

## Phase 3: Bootstrap secrets from Actions (US3)

**Goal**: log credentials from Environment secrets, no Bitwarden for logs. **Independent test**: T007 and T008 pass in the `shell` job.

- [X] T011 [US3] `.github/scripts/factory-ci-secrets.sh` (contracts/bootstrap-secrets.md; FR-010 to FR-012): replace `fci_log_creds` by `fci_log_env`; update the file header and the comment on `FCI_HAVE_LOGS`. Proof: T007 passes; `grep -n fci_log_creds .github` prints nothing.
- [X] T012 [US3] `.github/scripts/factory-ci-plan.sh` (FR-010 to FR-012, FR-031): call `fci_log_env` after the blob-key check and `fail "$FCI_ERR"` on error; refuse a `FACTORY_CI_BWS_SERVER_URL` that is set and does not start with `https://`; keep the `fci_list` calls; delete the two `fci_log_creds` calls and the `have` logic; set, check and mask the log values whenever `FCI_HAVE_LOGS` is set and rows exist; update the header. Needs T011. Proof: T008 plan cases pass.
- [X] T013 [US3] `.github/scripts/factory-ci-job.sh` (FR-010 to FR-013, FR-031): `fci_log_env` first with `refuse "$FCI_ERR"`; the same `https://` refusal before the first `bws` call; call `fci_list` only when `names` is not empty; keep the short-line check, the `fail_closed` rule and masking. Needs T011. Proof: T008 job cases pass; `bash -n`.
- [X] T014 [US3] `.github/workflows/factory-ci-tick.yml` (contracts/bootstrap-secrets.md; FR-004, FR-010, FR-031): add `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` from `secrets.*` to the `env` of the `plan` step `plan`, and of the `job` step in `run` and `run-docker`, and to no other step; replace the bare `docker create` in the three `fetch runner` steps with the guarded form and its fixed message; add the `https://` guard to the three `install bws` steps. Proof: T008 workflow cases; the lint passes after T015.

## Phase 4: Self-consistency (US1)

**Goal**: no leftover from the move; pinned runners; the watch list names both repos; the lint keeps it so.

- [X] T015 [US1] [US5] `.github/scripts/factory-ci-lint-workflow.sh` (FR-003, FR-020, FR-027, FR-029; research D9): add `runs-on-pinned` (every `*.yml` in the directory of the given workflow, exactly `ubuntu-24.04`), `no-cargo` (every workflow except the selftest, the release workflow and, while it exists, `factory-ci-bootstrap.yml`), `no-pull-request-target` over every workflow, and the selftest rules (no `workflow_run`, `secrets.`, `environment:`, `id-token`; no `paths:` under `pull_request`; `persist-credentials: false` on every checkout); add a mutation row to the self-test for each, including the flow-style forms `on: [pull_request_target]` and `on: {pull_request_target: ...}`; keep the `shortcut` note. Proof: the lint passes on the repo; `--self-test` passes and each new rule fails on its mutated copy.
- [X] T016 [US1] [US2] Pin and lint the directory (FR-003, FR-029): `ubuntu-24.04` in `.github/workflows/factory-ci-selftest.yml` (1 place) and `.github/workflows/rust-ci-runner-release.yml` (3 places); the selftest `workflow` job lints the whole `.github/workflows/` directory with `FACTORY_CI_LINT_STRICT=1`. Needs T004, T006 and T015. Proof: `grep -rn 'ubuntu-latest' .github` prints nothing; green `workflow` job.
- [X] T017 [US1] `.github/factory-ci-repos.toml` (FR-002, FR-017): add the two entries of `contracts/pipeline-files.md`, keep the header comment. Proof: the selftest `list` job (`plan --check`); T009 (b).
- [X] T018 [US1] Wording (FR-001, FR-005), excluding the two READMEs: comments and strings in `.github/workflows/factory-ci-tick.yml` ("B is public" becomes "this repo is public"), `.github/scripts/factory-ci-plan.sh` ("B's checkout"), the other scripts, the lint header, `rust-ci-runner/tests/fixtures/selftest/.factory-ci.toml` if it needs it; `grep -rnP '(?<!factory-)monorepo|\bB\b'` over `.github` and `rust-ci-runner` (outside Rust test data) decides what to fix. In `.github/scripts/test-factory-ci-hardening.sh` point the README checks at the root `README.md`. Proof: T009 (a) passes for these paths.
- [X] T019 [US1] Release and mirror (FR-004, FR-033): `.github/workflows/rust-ci-runner-release.yml` `publish` job gets `environment: factory-ci-release`, and the digest summary step adds the line "Make the GHCR package public once, then set RUNNER_IMAGE_DIGEST"; `.github/workflows/factory-ci-mirror-images.yml` `mirror` job gets `environment: factory-ci-release` and `if: github.ref == 'refs/heads/main'`. Needs T016. Proof: the lint passes; T009 (d) once T024 lands. The heartbeat note moved to T036.

## Phase 5: The runner tests itself (US5)

- [X] T020 [US5] `rust-ci-runner/.factory-ci.toml` (FR-018, FR-019, FR-030): steps `lint` and `test` as in `contracts/pipeline-files.md`; `test` runs `cargo test --locked` in three processes (all but `ship_tests` and `job_tests`, then each of the two), because the `tiny_http` mock servers exceed the 512 pids limit of a step container in one process (commit 943e73e); `lint` has `timeout = 1200` and `test` `timeout = 1800`. Needs T010. Proof: T022's job runs it.
- [X] T021 [US5] `.factory-ci.toml` at the repo root (FR-018, FR-019): steps `shell` and `lint-workflows` with `git` in the package list and `git config --global --add safe.directory '*'`; read the scripts for the package list; a comment on any test that needs the host and stays out. Needs T007 to T009 for the script names. Proof: T022's job runs it.
- [X] T022 [US5] `.github/workflows/factory-ci-selftest.yml` (FR-021, FR-030; D8): add the `own-pipelines` job (builds the runner or takes it as an artifact from `pipeline`; sets `FACTORY_CI_SHA`, `FACTORY_CI_EVENT=push`, `FACTORY_CI_BRANCH=selftest`; separate `CARGO_TARGET_DIR`) running `rust-ci-runner run --dir rust-ci-runner` and `run --dir .`; add a step that downloads the pinned `bws` archive and compares its SHA-256 with `BWS_SHA256` (002 T054, CI half). Needs T020, T021 and T016. Proof: green selftest; the log shows `cargo test --locked` passing in the container without an engine and the root pipeline passing with `git`.
- [X] T023 [US5] State the bootstrap exception (FR-020): one paragraph in the header comment of `.github/workflows/rust-ci-runner-release.yml` (the selftest and bootstrap headers are done in T001 and T002); the README text lives in T024; this task checks the headers and the README agree. Needs T024. Proof: T009 (e).

## Phase 6: README Setup (US6)

- [X] T024 [US6] Root `README.md` (FR-010, FR-016, FR-017, FR-020, FR-022, FR-023; `data-model.md` tables, `quickstart.md` Part 2): rewrite Setup. Keep the one-paragraph intro and Docs list (point to `specs/003-infrastructure-home/`). Tables of secrets by Environment (the matrix in `data-model.md`, two Loki tokens) and variables with scope; the four Environments; then every item of FR-022 in its order: repository rules, Environments, accounts and tokens, secrets, merge, release tag, public package and its check, variables, mirror, scheduler with headers and body, heartbeat, first manual tick, `gh secret list` check, the no-`.factory-ci.toml` behaviour, the late deletion of the old Bitwarden log keys, rollback with what a revert does not restore, the notes (deprecation notices, Docker Hub limits, cost). Add the bootstrap-exception sentence. Remove the sentence that says the log credentials are not read from GitHub. Keep the lines the hardening test reads: `RUNNER_IMAGE_DIGEST` without an `@`, `sha256:<64 hex>`, and the first `FACTORY_CI_BLOB_KEY_DOCKER` line marked "optional". Needs T014 and T019 for the names. Proof: T009 (d), (e), (g).
- [X] T025 [US6] `rust-ci-runner/README.md` (FR-001, FR-024): replace "Set up B" by a pointer to the root README Setup; change "B" to "this repo" everywhere; "Secrets" and "Log shipping" say the log credentials come from Environment secrets, with the rollback text updated (removing the Bitwarden token no longer stops log shipping); keep runner behaviour, commands and the pipeline format. Needs T024. Proof: T009 (a), (f).

## Phase 7: Governance and 002 pointers

- [X] T026 Constitution PATCH 1.0.1 (FR-026, research D13): `.specify/memory/constitution.md` Principle II and "Repository Structure and Constraints" say "GitHub Actions secrets and variables of this repository, repository-level or in its Environments"; bump `Version` to 1.0.1 and `Last Amended` to the date of the change; the PR body states the reason and lists the README and workflows as the components that comply. Proof: grep of the version line.
- [X] T027 [US7] 002 pointers (FR-025): in `specs/002-ci-runner/tasks.md` mark T045 and T081 as done by 003 T005 and T004, and T046, T054 and T065 as moved to 003 T032, T033 and T034, each with a one-line note (edit only those lines and the status note at the top of the file); add a short "Superseded in part by 003" block to the top of `specs/002-ci-runner/spec.md` listing the rows of the table in 003 `spec.md`. Proof: grep for each ID shows the pointer.

## Phase 7b: Review round 2 (before T029 and T030; decisions R2-1 to R2-9 in the spec)

**Goal**: the PR review findings, each as a task with its files, FR and proof. Test rows come first inside each shell task and fail until the change lands. Shell tests may run on the VM with a stand-in `openssl` (a script on `PATH` that echoes a fixed ciphertext and accepts `-pass env:`); nothing else runs locally.

- [X] T036 [US6] Heartbeat requirement (FR-034; R2-1; `contracts/tick-workflow.md`): the `verdict` job pings `<url>/fail` on a red tick, a failed or cancelled plan, or `fail_tick=1`, and the plain URL otherwise. Files: `.github/workflows/factory-ci-tick.yml`, `.github/scripts/factory-ci-lint-workflow.sh` (rule `verdict-ping`), `.github/scripts/test-factory-ci-hardening.sh`, `specs/003-infrastructure-home/contracts/tick-workflow.md`. Already done: the engineer's tick change plus QA's 13 rows in commit aa6a307 (fake `curl`: green, fail, idle, off-main, no URL, curl error). Moved out of T019. Proof: the `shell` job is green on the hardening test.
- [X] T037 [US6] README heartbeat text and docs-test row (FR-034, FR-022; R2-1): in `README.md` Setup step 11, replace the sentences that say a failed tick does not make the check late and that you must watch the `factory-ci-tick` runs. Say instead: a red tick or a failed plan pings `/fail` and alerts at once; a late check means ticks stopped. Add a row to `.github/scripts/test-factory-ci-docs.sh` (README keyword rows, test (e)) asserting `/fail` appears in `README.md`, with a mutation check (README copy without `/fail` fails). Files: `README.md`, `.github/scripts/test-factory-ci-docs.sh`. Write the docs-test row first. Needs T036. Proof: docs test green; mutation row red on the broken copy.
- [X] T038 [US6] Merge rules for a one-person repo (FR-022; R2-2, research D20): in `README.md` Setup step 1 make item 1 the `main` ruleset (pull request required, 0 approvals, the selftest jobs as the only required status checks, no force push, no deletion, empty bypass list), delete item 2 (CODEOWNERS and code-owner review), keep the tag ruleset and Actions settings, renumber, and add the reason: GitHub never lets an author approve their own pull request, only the owner has write access, so only the owner can merge, and the selftest is the gate. Remove every other CODEOWNERS mention in the README. In `.github/scripts/test-factory-ci-docs.sh` drop the CODEOWNERS keyword row and its fixture line, add rows for "0 approvals" and the owner-only rationale, and a row that fails if `CODEOWNERS` appears in `README.md`. Files: `README.md`, `.github/scripts/test-factory-ci-docs.sh`. Needs T037 (shared files). Proof: docs test green on the README; each new row red on a mutated copy; `grep -rn CODEOWNERS README.md .github` prints nothing.
- [X] T039 [US1] Lint hardening (FR-035, FR-003, FR-027, FR-029; R2-3, research D21): in `.github/scripts/factory-ci-lint-workflow.sh`, still line-based, add the rules of `contracts/tick-workflow.md` (Lint rules): the tick `on:` block has `workflow_dispatch:` as its only key (any other 2-space key under `on:`, quoted or not, and a flow-style `on:` fail); the loop covers `*.yml` and `*.yaml`; the 40-hex `uses:` pin rule and the `permissions` present rule run on every workflow file; the `environment:` allowlist per tick job (`plan` and `verdict` use `factory-ci-plan`, `run` uses `factory-ci`, `run-docker` uses `factory-ci-docker`); the main guard regex is end-anchored `if: github.ref == 'refs/heads/main'( && .*)?$` with no `||` (adjust the rule at the line that reads the tick's job guards and the mirror job); the selftest rule also rejects `secrets: inherit` and `secrets[`. Add one mutation row to `--self-test` per new rule (quoted key `"push":`, unquoted `push:`, flow `on: [workflow_dispatch]`, a `.yaml` copy with `ubuntu-latest`, an unpinned `uses:` and a file without `permissions` in a second file, a swapped `environment:`, `if: github.ref == 'refs/heads/main' || x`, `secrets: inherit`, `secrets[`). Keep the `shortcut` note. Files: `.github/scripts/factory-ci-lint-workflow.sh`. Proof: the lint passes on the repo; `--self-test` passes and every new row fails for its named rule (CI `workflow` and `shell` jobs).
- [X] T040 [US3] Blob keys are 64 lower-case hex (FR-036; R2-4, research D22): write the shell rows first in `.github/scripts/test-factory-ci-hardening.sh` and `.github/scripts/test-factory-ci-secrets.sh` (short key, 64-character non-hex key, 64 upper-case hex, 63 and 65 hex characters, valid key; for `FACTORY_CI_BLOB_KEY` and `_DOCKER`; plan, job, status and blob functions; the tick guard steps by `grep` for `[0-9a-f]{64}`). Then require `^[0-9a-f]{64}$` in the three guard steps of `.github/workflows/factory-ci-tick.yml` (the docker key stays optional in `plan`), in `.github/scripts/factory-ci-plan.sh` (both keys), and in `fci_blob_encrypt` and `fci_blob_decrypt` of `.github/scripts/factory-ci-blob.sh` (so `factory-ci-job.sh` and `factory-ci-status.sh` refuse through them). The message names the variable and never the value. Update the comment lines that say "32 characters". Files: `.github/workflows/factory-ci-tick.yml`, `.github/scripts/factory-ci-plan.sh`, `.github/scripts/factory-ci-blob.sh`, `.github/scripts/test-factory-ci-hardening.sh`, `.github/scripts/test-factory-ci-secrets.sh`. Needs T039 (the lint must still pass on the tick). Proof: new rows red before the change and green after; sentinel keys never in output.
- [X] T041 [US3] Plan phase A without the Loki credentials (FR-037; R2-5, research D23): in `.github/scripts/factory-ci-plan.sh`, the phase A call (`plan --repos ... --rows-out`) runs with `-u FACTORY_CI_LOG_URL -u FACTORY_CI_LOG_USER -u FACTORY_CI_LOG_TOKEN` in addition to what `run()` strips; the phase C call (`--rows-in`) keeps them. Check done in research D23: `rust-ci-runner/src/plan.rs` sends the log line only from `plan::finish` (phase C), so the skip clause does not apply. Add the test row first in `.github/scripts/test-factory-ci-hardening.sh`: a fake `rust-ci-runner` records its environment per call; phase A sees none of the three names, phase C sees all three, and with all three unset the plan still passes. Files: `.github/scripts/factory-ci-plan.sh`, `.github/scripts/test-factory-ci-hardening.sh`. Needs T040 (shared plan script). Proof: the new row is red before and green after.
- [X] T042 [US3] Store check only when a row can use a secret (FR-038, FR-013; R2-6, research D19): in `.github/scripts/factory-ci-plan.sh` run `fci_list` for the plain store only when a plain row has `secrets_ok == true` and (`secrets != ""` or `repo_id != ""`), and the same for the docker store and docker rows (`jq` over `$cand`). Keep the short-line and mask logic. Before coding, confirm in `rust-ci-runner/src/job.rs` and the secret-name resolution that a pipeline reaches a secret only through shared names or an owner-scoped key (needs `id`); if a third route exists, widen the predicate and say so in the task notes. Add the test rows first in `.github/scripts/test-factory-ci-hardening.sh` (fake runner writes the candidate file, fake `bws` counts calls): a plain row with no names and no id, token set, `bws` failing, tick passes with zero `bws` calls; a row with `secrets` set and a working `bws`, one call; a row with `repo_id` set and a failing `bws`, the plan fails with the store message; a row with `secrets_ok` false, zero calls; a docker row follows the docker token. Files: `.github/scripts/factory-ci-plan.sh`, `.github/scripts/test-factory-ci-hardening.sh`. Needs T041. Proof: rows red before, green after.
- [X] T043 [P] [US5] Remove the dead `container` parameter of `capture()` (FR-008; R2-7, research D24): in `rust-ci-runner/src/engine.rs` delete the `container: Option<&str>` parameter, its `if let Some(name) = container { self.kill(name); }` branch and its doc line; update the callers, which all pass `None` (`quick`, and the call near line 372; `grep -n 'capture(' rust-ci-runner/src rust-ci-runner/tests`). `kill` stays (other callers). No other Rust change. Files: `rust-ci-runner/src/engine.rs`. Proof: green `check`, `pipeline` and `own-pipelines` selftest jobs; `cargo fmt --check` passes (apply the diff the failing step prints by hand, the bootstrap workflow is gone). CI budget: 4 runs, then stop and report.
- [X] T044 [US5] Contract drift (FR-019, FR-030; R2-8): `specs/003-infrastructure-home/contracts/pipeline-files.md` now shows the `test` step as three `cargo test --locked` commands with the pids-limit reason and the two timeouts; T020's text above says the same; T005 holds the deviation note (budget extended to 8 runs). Files: `specs/003-infrastructure-home/contracts/pipeline-files.md`, `specs/003-infrastructure-home/tasks.md`. Done in this amendment. Proof: `grep -n 'ship_tests' specs/003-infrastructure-home/contracts/pipeline-files.md rust-ci-runner/.factory-ci.toml` shows the same three commands.
- [X] T045 [US6] Repository identity (FR-039; R2-10): root `README.md` intro and the constitution preamble (version 1.0.2, PATCH, wording only; no principle changes, no component must change to comply) say the repo is the open-source home of sudtanj's infrastructure and DevOps tools; the GitHub description is set with `gh repo edit sudtanj/infrastructure --description`. Files: `README.md`, `.specify/memory/constitution.md`. Proof: `head -3 README.md`; `gh repo view sudtanj/infrastructure --json description`.
- [X] T046 [US6] Fit notice (FR-040; R2-11): a paragraph after the first paragraph of the root `README.md` says the tools are tailored to the owner's needs, may not fit every use case, and carry no promise of support or stable interfaces. Files: `README.md`. Proof: `sed -n 5p README.md`.

**Checkpoint**: every selftest job is green with the round 2 changes, then T029 and T030.

## Phase 7c: Review round 3 (lint and guard gaps; decisions R2-12 to R2-18 in the spec)

**Goal**: close the reviewer and security findings of round 3, one task each, in this order. Test rows first inside each task and red before the change. Shell tests run on the VM with the stand-in `openssl` on `PATH` for the test run only.

- [ ] T047 [US6] README blob-key wording (FR-041; R2-12): write a docs-test row in `.github/scripts/test-factory-ci-docs.sh` that fails when the README says "32 characters" (build the pattern so the test does not match itself); then change `README.md` (Setup secrets step, secrets table, troubleshooting table) to say 64 lower-case hex characters from `openssl rand -hex 32`. Files: `README.md`, `.github/scripts/test-factory-ci-docs.sh`. Proof: `bash .github/scripts/test-factory-ci-docs.sh`.
- [ ] T048 [US1] Lint `permissions` shape (FR-042; R2-13): add mutation rows for `permissions: write-all` and `permissions: read-all` (workflow level and job level) to `--self-test`, then make the rule accept only `{}` or a block mapping of `scope: read|write|none`. Files: `.github/scripts/factory-ci-lint-workflow.sh`. Proof: `--self-test` prints `self-test ok`.
- [ ] T049 [US1] Lint both blob-key guards (FR-043; R2-14): add a mutation row that deletes the `_DOCKER` guard line in the plan job, then check each variable name has a 64-hex guard. Files: `.github/scripts/factory-ci-lint-workflow.sh`. Proof: `--self-test`.
- [ ] T050 [US1] Lint flow-style and quoted keys (FR-044; R2-15): add one flow-style and one quoted mutation row for `uses:` and one for `runs-on:`, then broaden the line regexes. Files: `.github/scripts/factory-ci-lint-workflow.sh`. Proof: `--self-test`.
- [ ] T051 [US1] Lint release guard on every file (FR-045; R2-16): add a mutation row (the mirror workflow copied under another name without its guard) and one for the release workflow without its `is-ancestor` check, then check every scanned workflow with `environment: factory-ci-release`. Files: `.github/scripts/factory-ci-lint-workflow.sh`. Proof: `--self-test`.
- [ ] T052 [US3] Equal blob keys (FR-046; R2-17): add rows to `.github/scripts/test-factory-ci-hardening.sh` (equal keys fail with both variable names and no value in the output, different keys pass, docker key unset passes), then add the check to `.github/scripts/factory-ci-plan.sh` and the plan guard step of `.github/workflows/factory-ci-tick.yml`, and one README sentence. Files: those three plus `README.md`. Proof: `bash .github/scripts/test-factory-ci-hardening.sh`.
- [ ] T053 [US6] Label and risk records (R2-18): rename the deferred "security L1" to "security L7" in `plan.md` and `research.md`; record R-12 and R-13 in `plan.md`; FR-034 maps to T036 and T037 (the 13 heartbeat rows of aa6a307) and FR-035 to T039 (the lint mutation rows), both already in the coverage table. Files: `specs/003-infrastructure-home/*.md`. Proof: `grep -n "L7" specs/003-infrastructure-home/plan.md`.

**Checkpoint**: every selftest job green, then T029 and T030.

## Phase 8: Close the pull request

- [X] T028 Remove the temporary bootstrap workflow (FR-008, FR-009): delete `.github/workflows/factory-ci-bootstrap.yml` and its entry in the `no-cargo` allowlist of the lint, after the last fmt patch is applied. Needs T005 and every impl task above. After this, a review-time Rust edit that fails `cargo fmt --check` is fixed by applying the diff the failing step prints, by hand. Proof: green selftest with no bootstrap workflow; `grep -rn bootstrap .github` finds nothing but comments about the exception.
- [ ] T029 Read-through (FR-001, FR-005): `grep` for removed names (`fci_log_creds`), for `ubuntu-latest`, for bare "monorepo", for "Set up B" and for private identifiers over tracked files; confirm every task above is ticked or blocked on Setup; the PR body links `specs/003-infrastructure-home/`. Also grep for `CODEOWNERS`. Proof: the greps print nothing.
- [ ] T030 Request review (FR-008). Fix findings through the Spec Kit steps (a change to a spec or plan reruns tasks and analyze). Merge only when every selftest job is green and the owner has read the README Setup (Development Workflow). Proof: the selftest jobs are the required checks on `main` (D3).

## Phase 9: After Setup (blocked on the owner's manual steps and factory-monorepo PR 2; US7)

- [ ] T031 [US7] Owner pushes `rust-ci-runner-v0.1.0`; the engineer checks that the release workflow is green, the image exists at `ghcr.io/sudtanj/rust-ci-runner`, the package is public (`gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility`), the digest in the summary is `sha256:<64 hex>`, and the run was on `main` (FR-004). Also records how the mirrored clone image digest was checked (deferred risk M1). Proof: the `gh api` output and the run summary; docker is not run on the VM.
- [ ] T032 [US7] Live walk of `quickstart.md` Part 3 (002 T046; FR-017, FR-025): checks 1, 2, 5, 12 and 13 against `sudtanj/infrastructure`; checks 3 and 4 against factory-monorepo only after its PR 2 is merged (blocker); record in `research.md` "Live check results". Needs T031 and the Setup.
- [ ] T033 [US7] Bitwarden live check (002 T054; FR-013, FR-025): quickstart checks 6, 8 and 11; also a key containing `/` is accepted (else set `FCI_KEY_SEP` to `:` and change the README), a revoked token matches the 401 or 403 stderr text, the `--server-url` or `bws config server-base` form works, an idle tick makes zero Bitwarden calls. Needs T032.
- [ ] T034 [US7] Hardening live check (002 T065; FR-015, FR-025): quickstart checks 5, 7, 9 and 10, plus `GET /user` with a fine-grained token, the ping alert, the IPv6 coverage of the DROP rules (deferred risk M6), and the rootless-dind and `--cap-drop ALL` trials recorded but not adopted. Needs T032.
- [ ] T035 [US7] Record every live result and every spec change it causes in `research.md` (FR-032): no private run URL, Grafana hostname or token (the docs test scans it); if a check fails, open a new task here and rerun the Spec Kit steps. When checks 1 to 5 and 8 pass and some days of ticks are green, tell the owner to delete the old `FACTORY_CI_LOG_*` keys from Bitwarden. Needs T032 to T034.

---

## Requirement coverage

| Requirement | Tasks |
| --- | --- |
| FR-001 | T009, T018, T024, T025, T029 |
| FR-002 | T009, T017 |
| FR-003 | T015, T016, T039 |
| FR-004 | T008, T014, T019, T024, T031 |
| FR-005 | T018, T029 |
| FR-006 | T004, T009 |
| FR-007 | T001, T004, T005, T009 |
| FR-008 | T001, T005, T028, T030, T043 |
| FR-009 | T002, T003, T004, T028 |
| FR-010 | T007, T008, T011, T012, T013, T014, T024 |
| FR-011 | T007, T008, T012, T013 |
| FR-012 | T007, T008, T011, T012 |
| FR-013 | T008, T013, T033, T042 |
| FR-014 | T005 (existing `pipeline.rs` reserved-name tests stay green) |
| FR-015 | T008, T034 |
| FR-016 | T009, T024 |
| FR-017 | T015, T017, T024, T032 |
| FR-018 | T020, T021 |
| FR-019 | T020, T021, T022, T044 |
| FR-020 | T001, T015, T023, T024 |
| FR-021 | T022 |
| FR-022 | T009, T024, T037, T038 |
| FR-023 | T009, T019, T024 |
| FR-024 | T009, T025 |
| FR-025 | T004, T005, T027, T032, T033, T034 |
| FR-026 | T026 |
| FR-027 | T015, T039 |
| FR-028 | T006 |
| FR-029 | T001, T015, T016, T039 |
| FR-030 | T001, T010, T020, T022, T044 |
| FR-031 | T008, T012, T013, T014, T033 |
| FR-032 | T009, T035 |
| FR-033 | T019, T009 |
| FR-034 | T036, T037 |
| FR-035 | T039 |
| FR-036 | T040 |
| FR-037 | T041 |
| FR-038 | T042 |
| FR-039 | T045 |
| FR-040 | T046 |
| FR-041 | T047 |
| FR-042 | T048 |
| FR-043 | T049 |
| FR-044 | T050 |
| FR-045 | T051 |
| FR-046 | T052 |
| SC-001 | T005, T006, T028 |
| SC-002 | T009, T015 |
| SC-003 to SC-005 | T032, T033, T034 |
| SC-006 | T024, T032 |

## Dependencies

- T001 -> T002 -> T003 -> T004 -> T005 -> T006; T028 needs T005 and every impl task.
- T007, T008, T009 need T001 for the repo to build and run in parallel; T010 needs T005.
- T011 needs T007; T012 and T013 need T011 and T008; T014 needs T008.
- T015 needs nothing; T016 needs T004, T006 and T015; T017 and T018 need nothing; T019 needs T016.
- T020 needs T010; T021 needs T007 to T009; T022 needs T020, T021 and T016; T023 needs T024.
- T024 needs T014 and T019; T025 needs T024.
- T026 and T027 need nothing.
- T047 to T053 run in ID order (shared lint script and README); they need T039 to T042.
- T037 needs T036; T038 needs T037; T039 needs nothing; T040 needs T039; T041 needs T040; T042 needs T041; T043 and T044 need nothing. T029 and T030 follow T028 and T036 to T044.
- T031 needs the merge and the owner's Setup; T032 needs T031 (monorepo checks also need factory-monorepo PR 2); T033 and T034 need T032; T035 needs T032 to T034.

Parallel groups: {T007, T008, T009}, {T015, T017, T018, T026, T027}, {T020, T021}, {T033, T034}, {T039, T043} (the shell chain T037 to T038 and T040 to T042 runs in ID order because of shared files).

Round 3 shared files: README T047, T052; docs test T047; lint script T048 to T051; hardening test, plan script and tick workflow T052.

Shared files in order: selftest workflow T001, T004, T006, T016, T022; release workflow T004, T006, T016, T019, T023; tick workflow T014, T018; lint script T015, T028; READMEs T024, T025; `Cargo.toml` T001; README T037, T038; docs test T037, T038; hardening test T036, T040, T041, T042; plan script T040, T041, T042; tick workflow T036, T040; lint script T039.

## Engineer briefs

Each brief covers one feature folder, `specs/003-infrastructure-home/`, and the task IDs it names. Implement with `/speckit-implement`, only these tasks. Common constraints C1 to C7 apply.

### S1. First compile (T001 to T006)

**Goal**: a green selftest and a committed `Cargo.lock`.
**Files**: `.github/workflows/factory-ci-selftest.yml`, `.github/workflows/factory-ci-bootstrap.yml` (new), `.github/workflows/rust-ci-runner-release.yml`, `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`, `rust-ci-runner/Makefile`, `rust-ci-runner/src/**` (fixes only).
**Acceptance**: every selftest job green with `--locked`, including `check` and `build-release`; `cargo fmt --check` passes; `rust-version = "1.90"`; every cargo call in the three files has `--locked`.
**Constraints**: you cannot run cargo. Work in a loop with `gh`; read the whole public log each run; budget 5 runs, then stop and report. Fix the source, never a test. Download the artifact only from the run of this pull request, apply with `git apply --include='rust-ci-runner/*'`, read the diff first. `rustup` 1.28 or later does not install the toolchain file by itself (research D4). Keep the bootstrap workflow until S7 removes it. Mark the draft PR ready only in T030.

### S2. Tests first (T007, T008, T009, T010)

**Goal**: failing tests that the impl tasks turn green, and the engine tests marked `#[ignore]`.
**Files**: `.github/scripts/test-factory-ci-secrets.sh`, `.github/scripts/test-factory-ci-hardening.sh`, `.github/scripts/test-factory-ci-docs.sh` (new), `rust-ci-runner/tests/*.rs` and `src/*_tests.rs` (`#[ignore]` only), `specs/003-infrastructure-home/research.md` (classification), `rust-ci-runner/tests/job_rules.rs` only if the `LOG_VARS` case is missing.
**Acceptance**: each requirement row in `plan.md` Test plan has a case; each case fails on the current tree for the right reason; P1, P2, P3, P6 are property-style loops (subsets, sentinels); T010 lists every test file once.
**Constraints**: the docs test builds its patterns so it does not match itself. Use the `eq` helper style of the hardening test. T007 to T009 run in parallel; T010 waits for T005.

### S3. Secrets scripts and tick (T011 to T014)

**Goal**: log credentials from the environment; no Bitwarden call without a pipeline secret; the `https://` guard.
**Files**: `.github/scripts/factory-ci-secrets.sh`, `factory-ci-plan.sh`, `factory-ci-job.sh`, `.github/workflows/factory-ci-tick.yml`.
**Acceptance**: T007 and T008 pass in the `shell` job; no occurrence of `fci_log_creds`; `secrets.FACTORY_CI_LOG_` appears 9 times, in exactly three steps.
**Constraints**: no `set -x`; no value in any message; keep the store check in the plan; keep masking and the short-line rule. Contract: `contracts/bootstrap-secrets.md`.

### S4. Lint and consistency (T015 to T019)

**Goal**: rules that keep the repo consistent, and the repo made consistent.
**Files**: `.github/scripts/factory-ci-lint-workflow.sh`, the selftest, release and mirror workflows, `.github/factory-ci-repos.toml`, comments in scripts and workflows.
**Acceptance**: lint and `--self-test` pass with the new rules; no `ubuntu-latest`; the watch list has both entries; wording greps empty outside the READMEs; the release `publish` and mirror jobs name `factory-ci-release`, and the mirror has the `main` guard.
**Constraints**: the lint is line-based (see its `shortcut` note); every new rule gets a mutation row that fails for the named rule. T016 waits for T004 and T006; T019 waits for T016.

### S5. Pipelines and selftest (T020 to T023)

**Goal**: this repo's two pipeline files and the pre-merge run of them.
**Files**: `rust-ci-runner/.factory-ci.toml`, `.factory-ci.toml`, the selftest workflow, the release workflow header.
**Acceptance**: `own-pipelines` is green; the `bws` hash check runs; the bootstrap-exception paragraph is in all three workflow headers.
**Constraints**: images pinned to tags as in `contracts/pipeline-files.md`; no `allow_docker`; a test that needs the host stays in the selftest `shell` job with a comment. Follow the TOML rules of `rust-ci-runner/README.md`.

### S6. README, constitution, 002 pointers (T024 to T027)

**Goal**: the owner can set everything up from the README; governance and 002 point to 003.
**Files**: `README.md`, `rust-ci-runner/README.md`, `.specify/memory/constitution.md`, `specs/002-ci-runner/tasks.md`, `specs/002-ci-runner/spec.md`.
**Acceptance**: T009 passes; the README tables equal the workflow names both ways; the version reads 1.0.1; 002 shows a pointer for T045, T046, T054, T065, T081.
**Constraints**: load `stop-slop` first. Names only, never values, never a private run URL or Grafana hostname. T024 before T025. In 002 edit only the lines named in T027.

### S7. Close the pull request (T028, T029, T030)

**Goal**: a mergeable PR.
**Files**: `.github/workflows/factory-ci-bootstrap.yml` (delete), the lint allowlist.
**Acceptance**: no bootstrap workflow; green selftest; greps of T029 empty; the PR body links the folder.
**Constraints**: do not merge before the owner has read the README Setup.

### S8. Live checks (T031 to T035)

**Goal**: the live results of 002's checks.
**Files**: `specs/003-infrastructure-home/research.md`.
**Acceptance**: each check has a recorded pass or fail with a one-line cause and no private identifier.
**Constraints**: start only after the owner confirms Setup; monorepo checks wait for factory-monorepo PR 2. Do not tag a release or set a variable yourself; those are the owner's steps. A failed check opens a new task and reruns specify, plan, tasks and analyze as needed.

### S10. Review round 3 (T047 to T053)

**Goal**: the round 3 findings, in task order.
**Files**: `README.md`, `.github/scripts/factory-ci-lint-workflow.sh`, `factory-ci-plan.sh`, `test-factory-ci-docs.sh`, `test-factory-ci-hardening.sh`, `.github/workflows/factory-ci-tick.yml`, spec folder.
**Acceptance**: see T047 to T053; lint passes on the repo, also with `FACTORY_CI_LINT_STRICT=1`.
**Constraints**: C1; test rows first; sentinel values only; CI budget 4 selftest runs.

### S9. Review round 2 (T036 to T044)

**Goal**: the PR review findings, in task order. T036 and T044 are done; T037 to T043 are open.
**Files**: `.github/workflows/factory-ci-tick.yml`, `.github/scripts/factory-ci-plan.sh`, `factory-ci-blob.sh`, `factory-ci-lint-workflow.sh`, `.github/scripts/test-factory-ci-hardening.sh`, `test-factory-ci-secrets.sh`, `test-factory-ci-docs.sh`, `README.md`, `rust-ci-runner/src/engine.rs`. Contract: `contracts/tick-workflow.md`. Decisions: `research.md` D19 to D25.
**Acceptance**:
- T037: README step 11 says a red tick or failed plan pings `/fail` and alerts at once, and a late check means ticks stopped; the docs test asserts `/fail` and fails on a README without it.
- T038: README has the `main` ruleset with 0 approvals, the selftest jobs as required checks, no force push, no deletion, an empty bypass list, the tag ruleset and the owner-only rationale; no `CODEOWNERS` in `README.md` or `.github/`; docs-test rows changed to match.
- T039: the six lint changes of FR-035 each have a mutation row that fails for its named rule; the lint passes on the repo.
- T047 to T053: each lint rule has a mutation row that fails for its named rule; `--self-test` prints `self-test ok`; the three shell tests pass with 0 failed.
- T040: every guard on a blob key requires `^[0-9a-f]{64}$`; new shell rows cover short, non-hex, upper-case and valid keys.
- T041: phase A runs without the three `FACTORY_CI_LOG_*` names and phase C with them (the check of D23 shows the plan binary ships logs only in phase C, so the skip clause does not apply).
- T042: `fci_list` runs only when a row has `secrets_ok` and (`secrets` or `repo_id`); a Bitwarden outage with no such row passes; rows for each case.
- T043: `capture()` has no `container` parameter; no behaviour change; CI green.
**Constraints**: no local cargo, docker, make or builds (C1). Shell tests may run on the VM with a stand-in `openssl` on `PATH` (accepts `-pass env:` and echoes fixed text); everything else runs in CI. CI budget: 4 selftest runs for T043 and the shell changes together, then stop and report. Write the test rows first in each shell task. Order by shared files: T037, T038 (README, docs test); T039; T040, T041, T042 (plan script, hardening test). T043 is independent. Never print a key, token or credential; sentinel values only in tests. Edit nothing outside the files listed; work not listed goes back through spec, plan and tasks.
