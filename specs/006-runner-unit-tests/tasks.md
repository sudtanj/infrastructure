# Tasks: Run the runner's unit tests through the runner

**Input**: `specs/006-runner-unit-tests/` (spec.md, plan.md)

**Tests**: no code under test. T004 runs the static checks that plan.md names as the test stand-in.

## Phase 1: User Story 1 - run the runner's unit tests by hand (Priority: P1)

**Goal**: a pipeline file the runner reads from `rust-ci-runner/`, and a workflow input that points the run at it.

**Independent Test**: T004 passes; the owner's first run with pipeline `rust-ci-runner` ends with the runner's exit code.

- [x] T001 [US1] Check in `rust-ci-runner/src/lib.rs` that `ship_tests` and `job_tests` are test modules reachable by the filters, and grep `rust-ci-runner/src/` for lib tests that shell out to docker; record the result in the report (FR-001).
- [x] T002 [US1] Create `rust-ci-runner/.factory-ci.toml`: step `test`, image `rust:1.90`, timeout 1800, the three `cargo test --lib` commands, comments for `--lib` only, the pids limit split and the no-`--locked` shortcut; fields `image`, `commands`, `timeout` only (FR-001, FR-002).
- [x] T003 [US1] [US2] Edit `.github/workflows/factory-monorepo-ci.yml`: `pipeline` choice input, `if:` on `validate ref` and `fetch token and clone factory-monorepo`, `DIR` env on `refuse docker steps` and the run step, `PIPELINE` env and summary label, concurrency group with the pipeline, header comment, step renamed `run pipeline` (FR-003 to FR-008).
- [x] T004 [US1] [US2] Run the static checks: YAML parse of the workflow, `bash -n` on each changed `run:` block, grep that no `${{` sits in a `run:` block, TOML parse of `rust-ci-runner/.factory-ci.toml` with only the allowed fields, grep that it has no `docker = true`, and a test of the summary label logic against both pipelines.
- [x] T005 Update `README.md` section "Run factory-monorepo CI by hand": how to start the runner's unit tests (Actions, `factory-monorepo-ci`, Run workflow, `pipeline` = `rust-ci-runner`), that only `--lib` unit tests run, that `ref` is ignored for it. The GitHub repository description needs no change unless it names the workflow. Last task (FR-009).

## Dependencies

T001, then T002, then T003, then T004, then T005.
