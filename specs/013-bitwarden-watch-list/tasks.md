---

description: "Task list for the Bitwarden watch list and the 5-minute schedule"
---

# Tasks: Bitwarden watch list and a 5-minute schedule for the factory CI tick

**Input**: `specs/013-bitwarden-watch-list/` (spec.md, plan.md, research.md, data-model.md, contracts/runner-cli.md, contracts/tick-steps.md, quickstart.md)

**Prerequisites**: plan.md, spec.md. The full brief of each task is in plan.md under "Ordered steps and engineer briefs"; IDs match. 012 is merged to `main` first (plan.md "Merge order and conflicts"); rebase on `main` before T003.

**Tests**: required (constitution Principle V). Tests come first; each carries `#[ignore = "test-first"]` where the code is missing, and the marker comes off with the code.

**Rules for everyone**:
- No cargo, docker, builds or tests on the agent VM (low RAM). Tests run in the selftest and the `rust-ci-runner` pipeline. Git and file reads only.
- Never write a secret value, token or real watched repo name anywhere: files, logs, commits, messages, test fixtures (use `SECRETMARK_*` markers and `owner/name`).
- No agent calls `bws` or `bw`, reads the Bitwarden token or creates Bitwarden keys. Every Bitwarden action is the owner's (T001, T002, T023, T025).
- `watch.rs` and `plan.rs` must not mention `bws` or `bitwarden` (`tests/no_vendor.rs`).
- No file under `.github/scripts/` changes; `git diff --stat origin/main -- .github/scripts` is empty before every commit. Workflow edits are limited to the five files the owner named.
- Nothing that holds a token or the list text derives `Debug`. No `#[allow(clippy::...)]`.
- `git add` with explicit paths. Never force-push. Code lands only through the selftest on the branch.
- Delivery is three pull requests: PR 1 runner (T003 to T014), PR 2 tick, stub, pin, constitution (T015, T016, T019 to T022), PR 3 cron and selftest (T024). T026 last.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 tick reads Bitwarden, US2 bad list fails closed, US3 numbers only, US4 manual `ci`, US5 schedule, US6 stub, US7 pre-check

## Phase 1: Owner setup (before any merge)

- [ ] T001 [Owner] Create the Bitwarden key `FACTORY_CI_REPOS` in the project named by `FACTORY_CI_BWS_PROJECT`, holding the current two `[[watch]]` tables of `.github/factory-ci-repos.toml` (plus the 012 T020 or 011 T030 edits if they landed). Must happen before T019 merges, or the tick watches nothing. Accept: exactly one key of that name; after T011, piping the value into `rust-ci-runner plan --repos - --check` prints `2 entries ok`. Maps to US1, FR-015, SC-002.
- [ ] T002 [Owner] Confirm the Bitwarden secret value size limit, that the plan job's access token can read the project, and that the variable `FACTORY_CI_BWS_PROJECT` holds that project's id. Record the answers on this line. Accept: a recorded yes or no for each; a limit below the list size stops the feature (plan.md "Open risks").

## Phase 2: Tests first (PR 1)

- [ ] T003 [P] [US2] Write `parse_private` tests in `rust-ci-runner/src/watch_tests.rs`: every message of the problem table in `contracts/runner-cli.md`; a marker property loop (repo, glob, secret name, id string, unknown key, TOML bytes all marker strings) in which no error text holds a marker; `number` counts from 1 and counts invalid entries; `parse` still names repos; `parse` and `parse_private` accept the same texts (I3). Cites `data-model.md` `WatchEntry.number`. Accept: tests fail before T009 and pass after.
- [ ] T004 [P] [US2] Write store tests in `rust-ci-runner/src/store_tests.rs`: `watch_text` for missing, two rows, empty, blank, null, non-string, one row among 30, wrong case, newlines and tabs accepted; `attempt` passes `--project` as one argv entry and refuses `--x`, empty, 65 characters and a space; `fetch_json` retries once on a transient failure and never on 400, 401 or 403. Cites `data-model.md` `ListError` and key `FACTORY_CI_REPOS`. Accept: as listed.
- [ ] T005 [P] [US2] Write `rust-ci-runner/tests/watch_list.rs` with a fake `bws` script: valid list writes the exact text with mode 0600; each failure of FR-003 gives exit 1, the fixed stderr line of `contracts/runner-cli.md`, no file, and removes a pre-existing file; a hostile list leaves no marker in stdout, stderr or the directory; a token echoed by the fake `bws` never reaches stderr; no token prints `watch list: BWS_ACCESS_TOKEN is not set`; a missing `--out` directory prints `watch list: cannot write the output file`. Accept: as listed.
- [ ] T006 [P] [US3] Write plan tests in `rust-ci-runner/src/plan_tests.rs`, `plan_error_tests.rs`, `plan_cap_tests.rs` and `rust-ci-runner/tests/cli.rs`: the summary over marker repos holds `- item N` and no marker (I4); a problem line starts `entry K:`; K survives the docker partition; the unchecked list holds `entry N`; an API error that quotes a repo name is masked; the private-repo refusal says `entry N`; `plan_a` with an empty list exits 1 and writes no candidates file (US2.6). Update the existing tests that expect repo names in problems. Accept: as listed.
- [ ] T007 [P] [US4] Write `mod list_store` at the end of `rust-ci-runner/src/ci_tests.rs` with a fake `bws` serving the four settings and the list under the store source: listed repo proceeds with exactly one `bws` call (I6); unlisted prints the old line; empty token prints the old line and makes no call (`smoke_unlisted_without_token`); missing, duplicate, empty, malformed and 101-entry lists print `watch list unusable`; no marker reaches stdout or stderr. Existing file-based tests keep the file source. Accept: as listed.
- [ ] T008 [P] [US7] Write `plan --repos - --check` tests in `rust-ci-runner/tests/cli.rs`: valid stdin prints `N entries ok`; invalid prints `entry N:` lines, exit 1, no marker; empty stdin prints `watch list: stdin is empty`; zero tables prints `watch list: FACTORY_CI_REPOS has no entries`; `--repos -` without `--check` is a usage error. Accept: as listed.

## Phase 3: Runner implementation (PR 1)

- [ ] T009 [US2] Implement `number` and `parse_private` in `rust-ci-runner/src/watch.rs` with one shared body and a `private` flag; messages from the fixed table only; `parse` output unchanged byte for byte; no vendor word in the file. Cites `data-model.md` `WatchEntry.number` and the Key `FACTORY_CI_REPOS` value limits (64 KiB, 100 entries, 20 items; unchanged, not restated). Accept: T003 and the old `watch_tests` pass.
- [ ] T010 [US2] Implement in `rust-ci-runner/src/store.rs`: `project` argument of `attempt` with its validation, `fetch_json`, `watch_text`, `ListError`, the key constant; `fetch_with` unchanged for callers; the list value type has no `Debug`. Cites `data-model.md` `ListError` and Key `FACTORY_CI_REPOS`.`Name`. Accept: T004 and the old `store_tests` pass.
- [ ] T011 [US1] [US2] [US7] Implement in `rust-ci-runner/src/main.rs`: the `watch-list` command (token read and removed before any thread, 2 attempts at 60 s, `watch_text`, `parse_private`, zero entries refused, write with `write_private`, remove the path on any failure, stdout empty), `plan --repos - --check`, the zero-entry refusal in `plan_a`, `silence_panics()` for `plan` and `watch-list`, redaction seeds (token, repo names, owners) in `plan_a` and `plan_c`, USAGE and its environment sentence. Cites `contracts/runner-cli.md`. Accept: T005, the `plan_a` part of T006 and T008 pass.
- [ ] T012 [US3] Implement in `rust-ci-runner/src/plan.rs`: `entry` on rows and the status records that carry a repo, `problem()` and `summary()` by number, `unchecked` as `entry N`; `Candidates` still round-trips through serde. Cites `data-model.md` `Plan rows`. Accept: T006 passes and no `summary()` output holds a repo name for any input.
- [ ] T013 [US4] Implement in `rust-ci-runner/src/ci.rs` and `rust-ci-runner/src/log.rs`: `ListSource` seam (`Store` in `Seams::production`, `File` in the test constructor); the store path of plan.md D4 with one `bws` call; fault `watch list unusable` in `FAULTS` and `is_fault`; remove `BWS_ACCESS_TOKEN is not set` from `FAULTS` and update the tests that expected it; the token joins the redactor. Accept: T007 passes, existing `ci_tests` pass, the release smoke expectation holds.
- [ ] T014 Set the version to `0.5.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`. Accept: `cargo_toml_and_lock_versions_match` in `rust-ci-runner/tests/repo_files.rs` passes.

## Phase 4: PR 1 gate

- [ ] T017 [Lead] Push the branch, wait for a green `factory-ci-selftest` on its head commit (it covers `--lib`, `--test '*'`, `make lint`, the shell tests and the workflow lint), merge PR 1, tag `rust-ci-runner-v0.5.0`, confirm the release smoke test passes, record the digest, `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`. Accept: smoke test green; digest recorded on this line. Depends on T003 to T014.
- [ ] T018 [Owner] Dispatch one tick with the new runner and the old workflow. Accept: green tick, same rows as before (the runner change alone moved nothing). Depends on T017.

## Phase 5: Tick, stub, pin, constitution (PR 2)

- [ ] T015 [P] [US6] Test first, in `rust-ci-runner/tests/repo_files.rs`: add `committed_watch_file_lists_no_repo` (zero entries, no line matching `^\s*repo\s*=`); keep `repeated`, `entry_problems` and their self-tests on fixture text; delete the real-file tests `no_secret_name_in_two_committed_entries` and the infrastructure-entry test; note in the file header that the owner checks the real list with `plan --repos - --check`. Accept: fails while the file holds a `[[watch]]` table, passes with the stub. Depends on T017.
- [ ] T016 [P] [US1] Test first, in `.github/workflows/factory-ci-selftest.yml`: a `workflow` job step asserting the checks 1 to 3 of `contracts/tick-steps.md` (step order in the `plan` job, no `watch-list` word in `run`, `run-docker`, `verdict` or `factory-monorepo-ci.yml`, no `upload-artifact`). Accept: green on T019; red on a copy with the step moved. Depends on T017.
- [ ] T019 [US1] [US2] Edit `.github/workflows/factory-ci-tick.yml`: add `fetch watch list` and `remove watch list` exactly as in `contracts/tick-steps.md`; nothing else. Accept: the lint, the shell tests and the `workflow` selftest job are green on the branch; `git diff` of the file shows only the two steps. Depends on T016, T017.
- [ ] T020 [US4] Edit `.github/workflows/factory-monorepo-ci.yml`: set the `RUNNER_IMAGE` line to the `0.5.0` tag and the digest from T017; nothing else. Accept: `.github/scripts/test-factory-monorepo-ci.sh` passes in the selftest. Depends on T017.
- [ ] T021 [US6] Replace `.github/factory-ci-repos.toml` with the comment-only stub of plan.md T021 (no `[[watch]]` table, no real repo name). Accept: T015 passes; `plan --repos .github/factory-ci-repos.toml --check` prints `0 entries ok`. Depends on T001, T018.
- [ ] T022 Amend `.specify/memory/constitution.md` with the `speckit-constitution` rules: Principle I line names the Bitwarden key, Principle II lists the watch list as a Bitwarden value read by the plan job and `ci`, MINOR bump relative to `main` (plan.md D8), Sync Impact note, last-amended date. Accept: no sentence names the committed file as the source. Depends on T019.
- [ ] T023 [Owner] After PR 2 merges, dispatch one tick. Accept: green; the `fetch watch list` step prints nothing; the summary holds `item N` only; rows equal T018 (SC-002); rename the key, dispatch, see a red tick, `watch list: key FACTORY_CI_REPOS not found` and no new status, rename it back; search for each watched repo name in the run's logs and summary and find none (SC-001). Depends on T019 to T022.

## Phase 6: Schedule (PR 3)

- [ ] T024 [US5] Add `schedule:` with `- cron: "*/5 * * * *"` to `.github/workflows/factory-ci-tick.yml`, and in `.github/workflows/factory-ci-selftest.yml` the cron assertion (check 4) and the lint plus `--self-test` runs on the copy without the two schedule lines (check 5), in the `workflow` and `shell` jobs; keep the text `factory-ci-lint-workflow.sh --self-test` in the shell job. Accept: both jobs green with the cron present; a second cron line or a missing line turns the assertion red. Depends on T023.
- [ ] T025 [Owner] Watch the schedule for a week. Accept: Actions history shows a tick at least every 15 minutes in a 24-hour window, or a note of the gaps (SC-004); if the schedule was disabled or skipped for long, say so in the README. Depends on T024.

## Phase 7: Docs (last)

- [ ] T026 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit sudtanj/infrastructure --description ...`, run by the lead) to match this feature (FR-014): the list lives in `FACTORY_CI_REPOS`; the stub; owner setup (T001); the `watch-list` step and its 0600 file; `ci` reads the list in-process; the schedule and GitHub's limits (best effort, 60-day disable, main only, serial concurrency group); an idle tick now makes one Bitwarden call; `plan --repos - --check`; names stay in git history; the manual workflow input `repo` is public; the variable table notes that `FACTORY_CI_BWS_PROJECT` is also read by the watch-list step; replace README step 7 ("list repos in `.github/factory-ci-repos.toml`"), the `ci` paragraph and the input table row that point to the file. Accept: a grep of both READMEs finds no instruction to list repos in the file. Depends on all other tasks.

## Dependencies and order

- T001, T002 first (owner). T003 to T008 in parallel, then T009 to T014 (T009, T010 parallel; T011 after both; T012 after T009; T013 after T010; T014 any time after T009), then T017, T018.
- PR 2: T015, T016 in parallel, then T019, T020, T021 (T021 needs T001 and T018), T022, then T023.
- PR 3: T024, then T025. T026 last.
- Task count: 26 (T001 to T026). Story counts: US1 4 (T001, T011 shared, T016, T019), US2 7, US3 2, US4 3, US5 1, US6 2, US7 2; some tasks serve two stories.

## MVP

US1 with US2 and US3: T001 to T014, T017, T018, T019 to T023. US4 to US7 follow. Nothing changes for the owner until T021 and T023.
