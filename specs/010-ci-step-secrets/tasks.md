---

description: "Task list for step secrets in the ci command"
---

# Tasks: Step secrets for the ci command

**Input**: `specs/010-ci-step-secrets/` (spec.md, plan.md, research.md, data-model.md, contracts/ci-secrets.md, quickstart.md)

**Prerequisites**: plan.md, spec.md. The full brief of each task is in plan.md under "Engineer briefs" (T008 to T023) and "Test plan" (T001 to T007); IDs match.

**Tests**: required (constitution Principle V). They come first and must fail before the code exists.

**Rules for everyone**:
- No cargo, docker or builds on the agent VM (1 GB); tests run in the `rust-ci-runner` pipeline and the selftest.
- Never write a secret value anywhere, in files, logs, commits or messages.
- No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or any Environment secret, or creates or reads Bitwarden keys. Debugging uses the Actions log and Loki only. Every Bitwarden action is the owner's.
- `redact.rs` and `watch.rs` must not mention `bws` or `bitwarden` (`rust-ci-runner/tests/no_vendor.rs`).
- Nothing that holds a token or a secret value derives `Debug`.
- `git add` with explicit paths. Push the branch only; never force-push; never touch `main` except the fast-forward of T017.

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 run with its secrets, US2 allowlist and ref gate, US3 missing value stops early, US4 nothing leaks

## Phase 1: Tests first (shared by all stories, no story label)

- [ ] T001 [P] Write the named-lookup unit tests of plan.md "Test plan" T001 (FR-001, FR-003, FR-005 to FR-007, FR-018: exact key, counts, control characters, line boundary, blank inner line, 16 MiB, `Debug` and `Display` without value) in `rust-ci-runner/src/store_tests.rs`
- [ ] T002 [P] Write the pipeline helper tests of T002 (FR-001 to FR-004, FR-007, FR-009: `unlisted`, `secret_names`, any-docker rule with two separate steps, four runner settings) in the `tests` module of `rust-ci-runner/src/pipeline.rs`
- [ ] T003 [P] Write the watch tests of T003 (FR-004 four names; FR-010 `secrets_allowed_on` with `branches` omitted, `["*"]`, `["main"]`, `["release/*"]`, `secret_branches` matching, not matching, `["*"]`) in `rust-ci-runner/src/watch_tests.rs`, and the `BW_`, `BITWARDEN` rows in `rust-ci-runner/src/secret_names_tests.rs`
- [ ] T004 [P] Write the redactor and log tests of T004 (FR-011 to FR-013: `echo "$V" | base64`, `printf 'x%s'` base64, wrapped, URL-safe, unpadded, percent, JSON, hex, secret `pass` keeps the public line, `add_secrets` ordering and size state, `FAULTS` 16) in the `tests` modules of `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`
- [ ] T005 Write the `ci::run` integration tests of T005 in a new `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs` (FR-001 to FR-013, US1 to US4, SC-002, SC-003; the second call uses the access token, entries with `branches = ["main"]`); change `declared_secret_fails_before_any_container` and add two rows to `every_stderr_line_is_in_the_fixed_list` in the same file
- [ ] T006 [P] Write the pump tests of T006 (FR-013: value across the 64 KB cut, order and tail flush, no UTF-8 cut) in the `tests` module of `rust-ci-runner/src/engine.rs`
- [ ] T007 [P] Write `rust-ci-runner/tests/watch_file.rs` (FR-016: no secret name in two committed entries; `sudtanj/infrastructure` has no `secrets`); do not add it to the integration command of `rust-ci-runner/.factory-ci.toml`

## Phase 2: Foundation (blocks T013; T008 to T012 are independent of each other)

- [ ] T008 [P] [US4] Add `Redactor::add` with the FR-011 forms and `Log::add_secrets`, `Log::longest_secret`, the `RwLock`, public lines from fixed grammar and the two new `FAULTS` lines, in `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs` (FR-011 to FR-013)
- [ ] T009 [P] [US3] Add `RUNNER_KEYS`, `fetch_named`, `parse_named`, `StepSecrets`, `NamedError` and the value rule in `rust-ci-runner/src/store.rs`, sharing the retry and server check with `fetch`; value rule per data-model.md "Step secret values"; drop the list and rows before returning (FR-005 to FR-007, FR-018)
- [ ] T010 [P] [US2] Add `Pipeline::secret_names`, `unlisted`, `docker_secret_problems`; reserve `BW_` and `BITWARDEN`; refuse `RUNNER_KEYS` names, in `rust-ci-runner/src/pipeline.rs` (FR-001 to FR-004, FR-007, FR-009)
- [ ] T011 [P] [US4] Carry the boundary tail in `pump` in `rust-ci-runner/src/engine.rs` (FR-013)
- [ ] T012 [P] [US2] Add `WatchEntry::secrets_allowed_on` in `rust-ci-runner/src/watch.rs` per data-model.md "Ref gate" (FR-010)

## Phase 3: User Story 1, a pipeline runs with its secrets (P1), carrying US2 to US4

**Goal**: `ci` runs a fixture pipeline with its allowlisted secrets; every refusal stops before any container; nothing leaks.

**Independent test**: the tests of T005 pass in the `rust-ci-runner` pipeline.

- [ ] T013 [US1] Wire `rust-ci-runner/src/ci.rs`: keep the watch entry, build the `fetch` closure in `job()` from the Bitwarden `token` (not `sec.token`), apply the order in plan.md "Order inside `run_checkout`", call `log.add_secrets`, pass the map to `run_pipeline`; make `load_pipeline` `pub(crate)` in `rust-ci-runner/src/job.rs` (FR-001 to FR-013, FR-018)
- [ ] T014 [P] [US1] Split the `ci_tests::` group in `rust-ci-runner/.factory-ci.toml`: `ci_tests:: -- --skip ci_tests::step_secrets::` and a separate `ci_tests::step_secrets::` command; update the header comment
- [ ] T015 [P] [US1] Change the `rust-ci-runner` package version to `0.3.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, by hand, two lines (FR-014)

**Checkpoint**: T001 to T015 are on `main`; `make lint` and the selftest pass; the `rust-ci-runner` pipeline passes on that commit.

## Phase 4: Release and first use (sequential, after the checkpoint)

- [ ] T016 [US1] Dispatch the `rust-ci-runner` pipeline on `main`; record its run id and full SHA; confirm the SHA equals `main` (re-run if `main` moved) and clippy `-D warnings` passed; tag `rust-ci-runner-v0.3.0` on that SHA; wait for `.github/workflows/rust-ci-runner-release.yml`; record and `docker pull` the digest (FR-014)
- [ ] T017 [US1] Commit the `RUNNER_IMAGE` pin (`0.3.0@sha256:<digest>`) in `.github/workflows/factory-monorepo-ci.yml` to a branch; confirm `factory-ci-selftest.yml` runs `.github/scripts/test-factory-monorepo-ci.sh` green; fast-forward `main`; do not bump `RUNNER_IMAGE_DIGEST` (FR-015)
- [ ] T018 [US1] Run `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner` on the new pin; it must end in `[runner] pipeline passed`; otherwise revert the pin (plan.md "Rollback")
- [ ] T019 [US2] Re-check with read-only `gh api` before the first secret run (FR-019): Environment `factory-ci` has one custom-branch policy, `main`, and no required reviewer; the only collaborator of both repos is the owner; no Dependabot, Renovate or auto-merge on `factory-monorepo` `main`; the `rust-ci-runner-v*` tag ruleset exists; the GHCR package is readable; the machine account reads one project
- [ ] T020 [US2] Rewrite the header comment of `.github/factory-ci-repos.toml` (`ci` reads `secrets` per repo and applies the FR-010 gate); add to the `sudtanj/factory-monorepo` entry only non-vault names the browser project needs (none today, or `BROWSER_TEST_URL` if the owner treats it as secret); never add `secrets` to `sudtanj/infrastructure`; `tests/watch_file.rs` and `plan --check` pass (FR-016)
- [ ] T021 [US4] Canary values run per quickstart.md section 7 (owner creates and deletes the Bitwarden key; a temporary name in the entry and in a `factory-monorepo` step; plain, `echo "$V" | base64` and percent-encoded; check Loki; remove the name) (FR-019)
- [ ] T022 [US1] First real run per quickstart.md section 8, once feature 011 or T020 supplies a real step secret; until then 010 is complete without it (SC-001)

## Final Phase: Docs (last task)

- [ ] T023 Update `README.md`, `rust-ci-runner/README.md` and the GitHub description (`gh repo edit`, needs admin) as listed under T023 in plan.md, including the rollback list and the runbook line (FR-017)

## Dependencies

- T001 to T007 first, in parallel; T005 is one file.
- T008 to T012 in parallel after their tests; T013 after all five; T014 and T015 any time.
- T016 after T013 to T015 are on `main`; T017 after T016; T018 after T017; T019 after T018; T020 after T019; T021 after T020 and the owner's canary key; T022 after T021 and a real secret; T023 last.

## Implementation strategy

MVP is T001 to T018: the code, the release and a proof that 0.3.0 runs. Stop after the Phase 3 checkpoint if the code should be reviewed before the release. Run `/speckit-analyze` before `/speckit-implement`.
