# Tasks: Plain Actions CI for factory-monorepo

**Input**: `specs/005-plain-actions-ci/` (spec.md, plan.md)

**Tests**: no code under test. T005 runs the static checks that plan.md names as the test stand-in.

## Phase 1: User Story 1 and 2 - run by hand, nothing private in the log (Priority: P1)

**Goal**: one manually started workflow that builds the runner, runs factory-monorepo's pipeline and prints only result lines.

**Independent Test**: T005 passes; the owner's first run on `main` ends with the runner's exit code and a log with result lines only.

- [x] T001 [US1] Create `.github/workflows/factory-monorepo-ci.yml`: owner-exception comment on line 1, `workflow_dispatch` with input `ref`, `permissions: {}`, concurrency per ref, job guard `github.ref == 'refs/heads/main'`, `environment: factory-ci`, job `contents: read`, `ubuntu-24.04`, 30 minute timeout, pinned checkout with `persist-credentials: false`, and a `ref` validation step (FR-001 to FR-004, FR-007).
- [x] T002 [US2] In `.github/workflows/factory-monorepo-ci.yml`, add the `bws` install step: pinned zip digest, `https`-only curl with `--max-time 120`, archive lists exactly `bws`, install to `$RUNNER_TEMP/bin`, optional `https://` server URL (FR-005).
- [x] T003 [US2] In `.github/workflows/factory-monorepo-ci.yml`, add the fetch-and-clone step: one `FACTORY_CI_TOKEN` match, shape check, mask raw and base64 forms, clone through `GIT_CONFIG_*` environment so the token is not in argv or `.git/config` (FR-006).
- [x] T004 [US1] In `.github/workflows/factory-monorepo-ci.yml`, add the cargo build step (`cargo build --release` in `rust-ci-runner/`, no `--locked`, with the shortcut comment) and the run step: `--quiet-logs`, empty `FACTORY_CI_SECRETS`, output to `$RUNNER_TEMP/runner.log`, allow-pattern filter, `$GITHUB_STEP_SUMMARY`, runner exit code (FR-007 to FR-010).
- [x] T005 [US1] Run the static checks on `.github/workflows/factory-monorepo-ci.yml`: YAML parse, `bash -n` on each extracted `run:` block, grep that no `${{` sits inside a `run:` block, `shellcheck` on each block if installed, and a test of the filter and of the docker guard against sample input.
- [x] T006 [US1] Review fix, FR-007: in `.github/workflows/factory-monorepo-ci.yml`, drop `--locked` from the build and add the shortcut comment (no `Cargo.lock` on `main`).
- [x] T007 [US1] Review fix, FR-008: in the run step of `.github/workflows/factory-monorepo-ci.yml`, set a throwaway loopback log store so `--quiet-logs` is accepted, with a one-line comment.
- [x] T008 [US2] Review fix, FR-009: in the run step, print and summarise the fixed line `runner failed before running any step (see a run with a log store for details)` when the exit code is non-zero and no result line was printed.
- [x] T009 [US2] Review fix, FR-012: add the `refuse docker steps` step to `.github/workflows/factory-monorepo-ci.yml` before the run step; it fails closed on `docker = true` or a failed check and prints only a fixed message.
- [x] T011 On a failed `FACTORY_CI_TOKEN` lookup, print only the number of secrets the access token sees and how many keys match ignoring case and surrounding spaces, never names or values (FR-004 diagnostics, found by the first live run).
- [x] T010 Update `README.md`: how to start the workflow, the Environment and secret it needs, stopgap until the runner image release (feature 003), the owner exception of 2026-11-27, logs minimal on purpose, docker steps refused, `FACTORY_CI_BWS_SERVER_URL` kept unset, and a Bitwarden machine account that reads only a project holding `FACTORY_CI_TOKEN` (FR-011). Last task.

## Dependencies

T001, then T002 to T004 and T006 to T009 in order (same file), then T005, then T011, then T010.
