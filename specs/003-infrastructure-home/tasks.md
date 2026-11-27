# Tasks: Infrastructure Home

**Input**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: required by the constitution (Principle V). Test tasks come first in their phase and fail until the matching impl task lands.

**Format**: `- [ ] T### [P?] [US#] description with path`. `[P]` means the task touches different files from every other open task of its group and can run in parallel with them. Paths are from the repo root.

## Common constraints (every task and brief)

- C1. No local `cargo`, `docker`, `make` or test run: the machine has 1 GB of RAM. Read, write, `grep`, `bash -n`, `git` and `gh` only. Re-read each file you change. List in the report what you could not run. CI is the compiler.
- C2. No new dependency, no new action without a SHA pin, no new file outside the plan's list.
- C3. Follow `feiyi-anti-slop-code`: comments only for a reason the code cannot show; match the style of the file you edit.
- C4. Never print a secret, token or log credential. Never use `set -x` in a script that holds one. Docs name a secret by key, never by value.
- C5. Prose in docs and comments follows `stop-slop`: no em dashes, no filler.
- C6. Use explicit `git add` paths. Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- C7. Implement with `/speckit-implement`, only the tasks named in your brief. Work not listed goes back through spec, plan and tasks.

## Phases and order

Phase 1 is the gate: nothing else can be checked until the crate compiles. Phase 2 (tests) runs in parallel with Phase 1 after T001. Phases 3 to 7 are impl; files shared between tasks run in the order of the task IDs. Phase 8 closes the pull request. Phase 9 is blocked on the owner's Setup and stays open after merge.

---

## Phase 1: First compile (US2)

**Goal**: a green selftest with a committed lock file. **Independent test**: the selftest check on the PR.

- [ ] T001 [US2] Selftest preparation (brief S1) in `.github/workflows/factory-ci-selftest.yml`: add a step `rustup toolchain install` before cargo in every job that builds; add `cargo fmt --check`, `cargo clippy -- -D warnings` and `cargo test` steps to the `pipeline` job (no `--locked` yet); add a temporary job `bootstrap` (`ubuntu-24.04`, `contents: read`) that runs `cargo generate-lockfile` and `cargo fmt` in `rust-ci-runner`, writes `fmt.patch` with `git diff -- rust-ci-runner`, and uploads `Cargo.lock` and `fmt.patch` with `actions/upload-artifact` pinned by SHA (look the SHA up with `gh api repos/actions/upload-artifact/git/ref/tags/v4`, retention 3 days); put the bootstrap-exception comment in the file header (FR-020). Proof: the YAML reads back clean and the run in T002 starts.
- [ ] T002 [US2] Push the branch and open the pull request as a draft. Wait for the selftest run. Download the artifact with `gh run download`. If the `bootstrap` job fails, fix the job and rerun (research D4). Proof: `Cargo.lock` and `fmt.patch` exist on the engineer's disk.
- [ ] T003 [US2] Land the lock (FR-006, FR-007; 002 T081): commit `rust-ci-runner/Cargo.lock`; apply `fmt.patch` with `git apply`; add `rust-version = "1.82"` to `rust-ci-runner/Cargo.toml`; add `--locked` to every cargo call that accepts it in `rust-ci-runner/Makefile` (`fetch`, `test`, `clippy`, `build`), in the selftest and in `.github/workflows/rust-ci-runner-release.yml`. Lock and `--locked` land in one commit. Proof: `grep -n cargo` on the three files shows `--locked` on every build, test, clippy and fetch line; `cargo fmt` lines stay without it.
- [ ] T004 [US2] Compile-fix loop (002 T045): push, read `gh run view --log-failed`, fix every error in the log at its source, never by loosening or deleting a test, push again. Repeat until `pipeline`, `list`, `workflow` and `shell` are green. After each fix that touches Rust, take the fresh `fmt.patch` from the `bootstrap` job and apply it. Record each class of failure in the task notes. Proof: green selftest run; `cargo fmt --check` step green.

**Checkpoint**: the selftest is green with `--locked`.

## Phase 2: Tests first (parallel, after T001)

- [ ] T005 [P] [US3] Tests in `.github/scripts/test-factory-ci-secrets.sh` for `fci_log_env` (FR-010 to FR-012; I1, P1, P2): three set gives `on` and the three values; none gives `off`; every other subset of one or two returns 1 and `FCI_ERR` names exactly the empty variables; a value with a 3-character line is refused by the callers' short-line check; `FCI_ERR` never contains a value (sentinels). Remove the cases for `fci_log_creds`. Fails until T009.
- [ ] T006 [P] [US3] Tests in `.github/scripts/test-factory-ci-hardening.sh` (a fake `rust-ci-runner`, fake `bws`, `GITHUB_OUTPUT` and summary files) for FR-010 to FR-015 (P2, P3, P7, I2 to I4): the plan fails before the first runner call when one or two log variables are set, and names them; the plan with all three set and no Bitwarden token exports them to phase C; `job` with a no-secret pipeline never calls `bws`, even with `BWS_ACCESS_TOKEN` set; `job` with a partial set refuses with "log credentials incomplete"; `job` with a Bitwarden token unset and a GitHub secret of the pipeline's secret name set ends in `error` (no fallback); sentinel values in every bootstrap variable appear in no stdout, stderr, summary or output file except in `::add-mask::` lines (use `%`, a newline and a 3-character line among the sentinels); the three jobs of the tick workflow carry the fixed "make the GHCR package public" message. Fails until T010 to T012.
- [ ] T007 [P] [US1] [US6] New `.github/scripts/test-factory-ci-docs.sh`, same `eq` style as the hardening test (FR-001 to FR-005, FR-016, FR-017, FR-022 to FR-024; P6): (a) no `(?<!factory-)monorepo`, no "separate public copy" in tracked files outside `specs/`, `.specify/`, `.claude/`, and no standalone `B` as the central repo's name in `README.md`, `rust-ci-runner/README.md` and `.github/` only (Rust test data uses "B" legitimately); build the patterns in the script so it does not match itself; (b) `.github/factory-ci-repos.toml` lists both repos; (c) no `ubuntu-latest` anywhere under `.github`; (d) the set of `secrets.X` and `vars.X` names in `.github/workflows/*.yml` (minus `GITHUB_TOKEN`) equals the set of names in the root README Setup tables, both directions, and each name sits in a table row with an Environment or scope; (e) the README sentence naming the bootstrap exception, the word "public" next to the GHCR package, and both repo names next to `FACTORY_CI_TOKEN`; (f) `rust-ci-runner/README.md` has no "Set up B"; (g) every `cargo build|test|clippy|fetch` line of the Makefile, the selftest, the release workflow and both pipeline files has `--locked`; (h) no workflow other than the selftest and release runs `cargo`; (i) no file under `.github/workflows/` names `factory-monorepo` (FR-017). A mutation run in the script's own self-check (copy a fixture tree, break one rule, expect a named failure) for (a), (c) and (d). Fails until T014 to T023.
- [ ] T008 [P] [US5] Classification (research D6): read `rust-ci-runner/tests/*.rs` and name, per file, whether it needs a real container engine (`executor.rs` does; check `argv.rs`, `job_prepare.rs`, `job_rules.rs`, `cli.rs` for a stub `docker` on `PATH` versus the real one). Write the table under "Test classification" in `research.md`, and the list of Docker-free files for the `test` step. Proof: every file in `tests/` appears once; the list uses `--test <name>` flags that match file names (`ls` check).

## Phase 3: Bootstrap secrets from Actions (US3)

**Goal**: log credentials from Environment secrets, no Bitwarden for logs. **Independent test**: T005 and T006 pass in the `shell` job.

- [ ] T009 [US3] `.github/scripts/factory-ci-secrets.sh` (contracts/bootstrap-secrets.md): replace `fci_log_creds` by `fci_log_env`; update the file header and the comment on `FCI_HAVE_LOGS`. Proof: T005 passes; `grep -n fci_log_creds .github` prints nothing.
- [ ] T010 [US3] `.github/scripts/factory-ci-plan.sh`: call `fci_log_env` after the blob-key check and `fail "$FCI_ERR"` on error; keep the `fci_list` calls; delete the two `fci_log_creds` calls and the `have` logic; set, check and mask the log values whenever `FCI_HAVE_LOGS` is set and rows exist; update the header. Needs T009. Proof: T006 plan cases pass.
- [ ] T011 [US3] `.github/scripts/factory-ci-job.sh`: `fci_log_env` first with `refuse "$FCI_ERR"`; call `fci_list` only when `names` is not empty; keep the short-line check, the `fail_closed` rule and masking. Needs T009. Proof: T006 job cases pass; `bash -n`.
- [ ] T012 [US3] `.github/workflows/factory-ci-tick.yml` (contracts/bootstrap-secrets.md): add `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` from `secrets.*` to the `env` of the `plan` step `plan`, and of the `job` step in `run` and `run-docker`, and to no other step; replace the bare `docker create` in the three `fetch runner` steps with the guarded form and its fixed message (FR-004). Proof: T006 workflow case; the lint passes after T013.

## Phase 4: Self-consistency (US1)

**Goal**: no leftover from the move; pinned runners; the watch list names both repos.

- [ ] T013 [US1] [US3] [US5] `.github/scripts/factory-ci-lint-workflow.sh`: add rules `runs-on-pinned` (every `*.yml` in the directory of the given workflow), `no-cargo` (every workflow except `factory-ci-selftest.yml` and `rust-ci-runner-release.yml`), `log-secrets-scope` (`secrets.FACTORY_CI_LOG_*` only in the `plan` step `plan` and the `job` steps), and extend `no-pull-request-target` to every workflow file; add a mutation row to the self-test for each (FR-003, FR-017, FR-020, FR-012, FR-027); keep the `shortcut` note. Proof: the lint passes on the repo; `--self-test` passes and each new rule fails on its mutated copy. Needs T012.
- [ ] T014 [US1] Pin `ubuntu-24.04` (FR-003): `.github/workflows/factory-ci-selftest.yml` (1 place) and `.github/workflows/rust-ci-runner-release.yml` (3 places). Needs T003. Proof: `grep -rn 'ubuntu-latest' .github` prints nothing; the lint rule passes.
- [ ] T015 [US1] `.github/factory-ci-repos.toml`: add the two entries of `contracts/pipeline-files.md`, keep the header comment (FR-002). Proof: the selftest `list` job (`plan --check`); T007 (b).
- [ ] T016 [US1] Wording (FR-001, FR-005), excluding the two READMEs: comments and strings in `.github/workflows/factory-ci-tick.yml` ("B is public" becomes "this repo is public"), `.github/scripts/factory-ci-plan.sh` ("B's checkout"), the other scripts, the lint header, `rust-ci-runner/tests/fixtures/selftest/.factory-ci.toml` if it needs it; `grep -rnP '(?<!factory-)monorepo|\bB\b'` over `.github` and `rust-ci-runner` (outside Rust test data) decides what to fix. In `.github/scripts/test-factory-ci-hardening.sh` point the README checks at the root `README.md`. Proof: T007 (a) passes for these paths.
- [ ] T017 [US1] `.github/workflows/rust-ci-runner-release.yml`: the digest summary step adds one line, "Make the GHCR package public once, then set RUNNER_IMAGE_DIGEST" (FR-004). Needs T014. Proof: the file reads clean; the lint passes.

## Phase 5: The runner tests itself (US5)

- [ ] T018 [US5] `rust-ci-runner/.factory-ci.toml` (FR-019, research D7): steps `lint` and `test` as in `contracts/pipeline-files.md`, the `test` flags from T008. Needs T008. Proof: T020's job runs it.
- [ ] T019 [US5] `.factory-ci.toml` at the repo root: steps `shell` and `lint-workflows`; read the scripts for the package list; a comment on any test that needs the host and stays out (FR-019). Needs T005 to T007 for the script names. Proof: T020's job runs it.
- [ ] T020 [US5] `.github/workflows/factory-ci-selftest.yml`: add the `own-pipelines` stage running `rust-ci-runner run --dir rust-ci-runner` and `run --dir .` with the built binary (FR-021); run the lint with `FACTORY_CI_LINT_STRICT=1`; add a step that downloads the pinned `bws` archive and compares its SHA-256 with `BWS_SHA256` (002 T054, CI half); add `.factory-ci.toml`, `README.md` and the release workflow to the triggers as in `contracts/selftest-workflow.md`; `timeout-minutes` as needed. Needs T018, T019. Proof: green selftest.
- [ ] T021 [US5] State the bootstrap exception (FR-020): one paragraph in the header comment of `.github/workflows/rust-ci-runner-release.yml` (the selftest header is done in T001) and in the README (T022 holds the text; this task checks it and the two headers agree). Proof: T007 (e).

## Phase 6: README Setup (US6)

- [ ] T022 [US6] Root `README.md` (FR-016, FR-022, FR-023; `data-model.md` tables, `quickstart.md` Part 2): rewrite Setup. Keep the one-paragraph intro and Docs list (point to `specs/003-infrastructure-home/`). Tables of secrets by Environment (the matrix in `data-model.md`), variables with scope, then the hand steps in order: create the three Environments limited to `main`; Bitwarden (machine accounts, projects, tokens, project IDs; the old `FACTORY_CI_LOG_*` keys in Bitwarden are deleted); Grafana Cloud Loki token (`logs:write`) and the three values; fine-grained tokens (the `FACTORY_CI_TOKEN` covers `sudtanj/infrastructure` and `sudtanj/factory-monorepo`; the scheduler token, Actions write, this repo only); set secrets; merge; tag `rust-ci-runner-vX.Y.Z`; make the `rust-ci-runner` package public; set `RUNNER_IMAGE_DIGEST`; run `factory-ci-mirror-images` and make those packages public; set the other variables; set up the scheduler (`POST` to the dispatch endpoint, `{"ref":"main"}`, every 5 minutes); one manual tick; branch protection. Add the bootstrap-exception sentence (FR-020). Keep the lines the hardening test reads: `RUNNER_IMAGE_DIGEST` without an `@`, `sha256:<64 hex>`, and the first `FACTORY_CI_BLOB_KEY_DOCKER` line marked "optional". Remove the sentence that says the log credentials are not read from GitHub. Needs T012 for the names. Proof: T007 (d), (e).
- [ ] T023 [US6] `rust-ci-runner/README.md`: replace "Set up B" by a pointer to the root README Setup (FR-024); change "B" to "this repo" everywhere; "Secrets" and "Log shipping" say the log credentials come from Environment secrets, with the rollback text updated (removing the Bitwarden token no longer stops log shipping); keep runner behaviour, commands and the pipeline format. Needs T022. Proof: T007 (a), (f).

## Phase 7: Governance and 002 pointers

- [ ] T024 Constitution PATCH 1.0.1 (FR-026, research D13): `.specify/memory/constitution.md` Principle II and "Repository Structure and Constraints" say "GitHub Actions secrets and variables of this repository, repository-level or in its Environments"; bump `Version` to 1.0.1 and `Last Amended` to the date of the change; the PR body states the reason and lists the README and workflows as the components that comply. Proof: grep of the version line.
- [ ] T025 [US7] 002 pointers (FR-025): in `specs/002-ci-runner/tasks.md` mark T045 and T081 as done by 003 T004 and T003, T046, T054 and T065 as moved to 003 T030, T031 and T032, each with a one-line note (edit only those lines and the status note at the top of the file); add a short "Superseded in part by 003" block to the top of `specs/002-ci-runner/spec.md` listing the rows of the table in 003 `spec.md`. Proof: grep for each ID shows the pointer.

## Phase 8: Close the pull request

- [ ] T026 Remove the temporary `bootstrap` job from `.github/workflows/factory-ci-selftest.yml` (FR-009) after the last fmt patch is applied. Needs T004 and every task above. Proof: a green selftest with no `bootstrap` job and `cargo fmt --check` green.
- [ ] T027 Read-through (like 002 T044): `grep` for removed names (`fci_log_creds`), for `ubuntu-latest`, for bare "monorepo" and for "Set up B" over tracked files; confirm every task above is ticked or blocked on Setup; the PR body links `specs/003-infrastructure-home/`. Proof: the greps print nothing.
- [ ] T028 Request review. Fix findings through the Spec Kit steps (a change to a spec or plan reruns tasks and analyze). Merge only when the selftest is green and the owner has read the README Setup (Development Workflow).

## Phase 9: After Setup (blocked on the owner's manual steps; US7)

- [ ] T029 [US7] Owner pushes `rust-ci-runner-v0.1.0`; the engineer checks that the release workflow is green, the image exists at `ghcr.io/sudtanj/rust-ci-runner`, the package is public, and the digest in the summary is `sha256:<64 hex>`. Proof: `docker manifest inspect` is not run on the VM; use `gh api` or the run summary.
- [ ] T030 [US7] Live walk of `quickstart.md` Part 3 (002 T046): checks 1 to 5 and 11 against the real repos; record in `research.md` "Live check results". Needs T029 and the Setup.
- [ ] T031 [US7] Bitwarden live check (002 T054): quickstart checks 6 and 8; also a key containing `/` is accepted (else set `FCI_KEY_SEP` to `:` and change the README), a revoked token matches the 401 or 403 stderr text, the `--server-url` or `bws config server-base` form works, an idle tick makes zero Bitwarden calls. Needs T030.
- [ ] T032 [US7] Hardening live check (002 T065): quickstart checks 5, 7, 9 and 10, plus `GET /user` with a fine-grained token, the ping alert, and the rootless-dind and `--cap-drop ALL` trials recorded but not adopted. Needs T030.
- [ ] T033 [US7] Record every live result and every spec change it causes in `research.md`; if a check fails, open a task here and rerun the Spec Kit steps. Needs T030 to T032.

---

## Requirement coverage

| Requirement | Tasks |
| --- | --- |
| FR-001 | T007, T016, T022, T023, T027 |
| FR-002 | T007, T015 |
| FR-003 | T007, T013, T014 |
| FR-004 | T006, T012, T017, T022 |
| FR-005 | T007, T016, T027 |
| FR-006 | T003, T007 |
| FR-007 | T003, T004, T007 |
| FR-008 | T004, T026 |
| FR-009 | T001, T026 |
| FR-010 | T005, T006, T009, T010, T011, T012 |
| FR-011 | T005, T006, T010, T011 |
| FR-012 | T005, T006, T010, T013 |
| FR-013 | T006, T011 |
| FR-014 | T004 (existing `pipeline.rs` tests stay green) |
| FR-015 | T006, T032 |
| FR-016 | T007, T022 |
| FR-017 | T007, T013, T015 |
| FR-018 | T018, T019, T020 |
| FR-019 | T018, T019 |
| FR-020 | T001, T007, T013, T021, T022 |
| FR-021 | T020 |
| FR-022 | T007, T022 |
| FR-023 | T007, T022 |
| FR-024 | T007, T023 |
| FR-025 | T003, T004, T025, T030, T031, T032 |
| FR-026 | T024 |
| FR-027 | T013 |
| SC-001 | T004, T026 |
| SC-002 | T007, T013 |
| SC-003 to SC-005 | T030, T031, T032 |
| SC-006 | T022, T030 |

## Dependencies

- T001 -> T002 -> T003 -> T004 -> T026.
- T005, T006, T007, T008 need nothing except T001 for the repo to build; they run in parallel.
- T009 needs T005; T010 and T011 need T009 and T006; T012 needs T006; T013 needs T012.
- T014 needs T003; T017 needs T014; T015 and T016 need nothing.
- T018 needs T008; T019 needs T007; T020 needs T018 and T019 and T014.
- T022 needs T012; T023 needs T022; T021 needs T001 and T022.
- T024 and T025 need nothing.
- T026 needs T004 and Phases 3 to 7; T027 and T028 follow T026.
- T029 needs the merge and the owner's Setup; T030 needs T029; T031 and T032 need T030; T033 needs them.

Parallel groups: {T005, T006, T007, T008}, {T015, T016, T024, T025}, {T018, T019}, {T031, T032}.

Shared files in order: selftest workflow T001, T003, T014, T020, T026; release workflow T003, T014, T017, T021; tick workflow T012, T016; lint script T013; READMEs T022, T023.

## Engineer briefs

Each brief covers one feature folder, `specs/003-infrastructure-home/`, and the task IDs it names. Implement with `/speckit-implement`, only these tasks. Common constraints C1 to C7 apply.

### S1. First compile (T001, T002, T003, T004; T026 later)

**Goal**: a green selftest and a committed `Cargo.lock`.
**Files**: `.github/workflows/factory-ci-selftest.yml`, `.github/workflows/rust-ci-runner-release.yml`, `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`, `rust-ci-runner/Makefile`, `rust-ci-runner/src/**` (fixes only).
**Acceptance**: the selftest run is green with `--locked`; `cargo fmt --check` passes; `rust-version = "1.82"` is in `Cargo.toml`; every cargo call in the three files has `--locked`.
**Constraints**: you cannot run cargo. Work in a loop with `gh` and read the whole public log each run. Fix the source, never a test. `rustup` 1.28 or later does not install the toolchain file by itself (research D4). Keep the `bootstrap` job until S7 removes it. Do not touch other tasks' files in these commits. Mark the draft PR ready only in T028.

### S2. Tests first (T005, T006, T007, T008)

**Goal**: failing tests that the impl tasks turn green.
**Files**: `.github/scripts/test-factory-ci-secrets.sh`, `.github/scripts/test-factory-ci-hardening.sh`, `.github/scripts/test-factory-ci-docs.sh` (new), `specs/003-infrastructure-home/research.md` (classification).
**Acceptance**: each requirement row in `plan.md` Test plan has a case; each case fails on the current tree for the right reason; P1, P2, P3, P6 are property-style loops (subsets, sentinels).
**Constraints**: the docs test builds its patterns so it does not match itself. Use the `eq` helper style of the hardening test. T005 to T007 may run in parallel; T008 reads test files only.

### S3. Secrets scripts and tick (T009, T010, T011, T012)

**Goal**: log credentials from the environment; no Bitwarden call without a pipeline secret.
**Files**: `.github/scripts/factory-ci-secrets.sh`, `factory-ci-plan.sh`, `factory-ci-job.sh`, `.github/workflows/factory-ci-tick.yml`.
**Acceptance**: T005 and T006 pass in the `shell` job; no occurrence of `fci_log_creds`; the three env entries sit in exactly three steps.
**Constraints**: no `set -x`; no value in any message; keep the store check in the plan; keep masking and the short-line rule. Contract: `contracts/bootstrap-secrets.md`.

### S4. Lint and consistency (T013, T014, T015, T016, T017)

**Goal**: rules that keep the repo consistent, and the repo made consistent.
**Files**: `.github/scripts/factory-ci-lint-workflow.sh`, the two other workflows, `.github/factory-ci-repos.toml`, comments in scripts and workflows.
**Acceptance**: lint and `--self-test` pass with the three new rules and the widened one; no `ubuntu-latest`; the watch list has both entries; wording greps empty outside the READMEs.
**Constraints**: the lint is line-based (see its `shortcut` note); every new rule gets a mutation row that fails for the named rule. T014 and T017 wait for T003.

### S5. Pipelines and selftest (T018, T019, T020, T021)

**Goal**: this repo's two pipeline files and the pre-merge run of them.
**Files**: `rust-ci-runner/.factory-ci.toml`, `.factory-ci.toml`, the selftest workflow, the release workflow header.
**Acceptance**: `own-pipelines` is green; the strict lint and the `bws` hash check run; the bootstrap-exception paragraph is in both workflow headers.
**Constraints**: images pinned to tags as in `contracts/pipeline-files.md`; no `allow_docker`; a test that needs the host stays in the selftest `shell` job with a comment. Follow the TOML rules of `rust-ci-runner/README.md`.

### S6. README, constitution, 002 pointers (T022, T023, T024, T025)

**Goal**: the owner can set everything up from the README; governance and 002 point to 003.
**Files**: `README.md`, `rust-ci-runner/README.md`, `.specify/memory/constitution.md`, `specs/002-ci-runner/tasks.md`, `specs/002-ci-runner/spec.md`.
**Acceptance**: T007 passes; the README tables equal the workflow names both ways; the version reads 1.0.1; 002 shows a pointer for T045, T046, T054, T065, T081.
**Constraints**: load `stop-slop` first. Names only, never values. T022 before T023. In 002 edit only the lines named in T025.

### S7. Close the PR, then live checks (T026, T027, T028; T029 to T033)

**Goal**: a mergeable PR; later the live results.
**Files**: the selftest workflow (T026), `research.md` (T033).
**Acceptance**: no `bootstrap` job; green selftest; greps of T027 empty. T029 to T033 start only after the owner confirms Setup and stay open after merge.
**Constraints**: do not tag a release or set a variable yourself; those are the owner's steps. A failed live check opens a new task and reruns specify, plan, tasks and analyze as needed.
