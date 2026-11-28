# Implementation Plan: Public Docker images from `docker/`

**Branch**: `012-docker-images` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/012-docker-images/spec.md`

## Decisions fixed by the lead (2026-11-28) and what stays open

Decided, folded into spec, plan, tasks and contracts:

1. **Token route.** The runner cannot read Bitwarden on the tick: `.github/scripts/factory-ci-job.sh:115` unsets the store token and project before it `exec`s the runner (line 117), and the script is frozen. The frozen script fetches the token through its names-file mechanism (`job.sh:60-101`), only for a passing `docker/<image>` row on `main`. Only the runner and the push container receive it. Spec clarification and A11 say so.
2. **Branch exposure.** `branches = ["main"]` stays. The plan gives a row `allow_docker` only for `docker/<image>` on `main` (`plan.rs:307` today copies the entry flag to every row). `rust-ci-runner` and `ci-canary` stay in the plain job and `docker_problems(false)` still refuses `docker = true` for them. The `walk.rs` marker rule is dropped. Other branches get the static check as a Rust test (`tests/repo_files.rs`).
3. **Token handling.** Tear the sidecar down before publish; the token goes to `crane` on standard input, never env or argv; a fresh read-only directory holds only the two archives; publish verifies each archive's config first.
4. **Check hardening.** Instruction allowlist, heredoc and `--from` rules, case-insensitive directives, pinned pipeline step images.
5. **Tags.** `sha-<12 hex>`. Reuse of an existing list compares child digests with the archives and fails on mismatch.
6. **Supply chain.** Go builder from `mirror.gcr.io/library/golang:<supported>@sha256:<index digest>`, `GOTOOLCHAIN=local`.
7. **Owner setup.** Public repository pre-created; token, Bitwarden project and Environment as in spec A14.

Still open, none of them blocks starting work:

- **K1, push by digest is unverified.** The publish writes platform images to `repo@sha256:<digest>` so that no extra tag exists. If Docker Hub refuses, the fallback is a temporary tag, which breaks FR-017. T001 checks it with a throwaway repository; a refusal goes back to the lead.
- **K2, two tokens, one user.** The entry is now mixed: `plan.rs` reads statuses and posts `unchanged` and `invalid` with the docker token, while plain rows post `pending` and the final status with the plain token (`plan.rs:278` for `unchanged` and `invalid`, `plan.rs:455` for `pending`, field `Status.docker`). `walk::own` only trusts statuses made by the API user it read (`walk.rs:67`). If `FACTORY_CI_TOKEN` and `FACTORY_CI_TOKEN_DOCKER` belong to different GitHub users, every plain row of this repo is planned again every tick. T025 has the owner confirm one user; T026 watches two ticks.
- **K3, retry after a written list.** A rebuild is not byte-identical, so a retry that finds the `sha-` list already written fails the digest comparison (`existing tag differs from this build`) instead of finishing `latest`. The next commit publishes. A reproducible build would lift this; it is out of scope.
- **K4, selftest does not cover `docker/**`.** `factory-ci-selftest.yml` triggers on `rust-ci-runner/**`, workflow, script and watch-list paths only. A push that changes nothing but `docker/` does not start it, and the YAML cannot change. Known limit (spec FR-006). The check on `main` still blocks a publish.
- **K5, 011 overlap.** `plan.rs`, `job.rs`, `engine.rs`, `projects.rs` are also large diffs on `011-ci-state-queue`. Whichever branch merges second rebases. 012 needs the tick and the `job.sh` names-file contract as on `main`.
- **K6, FR-022 test.** It needs git history, which the runner pipeline mount lacks. It runs as a documented command (quickstart 3) and as a guarded test the selftest runs.
- **K7, FR-015 wording.** The push container is a runner-owned container, not a user step; "step" in FR-015 is read that way.

## Summary

Add `docker/<image>/` folders that the existing tick finds on `main`, builds in the per-run docker sidecar for `linux/amd64` and `linux/arm64`, checks and publishes to Docker Hub as one multi-arch manifest list per tag. All change is Rust under `rust-ci-runner/`, toml, `docker/` content and docs. Runner changes:

1. Discovery of `docker/<image>` projects, and row `allow_docker` and `secrets_ok` only for those on `main` (`projects.rs`, `plan.rs`, `job.rs` target check).
2. `image.rs`: the check, run before the build and again inside publish. Allowlist parser; refuses any `RUN` outside a `--platform=$BUILDPLATFORM` stage because arm64 is cross-compiled, never emulated.
3. `archive.rs`: reads the image config of a `docker save` tarball (user, entrypoint, platform).
4. `engine.rs`: `capture_in` (a child with the standard input fed) and `run_left`.
5. `publish.rs`: after the pipeline, tear the sidecar down, verify both archives, read the tip of `main`, push both platform images by digest and write the manifest list in one registry write from a `crane` container that receives the token on standard input, check the list with `verify_index`, tag `latest` only when the commit is still the tip.

Toml: `allow_docker = true` on the `sudtanj/infrastructure` entry; `branches` unchanged. Runner `0.3.0` to `0.4.0`; the owner sets `RUNNER_IMAGE_DIGEST` after the release. That variable is the switch and the rollback.

## Technical Context

**Language/Version**: Rust 2021, toolchain 1.90 (`rust-ci-runner/rust-toolchain.toml`).

**Primary Dependencies**: none added. `std`, `serde`, `serde_json`, `toml`, `ureq`. No `regex`; token-shape scans and the tar header reader are hand-written.

**Storage**: files only. Two archives in the checkout, moved into a fresh directory for the push, removed with the run.

**Testing**: `cargo test --locked` in the runner pipeline (`unit`, `integration` steps of `rust-ci-runner/.factory-ci.toml`) and the selftest (`--lib`, `--test '*'`); stub `docker` and fake GitHub from `tests/common/mod.rs`; one real publish by hand.

**Target Platform**: GitHub-hosted `ubuntu-24.04` VMs, the runner binary on the host, containers through the host engine.

**Project Type**: CLI runner plus repo content.

**Performance Goals**: push within 15 minutes of the merge (SC-001).

**Constraints**: no change under `.github/workflows/` or `.github/scripts/`; no secret in any repo file; no build on the 1 GB planning VM.

**Scale/Scope**: one image now; one folder per later image, nothing else changes (SC-005).

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Automation stays in `rust-ci-runner/` and `.github/`. The layout bullet of "Repository Structure and Constraints" gets `docker/` by a PATCH amendment (T023). |
| II Secrets source | Pass. The token is in Bitwarden, fetched by the existing script, in no repo file. No new Actions secret or variable. |
| III Nothing private in public logs | Pass. Statuses are fixed lines; the token is masked by the script, redacted in the runner, and never in argv or a container environment; crane output goes to `/dev/null`. |
| IV Simplicity | Pass with the three items in Complexity Tracking. |
| V Tested changes | Pass. The FR-to-test map is below. FR-022 and the `docker/**` selftest gap are the two limits (K4, K6). |
| VI Spec-driven | Pass. Every change has a task. |

## Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| `crane` image const | Pushes two archives by digest and writes the list without a hand-written registry client | A `ureq` registry client is about 300 lines plus digest code |
| `image.rs` Dockerfile parser with an allowlist | FR-009 to FR-011 and FR-029 need instructions, not text | A line grep fails heredocs, stages and `ADD` |
| `archive.rs` tar header reader | FR-030 checks the image that will be pushed, not the Dockerfile that claims to build it | Trusting the Dockerfile leaves a `main` step free to write any archive |

## Architecture decision

**Chosen.** The tick stays the only trigger. The planner discovers `docker/<image>` and gives those rows on `main` the docker job and the token name. The job clones, runs the check, runs the user pipeline (build and save) in the docker sidecar, tears the sidecar down, verifies the archives, and the runner pushes through a `crane` container that gets the token on standard input.

**Rejected.**

- Runner reads Bitwarden itself: impossible without a script edit (research D1).
- Native Rust registry client: larger, new attack surface (D2).
- Push from the sidecar, host `docker login`, token in env or argv: break FR-015.
- Check at tick plan time: no file contents there without a new `Api` method (D5).
- Wider `branches`, or a branch publish marker rule: lead decision 2; the marker analysis stays in research D3 for a later widening.
- A per-project docker key in the watch file: parser, plan and test cost.
- QEMU in the sidecar: D10. Signing, version tags: spec non-goals.

## Files to add or change

All paths relative to the repo root. No path under `.github/workflows/` or `.github/scripts/`.

| Path | Change |
|---|---|
| `rust-ci-runner/src/image.rs`, `image_tests.rs` | NEW. Constants, `image_project`, `publishes`, `name_ok`, parser, `check`; tests per rule and bypass. |
| `rust-ci-runner/src/archive.rs`, `archive_tests.rs` | NEW. `read_config(path)`; tar built in the test. |
| `rust-ci-runner/src/publish.rs`, `publish_tests.rs` | NEW. `publish`, `verify_index`; stub engine and fake `Api`. |
| `rust-ci-runner/src/engine.rs` | `capture_in(args, env, input, limit, container)`; `capture` calls it with no input; `run_left(run_id)`. Tests in its `tests` module. |
| `rust-ci-runner/src/lib.rs` | `pub mod image; pub mod archive; pub mod publish;` and test modules. |
| `rust-ci-runner/src/projects.rs`, `projects_tests.rs` | `find`, `discover` for `docker/<name>`; `project_path_ok`. |
| `rust-ci-runner/src/plan.rs`, `plan_tests.rs`, `secret_names_tests.rs` | Row `allow_docker` and `secrets_ok` from `image::publishes`; tests. `walk.rs` does not change. |
| `rust-ci-runner/src/job.rs`, `job_tests.rs` | `target()` uses `project_path_ok`; `secret_names` writes the token name; `attempt` runs the check and `publish`; `token_values` adds the token and its docker-auth base64. |
| `rust-ci-runner/tests/common/mod.rs` | `Fake` answers `branches`; a `STUB_` hook for the crane output and stdin capture. |
| `rust-ci-runner/tests/job_publish.rs` | NEW. Binary-level publish tests. |
| `rust-ci-runner/tests/job_prepare.rs`, `job_rules.rs`, `no_vendor.rs` | Update "names file always empty" expectations. |
| `rust-ci-runner/tests/repo_files.rs` | Static check over every `docker/` folder (FR-006), watch-entry facts, guarded FR-022 path test. |
| `rust-ci-runner/tests/fixtures/image/` | NEW. Passing project and one failing fixture per rule and bypass. |
| `rust-ci-runner/.factory-ci.toml` | Add `--test job_publish` to the integration command. |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | Version `0.4.0`. |
| `rust-ci-runner/README.md`, `README.md` | Image projects, publish step, key name, residual risk, rollback. Last task. |
| `.github/factory-ci-repos.toml` | `sudtanj/infrastructure`: add `allow_docker = true`; `branches` stays. |
| `docker/hello-static/{Dockerfile,hello.go,go.mod,README.md,.factory-ci.toml}`, `docker/README.md` | NEW first image and the refresh routine. |
| `.specify/memory/constitution.md` | PATCH: add `docker/` to the layout bullet. |

## Data and API contracts

- [data-model.md](data-model.md): check ids, constants, tags, manifest list, status texts, state flow.
- [contracts/publish-step.md](contracts/publish-step.md): names-file hand-off, publish function, teardown, fresh directory, crane containers and stdin protocol, exit codes, redaction.
- [contracts/image-project.md](contracts/image-project.md): folder, pipeline and Dockerfile shapes.
- No HTTP API of ours changes. `Api` gains no method; the tip comes from `branches`.

## Ordered steps and engineer briefs

Feature folder for every brief: `specs/012-docker-images/`. Task IDs are in `tasks.md`. Every brief carries: the repo is public, no secret value in any file, tests run only in the runner pipeline, no workflow or script edit, no new crate.

Parallel groups: **A** = S1, S2, S4, S5 (disjoint files). **B** = S6 after S1, S4, S5. **C** = S7 after A and B. **D** = S3, S8, S9, S10 after C. **E** = S12, owner steps and release, after D. S11 (docs) last, after S12 proves the feature.

### S1. Check module (group A)

- **Goal**: `image::check(dir) -> Vec<&'static str>` and the constants of `data-model.md`.
- **Files**: `image.rs`, `image_tests.rs`, `lib.rs`, `tests/fixtures/image/`.
- **Acceptance**: every id and edge answer of the data-model table has a test that fails without the rule. Name boundaries 2 and 64 pass, 1 and 65 fail. Each bypass has its own test: `ADD`, `ONBUILD`, `SHELL`, `HEALTHCHECK`, `VOLUME` (`instruction`); `<<` in `ENV`, `ARG`, `LABEL`, `FROM` (`heredoc`) while `RUN` and `COPY` heredoc bodies are skipped and a `FROM` inside one is ignored; `COPY --from=alpine`, `--from=docker.io/x`, `--from=5` (index not earlier), `RUN --mount=type=bind,from=alpine,target=/x` (`external-source`) while `--from=build` and `--from=0` pass; directives `# syntax=x`, `#syntax = x`, `# SYNTAX=x`, `# Escape=\``, `#   syntax=x` (`directive`); a pipeline step `image = "alpine"`, `image = "alpine:3.23"` (`step-image`). Plus CRLF, BOM, comment-`FROM`, `FROM alpine AS alpine`, stage chains, `--platform=$BUILDPLATFORM`, `emulation` with and without the flag, `USER $X`, `ARG TARGETARCH`. A 1 MB Dockerfile, NUL and invalid UTF-8 return `layout`, never panic. Passing fixture returns empty. Output sorted and unique.
- **Constraints**: no regex crate; Dockerfile capped at 256 KB and read with `symlink_metadata`; only `&'static str` ids leave the module; the allowlist is `FROM ARG ENV WORKDIR COPY RUN USER ENTRYPOINT CMD LABEL EXPOSE`.

### S2. Discovery and plan rows (group A)

- **Goal**: `docker/<name>` is a project the plan lists and the job accepts; only `docker/<name>` rows on `main` get `allow_docker` and `secrets_ok`.
- **Files**: `projects.rs`, `projects_tests.rs`, `plan.rs`, `plan_tests.rs`, `secret_names_tests.rs`, `job.rs` (`target()` only), `job_tests.rs`.
- **Acceptance**: `find` returns `docker/x` for a folder with `.factory-ci.toml` or `Dockerfile`, ignores `docker/x/y/.factory-ci.toml`, a `docker` file and a folder with neither; `valid` follows the FR-002 name rule; a truncated tree still finds it; `owns("docker/x", "docker/x/Dockerfile", set)` true, `docker/xy/...` false; with the entry `allow_docker = true`, a `docker/x` row on `main` has `allow_docker` and `secrets_ok` true, a `rust-ci-runner` row on `main` has both false and lands in `Final.matrix`, a `docker/x` row on another branch has both false; `target()` accepts `docker/hello-static`, rejects `a/b`, `docker/Hello`, `../x`; `ci.rs` unchanged and a test shows `ci` still refuses `docker/x`; `walk.rs` unchanged.
- **Constraints**: take `publishes` and `image_project` from S1's first commit; merge that first.

### S3. Watch entry and hello-static content (group D)

- **Goal**: the first image exists and the entry turns docker on for its rows.
- **Files**: `.github/factory-ci-repos.toml`, `docker/hello-static/*`, `docker/README.md`.
- **Acceptance**: the entry keeps `repo`, `id` and `branches = ["main"]`, gains `allow_docker = true`; `watch::parse` accepts it; `hello-static` passes `image::check`; the Dockerfile follows `contracts/image-project.md` with the pins of `pins.md` (index digests; builder `mirror.gcr.io/library/golang:<supported>@sha256:<index>`; `GOTOOLCHAIN=local`; `docker buildx imagetools inspect` lists both platforms for each, recorded in the task); the pipeline builds and saves both platforms and ends `chmod 0644 image-*.tar`; `docker/README.md` names the digest refresh routine.
- **Constraints**: safe to merge before the release: the 0.3.0 runner ignores `docker/` and the new flag only changes row routing (rollout step 2).

### S4. Engine additions (group A)

- **Goal**: `Engine::capture_in` and `Engine::run_left`.
- **Files**: `engine.rs`.
- **Acceptance**: `capture_in` writes `input` to the child's standard input from a thread and closes it, keeps the env-clearing of `Engine::command`, returns stdout, reports a non-zero exit as `exit status: <n>` and kills the container on timeout; `capture` behaves as before (existing tests green); `run_left(run_id)` is true while any container, network or volume with the label `factory-ci.run=<run_id>` exists. Tests with a stub engine script that echoes its stdin to a file: the bytes arrive; argv and the client env hold none of them.
- **Constraints**: no `Command::new` outside `command()`; the input never reaches a `format!`.

### S5. Archive config reader (group A)

- **Goal**: `archive::read_config(path) -> Result<ImageConfig, &'static str>`.
- **Files**: `archive.rs`, `archive_tests.rs`, `lib.rs`.
- **Acceptance**: reads `manifest.json` then the config member it names (both `blobs/sha256/<hex>` and `<hex>.json` forms) from a `docker save` tar by walking 512-byte headers and seeking past data; returns `os`, `architecture`, `user`, entrypoint and cmd; one image entry required; member size cap 1 MB; GNU long-name and PAX headers skipped; a truncated or garbage file, a missing member and a manifest naming a path that is not in the tar return fixed errors, never panic; tests build tars in memory.
- **Constraints**: no new crate; no allocation proportional to the archive size.

### S6. Publish step (group B)

- **Goal**: `publish::publish` and `publish::verify_index` per `contracts/publish-step.md`.
- **Files**: `publish.rs`, `publish_tests.rs`, `lib.rs`.
- **Acceptance**: with a stub engine and a fake `Api`: not `main` or not an image project returns without an env read, engine call or API call; missing or empty token gives `publish refused: no token`; a missing, symlinked, empty or oversized archive gives the archive lines; an archive whose config has an empty, non-numeric or 0 user, a shell entrypoint or the wrong platform gives its fixed line and starts no container; an unreadable first tip pushes nothing; `run_left` true after teardown gives `publish refused: sidecar still running` and no container starts; teardown (`cleanup_run`) happens before the first publish container and before the token is read from the environment; the publish directory holds exactly the two archives and is mounted `:ro`; container argv holds no token and no `-e` for it, the client env holds none, the token bytes arrive only on standard input of containers A and B; exit 41, 43, 45, other codes and a timeout map as the contract says; a list whose children differ from the archives is `existing tag differs from this build` and container B never runs; `verify_index` cases of `data-model.md`; tip equal on both reads runs B, different gives `not latest`; repeat publish with equal digests ends in the same text.
- **Constraints**: `Engine::capture_in` for both containers, no `Command::new`; the crane image const is a full digest pin (test asserts `@sha256:` shape); uid and gid from `MetadataExt` of the publish directory; no token in a `format!` that reaches a `Done` text; captured stdout parsed, never logged.

### S7. Wire the job (group C)

- **Goal**: `job.rs` runs the check, the pipeline and publish, and lists the token name.
- **Files**: `job.rs`, `job_tests.rs`.
- **Acceptance**: prepare mode: the names file is `DOCKERHUB_TOKEN\n` only for `publishes` plus a clean check, empty otherwise, `NO_JOB_SECRETS` still first; `attempt` runs the check before `prepare()` (`check failed: <ids>`, no container), runs the pipeline, then `publish` before the final `cleanup_run`; non-image projects behave as before; `token_values` gains the token and the base64 of `sudtanj:<token>`; a failing build step ends `failure` with fixed lines.
- **Constraints**: do not log or `format!` the env value.

### S8. Integration tests (group D)

- **Goal**: binary-level proof with the stub engine and fake API.
- **Files**: `tests/job_publish.rs`, `tests/common/mod.rs`, `tests/job_prepare.rs`, `tests/job_rules.rs`, `tests/no_vendor.rs`, `rust-ci-runner/.factory-ci.toml`.
- **Acceptance**: the list in "Test plan"; old assertions kept for non-image projects; the integration command names `job_publish`.
- **Constraints**: canary `canary-Zq81-value-key-0123456789` only; fixture archives are small tars built in the test.

### S9. Version and constitution (group D)

- **Files**: `rust-ci-runner/Cargo.toml`, `Cargo.lock`, `.specify/memory/constitution.md`.
- **Acceptance**: `repo_files::cargo_toml_and_lock_versions_match` passes with `0.4.0`; the layout bullet lists `docker/`, version `1.1.1`, through the `speckit-constitution` skill.

### S10. Repo-file tests (group D)

- **Files**: `tests/repo_files.rs`.
- **Acceptance**: every folder under `docker/` has the three files, a kebab name and an empty `image::check` result (this is FR-006); the infra entry has `allow_docker` true, `branches == ["main"]`, no secrets; the FR-022 test runs `git diff --name-only <base>...HEAD` (`<base>` from `FACTORY_CI_BASE` or `origin/main`), fails on a path under `.github/workflows/` or `.github/scripts/`, and prints a skip line, not a pass, when git or the base is missing.
- **Constraints**: stays out of the pipeline's integration command (the file's own comment says why).

### S11. Docs and repository description (last task, after S12)

- **Files**: `README.md`, `rust-ci-runner/README.md`; the description by `gh repo edit sudtanj/infrastructure --description ...`.
- **Acceptance**: the root README says what `docker/` is and how to add an image, both architectures, the owner steps (repository created by hand, token, key); the runner README documents the publish step, the check ids, `DOCKERHUB_TOKEN` and its key, the post-publish check, the rollback, and states the residual risk plainly: whoever controls a `main` build step can tamper with the host through the privileged sidecar and is effectively a token holder; `main` is owner-merged; the check is review help, not a barrier; a rootless BuildKit is the future fix, out of scope; it also amends the sentence that the tick hands no secret to a step (true for user steps, relaxed for the built-in publish). `gh repo view --json description` shows the description. Prose per `stop-slop`.

### S12. Release and end to end (owner plus engineer, after group D)

- **Steps**: quickstart 1, 4, 5 and the post-publish check. The engineer tags; the owner holds Bitwarden, Docker Hub and variable steps.
- **Acceptance**: SC-001, SC-003, SC-006 and SC-007 evidence in `tasks.md`.

## Test plan (qa runs this)

**Unit** (runner pipeline `unit` step; also the selftest):

| Area | Tests |
|---|---|
| Check rules | one per id and edge, one per bypass in S1 (`image_tests.rs`), `check_never_panics_on_junk` |
| Discovery and rows | S2 list (`projects_tests.rs`, `plan_tests.rs`, `secret_names_tests.rs`) |
| Engine | S4 list (stdin delivered, not in argv or env) |
| Archive | S5 list over in-memory tars |
| Publish | S6 list, `verify_index` over generated permutations and mutations |
| Job | prepare names table, attempt flow, redaction (`job_tests.rs`) |
| Names | `dockerhub_token_is_a_valid_secret_name` against `valid_secret_name` |

**Integration** (`integration` step, `tests/job_publish.rs`, stub docker, fake GitHub):

| Test | FR |
|---|---|
| `main_image_publishes_both_tags` | FR-014, FR-017 |
| `list_has_both_platforms` and `single_arch_or_bad_index_never_tags` | FR-026 |
| `failed_build_pushes_nothing` (one archive absent, or a failing step) | FR-026, FR-007 |
| `failed_check_pushes_nothing` per sampled id and bypass, no container starts | FR-008, FR-029, SC-002 |
| `omitted_check_step_still_refused_in_publish` (a build step rewrites the Dockerfile) | FR-008 |
| `pipeline_with_secrets_fails_check`, `step_image_unpinned_fails_check` | FR-011, FR-029 |
| `non_image_project_reads_no_token` (prepare names empty, publish never called) | FR-013, SC-003 |
| `canary_token_absent_from_public_lines_argv_envs_status` over every output and every engine call | FR-015, SC-003 |
| `token_only_on_stdin_of_publish_containers` (argv, client env, container env, stdin capture) | FR-015 |
| `sidecar_removed_before_publish_containers_start` (call order in the stub log) | FR-015 |
| `publish_dir_holds_only_the_archives_read_only` | FR-014, FR-015 |
| `archive_with_root_user_shell_entrypoint_or_wrong_platform_refused` | FR-030 |
| `older_commit_pushes_sha_only`, `unreadable_tip_pushes_nothing`, `second_tip_read_fails_leaves_sha_tag` | FR-018 |
| `repeat_publish_with_equal_digests_continues`, `existing_list_with_other_digests_errors` | FR-020 |
| `no_tag_other_than_sha12_and_latest_is_ever_named` over every engine argv and env | FR-017, FR-026 |
| `deleted_folder_makes_no_call`, `unchanged_commit_builds_nothing` (plan level) | FR-019, FR-005 |
| `user_step_env_never_holds_token` (stub envs file) | FR-015, FR-024 |
| `plain_rows_stay_in_the_plain_matrix` (plan level, `rust-ci-runner` row on `main` with the entry flag) | FR-023 |
| `repo_files::docker_folders_pass_the_static_check` (selftest) | FR-006 |

**Real end to end** (quickstart 5): once on `hello-static`, SC-001, SC-003, SC-006; then the post-publish check (SC-007) after every tick that ran a `docker/` change.

**Not automated**: SC-005; FR-022 inside the runner pipeline (K6); a branch push that touches only `docker/` (K4).

## Failure-mode table

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| Discovery | `docker/` too deep in a truncated tree | `discover` | extra tree read | `discover_truncated_tree_reads_docker` |
| Rows | entry flag copied to a non-image row | `plan.rs` | only `docker/*` on `main` get it | `plain_rows_stay_in_the_plain_matrix` |
| Rows | two tokens, two users (K2) | second tick re-plans a plain row | owner check T025, canary T026 | quickstart 4 |
| Check | Dockerfile unreadable, huge, not UTF-8, hostile syntax | `image::check` | named id, `failure` | S1 tests |
| Prepare | check fails | `secret_names` | empty names, real run reports `failure` | prepare names table |
| Token fetch | key missing, ambiguous, store down | script (`job.sh:78-86`) | `error`, the script's fixed line | E2E |
| Build | `docker build` fails, disk full, a base is single-arch | step exit | `failure` or `error` (125), no archive pair | `failed_build_pushes_nothing` |
| Teardown | sidecar or volume left | `run_left` | `error` `publish refused: sidecar still running` | publish tests |
| Archive | missing, symlink, too big, unreadable | `publish` | `error`, no push | publish tests |
| Archive | root user, shell entrypoint, wrong platform | `archive::read_config` | `error`, no push | `archive_with_root_user_...` |
| Token | empty in env | `publish` | `error` `publish refused: no token` | publish tests |
| Registry | 401, 403, repository missing | exit 41 | `error` `registry refused the token` | publish tests |
| Registry | network, rate limit | exit 42, 43, timeout | `error` `push failed`, one retry after 30 minutes | publish tests |
| Index | list malformed or missing a platform | `verify_index` | `error` `image index incomplete`, `latest` untouched | `single_arch_or_bad_index_never_tags` |
| Reuse | `sha-` list exists with other digests (K3) | exit 45 | `error` `existing tag differs from this build`, no tag moved | `existing_list_with_other_digests_errors` |
| Tip | `branches` fails or no `main` | `publish` | first read: nothing pushed; second read: `sha-` stays | tip tests |
| Tip | newer commit exists | `sha != tip` | `success` `published sha-<12>, not latest` | `older_commit_pushes_sha_only` |
| Partial | list written, `latest` fails | exit of B | `error`; retry hits K3 unless the rebuild matches | `repeat_publish_...` |
| Status post | GitHub down | existing `post` | exit 1, script fallback | existing |
| Crash mid-publish | VM dies | stale `pending` after 2 h | existing retry rule | K3 note |

## Pre-mortem

1. **`crane` rejects a `docker save` tarball, refuses a push by digest, or `crane:debug` has no `sh`; or a pinned base is not an index so one platform fails.** Prevention: T001 runs the checks on a throwaway repository before any code depends on them; a push-by-digest refusal goes back to the lead (K1); base digests are checked with `imagetools inspect` (FR-028).
2. **The token leaks.** Prevention: stdin only; no env, no argv, no mount; `env_clear` in `Engine::command`; the sidecar is gone before the containers start; archives are checked; crane output to `/dev/null`; redactor holds the token and its docker-auth base64; the canary test covers every engine call; non-image and failing-check runs never get the name.
3. **Turning on `allow_docker` breaks the repo's own CI.** Likely: `FACTORY_CI_TOKEN_DOCKER` missing or unable to write statuses, so `plan_a` drops the repo (`main.rs:100-120`); or two users (K2). Prevention: owner checks in T025 before the digest moves; the canary tick watches two ticks; the toml line is safe to merge early; rollback is the digest variable plus a separate toml revert.
4. **A hostile or sloppy Dockerfile passes the check.** Prevention: allowlist, heredoc and `--from` rules, directives in any case, pinned step images, archive config check at publish; residual risk stated plainly (R12): the check is review help, `main` is owner-merged.
5. **The release or pin goes wrong.** A bad `RUNNER_IMAGE_DIGEST` fails the tick guard (`factory-ci-tick.yml:42-44`) and every project stops. Prevention: set it only from the release workflow's printed digest; keep the old digest; canary before any image push.

## Invariants (qa turns these into property tests)

- I1. `publish` makes zero reads of `DOCKERHUB_TOKEN`, zero engine calls and zero API calls for a non-image project or a ref other than `main`.
- I2. For every Dockerfile and pipeline, `check` returns sorted unique ids from the fixed set and never panics.
- I3. A check that returns any id leads to zero push calls, whatever the pipeline did.
- I4. The token value and the base64 of `sudtanj:<token>` appear in no argv, no environment of the client or of any container, no status text, no log line; they appear only as the standard input of containers A and B.
- I5. `latest` is tagged only when the tip read in the same publish, twice, equals the commit.
- I6. The `sha-` tag is written only by `crane index append`; `latest` only by `crane tag` of it; no other tag is ever named (`sha-` plus exactly 12 hex).
- I7. `secrets_ok` and row `allow_docker` are true only for project `docker/*` on branch `main`, and only when the entry has `allow_docker`.
- I8. Prepare mode lists a name only when I7's condition and a clean check both hold, and never a name the pipeline asked for.
- I9. `find` never returns a path of another shape than `.`, `<folder>`, `docker/<name>`.
- I10. `verify_index` accepts a list only when it holds exactly one `linux/amd64` and one `linux/arm64` entry, for every permutation and mutation (drop, duplicate, add, relabel).
- I11. Container B never runs when `verify_index` rejected the list, a digest comparison failed, or either tip read differs from the commit.
- I12. Every publish container starts after `cleanup_run` returned and `run_left` is false, and mounts a directory with exactly two files, read-only.
- I13. `archive::read_config` never panics and never reads more than 1 MB of one member, for any byte input.

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
|---|---|
| Inputs | Handled. Folder names 1, 2, 64, 65 characters; Dockerfile empty, 1 MB, NUL, invalid UTF-8, CRLF, BOM, heredocs, comment `FROM`, stage named like an image, hostile directives, `ADD`, `ONBUILD`, `--from` an external image (`image_tests.rs`); tar garbage and truncation (`archive_tests.rs`); a registry answer that is not JSON or has extra entries (`verify_index`). Duplicates: the same row twice is the existing dedupe. |
| State | Handled. Repository must exist (owner creates it); no data; old runner ignores `docker/`. Crash mid-publish: stale pending plus the digest comparison. Partial failure: list written, `latest` fails (K3). Retry: reuse path with equal digests continues, unequal stops. |
| Time and concurrency | Handled. Ticks are serialised (`factory-ci-tick.yml:9-11`); tip read twice; each publish container has a 10-minute limit; the pipeline has the 30-minute run deadline. Clock: no new time logic. An older commit pushes `sha-` only. |
| Resources | Handled. Archives capped at 2 GiB each; publish containers `--memory 1g`, `--pids-limit 256`; the archives are moved, not copied; the directory is removed after publish; existing 10 MB log cap. Smallest machine: the GitHub VM, not the 1 GB planning VM. |
| Permissions and trust | Handled. The token reaches only the runner process and two publish containers on stdin, only for a passing `docker/*` row on `main`. A user step never gets it. Residual risk R12: a `main` build step controls a privileged sidecar and is effectively a token holder; `main` is owner-merged. |
| UI | N/A. Surfaces are fixed commit status text and the Docker Hub page. |

## Rollout and rollback

**Rollout, in order.**

1. Owner setup (quickstart 1, spec A14). Record the current digest: `gh variable get RUNNER_IMAGE_DIGEST`.
2. Merge the PR once the `factory-ci-selftest` result is green. The tick still runs the 0.3.0 runner: it does not see `docker/`, and `allow_docker` on the entry only moves the row routing, which the old runner treats as before. Check one tick.
3. Tag `rust-ci-runner-v0.4.0` on the merge commit. Wait for the release workflow and record its digest.
4. `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`. No YAML edit: the tick reads `vars.RUNNER_IMAGE_DIGEST` (`factory-ci-tick.yml:37,57,133`).
5. Canary: two ticks. `factory-ci/rust-ci-runner` and `factory-ci/ci-canary` post once and are not planned again (K2). Then `factory-ci/docker/hello-static` posts; run quickstart 5 and the post-publish check.

**Rollback.**

- Runner: `gh variable set RUNNER_IMAGE_DIGEST --body <the digest recorded in step 1>`. The next tick runs 0.3.0, discovers no `docker/` project, publishes nothing.
- Docker Hub: delete the published tags by hand if needed (nothing in the runner deletes them), and revoke the Hub token.
- Toml: revert the `allow_docker` line in a separate reviewed PR.
- Token: delete the Bitwarden key; every `main` image run then ends in `error` and pushes nothing.
- No data migration exists.

## Infrastructure assessment

Findings from the devops review and where this plan answers each.

| Finding | Answer |
|---|---|
| Branch exposure would put every branch of `rust-ci-runner` and `ci-canary` on the docker job (blocker) | `branches = ["main"]` stays; rows narrowed (S2, I7) |
| First push to a missing repository gives 401 | Owner pre-creates the public repository (A5, T025, FR-016) |
| Token in env or argv of the push | Standard input only (S4, S6, I4) |
| Docker Hub pull limit for the Go builder | `mirror.gcr.io/library/golang` by digest (A13, T001 checks the mirror serves the digest) |
| Environment and secrets of the docker path | Owner list in A14 and T025: Environment limited to `main`, `FACTORY_CI_BLOB_KEY_DOCKER` set, status token scope, same user for both tokens (K2) |
| Post-publish confidence | SC-007 and task T027: after a tick with a docker change, `sha-<12 of the main tip>` exists; no answer counts as unknown |
| Runner minutes | Each image builds twice inside the 30-minute run deadline; cross-compilation keeps both near native speed; only `docker/` rows use the docker job |

Not verifiable here: the Environment's protection rules, the status token's scope, the user behind each token. T025 lists them for the owner.

## Security assessment

Findings from the security review and where this plan answers each.

| Finding | Answer |
|---|---|
| Critical: a branch could run a privileged build and a token path | No branch is planned; rows narrowed; the static check runs as a test only (S2, S10) |
| High: the sidecar is privileged and shares a host with the token | Teardown and `run_left` before the publish containers; fresh directory with two archives, read-only; token on stdin (S4, S6, I12) |
| High: the published image is not the one the Dockerfile describes | `archive::read_config` checks user, entrypoint and platform of each archive before any push (S5, S6) |
| High: parser bypasses (`ADD`, `ONBUILD`, heredocs, external `--from`, directive spellings, unpinned step image) | Allowlist and rules with one test per bypass (S1) |
| Tag squatting and retries | `sha-<12 hex>`; existing list compared with the archives (K3) |
| Supply chain | Digest pins for every image (builder from a mirror, distroless, `docker:cli`, `crane`); refresh routine in `docker/README.md` |

**Residual risk, stated plainly.** Whoever controls a `main` build step can tamper with the host through the privileged docker sidecar before the runner removes it, and so is effectively a token holder. `main` is owner-merged. The check is review help, not a barrier. Rootless BuildKit is the future fix and is out of scope. The token is scoped Read & Write with an expiry (A14). One Docker Hub token covers the whole `sudtanj` namespace (R2).

## Open risks

- R-A the pipeline can still push any archive that passes the archive check; `main` is the owner's.
- R-B `docker save`, `crane push` by digest and `crane index append` compatibility is untested until T001 and T027.
- R-C `FACTORY_CI_TOKEN_DOCKER` scope, Environment protection and the two-user question are owner checks (K2).
- R-D overlap with 011 (K5).
- R-E a Docker ID that differs from the namespace makes login fail with `registry refused the token`.
- R-F pinned images (`crane:debug`, `docker:cli`, builders) freeze until someone bumps the digest.
