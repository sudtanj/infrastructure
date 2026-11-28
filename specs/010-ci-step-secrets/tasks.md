---

description: "Task list for step secrets in the ci command"
---

# Tasks: Step secrets for the ci command

**Input**: `specs/010-ci-step-secrets/` (spec.md, plan.md, research.md, data-model.md, contracts/ci-secrets.md, quickstart.md)

**Prerequisites**: plan.md, spec.md. The full brief for each task is in plan.md under "Engineer briefs" and "Test plan"; IDs match.

**Tests**: Required (constitution Principle V). They come first and must fail before the code exists.

**Rules for everyone**: no cargo, docker or builds on the agent VM (1 GB); tests run in the `rust-ci-runner` pipeline. Never write a secret value anywhere. `redact.rs` and `watch.rs` must not mention `bws` or `bitwarden` (`rust-ci-runner/tests/no_vendor.rs`). `git add` with explicit paths.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 run with its login, US2 allowlist, US3 missing value stops early, US4 nothing leaks

## Phase 1: Tests first (no story label, they serve all four)

- [ ] T001 [P] Write the named-lookup unit tests listed under T001 in plan.md (FR-001, FR-003, FR-005 to FR-007) in `rust-ci-runner/src/store_tests.rs`, calling `store::fetch_named` and `store::parse_named` and the `NamedError` texts in data-model.md
- [ ] T002 [P] Write the pipeline helper tests listed under T002 in plan.md (FR-001 to FR-004, FR-007, FR-009), including all four runner settings refused in a step, in the `tests` module of `rust-ci-runner/src/pipeline.rs`
- [ ] T003 [P] Write `each_runner_setting_in_secrets_refuses_the_file` for all four names (FR-004) in `rust-ci-runner/src/watch_tests.rs`
- [ ] T004 [P] Write the redactor and log tests listed under T004 in plan.md (FR-011, FR-012, FR-013) in the `tests` modules of `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`, and update `fault_accepts_only_the_fixed_list` to 16 entries
- [ ] T005 Write the `ci::run` integration tests listed under T005 in plan.md (FR-001 to FR-013, US1 to US4, SC-002, SC-003) at the end of `rust-ci-runner/src/ci_tests.rs`; change `declared_secret_fails_before_any_container` to expect `step secret not allowed` and add two rows to `every_stderr_line_is_in_the_fixed_list`; use unique canaries, never reuse a real value

## Phase 2: Foundation (blocks T009; each task is independent of the others)

- [ ] T006 [P] [US4] Add `Redactor::add` in `rust-ci-runner/src/redact.rs`; hold the redactor in an `RwLock` in `rust-ci-runner/src/log.rs`, add `Log::add_secrets`, and add `step secret not allowed` and `step secret unavailable` to `FAULTS` (FR-011, FR-012)
- [ ] T007 [P] [US3] Add `fetch_named`, `parse_named`, `NamedError` and the value rule in `rust-ci-runner/src/store.rs`, extracting the shared retry and server check from `fetch`; value rules cite data-model.md "Step secret values" (FR-005 to FR-007)
- [ ] T008 [P] [US2] Add `Pipeline::secret_names`, `Pipeline::unlisted` and `Pipeline::docker_secret_problems` in `rust-ci-runner/src/pipeline.rs`; cite data-model.md "Allowlist" and "Requested names" (FR-001 to FR-003, FR-007, FR-009)

## Phase 3: User Story 1, a pipeline runs with its secrets (P1) and the other P1 stories it carries

**Goal**: `ci` runs a fixture pipeline with its allowlisted secrets; refusals stop before any container; nothing leaks.

**Independent test**: the tests of T005 pass in the `rust-ci-runner` pipeline.

- [ ] T009 [US1] Wire `rust-ci-runner/src/ci.rs` (keep the watch entry, pass its `secrets`, apply the order in plan.md "Architecture decision", call `store::fetch_named`, call `log.add_secrets`, pass the map to `run_pipeline`) and make `load_pipeline` `pub(crate)` in `rust-ci-runner/src/job.rs` (FR-001 to FR-013)
- [ ] T010 [P] [US1] Change the `rust-ci-runner` package version to `0.3.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, by hand, two lines (FR-014)

**Checkpoint**: T001 to T010 are on `main` and the `rust-ci-runner` pipeline passes on that commit.

## Phase 4: Release and first use (P1, sequential, after the checkpoint)

- [ ] T011 [US1] Tag `rust-ci-runner-v0.3.0` on the `main` commit that passed; wait for `.github/workflows/rust-ci-runner-release.yml` and record the digest; verify it with `docker pull` (FR-014)
- [ ] T012 [US1] In a separate reviewed commit after T011, set `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest>`; run `.github/scripts/test-factory-monorepo-ci.sh` (FR-015)
- [ ] T013 [US2] Add the three secret names to the `sudtanj/factory-monorepo` entry and rewrite the header comment in `.github/factory-ci-repos.toml`; the owner creates the Bitwarden keys first (FR-016)
- [ ] T014 [US1] Start `factory-monorepo-ci` on `main` for `sudtanj/factory-monorepo` and check the result as in `specs/010-ci-step-secrets/quickstart.md` (SC-001)

## Final Phase: Docs (last task)

- [ ] T015 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit`) as listed under T015 in plan.md (FR-017)

## Dependencies

- T001 to T005 first, in parallel; T005 is one file.
- T006, T007, T008 in parallel after their tests; T009 after all three; T010 any time.
- T011 after T009, T010 and a passing `rust-ci-runner` pipeline on `main`; T012 after T011; T013 after T012; T014 after T012, T013 and the Bitwarden keys; T015 last.

## Implementation strategy

MVP is T001 to T014: the whole feature is one vertical slice, since every story needs the wiring in T009. Stop after the Phase 3 checkpoint if the owner wants the code reviewed before the release. Run `/speckit-analyze` before `/speckit-implement`.
