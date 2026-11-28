# Implementation Plan: Public Docker images from `docker/`

**Branch**: `012-docker-images` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/012-docker-images/spec.md`

## Decisions fixed by the lead (2026-11-28) and what stays open

Decided, folded into spec, plan, tasks and contracts:

1. **Token route.** The runner cannot read Bitwarden on the tick: `.github/scripts/factory-ci-job.sh:115` unsets the store token and project before it `exec`s the runner (line 117), and the script is frozen. The frozen script fetches the token through its names-file mechanism (`job.sh:60-101`), only for a passing `docker/<image>` row on `main`. Only the runner and the push container receive it. Spec clarification and A11 say so.
2. **Branch exposure.** `branches = ["main"]` stays. The plan gives a row `allow_docker` only for `docker/<image>` on `main` (`plan.rs:307` today copies the entry flag to every row). `rust-ci-runner` and `ci-canary` stay in the plain job and `docker_problems(false)` still refuses `docker = true` for them. The `walk.rs` marker rule is dropped. Other branches get the static check as a Rust test (`tests/repo_files.rs`).
3. **Token handling.** Tear the sidecar down before publish; the token goes to `crane` on standard input, never env or argv; a fresh read-only directory holds only the two archives; publish verifies each archive's config first.
4. **Check hardening.** Instruction allowlist, heredoc and `--from` rules, case-insensitive directives, pinned pipeline step images.
5. **Tags.** `sha-<12 hex>`. A valid existing `sha-` list is reused and only `latest` is tagged (see decision 12; it replaces the digest comparison first decided here).
6. **Supply chain.** Go builder from `mirror.gcr.io/library/golang:<supported>@sha256:<index digest>`, `GOTOOLCHAIN=local`.
7. **Owner setup.** The two keys `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` already sit in the Bitwarden project `FACTORY_CI_BWS_PROJECT_DOCKER` names, as shared names. The owner confirms the names, the token scope, the status token and the Environment (spec A14, T028). The owner no longer creates the repository.
8. **Shared names (owner change).** The entry lists `secrets = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]` so the frozen script's shared route resolves them (`factory-ci-secrets.sh:69-80`; the row's `secrets` field is `entry.secrets.join(" ")`, `plan.rs:311`). 010's repo-files test that the entry has no `secrets` changes to allow exactly these two names. The entry has no `secret_branches`, so the manual `ci` path unlocks nothing (`watch.rs:39-43`, `ci.rs:224-231`, `ci.rs:385-391`); a test pins it. The username is not a secret to the runner: never in a redactor, no length floor, compared with the namespace `sudtanj` and used for the login. The script's floor is 4 characters (`factory-ci-secrets.sh:103-110`), and `sudtanj` has 7; the runner's 8-character floor (`store.rs:175-189`) applies to the `ci` path only and is not on this route. The script does mask the username in the Actions log (`***` for `sudtanj` in later lines of that job); statuses are unaffected.
9. **Repository creation (owner change).** A missing `sudtanj/<image>` is created public by the runner through the Docker Hub API (`POST /v2/auth/token`, then `POST /v2/repositories/`) with `ureq`, already a dependency; the token stays in runner memory, in no argv or env of a child; an existing private repository stops the item with a fixed line. T002 checks the API accepts a Read & Write token; a broader scope is an owner item.

Folded in after the critic review and the qa test-first commit `5385699`:

10. **Watch-entry edit is a separate follow-up PR (MAJOR-1).** Under the pinned 0.3.0 runner `plan.rs:307` copies the entry's `allow_docker` to every row and `plan.rs:489-490` partitions `matrix_docker` on it, so `rust-ci-runner` and `ci-canary` would move into `run-docker` (Environment `factory-ci-docker`, the `_DOCKER` store token) when the toml merges; `main.rs:115-118` drops the whole entry when `FACTORY_CI_TOKEN_DOCKER` is unset; `plan.rs:185` and `:278` read and post statuses with the docker token (K2). The edit (T020) therefore merges only after T029 pinned 0.4.0 and after the owner checks (T028). The main PR leaves the entry unchanged, and the repo-files test accepts both shapes.
11. **A failed step is `error` for image rows (MAJOR-2).** `job.rs:555-560` maps a failed step to `failure` and only exit 125 or a spawn failure to `error`; `walk.rs:96-107` retries only `error`. A rate limit or network error during the base pulls inside the sidecar would be final. For `docker/<image>` rows a failed step and a timeout map to `error` (FR-007); deterministic mistakes already fail earlier as `failure` at the static check. Other projects keep `failure`.
12. **A valid existing `sha-` list is reused (MAJOR-3).** `docker build` plus `docker save` records the build time in the image config, so a rebuild never matches the existing digests and a digest comparison would fail every retry. When the existing list passes `verify_index`, the new archives pass the archive checks and the tip still equals the commit, nothing is pushed or overwritten; only `latest` is tagged (FR-020). This replaces the digest comparison of lead decision 4.
13. **Explicit entrypoint (MAJOR-4).** `crane:debug` has a non-shell entrypoint, so the publish containers run `--entrypoint <CRANE_SH> <CRANE_IMAGE> -c '<script>'` as `engine.rs:382` does for the clone container, and the scripts call crane by `CRANE_BIN`. T002 confirms both paths in the pinned image; a test pins the argv order.
14. **Hub test override only in debug builds (MINOR-2).** `ensure_public` takes the base as an argument; `job.rs` reads `FACTORY_CI_HUB_URL` only under `#[cfg(debug_assertions)]`, so the release binary never reads it.
15. **Shared deadline (MINOR-5).** Clone (10 minutes, `job.rs:492`), pipeline (30 minutes, `job.rs:109`) and publish (two containers of 10 minutes plus four Hub calls) could add up past the 45-minute job limit (`factory-ci-tick.yml:215`). `attempt` starts one deadline of 40 minutes (`JOB_DEADLINE`); clone, pipeline and each publish call use `min(own limit, time left)`; too little left ends `publish refused: time ran out` (FR-033).
16. **Small hardening (MINOR-5).** The `:` check of `mount_arg` (`engine.rs:388-393`) applies to the fresh publish mount; `run_left` fails closed (any engine error counts as "left"); `plan_hardening_tests.rs:89` asserts `!r.secrets_ok` for its fixture, which has no `docker/` project, and must keep none.
17. **Serialised ticks (R4, FR-018).** `factory-ci-tick.yml:9-11` sets `concurrency: factory-ci-tick` with `cancel-in-progress: false`, so ticks never overlap; the tip read before the container and again before the tag covers the rest. FR-013 and SC-003 speak of credentials exported to the runner, because `job.sh:75-76` loads the docker project into a shell variable whenever the store token is set. FR-015's "never to a sidecar" covers what the runner hands over; a host escape from the privileged sidecar is R12.

Plan ambiguities settled for qa (data-model.md has the details):

- **QA-1** The archive policy errors (`image runs as root`, `image has a shell entrypoint`, `image platform mismatch`) are applied by `publish` (S6). `archive::read_config` returns what it finds and fails only with `archive unreadable`.
- **QA-2** `ImageConfig` has `os`, `architecture`, `user` as `String`, `entrypoint`, `cmd` as `Vec<String>`, and derives `Debug`.
- **QA-3** `Engine::capture_in` takes the input as `&[u8]`.
- **QA-4** `run_left` fails closed: it returns true on any engine error.
- **QA-5** The `name` check id is enforced by `image::name_ok` on the folder name, outside `check(dir)`; `check` never returns it.
- **QA-6** A `final-base` chain deeper than 20 fails; exactly 20 passes.
- **QA-7** Size limits are inclusive: a 1 MiB archive member and a 256 KiB Dockerfile pass; one byte more fails.
- **QA-8** The Hub login endpoint stays pending T002 (`/v2/auth/token` first, `/v2/users/login` as the fallback).
- **QA-9** The roughly 30-second Hub timeout test stays, in its own `cargo test --lib hub_tests::` command (and `publish_tests::` likewise) because of the step container's `--pids-limit 512`; its fake Hub ends its accept thread on drop like `ci_tests`.
- **QA-10** The engine tests live in the sibling file `rust-ci-runner/src/engine_tests.rs`.

Still open, none of them blocks starting work:

- **K1, push by digest is unverified.** The publish writes platform images to `repo@sha256:<digest>` so that no extra tag exists. If Docker Hub refuses, the fallback is a temporary tag, which breaks FR-017. T002 checks it with a throwaway repository; a refusal goes back to the lead.
- **K2, two tokens, one user.** The entry is now mixed: `plan.rs` reads statuses and posts `unchanged` and `invalid` with the docker token, while plain rows post `pending` and the final status with the plain token (`plan.rs:278` for `unchanged` and `invalid`, `plan.rs:455` for `pending`, field `Status.docker`). `walk::own` only trusts statuses made by the API user it read (`walk.rs:67`). If `FACTORY_CI_TOKEN` and `FACTORY_CI_TOKEN_DOCKER` belong to different GitHub users, every plain row of this repo is planned again every tick. T028 has the owner confirm one user; T029 watches two ticks.
- **K3, retry after a written list (resolved, decision 12).** A rebuild never matches the existing digests because the image config records the build time, so a digest comparison would fail every retry. A retry reuses a valid existing list and only tags `latest`; nothing is overwritten.
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
5. `hub.rs`: a Docker Hub API client on `ureq` that logs in and creates a missing public repository (never changes an existing one).
6. `publish.rs`: after the pipeline, tear the sidecar down, verify both archives, read the tip of `main`, ensure the repository, push both platform images by digest and write the manifest list in one registry write from a `crane` container that receives the token on standard input, check the list with `verify_index`, tag `latest` only when the commit is still the tip.

Toml, in a separate follow-up PR that merges after the 0.4.0 runner is pinned and the owner checks are done (decision 10): `allow_docker = true` and `secrets = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]` on the `sudtanj/infrastructure` entry; `branches` unchanged. Runner `0.3.0` to `0.4.0`; the owner sets `RUNNER_IMAGE_DIGEST` after the release. That variable is the switch and, with the toml revert, the rollback.

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
| I One home | Pass. Automation stays in `rust-ci-runner/` and `.github/`. The layout bullet of "Repository Structure and Constraints" gets `docker/` by a PATCH amendment (T026). |
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
| `hub.rs` Docker Hub client | The owner asked for automatic creation of a missing public repository; the runner has `ureq` and a token in memory | A `curl` container needs a pinned image and a shell script and puts the same token on standard input; no gain |
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
| `rust-ci-runner/src/hub.rs`, `hub_tests.rs` | NEW. `ensure_public` over `ureq`; `tiny_http` fake. |
| `rust-ci-runner/src/publish.rs`, `publish_tests.rs` | NEW. `publish`, `verify_index`; stub engine, fake `Api`, fake Hub. |
| `rust-ci-runner/src/engine.rs`, `engine_tests.rs` | `capture_in(args, env, input: &[u8], limit, container)`; `capture` calls it with no input; `run_left(run_id)` fails closed. Tests in the sibling `engine_tests.rs`. |
| `rust-ci-runner/src/lib.rs` | `pub mod image; pub mod archive; pub mod hub; pub mod publish;` and test modules. |
| `rust-ci-runner/src/projects.rs`, `projects_tests.rs` | `find`, `discover` for `docker/<name>`; `project_path_ok`. |
| `rust-ci-runner/src/plan.rs`, `plan_tests.rs`, `secret_names_tests.rs`, `plan_hardening_tests.rs` | Row `allow_docker` and `secrets_ok` from `image::publishes`; tests. `walk.rs` does not change. `plan_hardening_tests.rs:89` asserts `!r.secrets_ok` for a fixture with only a `.` project: keep it free of a `docker/` project. |
| `rust-ci-runner/src/job.rs`, `job_tests.rs` | `target()` uses `project_path_ok`; `secret_names` writes the two names; `attempt` starts the shared deadline, runs the check, maps a failed step or a timeout of an image row to `error`, calls `publish`; `token_values` adds the token and its docker-auth base64; the test override `FACTORY_CI_HUB_URL` is read under `#[cfg(debug_assertions)]` only. |
| `rust-ci-runner/tests/common/mod.rs` | `Fake` answers `branches`; a `STUB_` hook for the crane output and stdin capture. |
| `rust-ci-runner/tests/job_publish.rs` | NEW. Binary-level publish tests. |
| `rust-ci-runner/tests/job_prepare.rs`, `job_rules.rs`, `no_vendor.rs` | Update "names file always empty" expectations. |
| `rust-ci-runner/tests/repo_files.rs` | Static check over every `docker/` folder (FR-006); the entry test that said "no secrets" now allows exactly the two names and requires no `secret_branches`; guarded FR-022 path test. |
| `rust-ci-runner/src/ci_tests.rs` | One test: an entry with `secrets` and no `secret_branches` gives a `ci` run no step secret on any ref. |
| `rust-ci-runner/tests/fixtures/image/` | NEW. Passing project and one failing fixture per rule and bypass. |
| `rust-ci-runner/.factory-ci.toml` | Unit step: `hub_tests::` and `publish_tests::` run as their own `cargo test --locked --lib` commands (the first command skips them), because the step container has `--pids-limit 512` and the fake servers start threads; integration command gains `--test job_publish`. |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | Version `0.4.0`. |
| `rust-ci-runner/README.md`, `README.md` | Image projects, publish step, key name, residual risk, rollback. Last task. |
| `.github/factory-ci-repos.toml` | Follow-up PR, after the 0.4.0 runner is pinned and T028 is done: `sudtanj/infrastructure` gains `allow_docker = true` and `secrets = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]`; `branches` stays, no `secret_branches`. |
| `docker/hello-static/{Dockerfile,hello.go,go.mod,README.md,.factory-ci.toml}`, `docker/README.md` | NEW first image and the refresh routine. |
| `.specify/memory/constitution.md` | PATCH: add `docker/` to the layout bullet. |

## Data and API contracts

- [data-model.md](data-model.md): check ids, constants, tags, manifest list, status texts, state flow.
- [contracts/publish-step.md](contracts/publish-step.md): names-file hand-off, publish function, teardown, fresh directory, crane containers and stdin protocol, exit codes, redaction.
- [contracts/image-project.md](contracts/image-project.md): folder, pipeline and Dockerfile shapes.
- No HTTP API of ours changes. `Api` gains no method; the tip comes from `branches`.

## Ordered steps and engineer briefs

Feature folder for every brief: `specs/012-docker-images/`. Task IDs are in `tasks.md`. Every brief carries: the repo is public, no secret value in any file, tests run only in the runner pipeline, no workflow or script edit, no new crate.

Parallel groups: **A** = S1, S2, S4, S5, S5a (disjoint files). **B** = S6 after S1, S4, S5, S5a. **C** = S7 after A and B. **D** = S3, S8, S9, S10 after C. **E** = S12, owner steps and release, after D. S11 (docs) last, after S12 proves the feature.

### S1. Check module (group A)

- **Goal**: `image::check(dir) -> Vec<&'static str>` and the constants of `data-model.md`.
- **Files**: `image.rs`, `image_tests.rs`, `lib.rs`, `tests/fixtures/image/`.
- **Acceptance**: every id and edge answer of the data-model table has a test that fails without the rule. Name boundaries 2 and 64 pass, 1 and 65 fail. Each bypass has its own test: `ADD`, `ONBUILD`, `SHELL`, `HEALTHCHECK`, `VOLUME` (`instruction`); `<<` in `ENV`, `ARG`, `LABEL`, `FROM` (`heredoc`) while `RUN` and `COPY` heredoc bodies are skipped and a `FROM` inside one is ignored; `COPY --from=alpine`, `--from=docker.io/x`, `--from=5` (index not earlier), `RUN --mount=type=bind,from=alpine,target=/x` (`external-source`) while `--from=build` and `--from=0` pass; directives `# syntax=x`, `#syntax = x`, `# SYNTAX=x`, `# Escape=\``, `#   syntax=x` (`directive`); a pipeline step `image = "alpine"`, `image = "alpine:3.23"` (`step-image`). Plus CRLF, BOM, comment-`FROM`, `FROM alpine AS alpine`, stage chains, `--platform=$BUILDPLATFORM`, `emulation` with and without the flag, `USER $X`, `ARG TARGETARCH`. A 1 MB Dockerfile, NUL and invalid UTF-8 return `layout`, never panic. Passing fixture returns empty. Output sorted and unique.
- **Constraints**: no regex crate; Dockerfile capped at 256 KiB, inclusive (262144 bytes pass, one more is `layout`), read with `symlink_metadata`; only `&'static str` ids leave the module; the allowlist is `FROM ARG ENV WORKDIR COPY RUN USER ENTRYPOINT CMD LABEL EXPOSE`; the `name` rule is `image::name_ok` on the folder name, outside `check(dir)` (QA-5), and `check` never returns `name`; a `final-base` chain of 21 links fails, 20 passes (QA-6).

### S2. Discovery and plan rows (group A)

- **Goal**: `docker/<name>` is a project the plan lists and the job accepts; only `docker/<name>` rows on `main` get `allow_docker` and `secrets_ok`.
- **Files**: `projects.rs`, `projects_tests.rs`, `plan.rs`, `plan_tests.rs`, `secret_names_tests.rs`, `job.rs` (`target()` only), `job_tests.rs`.
- **Acceptance**: `find` returns `docker/x` for a folder with `.factory-ci.toml` or `Dockerfile`, ignores `docker/x/y/.factory-ci.toml`, a `docker` file and a folder with neither; `valid` follows the FR-002 name rule; a truncated tree still finds it; `owns("docker/x", "docker/x/Dockerfile", set)` true, `docker/xy/...` false; with the entry `allow_docker = true`, a `docker/x` row on `main` has `allow_docker` and `secrets_ok` true, a `rust-ci-runner` row on `main` has both false and lands in `Final.matrix`, a `docker/x` row on another branch has both false; `target()` accepts `docker/hello-static`, rejects `a/b`, `docker/Hello`, `../x`; `ci.rs` unchanged and a test shows `ci` still refuses `docker/x`; `walk.rs` unchanged.
- **Constraints**: take `publishes` and `image_project` from S1's first commit; merge that first. `plan_hardening_tests.rs:89` (`assert!(!r.secrets_ok)`) stays green: its fixture has only a `.` project, so it gets no `docker/` row; add no `docker/` project to it.

### S3. Watch entry and hello-static content (group D)

- **Goal**: the first image exists (main PR), and later, in a separate follow-up PR, the entry turns docker on for its rows.
- **Files**: main PR: `docker/hello-static/*`, `docker/README.md`. Follow-up PR (T020): `.github/factory-ci-repos.toml`.
- **Acceptance**: main PR: `hello-static` passes `image::check`; the Dockerfile follows `contracts/image-project.md` with the pins of `pins.md` (index digests; builder `mirror.gcr.io/library/golang:<supported>@sha256:<index>`; `GOTOOLCHAIN=local`; `docker buildx imagetools inspect` lists both platforms for each, recorded in the task); the pipeline builds and saves both platforms and ends `chmod 0644 image-*.tar`; `docker/README.md` names the digest refresh routine. Follow-up PR: the entry keeps `repo`, `id` and `branches = ["main"]`, gains `allow_docker = true` and `secrets = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]`; `watch::parse` accepts it (both names pass `valid_secret_name`).
- **Constraints**: the toml edit does not merge before T029 (0.4.0 pinned) and T028 (owner checks): under 0.3.0 it would move `rust-ci-runner` and `ci-canary` into the docker job (decision 10). `docker/` content is safe to merge earlier: the 0.3.0 runner does not discover it.

### S4. Engine additions (group A)

- **Goal**: `Engine::capture_in` and `Engine::run_left`.
- **Files**: `engine.rs`, `engine_tests.rs` (sibling test file, QA-10).
- **Acceptance**: `capture_in(args, env, input: &[u8], limit, container)` writes `input` to the child's standard input from a thread and closes it, keeps the env-clearing of `Engine::command`, returns stdout, reports a non-zero exit as `exit status: <n>` and kills the container on timeout; `capture` behaves as before (existing tests green); `run_left(run_id)` is true while any container, network or volume with the label `factory-ci.run=<run_id>` exists, and fails closed: it also returns true when `ps`, `network ls` or `volume ls` fails or cannot start. Tests with a stub engine script that echoes its stdin to a file: the bytes arrive; argv and the client env hold none of them.
- **Constraints**: no `Command::new` outside `command()`; the input never reaches a `format!`.

### S5. Archive config reader (group A)

- **Goal**: `archive::read_config(path) -> Result<ImageConfig, &'static str>`.
- **Files**: `archive.rs`, `archive_tests.rs`, `lib.rs`.
- **Acceptance**: reads `manifest.json` then the config member it names (both `blobs/sha256/<hex>` and `<hex>.json` forms) from a `docker save` tar by walking 512-byte headers and seeking past data; returns `ImageConfig { os: String, architecture: String, user: String, entrypoint: Vec<String>, cmd: Vec<String> }` deriving `Debug` (QA-2); one image entry required; a member of exactly 1 MiB passes and one byte more fails (QA-7); GNU long-name and PAX headers skipped; a truncated or garbage file, a missing member and a manifest naming a path that is not in the tar fail with `archive unreadable`, never panic; tests build tars in memory. `read_config` judges nothing: the root, shell and platform policy and its three fixed lines belong to `publish` (QA-1).
- **Constraints**: no new crate; no allocation proportional to the archive size.

### S5a. Docker Hub API client (group A)

- **Goal**: `hub::ensure_public(base, user, token, namespace, name) -> Result<(), &'static str>` per `contracts/hub-api.md`.
- **Files**: `hub.rs`, `hub_tests.rs`, `lib.rs`.
- **Acceptance**: against a `tiny_http` fake: login 401 gives `registry refused the token`; login ok then probe 200 public does nothing more; probe 200 private gives `publish refused: repository is private` and no create call; probe 404 then create 201 then a probe that shows public succeeds; create answered 400 or 409 (a race) is followed by a probe and succeeds when public; probe 401 or 403 with a valid login gives `docker hub api refused repository access` and never `registry refused the token`; create 401 or 403 gives `docker hub api refused repository creation`; create ok but the probe shows private gives `publish refused: repository is private`; any 5xx, a non-JSON body and a timeout give `docker hub api error`; response bodies never reach a returned string; the request bodies carry the token only in the login call; only the `access_token` and `is_private` fields are read; `base` other than `https://` or `http://127.0.0.1` is refused. The login endpoint stays pending T002 (QA-8). The roughly 30-second timeout test stays and runs in its own `cargo test --locked --lib hub_tests::` command (QA-9); the fake Hub ends its accept thread on drop like `ci_tests`.
- **Constraints**: `ureq` only, no new crate; 30-second timeout per call, never more than the time left of `JOB_DEADLINE`; the base is a function argument, and `job.rs` reads `FACTORY_CI_HUB_URL` only under `#[cfg(debug_assertions)]`; no env or argv for the token; the token and the JWT are never in a `format!` that is returned or logged.

### S6. Publish step (group B)

- **Goal**: `publish::publish` and `publish::verify_index` per `contracts/publish-step.md`.
- **Files**: `publish.rs`, `publish_tests.rs`, `lib.rs`.
- **Acceptance**: with a stub engine, a fake `Api` and a fake Docker Hub: not `main` or not an image project returns without an env read, engine call, API call or Hub call; a missing or empty token gives `publish refused: no token`; an empty username gives `publish refused: no login name`; a username other than `sudtanj` gives `publish refused: login is not the namespace`, prints no value and starts nothing; a missing, symlinked, empty or oversized archive (limit inclusive) gives the archive lines; `publish` applies the archive policy to `read_config`'s answer (QA-1): an empty, non-numeric or 0 user, a shell entrypoint or the wrong platform gives its fixed line and starts no container; an unreadable first tip pushes nothing; `run_left` true after teardown gives `publish refused: sidecar still running` and no container starts; teardown (`cleanup_run`) happens before the first publish container and before the first Hub call; `hub::ensure_public` runs after teardown and before container A, and its errors end the item with its fixed lines and start no container; the fresh directory path with a `:` is refused (`publish refused: archive unreadable`), the directory holds exactly the two archives and is mounted `:ro`; container argv is `--entrypoint <CRANE_SH> <CRANE_IMAGE> -c <script>` in that order, holds no token and no `-e` for it, the client env holds none, the token bytes arrive only on standard input of containers A and B; the username is in no redactor and not in `creds`; exit 41, other codes and a timeout map as the contract says; an existing `sha-<12>` list that passes `verify_index` runs no push and no overwrite and goes on to the tip check and, when both reads equal the commit, `latest` (no digest comparison); an existing list that fails ends `image index incomplete` and container B never runs; `verify_index` cases of `data-model.md`; tip equal on both reads runs B, different gives `not latest`; with too little time left before a call the item ends `publish refused: time ran out` and the call is not made; a repeat publish of a commit whose list exists ends in the same text as the first run. `publish_tests::` runs as its own cargo command (MINOR-1).
- **Constraints**: `Engine::capture_in` for both containers, no `Command::new`; the crane image const is a full digest pin (test asserts `@sha256:` shape); uid and gid from `MetadataExt` of the publish directory; no token in a `format!` that reaches a `Done` text; captured stdout parsed, never logged; each container and Hub call uses `min(own limit, time left)`.

### S7. Wire the job (group C)

- **Goal**: `job.rs` runs the check, the pipeline and publish, and lists the token name.
- **Files**: `job.rs`, `job_tests.rs`.
- **Acceptance**: prepare mode: the names file is `DOCKERHUB_USERNAME\nDOCKERHUB_TOKEN\n` only for `publishes` plus a clean check, empty otherwise, `NO_JOB_SECRETS` still first; `attempt` starts the shared `JOB_DEADLINE` (40 minutes) and gives clone, pipeline and publish the time left; it runs the check before `prepare()` (`check failed: <ids>`, no container), runs the pipeline, then `publish` before the final `cleanup_run`; for an image row a failed step (any exit code) and a timeout end `error` with the fixed `step <n> failed: ...` or `run timed out` line, and a test shows the next tick would retry it (`error`, not marked); a step of any other project still ends `failure`; non-image projects behave as before; `token_values` gains the token and the base64 of `sudtanj:<token>`, not the username.
- **Constraints**: do not log or `format!` the env value; `FACTORY_CI_HUB_URL` is read under `#[cfg(debug_assertions)]` only.

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
- **Acceptance**: every folder under `docker/` has the three files, a kebab name and an empty `image::check` result (this is FR-006); the infra entry satisfies, in both shapes (before and after the follow-up PR): `branches == ["main"]`, `secret_branches` absent, `allow_docker` false or true, `secrets` empty or exactly `["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]`, and `secrets` non-empty only when `allow_docker` is true (010 FR-016 relaxed for these two names because the script's shared route needs them in the entry; the test records why); the FR-022 test runs `git diff --name-only <base>...HEAD` (`<base>` from `FACTORY_CI_BASE` or `origin/main`), fails on a path under `.github/workflows/` or `.github/scripts/`, and prints a skip line, not a pass, when git or the base is missing.
- **Constraints**: stays out of the pipeline's integration command (the file's own comment says why).

### S11. Docs and repository description (last task, after S12)

- **Files**: `README.md`, `rust-ci-runner/README.md`; the description by `gh repo edit sudtanj/infrastructure --description ...`.
- **Acceptance**: the root README says what `docker/` is and how to add an image, both architectures, the owner steps (the two Bitwarden keys, token scope), that the runner creates a missing public repository; the runner README documents the publish step, the check ids, `DOCKERHUB_TOKEN` and its key, the post-publish check, the rollback, and states the residual risk plainly: whoever controls a `main` build step can tamper with the host through the privileged sidecar and is effectively a token holder; `main` is owner-merged; the check is review help, not a barrier; a rootless BuildKit is the future fix, out of scope; it also amends the sentence that the tick hands no secret to a step (true for user steps, relaxed for the built-in publish). `gh repo view --json description` shows the description. Prose per `stop-slop`.

### S12. Release and end to end (owner plus engineer, after group D)

- **Steps**: quickstart 0 (owner spikes, T002, including the two crane paths), 1, 4 (release, canary, then the watch-entry follow-up PR T020), 5 and the post-publish check. The engineer tags; the owner holds Bitwarden, Docker Hub and variable steps.
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
| `non_image_project_exports_no_credential` (prepare names empty, publish never called, no `DOCKERHUB_*` in the runner env, no Hub call) | FR-013, SC-003 |
| `canary_token_absent_from_public_lines_argv_envs_status` over every output and every engine call | FR-015, SC-003 |
| `token_only_on_stdin_of_publish_containers` (argv, client env, container env, stdin capture) | FR-015 |
| `sidecar_removed_before_publish_containers_start` (call order in the stub log) | FR-015 |
| `publish_dir_holds_only_the_archives_read_only` | FR-014, FR-015 |
| `archive_with_root_user_shell_entrypoint_or_wrong_platform_refused` | FR-030 |
| `older_commit_pushes_sha_only`, `unreadable_tip_pushes_nothing`, `second_tip_read_fails_leaves_sha_tag` | FR-018 |
| `existing_valid_list_is_reused_without_digest_compare`, `existing_invalid_list_errors`, `repeat_publish_only_tags_latest` | FR-020 |
| `image_step_failure_is_error_and_retried` (a failing step and a timeout of an image row post `error`; a step of another project posts `failure`) | FR-007 |
| `publish_container_names_the_entrypoint` (`--entrypoint <CRANE_SH> <CRANE_IMAGE> -c`, crane called by `CRANE_BIN`) | FR-014 |
| `publish_stops_when_the_deadline_is_near` and `mount_path_with_colon_is_refused` | FR-033, FR-015 |
| `no_tag_other_than_sha12_and_latest_is_ever_named` over every engine argv and env | FR-017, FR-026 |
| `missing_repository_is_created_public_before_the_push`, `existing_private_repository_stops_the_item`, `probe_401_is_not_a_bad_token` (fake Hub; call order in the stub log) | FR-016 |
| `login_name_must_equal_the_namespace_and_is_never_redacted_or_printed` | FR-031 |
| `hub_requests_carry_the_token_only_in_the_login_body` (fake Hub request log; no argv, no env, no status text) | FR-015, FR-016 |
| `repo_files::infrastructure_entry_lists_exactly_the_two_dockerhub_names_and_no_secret_branches` and `ci_tests::entry_secrets_without_secret_branches_unlock_nothing` | FR-023, 010 FR-010 |
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
| Rows | two tokens, two users (K2) | second tick re-plans a plain row | owner check T028, canary T029 | quickstart 4 |
| Check | Dockerfile unreadable, huge, not UTF-8, hostile syntax | `image::check` | named id, `failure` | S1 tests |
| Prepare | check fails | `secret_names` | empty names, real run reports `failure` | prepare names table |
| Token fetch | key missing, ambiguous, store down | script (`job.sh:78-86`) | `error`, the script's fixed line | E2E |
| Build | a step fails or times out (rate limit, network, a disk error, a single-arch base) | step exit | image row: `error` (single retry); other rows: `failure`; no archive pair, nothing pushed | `image_step_failure_is_error_and_retried`, `failed_build_pushes_nothing` |
| Deadline | clone, pulls, pipeline and publish together near 45 minutes | `JOB_DEADLINE` check before each call | `error` `publish refused: time ran out`, no further call | `publish_stops_when_the_deadline_is_near` |
| Teardown | sidecar or volume left, or `ps`, `network ls`, `volume ls` fails | `run_left` (fails closed) | `error` `publish refused: sidecar still running` | publish tests, engine tests |
| Mount | the fresh directory path holds `:` | mount check | `error` `publish refused: archive unreadable` | publish tests |
| Entrypoint | the pinned crane image lacks `CRANE_SH` or `CRANE_BIN` | T002, argv test | owner or engineer fixes the constants before merge; a wrong path makes the container fail as `push failed` | `publish_container_names_the_entrypoint`, T002 |
| Reuse | `sha-` list exists | exit 0 of A, `verify_index` | reuse, push nothing; invalid list gives `image index incomplete`, nothing overwritten | `existing_valid_list_is_reused_without_digest_compare`, `existing_invalid_list_errors` |
| Partial | list written, `latest` fails | exit of B | `error`; the retry reuses the list and only tags `latest` | `repeat_publish_...` |
| Archive | missing, symlink, too big, unreadable | `publish` | `error`, no push | publish tests |
| Archive | root user, shell entrypoint, wrong platform | `archive::read_config` | `error`, no push | `archive_with_root_user_...` |
| Token | empty in env | `publish` | `error` `publish refused: no token` | publish tests |
| Hub | login rejected (401) | `hub::ensure_public` | `error` `registry refused the token`, nothing created or pushed | `hub_tests.rs` |
| Hub | probe or create refused (401, 403) with a valid login | `hub::ensure_public` | `error` `docker hub api refused repository access` or `... creation`, an owner scope item (R13) | `probe_401_is_not_a_bad_token` |
| Hub | repository exists and is private | probe shows `is_private` | `error` `publish refused: repository is private`, nothing changed | `existing_private_repository_stops_the_item` |
| Hub | API down, 5xx, bad body, timeout | `hub::ensure_public` | `error` `docker hub api error`, one retry after 30 minutes | `hub_tests.rs` |
| Login | username empty or not `sudtanj` | `publish` | `error` `publish refused: no login name` or `login is not the namespace`, no value printed | `login_name_must_equal_...` |
| Registry | 401 or 403 on the push after the repository was ensured | exit 41 | `error` `registry refused the token` | publish tests |
| Registry | network, rate limit | exit 42, 43, timeout | `error` `push failed`, one retry after 30 minutes | publish tests |
| Index | list malformed or missing a platform | `verify_index` | `error` `image index incomplete`, `latest` untouched | `single_arch_or_bad_index_never_tags` |
| Tip | `branches` fails or no `main` | `publish` | first read: nothing pushed; second read: `sha-` stays | tip tests |
| Tip | newer commit exists | `sha != tip` | `success` `published sha-<12> not latest` | `older_commit_pushes_sha_only` |
| Status post | GitHub down | existing `post` | exit 1, script fallback | existing |
| Crash mid-publish | VM dies | stale `pending` after 2 h | existing retry rule; the retry reuses a written list | `repeat_publish_only_tags_latest` |

## Pre-mortem

1. **`crane` rejects a `docker save` tarball, refuses a push by digest, or `crane:debug` has no `sh`; or a pinned base is not an index so one platform fails.** Prevention: T002 runs the checks on a throwaway repository before any code depends on them; a push-by-digest refusal goes back to the lead (K1); base digests are checked with `imagetools inspect` (FR-028).
2. **The token leaks.** Prevention: stdin only; no env, no argv, no mount; `env_clear` in `Engine::command`; the sidecar is gone before the containers start; archives are checked; crane output to `/dev/null`; redactor holds the token and its docker-auth base64; the canary test covers every engine call; non-image and failing-check runs never get the name.
3. **Turning on `allow_docker` breaks the repo's own CI.** Under the pinned 0.3.0 runner the entry flag would route every row into the docker job (`plan.rs:307`, `plan.rs:489-490`), a missing `FACTORY_CI_TOKEN_DOCKER` would drop the whole entry (`main.rs:115-118`), and two token users would re-plan plain rows (K2, `plan.rs:185`). Prevention: the toml edit is a separate follow-up PR that merges only after 0.4.0 is pinned and the owner checks (T028) are done; a canary of two ticks before and after it; rollback reverts the toml first, then the digest.
4. **A hostile or sloppy Dockerfile passes the check.** Prevention: allowlist, heredoc and `--from` rules, directives in any case, pinned step images, archive config check at publish; residual risk stated plainly (R12): the check is review help, `main` is owner-merged.
5. **The release or pin goes wrong.** A bad `RUNNER_IMAGE_DIGEST` fails the tick guard (`factory-ci-tick.yml:42-44`) and every project stops. Prevention: set it only from the release workflow's printed digest; keep the old digest; canary before any image push.

## Invariants (qa turns these into property tests)

- I1. `publish` makes zero reads of `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`, zero engine calls, zero API calls and zero Hub calls for a non-image project or a ref other than `main`.
- I2. For every Dockerfile and pipeline, `check` returns sorted unique ids from the fixed set and never panics.
- I3. A check that returns any id leads to zero push calls, whatever the pipeline did.
- I4. The token value and the base64 of `sudtanj:<token>` appear in no argv, no Hub request except the login body, no environment of the client or of any container, no status text, no log line; they appear only as the standard input of containers A and B.
- I5. `latest` is tagged only when the tip read in the same publish, twice, equals the commit.
- I6. The `sha-` tag is written only by `crane index append`; `latest` only by `crane tag` of it; no other tag is ever named (`sha-` plus exactly 12 hex).
- I7. `secrets_ok` and row `allow_docker` are true only for project `docker/*` on branch `main`, and only when the entry has `allow_docker`.
- I8. Prepare mode lists a name only when I7's condition and a clean check both hold, and never a name the pipeline asked for.
- I9. `find` never returns a path of another shape than `.`, `<folder>`, `docker/<name>`.
- I10. `verify_index` accepts a list only when it holds exactly one `linux/amd64` and one `linux/arm64` entry, for every permutation and mutation (drop, duplicate, add, relabel).
- I11. Container B never runs when `verify_index` rejected the list or either tip read differs from the commit.
- I12. Every publish container starts after `cleanup_run` returned and `run_left` is false, and mounts a directory with exactly two files, read-only.
- I13. `archive::read_config` never panics and never reads more than 1 MiB of one member, for any byte input.
- I14. An existing `sha-<12>` list is never pushed over or retagged: the publish only reads it and tags `latest` on it.
- I15. For an image row, a failed step and a timeout end `error`; for any other project they end `failure`.
- I16. No publish call starts after `JOB_DEADLINE` minus its own minimum; the item then ends `publish refused: time ran out`.

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
|---|---|
| Inputs | Handled. Folder names 1, 2, 64, 65 characters; Dockerfile empty, 1 MB, NUL, invalid UTF-8, CRLF, BOM, heredocs, comment `FROM`, stage named like an image, hostile directives, `ADD`, `ONBUILD`, `--from` an external image (`image_tests.rs`); tar garbage and truncation (`archive_tests.rs`); a registry answer that is not JSON or has extra entries (`verify_index`). Duplicates: the same row twice is the existing dedupe. |
| State | Handled. A missing repository is created public by the runner, a private one stops the item (FR-016); no data; old runner ignores `docker/`. Crash mid-publish: stale pending, then a retry that reuses the existing list. Partial failure: list written, `latest` fails; the retry only tags `latest` (FR-020). Retry: a valid existing list is reused and never overwritten. |
| Time and concurrency | Handled. Ticks are serialised (`factory-ci-tick.yml:9-11`); tip read twice; one shared `JOB_DEADLINE` of 40 minutes covers clone, pulls, pipeline and publish under the 45-minute job limit (FR-033); each publish call uses the time left. Clock: no new time logic. An older commit pushes `sha-` only. |
| Resources | Handled. Archives capped at 2 GiB each; publish containers `--memory 1g`, `--pids-limit 256`; the archives are moved, not copied; the directory is removed after publish; existing 10 MB log cap. Smallest machine: the GitHub VM, not the 1 GB planning VM. |
| Permissions and trust | Handled. The token reaches only the runner process and two publish containers on stdin, only for a passing `docker/*` row on `main`. A user step never gets it. Residual risk R12: a `main` build step controls a privileged sidecar and is effectively a token holder; `main` is owner-merged. |
| UI | N/A. Surfaces are fixed commit status text and the Docker Hub page. |

## Rollout and rollback

**Rollout, in order.** The watch-entry edit is a separate follow-up PR (decision 10).

1. Owner setup can start (quickstart 0 and 1, spec A14). Record the current digest: `gh variable get RUNNER_IMAGE_DIGEST`.
2. Merge the main PR (runner code, `docker/hello-static`, tests) once the `factory-ci-selftest` result is green. The watch entry is unchanged, so the tick still runs the 0.3.0 runner and plans the repo exactly as before; `docker/` is not discovered. Check one tick.
3. Tag `rust-ci-runner-v0.4.0` on the merge commit. Wait for the release workflow and record its digest.
4. `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`. No YAML edit: the tick reads `vars.RUNNER_IMAGE_DIGEST` (`factory-ci-tick.yml:37,57,133`). Canary, two ticks: `factory-ci/rust-ci-runner` and `factory-ci/ci-canary` post once and are not planned again.
5. The owner finishes T028 (key names, token scope, `FACTORY_CI_TOKEN_DOCKER` coverage and user, Environment).
6. Merge the follow-up PR with `allow_docker = true` and the two `secrets` names. Canary again, two ticks (K2): the plain projects are not planned again. Then `factory-ci/docker/hello-static` posts; run quickstart 5 and the post-publish check.

Why the order: under 0.3.0 the toml edit would route every row of the repo into the docker job, and a missing `FACTORY_CI_TOKEN_DOCKER` would drop the whole entry (`main.rs:115-118`).

**Rollback.** In this order.

1. If the follow-up PR is merged, revert it first in a separate reviewed PR. Plain rows return to the plain job.
2. `gh variable set RUNNER_IMAGE_DIGEST --body <the digest recorded in step 1>`. The next tick runs 0.3.0, discovers no `docker/` project, publishes nothing.
3. Docker Hub: delete the published tags and any repository the runner created by hand if needed (nothing in the runner deletes them), and revoke the Hub token.
4. Token: delete the Bitwarden key; every `main` image run then ends in `error` and pushes nothing.
5. No data migration exists.

## Infrastructure assessment

Findings from the devops review and where this plan answers each.

| Finding | Answer |
|---|---|
| Branch exposure would put every branch of `rust-ci-runner` and `ci-canary` on the docker job (blocker) | `branches = ["main"]` stays; rows narrowed (S2, I7) |
| First push to a missing repository gives 401 | The runner creates the public repository first through the Docker Hub API (FR-016, S5a); a 401 or 403 on the probe or creation is a scope line, never "bad token"; T002 checks the scope a Read & Write token has |
| Token in env or argv of the push | Standard input only (S4, S6, I4) |
| Docker Hub pull limit for the Go builder | `mirror.gcr.io/library/golang` by digest (A13, T001 checks the mirror serves the digest) |
| Environment and secrets of the docker path | Owner list in A14 and T028: the two key names present in the docker project, token scope, status token coverage and same user for both tokens (K2), Environment limited to `main`, `FACTORY_CI_BLOB_KEY_DOCKER` set |
| Post-publish confidence | SC-007 and task T030: after a tick with a docker change, `sha-<12 of the main tip>` exists; no answer counts as unknown |
| Runner minutes | Each image builds twice inside the 30-minute run deadline; cross-compilation keeps both near native speed; only `docker/` rows use the docker job |

Not verifiable here: the Environment's protection rules, the status token's scope, the user behind each token. T028 lists them for the owner.

## Security assessment

Findings from the security review and where this plan answers each.

| Finding | Answer |
|---|---|
| Critical: a branch could run a privileged build and a token path | No branch is planned; rows narrowed; the static check runs as a test only (S2, S10) |
| High: the sidecar is privileged and shares a host with the token | Teardown and `run_left` before the publish containers; fresh directory with two archives, read-only; token on stdin (S4, S6, I12) |
| High: the published image is not the one the Dockerfile describes | `archive::read_config` checks user, entrypoint and platform of each archive before any push (S5, S6) |
| High: parser bypasses (`ADD`, `ONBUILD`, heredocs, external `--from`, directive spellings, unpinned step image) | Allowlist and rules with one test per bypass (S1) |
| Tag squatting and retries | `sha-<12 hex>`; a valid existing list is reused and never overwritten, no digest comparison (decision 12) |
| Supply chain | Digest pins for every image (builder from a mirror, distroless, `docker:cli`, `crane`); refresh routine in `docker/README.md` |

Review and security fixes of S6 (tasks T032 to T042):

| Finding | Answer |
|---|---|
| High (S1): the sidecar's `dockerd` listens on `tcp://0.0.0.0:2375` with no authentication, and a nested build container reaches it at the bridge gateway | After `docker info` succeeds, `Sidecar::start` runs `docker exec <sidecar> iptables -I INPUT -p tcp --dport 2375 ! -i eth0 -j DROP` and fails closed on an error. Chosen over a unix socket in a per-run volume because a socket would need a shared volume and a socket mode that every step image's user can use, and `DOCKER_HOST` and the mounts would change for every `allow_docker` pipeline; the rule changes one place, keeps `DOCKER_HOST=tcp://...` and reaches only what is in the sidecar's namespace: the nested bridge and `lo`. The step reaches the daemon through `eth0`, the run network. Applies to every `allow_docker` run. Pinned by `tests/job_rules.rs` |
| Medium (S2): free shell in the step can build another Dockerfile or `docker save` any image | `image::check` requires exactly one step `build` with `docker = true` and the contract commands (`step-template`) |
| Medium (S3): only three files are scanned | `image::check` walks the folder with caps, refuses non-regular entries and credential file names, and scans every file (`layout`, `secret-file`, `secret-value`) |
| Medium (S4): a later `secret_branches` would hand the Docker Hub names to the manual `ci` | `ci.rs` strips both names from the allowed list |
| Low (S6, S7) | `archive.rs` refuses extended tar headers and a second `manifest.json`; `job` and `status` silence the panic hook |
| Low (S8) | A frozen script: not fixed |
| R3: prepare's time is not counted against `JOB_DEADLINE` | Accepted. A frozen script runs prepare and the job as two processes, so no instant passes between them. Prepare's only slow step is the clone, capped at 10 minutes; in the worst case the 45-minute job limit stops the run and the next tick retries it. No credential is exposed by that |

**Residual risk, stated plainly.** Whoever controls a `main` build step can tamper with the host through the privileged docker sidecar before the runner removes it, and so is effectively a token holder. `main` is owner-merged. The check is review help, not a barrier. Rootless BuildKit is the future fix and is out of scope. The token is scoped Read & Write with an expiry (A14). One Docker Hub token covers the whole `sudtanj` namespace (R2).

## Open risks

- R-A the pipeline can still push any archive that passes the archive check; `main` is the owner's.
- R-B `docker save`, `crane push` by digest and `crane index append` compatibility is untested until T002 and T030.
- R-C `FACTORY_CI_TOKEN_DOCKER` scope, Environment protection and the two-user question are owner checks (K2).
- R-G a Read & Write token may not be allowed to create a repository through the Docker Hub API (spec R13): T002 checks it before any code depends on it; a refusal is an owner decision on scope, and the runner reports `docker hub api refused repository creation` and pushes nothing.
- R-D overlap with 011 (K5).
- R-E a Docker ID that differs from the namespace makes login fail with `registry refused the token`.
- R-F pinned images (`crane:debug`, `docker:cli`, builders) freeze until someone bumps the digest.
