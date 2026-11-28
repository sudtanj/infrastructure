---

description: "Task list for the strict queue, saved state, debug artifacts and scheduled runs of the ci command (runner 0.4.0), then the vault login (0.4.1)"
---

# Tasks: Scheduled runs, a strict queue, saved state and debug artifacts for the ci command

**Input**: `specs/011-ci-state-queue/` (spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md)

**Prerequisites**: 010 is merged to `main` and released (0.3.0, pinned). Branch from that `main`. The full brief for each task is in plan.md under "Engineer briefs", the test names under "Test plan"; IDs match. 010's real names apply (`REQUIRED_KEYS`, `fetch_with`, `Held::take`, `StepSecrets`, `NamedError`, `Redactor::forms`, `Log::secret_forms`, `Args`); if 010 changed one, follow 010.

**Tests**: Required (constitution Principle V). They come first and must fail before the code exists.

**Rules for everyone**: no cargo, docker or builds on the agent VM (1 GB); tests run in the `rust-ci-runner` pipeline and the selftest. Never write, print or set a secret value anywhere. `git add` with explicit paths. `redact.rs`, `watch.rs`, `log.rs` and every new file except `vault.rs` must not name the vendor (`rust-ci-runner/tests/no_vendor.rs`). The state token, keys and vault credentials reach only `releases.rs`, `seal.rs` and `vault.rs`. No agent ever holds a Bitwarden credential. Use membership asserts for `RUNNER_KEYS` and `FAULTS`, never counts. New data goes into `Args`, not new parameters.

Constrained fields are defined in `data-model.md`; tasks cite them as `data-model.md` `Entity.field`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 state kept between runs, US2 strict queue, US3 schedule, US4 failure evidence, US5 nothing leaks and settings stay runner-only, US6 vault login (0.4.1), US7 artifact scan

## Phase 1: Setup (serial)

- [ ] T001 Add the new empty modules and test modules to `rust-ci-runner/src/lib.rs`, create the empty files, add `ring`, `memchr`, `libc`, `miniz_oxide` to `rust-ci-runner/Cargo.toml` and the two new lock entries (`adler2` 2.0.1, `miniz_oxide` 0.8.9) plus the four dependency names of the root package to `rust-ci-runner/Cargo.lock` by hand; check both checksums against the crates.io index; land it alone on a branch first so the selftest proves `--locked` (plan.md T001)

## Phase 2: Tests first, runner 0.4.0 (parallel, one file each; they serve all stories)

- [ ] T002 [P] [US5] Write the runner-key, single-call, key-format, name-refusal and watch-key tests in `rust-ci-runner/src/store_tests.rs`, `rust-ci-runner/src/secret_names_tests.rs`, `rust-ci-runner/src/watch_tests.rs`; cite `data-model.md` `Runner keys.FACTORY_CI_STATE_KEY`, `Runner keys.FACTORY_CI_ARTIFACT_KEY`, `Watch entry additions.state`, `Watch entry additions.vault_items` (FR-019, FR-020, FR-034, FR-043, FR-059)
- [ ] T003 [P] [US1] Write the seal tests, including the exact-64-KiB case, in `rust-ci-runner/src/seal_tests.rs` against contracts/asset-format.md (FR-027)
- [ ] T004 [P] [US1] Write the declaration tests, the tag-injectivity table and the project-name rules in the `tests` module of `rust-ci-runner/src/pipeline.rs`; cite `data-model.md` Pipeline file additions (FR-017, FR-018, FR-042, FR-059)
- [ ] T005 [P] [US2] Write the release-client tests in `rust-ci-runner/src/releases_http_tests.rs` and the `Date` parser test in `rust-ci-runner/src/util.rs`, plus the in-memory `FakeReleases` in `rust-ci-runner/src/fake.rs` (FR-013, FR-036, FR-051, FR-054)
- [ ] T006 [P] [US2] Write the queue, race, budget, renewal and signal tests in `rust-ci-runner/src/queue_tests.rs` over `FakeReleases`, at most 6 joined threads per test (FR-006 to FR-016, FR-052 to FR-054, SC-002); cite `data-model.md` `Ticket.name`, `Ticket.label`
- [ ] T007 [P] [US4] Write the bundle and scan tests in `rust-ci-runner/src/bundle_tests.rs` and `rust-ci-runner/src/scan_tests.rs`: the sentinel for plain, base64, one line of a multi-line value, `trace.network` inside a zip, a nested zip, the FR-050 state strings, depth 3, `.html` and unreadable members (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058, SC-008); cite `data-model.md` `Artifact bundle.plaintext`, `Artifact bundle.asset`
- [ ] T008 [P] [US1] Write the state tests in `rust-ci-runner/src/state_tests.rs`, including the step-window tests (absent before, 0600 inside, gone after, never seen by another step), the `/proc/self` and FIFO cases and the manual-run restore (FR-021 to FR-027, FR-050, FR-055, SC-005); cite `data-model.md` `State asset.plaintext`, `State asset.asset`
- [ ] T009 [US5] Write the `ci::run` integration tests (a manual dispatch behaves as before, FR-005) in `rust-ci-runner/src/ci_state_tests.rs`: three-run counter, four-run queue, `--entry`, every refusal line, the 0.4.0 vault refusal, hostile fixture with canaries for all thirteen runner keys, one `bws` call, every Loki string of contracts/loki-lines.md (all stories, FR-029 to FR-031, FR-048, FR-051 to FR-060, SC-001 to SC-006, SC-008, SC-009)
- [ ] T010 [P] [US3] Update `.github/scripts/test-factory-monorepo-ci.sh` for the cron lines and `case` (FR-003, FR-004, FR-005), one `docker run`, 210 minutes, no `concurrency`, no `due`, and add the checks that the tick is gone and unreferenced (FR-001 to FR-004, FR-039, FR-060); cite `data-model.md` `Cron entry.entry`, `Cron entry.branch`, `Cron entry.project`

## Phase 3: Code, runner 0.4.0 (parallel by file, after T001 and each file's tests)

- [ ] T011 [P] [US5] Grow `RUNNER_KEYS` to thirteen, keep the other nine rows in `Held` from the one call, add the key checks in `rust-ci-runner/src/store.rs`, and add `state` and `vault_items` to `rust-ci-runner/src/watch.rs` (FR-019, FR-020, FR-034, FR-043, FR-059)
- [ ] T012 [P] [US1] Add `[state]`, `[artifacts]`, `Step.state`, `Step.vault` and their validation to `rust-ci-runner/src/pipeline.rs` (FR-017, FR-018, FR-042, FR-059); cite `data-model.md` Pipeline file additions
- [ ] T013 [P] [US1] Implement `seal` and `open` in `rust-ci-runner/src/seal.rs` (FR-027)
- [ ] T014 [P] [US2] Implement `trait Releases`, `HttpReleases` and `parse_http_date` in `rust-ci-runner/src/releases.rs` and `rust-ci-runner/src/util.rs` (FR-013, FR-036, FR-051, FR-054)
- [ ] T015 [US2] Implement the queue and the signal handler in `rust-ci-runner/src/queue.rs` and `rust-ci-runner/src/signal.rs` (FR-006 to FR-016, FR-052 to FR-054); after T014; cite `data-model.md` `Ticket.label`
- [ ] T016 [US4] Implement the bundle writer, the scan and the artifact upload in `rust-ci-runner/src/bundle.rs` and `rust-ci-runner/src/scan.rs` (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058); after T014; cite `data-model.md` `Artifact bundle.plaintext`
- [ ] T017 [US1] Implement restore, `StateHook` and save in `rust-ci-runner/src/state.rs` (FR-021 to FR-027, FR-050, FR-055); after T013 and T014; cite `data-model.md` `State asset.plaintext`
- [ ] T018 [P] [US5] Add the nine fault lines and the queue line to `rust-ci-runner/src/log.rs`, `StepHook` and `Engine::own_paths` to `rust-ci-runner/src/engine.rs`, and `RunExtras` (deadline, hook) to `run_pipeline` in `rust-ci-runner/src/job.rs` (FR-035, FR-037, FR-052)
- [ ] T019 [US1] Wire `rust-ci-runner/src/ci.rs` and `rust-ci-runner/src/stateful.rs`: `--entry`, the store identity, the D7 order, the deadline of D16, the 0.4.0 vault refusal (FR-001, FR-002, FR-006 to FR-009, FR-019 to FR-027, FR-049 to FR-055, FR-059); after T011 to T018
- [ ] T020 [P] [US4] Add `decrypt-artifact` and the signal install to `rust-ci-runner/src/main.rs` (FR-033); after T013
- [ ] T021 Set the version to `0.4.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, and add the `releases_http_tests::` and `ci_state_tests::` process groups with the matching `--skip` flags to `rust-ci-runner/.factory-ci.toml` (FR-038); after T019
- [ ] T022 [P] [US3] Retire the tick: delete `.github/workflows/factory-ci-tick.yml`, drop its references from `.github/workflows/factory-ci-selftest.yml`, `.github/scripts/*`, `README.md` and `rust-ci-runner/README.md`, remove the settings only it used from the docs, and write down which scripts and `plan` code stay for a later cleanup (FR-060)
- [ ] T023 [US3] Rewrite `.github/workflows/factory-monorepo-ci.yml` per contracts/config-files.md (up to 3 cron lines, the `case`, 210 minutes, no `concurrency`, `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_RUNNER_IMAGE`, the Environment of the plan's D18) and update the header of `.github/factory-ci-repos.toml`; merge only after T025 (FR-001 to FR-004, FR-039)

**Checkpoint**: T001 to T022 are on `main`, the selftest and `make lint` pass, and the `rust-ci-runner` pipeline passes on that commit.

## Phase 4: Release 0.4.0 and first use (serial, after the checkpoint)

- [ ] T024 Tag `rust-ci-runner-v0.4.0` on the passing `main` commit (run id and SHA equal), wait for `.github/workflows/rust-ci-runner-release.yml`, record the digest and the sha256 of `rust-ci-runner-x86_64-linux`, verify the digest with `docker pull` (FR-038)
- [ ] T025 In a separate reviewed commit pushed first on a branch so `factory-ci-selftest` runs, set `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.4.0@sha256:<digest>`, then fast-forward `main` (FR-038)
- [ ] T026 Run the no-secret canary (`repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner`); after T023 lands, one manual dispatch of the new workflow (quickstart.md section 3 step 1)
- [ ] T027 [P] Write the Grafana alert rules in `rust-ci-runner/alerts/factory-ci.yaml` matching `contracts/loki-lines.md` and `FAULTS`, with a shell check that greps each string; after T024 (FR-057)
- [ ] T028 [P] Write `rust-ci-runner/scripts/open-artifact.sh` (pinned-sha256 runner binary, decrypt with exit check, `tar -x --no-same-owner` into a new empty directory, sanitised capped output) and its shell test; after T024 (FR-033)
- [ ] T029 Owner: do the owner actions of plan.md, settle the Environment decision, then run the release canary of quickstart.md section 2 on the real store repo (200 trials, two racing runners, a real Playwright failure trace through the scan); nobody prints a value (FR-020, FR-049)
- [ ] T030 [US1] Add `state = true` and `secret_branches = ["main"]` to the `sudtanj/factory-monorepo` entry only, and the first cron line with its `case` branch when the Playwright pipeline has landed, in `.github/factory-ci-repos.toml` and `.github/workflows/factory-monorepo-ci.yml`; after T022 and T029 (FR-040)
- [ ] T031 [US1] Run the factory-monorepo pipeline through quickstart.md section 3 steps 1 to 5 and 7 to 9 (SC-001 to SC-005, SC-009); after T025, T027, T028, T029, T030

## Phase 5: Runner 0.4.1, the vault login (after T031)

- [ ] T032 [P] [US6] Write the vault tests in `rust-ci-runner/src/vault_tests.rs` (fake `bw`), add `vault` and `vault_tests` to `rust-ci-runner/src/lib.rs`, and add the vault cases to `rust-ci-runner/src/ci_state_tests.rs` (FR-042 to FR-047, FR-056, SC-007)
- [ ] T033 [US6] Implement the vault read and wipe in `rust-ci-runner/src/vault.rs` and allow it in `rust-ci-runner/tests/no_vendor.rs` (FR-044 to FR-047, FR-056); cite `data-model.md` `Vault read.map`
- [ ] T034 [P] [US6] Add Node and the pinned `bw` bundle (sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3`), the wrapper and `RUN bw --version` to `rust-ci-runner/Dockerfile` and the smoke to `.github/workflows/rust-ci-runner-release.yml` (amd64 blocks, arm64 does not); record image size and pull time (FR-044)
- [ ] T035 [US6] Replace the 0.4.0 vault refusal with the read in `rust-ci-runner/src/ci.rs`, `rust-ci-runner/src/stateful.rs` and `rust-ci-runner/src/engine.rs`, and set the version to `0.4.1` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock` (FR-044 to FR-047, FR-056); after T032 to T034
- [ ] T036 [US6] Tag `rust-ci-runner-v0.4.1`, pin it in its own commit on a branch first, run the no-secret canary and the manual canary (as T024 to T026)
- [ ] T037 [US6] Owner: create the vault credentials and the collection account, add `vault_items = ["ci-login-1"]` to the `sudtanj/factory-monorepo` entry, run the `bw` row of quickstart.md section 2 and section 3 step 6 (FR-040, FR-045)

## Final Phase: Docs (last task)

- [ ] T038 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit`) as listed under T038 in plan.md, including the release checklist line (image digest, agent script sha), the `state restore failed` runbook, the day-55 note and the accepted risks (FR-041, FR-048, FR-057)

## Dependencies

- T001 first, alone. T002 to T010 in parallel after it; T009 is one file.
- T011 to T018, T020, T022 in parallel after T001 and their tests, except: T015 and T016 after T014; T017 after T013 and T014. T019 after T011 to T018. T021 after T019. T023 after T010 and merges after T025.
- T024 after the checkpoint. T025 after T024. T026 after T025 and T023. T027, T028 after T024. T029 any time after T024. T030 after T022 and T029. T031 after T025, T027, T028, T029, T030.
- Phase 5 starts after T031: T032 and T034 in parallel; T033 after T032; T035 after T032 to T034; T036 after T035; T037 after T036. T038 last.

## Parallel example

After T001: one engineer per test file T002 to T010. After the tests: T011, T012, T013, T014, T018, T020, T022 at once; then T015, T016, T017; then T019.

## Implementation strategy

0.4.0 is the first release: T001 to T031. It carries US1 to US5 and US7 and the schedule. The vault (US6, SC-007) is its own 0.4.1 phase, T032 to T037, so a vault problem never blocks the queue and state. Interim logins use 010 step secrets.

## Notes

- Spec Kit: spec, plan and tasks of this folder changed after the critic review; `/speckit-clarify` and `/speckit-analyze` ran on this revision before `/speckit-implement`.
- Anything not listed here goes back through spec, plan and tasks.
