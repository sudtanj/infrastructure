# Tasks: Runner-owned CI

**Input**: `specs/008-runner-owned-ci/` (spec.md, plan.md; the plan holds the contracts, test names and briefs B1 to B11)

**Tests**: required by Constitution Principle V and FR-028. Rust tests sit beside each task and run only through the `rust-ci-runner` pipeline. No one runs `cargo`, `docker` or a build on the engineer's machine (1 GB); the check there is `bash -n`, grep and review.

**Format**: `[ID] [P?] [Story] Description with paths`. `[P]` means no file or ordering conflict with the other `[P]` tasks of its phase. `[owner]` marks a human action.

## Phase 1: Setup

- [x] T001 Amend `.specify/memory/constitution.md` to 1.1.0 (Principle II: the manual workflow holds only `BWS_ACCESS_TOKEN`) and set `.specify/feature.json` to this folder. Done in the commit that adds this folder. Brief: none.

## Phase 2: Shared runner changes (blocks Phase 3; all `[P]` except T003)

**Goal**: the parts of the `ci` command that also fix `job`, `plan` and `run`.

- [ ] T002 [P] [US3] Edit `rust-ci-runner/src/ship.rs` and `ship_tests.rs`: `LogConfig::from_env` strips a trailing `/`, then `/loki/api/v1/push`, then a trailing `/`; tests `url_loses_the_push_path_and_slashes`, `url_normalisation_is_idempotent`, `url_with_another_path_is_kept` (FR-015, I3). Brief B1.
- [ ] T003 [US3] Edit `rust-ci-runner/src/ship.rs` and `ship_tests.rs`: `pub fn probe(cfg, run_id) -> Result<u16, u16>` with a 15 s limit, basic auth, probe labels, no redirects; tests `probe_204_is_ok`, `probe_401_reports_the_code`, `probe_with_no_answer_reports_000_in_time`, `probe_posts_one_line_with_probe_labels_and_basic_auth` (FR-016). After T002 (same files). Brief B1.
- [ ] T004 [P] [US2] Edit `rust-ci-runner/src/log.rs`: `Log::public_events(bool)` and `public_form(who, text)`; tests `public_mode_prints_only_the_closed_grammar`, `public_mode_drops_the_reason_after_failed_and_skipped`, `public_mode_ships_the_full_text`, `public_mode_refuses_a_bad_name` (FR-018, I1). Brief B2.
- [ ] T005 [P] [US4] Edit `rust-ci-runner/src/engine.rs`: `command()` removes `BWS_ACCESS_TOKEN`; add `rust-ci-runner/src/engine_tests.rs` with `bws_token_is_absent_from_engine_children` and one `mod` line in `lib.rs` (FR-012, I9). Brief B3.
- [ ] T006 [P] [US1] Add `rust-ci-runner/src/store.rs`, `store_tests.rs` and `mod` lines in `lib.rs`: `fetch`, `parse_list`, `StoreError`, `Secrets`; the nine tests of the plan's test plan (FR-009, FR-010, FR-011, I8). Brief B4.
- [ ] T007 [P] [US3] Edit `rust-ci-runner/src/watch.rs`, `watch_tests.rs`, `job.rs`, `job_tests.rs`, `main.rs` (`load_list` only): `watch::read`, `pub(crate)` helpers, `project_dir`, and `local` fails when a passing run dropped lines; tests `read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref`, `run_quiet_logs_fails_when_a_passing_run_dropped_lines` (FR-014, FR-017). Brief B5.

## Phase 3: User Story 1, 2, 3, 4 - the `ci` command (Priority: P1)

**Goal**: `rust-ci-runner ci` runs the whole job and prints only the closed grammar.

**Independent Test**: `ci_tests` pass in the `rust-ci-runner` pipeline: exit codes, grammar, containment, no secret in output or argv.

- [ ] T008 [US1] [US2] [US3] [US4] Add `rust-ci-runner/src/ci.rs`: argument parsing and validation, watch gate, `store::fetch`, `LogConfig` from the fetched map, probe, shipper, engine check, `git` clone with the env header and fixed host, `project_dir`, `prepare` with no secrets and the docker gate, `run_pipeline` in quiet public mode, exit-code table, fixed stderr list (FR-006 to FR-008, FR-013, FR-016, FR-017, FR-019 to FR-024). After T002 to T007. Brief B9.
- [ ] T009 [US1] Edit `rust-ci-runner/src/main.rs` and `lib.rs`: `ci` dispatch, usage text, a panic hook that prints `[runner] internal error` for `ci`, `mod ci;` and `mod ci_tests;` (FR-019). After T008. Brief B9.
- [ ] T010 [US1] [US2] [US3] [US4] Add `rust-ci-runner/src/ci_tests.rs` with every `ci_tests` row of the plan's test plan and invariants I1, I2, I4 to I7, I10 (FR-028). Stubs for `bws`, `docker`, `git`; `tiny_http` for Loki; clone from a local `file://` repo through the base-URL seam. Also `missing_access_token_exits_2_without_calling_bws`, `ci_leaves_the_process_environment_unchanged`, `engine_fault_during_run_exits_2_with_runner_fault`. Written with T008. Brief B9.
- [ ] T011 [US1] Edit `rust-ci-runner/.factory-ci.toml` (add `--skip ci_tests::` to the first command and a fourth command `cargo test --lib ci_tests::`, update the comments) and `rust-ci-runner/Cargo.toml` (version 0.2.0) (FR-026, FR-027). After T010. Brief B9.

## Phase 4: User Story 5 - image and release (Priority: P2), parallel with Phases 2 and 3

**Goal**: a published image with the runner, `docker`, `git`, CA certificates and `bws`.

**Independent Test**: the release run for `rust-ci-runner-v0.2.0` ends green and prints a digest.

- [ ] T012 [P] [US5] Rewrite `rust-ci-runner/Dockerfile`: Alpine base pinned by digest (looked up, not guessed), `bws` stage with SHA-256 and `unzip -Z1` checks, `apk add git docker-cli ca-certificates`, binary at `/rust-ci-runner`, non-root user (FR-025). Brief B6.
- [ ] T013 [P] [US5] Edit `.github/workflows/rust-ci-runner-release.yml` (image for `linux/amd64`, both binary assets kept, digest line kept) and `.github/scripts/factory-ci-lint-workflow.sh` (`release-platforms` rule and its self-test row) (FR-026). Brief B7.
- [ ] T014 [P] [US1] Edit `.github/factory-ci-repos.toml`: entries for `sudtanj/factory-monorepo` and `sudtanj/infrastructure`; header comment says which keys `ci` reads (FR-006). Brief B8.

## Gate: bootstrap (the owner or lead; not an engineer task)

- [ ] T015 [owner] Merge PR-A (T001 to T014). Start the old `factory-monorepo-ci` with `pipeline = rust-ci-runner` on `main`; it must pass. Push the tag `rust-ci-runner-v0.2.0` on a commit on `main`. Copy the digest from the release summary, then verify it with `docker pull` and `docker run --rm --entrypoint docker <image>@<digest> --version`. Give the digest to the engineer for T016 (spec Assumptions; plan Rollout steps 1 to 3).

## Phase 5: User Story 1 - the trimmed workflow (Priority: P1), after T015

**Goal**: `factory-monorepo-ci.yml` is a trigger.

**Independent Test**: `test-factory-monorepo-ci.sh` passes in the selftest; the owner's first runs (T018) pass.

- [ ] T016 [US1] Rewrite `.github/workflows/factory-monorepo-ci.yml`: inputs `repo`, `ref`, `project`; main guard; Environment `factory-ci`; `RUNNER_IMAGE` with the verified digest; steps: pinned checkout, one step with ghcr login over stdin, `mkdir`, one `docker run` as in the plan; job limit 35 minutes; concurrency per repo, ref and project; no summary, no status; header comment states the owner exception and the specs it supersedes (FR-001 to FR-005). Brief B10.
- [ ] T017 [US1] Add `.github/scripts/test-factory-monorepo-ci.sh` (checks in the plan's test plan, with a `--self-test` that mutates a temp copy) and call it from the `shell` job of `.github/workflows/factory-ci-selftest.yml` (FR-028, SC-001, SC-005). After T016. Brief B10.
- [ ] T018 [owner] First runs: `sudtanj/infrastructure` ref `main` project `rust-ci-runner`, then `sudtanj/factory-monorepo` ref `main` project `.`. Check: the log holds only grammar lines; Loki holds the step output; a wrong `FACTORY_CI_LOG_USER` fails at the probe; a step reads a file of the clone (mount path check) (SC-001, SC-004, SC-006; plan pre-mortem 1 and 2).

## Phase 6: Polish (last)

- [ ] T019 Update `README.md` (section "Run factory-monorepo CI by hand", secrets and variables, the owner exception) and `rust-ci-runner/README.md` (command list with `ci`, image contents, release and bootstrap order, minimal-output rule); state whether the GitHub repository description changes and give the text for the owner to apply (FR-029). Last task. Brief B11.

## Dependencies

- T001 done. T002 then T003 (same files). T004, T005, T006, T007 independent of each other and of T002. T012, T013, T014 independent of everything in Phases 2 and 3.
- T008 needs T002 to T007. T009 and T010 need T008. T011 needs T010.
- T015 needs T001 to T014 merged. T016 needs T015. T017 needs T016. T018 needs T016. T019 needs T017 and T018 (it documents what ran).
- User stories: US3 is T002, T003, T007 plus T008; US2 is T004 plus T008; US4 is T005, T006 plus T008; US1 is T006, T008 to T011, T014, T016 to T018; US5 is T012, T013, T015.

## Parallel execution

- Group 1 (start together): T002 then T003 in one stream; T004; T005; T006; T007; T012; T013; T014. `lib.rs` gets one line each from T005 and T006: merge by keeping both.
- Group 2: T008, T009, T010, T011, one engineer.
- Group 3: T016 then T017, after the gate.
- Last: T019.

## Implementation strategy

MVP is Phase 2 plus Phase 3 plus Phase 4, merged as PR-A, because the workflow cannot change before the image exists. PR-B (T016, T017, T019) follows the gate. Each phase can be reviewed alone. Stop after PR-A if the owner wants to review the runner before the workflow changes: the old workflow keeps working on the 0.1.0 digest.
