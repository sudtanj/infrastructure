---

description: "Task list for the tick-path schedule, ledger, strict queue, saved state and debug artifacts (runner 0.4.0 and 0.4.1; Rust and toml only; owner rule and owner tick decisions of 2026-11-28)"
---

# Tasks: Scheduled tick runs, a strict queue, saved state and debug artifacts

**Input**: `specs/011-ci-state-queue/` (spec.md, plan.md, research.md, data-model.md, contracts/, quickstart.md)

**Prerequisites**: 010 is merged to `main` and released (0.3.0, in the tick today) and **012 is merged to `main` first** (plan D25). Implementation branches start from that `main`, never from this documentation branch (it is behind `main`). The full brief for each task is in plan.md under "Engineer briefs", the test names under "Test plan"; IDs match. 010's and 012's real names apply (`REQUIRED_KEYS`, `fetch_with`, `Held::take`, `StepSecrets`, `NamedError`, `Redactor::forms`, `Log::secret_forms`, `Args`, `image::publishes`, `project_path_ok`, `secret_names`, `JOB_DEADLINE`); if one changed, follow it.

**Amendment of 2026-11-28 (feature 013, `specs/013-bitwarden-watch-list/`)**: 013 adds a cron line, moves the watch list to Bitwarden (the file is deleted), allows script edits for itself and adds a permanent hostile-list test (013 T008). Changes for this task list: T030 is the owner editing `secret_branches` in the Bitwarden value `FACTORY_CI_REPOS` (no pull request, no toml); every task that adds a public line, a problem line or a status text must pass the 013 hostile-list test and name entries as `entry N`; scheduled rows carry `entry`; recheck the hourly call and write budgets with ticks about every 2.5 minutes while the outside dispatcher and the cron both run; the release numbers are renumbered by the lead after 012 (0.4.0) and 013 (0.5.0).


**Tests**: Required (constitution Principle V). They come first and must fail before the code exists.

**Rules for everyone**: no file under `.github/workflows/` or `.github/scripts/` changes (owner rule of 2026-11-28, corrected the same day); `git diff --stat origin/main -- .github/workflows .github/scripts` is empty before every commit; the only `.github` file that changes is `.github/factory-ci-repos.toml` in T030, and `rust-ci-runner/Dockerfile` changes only in T034. No cargo, docker or builds on the agent VM (1 GB); tests run in the `rust-ci-runner` pipeline and the selftest. Never write, print or set a secret value anywhere. `git add` with explicit paths. `redact.rs`, `watch.rs`, `log.rs` and every new file except `vault.rs` must not name the vendor (`rust-ci-runner/tests/no_vendor.rs`). The state token, keys and vault credentials reach only `releases.rs`, `ledger.rs`, `seal.rs` and `vault.rs`; the plan code never holds them. No agent ever holds a Bitwarden credential. Use membership asserts for `RUNNER_KEYS` and `FAULTS`, never counts. New data goes into `Args` or `StatefulSeams`, not new parameters.

Constrained fields are defined in `data-model.md`; tasks cite them as `data-model.md` `Entity.field`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 state kept between runs, US2 strict queue, US3 schedule on the tick, US4 failure evidence, US5 nothing leaks and settings stay runner-only, US6 vault login (0.4.1), US7 artifact scan

## Phase 1: Setup (serial)

- [ ] T001 Add the new empty modules and test modules to `rust-ci-runner/src/lib.rs` (`seal`, `releases`, `queue`, `signal`, `state`, `bundle`, `scan`, `stateful`, `schedule`, `ledger`; `seal_tests`, `releases_http_tests`, `queue_tests`, `state_tests`, `bundle_tests`, `scan_tests`, `schedule_tests`, `ledger_tests`, `plan_schedule_tests`, `tick_state_tests`), create the empty files, declare `trait Releases` and its types in `rust-ci-runner/src/releases.rs` and the kill seam `trait Killer { fn kill_run(&self, prefix: &str) -> Result<(), String>; }` in `rust-ci-runner/src/engine.rs` (bodies unimplemented, so tests compile), add `ring`, `memchr`, `libc`, `miniz_oxide` to `rust-ci-runner/Cargo.toml` and the two new lock entries (`adler2` 2.0.1, `miniz_oxide` 0.8.9) plus the four dependency names of the root package to `rust-ci-runner/Cargo.lock` by hand; check both checksums against the crates.io index; land it alone on a branch first so the selftest proves `--locked` (plan.md T001)

## Phase 2: Tests first, runner 0.4.0 (parallel, one file each; they serve all stories)

- [ ] T002 [P] [US5] Write the runner-key, intake, key-format, name-refusal and gate tests (`take_runner_env`, `store_allowed` table, `state` and `vault_items` stay unknown watch keys) in `rust-ci-runner/src/store_tests.rs`, `rust-ci-runner/src/secret_names_tests.rs`, `rust-ci-runner/src/watch_tests.rs`; cite `data-model.md` `Runner keys.FACTORY_CI_STATE_KEY`, `Runner keys.FACTORY_CI_ARTIFACT_KEY`, `Watch entry` (FR-019, FR-020, FR-034, FR-059, FR-066, FR-067)
- [ ] T003 [P] [US1] Write the seal tests, including the exact-64-KiB case and the `decrypt-artifact` tests (key from the environment only, never echoed, one generic stderr line, truncated stream exits non-zero), in `rust-ci-runner/src/seal_tests.rs` against contracts/asset-format.md (FR-027)
- [ ] T004 [P] [US1] Write the declaration tests, the tag-injectivity table and the project-name rules in the `tests` module of `rust-ci-runner/src/pipeline.rs`; cite `data-model.md` Pipeline file additions (FR-017, FR-018, FR-042, FR-059)
- [ ] T005 [US2] Write the release-client tests in `rust-ci-runner/src/releases_http_tests.rs` and the `Date` parser test in `rust-ci-runner/src/util.rs`, plus the in-memory `FakeReleases` and a recording `FakeKiller` (both append to one shared event log) in `rust-ci-runner/src/fake.rs` (FR-013, FR-036, FR-051, FR-054)
- [ ] T006 [P] [US2] Write the queue, race, budget, renewal, cancel-and-kill and signal tests in `rust-ci-runner/src/queue_tests.rs` over `FakeReleases`, at most 6 joined threads per test (FR-006 to FR-016, FR-052 to FR-054, SC-002); cite `data-model.md` `Ticket.name`, `Ticket.label`
- [ ] T007 [P] [US4] Write the bundle and scan tests in `rust-ci-runner/src/bundle_tests.rs` and `rust-ci-runner/src/scan_tests.rs` (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058, SC-008); cite `data-model.md` `Artifact bundle.plaintext`, `Artifact bundle.asset`
- [ ] T008 [P] [US1] Write the state tests (step window, `/proc/self` and FIFO cases, cookie set during the run, the helper holds no secret) in `rust-ci-runner/src/state_tests.rs` (FR-021 to FR-027, FR-050, FR-055, SC-005); cite `data-model.md` `State asset.plaintext`, `State asset.asset`
- [ ] T009 [US5] Write the tick `job` integration tests in `rust-ci-runner/src/tick_state_tests.rs` and `rust-ci-runner/src/job_tests.rs`: three-run counter, four-run queue, non-`main` rows run without the features, the `secrets_ok` contract of the frozen script, every refusal text, hostile fixture with canaries for all thirteen keys over every sink, intake removes the variables, attempt and ids and helper image (D19), `job_limit_equals_both_workflow_timeouts`, prepare-names table, scheduled run flow (nothing due, claim after the turn, lost claim, cancel), every Loki string of contracts/loki-lines.md (FR-005, FR-029 to FR-031, FR-039, FR-045, FR-048, FR-051 to FR-067, FR-070, FR-072, SC-002, SC-004 to SC-006, SC-008)
- [-] T010 [US3] Dropped by the owner rule: updating `.github/scripts/test-factory-monorepo-ci.sh` is a script edit; the workflow it checks does not change.
- [ ] T040 [P] [US3] Write the schedule and ledger tests in `rust-ci-runner/src/schedule_tests.rs` and `rust-ci-runner/src/ledger_tests.rs` (the lists under "Test plan, schedule and ledger"); cite `data-model.md` `Schedule.every`, `Schedule.offset`, `Claim.name`, `Claim.label` (FR-001 to FR-004, FR-063, FR-064)
- [ ] T043 [US3] Write the plan tests in `rust-ci-runner/src/plan_schedule_tests.rs` and extend `rust-ci-runner/src/plan_hardening_tests.rs` (keep 012's `!r.secrets_ok` fixture free of `docker/` and store rows): ten-tick walk, never-ran schedule, unchanged sha, `prev == sha` marker, no `pending`, push wins, context table, status flip and 422, ignored schedules, ledger fault, blob cap, `secrets_ok` table (FR-002, FR-064 to FR-068); after T005 and T040

## Phase 3: Code, runner 0.4.0 (parallel by file, after T001 and each file's tests)

- [ ] T011 [P] [US5] Grow `RUNNER_KEYS` to thirteen, add `take_runner_env` and the key checks in `rust-ci-runner/src/store.rs`, add `WatchEntry::store_allowed` and no new key in `rust-ci-runner/src/watch.rs` (FR-019, FR-020, FR-034, FR-059, FR-066, FR-067); cite `data-model.md` `Runner keys.FACTORY_CI_STATE_KEY`, `Runner keys.FACTORY_CI_ARTIFACT_KEY`, `Watch entry`
- [ ] T012 [P] [US1] Add `[state]`, `[artifacts]`, `[schedule]` (through `schedule::parse`), `Step.state`, `Step.vault` and their validation to `rust-ci-runner/src/pipeline.rs` (FR-001, FR-017, FR-018, FR-042, FR-059); cite `data-model.md` Pipeline file additions, `Schedule.every`, `Schedule.offset`
- [ ] T013 [P] [US1] Implement `seal` and `open` in `rust-ci-runner/src/seal.rs` (FR-027)
- [ ] T014 [P] [US2] Implement `trait Releases`, `HttpReleases` and `parse_http_date` in `rust-ci-runner/src/releases.rs` and `rust-ci-runner/src/util.rs` (FR-013, FR-036, FR-051, FR-054)
- [ ] T041 [P] [US3] Implement `parse` and `due` in `rust-ci-runner/src/schedule.rs` (FR-001, FR-002); cite `data-model.md` `Schedule.every`, `Schedule.offset`; after T040
- [ ] T042 [US3] Implement names, labels (`r`, `p`, `f`; no `e`), ordering by `(slot, n)`, the 4xx compare-and-delete of half-created claims, the release re-fetch, the stale rule, `decide`, `claim`, `finish`, `sweep` and the read-only plan reader in `rust-ci-runner/src/ledger.rs` (FR-003, FR-063 to FR-065); cite `data-model.md` `Claim.name`, `Claim.label`; after T014 and T041
- [ ] T015 [US2] Implement the queue, the renewal-failure stop (cancel flag, kill the run's containers, wait, then delete the ticket) and the signal handler (ticket first, then the claim) in `rust-ci-runner/src/queue.rs` and `rust-ci-runner/src/signal.rs` (FR-006 to FR-016, FR-052 to FR-054, FR-064); after T014 and T018; cite `data-model.md` `Ticket.label`
- [ ] T016 [US4] Implement the bundle writer, the scan and the artifact upload in `rust-ci-runner/src/bundle.rs` and `rust-ci-runner/src/scan.rs` (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058); after T014; cite `data-model.md` `Artifact bundle.plaintext`
- [ ] T017 [US1] Implement restore, `StateHook` and save in `rust-ci-runner/src/state.rs` (FR-021 to FR-027, FR-050, FR-055); after T013, T014 and T018; cite `data-model.md` `State asset.plaintext`
- [ ] T018 [P] [US5] Add the nine fault lines and the queue line to `rust-ci-runner/src/log.rs`, a `completed` flag to `RunExtras` shared with the signal path, `StepHook`, `Engine::kill_run` and `Engine::own_paths` (clone image, `--network none`, no `-e`, `--entrypoint chown` then `chmod`) to `rust-ci-runner/src/engine.rs`, and `RunExtras` (deadline, hook, cancel flag) to `run_pipeline` in `rust-ci-runner/src/job.rs`; `job` and `local` ignore the new keys on rows not allowed the store and refuse a vault declaration (FR-035, FR-037, FR-045, FR-052, FR-062, FR-067, FR-072)
- [ ] T044 [US3] Add `Api::file`, `TreeEntry.sha` with one read per distinct blob sha, `pipeline::parse` validation, the schedule-context backoff, push rows kept on a ledger fault, due and scheduled rows, `secrets_ok = store_allowed || image rule`, no `pending` and no write-cap use for scheduled rows, order and drop rules, summary counts to `rust-ci-runner/src/plan.rs`, `rust-ci-runner/src/github.rs`, `rust-ci-runner/src/fake.rs` (FR-002, FR-064 to FR-068); after T011, T042, T043 and the 012 merge
- [ ] T045 [P] [US5] Make the manual `ci` refuse `[state]`, `[artifacts]`, `[schedule]`, `state = true` and vault declarations with `[runner] state not allowed` in `rust-ci-runner/src/ci.rs`, with tests in `rust-ci-runner/src/ci_tests.rs` (FR-069); after T012
- [ ] T046 [US3] Make `job::status` route by the latest own status of the push context (`pending` posts to the push context as today, anything else to `factory-ci-schedule[/<project>]`, a failed read posts nothing) in `rust-ci-runner/src/job.rs`, with the cancel, refuse and prepare-failure tests in `rust-ci-runner/src/job_tests.rs` that pin the push context unchanged (FR-073, FR-074, D26); after T018
- [ ] T019 [US1] Wire the tick `job` in `rust-ci-runner/src/job.rs` and `rust-ci-runner/src/stateful.rs`: order turn, claim, restore; the label `p` or `f` right after the pipeline returns; scheduled deadline of 10 minutes; `schedule silent exit` line; pre-claim failures end in a schedule-context `error`; cancel rule (delete only while `r` and no step completed, else `f`); prepare names (D7), intake through `take_runner_env` before any thread, `log.add_secrets` first, derived store repo, the order of D7 steps 1 to 8, attempt from `GITHUB_RUN_ATTEMPT`, ids from `--run-id`, clone-image helper, `JOB_LIMIT` 45 minutes, the budget through `StatefulSeams.timing`, scheduled-row flow and status rules, rows without the store unchanged, the workflow-timeout test (FR-005, FR-006 to FR-009, FR-019 to FR-027, FR-045, FR-049 to FR-055, FR-059, FR-061, FR-066 to FR-070); after T011 to T018, T042 and the 012 merge
- [ ] T020 [P] [US4] Add `decrypt-artifact` and the `job` signal install to `rust-ci-runner/src/main.rs` (FR-033); after T013 and T015
- [ ] T021 Set the version to `0.4.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, and add the `releases_http_tests::` and `tick_state_tests::` process groups with the matching `--skip` flags to `rust-ci-runner/.factory-ci.toml` (FR-038); after T019, T020, T044, T045, T046
- [-] T022 [US3] Dropped by the owner rule (reversed): the tick workflow stays; so do the selftest workflow and the scripts.
- [-] T023 [US3] Dropped by the owner rule: rewriting `.github/workflows/factory-monorepo-ci.yml` is a workflow edit.

**Checkpoint**: T001 to T021 and T040 to T046 are on `main` after 012, the selftest and `make lint` pass, and the `rust-ci-runner` pipeline passes on that commit. `git diff --stat` of `.github/workflows` and `.github/scripts` against `origin/main` is empty.

## Phase 4: Release 0.4.0 and the tick (serial, after the checkpoint)

- [ ] T024 Tag `rust-ci-runner-v0.4.0` on the passing `main` commit (run id and SHA equal), wait for the existing release workflow (unchanged), record the digest, verify it with `docker pull`; nothing pins it in the manual workflow (FR-038)
- [-] T025 Dropped by the owner rule: the `RUNNER_IMAGE` pin is a line of `factory-monorepo-ci.yml`.
- [ ] T039 Tick variable: record the current `RUNNER_IMAGE_DIGEST` (`gh variable get`), set it to the digest verified in T024 (`gh variable set RUNNER_IMAGE_DIGEST --body sha256:<digest>`), watch two ticks (`fail_tick=0`, the same commit statuses, no scheduled row, no new public line), and on any failure set the recorded old digest back; owner-run, or agent-run when the owner approves it in writing; repeat with the 0.4.1 digest after T036; tell feature 012's owner first (FR-038, plan D20); after T024
- [ ] T030 [013: now an owner edit of the Bitwarden value `FACTORY_CI_REPOS`, not a toml pull request; the rest of this line is history] `secret_branches = ["main"]` pull request (its text says the key also enables declared step secrets for the manual `ci` on `main`): add it to the `sudtanj/factory-monorepo` entry of `.github/factory-ci-repos.toml` and update the comment block; check `plan --check` with 0.4.x and 0.3.0; merges only after T039, never before (D20, FR-040)
- [ ] T027 [P] Write the Grafana alert rules (including more than 3 silent exits in a row and a tick over 20 minutes) in `rust-ci-runner/alerts/factory-ci.yaml` matching `contracts/loki-lines.md` (ledger lines included) and `FAULTS`, and a Rust test in `rust-ci-runner/src/tick_state_tests.rs` that reads the file and the contract and asserts every string is in both; after T024 (FR-057)
- [ ] T028 [P] Write `rust-ci-runner/scripts/open-artifact.sh` (pinned-sha256 runner binary, decrypt with exit check, `tar -x --no-same-owner` into a new empty directory, sanitised capped output), its shell test `rust-ci-runner/scripts/test-open-artifact.sh`, and the `rust-ci-runner/.factory-ci.toml` step that runs it; both scripts live under `rust-ci-runner/scripts/`, never under `.github/scripts/`; after T024 (FR-033)
- [ ] T026 Canary through the tick with a throwaway fixture project on `main` of a scratch private repo: names resolved, claim created and finished, one status, no canary in any sink, a second tick runs nothing for the slot; after T039 and T030; owner or an approved agent (quickstart section 4)
- [ ] T029 Owner actions (seed the ledger release, add the clone token scope) and the release canary on the real store (tag lookup lag and the 403 or 404 of a token without contents scope included; the quickstart section 4 table, including 20 races for one slot, the names route and the clone-image helper); owner; after T039 and T030
- [ ] T031 First real runs through the tick (quickstart section 5): SC-001, SC-003 and SC-009 measured; after T029 and the project toml change

## Phase 5: Runner 0.4.1, the vault login (after T024; the release workflow is not edited)

- [ ] T032 [P] [US6] Write the vault tests in `rust-ci-runner/src/vault_tests.rs` (fake `bw`), add `vault` and `vault_tests` to `rust-ci-runner/src/lib.rs`, and add the vault cases to `rust-ci-runner/src/tick_state_tests.rs` (FR-042 to FR-047, FR-056, SC-007)
- [ ] T033 [US6] Implement the vault read and wipe in `rust-ci-runner/src/vault.rs` and allow it in `rust-ci-runner/tests/no_vendor.rs` (FR-043 to FR-047, FR-056); cite `data-model.md` `Vault read.map`
- [ ] T034 [P] [US6] Add Node and the pinned `bw` bundle (sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3`), the wrapper and `ARG TARGETARCH` with `RUN bw --version || [ "$TARGETARCH" != amd64 ]` to `rust-ci-runner/Dockerfile` only; record image size and pull time (FR-044)
- [ ] T035 [US6] Replace the 0.4.0 vault refusal with the read in `rust-ci-runner/src/job.rs`, `rust-ci-runner/src/stateful.rs` and `rust-ci-runner/src/engine.rs`, add the five vault names to the prepare names for `main` rows that declare a vault list, and set the version to `0.4.1` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock` (FR-044 to FR-047, FR-056, FR-066); after T032 to T034
- [ ] T036 [US6] Tag `rust-ci-runner-v0.4.1`, record the digest, then update the pinned sha256 in `rust-ci-runner/scripts/open-artifact.sh` in its own commit; nothing pins it in the manual workflow; then repeat T039 with the new digest
- [ ] T037 [US6] Vault owner actions (dedicated account, one collection, opaque item names, the five keys in Bitwarden) and the first vault run through the tick on `main` (quickstart section 5 step 5); after T036

## Final Phase: Docs (last task)

- [ ] T038 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit`) as listed under T038 in plan.md and FR-041: the schedule format and tick behaviour, the `main`-only rule and literal `secret_branches`, the per-repo Bitwarden keys, the store repo and the read scope, the runbooks (`state restore failed`, a stuck claim), the release checklist line (image digest, agent script sha, the tick variable), that a project using the new keys cannot run through the frozen manual `ci`, the Grafana rules, the `decrypt-artifact` command and agent kit, what 011 leaves out and why, and the accepted risks; stop-slop applies (FR-041, FR-048, FR-057)

## Dependencies

- Merge order: 012 first (plan D25). T001 to T009, T040 and the code tasks that touch none of `plan.rs`, `job.rs`, `engine.rs`, `projects.rs` may proceed on a branch from current `main`; T019, T044 and anything in those four files wait for 012.
- T001 first, alone (it also declares the `Releases` trait and its types).
- Tests: T002, T003, T004, T009 and T040 in parallel after T001. T005 after T001. T006, T007, T008 after T005 (they use `FakeReleases`). T043 after T005 and T040.
- Code, after T001 and each file's tests: T011, T012, T013, T014, T018 and T041 in parallel. T042 after T014 and T041. T015 after T014 and T018; T016 after T014; T017 after T013, T014 and T018. T044 after T011, T042, T043 and 012. T045 after T012; T046 after T018. T019 after T011 to T018, T042 and 012. T020 after T013 and T015. T021 after T019, T020, T044, T045, T046.
- Release: T024 after the checkpoint. Then in this order: T039, T030, then the owner actions (T029) and T026, then the project toml change, then T031. T027 and T028 after T024 (parallel with T039).
- Phase 5 starts after T024: T032 and T034 in parallel; T033 after T032; T035 after T032 to T034; T036 after T035; T037 after T036; T039 repeats after T036. T038 last.
- Dropped IDs (T010, T022, T023, T025) are not built and block nothing.

## Parallel example

After T001: T002, T003, T004, T009, T040 and T005 at once, then T006, T007, T008 and T043 once T005 is done. After the tests: T011, T012, T013, T014, T018, T041 at once; then T015, T016, T017, T042 and T045; then T044 and T019 (both after 012) and T020; then T021.

## Implementation strategy

0.4.0 (T001 to T021, T040 to T046, T024, T027, T028) carries US1, US2, US3, US4, US5 and US7. The vault (US6, SC-007) is its own 0.4.1 phase, T032 to T037, so a vault problem never blocks the schedule, queue and state. Suggested MVP: US3 and US1 together on the tick (T001, T040 to T044, T011 to T014, T017, T018, T019): a scheduled run that claims, restores and saves. Then US2 and US4. The manual `ci` stays on 0.3.0 and refuses the new declarations (T045). The tick takes each release through the repository variable (T039); the project toml keys land only after that, because 0.3.0 refuses them. Interim logins use 010 step secrets, which the tick passes on `main`.

## Notes

- Spec Kit: the owner rule and the owner tick decisions are dated clarifications in spec.md; spec, plan and tasks of this folder were amended in that order, and `/speckit-analyze` ran on this revision before `/speckit-implement`.
- Anything not listed here goes back through spec, plan and tasks.
