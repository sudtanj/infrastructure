# Feature Specification: Run the runner's unit tests through the runner

**Feature Branch**: `006-runner-unit-tests`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "A `.factory-ci.toml` in `rust-ci-runner/` that runs the runner's `--lib` unit tests through rust-ci-runner itself, and a `pipeline` choice input (`factory-monorepo` or `rust-ci-runner`) on the manual workflow `factory-monorepo-ci.yml`. Extends 005-plain-actions-ci and keeps its owner exception."

## Clarifications

### Session 2026-11-27

- Q: Which tests run? → A: Only the `--lib` unit tests. `tests/executor.rs` and the other integration tests call the real `docker` binary, which a step container does not have.
- Q: Why three `cargo test` commands? → A: A step container has `--pids-limit 512`. The `ship_tests` and `job_tests` modules start mock HTTP servers whose threads never exit, so each runs in its own process.
- Q: Which source does the `rust-ci-runner` pipeline test? → A: The `main` checkout of this repo already in the workspace. The `ref` input does not apply and is ignored.
- Q: Is `--locked` used? → A: No. `rust-ci-runner/Cargo.lock` is not on `main`. Revisit when one is committed.
- Q: Does the pipeline need pipeline-side credentials or docker steps? → A: No. The step sets no `docker` and declares none; the existing refusal of `docker = true` runs on this pipeline too.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Run the runner's unit tests by hand (Priority: P1)

The owner starts `factory-monorepo-ci` on `main`, picks `rust-ci-runner` as the pipeline, and gets pass or fail for the runner's unit tests, executed by the runner just built from the same checkout.

**Why this priority**: It is the whole feature. Today nothing runs these tests in CI.

**Independent Test**: Start the workflow on `main` with pipeline `rust-ci-runner`. The run builds the runner, runs the step `test` in a `rust:1.90` container and ends with the runner's exit code.

**Acceptance Scenarios**:

1. **Given** the workflow runs on `main` with pipeline `rust-ci-runner`, **When** all unit tests pass, **Then** the job succeeds and the summary starts with `rust-ci-runner main: passed`.
2. **Given** a unit test fails, **When** the run ends, **Then** the job fails and the log shows the step `test` as failed with its duration.
3. **Given** pipeline `rust-ci-runner`, **When** the job starts, **Then** no clone of factory-monorepo happens and `ref` is not validated or used.

---

### User Story 2 - The factory-monorepo run is unchanged (Priority: P1)

Starting the workflow with the default pipeline behaves exactly as under feature 005.

**Why this priority**: Adding the input must not break the existing stopgap.

**Independent Test**: Start with the default input. The log and summary match 005, first line `factory-monorepo <ref>: passed` or `failed`.

**Acceptance Scenarios**:

1. **Given** pipeline `factory-monorepo` (default), **When** the job runs, **Then** it validates `ref`, clones factory-monorepo at `ref`, refuses docker steps and runs the clone's pipeline.
2. **Given** two runs with different pipelines start together, **When** they are queued, **Then** they do not block each other.

### Edge Cases

- The runner's own `.factory-ci.toml` gains `docker = true`: the docker refusal fails the job, as for factory-monorepo.
- A lib test needs the `docker` binary: it fails in the container and the job fails. No lib test does today.
- `ref` is bad while the pipeline is `rust-ci-runner`: it is ignored, the job runs.
- A mock-server test leaves threads behind: the three-process split keeps each process under the pids limit.
- The runner fails before any step: the fixed line from 005 is printed.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: `rust-ci-runner/.factory-ci.toml` MUST define one step `test` with image `rust:1.90`, timeout 1800 and three commands: `cargo test --lib -- --skip ship_tests:: --skip job_tests::`, `cargo test --lib ship_tests::`, `cargo test --lib job_tests::`. It MUST use only the fields `image`, `commands` and `timeout`, and MUST NOT set `docker`.
- **FR-002**: The file MUST carry comments saying why only `--lib` runs, why `ship_tests` and `job_tests` run in separate processes, and a shortcut comment for the missing `--locked`.
- **FR-003**: The workflow MUST gain a `workflow_dispatch` input `pipeline`, type choice, options `factory-monorepo` and `rust-ci-runner`, default `factory-monorepo`. Its description MUST say `ref` is ignored for `rust-ci-runner`.
- **FR-004**: For `rust-ci-runner`, the steps `validate ref` and `fetch token and clone factory-monorepo` MUST NOT run. The runner runs with `--dir` set to `rust-ci-runner/` of the workspace checkout.
- **FR-005**: The docker refusal and the run step MUST read the pipeline directory from one `DIR` variable, set from the input through `env:`. No `${{ }}` expression appears inside a `run:` script.
- **FR-006**: The first summary line MUST be `<pipeline> <ref>: <result>` for factory-monorepo and `rust-ci-runner main: <result>` for the runner pipeline. The pipeline name reaches the script through `env:`.
- **FR-007**: The concurrency group MUST include the pipeline and `ref`.
- **FR-008**: Bitwarden handling, Loki keys, masking, the output filter, the main-only guard, the Environment and the 30 minute timeout MUST stay as in 005. The run step is renamed `run pipeline`.
- **FR-009**: The root `README.md` MUST say how to start the runner's unit tests and that only `--lib` unit tests run.

### Key Entities

- **Pipeline**: one of `factory-monorepo`, `rust-ci-runner`; picks the directory the runner reads.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: One manual start with pipeline `rust-ci-runner` gives pass or fail for the unit tests in under 30 minutes.
- **SC-002**: A start with the default pipeline produces the same steps and output as before this feature.
- **SC-003**: The public log and summary of either pipeline hold zero secret values and no step output.
- **SC-004**: Deleting `rust-ci-runner/.factory-ci.toml` and the `pipeline` input leaves the 005 workflow intact.

## Assumptions

- This feature extends `specs/005-plain-actions-ci/` and keeps its owner exception of 2026-11-27: the runner is built with cargo in a plain workflow; nothing else is.
- The `rust:1.90` image has `sh`, `env` and `git`; the lib tests use fake engine scripts and mock servers, no real container engine.
- The hosted runner has Docker for the step container, as in 005.
- Bitwarden is still read for the Loki keys on both pipelines; only the clone token is skipped for `rust-ci-runner`.
