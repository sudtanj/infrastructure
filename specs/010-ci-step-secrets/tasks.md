---

description: "Task list for step secrets in the ci command"
---

# Tasks: Step secrets for the ci command

**Input**: `specs/010-ci-step-secrets/` (spec.md, plan.md, research.md, data-model.md, contracts/ci-secrets.md, quickstart.md)

**Prerequisites**: plan.md, spec.md. The full brief of each task is in plan.md under "Engineer briefs" (T008 to T027) and "Test plan" (T001 to T007); IDs match.

**Tests**: required (constitution Principle V, with the justified exception recorded in plan.md). They are written first; the branch selftest in T018 is where "green only with the code" is checked.

**Rules for everyone**:
- No cargo, docker, builds or tests on the agent VM (1 GB); tests run in the selftest and the `rust-ci-runner` pipeline. Git and file reads only. Do not run `.github/scripts/test-factory-monorepo-ci.sh` by hand; the selftest runs it.
- Never write a secret value anywhere, in files, logs, commits or messages.
- No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or any Environment secret, or creates or reads Bitwarden keys. Debugging uses the Actions log and Loki only. Every Bitwarden action is the owner's.
- `redact.rs` and `watch.rs` must not mention `bws` or `bitwarden` (`rust-ci-runner/tests/no_vendor.rs`).
- Nothing that holds a token or a secret value derives `Debug`. No `#[allow(clippy::...)]`.
- `git add` with explicit paths. Never force-push. Code lands only through T018.
- 010 merges before 011 (shared files: `Cargo.toml`, `log.rs` `FAULTS`, `ci.rs`, READMEs).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 run with a secret, US2 allowlist and ref gate, US3 missing or bad value stops early, US4 nothing leaks

## Phase 1: Tests first (shared by all stories, no story label)

- [ ] T001 [P] Write the keyed-lookup unit tests of plan.md "Test plan" T001 (FR-001, FR-003, FR-005 to FR-007, FR-018: `Held::take` exact key, found 0 and 2, control characters, line boundary, blank inner line, trailing newline, surrounding whitespace, 32 KiB cap, lazy validation, `fetch_with`, `Debug` and `Display` without value or name) in `rust-ci-runner/src/store_tests.rs`
- [ ] T002 [P] Write the pipeline helper tests of T002 (FR-001 to FR-004, FR-007, FR-009: `unlisted`, `secret_names`, any-docker rule with two separate steps, four runner settings) in the `tests` module of `rust-ci-runner/src/pipeline.rs`
- [ ] T003 [P] Write the watch tests of T003 (FR-004 four names; FR-010 `secrets_allowed_on` with `branches` omitted, `["*"]`, `["?*"]`, `["*?"]`, `["main"]`, `["release/*"]`, `secret_branches` matching, not matching, `["*"]`) in `rust-ci-runner/src/watch_tests.rs`, and the `BW_`, `BITWARDEN` rows in `rust-ci-runner/src/secret_names_tests.rs`
- [ ] T004 [P] Write the redactor and log tests of T004 (FR-011 to FR-013: `new(["pw"])` still masks, `echo "$V" | base64`, `printf 'x%s'` base64, wrapped, URL-safe, unpadded, percent, JSON, hex, floors and caps, batch equals `new`, `forms()`, secret `pass` keeps the public line, `add_secrets` ordering and size state, `FAULTS` membership) in the `tests` modules of `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`
- [ ] T005 Write the `ci::run` integration tests of T005 in a new `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs` (FR-001 to FR-013, US1 to US4, SC-002, SC-003; one `bws` call per run; entries with `branches = ["main"]`); change `declared_secret_fails_before_any_container` and add two rows to `every_stderr_line_is_in_the_fixed_list` in the same file
- [ ] T006 [P] Write the pump tests of T006 (FR-013: value across the 64 KB cut, a 32 KiB value across it, one byte over the cap, order and tail flush, no UTF-8 cut) in the `tests` module of `rust-ci-runner/src/engine.rs`
- [ ] T007 [P] Write `rust-ci-runner/tests/repo_files.rs` (FR-014, FR-016: no secret name in two committed entries; `sudtanj/infrastructure` has no `secrets`; `Cargo.toml` and `Cargo.lock` versions are equal); it reads `../.github` and needs a full checkout, so do not add it to the integration command of `rust-ci-runner/.factory-ci.toml`

## Phase 2: Foundation (T008 first; T009, T010, T013 to T017 in parallel with it; T011 after T008; T012 after T011)

- [ ] T008 [P] [US3] Add `REQUIRED_KEYS` (rename), `fetch_with`, `parse_list_with`, `Fetched`, `Held`, `StepSecrets`, `NamedError` (position-keyed) and the value rule in `rust-ci-runner/src/store.rs`, keeping `fetch` and `parse_list` as they behave today; rule per data-model.md "Step secret values" and "NamedError" (FR-005 to FR-007, FR-018)
- [ ] T009 [P] [US2] Add `Pipeline::secret_names`, `unlisted`, `docker_secret_problems`; reserve `BW_` and `BITWARDEN` in `rust-ci-runner/src/pipeline.rs` (FR-001 to FR-004, FR-007, FR-009)
- [ ] T010 [P] [US2] Add `WatchEntry::secrets_allowed_on` in `rust-ci-runner/src/watch.rs` per data-model.md "Ref gate", treating patterns made only of `*` and `?` as wild (FR-010)
- [ ] T011 [US4] Add `Redactor::add` (batch, forms, floors and caps) and `forms`, and `Log::add_secrets`, `secret_forms`, `longest_secret`, the `RwLock`, public lines from raw event text and the two new `FAULTS` lines, in `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`; uses `REQUIRED_KEYS` from T008 (FR-011 to FR-013)
- [ ] T012 [US4] Carry the boundary tail (`carry` = `Log::longest_secret()`) in `pump` in `rust-ci-runner/src/engine.rs`; needs T011 (FR-013)
- [ ] T013 [P] Make the `list` job of `.github/workflows/factory-ci-selftest.yml` run `cargo test --locked --lib` without `--no-run` and rename the step; raise `timeout-minutes` only if needed
- [ ] T014 [P] Split the `ci_tests::` group in `rust-ci-runner/.factory-ci.toml` (`ci_tests:: -- --skip ci_tests::step_secrets::` and a separate `ci_tests::step_secrets::` command) and correct its header comment about mock-server threads
- [ ] T015 [US1] Wire `rust-ci-runner/src/ci.rs` per plan.md "Order inside `run_checkout`": put `id`, `allowed`, `held`, `ref_may_hold_secrets` into `Args`, call `store::fetch_with` in `job()` with the Bitwarden `token`, no new parameter beyond 7, no `#[allow]`; make `load_pipeline` `pub(crate)` in `rust-ci-runner/src/job.rs`; needs T008 to T012 (FR-001 to FR-013, FR-018)
- [ ] T016 [P] Change the `rust-ci-runner` package version to `0.3.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, by hand, two lines (FR-014)
- [ ] T017 [P] Add `ci-canary/.factory-ci.toml`: one step, image pinned by digest, `commands = ["echo ok"]`, no secrets (canary pipeline)

## Phase 3: Landing (P1 stories US1 to US4 are all carried by T015)

**Goal**: `ci` runs a fixture pipeline with its allowlisted secrets; every refusal stops before any container; nothing leaks.

**Independent test**: the tests of T005 pass in the selftest on the branch.

- [ ] T018 [US1] Land the code: commit T001 to T017 together with messages that link `specs/010-ci-step-secrets/` (no PR is possible, creation returns 403); push the branch; the selftest (lint, full lib tests, integration tests, `plan --check`) must be green; then fast-forward `main` with the same commits in one push

**Checkpoint**: the code is on `main`.

## Phase 4: Release and first use (sequential)

- [ ] T019 [US1] Dispatch the `rust-ci-runner` pipeline on `main`; record its run id and full SHA; confirm the SHA equals `git rev-parse main` (re-run if `main` moved); tag `rust-ci-runner-v0.3.0` on that SHA; wait for `.github/workflows/rust-ci-runner-release.yml`; record the digest from its run summary, no `docker pull` on the agent VM (FR-014)
- [ ] T020 [US1] On a branch push two commits together: add `.github/workflows/factory-monorepo-ci.yml` to both `paths` lists in `.github/workflows/factory-ci-selftest.yml`, then set `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest>`; the selftest job `shell` runs `test-factory-monorepo-ci.sh` and is green; fast-forward `main`; do not bump `RUNNER_IMAGE_DIGEST` (FR-015)
- [ ] T021 [US1] Run `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=ci-canary`; it pulls the image and must end in `[runner] pipeline passed`; otherwise revert the pin (plan.md "Rollback")
- [ ] T022 [US2] Re-check with read-only `gh api`, token with admin scope, run by the owner, before the first secret run (FR-019): Environment `factory-ci` has one custom-branch policy, `main`, and no required reviewer; the only collaborator of both repos is the owner; no Dependabot, Renovate or auto-merge on `factory-monorepo` `main`; the `rust-ci-runner-v*` tag ruleset exists; the GHCR package is readable; the machine account reads one project
- [ ] T023 [US2] Rewrite the header comment of `.github/factory-ci-repos.toml` (`ci` reads `secrets` per repo and applies the FR-010 gate); add to the `sudtanj/factory-monorepo` entry only the non-login names a step needs (none today; the owner names them); never add `secrets` to `sudtanj/infrastructure`; `tests/repo_files.rs` and `plan --check` pass (FR-016)
- [ ] T024 [US4] Canary values run per quickstart.md section 7 (owner creates and deletes the Bitwarden key; a temporary name in the entry and in a `factory-monorepo` step; plain, `echo "$V" | base64` and percent-encoded; check Loki; remove the name) (FR-019)
- [ ] T025 [US1] First real run per quickstart.md section 8, once a real non-login step secret exists; until then 010 is complete without it; logins come from feature 011 (SC-001)
- [ ] T026 [US1] One week after T025 list the runs of `factory-monorepo-ci` with `gh run list` (Actions read is enough; T022 is the task that needs admin scope) and count `step secret not allowed` and `step secret unavailable`; an unexpected one is a README fix (SC-004)

## Final Phase: Docs (last task)

- [ ] T027 Update `README.md` (line 55), `rust-ci-runner/README.md` (the `ci` list, the fault list, Secrets, risks), the GitHub description (`gh repo edit`, needs admin) and add `readme_has_no_stale_secrets_sentence` to `rust-ci-runner/tests/repo_files.rs`, as listed under T027 in plan.md, including the rollback list, the runbook line and the note that `repo_files.rs` needs `../` (FR-017)

## Dependencies

- T001 to T007 first, in parallel; T005 is one file.
- T008, T009, T010, T013 to T017 start together; T011 after T008; T012 after T011; T015 after T008 to T012; T018 after T001 to T017.
- T019 after T018; T020 after T019; T021 after T020; T022 after T021; T023 after T022; T024 after T023 and the owner's canary key; T025 after T024 and a real secret; T026 one week after T025; T027 last.

## Implementation strategy

MVP is T001 to T021: the code, the release and a proof that 0.3.0 runs. Run `/speckit-analyze` before `/speckit-implement`. 27 tasks.
