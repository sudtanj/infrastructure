# Tasks: Runner image in the pipeline workflow

**Input**: `specs/007-runner-image/` (spec.md, plan.md)

**Tests**: no code under test. T004 runs the static checks and the shell test of the URL normalisation that plan.md names as the test stand-in.

## Phase 1: User Stories 1 to 3 - released runner, probed log store, native release build (Priority: P1)

**Goal**: the workflow copies the runner out of the released image and checks the Loki settings before the runner starts.

**Independent Test**: T004 passes; the owner's first run, after filling in the digest, pulls the image and prints `log store answered HTTP 204`.

- [x] T001 [US1] Edit `.github/workflows/factory-monorepo-ci.yml`: top-level `env: RUNNER_IMAGE` with `DIGEST_PLACEHOLDER` and the pin comment, `packages: read` on the job, step `fetch rust-ci-runner` replacing `build rust-ci-runner` (login over stdin, pull, create, cp, rm, chmod), run step calls `$RUNNER_TEMP/bin/rust-ci-runner` (FR-001 to FR-004, FR-008).
- [x] T002 [US2] Edit the run step in `.github/workflows/factory-monorepo-ci.yml`: strip trailing `/` and `/loki/api/v1/push` from `FACTORY_CI_LOG_URL`, export it, probe with `jq -nc` body and `curl -K -`, print `log store answered HTTP <code>`, fail on non-2xx with the fixed hint (FR-005 to FR-007).
- [x] T003 [US1] Rewrite the header comment of `.github/workflows/factory-monorepo-ci.yml`: runner from the released image, only `rust-ci-runner-release.yml` builds it (FR-010).
- [x] T004 [US1] [US2] Run the static checks: YAML parse of both workflows, `bash -n` on each changed `run:` block, grep that no `${{` sits in a `run:` block, grep that `cargo` is absent and `DIGEST_PLACEHOLDER` appears once, a shell test of the URL normalisation on four inputs, and a check that no secret value is in argv or printed.
- [x] T005 [US3] Edit `.github/workflows/rust-ci-runner-release.yml`, job `build`: `runs-on: ${{ matrix.runner }}`, `runner` per matrix entry (`ubuntu-24.04`, `ubuntu-24.04-arm`), `musl-gcc` and `musl-tools` for both, comment why; check `rust-ci-runner/rust-toolchain.toml` lists both musl targets (FR-012).
- [x] T006 Update `README.md` section "Run factory-monorepo CI by hand" and, if it says how CI builds the runner, `rust-ci-runner/README.md`: the runner comes from the released image pinned in `RUNNER_IMAGE`, bump it after each release, the Loki probe and its fixed hint, the retired build exception. State whether the GitHub repository description changes. Last task (FR-011).

## Dependencies

T001, then T002, then T003, then T005, then T004, then T006.
