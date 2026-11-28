---

description: "Task list for the strict queue, saved state and debug artifacts of the ci command (runner 0.4.0, Rust and toml only; owner rule of 2026-11-28)"
---

# Tasks: A strict queue, saved state and debug artifacts for the ci command

**Input**: `specs/011-ci-state-queue/` (spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md)

**Prerequisites**: 010 is merged to `main` and released (0.3.0, pinned). Branch from that `main`. The full brief for each task is in plan.md under "Engineer briefs", the test names under "Test plan"; IDs match. 010's real names apply (`REQUIRED_KEYS`, `fetch_with`, `Held::take`, `StepSecrets`, `NamedError`, `Redactor::forms`, `Log::secret_forms`, `Args`); if 010 changed one, follow 010.

**Tests**: Required (constitution Principle V). They come first and must fail before the code exists.

**Rules for everyone**: no file under `.github/` and no `rust-ci-runner/Dockerfile` changes (owner rule of 2026-11-28); `git diff --stat main -- .github rust-ci-runner/Dockerfile` is empty before every commit. no cargo, docker or builds on the agent VM (1 GB); tests run in the `rust-ci-runner` pipeline and the selftest. Never write, print or set a secret value anywhere. `git add` with explicit paths. `redact.rs`, `watch.rs`, `log.rs` and every new file must not name the vendor (`rust-ci-runner/tests/no_vendor.rs`). The state token, keys and vault credentials reach only `releases.rs` and `seal.rs`. No agent ever holds a Bitwarden credential. Use membership asserts for `RUNNER_KEYS` and `FAULTS`, never counts. New data goes into `Args`, not new parameters.

Constrained fields are defined in `data-model.md`; tasks cite them as `data-model.md` `Entity.field`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 state kept between runs, US2 strict queue, US3 schedule (dropped), US4 failure evidence, US5 nothing leaks and settings stay runner-only, US6 vault login (dropped; only the refusal ships), US7 artifact scan

## Phase 1: Setup (serial)

- [ ] T001 Add the new empty modules and test modules to `rust-ci-runner/src/lib.rs`, create the empty files, declare `trait Releases` and its types in `rust-ci-runner/src/releases.rs` and the kill seam `trait Killer { fn kill_run(&self, prefix: &str) -> Result<(), String>; }` in `rust-ci-runner/src/engine.rs` (bodies unimplemented, so T005 to T008 compile), add `ring`, `memchr`, `libc`, `miniz_oxide` to `rust-ci-runner/Cargo.toml` and the two new lock entries (`adler2` 2.0.1, `miniz_oxide` 0.8.9) plus the four dependency names of the root package to `rust-ci-runner/Cargo.lock` by hand; check both checksums against the crates.io index; land it alone on a branch first so the selftest proves `--locked` (plan.md T001)

## Phase 2: Tests first, runner 0.4.0 (parallel, one file each; they serve all stories)

- [ ] T002 [P] [US5] Write the runner-key, single-call, key-format, name-refusal and watch-key tests in `rust-ci-runner/src/store_tests.rs`, `rust-ci-runner/src/secret_names_tests.rs`, `rust-ci-runner/src/watch_tests.rs`; cite `data-model.md` `Runner keys.FACTORY_CI_STATE_KEY`, `Runner keys.FACTORY_CI_ARTIFACT_KEY`, `Watch entry additions.state`, `Watch entry additions.vault_items` (FR-019, FR-020, FR-034, FR-043, FR-059)
- [ ] T003 [P] [US1] Write the seal tests, including the exact-64-KiB case and the `decrypt-artifact` tests (key from the environment only, never echoed, one generic stderr line, truncated stream exits non-zero), in `rust-ci-runner/src/seal_tests.rs` against contracts/asset-format.md (FR-027)
- [ ] T004 [P] [US1] Write the declaration tests, the tag-injectivity table and the project-name rules in the `tests` module of `rust-ci-runner/src/pipeline.rs`; cite `data-model.md` Pipeline file additions (FR-017, FR-018, FR-042, FR-059)
- [ ] T005 [US2] Write the release-client tests in `rust-ci-runner/src/releases_http_tests.rs` and the `Date` parser test in `rust-ci-runner/src/util.rs`, plus the in-memory `FakeReleases` and a recording `FakeKiller` (both append to one shared event log, so T006 can assert order) in `rust-ci-runner/src/fake.rs` (FR-013, FR-036, FR-051, FR-054)
- [ ] T006 [P] [US2] Write the queue, race, budget, renewal, cancel-and-kill (the ticket is deleted only after the containers stopped) and signal tests in `rust-ci-runner/src/queue_tests.rs` over `FakeReleases`, at most 6 joined threads per test (FR-006 to FR-016, FR-052 to FR-054, SC-002); cite `data-model.md` `Ticket.name`, `Ticket.label`
- [ ] T007 [P] [US4] Write the bundle and scan tests in `rust-ci-runner/src/bundle_tests.rs` and `rust-ci-runner/src/scan_tests.rs`: the sentinel for plain, base64, one line of a multi-line value, `trace.network` inside a zip, a nested zip, the FR-050 state strings, depth 3, `.html` and unreadable members (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058, SC-008); cite `data-model.md` `Artifact bundle.plaintext`, `Artifact bundle.asset`
- [ ] T008 [P] [US1] Write the state tests (a cookie set during the run that appears in a trace refuses the artifact; the chown helper holds no secret in argv or environment) in `rust-ci-runner/src/state_tests.rs`, including the step-window tests (absent before, 0600 inside, gone after, never seen by another step), the `/proc/self` and FIFO cases and the manual-run restore (FR-021 to FR-027, FR-050, FR-055, SC-005); cite `data-model.md` `State asset.plaintext`, `State asset.asset`
- [ ] T009 [US5] Write the `ci::run` integration tests (a manual dispatch behaves as before, FR-005) in `rust-ci-runner/src/ci_state_tests.rs`: three-run counter, four-run queue, the missing `--entry` flag (`bad arguments`), every refusal line, the vault refusal (`vault unavailable` before a ticket), hostile fixture with canaries for all thirteen runner keys, one `bws` call, every Loki string of contracts/loki-lines.md; plus the own-container tests of D19 (attempt from the container name, environment fallbacks, failed `inspect` and empty image end `runner fault` before a ticket in a stateful run, no `inspect` in a stateless run), `job_limit_equals_the_workflow_timeout` (reads `.github/workflows/factory-monorepo-ci.yml` read only), and the tick-path tests of FR-062 in `rust-ci-runner/src/job_tests.rs` (no store call, restore, save or bundle; the state step sees no file; a vault declaration ends `vault unavailable`) (FR-005, FR-029 to FR-031, FR-039, FR-045, FR-048, FR-051 to FR-062, SC-002, SC-004 to SC-006, SC-008)
- [-] T010 [US3] Dropped by the owner rule: updating `.github/scripts/test-factory-monorepo-ci.sh` is a script edit, and the workflow it would check does not change.

## Phase 3: Code, runner 0.4.0 (parallel by file, after T001 and each file's tests)

- [ ] T011 [P] [US5] Grow `RUNNER_KEYS` to thirteen, keep the other nine rows in `Held` from the one call, add the key checks in `rust-ci-runner/src/store.rs`, and add `state` and `vault_items` to `rust-ci-runner/src/watch.rs` (FR-019, FR-020, FR-034, FR-043, FR-059)
- [ ] T012 [P] [US1] Add `[state]`, `[artifacts]`, `Step.state`, `Step.vault` and their validation to `rust-ci-runner/src/pipeline.rs` (FR-017, FR-018, FR-042, FR-059); cite `data-model.md` Pipeline file additions
- [ ] T013 [P] [US1] Implement `seal` and `open` in `rust-ci-runner/src/seal.rs` (FR-027)
- [ ] T014 [P] [US2] Implement `trait Releases`, `HttpReleases` and `parse_http_date` in `rust-ci-runner/src/releases.rs` and `rust-ci-runner/src/util.rs` (FR-013, FR-036, FR-051, FR-054)
- [ ] T015 [US2] Implement the queue, the renewal-failure stop (cancel flag, kill the run's containers, wait, then delete the ticket) and the signal handler in `rust-ci-runner/src/queue.rs` and `rust-ci-runner/src/signal.rs` (FR-006 to FR-016, FR-052 to FR-054); after T014 and T018; cite `data-model.md` `Ticket.label`
- [ ] T016 [US4] Implement the bundle writer, the scan and the artifact upload in `rust-ci-runner/src/bundle.rs` and `rust-ci-runner/src/scan.rs` (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058); after T014; cite `data-model.md` `Artifact bundle.plaintext`
- [ ] T017 [US1] Implement restore, `StateHook` (the `after` hook adds the state strings to the log before any artifact) and save in `rust-ci-runner/src/state.rs` (FR-021 to FR-027, FR-050, FR-055); after T013, T014 and T018; cite `data-model.md` `State asset.plaintext`
- [ ] T018 [P] [US5] Add the nine fault lines and the queue line to `rust-ci-runner/src/log.rs`, `StepHook`, `Engine::kill_run` and `Engine::own_paths` (`--network none`, no `-e`, `--entrypoint chown`) to `rust-ci-runner/src/engine.rs`, and `RunExtras` (deadline, hook, cancel flag) to `run_pipeline` in `rust-ci-runner/src/job.rs`, add `Engine::inspect_self` (D19) to `engine.rs`, and make `job` and `local` ignore `[state]`, `[artifacts]` and `state = true` and refuse a vault declaration with `vault unavailable` in `job.rs` (FR-035, FR-037, FR-045, FR-052, FR-062)
- [ ] T019 [US1] Wire `rust-ci-runner/src/ci.rs` and `rust-ci-runner/src/stateful.rs`: no `--entry` flag (names carry the entry `0`); resolve the attempt and the runner image once through `Engine::inspect_self` with the environment overrides (D19), `runner fault` before any ticket when a stateful run has no image; `JOB_LIMIT` of 45 minutes and the deadlines of D16; before any line that can carry a value (steps 1 to 5 print fixed text) and before any store call, `log.add_secrets` of the state token, both keys, the store repo name and the `x-access-token:` base64 (FR-061); the budget end through `Seams.timing`; the vault refusal before a ticket; the store identity; the D7 order (FR-005, FR-006 to FR-009, FR-019 to FR-027, FR-045, FR-049 to FR-055, FR-059, FR-061); after T011 to T018
- [ ] T020 [P] [US4] Add `decrypt-artifact` and the signal install to `rust-ci-runner/src/main.rs` (FR-033); after T013 and T015
- [ ] T021 Set the version to `0.4.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, and add the `releases_http_tests::` and `ci_state_tests::` process groups with the matching `--skip` flags to `rust-ci-runner/.factory-ci.toml` (FR-038); after T019
- [-] T022 [US3] Dropped by the owner rule (reversed): the tick workflow stays, and so do the selftest workflow, the scripts and the READMEs' tick text. FR-060 now says the tick is untouched.
- [-] T023 [US3] Dropped by the owner rule: rewriting `.github/workflows/factory-monorepo-ci.yml` (cron lines, `case`, 210 minutes, no `concurrency`, `-e` flags, Environment) is a workflow edit.

**Checkpoint**: T001 to T021 are on `main`, the selftest and `make lint` pass, and the `rust-ci-runner` pipeline passes on that commit. `git diff --stat` of `.github` and `rust-ci-runner/Dockerfile` against the branch point is empty.

## Phase 4: Release 0.4.0 (serial, after the checkpoint); the pin and the real runs are dropped

- [ ] T024 Tag `rust-ci-runner-v0.4.0` on the passing `main` commit (run id and SHA equal), wait for the existing `.github/workflows/rust-ci-runner-release.yml` (unchanged), record the digest, verify it with `docker pull`; nothing pins it (FR-038)
- [-] T025 Dropped by the owner rule: the `RUNNER_IMAGE` pin is a line of `factory-monorepo-ci.yml`. Without it no real run uses 0.4.0.
- [-] T026 Dropped (needs the pin): the no-secret canary through the new pin.
- [-] T027 Dropped by the owner rule: `rust-ci-runner/alerts/factory-ci.yaml` is a YAML file. The owner builds the alert rules in Grafana from `contracts/loki-lines.md` (FR-057).
- [-] T028 Dropped by the owner rule: `rust-ci-runner/scripts/open-artifact.sh` is a shell script. `decrypt-artifact` (T020) and the README command remain (FR-033).
- [-] T029 Dropped (needs the pin): the owner actions and the release canary on the real store repo. The owner actions stay listed in plan.md for the day the rule lifts.
- [-] T030 Dropped by the owner rule: the watch keys would be refused by the pinned `ci` runner and the tick's `plan`, and the first cron line is a workflow edit (FR-040).
- [-] T031 Dropped (needs the pin and T030): the first real run (SC-001, SC-003, SC-009 deferred).

## Phase 5: Runner 0.4.1, the vault login (dropped by the owner rule)

- [-] T032 [US6] Dropped: vault tests need the vault read, which needs `bw` in the image (a `Dockerfile` change). The vault refusal tests live in T009 and T004.
- [-] T033 [US6] Dropped: the vault read (FR-044, FR-046, FR-047, FR-056).
- [-] T034 [US6] Dropped by the owner rule: `rust-ci-runner/Dockerfile` and the release workflow smoke step may not change.
- [-] T035 [US6] Dropped: wiring the vault read; the 0.4.0 refusal stays for good.
- [-] T036 [US6] Dropped: no 0.4.1 release.
- [-] T037 [US6] Dropped: the vault owner actions and first vault run.

## Final Phase: Docs (last task)

- [ ] T038 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit`) as listed under T038 in plan.md, including the release checklist line (image digest), the `state restore failed` runbook, the Grafana rules to build by hand, the `decrypt-artifact` command, what 011 leaves out and why (schedule, vault read, pin, alert and agent files), the 5-minute wait and the concurrency group that stays, and the accepted risks (FR-041, FR-048, FR-057)

## Dependencies

- T001 first, alone (it also declares the `Releases` trait and its types).
- Tests: T002, T003, T004, T009 in parallel after T001. T005 after T001. T006, T007, T008 after T005 (they use `FakeReleases`); T009 is one file plus `job_tests.rs`.
- Code, after T001 and each file's tests: T011, T012, T013, T014, T018 in parallel. T015 after T014 and T018; T016 after T014. T017 after T013, T014 and T018. T020 after T013 and T015. T019 after T011 to T018. T021 after T019 and T020.
- Release: T024 after the checkpoint. T038 last.
- Dropped IDs (T010, T022, T023, T025 to T037) are not built and block nothing.

## Parallel example

After T001: T002, T003, T004, T005, T009 at once, then T006, T007, T008 once T005 is done. After the tests: T011, T012, T013, T014, T018 at once; then T015, T016, T017; then T020 and T019.

## Implementation strategy

0.4.0 is the only release: T001 to T021, T024 and T038. It carries US1, US2, US4, US5 and US7. US3 (schedule) and US6 (vault read) are dropped by the owner rule of 2026-11-28, which forbids any change to a workflow, a script or the Dockerfile. The image is released and not pinned, so no real run uses it until the owner lifts the rule or moves the pin. Interim logins use 010 step secrets.

## Notes

- Spec Kit: the owner rule of 2026-11-28 is a dated clarification in spec.md; spec, plan and tasks of this folder were amended in that order, and `/speckit-analyze` ran on this revision before `/speckit-implement`.
- Anything not listed here goes back through spec, plan and tasks.
