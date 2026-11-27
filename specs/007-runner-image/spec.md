# Feature Specification: Runner image in the pipeline workflow

**Feature Branch**: `007-runner-image`

**Created**: 2026-11-27

**Status**: Draft. Superseded in part by 008-runner-owned-ci: FR-002 to FR-009, for `factory-monorepo-ci.yml` (FR-012, the native arm64 build, stays)

**Input**: User description: "Stop compiling rust-ci-runner inside factory-monorepo-ci.yml. The workflow uses the released runner image and checks that the Grafana Loki settings work."

## Clarifications

### Session 2026-11-27 (owner decisions)

- Q: Why does the arm64 release build change? → A: Run 46062705748 failed with `bits/libc-header-start.h: No such file or directory` from `aarch64-linux-gnu-gcc` building ring 0.17.14 for `aarch64-unknown-linux-musl`. Native runners replace the cross compiler.

- Q: Which workflow may build the runner? → A: Only `.github/workflows/rust-ci-runner-release.yml`. The pipeline workflow fully relies on the compiled image for everything except that build. This replaces the build exception recorded in 005.
- Q: Does bws stay? → A: Yes. It is the only source of the Loki settings and the factory-monorepo clone token (one Bitwarden token, project rust-ci-runner).
- Q: Does the runner run inside its image? → A: No. The image is `FROM scratch` with only `/rust-ci-runner`, a static musl binary: no docker CLI, no shell. The workflow copies the binary out and runs it on the VM.
- Q: Is the test compile of the runner's own pipeline a build of the runner? → A: No. The step container compiles the tests in `rust:1.90`; that is the pipeline.
- Q: How is the image referenced? → A: By tag and digest, in one `env:` line that the owner bumps after each release.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Pipeline runs on the released runner (Priority: P1)

The owner starts `factory-monorepo-ci` and the job runs the released runner binary instead of compiling one, so a run no longer spends minutes on a build and no longer depends on the state of the runner source on `main`.

**Why this priority**: It is the owner's decision (a): only the release workflow builds the runner.

**Independent Test**: Start the workflow with each pipeline. The log shows a `fetch rust-ci-runner` step, no compile step, and the run step ends with the runner's exit code.

**Acceptance Scenarios**:

1. **Given** the pinned image exists in ghcr.io, **When** the job runs, **Then** the binary is copied out of it into `$RUNNER_TEMP/bin` and the run step calls that binary.
2. **Given** the package is private, **When** the job logs in with the job's own token, **Then** the pull succeeds.
3. **Given** the pinned digest does not exist, **When** the pull fails, **Then** the job fails before any pipeline step runs.

---

### User Story 2 - Bad Loki settings fail the job loudly (Priority: P1)

The runner's local mode drops failed Loki pushes silently, and the owner saw no logs in Grafana. The workflow now posts one probe line to the log store before the runner starts, and fails with a fixed hint if the store does not accept it.

**Why this priority**: A run whose logs go nowhere is worse than a failed run: step output is public-log-free by design.

**Independent Test**: Run with a wrong `FACTORY_CI_LOG_USER`. The job fails at the probe with `log store answered HTTP 401` and the fixed hint.

**Acceptance Scenarios**:

1. **Given** `FACTORY_CI_LOG_URL` ends with `/loki/api/v1/push` or `/`, **When** the run step reads it, **Then** both suffixes are stripped before export, so the runner's own appended path is not doubled.
2. **Given** correct settings, **When** the probe posts one line, **Then** the log shows `log store answered HTTP 204` and the runner starts.
3. **Given** the store answers a non-2xx code or does not answer in 15 s, **When** the probe ends, **Then** the job fails with the fixed hint and the runner never starts.

### User Story 3 - Release builds both architectures (Priority: P1)

The release run for `rust-ci-runner-v0.1.0` failed on arm64 and published nothing, so the pipeline has no image to pull. Each architecture builds on a native runner, with no cross compiler.

**Independent Test**: Push a release tag. Both build jobs pass and the image has two platforms.

**Acceptance Scenarios**:

1. **Given** a release tag, **When** the build job runs, **Then** amd64 builds on `ubuntu-24.04` and arm64 on `ubuntu-24.04-arm`.

### Edge Cases

- The digest in `RUNNER_IMAGE` is the placeholder before the first release: the pull fails and the job fails. The owner fills it in once the release finishes.
- The binary is missing from the image: `docker cp` fails; `set -e` stops the job; the temporary container is removed on that path too.
- The Loki URL has a path other than `/loki/api/v1/push`: it is kept as is; the probe shows the resulting code.
- curl cannot connect: it prints `000`, which is not 2xx, so the job fails.
- The Loki password never appears in argv or in the log: credentials go to curl on stdin; the token is already masked.
- A pull for an arm64-only digest on the x86_64 runner: the pull fails; the pin must be the multi-arch index digest.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The workflow MUST NOT compile the runner. The only build of the runner is `rust-ci-runner-release.yml`.
- **FR-002**: A top-level `env:` MUST hold `RUNNER_IMAGE`, `ghcr.io/sudtanj/rust-ci-runner:<version>@sha256:<digest>`, with a comment that it is pinned by digest and bumped after each release.
- **FR-003**: A step `fetch rust-ci-runner` MUST log in to ghcr.io with the job token over stdin, pull the image, create a container, copy `/rust-ci-runner` to `$RUNNER_TEMP/bin/rust-ci-runner`, remove the container and make the file executable. The job gets `packages: read`.
- **FR-004**: The run step MUST call `$RUNNER_TEMP/bin/rust-ci-runner`. The `rust-ci-runner` pipeline still uses `$GITHUB_WORKSPACE/rust-ci-runner` as its directory.
- **FR-005**: After reading the three `FACTORY_CI_LOG_*` values and before the runner starts, the run step MUST strip a trailing `/`, then a trailing `/loki/api/v1/push` from `FACTORY_CI_LOG_URL` and export the result.
- **FR-006**: The run step MUST then POST one JSON line to `$FACTORY_CI_LOG_URL/loki/api/v1/push` with a 15 s limit, https only, basic auth credentials passed on stdin, never in argv. The stream labels are `repo`, `project=probe`, `step=probe`, `stream=stdout`.
- **FR-007**: The step MUST print exactly `log store answered HTTP <code>` and fail on a non-2xx code, with the fixed hint: `check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)`.
- **FR-008**: Every value reaches `run:` scripts through `env:`; no `${{ }}` sits inside a `run:` block.
- **FR-009**: bws stays as the only source of the Loki settings and the clone token.
- **FR-010**: The header comment states that the runner comes from the released image and only the release workflow builds it.
- **FR-011**: `README.md` and `rust-ci-runner/README.md` say how CI gets the runner, where they mention it.
- **FR-012**: The `build` job of `rust-ci-runner-release.yml` MUST build each architecture on a native runner (`ubuntu-24.04` for amd64, `ubuntu-24.04-arm` for arm64) with `musl-gcc`, no cross compiler. Run 46062705748 failed because the aarch64 cross gcc lacked the libc headers (`bits/libc-header-start.h`) while building ring 0.17.14, and nothing was published.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: No compile step runs in the pipeline workflow; the word `cargo` does not appear in `factory-monorepo-ci.yml`.
- **SC-002**: A run with wrong Loki settings fails in under one minute at the probe, with one fixed line and one fixed hint.
- **SC-003**: Across the public log and summary of a run, zero occurrences of the Loki token or the clone token.
- **SC-004**: A tag push builds both binaries and publishes the image.

## Assumptions

- The release `rust-ci-runner-v0.1.0` publishes a multi-arch image `ghcr.io/sudtanj/rust-ci-runner:0.1.0` and `:latest`, built `FROM scratch`.
- The owner replaces `DIGEST_PLACEHOLDER` with the release digest before the first run.
- The hosted runner has `docker`, `curl`, `jq` and `date` with `%N`.
- Loki answers 204 on a valid push.
- Supersedes the build exception in 005 (FR-007 there) and 006's reliance on it.
