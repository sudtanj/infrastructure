# Feature Specification: Plain Actions CI for factory-monorepo

**Feature Branch**: `005-plain-actions-ci`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "A manually started plain GitHub Actions workflow that builds rust-ci-runner from source and runs the private repo sudtanj/factory-monorepo's `.factory-ci.toml` pipeline with it, with minimal public output. Stopgap until the runner image release (feature 003) lands."

## Clarifications

### Session 2026-11-27

- Q: Is any step output shown in the public log? → A: No. The runner runs with `--quiet-logs`, its stdout and stderr go to a file, and only allow-listed result lines are printed. A failure reason after `failed in Ns` is dropped.
- Q: Do pipeline steps get secrets? → A: No. `FACTORY_CI_SECRETS` is set empty for the runner, so no environment value is read as a step secret. A step that declares a secret fails.
- Q: What can `ref` name? → A: A branch of factory-monorepo only. Tags and SHAs are out of scope.
- Q: Is the cargo build cached? → A: No. No cache action, to keep the workflow small and avoid an unpinned action.
- Q: Which Bitwarden project holds `FACTORY_CI_TOKEN`? → A: Any the access token can read. The secret list is not limited to one project; exactly one key match is required.

### Session 2026-11-27 (review)

- Q: Does the build use `--locked`? → A: No. `rust-ci-runner/Cargo.lock` is not on `main`, so `--locked` fails. Revisit when a lock file is committed.
- Q: How does quiet mode work with no log store? → A: The runner refuses `--quiet-logs` without one, so a loopback store (`127.0.0.1:9`) is configured and discarded.
- Q: Are docker steps allowed? → A: No. The job fails closed when the root `.factory-ci.toml` has `docker = true` (or cannot be read).
- Q: What do readers see when the runner fails before any step? → A: One fixed line, never log text.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Run the monorepo pipeline by hand (Priority: P1)

The owner starts the workflow from the Actions tab on `main`, picks a factory-monorepo branch (default `main`), and gets a pass or fail for each pipeline step without waiting for the runner image release.

**Why this priority**: It is the whole feature. The tick and the runner image are not available yet.

**Independent Test**: Start the workflow on `main` with the default ref. The run builds the runner, clones factory-monorepo, runs its steps in Docker and ends green or red with the runner's exit code.

**Acceptance Scenarios**:

1. **Given** the workflow runs on `main` with a valid ref, **When** every pipeline step passes, **Then** the job succeeds and the log shows one result line per step.
2. **Given** a step fails, **When** the run ends, **Then** the job fails and the log shows which step failed and its duration.
3. **Given** the workflow is started from a branch other than `main`, **When** the job is evaluated, **Then** it does not run.

---

### User Story 2 - Nothing private reaches the public log (Priority: P1)

This repo and its logs are public, factory-monorepo is private. A reader of the log learns step names, pass or fail and durations, and nothing else.

**Why this priority**: Constitution Principle III. A leak cannot be undone.

**Independent Test**: Read the whole job log and summary of a run. No token, no encoded token, no test output, no file path from the private repo appears.

**Acceptance Scenarios**:

1. **Given** a step prints text that looks like a runner result line, **When** the log is filtered, **Then** that text is not printed.
2. **Given** the clone token is fetched, **When** any later step runs, **Then** the token and its base64 form are masked and absent from the runner's environment and from `.git/config`.
3. **Given** a bad `ref` input, **When** the job starts, **Then** it fails before any secret is read.

### Edge Cases

- `ref` has a leading `-`, contains `..`, or has characters outside `[A-Za-z0-9._/-]`: rejected before use.
- `ref` names no branch of factory-monorepo: the clone fails with a message that names no secret; the job fails.
- Bitwarden has zero or several secrets with key `FACTORY_CI_TOKEN`: the job fails naming the key, never a value.
- Bitwarden is unreachable or rejects the token: the job fails; each external call has a timeout.
- The bws download does not match the pinned digest, or the archive lists more than `bws`: the job fails.
- The runner prints nothing recognisable, or fails before any step: the job ends with the runner's exit code and a summary line `passed` or `failed`, plus the fixed failure line on a non-zero exit.
- Two runs for the same ref start together: they queue; one ref never runs twice at once.
- A step needs a pipeline secret: this workflow supplies none, so that step fails.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The workflow MUST start only by hand (`workflow_dispatch`) with one input, `ref`, a factory-monorepo branch name, default `main`. No push or schedule trigger.
- **FR-002**: The job MUST run only when the workflow runs on `main`, in the Environment `factory-ci`.
- **FR-003**: The workflow MUST grant no permissions at the top and only `contents: read` to the job. Concurrency is per `ref`. The job times out after 30 minutes.
- **FR-004**: The `ref` input MUST be validated before use: only `[A-Za-z0-9._/-]`, no leading `-`, no `..`. It reaches scripts through `env:`; no `${{ }}` expression appears inside a `run:` script.
- **FR-005**: The workflow MUST install `bws` 2.1.0 from the official release, only if its SHA-256 equals the pinned digest and the archive lists exactly `bws`.
- **FR-006**: The workflow MUST read the Bitwarden secret `FACTORY_CI_TOKEN` (exactly one match), mask it and its base64 form before any other use, and use it only to clone factory-monorepo at `ref` (depth 1). The token MUST NOT remain in `.git/config`, a step output or the runner's environment.
- **FR-007**: The workflow MUST build `rust-ci-runner` from this repo with `cargo build --release` (no `--locked`: no `Cargo.lock` is committed, so dependencies float until one is), and check this repo out by pinned commit SHA without persisted credentials.
- **FR-008**: The workflow MUST run `rust-ci-runner run --dir <clone>` with Docker, with the runner's stdout and stderr written to a file that is never uploaded. The runner gets `--quiet-logs` and an empty `FACTORY_CI_SECRETS`. Quiet mode needs a log store, so a throwaway loopback one is configured; shipping to it fails silently.
- **FR-009**: The workflow MUST print, and write to the job summary, only lines that match a strict allow pattern for the runner's own step results (name, pass or fail, duration). It MUST NOT print step output. The job MUST exit with the runner's exit code. A failure reason after the duration is dropped. If the runner fails and no result line was printed, the workflow prints and summarises one fixed line, never the log.
- **FR-010**: The workflow MUST NOT post commit statuses.
- **FR-011**: The root `README.md` MUST say how to run the workflow, that it is a stopgap until the runner image is released, that logs are minimal on purpose, that docker steps are refused, that `FACTORY_CI_BWS_SERVER_URL` should stay unset, and that the `factory-ci` Bitwarden machine account should read only a project holding `FACTORY_CI_TOKEN`.
- **FR-012**: The workflow MUST fail before the run step if the clone's root `.factory-ci.toml` enables `docker = true` for any step, or cannot be checked. A docker-in-docker step would give root on the VM and access to the job's secrets, `BWS_ACCESS_TOKEN` included. Such repos use the factory CI runner with `allow_docker`.

### Key Entities

- **Run**: one manual start; inputs `ref`; result pass or fail.
- **Result line**: step name, state, duration; the only runner text that is shown.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: One manual start produces a pass or fail for the monorepo pipeline in under 30 minutes.
- **SC-002**: Across a run's public log and summary, zero occurrences of the token, its encoded form, or private test output.
- **SC-003**: A start on any ref other than `main` runs no job; an invalid `ref` fails before any secret is read.
- **SC-004**: The workflow can be deleted with no other file changing, once the runner image release lands.

## Assumptions

- Owner decision, 2026-11-27: building `rust-ci-runner` with cargo on a hosted runner through a plain workflow is an exception to the rule that compile, test and lint run only through the CI runner, because the runner cannot build itself before it exists. Only the runner's own build gets this exception. Factory-monorepo's steps still run through `rust-ci-runner`. The constitution is not edited.
- The Environment `factory-ci` exists, allows only `main`, and holds the secret `BWS_ACCESS_TOKEN`. The optional variable `FACTORY_CI_BWS_SERVER_URL`, when set, starts with `https://`.
- The Bitwarden secret `FACTORY_CI_TOKEN` is a GitHub token with read access to factory-monorepo contents.
- The hosted runner has Docker, `jq`, `unzip`, `curl` and `rustup`.
- This workflow is a stopgap; feature 003 (runner image, tick) replaces it.
