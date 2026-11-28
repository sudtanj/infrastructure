# Implementation Plan: Public Docker images from `docker/`

**Branch**: `012-docker-images` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/012-docker-images/spec.md`

## Blockers and owner decisions (read first)

0. **Spec change applied (owner, 2026-11-28).** Multi-arch moved into scope: FR-012 rewritten, FR-026 to FR-028, SC-006, A10, A11, R10, R11 added to `spec.md`. arm64 is cross-compiled with no emulation (research D10). Limit: a Dockerfile that must run target-arch code is refused by check id `emulation`; QEMU binfmt from the privileged sidecar would register in the VM kernel's global table, works on a hosted runner in principle, costs a pinned third-party root image and slow builds, and is left to a later spec.
1. **B1, the token path differs from the spec's words.** The runner cannot read Bitwarden itself on the tick. `.github/scripts/factory-ci-job.sh:115` unsets `BWS_ACCESS_TOKEN`, `FACTORY_CI_BWS_PROJECT` and `FACTORY_CI_BWS_SERVER_URL` before it `exec`s the runner (line 117), and the YAML cannot change (research D1). The plan uses the wrapper's own names-file mechanism (`job.sh:60-101`): the runner lists `DOCKERHUB_TOKEN` in prepare mode, the wrapper fetches it and exports it to the runner, the built-in publish step reads it from its environment. No YAML or script change. The owner must accept this reading of "the runner reads it"; without that acceptance the feature has no route and stops.
2. **O1, FR-015 wording.** The publish step runs a `crane` container with the token in its environment. The plan reads "step" in FR-015 as a user-written pipeline step.
3. **O2, one secret, no username key.** `DOCKERHUB_TOKEN` only. The login name is the constant `sudtanj`.
4. **O3, 011 overlap.** `walk.rs`, `plan.rs`, `job.rs` and `watch.rs` are also large diffs on branch `011-ci-state-queue`. 012 needs the tick (A8) and the names-file contract of `job.sh` as they are on `main` today. Whichever branch merges second rebases; the lead sequences it. If the amended 011 changes `factory-ci-job.sh` or the row fields, this plan's D1 must be re-read.
5. **O4, FR-022 test.** The path check needs git history, which the runner pipeline's mount does not have. It runs as a documented command (quickstart 3) and as a guarded test in `tests/repo_files.rs`, which the selftest runs. It is not a gate inside the `rust-ci-runner` pipeline step.

## Summary

Add `docker/<image>/` folders that the existing tick finds, builds in the per-run docker sidecar for `linux/amd64` and `linux/arm64`, checks and publishes to Docker Hub as one multi-arch manifest list per tag. All change is in Rust under `rust-ci-runner/`, toml files, `docker/` content and docs. Four runner changes carry it:

1. Discovery of `docker/<image>` projects (`projects.rs`, `job.rs` target check).
2. A check module `image.rs` that fails an unsafe Dockerfile or pipeline, run before the build and again inside publish. It refuses any `RUN` outside a `--platform=$BUILDPLATFORM` stage, because arm64 is built by cross-compiling, never by emulation.
3. A built-in publish step `publish.rs`: a digest-pinned `crane` container pushes the two `docker save` archives by digest, writes the manifest list to the `sha-<short>` tag in one registry write, and tags `latest` only when the commit is the tip of `main`. A pure `verify_index` function checks the list.
4. A walk rule so a branch-built commit that later becomes the `main` tip still publishes.

Toml: `allow_docker = true` and no `branches` key on the `sudtanj/infrastructure` entry. The runner goes `0.3.0` to `0.4.0`; the owner sets `RUNNER_IMAGE_DIGEST` after the release. That variable is the switch and the rollback.

## Technical Context

**Language/Version**: Rust 2021, toolchain 1.90 (`rust-ci-runner/rust-toolchain.toml`).

**Primary Dependencies**: none added. `std`, `serde`, `serde_json`, `toml`, `ureq`. No `regex`: token-shape scans are hand-written.

**Storage**: files only. `image-amd64.tar` and `image-arm64.tar` in the checkout, removed with the workspace.

**Testing**: `cargo test --locked` in the runner pipeline (`unit`, `integration` steps of `rust-ci-runner/.factory-ci.toml`); stub `docker` and fake GitHub from `tests/common/mod.rs`; a real publish once, by hand, against Docker Hub.

**Target Platform**: GitHub-hosted `ubuntu-24.04` VMs, the runner binary on the host, containers through the host engine.

**Project Type**: CLI runner plus repo content.

**Performance Goals**: push within 15 minutes of the merge (SC-001): one tick plus build.

**Constraints**: no change under `.github/workflows/` or `.github/scripts/`; no secret in any repo file; the VM for this planning has 1 GB RAM and runs no build.

**Scale/Scope**: one image now; one folder per later image, no change elsewhere (SC-005).

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Automation stays in `rust-ci-runner/` and `.github/`. The top-level layout bullet in "Repository Structure and Constraints" lists the folders; `docker/` is new content, so the bullet needs a PATCH amendment (task T022). |
| II Secrets source | Pass. The token lives in Bitwarden, read by the existing wrapper, in no repo file. No new Actions secret or variable (the README list does not grow). |
| III Nothing private in public logs | Pass. All statuses are fixed lines; the token is masked by the wrapper and redacted in the runner; crane output goes to `/dev/null`. |
| IV Simplicity | Pass with one justification in Complexity Tracking. |
| V Tested changes | Pass. FR-to-test map below. FR-022 is the one criterion not gated in the runner pipeline (O4). |
| VI Spec-driven | Pass. Every change below has a task. |

Re-checked after design: unchanged, plus the amendment task.

## Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| `walk.rs` flag `publishes` | `allow_docker` and `branches` make branch runs post `success`; a fast-forward to `main` would then never publish | Posting a different context on branches makes the branch tip look unplanned and rebuild every tick |
| `crane` image const | Pushes two `docker save` archives by digest and writes the manifest list without a hand-written registry client | A `ureq` registry client is about 300 lines plus tar and digest code (research D2) |
| `image.rs` Dockerfile parser | FR-009 to FR-011 need instructions, not text (heredocs, stages, continuations) | A line grep fails the spec's own edge cases |

## Architecture decision

**Chosen.** The tick stays the only trigger. The planner discovers `docker/<image>` as a project. The job clones, runs the check, runs the user pipeline (build and save) in the docker sidecar, then the runner itself publishes. The token reaches the runner process through the wrapper's names-file hand-off, only for a `main` image project whose check passed.

**Alternatives rejected.**

- Runner reads Bitwarden itself: impossible without a script edit (D1).
- Native Rust registry client: larger, new attack surface (D2).
- Push from the sidecar or host `docker login`: breaks FR-015.
- Check in the tick's plan phase: no file contents there without a new `Api` method (D5).
- A new per-project docker switch in the watch file: parser, plan and test cost; spec FR-023 names `allow_docker` (D3).
- Narrow `branches`: breaks branch names such as `012-docker-images` (D3).
- Version tags, signing, multi-arch: spec non-goals.

## Files to add or change

All paths relative to the repo root. No path under `.github/workflows/` or `.github/scripts/`.

| Path | Change |
|---|---|
| `rust-ci-runner/src/image.rs` | NEW. Constants, `image_project`, `publishes`, `name_ok`, Dockerfile parser, `check`. |
| `rust-ci-runner/src/image_tests.rs` | NEW. Unit tests for every check id and edge. |
| `rust-ci-runner/src/publish.rs` | NEW. `publish`: preconditions, container A, tip check, container B. |
| `rust-ci-runner/src/publish_tests.rs` | NEW. Unit tests with a stub engine script and a fake `Api`. |
| `rust-ci-runner/src/lib.rs` | `pub mod image; pub mod publish;` and the two test modules. |
| `rust-ci-runner/src/projects.rs`, `projects_tests.rs` | `find`, `discover` for `docker/<name>`; `project_path_ok`. |
| `rust-ci-runner/src/walk.rs`, `walk_tests.rs` | `decide(..., publishes)`; marker rule at tip and on ancestors. |
| `rust-ci-runner/src/plan.rs`, `plan_tests.rs`, `secret_names_tests.rs` | pass `publishes`; `secrets_ok` from `image::publishes`; tests that said "every row false" now say "false except main image rows". |
| `rust-ci-runner/src/job.rs`, `job_tests.rs` | `target()` uses `project_path_ok`; `secret_names` writes the token name; `attempt` runs the check, calls `publish`, posts the not-published text; `token_values` adds the token and its docker-auth base64. |
| `rust-ci-runner/tests/common/mod.rs` | `Fake`: a `branches` answer; `STUB_` support for the crane calls. |
| `rust-ci-runner/tests/job_publish.rs` | NEW. Binary-level publish tests, run in the `integration` step. |
| `rust-ci-runner/tests/job_prepare.rs`, `job_rules.rs`, `no_vendor.rs` | Update expectations that the names file is always empty. |
| `rust-ci-runner/tests/repo_files.rs` | Guarded FR-022 path test; watch-entry test; hello-static layout test. |
| `rust-ci-runner/tests/fixtures/image/` | NEW. One passing project and one failing fixture per check id. |
| `rust-ci-runner/.factory-ci.toml` | Add `--test job_publish` to the integration command. |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | Version `0.4.0`. |
| `rust-ci-runner/README.md`, `README.md` | Image projects, the publish step, `DOCKERHUB_TOKEN`, the rollback. Last task. |
| `.github/factory-ci-repos.toml` | Entry `sudtanj/infrastructure`: add `allow_docker = true`, remove `branches`. |
| `docker/hello-static/{Dockerfile,hello.go,go.mod,README.md,.factory-ci.toml}` | NEW first image, cross-compiled Go, both platforms. |
| `docker/README.md` | NEW. How to add an image: three files, no other change. |
| `.specify/memory/constitution.md` | PATCH: add `docker/` to the layout bullet. |

## Data and API contracts

- [data-model.md](data-model.md): check ids, constants, tags, status texts, walk rule, state flow.
- [contracts/publish-step.md](contracts/publish-step.md): names-file hand-off, publish function, crane container and exit codes, redaction.
- [contracts/image-project.md](contracts/image-project.md): folder layout, pipeline and Dockerfile shapes.
- No HTTP API of ours changes. `Api` gains no method: the tip comes from the existing `branches`.

## Ordered steps and engineer briefs

Feature folder for every brief: `specs/012-docker-images/`. Task IDs are in `tasks.md`. Every brief carries: the repo is public, no secret value in any file, tests run only in the runner pipeline, no workflow or script edit, no new crate.

Parallel groups: **A** = S1, S2, S3 (disjoint files). **B** = S4 after S1. **C** = S5 after A and B. **D** = S6, S7, S8 after C, mutually parallel. **E** = S10, the owner steps and the release, after D. S9 (docs and description) is the last step, after S10 proves the feature.

### S1. Check module (group A)

- **Goal**: `image::check(dir) -> Vec<&'static str>` and the constants and helpers of `data-model.md`.
- **Files**: `rust-ci-runner/src/image.rs`, `image_tests.rs`, `lib.rs`, `tests/fixtures/image/`. `emulation`: a stage with `RUN` and no `--platform=$BUILDPLATFORM` fails, the same stage with the flag passes, `${BUILDPLATFORM}` passes, a `RUN`-free final stage needs no flag; `ARG TARGETARCH` passes `secret-name`.
- **Acceptance**: every id and edge answer of the data-model table has a test that fails without the rule; name boundaries 2 and 64 pass, 1 and 65 fail, `Hello`, `hello_static`, `-a` fail; CRLF, BOM, heredoc, `FROM` in a comment, `FROM alpine AS alpine`, stage chains, `--platform=$BUILDPLATFORM`, `USER $X`; a 1 MB Dockerfile, NUL bytes and invalid UTF-8 return `layout`, never panic; the passing fixture returns an empty list; output sorted and de-duplicated.
- **Constraints**: no regex crate; Dockerfile read capped at 256 KB and read with `symlink_metadata`; return only `&'static str` ids so no repo text reaches a status. Includes the `emulation` id (a stage with `RUN` must carry `--platform=$BUILDPLATFORM`) and the `PLATFORMS` constant. Includes the `emulation` id (a stage with `RUN` must carry `--platform=$BUILDPLATFORM`) and the `PLATFORMS` constant.

### S2. Discovery, walk marker, `secrets_ok` (group A)

- **Goal**: `docker/<name>` is a project the plan lists and the job accepts; a `NOT_PUBLISHED` status does not stop a `main` run; only `main` image rows carry `secrets_ok = true`.
- **Files**: `projects.rs`, `projects_tests.rs`, `walk.rs`, `walk_tests.rs`, `plan.rs`, `plan_tests.rs`, `secret_names_tests.rs`, `job.rs` (`target()` only).
- **Acceptance**: `find` returns `docker/x` for a folder with `.factory-ci.toml` or `Dockerfile`, ignores `docker/x/y/.factory-ci.toml`, a `docker` file, and a folder with neither; `valid` follows the FR-002 name rule; a truncated tree still finds `docker/x`; `owns("docker/x", "docker/x/Dockerfile", set)` true and `docker/xy/...` false; `decide(publishes=true)`: tip with a `NOT_PUBLISHED` success gives `Run{retry:false}`, an ancestor with it gives `Run{prev}`; `decide(publishes=false)` is unchanged on the same input; plan rows: `secrets_ok` true only for project `docker/*` on branch `main`; `target()` accepts `docker/hello-static` and still rejects `a/b`, `docker/Hello`, `../x`; `ci.rs` is untouched and still refuses `docker/x`.
- **Constraints**: `publishes` is `branch == image::PUBLISH_BRANCH`, so S2 needs only the constants from S1; coordinate by defining `PUBLISH_BRANCH`, `NOT_PUBLISHED`, `publishes` and `image_project` in S1's first commit and merge that before S2 compiles. If the two engineers work apart, S2 may declare them in `projects.rs` temporarily and S1 re-exports.

### S3. Watch entry and hello-static content (group A)

- **Goal**: the first image exists and the watch entry turns docker on.
- **Files**: `.github/factory-ci-repos.toml`, `docker/hello-static/*`, `docker/README.md`.
- **Acceptance**: the entry keeps `repo` and `id`, gains `allow_docker = true`, loses `branches`; `watch::parse` accepts the file; `hello-static` passes `image::check` (S1 test reads the real folder); the Dockerfile `FROM` digests are real (resolved with `docker buildx imagetools inspect`, recorded in a comment with the source tag); `hello.go` prints a fixed line and builds with `CGO_ENABLED=0 GOOS=linux GOARCH=$TARGETARCH` in a `--platform=$BUILDPLATFORM` stage; every pinned base is an index digest and `docker buildx imagetools inspect` lists `linux/amd64` and `linux/arm64` for each (FR-028, recorded in the task); the build step image is digest-pinned and builds and saves both platforms (contracts/image-project.md).
- **Constraints**: do not merge the toml line before S10 is ready to run in order (rollout); the file is safe to merge earlier because the 0.3.0 runner ignores `docker/` (rollout step 2). No token or username in any file.

### S4. Publish step (group B, after S1)

- **Goal**: `publish::publish` and `publish::verify_index` per `contracts/publish-step.md`.
- **Files**: `publish.rs`, `publish_tests.rs`, `lib.rs`.
- **Acceptance**: with a stub engine and a fake `Api`: branch not `main` returns `success` `built and checked, not published` with zero env reads, zero engine calls and zero API calls; missing or empty token gives `error` `publish refused: no token`; a missing, symlinked, empty or oversized archive (either platform) gives the archive lines and starts no container; an unreadable first tip gives `error` `tip of main unreadable` and starts no container (FR-018); container A runs once per publish and its argv has no platform tag, only `REPO` and `TAG` envs (no tag other than `sha-<short>` can be named); exit 41 gives `registry refused the token`, 44 gives `image platform mismatch`, 42, any other code or a timeout gives `push failed`; an `INDEX` line that `verify_index` rejects, or no `INDEX` line, gives `image index incomplete` and container B never runs; tip equal on both reads runs container B, tip different on the first read (A still runs) or on the second gives the `not latest` text or `tip of main unreadable`; the argv of both containers holds no token value, no `-e DOCKERHUB_TOKEN=`, only `-e DOCKERHUB_TOKEN`; the client env holds the token only for the two publish calls; container flags are exactly those of the contract; a repeat publish of a commit whose list exists ends in the same text as the first run. `verify_index` tests: both platforms (OCI and Docker list media types, either order) pass; one entry, three entries, two `amd64`, no `arm64`, an `unknown/unknown` entry, a single image manifest, malformed digest, missing `platform`, not JSON all fail.
- **Constraints**: use `Engine::capture` for container A and `Engine::run_logged` for B, built on `Engine::command`, no `Command::new`; the crane image const is a full digest pin (the test asserts the `@sha256:` shape); the uid and gid come from `MetadataExt` of the project directory; no token in any `format!` that reaches a `Done` text; the captured stdout is parsed and never logged; the exit mapping reads `exit status: <n>` and a test pins that text.

### S5. Wire the job (group C)

- **Goal**: `job.rs` runs the check, the pipeline and publish, and lists the token name.
- **Files**: `job.rs`, `job_tests.rs`.
- **Acceptance**: prepare mode: names file is `DOCKERHUB_TOKEN\n` only for `docker/<image>` + `main` + clean check; empty for a failing check, for a branch, for a non-image project; a pipeline listing a secret still gives `NO_JOB_SECRETS`; `attempt`: check ids fail the project with `failure` before the pipeline starts, and no container starts; a clean image project on a branch posts the not-published text; on `main` it calls `publish` after a passing pipeline, and not after a failing one; `creds` for the final post include the token and `base64("sudtanj:" + token)`; non-image projects behave as before (all existing job tests unchanged and green).
- **Constraints**: `publish` runs after `run_pipeline` and before `cleanup_run`; the check in `attempt` runs before `prepare()` so a missing `.factory-ci.toml` is a `layout` failure; do not log or `format!` the env value.

### S6. Integration tests (group D)

- **Goal**: binary-level proof with the stub engine and fake API.
- **Files**: `tests/job_publish.rs`, `tests/common/mod.rs`, `tests/job_prepare.rs`, `tests/job_rules.rs`, `tests/no_vendor.rs`, `rust-ci-runner/.factory-ci.toml`.
- **Acceptance**: the test list in "Test plan" below; `job_prepare.rs` and `job_rules.rs` keep their old assertions for non-image projects; the integration command in `.factory-ci.toml` names `job_publish`.
- **Constraints**: canary token `canary-Zq81-value-key-0123456789` only; the fixture archives are a few bytes of text; add a `Fake` answer for `branches` and a `STUB_` hook to make container A print an `INDEX` line.

### S7. Version, constitution (group D)

- **Goal**: `0.4.0` and the layout amendment.
- **Files**: `rust-ci-runner/Cargo.toml`, `Cargo.lock`, `.specify/memory/constitution.md`.
- **Acceptance**: `repo_files::cargo_toml_and_lock_versions_match` passes; the constitution layout bullet lists `docker/` with version `1.1.1` and an amended date, written through the `speckit-constitution` skill.

### S8. Repo-file tests (group D)

- **Goal**: the committed facts are tested.
- **Files**: `tests/repo_files.rs`.
- **Acceptance**: the infra entry has `allow_docker` and no `branches` and no secrets; every folder under `docker/` has the three files and a kebab name; the FR-022 test runs `git diff --name-only <base>...HEAD` with `<base>` from `FACTORY_CI_BASE` or `origin/main`, fails on any path under `.github/workflows/` or `.github/scripts/`, and prints a skip line (not a pass) when git or the base is unavailable.
- **Constraints**: stays out of the pipeline's integration command (the existing comment in `repo_files.rs` explains why).

### S9. Docs and repository description (last task, after S10)

- **Goal**: `README.md`, `rust-ci-runner/README.md` and the GitHub description match the feature (FR-025).
- **Files**: `README.md`, `rust-ci-runner/README.md`; the description by `gh repo edit sudtanj/infrastructure --description ...`.
- **Acceptance**: the root README says what `docker/` is and how to add an image; the runner README documents the publish step, the check ids, `DOCKERHUB_TOKEN` and the key name, the rollback, and amends the sentence that the tick hands no secret to a step (true for steps, relaxed for the built-in publish); no secret value; the description mentions Docker Hub images; `gh repo view --json description` shows it.
- **Constraints**: prose per `stop-slop`; no secret value.

### S10. Release and end to end (owner plus engineer, after group D)

- **Goal**: ship and prove.
- **Steps**: quickstart sections 1, 4, 5. The engineer does the tag; the owner holds the Bitwarden and variable steps.
- **Acceptance**: SC-001, SC-003 and SC-006 evidence (both platforms listed, equal digests, a pull on an amd64 and an arm64 host) recorded in `tasks.md` as in feature 010 T021.

## Test plan (qa runs this)

**Unit** (runner pipeline `unit` step):

| Area | Tests (file) |
|---|---|
| Check rules | one test per id and edge answer (`image_tests.rs`), including `check_never_panics_on_junk` over generated byte strings |
| Discovery | `find_docker_projects`, `discover_truncated_tree_reads_docker`, `owns_prefix_boundary` (`projects_tests.rs`) |
| Walk | `marker_at_tip_runs_when_publishes`, `marker_on_ancestor_runs_when_publishes`, `marker_ignored_when_not_publishes` (`walk_tests.rs`) |
| Plan | `secrets_ok_true_only_for_main_image_rows`, existing "all false" tests narrowed (`plan_tests.rs`, `secret_names_tests.rs`) |
| Publish | the S4 list, including `verify_index` over generated permutations and mutations of a two-platform list (`publish_tests.rs`) |
| Job | prepare names table, attempt flow, redaction (`job_tests.rs`) |
| Names | `dockerhub_token_is_a_valid_secret_name` against `valid_secret_name` |

**Integration** (`integration` step, `tests/job_publish.rs`, stub docker, fake GitHub):

| Test | FR |
|---|---|
| `main_image_publishes_both_tags` | FR-014, FR-017 |
| `branch_run_reads_no_token_and_pushes_nothing` (token env absent, no crane call, status text) | FR-013, SC-003 |
| `failed_check_pushes_nothing` per id sampled, no container starts | FR-008, SC-002 |
| `omitted_check_step_still_refused_in_publish` (a build step rewrites the Dockerfile) | FR-008 |
| `pipeline_with_secrets_fails_check` | FR-011 |
| `canary_token_absent_from_public_lines_argv_envs_status` over every output | FR-015, SC-003 |
| `failed_build_pushes_nothing` (one archive absent, or a step fails) | FR-026, FR-007 |
| `older_commit_pushes_sha_only` (fake `branches` answers another sha on the first read, then on the second) | FR-018 |
| `unreadable_tip_pushes_nothing` (first read) and `second_tip_read_fails_leaves_sha_tag` | FR-018 |
| `repeat_publish_reuses_list` (the list exists, no push call, same text) | FR-020 |
| `list_has_both_platforms` (`INDEX` line accepted) and `single_arch_or_bad_index_never_tags` (container B never runs) | FR-026 |
| `no_tag_other_than_sha_and_latest_is_ever_named` over every engine argv and env | FR-017, FR-026 |
| `deleted_folder_makes_no_call` (plan level) | FR-019 |
| `unchanged_commit_builds_nothing` (plan level, existing walk) | FR-005 |
| `user_step_env_never_holds_token` (stub envs file) | FR-015, FR-024 |

**Real end to end** (quickstart 5): once, on `hello-static`, SC-001 and SC-003, then the branch and fast-forward cases.

**Not automated**: SC-005 (second image, a month later); FR-022 inside the runner pipeline (O4).

## Failure-mode table

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| Discovery | `docker/` too deep in a truncated tree | `discover` | extra tree read finds it | `discover_truncated_tree_reads_docker` |
| Check | Dockerfile unreadable, huge, not UTF-8 | `image::check` | `layout`, `failure` | `check_never_panics_on_junk` |
| Prepare | check fails | `secret_names` | empty names file, real run reports `failure` | prepare names table |
| Token fetch | key missing, ambiguous, store down | wrapper (`job.sh:78-86`) | `error` with the wrapper's fixed line | `.github/scripts` tests exist already; E2E |
| Build | `docker build` fails, disk full | step exit | `failure` or `error` (125) as today | existing |
| Archive | either platform archive missing, symlink, too big | `publish` | `error`, no push | publish tests |
| Token | empty in env | `publish` | `error` `publish refused: no token` | publish tests |
| Push A | 401 or 403 | exit 41 | `error` `registry refused the token` | publish tests |
| Push A | network, rate limit | exit 42, timeout | `error` `push failed`, one retry after 30 minutes | publish tests |
| Push A | pushed image has the wrong `os` or `architecture` (mislabeled build) | `crane config` check, exit 44 | `error` `image platform mismatch`, no tag written | publish tests |
| Index | list missing a platform or malformed | `verify_index` | `error` `image index incomplete`, `latest` untouched | `single_arch_or_bad_index_never_tags` |
| Build | one platform's build fails (base is single-arch, `exec format error`) | step exit | `failure`, no archive pair, nothing pushed | integration `failed_build_pushes_nothing` |
| Partial | sha list pushed, `latest` tag fails | exit of B | `error`; retry finds the sha tag, tags `latest` | `repeat_publish_reuses_list` |
| Tip | `branches` fails or no `main` | `publish` | first read: `error`, nothing pushed; second read: `error`, sha tag stays | `unreadable_tip_pushes_nothing`, `second_tip_read_fails_leaves_sha_tag` |
| Tip | newer commit exists | `sha != tip` | `success` `published sha-<short>, not latest` | `older_commit_pushes_sha_only` |
| Status post | GitHub down | existing `post` | exit 1, wrapper fallback | existing |
| Crash mid-publish | VM dies | stale `pending` after 2 h | existing retry rule; re-publish is idempotent | `repeat_publish_reuses_list` |

## Pre-mortem

Assume it shipped and broke. Likeliest five.

1. **`crane` rejects the `docker save` tarball, refuses a push by digest, or `crane:debug` has no `sh`; or the pinned base is not an index so one platform fails.** Prevention: the real E2E (S10) runs before anything depends on it; the research spike lists the three checks and the OCI-export fallback; the digests of the bases are checked with `imagetools inspect` in S3 (FR-028); a push-by-digest refusal goes back to the lead because the fallback breaks FR-017.
2. **The branch-built commit never publishes after a fast-forward.** Prevention: the `publishes` walk rule and three tests (D4).
3. **The token leaks.** Prevention: `-e NAME` only, `env_clear` in `Engine::command`, `ship = false`, crane output to `/dev/null`, redactor holds the token and its docker-auth base64, canary test over every output; branch runs never list the name, so the wrapper never fetches it.
4. **Turning on `allow_docker` and `branches = *` breaks the repo's own CI.** Likely failure: `FACTORY_CI_TOKEN_DOCKER` missing or unable to write statuses, so `plan_a` drops the repo (`main.rs:100-120`), or the environment `factory-ci-docker` blocks. Prevention: owner check in quickstart 1.3 before the digest moves; the toml is safe to merge early because the old runner behaves the same on non-docker projects; rollback is the digest variable plus a one-line toml revert.
5. **The release or pin goes wrong.** `RUNNER_IMAGE_DIGEST` set to an unreleased or wrong digest fails the tick guard (`factory-ci-tick.yml:42-44`) and every project stops. Prevention: set it only from the release workflow's printed digest; keep the 0.3.0 digest as rollback; the canary tick before any image push (as 010 T021).

## Invariants (qa turns these into property tests)

- I1. `publish` makes zero reads of `DOCKERHUB_TOKEN` and zero engine calls when the branch is not `main`, for any project, sha and env.
- I2. For every input Dockerfile and pipeline, `check` returns sorted unique ids from the fixed set and never panics.
- I3. A check that returns any id leads to zero push calls, whatever the pipeline did.
- I4. The token value, and the base64 of `sudtanj:<token>`, appear in no argv, no engine environment except container A and B calls, no status text, no log line.
- I5. `latest` is tagged only when the tip read in the same publish equals the commit.
- I6. Running `publish` twice for one commit leaves one digest behind `sha-<short>`.
- I7. `secrets_ok` is true only for project `docker/*` on branch `main`.
- I8. Prepare mode lists a name only when I7's condition and a clean check both hold, and never lists a name the pipeline asked for.
- I9. `find` never returns a path with more than the shapes `.`, `<folder>`, `docker/<name>`.
- I10. `decide(publishes=false)` equals the old `decide` for every input.
- I11. No engine argv or env ever names a tag other than `sha-<short>` and `latest`; the `sha-` tag is written only by `crane index append`; `latest` only by `crane tag` of the `sha-` tag.
- I12. `verify_index` accepts a list only when it holds exactly one `linux/amd64` and one `linux/arm64` entry, for every generated permutation and every mutation (drop, duplicate, add, relabel).
- I13. Container B never runs when `verify_index` rejected the list, a platform check failed, or either tip read differs from the commit.

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
|---|---|
| Inputs | Handled. Folder names 1, 2, 64, 65 characters; Dockerfile empty, 1 MB, NUL, invalid UTF-8, CRLF, BOM, heredoc, comment `FROM`, stage named like an image (`image_tests.rs`). Duplicate: the same project twice in a tree is impossible; the same row twice is the existing dedupe. Unicode in a folder name fails the kebab rule. |
| State | Handled. First push creates the repo (FR-016, crane push). Old data: the 0.3.0 runner ignores `docker/`. Crash mid-publish: stale pending plus idempotent re-publish. Partial failure: sha pushed, tag failed, the retry tags it. Retry safe: sha-tag existence check. |
| Time and concurrency | Handled. Ticks are serialised (`factory-ci-tick.yml:9-11`); tip re-read right before the tag; every container run has a 10-minute limit; the pipeline has the 30-minute run deadline (`job.rs:109`). Clock: no new time logic. Late events: an older commit pushes `sha-` only. |
| Resources | Handled. Archive capped at 4 GiB; publish container `--memory 1g`, `--pids-limit 256`; no new log growth (existing 10 MB cap, `MAX_LOG_BYTES`); the workspace is removed (`remove_ws`). Smallest machine: the GitHub VM, not the 1 GB dev VM; no build runs on the dev VM. |
| Permissions and trust | Handled. The token reaches only the runner process and two publish containers, only for `main` and a clean check. A user step never gets it. Open risk R-A: a `main` step could push an archive that is not the Dockerfile's image; `main` is the owner's. The walk marker is public text, not a secret. |
| UI | N/A, no UI. Surfaces are commit status text (fixed, tested) and the Docker Hub page. |

## Rollout and rollback

**Rollout, in order.**

1. Owner setup (quickstart 1). Record the old digest: `gh variable get RUNNER_IMAGE_DIGEST`.
2. Merge the PR. The tick still runs the 0.3.0 runner; it does not see `docker/` and plans the repo's other projects as before. The toml change moves them to the docker path with the old runner, which is the first real test of `allow_docker` and `branches`. Check one tick before step 3.
3. Tag `rust-ci-runner-v0.4.0` on the merge commit. Wait for the release workflow and record the digest it prints.
4. `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>` with the recorded digest. No YAML edit: the tick reads `vars.RUNNER_IMAGE_DIGEST` (`factory-ci-tick.yml:37,57,133`).
5. Canary: the first tick must still plan and post `factory-ci/rust-ci-runner` and `factory-ci/ci-canary`. Then `factory-ci/docker/hello-static` posts; run quickstart 5.

**Rollback.**

- Runner: `gh variable set RUNNER_IMAGE_DIGEST --body <the digest recorded in rollout step 1>`. The next tick runs the 0.3.0 runner, discovers no `docker/` project and publishes nothing. Published tags stay on Docker Hub (nothing deletes them).
- Toml: revert the one entry change (`allow_docker`, `branches`) in a reviewed commit if the repo's own projects misbehave.
- Token: delete the Bitwarden key. Without it every `main` image run ends in `error` with `secret DOCKERHUB_TOKEN not found` and pushes nothing.
- No data migration exists.

## Infrastructure assessment

The lead handed no devops assessment. The EM assessment, from the code:

- No new workflow, Environment, Actions secret or variable. One new Bitwarden key, one existing variable changes value.
- Runner-minute cost: `branches` default plus `allow_docker` runs the two long `rust-ci-runner` steps on every branch push of this repo, on the docker job (max 45 minutes, 4 parallel, 20 rows per tick).
- The publish container pulls `crane` from `gcr.io` on each run, about 10 MB. Each image builds twice (one `docker build` per platform) in the 30-minute run deadline; cross-compilation keeps both near native speed, emulation would not.
- Docker Hub rate limits apply to the base images in the sidecar and to pushes (R7); `error` plus one retry covers it.
- If the Environment `factory-ci-docker` has required reviewers, each docker tick waits. Not verified; the owner checks (quickstart 1.3).

## Security assessment

The lead handed no security assessment. The EM assessment:

- Trust boundary: a Dockerfile and a pipeline from `main` are trusted to the owner's level; branches are untrusted and never receive the token. The prepare run holds `env -i`.
- The token is in one process environment for the run. `Engine::command` clears child environments, and the wrapper masks the value on the Actions log.
- Check hardening beyond the spec: `# syntax=` refused, `final-shell` and `final-user` follow stage chains.
- Residual: the check is a heuristic (R5); a main-branch step can push an arbitrary archive (R-A, mitigation: owner-only merges and protected `main`); the crane image is a third-party supply chain pinned by digest; a token scoped to the whole `sudtanj` namespace (R2).
- The sidecar stays privileged for the build. The token is never in it.

## Open risks

- R-A step image pin and archive provenance are not enforced.
- R-B the `docker save` and `crane` compatibility is untested until S10.
- R-C `FACTORY_CI_TOKEN_DOCKER` scope and Environment protection are not verifiable from the repo.
- R-D overlap with 011 (O3).
- R-E a Docker ID that differs from the namespace makes login fail (O2).
- R-F `crane:debug` tag content changes are frozen by the digest; bumping it is a code change.
