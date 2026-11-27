# Tasks: Runner-owned CI

**Input**: `specs/008-runner-owned-ci/` (spec.md, plan.md; the plan holds the contracts, test names and briefs B1 to B11)

**Tests**: required by Constitution Principle V and FR-028. Rust tests sit beside each task and run only through the `rust-ci-runner` pipeline (and type-check in the selftest). No one runs `cargo`, `docker` or a build on the engineer's machine (1 GB, no docker); the check there is `bash -n`, grep and review.

**Format**: `[ID] [P?] [Story] Description with paths`. `[P]` means no file or ordering conflict with the other `[P]` tasks of its phase. `[owner]` marks a human action.

**Landing**: both landings go to `main` by direct push (PRs return 403): Landing A is T001 to T016, Landing B is T019, T020, T022.

## Phase 1: Setup

- [x] T001 Amend `.specify/memory/constitution.md` to 1.1.0 (Principle II: the manual workflow holds only `BWS_ACCESS_TOKEN`) and set `.specify/feature.json` to this folder. Done in the commit that adds this folder. Brief: none.

## Phase 2: Shared runner changes (blocks Phase 3; parallel streams)

**Goal**: the parts of the `ci` command that also fix `job`, `plan` and `run`.

- [X] T002 [P] [US3] Edit `rust-ci-runner/src/ship.rs` and `ship_tests.rs`: `LogConfig::from_env` strips a trailing `/`, then `/loki/api/v1/push`, then a trailing `/`; tests `url_loses_the_push_path_and_slashes`, `url_normalisation_is_idempotent`, `url_with_another_path_is_kept` (FR-015, I3). Brief B1. **Test-first: tests written in `ship_tests.rs` (`url_loses_the_push_path_and_slashes`, `url_normalisation_is_idempotent`, `url_with_another_path_is_kept`), marked `#[ignore = "test-first"]`; remove each marker as it passes.**
- [X] T003 [US3] Edit `rust-ci-runner/src/ship.rs` and `ship_tests.rs`: extract `render(labels, meta, batch)` from `Shipper::body`; `pub fn probe(cfg, run_id, limit: Duration) -> Result<u16, u16>` builds its body through `render` (five labels with `service_name`), one retry on 000, 429 and 5xx; tests `probe_204_is_ok`, `probe_401_reports_the_code`, `probe_with_no_answer_reports_000_in_time`, `probe_retries_once_on_5xx_and_429`, `probe_posts_one_line_with_the_five_labels_and_basic_auth` (FR-016). After T002 (same files). Brief B1. **Test-first: tests written in `ship_tests.rs` (the five `probe_*` tests), marked `#[ignore = "test-first"]`.**
- [X] T004 [P] [US2] Edit `rust-ci-runner/src/log.rs` and `pipeline.rs`: `Log::public_events(bool)`, `public_form`, `FAULTS` and `Log::fault`, `Log::emit` as the single choke point (every printed line starts with `[`); a step named `runner` is refused; tests `public_mode_prints_only_the_closed_grammar`, `public_mode_drops_the_reason_after_failed_and_skipped`, `public_mode_ships_the_full_text`, `public_mode_refuses_a_bad_name`, `fault_accepts_only_the_fixed_list`, `step_named_runner_is_refused` (FR-018, FR-030, I1). Brief B2. **Test-first: tests written: `log.rs` (`public_mode_*`, `fault_accepts_only_the_fixed_list`, `every_public_line_starts_with_a_bracket_for_hostile_input`) and `pipeline.rs` (`step_named_runner_is_refused`), marked `#[ignore = "test-first"]`; they need `Log::with_err` (see plan "Test-first interface notes").**
- [X] T005 [P] [US4] Edit `rust-ci-runner/src/engine.rs`: `command()` uses `env_clear`, the allowlist, `env_remove("BWS_ACCESS_TOKEN")`; `run_step` keeps its three-capability drop, `no-new-privileges` and `--pids-limit 512` and adds `--memory 5g --memory-swap 5g` (no `--cap-add`, no `--cpus`); create `rust-ci-runner/src/store_tests.rs` with `command_removes_the_inherited_token`, `explicit_env_entry_still_passes`, `engine_child_env_is_the_allowlist`, and `mod store_tests;` in `lib.rs` (FR-012, FR-024, I9, I10, I11). Brief B3. **Test-first: tests written in `store_tests.rs` (`command_removes_the_inherited_token`, `explicit_env_entry_still_passes`, `engine_child_env_is_the_allowlist`); the file is not wired into `lib.rs` yet, add `mod store_tests;` here; `Engine::command` must be `pub(crate)`.**
- [X] T006 [US1] Add `rust-ci-runner/src/store.rs` and append to `store_tests.rs`, adding `mod store;` to `lib.rs`: `fetch(bws_bin, token, server_url, limit)` with its own `Command` (cleared env, `PATH`, `HOME`, `BWS_ACCESS_TOKEN`, `BWS_SERVER_URL`), `parse_list`, `StoreError`, `Secrets`; the ten store tests of the plan (FR-009, FR-010, FR-011, I8). After T005 (same file, same stream). Brief B3. **Test-first: tests written in `store_tests.rs` (the ten plan tests plus `matches_other_than_one_never_return_a_value` and `secrets_debug_prints_no_value`); add `pub mod store;` to `lib.rs`; remove the `test-first` markers as they pass.**
- [X] T007 [P] [US3] Edit `rust-ci-runner/src/watch.rs`, `watch_tests.rs`, `job.rs`, `job_tests.rs`, `main.rs` (`load_list` only): `watch::read`, `pub(crate)` helpers, `project_dir`, and `local` fails when a passing run dropped lines; tests `read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref`, `run_quiet_logs_fails_when_a_passing_run_dropped_lines` (FR-014, FR-017). Brief B4. **Test-first: tests written: `watch_tests.rs` (`read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref`) and `job_tests.rs` (`run_quiet_logs_fails_when_a_passing_run_dropped_lines`, needs the `job::local_with` seam of the plan notes).**

## Phase 3: User Story 1, 2, 3, 4 - the `ci` command (Priority: P1)

**Goal**: `rust-ci-runner ci` runs the whole job and prints only the closed grammar.

**Independent Test**: `ci_tests` pass in the `rust-ci-runner` pipeline: exit 0 or 1, grammar, containment, no secret in output, argv or engine child environment.

- [X] T008 [US1] [US2] [US3] [US4] Add `rust-ci-runner/src/ci.rs`: strict `parse_args` (canonical `--work`, no repeated flags, no `--`-prefixed values), watch gate with required `id`, `store::fetch`, `LogConfig` from the fetched map, probe, shipper, engine check, identity check, `git init`/`fetch refs/heads/<ref>`/`checkout` with the env header and the protocol seam, `project_dir`, `prepare` with no secrets and the docker gate, `run_pipeline` in quiet public mode, exit 0 or 1, fault lines through `Log::fault`, explicit stdio and cleared env on every child, the run-wide step env with `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=safe.directory`, `GIT_CONFIG_VALUE_0=/workspace`, no `DOCKER_CONFIG` anywhere in step argv or the runner container, the full redactor (FR-006 to FR-008, FR-013, FR-016, FR-017, FR-019 to FR-024, FR-030). After T002 to T007. Brief B5.
- [X] T009 [US1] Edit `rust-ci-runner/src/main.rs` and `lib.rs`: `ci` dispatch, usage text, read `BWS_ACCESS_TOKEN` once and `remove_var` before any thread, a panic hook that prints `[runner] internal error` for `ci`, `mod ci;` and `mod ci_tests;` (FR-009, FR-019). After T008. Brief B5. **Test-first: wire `#[cfg(test)] mod ci_tests;` (file exists, see T010).**
- [X] T010 [US1] [US2] [US3] [US4] Add `rust-ci-runner/src/ci_tests.rs` with every `ci_tests` row of the plan's test plan and invariants I1, I2, I4 to I7, I10, I11 (FR-028). Stubs for `bws`, `docker`, `git`; `tiny_http` for Loki and the API; one real `file://` fetch test through the protocol seam; hostile-children test. Written with T008. Brief B5. **Test-first: tests written in `ci_tests.rs` (43 tests, every plan row plus extras listed in the plan notes), all `#[ignore = "test-first"]`; not wired into `lib.rs`; the interface they call is in the plan notes.**
- [X] T011 [US1] Edit `rust-ci-runner/.factory-ci.toml` (`--locked`, `--skip ci_tests::` on the first command, a fourth command `cargo test --locked --lib ci_tests::`, comments) (FR-026, FR-027). After T010; lands together with the lock of T016, never without it. The 0.2.0 version bump is in T014. Brief B5.

## Phase 4: User Story 5 - image, release, selftest, lockfile (Priority: P2), parallel with Phases 2 and 3

**Goal**: a published multi-arch image with the runner, `docker`, `git`, CA certificates and `bws`, and checks that run before the direct push.

**Independent Test**: the release run for `rust-ci-runner-v0.2.0` ends green, the smoke step passes and the summary prints both pins.

- [X] T012 [P] [US5] Rewrite `rust-ci-runner/Dockerfile`: `alpine:3.23` by index digest (looked up with `curl`, not typed), one `bws` stage per architecture with `ADD --checksum`, throwaway `unzip` stage, `unzip -p`, `FROM bws-${TARGETARCH}`, `apk add git docker-cli ca-certificates`, `LABEL org.opencontainers.image.source`, binary at `/rust-ci-runner`, non-root user (FR-025). Brief B6.
- [X] T013 [P] [US5] Edit `.github/workflows/rust-ci-runner-release.yml`: QEMU by SHA, `provenance: mode=max`, smoke step (amd64 and arm64, `docker create` plus `docker cp`, and a `ci --repo x/y` row that expects `[runner] repository or ref not in the watch list` and exit 1), `gh release create` after the smoke and idempotent, summary naming `RUNNER_IMAGE_DIGEST` and `RUNNER_IMAGE` (FR-026). The lint script stays unchanged. Brief B7.
- [X] T014 [P] [US5] Edit `.github/workflows/factory-ci-selftest.yml` and `rust-ci-runner/Cargo.toml`: version `0.2.0` first (so the lock carries it); `push` trigger for branches other than `main`; `list` job runs the temporary `cargo generate-lockfile` step with artifact upload, then `cargo test --lib --no-run`, `cargo clippy --locked -- -D warnings`, then `cargo run -q -- plan --check`; `pipeline` job adds a no-push `docker buildx build --platform linux/amd64,linux/arm64` with QEMU and buildx pinned by SHA (the arm64 binary is a stand-in copy); a step in the `workflow` job fails if the file contains `environment:`, any `secrets.` other than `GITHUB_TOKEN`, `id-token` or `pull_request_target`; header comment records the exception (FR-026, FR-027, FR-028). Brief B7.
- [X] T015 [P] [US1] Edit `.github/factory-ci-repos.toml`: entries for `sudtanj/infrastructure` and `sudtanj/factory-monorepo` with numeric `id` (looked up with `gh api`) and `branches = ["main"]`; header lines: which keys `ci` reads, and that listing a repo also makes the by-hand tick plan it (FR-006). Brief B8.
- [X] T016 [US5] After T014 ran on a branch push: download the generated `Cargo.lock` artifact, commit `rust-ci-runner/Cargo.lock`, add `--locked` to every `cargo build` and `cargo test` in the release workflow and the selftest, remove the generator and upload steps (FR-027). Brief B9.

## Gate: bootstrap and owner settings (owner or lead; not engineer tasks)

- [ ] T017 [owner] GitHub settings, before the first workflow run (not code): Environment `factory-ci` with a deployment-branch rule for `main` only and the owner as required reviewer; `BWS_ACCESS_TOKEN` only in that Environment; `FACTORY_CI_TOKEN` a read-only fine-grained PAT; the Loki token `logs:write` only; the Bitwarden machine account with read access to one project; a tag ruleset for `rust-ci-runner-v*`. After landing B: a ruleset on `main` (pull request required, code-owner review for `.github/**` and `rust-ci-runner/**`, no force-push, no deletion). Optional: make the GHCR package public so the login step can go.
- [ ] T018 [owner] Land A (T001 to T016) on `main`. Start the old `factory-monorepo-ci` with `pipeline = rust-ci-runner` on `main`; it must pass. Compare the committed `rust-ci-runner/Cargo.lock` with a local `cargo generate-lockfile` and stop on a difference. Push the tag `rust-ci-runner-v0.2.0` on a commit on `main`. Copy the digest from the release summary, verify it with `docker pull` and `docker run --rm --entrypoint docker <image>@<digest> --version`. Give the digest to the engineer for T019 (plan Rollout steps 1 to 3).

## Phase 5: User Story 1 - the trimmed workflow (Priority: P1), after T018

**Goal**: `factory-monorepo-ci.yml` is a trigger.

**Independent Test**: `test-factory-monorepo-ci.sh` passes in the selftest; the owner's first runs (T021) pass.

- [ ] T019 [US1] Rewrite `.github/workflows/factory-monorepo-ci.yml`: inputs `repo`, `ref`, `project`; main guard; Environment `factory-ci`; `RUNNER_IMAGE` with the verified digest; pinned checkout, one step with temporary `DOCKER_CONFIG`, quiet login over stdin, `mkdir`, one quoted `docker run -q` as in the plan; job limit 45 minutes; concurrency per repo, ref and project; no summary, no status; no `--allow-docker`; header comment states the owner exception, the specs it supersedes and never a self-hosted runner (FR-001 to FR-005). Brief B10.
- [ ] T020 [US1] Add `.github/scripts/test-factory-monorepo-ci.sh` (plain grep checks of the plan's test plan, including no `-e DOCKER_CONFIG` and no `DOCKER_CONFIG` mount on the `docker run` lines, no self-mutation) and call it from the `shell` job of `.github/workflows/factory-ci-selftest.yml` (FR-028, SC-001, SC-005). After T019. Brief B10. **Test-first: the script exists as `.github/scripts/test-factory-monorepo-ci.sh` (72 checks, fails against the old workflow, passes against a plan-shaped sample); only the call from the `shell` job of `factory-ci-selftest.yml` is left.**
- [ ] T021 [owner] First runs, after T017: `sudtanj/infrastructure` ref `main` project `rust-ci-runner`, then `sudtanj/factory-monorepo` ref `main` project `.`. Check: the log holds only grammar lines; Loki holds the step output with `service_name="factory-ci"`; a wrong `FACTORY_CI_LOG_USER` fails at the probe; a step reads and writes a file of the checkout and runs `git status` (mount path, ownership and `safe.directory` check) (SC-001, SC-004, SC-006; plan pre-mortem 1 and 2).

## Phase 6: Polish (last)

- [ ] T022 Update `README.md` (section "Run factory-monorepo CI by hand", secrets and variables, the owner exception, the selftest exception, oversize-line behaviour, owner settings of T017) and `rust-ci-runner/README.md` (command list with `ci`, image contents, release and bootstrap order, minimal-output rule); mark `specs/005-plain-actions-ci/spec.md`, `specs/006-runner-unit-tests/spec.md` and `specs/007-runner-image/spec.md` as superseded in part by 008; state whether the GitHub repository description changes and give the text for the owner to apply (FR-029, FR-031). Last task. Brief B11.

## Review fixes (Landing A)

Found by the review and security pass of Landing A. Each is one commit naming its ID.

- [X] R01 `log.rs` `emit`, `ci.rs`: in public mode count only the bytes printed, and never cut the fixed event and fault lines, so a step that writes more than 10 MB cannot hide `pipeline passed`. Test: a step writes over 10 MB and `[runner] pipeline passed` is still printed (FR-018, US1).
- [X] R02 `engine.rs` `run_capped`: put the child in its own process group and kill the group on timeout, so a grandchild holding the pipe cannot block the join. Test: a stub backgrounds `sleep` holding stdout.
- [X] R03 Run `cargo fmt --check` in the selftest and fix the drift it reports (`main.rs` `=>ExitCode` and any other).
- [X] R04 `Makefile`: `--locked` on the clippy call of the `lint` target.
- [X] R05 Release workflow: push only `:<version>` before the smoke step, tag `:latest` after the smoke step passes.
- [X] R06 `ci.rs`: in production read neither `FACTORY_CI_BWS` nor `FACTORY_CI_ENGINE`; use `/usr/local/bin/bws` and `docker`. Tests inject through `Seams`.
- [X] R07 `pipeline.rs`: `ci` joins the reserved step names, with a test.
- [X] R08 `ci.rs`: remove `ci-<run>-<rand>/` under `--work` after `cleanup_run`, as root through the container like `job.rs` `remove_ws`. Test: the directory is gone after the run.
- [X] R09 `factory-ci-selftest.yml`: the guard also fails on `secrets:` (with `inherit`), `secrets[`, `toJSON(secrets`, `workflow_run`, `repository_dispatch`; `persist-credentials: false` on every checkout.
- [X] R10 Document in `rust-ci-runner/README.md` and `plan.md` that `remove_var` leaves the process environment file under proc readable by the same uid (the runner container stays uid-isolated) and that the Loki host is not allowlisted (Bitwarden write access is a trust root).

## Dependencies

- T001 done. T002 then T003 (same files). T005 then T006 (same file). T004, T007 independent of each other and of T002. T012 to T015 independent of Phases 2 and 3.
- T016 needs T014 pushed on a branch and its selftest run finished. T008 needs T002 to T007. T009 and T010 need T008. T011 needs T010 and lands together with T016.
- T018 needs T001 to T016 landed. T019 needs T018. T020 needs T019. T021 needs T017 and T019. T022 needs T020 and T021 (it documents what ran).
- User stories: US3 is T002, T003, T007 plus T008; US2 is T004 plus T008; US4 is T005, T006 plus T008; US1 is T006, T008 to T011, T015, T019 to T021; US5 is T012, T013, T014, T016, T018.
- FR coverage: FR-001 to FR-005 T019, T020; FR-006 T008, T015; FR-007 T008; FR-008 T008; FR-009 T006, T009; FR-010, FR-011 T006, T008; FR-012 T005; FR-013 T008; FR-014 T007, T008; FR-015 T002; FR-016 T003, T008; FR-017 T007, T008; FR-018 T004; FR-019 T004, T008, T009; FR-020 T008; FR-021 to FR-023 T008; FR-024 T005, T008; FR-025 T012; FR-026 T013, T014; FR-027 T011, T014, T016; FR-028 T010, T020; FR-029 T022; FR-030 T004, T005, T008; FR-031 T022.

## Parallel execution

- Group 1 (start together): T002 then T003; T004; T005 then T006; T007; T012; T013 with T014; T015. T016 after T014 is pushed and its run is done.
- Group 2: T008, T009, T010, T011, one engineer, after Group 1; T011 lands with T016.
- Group 3: T019 then T020, after the gate.
- Last: T022.

## Implementation strategy

MVP is Phase 2 plus Phase 3 plus Phase 4, landed as A, because the workflow cannot change before the image exists. B (T019, T020, T022) follows the gate. Stop after A if the owner wants to review the runner before the workflow changes: the old workflow keeps working on the 0.1.0 digest.
