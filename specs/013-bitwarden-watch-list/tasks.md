---

description: "Task list for the Bitwarden watch list, the 5-minute schedule and the clean public repo (revision 2)"
---

# Tasks: Bitwarden watch list, a 5-minute schedule, and a clean public repo

**Input**: `specs/013-bitwarden-watch-list/` (spec.md, plan.md, research.md, data-model.md, contracts/runner-cli.md, contracts/script-changes.md, contracts/history-rewrite.md, quickstart.md)

**Prerequisites**: plan.md, spec.md. The full brief of each task is in plan.md under "Ordered steps and engineer briefs"; IDs match. 012 is merged to `main` first (plan.md "Merge order and conflicts"); rebase on `main` before T004.

**Tests**: required (constitution Principle V). Tests come first; each carries `#[ignore = "test-first"]` where the code is missing, and the marker comes off with the code.

**Rules for everyone**:
- No cargo, docker, builds or tests on the agent VM (low RAM). Tests run in the selftest and the `rust-ci-runner` pipeline. Git and file reads only.
- Never write a secret value, token or real watched repo name anywhere: files, logs, commits, messages, fixtures. Use `SECRETMARK_*` markers and `owner/name`.
- No agent calls `bws` or `bw`, reads the Bitwarden token or creates Bitwarden keys. Every Bitwarden action is the owner's.
- `watch.rs` and `plan.rs` must not mention `bws` or `bitwarden` (`rust-ci-runner/tests/no_vendor.rs`).
- Nothing that holds a token or the list text derives `Debug`. No `#[allow(clippy::...)]`.
- `git add` with explicit paths. Never force-push, except T033, which the lead runs after the owner's fresh explicit go.
- Delivery is five pull requests: PR-1 runner (T004 to T014), PR-2 script, lint, heartbeat, selftest (T017 to T020), PR-3 file deletion and manual cutover (T023 to T026), PR-4 cron (T028), PR-5 tree scrub and rename (T030). T032 docs, then T033.
- Order exception, recorded: the constitution's habit of ending with the README and description update is kept as T032, but the owner ordered the irreversible, owner-gated history rewrite T033 after it, so T033 is the very last task. T033 may never run.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 tick reads Bitwarden, US2 bad list fails closed, US3 numbers only, US4 manual `ci`, US5 schedule and heartbeat, US6 file gone, US7 pre-check, US8 clean public repo

## Phase 1: Owner setup (before any merge)

- [ ] T001 [Owner] Create the Bitwarden key `FACTORY_CI_REPOS` in the project named by `FACTORY_CI_BWS_PROJECT`, holding the two `[[watch]]` tables of `.github/factory-ci-repos.toml` (plus 012's follow-up T020 additions if they landed). Accept: exactly one row with that key. Maps to US1, FR-001, FR-017.
- [ ] T002 [Owner] Confirm: the Bitwarden secret value limit (the only figure found is 25,000 characters from the SDK's input validation, unverified against the docs); the machine account is read-only and scoped to this one project; `FACTORY_CI_BWS_PROJECT` holds that project's id; Bitwarden event logs are on. Accept: each answer recorded on this line.
- [ ] T003 [Owner] H1 and M4: the Environments `factory-ci-plan` (also used by `verdict`), `factory-ci` and `factory-ci-docker` accept deployments from `main` only and have no required reviewer; the Bitwarden tokens, both GitHub tokens and the blob keys exist as Environment secrets only (delete repo-level copies); a branch ruleset protects `.github/**` and `rust-ci-runner/**`; both GitHub tokens are fine-grained and scoped to the intended repos. Accept: recorded, including that a run from a non-main branch receives no secret.

## Phase 2: Tests first (PR-1, runner)

- [ ] T004 [P] [US2] Write the parser tests in `rust-ci-runner/src/watch_tests.rs`: every message of the table in `contracts/runner-cli.md`; a marker loop over every bad input with no marker in any error; `number` counts from 1 and counts invalid entries; the new rules (id with `allow_docker` or `secrets`, `*` and `**` in `secret_branches`, zero entries, owner mismatch through `check_owner`, unset owner); rewrite the tests that expect `repo: why` messages, accept `secret_branches = ["*"]` (010's test becomes a refusal test) or build `allow_docker` entries without `id`. Cites `data-model.md` `WatchEntry.number` and Key `FACTORY_CI_REPOS`.`Rules added here`. Accept: fails before T009, passes after.
- [ ] T005 [P] [US2] Write the store tests in `rust-ci-runner/src/store_tests.rs`: `watch_text` (missing, two rows, empty, blank, null, non-string, one row among 30, wrong case, newlines kept); `--project` as one argv entry and refused when it starts with `-`, is empty, has 65 characters or a space; `fetch_json` retries once on a transient failure and never on 400, 401, 403. Cites `data-model.md` `ListError`. Accept: as listed.
- [ ] T006 [P] [US4] Write `mod list_store` at the end of `rust-ci-runner/src/ci_tests.rs`: without `--repos`, with a fake `bws`: a listed repo proceeds with exactly one call and the project argument; unlisted prints the old line; empty token prints the old line with no call (`smoke_unlisted_without_token`); missing, duplicate, empty, malformed, 101-entry and owner-mismatch lists print `watch list unusable`; no marker reaches stdout or stderr; the existing file tests keep `--repos`. Accept: as listed.
- [ ] T007 [P] [US3] Write the plan tests in `rust-ci-runner/src/plan_tests.rs`, `plan_error_tests.rs`, `plan_cap_tests.rs`: `- item N` and `entry K:` forms; K survives the docker partition; the unchecked list holds `entry N`; the id-mismatch line carries no id; the private-repo refusal says `entry N`; an API error that quotes a repo name is masked in phase A and phase C (seeded from candidate rows); `B_PRIVATE` empty, `false`, garbage and `true` through `plan_opts()` and `fail_closed`. Update the tests that expect repo names. Accept: as listed.
- [ ] T008 [US2] [US3] [US7] Write the permanent hostile-list test in `rust-ci-runner/src/plan_hostile_tests.rs` and `rust-ci-runner/tests/cli.rs`: one list of entries with unique markers for repo, owner, branch, glob, secret name, id and an unknown key, including an `allow_docker` entry without a docker token; run `parse`, `plan --repos FILE|- --check`, `plan_a`, `plan_c` and `ci` (store and file source) through a fake API and fake `bws`; no marker in stdout, stderr, summary, candidates, rows or status text. Add the `--repos -` cases (valid, invalid, empty, zero entries, usage error without `--check`) and `plan_a` with zero entries (exit 1, no candidates file). A header comment names feature 011 as bound by this test. Accept: as listed.

## Phase 3: Runner implementation (PR-1)

- [ ] T009 [US2] Implement in `rust-ci-runner/src/watch.rs` (plan.md D2): number-only messages from the fixed table, `number`, the new rules, `check_owner`; valid input parses as before; no vendor word. Cites `data-model.md` `WatchEntry.number`. Accept: T004 passes.
- [ ] T010 [US2] Implement in `rust-ci-runner/src/store.rs` (plan.md D3): `project` argument with its validation, `fetch_json`, `watch_text`, `ListError`; `fetch_with` unchanged for its callers; no `Debug` on the list value. Cites `data-model.md` `ListError`. Accept: T005 and the old `store_tests` pass.
- [ ] T011 [US3] Implement in `rust-ci-runner/src/plan.rs` (D6); audit `walk.rs` and `projects.rs`: `entry` on rows and status records, `problem()`, `summary()`, `unchecked`, the id line at `plan.rs:231` becomes `id differs from the list`; grep every `format!` that touches `e.repo`, `e.id`, `e.branches` or `r.repo` in the four files, fix each, paste the grep in the commit message. Cites `data-model.md` `Plan rows`. Accept: T007 and T008 pass.
- [ ] T012 [US2] [US3] [US5] [US7] Implement in `rust-ci-runner/src/main.rs`: `plan --repos -` with `--check` (stdin capped at 64 KiB + 1 byte); zero-entry refusal and `check_owner` in `plan_a`; `b_private: var("B_PRIVATE") == "true"`; redaction seeds in `plan_a` and `plan_c` (D6); the lines at 131 and 180 use `entry N`; the `plan` arm calls `silence_panics()` and `rust-ci-runner/src/job_tests.rs` `only_the_job_and_status_arms_of_main_silence_the_panic_hook` now requires it in `job`, `status` and `plan` and forbids it in `run` (D9). Accept: T007, T008 and that test pass.
- [ ] T013 [US4] Implement in `rust-ci-runner/src/ci.rs` and `rust-ci-runner/src/log.rs` (D4): store source when `--repos` is absent, `FACTORY_CI_BWS_PROJECT` through the env closure, `watch list unusable` in `FAULTS` and `is_fault`, `BWS_ACCESS_TOKEN is not set` removed with its tests updated, the token in the redactor. Accept: T006 passes, the existing `ci_tests` pass, the smoke expectation holds.
- [ ] T014 Set the version to `0.5.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`. Accept: `cargo_toml_and_lock_versions_match` in `rust-ci-runner/tests/repo_files.rs` passes.

## Phase 4: PR-1 gate

- [ ] T015 [Lead] Wait for a green `factory-ci-selftest` on the PR-1 head commit, merge, tag `rust-ci-runner-v0.5.0`, confirm the release smoke passes. **First record the current `RUNNER_IMAGE_DIGEST` value on this line**, then `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`. Never roll the variable back below 0.5.0 unless `gh workflow disable factory-ci-tick` runs first (the public summary would print repo names and M1 to M4 would not apply). Accept: both digests recorded. Depends on T004 to T014.
- [ ] T016 [Owner] Dispatch one tick with the new runner, the old script and the committed file. Accept: green, same rows as before; pipe the Bitwarden value into `rust-ci-runner plan --repos - --check` and see the expected `N entries ok` (repeat after every later edit of the key). Depends on T001, T015.

## Phase 5: Script, lint, heartbeat (PR-2)

- [ ] T017 [P] [US1] [US2] Test first, in `.github/scripts/test-factory-ci-secrets.sh` and `.github/scripts/test-factory-ci-hardening.sh`: `plan_world` serves `FACTORY_CI_REPOS` through the fake store and creates no file under `.github`; assert the runner stub receives `--repos` pointing at a mode-0600 file holding the exact value, the file is gone after success and after every failure, the runner stub's environment has no Bitwarden token variable, an idle tick makes exactly one `bws` call (the old "no store call" test becomes this), and each failure (no token, store down, 401, key missing, key twice, key empty, value over 64 KiB) exits 1 with the fixed summary text, runs no phase A and leaves no list text in the summary or `GITHUB_OUTPUT`. Accept: fails on the old script. Depends on T015.
- [ ] T018 [US1] [US2] Implement `fci_watch_list` in `.github/scripts/factory-ci-secrets.sh` and the new steps in `.github/scripts/factory-ci-plan.sh` as in `contracts/script-changes.md`; `fail()` removes the temp file; no `set -x`; no list text in any `printf`. Accept: T017 passes. Depends on T017.
- [ ] T019 [US5] Edit `.github/scripts/factory-ci-lint-workflow.sh`: replace rule `no-schedule` with rule `schedule`, add `heartbeat-gate` and `plan-step`, replace the self-test row; edit `.github/workflows/factory-ci-tick.yml`: remove `if: always()` from the heartbeat step only. Accept: the lint passes on the tick, `--self-test` passes, `test-factory-ci-hardening.sh` passes (it lints the real workflow). Depends on T015.
- [ ] T020 [P] [US1] [US5] Add to `.github/workflows/factory-ci-selftest.yml` a `workflow`-job step asserting the plan step has no `[c]ontinue-on-error` and no `if:`, that `run`, `run-docker` and `verdict` hold no `FACTORY_CI_REPOS` and no `fci_watch_list`, and that the tick has no `[u]pload-artifact` (bracket trick, so the lint's own greps of this file do not match). Accept: green on T018 and T019; red on a copy with a violation (documented reasoning if no run is possible). Depends on T018, T019.
- [ ] T021 [Owner] After PR-2 merges, dispatch a tick. Accept: green; rows equal T016 (SC-002); summary has `item N` only; rename the key, dispatch, see a red tick, `watch list key FACTORY_CI_REPOS not found or empty`, no new status and no heartbeat, rename back; the `plan` log names no repo. Depends on T018 to T020.

## Phase 6: Delete the file, cut over the manual workflow (PR-3)

- [ ] T022 [Owner] Right before PR-3 merges, compare the Bitwarden value with `.github/factory-ci-repos.toml`, 012's T020 additions included (the infrastructure entry gains `allow_docker` and the Docker Hub secret names). Accept: equal entries, or the Bitwarden value is corrected first. Depends on T021.
- [ ] T023 [P] [US6] Test first, in `rust-ci-runner/tests/repo_files.rs` and `rust-ci-runner/tests/fixtures/watch-list.example.toml`: `committed_watch_file_is_absent`; no other committed `.toml` outside `tests/fixtures` holds a `[[watch]]` table; keep `repeated`, `entry_problems` and their self-tests on fixture text; delete the real-file tests (the input is no longer in git); add the fixture with fake `owner/name` entries. Accept: red while the file exists.
- [ ] T024 [US4] Edit `.github/workflows/factory-monorepo-ci.yml` (still under its old name until T030), `.github/workflows/rust-ci-runner-release.yml` and `.github/scripts/test-factory-monorepo-ci.sh` as in `contracts/script-changes.md`: pin `RUNNER_IMAGE` to 0.5.0 and its digest; drop the mount and `--repos`; add `-e FACTORY_CI_BWS_PROJECT -e GITHUB_REPOSITORY_OWNER` with their `env:` lines; drop the `repo` default; the smoke step runs `ci` without `--repos` and without the mount. Accept: that shell test passes (two mounts, no `--repos`, new flags) and the lint passes. Depends on T015.
- [ ] T025 [US6] Delete `.github/factory-ci-repos.toml`; remove it from both `paths` lists of `.github/workflows/factory-ci-selftest.yml`; the `list` job runs `plan --repos <fixture> --check`. Accept: T023 passes and the selftest is green. Depends on T022, T023, T024.
- [ ] T026 Amend `.specify/memory/constitution.md` with the `speckit-constitution` rules (plan.md D10): Principle I names the key, Principle II lists the watch list as a Bitwarden value read by the plan script and `ci`, the schedule is permitted, MINOR bump relative to `main`, Sync Impact note. Accept: no sentence names the committed file as the source. Depends on T025.
- [ ] T027 [Owner] After PR-3: dispatch a tick; run the manual workflow for a listed repo and for an unlisted repo (fixed lines only); grep the new runs' logs for the scrub strings of `contracts/history-rewrite.md`. Accept: green, no hit. Depends on T025, T026.

## Phase 7: Schedule (PR-4)

- [ ] T028 [US5] Add `schedule:` with `- cron: "*/5 * * * *"` to `.github/workflows/factory-ci-tick.yml`; add the self-test row that changes the real cron to `.github/scripts/factory-ci-lint-workflow.sh`; add the `[c]ron` check (exactly one line, equal to the owner's expression) to `.github/workflows/factory-ci-selftest.yml`. Accept: lint, `--self-test`, shell tests and selftest are green; a second cron line fails. Depends on T021, T027.
- [ ] T029 [Owner] Merge PR-4 from the owner's own account (GitHub mails that account on each failed run). Document the failure email per tick and the stop command `gh workflow disable factory-ci-tick`. Watch one week; count Canceled runs (a replaced pending run) apart from Failed. Accept: SC-004 recorded. Depends on T028.

## Phase 8: Tree scrub, logs, docs, rewrite

- [ ] T030 [US8] Scrub the tree: apply the owner's replace file to the working tree mechanically, then review the diff; `git mv` the manual workflow to `.github/workflows/factory-ci-manual.yml` and its test to `.github/scripts/test-factory-ci-manual.sh`; update the `name:`, the concurrency group, the test's default path and every pin in `.github/workflows/factory-ci-selftest.yml`, `.github/workflows/rust-ci-runner-release.yml`, `README.md` (including the line that names the default repo and the input table), the constitution and specs 005 to 013 (011 and 012 after they merge). Accept: the derivation commands of `contracts/history-rewrite.md` find no string in the tree except inside that file's own derivation text; the manual-workflow shell test and the selftest pass. Depends on T029 and on 012 and 011 having merged.
- [ ] T031 [Owner] Delete the old tick and manual runs that print repo names: list with `gh run list --workflow <old workflow> --limit 1000 --json databaseId` and delete each with `gh run delete <id>` for the tick and for the old manual workflow, until `gh run list` for both is empty. Accept: both lists empty. Depends on T030.
- [ ] T032 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit sudtanj/infrastructure --description ...`, run by the lead) to match this feature (FR-016): the list lives in `FACTORY_CI_REPOS` and has no committed copy; `plan --repos - --check` and the count check after every edit; the plain Bitwarden token is now required by every tick; `ci` reads the list in-process; the schedule and its limits (best effort, 60-day disable, main only, serial group); Canceled versus Failed; the failure email per tick and `gh workflow disable factory-ci-tick`; the heartbeat rule; the accepted public leaks (counts, item numbers, run timing, the presence of `allow_docker` rows, the manual workflow's public `repo` input); the rollback caution; `FACTORY_CI_BWS_PROJECT` read by the plan script and `ci`; the new manual workflow name; replace README step 7 and the `ci` paragraph that point to the file. Accept: a grep of both READMEs finds no instruction to list repos in a file and no scrub string. Depends on T031.
- [ ] T033 [Owner-gated] **Irreversible force-push: rewrite the public history to remove the private names.** The lead runs this only after a fresh, explicit owner go and only after 012, 011 and 013 have all merged with no other branch or open pull request left. Follow `contracts/history-rewrite.md` exactly: derive strings S1 to S5 with the git commands there (the owner confirms S5); tool `git filter-repo` with `--replace-text`, `--replace-message` and `--path-rename`; private mirror backup first (B1); pause the tick and the release workflow (B2); lift and later restore the protections (B3, B7); verify before the push (B5) and on a fresh mirror clone after it (B8); effects: every SHA changes, `rust-ci-runner-v*` tags move, open PRs and branches break, the commit statuses the tick walk uses vanish, so the first tick runs each project once at each watched tip (not every commit, no failure; `walk.rs` `decide` and `walk_one`); GitHub keeps old commits reachable by SHA until Support purges them, so send the drafted request after B8 (draft only; the lead does not send it without the owner). Accept: the verification greps find nothing on a fresh mirror clone, the first tick plans exactly one row per project of each watched tip of this repo (SC-006), and the owner has the backup. Depends on T032.

## Dependencies and order

- T001 to T003 first (owner). T004 to T008 in parallel, then T009, T010 (parallel), T011 after T009, T012 after T009 to T011, T013 after T010, T014 after T009; T015, T016.
- PR-2: T017, T019 in parallel; T018 after T017; T020 after T018 and T019; T021.
- PR-3: T022 and T023 in parallel; T024 parallel with them; T025 after all three; T026; T027.
- PR-4: T028, T029. Then T030 to T032 in order. T033 last.
- Task count: 33 (T001 to T033). By story: US1 5 (T001, T017, T018, T020 and T021 shared), US2 8, US3 4, US4 3, US5 5, US6 2, US7 2, US8 4 (T030, T031, T033 and T027 in part); tasks without a story are setup, release and docs (T002, T003, T014, T015, T016, T022, T026, T029, T032).

## MVP

US1, US2, US3 with the safety rules: T001 to T021. Nothing changes for the owner's public repo until T025. The cron (T028) and the scrub (T030 onward) follow.

## Analyze result (`/speckit-analyze`, read-only)

Cross-artifact check of spec.md, plan.md, tasks.md and the contracts, run after this file was written:

- Every FR maps to a task: FR-001 T017, T018, T013; FR-002 T009; FR-003 T017; FR-004 T004, T008, T009; FR-005 T008, T011, T012; FR-006 T004, T009, T012, T013; FR-007 T017, T020; FR-008 T017, T018; FR-009 T006, T013; FR-010 T007, T012; FR-011 T019, T020, T028; FR-012 T008, T012; FR-013 T023, T025; FR-014 T024, T030; FR-015 T030, T033; FR-016 T026, T032; FR-017 the phase order. No orphan task: T002, T003, T016, T021, T022, T027, T029, T031 are owner or gate tasks with an accept line.
- Fixed in this pass: the plan's contracts list named a removed `tick-steps.md` (now `script-changes.md`); T001's accept line no longer uses a command that does not exist yet; SC-001 is reworded to the two stages (before and after the rewrite).
- Inconsistency kept on purpose and recorded: constitution VI habit of ending on the README task versus the owner's order that T033 is last (tasks header).
- Open items for the owner, not for the engineer: confirm S5 (example repo names in the old file's comment: real or example); the Bitwarden value limit (T002); whether forks of the public repo exist (T033 B-steps); who runs T015 and T033 (the lead).
- Risk the analysis cannot close without a run: the shell tests, the lint self-test and the selftest. They run only in CI.
