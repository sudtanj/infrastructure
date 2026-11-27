# Tasks: Infrastructure Home

**Input**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: required by the constitution (Principle V). Test tasks come first in their phase and fail until the matching impl task lands.

**Format**: `- [ ] T### [P?] [US#] description with path`. `[P]` means the task touches different files from every other open task of its group and can run in parallel with them. Paths are from the repo root. Review round 1 (critic, devops, security; lead decisions in `spec.md`) renumbered every task; no ID from the first draft survives.

## Common constraints (every task and brief)

- C1. No local `cargo`, `docker`, `make` or test run: the machine has 1 GB of RAM. Read, write, `grep`, `bash -n`, `git` and `gh` only. Re-read each file you change. List in the report what you could not run. CI is the compiler.
- C2. No new dependency, no new action without a SHA pin (reuse the pins already in the repo), no new file outside the plan's list.
- C3. Follow `feiyi-anti-slop-code`: comments only for a reason the code cannot show; match the style of the file you edit.
- C4. Never print a secret, token or log credential. Never use `set -x` in a script that holds one. Docs and specs name a secret by key, never by value, and never hold a private repo's run URL or a Grafana hostname.
- C5. Prose in docs and comments follows `stop-slop`: no em dashes, no filler.
- C6. Use explicit `git add` paths, never `-a`. Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- C7. Implement with `/speckit-implement`, only the tasks named in your brief. Work not listed goes back through spec, plan and tasks.

## Phases and order

Phase 1 is the gate: nothing else can be checked until the crate compiles. Phase 2 (tests) runs in parallel with Phase 1 after T001, except T010, which edits Rust test files and waits for T005. Phases 3 to 7 are impl; files shared between tasks run in the order of the task IDs. Phase 8 closes the pull request. Phase 9 is blocked on the owner's Setup (and, for the monorepo checks, on factory-monorepo PR 2) and stays open after merge.

---

## Phase 1: First compile (US2)

**Goal**: a green selftest with a committed lock file. **Independent test**: the selftest checks on the PR.

- [X] T001 [US2] Selftest preparation (brief S1) in `.github/workflows/factory-ci-selftest.yml` and `rust-ci-runner/Cargo.toml` (FR-007, FR-008, FR-020, FR-029; D5, D15, D17): add `rust-version = "1.90"` to `Cargo.toml` first; in the selftest drop the `paths:` filter, add `workflow_dispatch`, add `concurrency: {group: selftest-${{ github.ref }}, cancel-in-progress: true}`, set `persist-credentials: false` on every checkout; add `rustup toolchain install` before cargo in every job that runs it; add a `check` job (`cargo check --all-targets`; T004 adds `--locked`); add `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test -- --include-ignored` to the `pipeline` job; no `secrets.`, `environment:`, `workflow_run`, `id-token` or `upload-artifact` in the file; put the bootstrap-exception comment in the header. Proof: the YAML reads back clean; the run in T003 starts.
- [X] T002 [US2] New `.github/workflows/factory-ci-bootstrap.yml` (temporary; FR-009; research D4): `pull_request`, one `ubuntu-24.04` job guarded by `if: github.event.pull_request.head.repo.full_name == github.repository`, `permissions: contents: read`, no Environment, no secret; `rustup toolchain install`; `generate-lockfile` only when `rust-ci-runner/Cargo.lock` is absent, with `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`; `cargo fmt || true`; `git diff -- 'rust-ci-runner/*.rs' > fmt.patch`; upload with `actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02` (the pin the release workflow uses), `if: always()`, paths `rust-ci-runner/Cargo.lock` and `fmt.patch` only, `include-hidden-files: false`, `retention-days: 3`. Header comment says it is the bootstrap exception and is deleted by T028. Proof: the YAML reads back clean.
- [X] T003 [US2] Push the branch and open the pull request as a draft. Wait for the run of this pull request. Download the artifact with `gh run download <run-id>` from that run only (FR-009). Read `fmt.patch` and the lock diff before applying anything. If the bootstrap job fails, fix it and rerun. Proof: `Cargo.lock` and `fmt.patch` are on the engineer's disk and the diff was read; the task notes name the run ID. Notes: first bootstrap run 45952679858 (artifact read, lock and fmt.patch applied; later bootstrap runs gave an empty patch).
- [X] T004 [US2] Land the lock (FR-006, FR-009; 002 T081): commit `rust-ci-runner/Cargo.lock`; apply the patch with `git apply --include='rust-ci-runner/*' fmt.patch`; add `--locked` to every cargo call that accepts it in `rust-ci-runner/Makefile` (`fetch`, `test`, `clippy`, `build`), the selftest (including the `check` job) and `.github/workflows/rust-ci-runner-release.yml`. Lock and `--locked` land in one commit. Proof: `grep -n cargo` on the three files shows `--locked` on every build, test, clippy, check and fetch line (`fmt` stays without it).
- [ ] T005 [US2] Compile-fix loop (FR-008; 002 T045): push, read `gh run view --log-failed`, fix every error in the log at its source, never by loosening or deleting a test, push again. Budget: 5 selftest runs; after the fifth red run, stop and report so the plan is revisited. After each Rust fix, take the fresh `fmt.patch` from the bootstrap run and apply it the same way. Record each class of failure in the task notes. Proof: every selftest job green, `cargo fmt --check` included. Notes (open, budget spent after 5 red selftest runs, 45952679834, 45952999801, 45953515782, 45953881768, 45954341009): E0308 array size in `tests/executor.rs`; unused `std::io::Read` in 4 test files; clippy `type_complexity` in `job.rs`; `job_rules` test expected the unsanitized comma (FR-051 sanitizer turns it into `_`); aarch64 `build-release` needed `libc6-dev-arm64-cross`. Still red: last step "no containers, networks or volumes are left" after `cargo test` ran in the same job. Budget extended by 3 selftest runs (coordinator): cause of the leak is a runner bug in `engine.rs` `run_logged`, where a timeout during container creation killed the client before the daemon finished the create and left a `Created` container; fix is `stop_and_remove` (kill the client, then `rm -f` until `container inspect` fails twice, bounded to 5 s).
- [X] T006 [US2] Release dry run (FR-028; D14): add a selftest job `build-release` with the release workflow's matrix (both musl targets, same packages, same `CC_*` and `CARGO_TARGET_*_LINKER` environment), `make build`, stage the binaries as the release workflow does, `docker buildx build` with `push: false` using the pinned `docker/setup-buildx-action` and no login; add `rustup toolchain install` before `make build` in `.github/workflows/rust-ci-runner-release.yml`; add no `workflow_dispatch` to the release workflow. Proof: green `build-release` job.

**Checkpoint**: the selftest is green with `--locked`.

## Phase 2: Tests first (parallel, after T001)

- [X] T007 [P] [US3] Tests in `.github/scripts/test-factory-ci-secrets.sh` for `fci_log_env` (FR-010 to FR-012; I1, P1, P2): three set gives `on` and the three values; none gives `off`; every other subset returns 1 and `FCI_ERR` names exactly the empty variables; a 3-character line is refused by the callers' short-line check; `FCI_ERR` never contains a value (sentinels). Remove the cases for `fci_log_creds`. Fails until T011.
- [X] T008 [P] [US3] Tests in `.github/scripts/test-factory-ci-hardening.sh` (a fake `rust-ci-runner`, fake `bws`, `GITHUB_OUTPUT` and summary files) for FR-004, FR-010 to FR-015, FR-031 (P2, P3, P7, I2 to I4): the plan fails before the first runner call when one or two log variables are set, and names them; the plan with all three set and no Bitwarden token exports them to phase C; `job` with a no-secret pipeline never calls `bws`, even with `BWS_ACCESS_TOKEN` set; `job` with a partial set refuses with "log credentials incomplete"; with the Bitwarden token unset and a GitHub secret of the pipeline's secret name set, the item ends in `error` (no fallback); sentinel values in every bootstrap variable (with `%`, a newline and a 3-character line among them) appear in no stdout, stderr, summary or output file except in `::add-mask::` lines; `grep -c 'secrets.FACTORY_CI_LOG_'` on the tick workflow is 9, in the `plan` step `plan` and the two `job` steps; step containers never get the log variables (name the existing `job.rs` or `tests/job_rules.rs` test that covers `LOG_VARS`, or add one case in `tests/job_rules.rs`); the three `fetch runner` steps carry the fixed "make the GHCR package public" message; the three `install bws` steps and both scripts refuse a `FACTORY_CI_BWS_SERVER_URL` that does not start with `https://`. Fails until T012 to T014.
- [X] T009 [P] [US1] [US6] New `.github/scripts/test-factory-ci-docs.sh`, same `eq` style as the hardening test (FR-001, FR-002, FR-006, FR-007, FR-016, FR-022 to FR-024, FR-032; P6): (a) no `(?<!factory-)monorepo` and no "separate public copy" in tracked files outside `specs/`, `.specify/`, `.claude/`, and no standalone `B` as the central repo's name in `README.md`, `rust-ci-runner/README.md` and `.github/` only (Rust test data uses "B" legitimately); build the patterns in the script so it does not match itself; (b) `.github/factory-ci-repos.toml` lists both repos; (c) every `cargo build|test|clippy|check|fetch` line of the Makefile, the selftest, the release workflow and both pipeline files has `--locked`, and `Cargo.toml` holds `rust-version = "1.90"`; (d) the set of `secrets.X`, `vars.X` names (minus `GITHUB_TOKEN`) and `environment:` names in `.github/workflows/*.yml` equals the set in the root README Setup tables, both directions, each secret or variable in a row with an Environment or scope; (e) README keyword rows: the bootstrap-exception sentence, "public" next to the GHCR package and the visibility command, both repo names next to `FACTORY_CI_TOKEN`, `factory-ci-release`, the main ruleset, CODEOWNERS, the tag ruleset, the scheduler headers and body, the heartbeat grace, the no-`.factory-ci.toml` sentence, the rollback list, the `gh secret list` check; (f) `rust-ci-runner/README.md` has no "Set up B"; (g) a scan of the README files, `research.md` and `quickstart.md` for private identifiers: no `github.com/sudtanj/factory-monorepo/actions` URL, no `grafana.net`, no token or key pattern (`ghp_`, `github_pat_`, `glc_`, `BEGIN .*PRIVATE KEY`). Mutation self-check (copy a fixture tree, break one rule, expect a named failure) for (a), (d) and (g). Fails until T016 to T025.
- [ ] T010 [P] [US5] Test classification (FR-030; research D6): read every file in `rust-ci-runner/tests/*.rs` and every `src/*_tests.rs` and find the tests that need a real container engine (the critic found `tests/executor.rs`; check `argv.rs`, `job_prepare.rs`, `job_rules.rs`, `cli.rs` for a stub `docker` on `PATH` versus the real one). Add `#[ignore]` with a reason string to each such test. Write the table under "Test classification" in `research.md`. Needs T005 (it edits Rust test files after the compile loop). Proof: every file appears once; the selftest `pipeline` job (with `--include-ignored`) stays green; `cargo test --locked` in the root of a container without an engine is exercised by T022.

## Phase 3: Bootstrap secrets from Actions (US3)

**Goal**: log credentials from Environment secrets, no Bitwarden for logs. **Independent test**: T007 and T008 pass in the `shell` job.

- [ ] T011 [US3] `.github/scripts/factory-ci-secrets.sh` (contracts/bootstrap-secrets.md; FR-010 to FR-012): replace `fci_log_creds` by `fci_log_env`; update the file header and the comment on `FCI_HAVE_LOGS`. Proof: T007 passes; `grep -n fci_log_creds .github` prints nothing.
- [ ] T012 [US3] `.github/scripts/factory-ci-plan.sh` (FR-010 to FR-012, FR-031): call `fci_log_env` after the blob-key check and `fail "$FCI_ERR"` on error; refuse a `FACTORY_CI_BWS_SERVER_URL` that is set and does not start with `https://`; keep the `fci_list` calls; delete the two `fci_log_creds` calls and the `have` logic; set, check and mask the log values whenever `FCI_HAVE_LOGS` is set and rows exist; update the header. Needs T011. Proof: T008 plan cases pass.
- [ ] T013 [US3] `.github/scripts/factory-ci-job.sh` (FR-010 to FR-013, FR-031): `fci_log_env` first with `refuse "$FCI_ERR"`; the same `https://` refusal before the first `bws` call; call `fci_list` only when `names` is not empty; keep the short-line check, the `fail_closed` rule and masking. Needs T011. Proof: T008 job cases pass; `bash -n`.
- [ ] T014 [US3] `.github/workflows/factory-ci-tick.yml` (contracts/bootstrap-secrets.md; FR-004, FR-010, FR-031): add `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` from `secrets.*` to the `env` of the `plan` step `plan`, and of the `job` step in `run` and `run-docker`, and to no other step; replace the bare `docker create` in the three `fetch runner` steps with the guarded form and its fixed message; add the `https://` guard to the three `install bws` steps. Proof: T008 workflow cases; the lint passes after T015.

## Phase 4: Self-consistency (US1)

**Goal**: no leftover from the move; pinned runners; the watch list names both repos; the lint keeps it so.

- [ ] T015 [US1] [US5] `.github/scripts/factory-ci-lint-workflow.sh` (FR-003, FR-020, FR-027, FR-029; research D9): add `runs-on-pinned` (every `*.yml` in the directory of the given workflow, exactly `ubuntu-24.04`), `no-cargo` (every workflow except the selftest, the release workflow and, while it exists, `factory-ci-bootstrap.yml`), `no-pull-request-target` over every workflow, and the selftest rules (no `workflow_run`, `secrets.`, `environment:`, `id-token`; no `paths:` under `pull_request`; `persist-credentials: false` on every checkout); add a mutation row to the self-test for each, including the flow-style forms `on: [pull_request_target]` and `on: {pull_request_target: ...}`; keep the `shortcut` note. Proof: the lint passes on the repo; `--self-test` passes and each new rule fails on its mutated copy.
- [ ] T016 [US1] [US2] Pin and lint the directory (FR-003, FR-029): `ubuntu-24.04` in `.github/workflows/factory-ci-selftest.yml` (1 place) and `.github/workflows/rust-ci-runner-release.yml` (3 places); the selftest `workflow` job lints the whole `.github/workflows/` directory with `FACTORY_CI_LINT_STRICT=1`. Needs T004, T006 and T015. Proof: `grep -rn 'ubuntu-latest' .github` prints nothing; green `workflow` job.
- [ ] T017 [US1] `.github/factory-ci-repos.toml` (FR-002, FR-017): add the two entries of `contracts/pipeline-files.md`, keep the header comment. Proof: the selftest `list` job (`plan --check`); T009 (b).
- [ ] T018 [US1] Wording (FR-001, FR-005), excluding the two READMEs: comments and strings in `.github/workflows/factory-ci-tick.yml` ("B is public" becomes "this repo is public"), `.github/scripts/factory-ci-plan.sh` ("B's checkout"), the other scripts, the lint header, `rust-ci-runner/tests/fixtures/selftest/.factory-ci.toml` if it needs it; `grep -rnP '(?<!factory-)monorepo|\bB\b'` over `.github` and `rust-ci-runner` (outside Rust test data) decides what to fix. In `.github/scripts/test-factory-ci-hardening.sh` point the README checks at the root `README.md`. Proof: T009 (a) passes for these paths.
- [ ] T019 [US1] Release and mirror (FR-004, FR-033): `.github/workflows/rust-ci-runner-release.yml` `publish` job gets `environment: factory-ci-release`, and the digest summary step adds the line "Make the GHCR package public once, then set RUNNER_IMAGE_DIGEST"; `.github/workflows/factory-ci-mirror-images.yml` `mirror` job gets `environment: factory-ci-release` and `if: github.ref == 'refs/heads/main'`. Needs T016. Proof: the lint passes; T009 (d) once T024 lands.

## Phase 5: The runner tests itself (US5)

- [ ] T020 [US5] `rust-ci-runner/.factory-ci.toml` (FR-018, FR-019, FR-030): steps `lint` and `test` as in `contracts/pipeline-files.md`, `test` running `cargo test --locked`. Needs T010. Proof: T022's job runs it.
- [ ] T021 [US5] `.factory-ci.toml` at the repo root (FR-018, FR-019): steps `shell` and `lint-workflows` with `git` in the package list and `git config --global --add safe.directory '*'`; read the scripts for the package list; a comment on any test that needs the host and stays out. Needs T007 to T009 for the script names. Proof: T022's job runs it.
- [ ] T022 [US5] `.github/workflows/factory-ci-selftest.yml` (FR-021, FR-030; D8): add the `own-pipelines` job (builds the runner or takes it as an artifact from `pipeline`; sets `FACTORY_CI_SHA`, `FACTORY_CI_EVENT=push`, `FACTORY_CI_BRANCH=selftest`; separate `CARGO_TARGET_DIR`) running `rust-ci-runner run --dir rust-ci-runner` and `run --dir .`; add a step that downloads the pinned `bws` archive and compares its SHA-256 with `BWS_SHA256` (002 T054, CI half). Needs T020, T021 and T016. Proof: green selftest; the log shows `cargo test --locked` passing in the container without an engine and the root pipeline passing with `git`.
- [ ] T023 [US5] State the bootstrap exception (FR-020): one paragraph in the header comment of `.github/workflows/rust-ci-runner-release.yml` (the selftest and bootstrap headers are done in T001 and T002); the README text lives in T024; this task checks the headers and the README agree. Needs T024. Proof: T009 (e).

## Phase 6: README Setup (US6)

- [ ] T024 [US6] Root `README.md` (FR-010, FR-016, FR-017, FR-020, FR-022, FR-023; `data-model.md` tables, `quickstart.md` Part 2): rewrite Setup. Keep the one-paragraph intro and Docs list (point to `specs/003-infrastructure-home/`). Tables of secrets by Environment (the matrix in `data-model.md`, two Loki tokens) and variables with scope; the four Environments; then every item of FR-022 in its order: repository rules, Environments, accounts and tokens, secrets, merge, release tag, public package and its check, variables, mirror, scheduler with headers and body, heartbeat, first manual tick, `gh secret list` check, the no-`.factory-ci.toml` behaviour, the late deletion of the old Bitwarden log keys, rollback with what a revert does not restore, the notes (deprecation notices, Docker Hub limits, cost). Add the bootstrap-exception sentence. Remove the sentence that says the log credentials are not read from GitHub. Keep the lines the hardening test reads: `RUNNER_IMAGE_DIGEST` without an `@`, `sha256:<64 hex>`, and the first `FACTORY_CI_BLOB_KEY_DOCKER` line marked "optional". Needs T014 and T019 for the names. Proof: T009 (d), (e), (g).
- [ ] T025 [US6] `rust-ci-runner/README.md` (FR-001, FR-024): replace "Set up B" by a pointer to the root README Setup; change "B" to "this repo" everywhere; "Secrets" and "Log shipping" say the log credentials come from Environment secrets, with the rollback text updated (removing the Bitwarden token no longer stops log shipping); keep runner behaviour, commands and the pipeline format. Needs T024. Proof: T009 (a), (f).

## Phase 7: Governance and 002 pointers

- [ ] T026 Constitution PATCH 1.0.1 (FR-026, research D13): `.specify/memory/constitution.md` Principle II and "Repository Structure and Constraints" say "GitHub Actions secrets and variables of this repository, repository-level or in its Environments"; bump `Version` to 1.0.1 and `Last Amended` to the date of the change; the PR body states the reason and lists the README and workflows as the components that comply. Proof: grep of the version line.
- [ ] T027 [US7] 002 pointers (FR-025): in `specs/002-ci-runner/tasks.md` mark T045 and T081 as done by 003 T005 and T004, and T046, T054 and T065 as moved to 003 T032, T033 and T034, each with a one-line note (edit only those lines and the status note at the top of the file); add a short "Superseded in part by 003" block to the top of `specs/002-ci-runner/spec.md` listing the rows of the table in 003 `spec.md`. Proof: grep for each ID shows the pointer.

## Phase 8: Close the pull request

- [ ] T028 Remove the temporary bootstrap workflow (FR-008, FR-009): delete `.github/workflows/factory-ci-bootstrap.yml` and its entry in the `no-cargo` allowlist of the lint, after the last fmt patch is applied. Needs T005 and every impl task above. After this, a review-time Rust edit that fails `cargo fmt --check` is fixed by applying the diff the failing step prints, by hand. Proof: green selftest with no bootstrap workflow; `grep -rn bootstrap .github` finds nothing but comments about the exception.
- [ ] T029 Read-through (FR-001, FR-005): `grep` for removed names (`fci_log_creds`), for `ubuntu-latest`, for bare "monorepo", for "Set up B" and for private identifiers over tracked files; confirm every task above is ticked or blocked on Setup; the PR body links `specs/003-infrastructure-home/`. Proof: the greps print nothing.
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
| FR-003 | T015, T016 |
| FR-004 | T008, T014, T019, T024, T031 |
| FR-005 | T018, T029 |
| FR-006 | T004, T009 |
| FR-007 | T001, T004, T005, T009 |
| FR-008 | T001, T005, T028, T030 |
| FR-009 | T002, T003, T004, T028 |
| FR-010 | T007, T008, T011, T012, T013, T014, T024 |
| FR-011 | T007, T008, T012, T013 |
| FR-012 | T007, T008, T011, T012 |
| FR-013 | T008, T013, T033 |
| FR-014 | T005 (existing `pipeline.rs` reserved-name tests stay green) |
| FR-015 | T008, T034 |
| FR-016 | T009, T024 |
| FR-017 | T015, T017, T024, T032 |
| FR-018 | T020, T021 |
| FR-019 | T020, T021, T022 |
| FR-020 | T001, T015, T023, T024 |
| FR-021 | T022 |
| FR-022 | T009, T024 |
| FR-023 | T009, T019, T024 |
| FR-024 | T009, T025 |
| FR-025 | T004, T005, T027, T032, T033, T034 |
| FR-026 | T026 |
| FR-027 | T015 |
| FR-028 | T006 |
| FR-029 | T001, T015, T016 |
| FR-030 | T001, T010, T020, T022 |
| FR-031 | T008, T012, T013, T014, T033 |
| FR-032 | T009, T035 |
| FR-033 | T019, T009 |
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
- T029 and T030 follow T028.
- T031 needs the merge and the owner's Setup; T032 needs T031 (monorepo checks also need factory-monorepo PR 2); T033 and T034 need T032; T035 needs T032 to T034.

Parallel groups: {T007, T008, T009}, {T015, T017, T018, T026, T027}, {T020, T021}, {T033, T034}.

Shared files in order: selftest workflow T001, T004, T006, T016, T022; release workflow T004, T006, T016, T019, T023; tick workflow T014, T018; lint script T015, T028; READMEs T024, T025; `Cargo.toml` T001.

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
