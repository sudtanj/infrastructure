---

description: "Task list for scheduled runs, the FIFO queue, saved state, debug artifacts and vault logins in the ci command"
---

# Tasks: Scheduled runs, a strict queue, saved state and debug artifacts for the ci command

**Input**: `specs/011-ci-state-queue/` (spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md)

**Prerequisites**: 010 is merged to `main` and 0.3.0 is released and pinned. Branch from that `main`. The full brief for each task is in plan.md under "Engineer briefs", the test names under "Test plan"; IDs match.

**Tests**: Required (constitution Principle V). They come first and must fail before the code exists.

**Rules for everyone**: no cargo, docker or builds on the agent VM (1 GB); tests run in the `rust-ci-runner` pipeline and the selftest. Never write, print or set a secret value anywhere. `git add` with explicit paths. `redact.rs`, `watch.rs`, `log.rs` and every new file except `vault.rs` must not name the vendor (`rust-ci-runner/tests/no_vendor.rs`). The state token, the keys and the vault credentials reach only `releases.rs`, `seal.rs` and `vault.rs`. No agent ever holds a Bitwarden credential.

Constrained fields are defined in `data-model.md`; tasks cite them as `data-model.md Entity.field`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 state kept between runs, US2 strict queue, US3 schedule, US4 failure evidence, US5 nothing leaks and settings stay runner-only, US6 vault login, US7 artifact scan

## Phase 1: Setup (serial)

- [ ] T001 Add the new empty modules and test modules to `rust-ci-runner/src/lib.rs`, create the empty files, add `ring`, `memchr`, `libc`, `miniz_oxide` to `rust-ci-runner/Cargo.toml` and the two new lock entries (`adler2` 2.0.1, `miniz_oxide` 0.8.9) plus the four dependency names of the root package to `rust-ci-runner/Cargo.lock` by hand; check both checksums against the crates.io index (plan.md T001)

## Phase 2: Tests first (parallel, one file each; they serve all stories)

- [ ] T002 [P] [US5] Write the runner-key, `fetch_runner_keys`, watch-key and name-refusal tests in `rust-ci-runner/src/store_tests.rs`, `rust-ci-runner/src/secret_names_tests.rs`, `rust-ci-runner/src/watch_tests.rs`, covering `data-model.md` Watch entry additions `state` and `vault_items`, and the ten `RUNNER_KEYS` (FR-019, FR-020, FR-034, FR-043)
- [ ] T003 [P] [US1] Write the seal tests in `rust-ci-runner/src/seal_tests.rs` against contracts/asset-format.md (FR-027, SC-008)
- [ ] T004 [P] [US3] Write the schedule tests in `rust-ci-runner/src/schedule_tests.rs`, covering `data-model.md` Schedule entry `interval_minutes`, `branch`, `project` and the 10-entry limit (FR-001 to FR-005)
- [ ] T005 [P] [US1] Write the declaration tests in the `tests` module of `rust-ci-runner/src/pipeline.rs`: `[state]`, `[artifacts]`, the `state = true` step and its ordering rule, vault entries, reserved name prefixes in `env`, `secrets` and vault `env` (FR-017, FR-018, FR-042); cite `data-model.md` Pipeline file additions
- [ ] T006 [P] [US2] Write the release-client tests in `rust-ci-runner/src/releases_http_tests.rs` and the `Date` parser test in `rust-ci-runner/src/util.rs`, plus the in-memory `FakeReleases` in `rust-ci-runner/src/fake.rs` (FR-013, FR-036)
- [ ] T007 [P] [US2] Write the queue and race tests in `rust-ci-runner/src/queue_tests.rs` over `FakeReleases`, at most 6 joined threads per test (FR-006 to FR-016, SC-002); cite `data-model.md` Ticket
- [ ] T008 [P] [US4] Write the bundle and scan tests in `rust-ci-runner/src/bundle_tests.rs` and `rust-ci-runner/src/scan_tests.rs`, including the sentinel tests for plain, base64, one line of a multi-line value, zip and nested zip, depth 3, and compressed-format magic (FR-028 to FR-032, FR-049, SC-008)
- [ ] T009 [P] [US1] Write the state tests in `rust-ci-runner/src/state_tests.rs`, including the step-scope tests: the file is absent before the declaring step, mode 0600 inside, gone after, never seen by another step, never world-readable (FR-021 to FR-027, FR-050, SC-005)
- [ ] T010 [P] [US6] Write the vault tests in `rust-ci-runner/src/vault_tests.rs` with a fake `bw` script that records argv and variables (FR-044 to FR-047, SC-007)
- [ ] T011 [US5] Write the `ci::run` integration tests in `rust-ci-runner/src/ci_state_tests.rs`: the three-run counter, the four-start queue, schedule targets, every refusal line, hostile fixture with canaries for all ten settings, the 25-line stderr list, the SIGTERM tests (all stories, SC-001 to SC-009)
- [ ] T012 [P] Update `.github/scripts/test-factory-monorepo-ci.sh` for the new workflow shape and allow `vault.rs` in `rust-ci-runner/tests/no_vendor.rs` (FR-039)

## Phase 3: Foundation code (parallel by file, after T001 and each file's tests)

- [ ] T013 [P] [US5] Grow `RUNNER_KEYS` to ten, add `BOOT_KEYS` and `fetch_runner_keys` in `rust-ci-runner/src/store.rs`, and add `state` and `vault_items` to `rust-ci-runner/src/watch.rs` (FR-019, FR-020, FR-034, FR-043); cite `data-model.md` Runner keys and Watch entry additions
- [ ] T014 [P] [US1] Add `[state]`, `[artifacts]`, `Step.state`, `Step.vault` and their validation, the ordering rule and the reserved-prefix checks to `rust-ci-runner/src/pipeline.rs` (FR-017, FR-018, FR-042); cite `data-model.md` Pipeline file additions
- [ ] T015 [P] [US1] Implement `seal` and `open` in `rust-ci-runner/src/seal.rs` per contracts/asset-format.md (FR-027)
- [ ] T016 [P] [US3] Implement the schedule parser and `due` in `rust-ci-runner/src/schedule.rs`; cite `data-model.md` Schedule entry (FR-001 to FR-004)
- [ ] T017 [P] [US2] Implement `trait Releases`, `HttpReleases` and `parse_http_date` in `rust-ci-runner/src/releases.rs` and `rust-ci-runner/src/util.rs` per contracts/release-store.md (FR-013, FR-036)
- [ ] T018 [US2] Implement the queue and the signal handler in `rust-ci-runner/src/queue.rs` and `rust-ci-runner/src/signal.rs` (FR-006 to FR-016); after T017; cite `data-model.md` Ticket
- [ ] T019 [US4] Implement the bundle writer, the scan and the artifact upload with retention in `rust-ci-runner/src/bundle.rs` and `rust-ci-runner/src/scan.rs` (FR-028 to FR-032, FR-049); after T017; cite `data-model.md` Artifact bundle
- [ ] T020 [P] [US6] Implement the vault read and wipe in `rust-ci-runner/src/vault.rs` (FR-044 to FR-047); cite `data-model.md` Vault read
- [ ] T021 [US1] Implement restore, the `StateHook` and save in `rust-ci-runner/src/state.rs` (FR-021 to FR-027, FR-050); after T015 and T017; cite `data-model.md` State asset
- [ ] T022 [P] [US5] Add `Redactor::needles`, `Log::scan_set`, the nine fault lines and the queue line to `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`, and the vault env export, `StepHook` and `Engine::own_paths` to `rust-ci-runner/src/engine.rs` (FR-035, FR-037, FR-044)
- [ ] T023 [P] [US6] Add Node and the pinned `bw` bundle to `rust-ci-runner/Dockerfile` (sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3`), the `bw` wrapper, `RUN bw --version`, and the smoke commands in `.github/workflows/rust-ci-runner-release.yml`; record the image size (FR-044)

## Phase 4: Wiring (serial)

- [ ] T024 [US1] Wire `rust-ci-runner/src/ci.rs` and `rust-ci-runner/src/stateful.rs`: the new `Seams` fields, `--schedule` and `--entry`, the identity `private` flag, the order of plan.md D7, the ref gate for vault, state and artifacts, the exit-path ticket guard (FR-001, FR-002, FR-006 to FR-009, FR-019 to FR-050)
- [ ] T025 [P] [US3] Add `due`, `decrypt-artifact`, `plan --schedule --check`, signal install and the usage text to `rust-ci-runner/src/main.rs` (FR-001, FR-004, FR-033)
- [ ] T026 Set the version to `0.4.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, and add the `releases_http_tests::` and `ci_state_tests::` process groups with the matching `--skip` flags to `rust-ci-runner/.factory-ci.toml` (FR-038)

## Phase 5: Workflow and config

- [ ] T027 [US3] Rewrite `.github/workflows/factory-monorepo-ci.yml` per contracts/config-files.md (schedule cron, `due` job, matrix, 190 minutes, no `concurrency`, schedule mount), add `.github/factory-ci-schedule.toml` (valid and empty), update the header of `.github/factory-ci-repos.toml`, and add the schedule check step to `.github/workflows/factory-ci-selftest.yml`; merge only after T029 (FR-039, FR-040)

**Checkpoint**: T001 to T026 are on `main`, the selftest and `make lint` pass, and the `rust-ci-runner` pipeline passes on that commit.

## Phase 6: Release and first use (serial, after the checkpoint)

- [ ] T028 Tag `rust-ci-runner-v0.4.0` on the passing `main` commit (run id and SHA equal), wait for `.github/workflows/rust-ci-runner-release.yml`, record the digest, verify it with `docker pull` (FR-038)
- [ ] T029 In a separate reviewed commit pushed first on a branch so `factory-ci-selftest` runs, set `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.4.0@sha256:<digest>`, then fast-forward `main` (FR-038)
- [ ] T030 Run the no-secret canary: `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner`; after T027 lands, one manual dispatch of the new workflow (quickstart.md section 3, steps 1)
- [ ] T031 Owner: create the six Bitwarden settings, the dedicated collection-limited vault account and the fine-grained state token, then run the API and vault canary on a throwaway repo (quickstart.md section 2); nobody prints a value (FR-020, FR-045)
- [ ] T032 [US1] Add `state = true` and `vault_items` to the `sudtanj/factory-monorepo` entry only and the first schedule entry when the Playwright pipeline has landed, in `.github/factory-ci-repos.toml` and `.github/factory-ci-schedule.toml` (FR-040)
- [ ] T033 [US1] Run the factory-monorepo pipeline through quickstart.md section 3, steps 2 to 8 (SC-001, SC-003, SC-009)

## Final Phase: Docs (last task)

- [ ] T034 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit`) as listed under T034 in plan.md (FR-041)

## Dependencies

- T001 first, alone. T002 to T012 in parallel after it; T011 is one file.
- T013 to T023 in parallel after T001 and their tests, except: T018 and T019 after T017; T021 after T015 and T017.
- T024 after T013 to T023. T025 after T015 and T016. T026 after T024.
- T027 can be written after T012 and merges after T029.
- T028 after the checkpoint. T029 after T028. T030 after T029 and T027. T031 any time after T028 and before T033. T032 after T031. T033 after T029, T031, T032. T034 last.

## Parallel example

After T001: one engineer per test file T002 to T012. After the tests: T013, T014, T015, T016, T017, T020, T022, T023 at once; then T018, T019, T021.

## Implementation strategy

Smallest useful slice (MVP): T001 to T005, T013 to T015, T017, T018, T021, T022, T024 give state and queue for one pipeline. The artifact path (T008, T019), vault (T010, T020, T023), schedule (T004, T016, T025, T027) and rollout follow, but ship together as 0.4.0: the spec has no partial release.

## Notes

- Spec Kit: this folder's spec was edited by the plan (Clarifications "plan calls"); `/speckit-analyze` runs before `/speckit-implement`.
- Anything not listed here goes back through spec, plan and tasks.
