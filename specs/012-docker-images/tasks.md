---

description: "Task list for public multi-arch Docker images from docker/"
---

# Tasks: Public Docker images from `docker/`

**Input**: `specs/012-docker-images/` (spec.md, plan.md, research.md, data-model.md, contracts/publish-step.md, contracts/image-project.md, quickstart.md)

**Prerequisites**: plan.md and spec.md. The brief of each step S1 to S10 is in plan.md under "Ordered steps and engineer briefs"; each task below names the step it carries out. `data-model.md` is the canonical source for check ids, constants, tags and the manifest list; tasks cite it and do not restate it.

**Tests**: required (constitution Principle V). They are written before the code they cover. All tests run in the runner pipeline or the selftest, never only on a laptop.

**Rules for everyone**:
- No cargo, docker, builds or tests on the 1 GB agent VM. Git and file reads only. Tests run in the `rust-ci-runner` pipeline of a pull request.
- Never write a secret value anywhere: files, logs, commits, messages. Fixtures use synthetic strings and the canary `canary-Zq81-value-key-0123456789`.
- No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or creates or reads Bitwarden keys. Every Bitwarden and Docker Hub action is the owner's (T024).
- No file under `.github/workflows/` and no file under `.github/scripts/` changes (FR-022). No new crate.
- Nothing that holds the token derives `Debug`. No `#[allow(clippy::...)]` unless the surrounding code already does.
- `git add` with explicit paths. Never force-push.
- Spec Kit applies: a brief that does not name `specs/012-docker-images/` and its task IDs is refused.
- Order against 011: whichever of 011 and 012 merges second rebases (`walk.rs`, `plan.rs`, `job.rs`, `watch.rs` overlap). 012 needs the tick and the `job.sh` names-file contract as on `main`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 a new image folder becomes a public multi-arch image, US2 an unsafe image cannot be published, US3 only `main` publishes and no secret leaks, US4 tags stay correct and removal is safe

## Phase 1: Setup

- [ ] T001 [P] Resolve and record every pin in `specs/012-docker-images/pins.md`: index digest of `golang:1.23`, index digest of `gcr.io/distroless/static`, digest of `docker:27.5.1-cli`, digest of `gcr.io/go-containerregistry/crane:debug`. For each: the source tag, the digest, and the platforms `docker buildx imagetools inspect` lists (golang, distroless and docker:cli must list `linux/amd64` and `linux/arm64`, FR-028). Run the three spike checks of research.md "Spike left to the engineer" with a throwaway Docker Hub repo the owner names, and record the result; a refusal of push by digest goes back to the lead (S3, S4, S10)
- [ ] T002 [P] Bump `rust-ci-runner/Cargo.toml` and the `rust-ci-runner` entry of `rust-ci-runner/Cargo.lock` from `0.3.0` to `0.4.0` (S7; `tests/repo_files.rs` `cargo_toml_and_lock_versions_match` is the proof)
- [ ] T003 [P] Create the fixture set `rust-ci-runner/tests/fixtures/image/`: one passing project (shape of contracts/image-project.md) and one failing copy per check id of data-model.md "Check rules" (`layout`, `directive`, `from-unpinned`, `from-arg`, `emulation`, `final-base`, `final-run`, `final-shell`, `final-user`, `secret-name`, `secret-value`, `secret-flag`, `pipeline-secrets`), each the passing file with one change; plus CRLF, BOM, heredoc, comment-`FROM` and stage-chain variants (S1). Token-shaped strings are built synthetically

## Phase 2: Foundational (blocks every story)

- [ ] T004 Add `rust-ci-runner/src/image.rs` with the constants of data-model.md "Constants" (`NAMESPACE`, `PUBLISH_BRANCH`, `TOKEN_NAME`, `PLATFORMS`, `ARCHIVE_MAX`, `CRANE_IMAGE` from T001, `NOT_PUBLISHED`, `REGISTRY`) and `image_project`, `publishes`, `name_ok` (data-model.md Image project.name, Constants.PLATFORMS, Constants.ARCHIVE_MAX, Constants.PUBLISH_BRANCH); register `pub mod image;` and the test module in `rust-ci-runner/src/lib.rs`. Merge before T007 to T010 compile (S1, S2)
- [ ] T005 [P] Write `rust-ci-runner/src/image_tests.rs`: one test per check id and per edge answer of data-model.md "Check rules" (Image project.name boundaries 1, 2, 64, 65; CRLF, BOM, heredoc, comment-`FROM`, `FROM alpine AS alpine`, stage chains, `--platform=$BUILDPLATFORM`, `emulation` with and without the flag, `USER $X`, `ARG TARGETARCH` passes `secret-name`), `check_never_panics_on_junk` (1 MB, NUL, invalid UTF-8), output sorted and unique, the passing fixture returns empty, and a test that reads the real `docker/hello-static` once T014 lands. Mark tests that need code `#[ignore = "test-first"]` (S1; I2)
- [ ] T006 Implement `image::check` and the Dockerfile parser in `rust-ci-runner/src/image.rs` (data-model.md "Check rules": ids, instruction parser, continuations, heredocs, directives, stage chains, hand-written token-shape scans, no regex crate, 256 KB read cap, `symlink_metadata`, `&'static str` ids only). Remove the `test-first` markers of T005 (S1; FR-008, FR-009, FR-010, FR-011, FR-027; required files per data-model.md Image project.required files)
- [ ] T007 [P] Discovery: `rust-ci-runner/src/projects.rs` `find`, `discover` (truncated tree reads one more level under `docker`), `project_path_ok`, with tests in `rust-ci-runner/src/projects_tests.rs`: `find_docker_projects`, `discover_truncated_tree_reads_docker`, `owns_prefix_boundary`, a folder with `Dockerfile` only is a project, a folder with neither is not, `docker/x/y/.factory-ci.toml` and a `docker` file are ignored, `valid` follows data-model.md Image project.name (S2, D8; FR-001, FR-002, FR-004)
- [ ] T008 [P] Walk marker: `rust-ci-runner/src/walk.rs` `decide(..., publishes: bool)` and the `NOT_PUBLISHED` rule at the tip (`at_tip`) and on an ancestor (`walk_one`) per data-model.md "Walk rule", with `marker_at_tip_runs_when_publishes`, `marker_on_ancestor_runs_when_publishes`, `marker_ignored_when_not_publishes` and an equality test against the old behaviour for `publishes = false` in `rust-ci-runner/src/walk_tests.rs` (S2; I10)
- [ ] T009 Plan rows: in `rust-ci-runner/src/plan.rs` pass `publishes` to `decide` (true when the row's branch is `image::PUBLISH_BRANCH`) and set `secrets_ok` from `image::publishes(project, branch)`; update `rust-ci-runner/src/plan_tests.rs` and `rust-ci-runner/src/secret_names_tests.rs` ("all rows false" becomes "false except `docker/*` rows on `main`"), add `secrets_ok_true_only_for_main_image_rows` (S2; I7). Depends on T004, T008
- [ ] T010 Job target: `rust-ci-runner/src/job.rs` `target()` accepts `docker/<name>` through `projects::project_path_ok` and still rejects `a/b`, `docker/Hello`, `../x`; test in `rust-ci-runner/src/job_tests.rs`; `rust-ci-runner/src/ci.rs` stays unchanged and a test shows `ci` still refuses `docker/x` (S2). Depends on T007

**Checkpoint**: an image project is discovered, validated and checked; nothing publishes yet.

## Phase 3: User Story 1 - A new image folder becomes a public multi-arch image (Priority: P1)

**Goal**: merging a valid `docker/hello-static/` publishes `sudtanj/hello-static:latest` and `:sha-<short>`, each a manifest list with a native `linux/amd64` and `linux/arm64` image.

**Independent test**: quickstart 5 steps 1 to 3 and the integration tests `main_image_publishes_both_tags`, `list_has_both_platforms`.

- [ ] T011 [P] [US1] Write `rust-ci-runner/src/publish_tests.rs` per plan.md S4 "Acceptance": `verify_index` cases of data-model.md "Tags and the manifest list" (Manifest list.mediaType, Manifest list.manifests, Manifest list.digest: both platforms in both media types and either order pass; one entry, three, two `amd64`, no `arm64`, `unknown/unknown`, single image manifest, malformed digest, missing `platform`, not JSON fail), plus every flow of S4 with a stub engine script and a fake `Api` (no token, archive cases, first and second tip read, exit 41 / 44 / 42 / timeout mapping including the `exit status: <n>` text, `INDEX` line missing or bad, repeat publish, argv and env holding no token value, container flags exactly as contracts/publish-step.md). Mark `#[ignore = "test-first"]` (S4; I1, I4, I5, I11, I12, I13)
- [ ] T012 [US1] Implement `rust-ci-runner/src/publish.rs` (`publish`, `verify_index`) and register it in `rust-ci-runner/src/lib.rs` per contracts/publish-step.md: preconditions in order, tip read 1, container A through `Engine::capture`, `INDEX` parse and `verify_index`, tip read 2, container B through `Engine::run_logged`, exit mapping, `--user` from `MetadataExt`. Remove the `test-first` markers of T011 (S4; FR-014, FR-016, FR-017, FR-018, FR-020, FR-021, FR-026; size limit per data-model.md Constants.ARCHIVE_MAX, platforms per Constants.PLATFORMS). Depends on T004, T011
- [ ] T013 [US1] Wire the job in `rust-ci-runner/src/job.rs` and `rust-ci-runner/src/job_tests.rs` (S5): `secret_names` writes `DOCKERHUB_TOKEN\n` only for `publishes` plus a clean `image::check`, empty otherwise, `NO_JOB_SECRETS` still first; `attempt` runs the check before `prepare()` (failure `check failed: <ids>`, no container), runs the pipeline, then `publish` before `cleanup_run`, and posts `NOT_PUBLISHED` on a branch; `token_values` gains the token and the base64 of `sudtanj:<token>`; non-image projects behave as before (existing job tests unchanged); a failing build step ends `failure` with fixed lines only (FR-006, FR-007). Depends on T006, T010, T012
- [ ] T014 [P] [US1] Add `docker/hello-static/{Dockerfile,hello.go,go.mod,README.md,.factory-ci.toml}` and `docker/README.md` per contracts/image-project.md with the pins of T001 (index digests, comment with the source tags); the pipeline builds and saves both platforms; `hello.go` prints one fixed line (S3; FR-003, FR-012). Depends on T001
- [ ] T015 [US1] Add the integration test file `rust-ci-runner/tests/job_publish.rs` and extend `rust-ci-runner/tests/common/mod.rs` (`Fake` answers `branches`; a `STUB_` hook makes container A print an `INDEX` line; fixtures carry both archives): `main_image_publishes_both_tags`, `list_has_both_platforms`, `failed_build_pushes_nothing` (one archive absent, or a failing step), `no_tag_other_than_sha_and_latest_is_ever_named` over every argv and env (S6; FR-014, FR-017, FR-026). Add `--test job_publish` to the integration command in `rust-ci-runner/.factory-ci.toml`. Depends on T013
- [ ] T016 [US1] Turn docker on for the repo: in `.github/factory-ci-repos.toml` add `allow_docker = true` to the `sudtanj/infrastructure` entry and remove its `branches` line, keeping `repo` and `id` (S3, FR-023; research D3). Safe to merge with the 0.3.0 runner still pinned (plan.md "Rollout" step 2). Depends on T014

## Phase 4: User Story 2 - An unsafe image cannot be published (Priority: P1)

**Goal**: every rule of data-model.md "Check rules" fails the project with a named id and zero pushes; a pipeline that omits a check still cannot publish.

**Independent test**: one fixture per id (T003) through `image::check` (T005) and through the job (T017).

- [ ] T017 [US2] In `rust-ci-runner/tests/job_publish.rs`: `failed_check_pushes_nothing` over a sample of ids (no container starts, status `check failed: <id>`), `omitted_check_step_still_refused_in_publish` (a build step rewrites the Dockerfile after the first check; the second check in `publish` refuses), `pipeline_with_secrets_fails_check` (`pipeline-secrets`, `secret-flag`), `emulation_refused` (S6; FR-008, FR-011, FR-027, SC-002). Depends on T015
- [ ] T018 [P] [US2] Extend `rust-ci-runner/tests/repo_files.rs`: the infrastructure entry has `allow_docker` true, no `branches`, no secrets; every folder under `docker/` has the three files and a name that passes `image::name_ok`; `docker/hello-static` passes `image::check`; the FR-022 test runs `git diff --name-only <base>...HEAD` with `<base>` from `FACTORY_CI_BASE` or `origin/main`, fails on any path under `.github/workflows/` or `.github/scripts/`, and prints a skip line, not a pass, when git or the base is missing. Keep the file out of the pipeline's integration command (S8). Depends on T014, T016

## Phase 5: User Story 3 - Only `main` publishes, and no secret leaks (Priority: P1)

**Goal**: a non-`main` run reads no token and pushes nothing; a `main` run leaks the token nowhere.

**Independent test**: `branch_run_reads_no_token_and_pushes_nothing` and `canary_token_absent_from_public_lines_argv_envs_status`.

- [ ] T019 [US3] In `rust-ci-runner/tests/job_publish.rs`: `branch_run_reads_no_token_and_pushes_nothing` (token env absent, zero crane calls, zero API tip reads, status `built and checked, not published`), `canary_token_absent_from_public_lines_argv_envs_status` over stdout, stderr, argv, every engine env except the two publish calls, status text and Loki lines, `user_step_env_never_holds_token` (stub envs file; no build step ever holds the token, so it cannot reach an image layer) (S6; FR-013, FR-015, FR-024, SC-003). Depends on T015
- [ ] T020 [P] [US3] Update the expectations that the names file is always empty in `rust-ci-runner/tests/job_prepare.rs`, `rust-ci-runner/tests/job_rules.rs` and `rust-ci-runner/tests/no_vendor.rs`: still empty for non-image projects, branches, a failing check and a pipeline that lists a secret (`NO_JOB_SECRETS` first); `DOCKERHUB_TOKEN` only for a `main` image project with a clean check; add `dockerhub_token_is_a_valid_secret_name` against `valid_secret_name` in `rust-ci-runner/src/pipeline.rs` tests (S6; I7, I8). Depends on T013

## Phase 6: User Story 4 - Tags stay correct and removal is safe (Priority: P2)

**Goal**: an older commit never moves `latest`; a deleted or untouched folder changes nothing.

**Independent test**: `older_commit_pushes_sha_only` and `deleted_folder_makes_no_call`.

- [ ] T021 [US4] In `rust-ci-runner/tests/job_publish.rs`: `older_commit_pushes_sha_only` (fake `branches` answers another sha on the first read, then on the second), `unreadable_tip_pushes_nothing`, `second_tip_read_fails_leaves_sha_tag`, `repeat_publish_reuses_list`; in `rust-ci-runner/src/plan_tests.rs`: `deleted_folder_makes_no_call` and `unchanged_commit_builds_nothing` for a `docker/<image>` project (S6; FR-005, FR-018, FR-019, FR-020). Depends on T015, T009

## Phase 7: Polish, release and proof

- [ ] T022 Amend the constitution layout bullet to list `docker/`, version `1.1.1`, through the `speckit-constitution` skill, in `.specify/memory/constitution.md` (S7; Governance)
- [ ] T023 Open the pull request for `012-docker-images`, link `specs/012-docker-images/`, and wait for the `factory-ci/rust-ci-runner` status to be green. Run quickstart 3 (the path check) and record `ok`. Merge only when green (FR-022, SC-004). Depends on T002 and T015 to T022
- [ ] T024 [Owner] Quickstart 1: create the Docker Hub token (read and write, account `sudtanj`, repositories public by default); add the Bitwarden key `sudtanj/infrastructure/DOCKERHUB_TOKEN` to the project `FACTORY_CI_BWS_PROJECT_DOCKER` names; confirm `FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY_DOCKER` exist in `factory-ci-docker` and that the token can write statuses on this repo; confirm no required reviewers block the Environment; accept plan.md blockers B1, O1 and O2. Record the answers here without any value
- [ ] T025 Release (quickstart 4): record `gh variable get RUNNER_IMAGE_DIGEST`; after the merge, tag `rust-ci-runner-v0.4.0`; record the digest the release workflow prints; `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`; run the canary tick (`factory-ci/rust-ci-runner` and `factory-ci/ci-canary` still post). Record both digests. Depends on T023, T024
- [ ] T026 End to end (quickstart 5 steps 1 to 8, SC-001, SC-003, SC-006): hello-static pulls without login, both tags show exactly `linux/amd64` and `linux/arm64` with equal digests, a pull and run on an amd64 host and on an arm64 host, a token search of the public log and status, the branch case, the fast-forward case, the broken-Dockerfile and `emulation` cases. Record evidence here. Depends on T025
- [ ] T027 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit sudtanj/infrastructure --description ...`) to match this feature (FR-025): what `docker/` is and how to add an image, both architectures, the publish step and its check ids, `DOCKERHUB_TOKEN` and its Bitwarden key, the rollback, and the amended sentence that the tick hands no secret to a step (true for steps, relaxed for the built-in publish); prose per `stop-slop`; confirm with `gh repo view --json description`. This is the last task. Depends on T026

## Dependencies and order

- Phase 1 first; T001 feeds T004 (`CRANE_IMAGE`) and T014.
- Phase 2: T004 before T007 to T010; T005 and T006 in sequence; T007, T008 in parallel with T005; T009 after T008; T010 after T007.
- Phase 3: T011 before T012; T012 before T013; T013 before T015; T014 parallel to T011 to T013; T016 after T014.
- Phases 4 to 6 run after T015 and are parallel to each other (shared file `tests/job_publish.rs`: append in separate commits).
- Phase 7: T022 any time; T023 after all code and test tasks; T024 owner, any time before T025; T025 to T027 in order.

## Parallel examples

- After T004: T005, T007, T008 together.
- After T013: T014 done earlier, then T015, T018, T020 together on different files.
- T011 and T014 together.

## Implementation strategy

MVP is Phase 1 to Phase 3: discovery, check, publish, wiring and `hello-static`. Stories 2 to 4 add proof that the guards hold. Nothing reaches Docker Hub before T025 moves `RUNNER_IMAGE_DIGEST`; until then the 0.3.0 runner ignores `docker/` (plan.md "Rollout"). Rollback is `gh variable set RUNNER_IMAGE_DIGEST` back to the value recorded in T025.
