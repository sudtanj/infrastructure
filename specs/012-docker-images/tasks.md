---

description: "Task list for public multi-arch Docker images from docker/"
---

# Tasks: Public Docker images from `docker/`

**Input**: `specs/012-docker-images/` (spec.md, plan.md, research.md, data-model.md, contracts/publish-step.md, contracts/image-project.md, quickstart.md)

**Prerequisites**: plan.md and spec.md. The brief of each step S1 to S12 is in plan.md under "Ordered steps and engineer briefs"; each task below names its step. `data-model.md` is the canonical source for check ids, constants, tags, archive config and the manifest list; tasks cite it and do not restate it.

**Tests**: required (constitution Principle V). They are written before the code they cover. All tests run in the runner pipeline or the selftest, never only on a laptop.

**Rules for everyone**:
- No cargo, docker, builds or tests on the 1 GB agent VM. Git and file reads only. Tests run in the selftest and the `rust-ci-runner` pipeline of a pull request.
- Never write a secret value anywhere: files, logs, commits, messages. Fixtures use synthetic strings and the canary `canary-Zq81-value-key-0123456789`.
- No agent calls `bws` or `bw`, reads a store token or creates or reads Bitwarden or Docker Hub credentials. Every such action is the owner's (T025).
- No file under `.github/workflows/` and no file under `.github/scripts/` changes (FR-022). No new crate.
- Nothing that holds the token derives `Debug`. No `#[allow(clippy::...)]` unless the surrounding code already does.
- `git add` with explicit paths. Never force-push.
- Spec Kit applies: a brief that does not name `specs/012-docker-images/` and its task IDs is refused.
- Order against 011: whichever of 011 and 012 merges second rebases (`plan.rs`, `job.rs`, `engine.rs`, `projects.rs` overlap). 012 needs the tick and the `job.sh` names-file contract as on `main`.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 a new image folder becomes a public multi-arch image, US2 an unsafe image cannot be published, US3 only `main` publishes and no secret leaks, US4 tags stay correct and removal is safe

## Phase 1: Setup

- [ ] T001 [P] Resolve and record every pin in `specs/012-docker-images/pins.md`: index digest of `mirror.gcr.io/library/golang:<a Go release that still gets security fixes>`, index digest of `gcr.io/distroless/static`, digest of `docker:27.5.1-cli`, digest of `gcr.io/go-containerregistry/crane:debug`. For each: the source tag, the digest, and the platforms `docker buildx imagetools inspect` lists (golang, distroless and docker:cli must list `linux/amd64` and `linux/arm64`, FR-028). Run the four spike checks of research.md "Spike left to the engineer" with a throwaway Docker Hub repository the owner creates, and record the result; a refusal of push by digest goes back to the lead (plan K1) (S3, S6, S12)
- [ ] T002 [P] Bump `rust-ci-runner/Cargo.toml` and the `rust-ci-runner` entry of `rust-ci-runner/Cargo.lock` from `0.3.0` to `0.4.0` (S9; `tests/repo_files.rs` `cargo_toml_and_lock_versions_match` is the proof)
- [ ] T003 [P] Create the fixture set `rust-ci-runner/tests/fixtures/image/`: one passing project (shape of contracts/image-project.md) and one failing copy per check id of data-model.md "Check rules" and per bypass listed in plan.md S1 (`ADD`, `ONBUILD`, heredoc in `ENV`, `COPY --from=alpine`, `RUN --mount=...from=alpine`, the five directive spellings, an unpinned and a tagged step image), plus CRLF, BOM, heredoc, comment-`FROM` and stage-chain variants (S1). Token-shaped strings are built synthetically

## Phase 2: Foundational (blocks every story)

- [ ] T004 Add `rust-ci-runner/src/image.rs` with the constants of data-model.md "Constants" (`NAMESPACE`, `PUBLISH_BRANCH`, `TOKEN_NAME`, `PLATFORMS`, `ARCHIVE_MAX`, `TAG_HEX`, `CRANE_IMAGE` from T001, `REGISTRY`; data-model.md Constants.NAMESPACE, Constants.PUBLISH_BRANCH, Constants.TOKEN_NAME, Constants.REGISTRY, Constants.CRANE_IMAGE) and `image_project`, `publishes`, `name_ok` (data-model.md Image project.name); register `pub mod image;` and its test module in `rust-ci-runner/src/lib.rs`. Merge before T007, T010, T013 compile (S1, S2)
- [ ] T005 [P] Write `rust-ci-runner/src/image_tests.rs`: one test per check id and per edge answer of data-model.md "Check rules" (Image project.name boundaries 1, 2, 64, 65; Image project.required files), one test per bypass of plan.md S1, `check_never_panics_on_junk` (1 MB, NUL, invalid UTF-8), output sorted and unique, the passing fixture returns empty, and a test that reads the real `docker/hello-static` once T015 lands. Mark tests that need code `#[ignore = "test-first"]` (S1; I2)
- [ ] T006 Implement `image::check` and the Dockerfile parser in `rust-ci-runner/src/image.rs` (data-model.md "Check rules": allowlist, instruction parser, continuations, heredocs for `RUN` and `COPY` only, stage index and name tracking, case-insensitive directives, hand-written token-shape scans, no regex crate, 256 KB read cap, `symlink_metadata`, `&'static str` ids only). Remove the `test-first` markers of T005 (S1; FR-008, FR-009, FR-010, FR-011, FR-027, FR-029; Image project.required files)
- [ ] T007 [P] Discovery: `rust-ci-runner/src/projects.rs` `find`, `discover` (truncated tree reads one more level under `docker`), `project_path_ok`, with tests in `rust-ci-runner/src/projects_tests.rs`: `find_docker_projects`, `discover_truncated_tree_reads_docker`, `owns_prefix_boundary`, a folder with `Dockerfile` only is a project, a folder with neither is not, `docker/x/y/.factory-ci.toml` and a `docker` file are ignored, `valid` follows data-model.md Image project.name, and the path shape is Image project.project path (S2, D8; FR-001, FR-002, FR-004)
- [ ] T008 [P] Engine additions in `rust-ci-runner/src/engine.rs`: `capture_in` (child standard input fed from a thread, closed after the write, env cleared as in `command`, `exit status: <n>` in the error, container killed on timeout), `capture` calling it with no input, `run_left(run_id)`; tests in the `tests` module of the same file with a stub engine script that copies its standard input to a file: the bytes arrive, argv and the client env hold none of them, existing `capture` tests stay green (S4; FR-015)
- [ ] T009 [P] Archive reader: `rust-ci-runner/src/archive.rs` `read_config` and `rust-ci-runner/src/archive_tests.rs` per data-model.md "Archive config" (Archive config.os, Archive config.architecture, Archive config.user, Archive config.entrypoint): both manifest forms, one image entry, 1 MB member cap, GNU long-name and PAX headers skipped, truncated and garbage input, a manifest naming a missing member, tars built in memory; register in `rust-ci-runner/src/lib.rs` (S5; FR-030, I13)
- [ ] T010 Plan rows: in `rust-ci-runner/src/plan.rs` set row `allow_docker` to `e.allow_docker && image::publishes(&p.path, &b.name)` and `secrets_ok` to `image::publishes(&p.path, &b.name)`; `walk.rs` does not change. Update `rust-ci-runner/src/plan_tests.rs` and `rust-ci-runner/src/secret_names_tests.rs` ("every row false" becomes "false except `docker/*` rows on `main`"); add `secrets_ok_true_only_for_main_image_rows` and `plain_rows_stay_in_the_plain_matrix` (a `rust-ci-runner` row on `main` under an entry with `allow_docker` lands in `Final.matrix`, a `docker/x` row in `Final.matrix_docker`) (S2; FR-023, I7). Depends on T004
- [ ] T011 Job target: `rust-ci-runner/src/job.rs` `target()` accepts `docker/<name>` through `projects::project_path_ok` and still rejects `a/b`, `docker/Hello`, `../x`; test in `rust-ci-runner/src/job_tests.rs`; `rust-ci-runner/src/ci.rs` stays unchanged and a test shows `ci` still refuses `docker/x` (S2). Depends on T007

**Checkpoint**: an image project is discovered, routed, validated and checked; nothing publishes yet.

## Phase 3: User Story 1 - A new image folder becomes a public multi-arch image (Priority: P1)

**Goal**: merging a valid `docker/hello-static/` publishes `sudtanj/hello-static:latest` and `:sha-<12 hex>`, each a manifest list with a native `linux/amd64` and `linux/arm64` image.

**Independent test**: quickstart 5 steps 1 to 3 and the integration tests `main_image_publishes_both_tags`, `list_has_both_platforms`.

- [ ] T012 [P] [US1] Write `rust-ci-runner/src/publish_tests.rs` per plan.md S6 "Acceptance": `verify_index` cases of data-model.md "Tags and the manifest list" (Manifest list.mediaType, Manifest list.manifests, Manifest list.digest: both platforms in both media types and either order pass; one entry, three, two `amd64`, no `arm64`, `unknown/unknown`, single image manifest, malformed digest, missing `platform`, not JSON fail), and every flow of S6 with a stub engine script and a fake `Api` (no token, archive cases, archive config cases, first and second tip read, teardown before any container and before the token read, `run_left` true, fresh directory with two files mounted `:ro`, token only on standard input, exit 41 / 43 / 45 / other / timeout mapping including the `exit status: <n>` text, `INDEX` missing or bad, digest comparison equal and unequal, argv and env holding no token value, container flags exactly as contracts/publish-step.md). Mark `#[ignore = "test-first"]` (S6; I1, I4, I5, I6, I10, I11, I12)
- [ ] T013 [US1] Implement `rust-ci-runner/src/publish.rs` (`publish`, `verify_index`) and register it in `rust-ci-runner/src/lib.rs` per contracts/publish-step.md: the ten steps in order, `Engine::capture_in` for containers A and B with the token as input, `INDEX` parse and `verify_index`, `--user` from `MetadataExt` of the fresh directory, archives moved by `rename`. Remove the `test-first` markers of T012 (S6; FR-013, FR-014, FR-015, FR-016, FR-017, FR-018, FR-020, FR-021, FR-026, FR-030; size limit per data-model.md Constants.ARCHIVE_MAX, platforms per Constants.PLATFORMS, tag length per Constants.TAG_HEX). Depends on T004, T008, T009, T012
- [ ] T014 [US1] Wire the job in `rust-ci-runner/src/job.rs` and `rust-ci-runner/src/job_tests.rs` (S7): `secret_names` writes `DOCKERHUB_TOKEN\n` only for `publishes` plus a clean `image::check`, empty otherwise, `NO_JOB_SECRETS` still first; `attempt` runs the check before `prepare()` (failure `check failed: <ids>`, no container), runs the pipeline, then `publish` before the final `cleanup_run`; `token_values` gains the token and the base64 of `sudtanj:<token>`; non-image projects behave as before (existing job tests unchanged); a failing build step ends `failure` with fixed lines only (FR-006 is the test of T019, FR-007). Depends on T006, T011, T013
- [ ] T015 [P] [US1] Add `docker/hello-static/{Dockerfile,hello.go,go.mod,README.md,.factory-ci.toml}` and `docker/README.md` per contracts/image-project.md with the pins of T001 (index digests, mirror builder, `GOTOOLCHAIN=local`, comment with the source tags); the pipeline builds and saves both platforms and ends with `chmod 0644`; `docker/README.md` has the digest refresh routine (S3; FR-003, FR-012, FR-028). Depends on T001
- [ ] T016 [US1] Add the integration test file `rust-ci-runner/tests/job_publish.rs` and extend `rust-ci-runner/tests/common/mod.rs` (`Fake` answers `branches`; a `STUB_` hook makes container A print an `INDEX` line and records its standard input; fixtures carry both archives as small tars): `main_image_publishes_both_tags`, `list_has_both_platforms`, `single_arch_or_bad_index_never_tags`, `failed_build_pushes_nothing`, `no_tag_other_than_sha12_and_latest_is_ever_named` over every argv and env (S8; FR-014, FR-017, FR-026). Add `--test job_publish` to the integration command in `rust-ci-runner/.factory-ci.toml`. Depends on T014
- [ ] T017 [US1] Turn docker on for the image rows: in `.github/factory-ci-repos.toml` add `allow_docker = true` to the `sudtanj/infrastructure` entry, keeping `repo`, `id` and `branches = ["main"]` (S3, FR-023; research D3). Safe to merge with the 0.3.0 runner still pinned (plan.md "Rollout" step 2). Depends on T015

## Phase 4: User Story 2 - An unsafe image cannot be published (Priority: P1)

**Goal**: every rule of data-model.md "Check rules" fails the project with a named id and zero pushes; a pipeline that omits a check still cannot publish; the bypasses fail.

**Independent test**: one fixture per id and per bypass (T003) through `image::check` (T005) and through the job (T018).

- [ ] T018 [US2] In `rust-ci-runner/tests/job_publish.rs`: `failed_check_pushes_nothing` over a sample of ids and bypasses (no container starts, status `check failed: <id>`), `omitted_check_step_still_refused_in_publish` (a build step rewrites the Dockerfile after the first check; the second check in `publish` refuses), `pipeline_with_secrets_fails_check`, `step_image_unpinned_fails_check`, `emulation_refused`, `archive_with_root_user_shell_entrypoint_or_wrong_platform_refused` (S8; FR-008, FR-011, FR-027, FR-029, FR-030, SC-002). Depends on T016
- [ ] T019 [P] [US2] Extend `rust-ci-runner/tests/repo_files.rs` (S10): `docker_folders_pass_the_static_check` runs `image::check` over every folder under `docker/` (this is FR-006); every folder has the three files and a name that passes `image::name_ok`; the infrastructure entry has `allow_docker` true, `branches == ["main"]` and no secrets; the FR-022 test runs `git diff --name-only <base>...HEAD` with `<base>` from `FACTORY_CI_BASE` or `origin/main`, fails on any path under `.github/workflows/` or `.github/scripts/`, and prints a skip line, not a pass, when git or the base is missing. Keep the file out of the pipeline's integration command. Record in the test file's header that the selftest paths filter lacks `docker/**` (plan K4). Depends on T015, T017

## Phase 5: User Story 3 - Only `main` publishes, and no secret leaks (Priority: P1)

**Goal**: a non-image project and a failing image never get the token; a `main` run leaks it nowhere and hands it only to the two publish containers on standard input.

**Independent test**: `non_image_project_reads_no_token`, `canary_token_absent_from_public_lines_argv_envs_status`, `token_only_on_stdin_of_publish_containers`.

- [ ] T020 [US3] In `rust-ci-runner/tests/job_publish.rs`: `non_image_project_reads_no_token` (prepare names empty, publish never called, zero crane calls, zero tip reads), `canary_token_absent_from_public_lines_argv_envs_status` over stdout, stderr, argv of every engine call, every engine env, status text and Loki lines, `token_only_on_stdin_of_publish_containers`, `sidecar_removed_before_publish_containers_start` (call order in the stub log), `publish_dir_holds_only_the_archives_read_only`, `user_step_env_never_holds_token` (stub envs file; no build step ever holds the token, so it cannot reach an image layer) (S8; FR-013, FR-015, FR-024, SC-003, I4, I12). Depends on T016
- [ ] T021 [P] [US3] Update the expectations that the names file is always empty in `rust-ci-runner/tests/job_prepare.rs`, `rust-ci-runner/tests/job_rules.rs` and `rust-ci-runner/tests/no_vendor.rs`: still empty for non-image projects, a failing check and a pipeline that lists a secret (`NO_JOB_SECRETS` first); `DOCKERHUB_TOKEN` only for a `main` image project with a clean check; add `dockerhub_token_is_a_valid_secret_name` against `valid_secret_name` in the `tests` module of `rust-ci-runner/src/pipeline.rs` (S8; I7, I8). Depends on T014

## Phase 6: User Story 4 - Tags stay correct and removal is safe (Priority: P2)

**Goal**: an older commit never moves `latest`; a retry cannot rewrite a published tag; a deleted or untouched folder changes nothing.

**Independent test**: `older_commit_pushes_sha_only`, `existing_list_with_other_digests_errors`, `deleted_folder_makes_no_call`.

- [ ] T022 [US4] In `rust-ci-runner/tests/job_publish.rs`: `older_commit_pushes_sha_only` (fake `branches` answers another sha on the first read, then on the second), `unreadable_tip_pushes_nothing`, `second_tip_read_fails_leaves_sha_tag`, `repeat_publish_with_equal_digests_continues`, `existing_list_with_other_digests_errors`; in `rust-ci-runner/src/plan_tests.rs`: `deleted_folder_makes_no_call` and `unchanged_commit_builds_nothing` for a `docker/<image>` project (S8; FR-005, FR-018, FR-019, FR-020). Depends on T016, T010

## Phase 7: Polish, release and proof

- [ ] T023 Amend the constitution layout bullet to list `docker/`, version `1.1.1`, through the `speckit-constitution` skill, in `.specify/memory/constitution.md` (S9; Governance)
- [ ] T024 Open the pull request for `012-docker-images`, link `specs/012-docker-images/`, and wait for the `factory-ci-selftest` result on the head commit to be green (it covers `--lib`, `--test '*'`, `repo_files` and the static check of every `docker/` folder). Run quickstart 3 (the path check) and record `ok`. Merge only when green (FR-022, SC-004). Depends on T002, T016 to T023
- [ ] T025 [Owner] Quickstart 1, spec A14: Docker Hub token with Read & Write (no delete, no admin) and an expiry; the public repository `sudtanj/hello-static` created by hand; a Bitwarden project that holds only `sudtanj/infrastructure/DOCKERHUB_TOKEN`, reachable only by the machine account behind `BWS_ACCESS_TOKEN_DOCKER`, named by `FACTORY_CI_BWS_PROJECT_DOCKER`; `FACTORY_CI_TOKEN_DOCKER` covers `sudtanj/infrastructure` with Commit statuses read and write and Contents and Metadata read, and is the same GitHub user as `FACTORY_CI_TOKEN`; `factory-ci-docker` limited to `main`, no required reviewers; `FACTORY_CI_BLOB_KEY_DOCKER` set; the digest `gh variable get RUNNER_IMAGE_DIGEST` recorded. Record the answers here without any value
- [ ] T026 Release (quickstart 4): merge, tag `rust-ci-runner-v0.4.0`, record the digest the release workflow prints, `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`, watch two ticks (`factory-ci/rust-ci-runner` and `factory-ci/ci-canary` post once and are not planned again; a re-plan means two token users, roll back). Record both digests. Depends on T024, T025
- [ ] T027 End to end and post-publish check (quickstart 5 and 6, SC-001, SC-003, SC-006, SC-007): hello-static pulls without login, both tags list exactly `linux/amd64` and `linux/arm64` with equal digests, a pull and run on an amd64 host and on an arm64 host, a token search of the public log and status, the broken-Dockerfile and `emulation` cases, and `sha-<first 12 hex of the main tip>` exists on Docker Hub after the tick (no answer counts as unknown). Repeat the post-publish check after every later tick that ran a `docker/` change. Record evidence here. Depends on T026
- [ ] T028 Update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description (`gh repo edit sudtanj/infrastructure --description ...`) to match this feature (FR-025): what `docker/` is and how to add an image, both architectures, the owner steps (repository by hand, token, Bitwarden key), the publish step and its check ids, `DOCKERHUB_TOKEN`, the post-publish check, the rollback (variable, delete tags by hand, revoke the token, revert the toml in a separate PR), the amended sentence that the tick hands no secret to a user step, and the residual risk stated plainly: whoever controls a `main` build step can tamper with the host through the privileged sidecar and is effectively a token holder; `main` is owner-merged; the check is review help, not a barrier; rootless BuildKit is the future fix, out of scope. Prose per `stop-slop`; confirm with `gh repo view --json description`. This is the last task. Depends on T027

## Dependencies and order

- Phase 1 first; T001 feeds T004 (`CRANE_IMAGE`) and T015.
- Phase 2: T004 before T007, T010, T013; T005 and T006 in sequence; T007, T008, T009 in parallel with T005; T010 after T004; T011 after T007.
- Phase 3: T012 before T013; T013 after T008 and T009; T014 after T006, T011, T013; T015 parallel to T012 to T014; T016 after T014; T017 after T015.
- Phases 4 to 6 run after T016 and are parallel to each other (shared file `tests/job_publish.rs`: append in separate commits).
- Phase 7: T023 any time; T024 after all code and test tasks; T025 owner, any time before T026; T026 to T028 in order.

## Parallel examples

- After T004: T005, T007, T008, T009 together.
- T012 and T015 together.
- After T016: T018, T019, T020, T021, T022 on different files or separate commits.

## Implementation strategy

MVP is Phase 1 to Phase 3: discovery, routing, check, archive check, publish, wiring and `hello-static`. Stories 2 to 4 add proof that the guards hold. Nothing reaches Docker Hub before T026 moves `RUNNER_IMAGE_DIGEST`; until then the 0.3.0 runner ignores `docker/` (plan.md "Rollout"). Rollback is `gh variable set RUNNER_IMAGE_DIGEST` back to the value recorded in T025.
