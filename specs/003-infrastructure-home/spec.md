# Feature Specification: Infrastructure Home

**Feature Branch**: `003-infrastructure-home`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "The factory CI runner moved from the private factory-monorepo into the public sudtanj/infrastructure repo (main is 6fdfb1a). Make the repo self-consistent after the move, put every GitHub Actions workflow for sudtanj's repos here, run factory-monorepo's checks only as runner pipeline steps, inject the first secrets (Bitwarden, Grafana and Loki) from GitHub Actions secrets, do the first compile that 002 never had, pin ubuntu-latest to ubuntu-24.04, and tell the owner in the README exactly what to set by hand."

## Clarifications

### Session 2026-11-27

Answers come from the lead's brief. Nothing blocked on the user; open questions are listed under Open Questions.

- Q: Where do factory-monorepo's build, lint and test checks run? → A: Only as runner pipeline steps declared in `.factory-ci.toml` files in that repo: one at the root for repo-wide checks, one in each top-level project folder. That repo holds no GitHub Actions workflow (its constitution v1.2.0, Principle VII). Every workflow for sudtanj's repos lives here (FR-017).
- Q: Where do the first secrets come from? → A: GitHub Actions secrets of this repo, set in its Environments: the Bitwarden access tokens and the Grafana and Loki log credentials. `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` move out of Bitwarden (FR-010). Pipeline secrets stay in Bitwarden only, with no GitHub-secret fallback (002 decision D-b, FR-013).
- Q: What format do pipelines use? → A: TOML, in `.factory-ci.toml` (FR-018).
- Q: May a workflow run cargo directly? → A: Only the selftest and release workflows, because no runner exists yet to build the first one. The README, both workflow headers and `plan.md` state this exception (FR-020).
- Q: Which image does the tick pull, and must it be public? → A: `ghcr.io/sudtanj/rust-ci-runner`, published from this repo. The package must be public or the tick cannot pull it (FR-004).

- Clarify pass: the coverage scan found no question that the brief did not answer. Non-blocking questions are recorded under Open Questions with defaults.

### Review round 1, 2026-11-27 (lead decisions after the critic, devops and security reviews)

- D1: `rust-version` is `1.90`, the value of the toolchain pin. The owner asked for the field in 002 T081; 1.82 was never checked against the dependencies (FR-007).
- D2: The temporary first-compile job lives in its own workflow `factory-ci-bootstrap.yml`, runs only for same-repo pull requests, and is deleted before merge. The selftest never uploads artifacts, reads a secret or names an Environment (FR-009, FR-029).
- D3: Required checks on `main` are the selftest jobs only. The runner's `factory-ci*` statuses stay informational (README, FR-022).
- D4: The selftest also builds the release binaries for both musl targets and builds the image without pushing it, so a release problem shows before the tag (FR-028).
- D5: Tests that need a real container engine carry `#[ignore]`; the runner pipeline runs `cargo test --locked`; the selftest host job adds `-- --include-ignored` (FR-030).
- D6: A watched repo with no `.factory-ci.toml` is skipped by the plan: no status, no job (checked in `plan.rs`). The README says so. factory-monorepo's root file is on its PR 2 and not on `main` yet, so the live check waits for that PR (SC-003).
- D7: The two Loki tokens: `factory-ci-docker` holds its own `FACTORY_CI_LOG_TOKEN` (FR-010).
- D8: Release publish and image mirror run in a fourth Environment, `factory-ci-release`, with no secrets (FR-033).
- D9: The compile loop has a budget of 5 selftest runs, then the engineer stops and the plan is revisited (FR-008).

### Review round 2, 2026-11-27 (lead decisions after the PR review)

- R2-1: The tick `verdict` job pings `<url>/fail` on a red tick, a failed or cancelled plan, or `fail_tick=1`, and the plain URL otherwise. QA covered it in commit aa6a307 (FR-034).
- R2-2: GitHub never lets an author approve their own pull request, and only the owner has write access, so only the owner can merge. The `main` ruleset needs a pull request, 0 approvals, the selftest jobs as required checks, no force push, no deletion and an empty bypass list. CODEOWNERS and code-owner review are dropped. The selftest is the gate. The tag ruleset stays (FR-022).
- R2-3: The workflow lint stays line-based; a YAML parser stays deferred. It gets stricter rules for the tick `on:` block, the file extensions it reads, the `uses:` pin and `permissions` rules, an `environment:` allowlist, an end-anchored main guard and `secrets: inherit` in the selftest (FR-035).
- R2-4: Every guard on `FACTORY_CI_BLOB_KEY*` requires 64 lowercase hex characters, the form `openssl rand -hex 32` gives, not only a length (FR-036).
- R2-5: The plan's phase A runs without the Loki credentials, because phase A never ships logs (FR-037).
- R2-6: The plan's store check runs only when at least one candidate row can use a secret. A Bitwarden outage then no longer fails a tick that needs no secret (FR-038).
- R2-7: `capture()` in `engine.rs` loses its dead `container` parameter (FR-008, T043).
- R2-8: `rust-ci-runner/.factory-ci.toml` runs `cargo test` in three processes because of the pids limit; the contract and T020 say so (FR-019, T044). The compile-loop budget of D9 was extended to 8 runs; T005 records it.
- R2-9: Deferred and recorded in `plan.md` Open risks, no task: the token split and the `alpine/git` clone container (security M3), the secret-name charset and reserved shell names (L3), the release verify job's dependence on the tag ruleset (L4), the concurrency slot shared with non-main dispatch (L6), `stop_and_remove` not strictly bounded at 30 s (reviewer m3), YAML escape sequences in triggers (covered only by the owner-only merge rule).
- R2-10: The owner asked that the repository present itself as the open-source home of their infrastructure and DevOps tools, not only of the CI runner. The README intro, the constitution preamble (PATCH 1.0.2, wording only) and the GitHub repository description say so (FR-039).
- R2-11: The owner asked the README to tell the public that these tools are tailored to the owner's needs and may not fit every use case (FR-040).
- R2-12: The README still said blob keys need "32 characters or more"; FR-036 requires 64 lower-case hex characters. The README follows FR-036, and a docs-test row fails on the old wording (FR-041).
- R2-13: The lint `permissions` rule only checked that the key exists, so `write-all` and `read-all` passed. It now requires `{}` or a block mapping of `scope: read|write|none` lines, at workflow level and job level (FR-042).
- R2-14: The `blob-key-length` rule passed when one of the two guards was deleted from the plan job. It now needs both blob-key variable names guarded (FR-043).
- R2-15: Flow-style (`- {uses: x@v4}`) and quoted (`"uses": x@v4`, `'runs-on': x`) keys slipped past the pin and runner rules. The line regex now reads both. A full YAML parse stays deferred (FR-044).
- R2-16: The `main` guard check for release jobs read only the mirror workflow by name. It now covers every workflow with a job in `factory-ci-release`; the release workflow's publish job keeps its tag-ref guard (FR-045).
- R2-17: Nothing checked that `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER` differ. The plan guard step and `factory-ci-plan.sh` fail when both are set and equal (FR-046).
- R2-18: The label "security L1" named two findings. The deferred one (a test that fetched file content never reaches the summary `problems` text) is now "security L7". QA's residual risk is recorded in `plan.md` Open risks as R-13: a pipeline that names owner-scoped secrets on a row with empty `repo_id` skips the early store check; the job still refuses. No code change.

### What this feature changes in 002

| 002 item | Now |
| --- | --- |
| A1 (central repo B is the monorepo or a public copy) | B is this repo. The "separate public copy" step is gone. |
| FR-029, FR-040 last sentence, US8 scenario 3 (log credentials in the Bitwarden default project) | They come from Environment secrets (FR-010). |
| FR-045 (the plan checks the store and that the three log values are all present or all absent) | The store check stays, but runs only when a candidate row can use a secret (FR-038). The all-or-none check on the log values moves to the start of the plan and reads the environment (FR-011). |
| Open tasks T045, T046, T054, T065, T081 | Done or tracked here (FR-025). |

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The repo reads as one place after the move (Priority: P1)

The owner opens the repo and finds no leftover from the monorepo: no "do not use the monorepo", no "B" for the central repo, no path that only existed there. The watch list names the monorepo and this repo. Every workflow runs on `ubuntu-24.04`.

**Why this priority**: Stale wording sends the owner through a setup that no longer exists (for example, "make a separate public copy"). A drifting runner image breaks the tick without a change here.

**Independent Test**: Run the shell test that greps tracked files for stale wording and for `ubuntu-latest`, and `plan --check` on the watch list.

**Acceptance Scenarios**:

1. **Given** any tracked file outside `specs/`, `.specify/` and `.claude/`, **When** the stale-wording test runs, **Then** it finds no bare "monorepo" (only the full name `factory-monorepo`), no "B" as the name of the central repo and no "separate public copy".
2. **Given** `.github/factory-ci-repos.toml`, **When** `rust-ci-runner plan --repos .github/factory-ci-repos.toml --check` runs, **Then** it passes and the file lists `sudtanj/factory-monorepo` and `sudtanj/infrastructure`.
3. **Given** every file in `.github/workflows/`, **When** the lint runs, **Then** every `runs-on` is `ubuntu-24.04`, and a copy with `ubuntu-latest` fails the lint.
4. **Given** the tick cannot pull `ghcr.io/sudtanj/rust-ci-runner@<digest>` because the package is private, **When** the fetch step fails, **Then** the log says in fixed text that the package must be public, and shows no other detail.

---

### User Story 2 - The first compile and a green selftest (Priority: P1)

The crate has never compiled. The pull request for this feature triggers `factory-ci-selftest.yml` (it runs on every pull request). That run is the first compile. The same pull request commits `Cargo.lock`, builds with `--locked`, formats the code and declares `rust-version = "1.90"`.

**Why this priority**: Nothing else ships while the code does not build. 002 T045 and T081 waited for this.

**Independent Test**: The selftest workflow is green on the pull request: build, format check, clippy, unit and integration tests, fixture pipeline, watch-list check, workflow lint and shell tests.

**Acceptance Scenarios**:

1. **Given** no `Cargo.lock` in the repo and no local cargo, **When** the temporary bootstrap workflow runs on a same-repo pull request, **Then** it uploads `Cargo.lock` and a format patch of `.rs` files as an artifact, and the engineer commits both after reading the diff.
2. **Given** `Cargo.lock` committed, **When** the Makefile, the selftest and the release workflow run cargo, **Then** each call carries `--locked`, and a stale lock fails the build.
3. **Given** the bootstrap workflow is no longer needed, **When** the lock and the patch are committed, **Then** the workflow file is deleted in the same pull request.
4. **Given** a compile or test failure in the first run, **When** the engineer reads the public run log, **Then** the fix lands at its source, never by loosening a test. The loop has a budget of 5 runs; a fast `cargo check --locked --all-targets` job gives the compile errors first.
5. **Given** `rust-ci-runner/Cargo.toml`, **When** it is read, **Then** it holds `rust-version = "1.90"`.
6. **Given** the selftest, **When** it runs, **Then** it also builds both musl release binaries with the release workflow's linker settings and builds the image without pushing it.

---

### User Story 3 - Bootstrap secrets come from Actions secrets (Priority: P1)

The owner sets the Bitwarden access tokens and the three log credentials as GitHub Actions secrets in the Environments. Log shipping works without any Bitwarden call. Pipeline secrets still come from Bitwarden.

**Why this priority**: Today the log credentials sit in Bitwarden, so a tick for a private repo needs a Bitwarden round trip before it can ship one line, and the README tells the owner something the constitution (Principle II) already contradicts.

**Independent Test**: Shell tests with a fake `bws` and sentinel values: all three set, none, one or two; short lines; no Bitwarden call for a job with no pipeline secret; no sentinel in any output.

**Acceptance Scenarios**:

1. **Given** all three `FACTORY_CI_LOG_*` secrets set in an Environment, **When** the plan or a job starts, **Then** log shipping is on, the values are masked, and no Bitwarden call is made to get them.
2. **Given** none of the three set, **When** the plan runs, **Then** shipping is off; a private watched repo is refused (fail closed, 002 FR-049).
3. **Given** one or two of the three set, **When** the plan starts, **Then** the tick fails before any repo is checked and names the missing names; a job that starts anyway refuses with "log credentials incomplete".
4. **Given** a pipeline that lists no secret, **When** its job runs, **Then** the job makes no Bitwarden call, even when `BWS_ACCESS_TOKEN` is set.
5. **Given** a pipeline that lists a secret and a store that has it, **When** the job runs, **Then** the value comes from Bitwarden and from nowhere else; with `BWS_ACCESS_TOKEN` unset the item ends in `error` ("no secret store configured").
6. **Given** a log credential line shorter than 4 characters, **When** the plan or a job reads it, **Then** it is refused, as for pipeline secrets (002 FR-053).
7. **Given** sentinel values in all bootstrap secrets, **When** the plan and job scripts run, **Then** no sentinel appears in stdout, stderr, the step summary or the step outputs, except as `::add-mask::` lines.

---

### User Story 4 - factory-monorepo is checked only through pipeline files (Priority: P1)

The runner watches `sudtanj/factory-monorepo` (private). Its build, lint and test checks are steps in `.factory-ci.toml`: a root file for repo-wide checks and one file in each top-level project folder. No Actions workflow in either repo builds or tests it.

**Why this priority**: One owner of automation (constitution Principle I). The private repo's step output goes only to Loki (Principle III).

**Independent Test**: The watch entry validates; the contract in `contracts/pipeline-files.md` states the file layout; the tick run in the quickstart posts `factory-ci` and `factory-ci/<folder>` statuses on the monorepo.

**Acceptance Scenarios**:

1. **Given** the watch list, **When** the plan reads the monorepo entry, **Then** the repo is treated as private, which needs log shipping, and runs without `allow_docker`.
2. **Given** a monorepo with a root `.factory-ci.toml` and one in each of `a/` and `b/`, **When** only `a/` changes, **Then** the root project and `a` run and `b` gets "unchanged since" (002 US2, US3).
3. **Given** this repo, **When** its workflows are listed, **Then** none checks out or builds the monorepo.
4. **Given** a watched repo with no `.factory-ci.toml` (factory-monorepo until its PR 2 merges), **When** a tick runs, **Then** the plan skips it: no status, no job. The README says so.

---

### User Story 5 - The runner tests itself (Priority: P2)

Once running, the runner builds, lints and tests itself through `rust-ci-runner/.factory-ci.toml`, and this repo's scripts and workflows through a root `.factory-ci.toml`. The selftest and release workflows keep running cargo directly: no runner exists yet to build the first one.

**Why this priority**: Principle V wants every check in the runner's own pipeline or this repo's workflows. The bootstrap exception is named so nobody copies it.

**Independent Test**: The selftest runs both pipeline files with the freshly built runner (`rust-ci-runner run --dir ...`) before merge.

**Acceptance Scenarios**:

1. **Given** `rust-ci-runner/.factory-ci.toml`, **When** the runner runs it, **Then** format, clippy and `cargo test --locked` pass; the tests that need a real container engine carry `#[ignore]` and run only in the selftest host job with `-- --include-ignored`.
2. **Given** the root `.factory-ci.toml`, **When** the runner runs it in its container image, **Then** the shell tests and the workflow lint pass, including the git calls the tests make.
3. **Given** the README, **When** the owner looks for the bootstrap exception, **Then** it says: the selftest and release workflows run cargo directly, and no other workflow may.

---

### User Story 6 - The README says what to do by hand (Priority: P2)

The README Setup section lists which secrets and variables go in which Environment, and the manual steps: repository rules, create the Environments, set the secrets, make the GHCR packages public, set up the scheduler that calls the tick, create the Bitwarden machine accounts, and the rollback.

**Why this priority**: A public repo shows its whole setup (Principle II). The owner cannot script these steps, so the list must be exact.

**Independent Test**: A shell test compares the names the workflows read with the names the README lists, both ways.

**Acceptance Scenarios**:

1. **Given** the workflows, **When** every `secrets.NAME` and `vars.NAME` is extracted (except `GITHUB_TOKEN`), **Then** each appears in the README Setup tables with its Environment, and each README name is read by a workflow.
2. **Given** a workflow change that reads a new secret, **When** the README is not updated, **Then** the test fails.
3. **Given** the Setup section, **When** the owner follows it from top to bottom, **Then** the order works: repository rules, Environments, Bitwarden, Grafana, tokens, secrets, merge, release, make public, variables, mirror, scheduler, first tick, and only then deletion of the old Bitwarden log keys.
4. **Given** every `environment:` name in the workflows, **When** compared with the README, **Then** the sets are equal.

---

### User Story 7 - The live checks of 002 run (Priority: P3)

After the owner finishes Setup, the live checks that 002 could not run (T046, T054, T065) run against the real services.

**Why this priority**: They need the owner's accounts, so they come last and stay blocked until Setup is done.

**Independent Test**: The quickstart checks listed in `quickstart.md`.

**Acceptance Scenarios**:

1. **Given** Setup done, **When** the 002 quickstart checks run, **Then** results are recorded in 003 `research.md`.
2. **Given** the pinned `BWS_SHA256`, **When** the selftest runs, **Then** the download of the pinned `bws` archive matches it and the lint runs with `FACTORY_CI_LINT_STRICT=1`.

---

### Edge Cases

- A second `Cargo.lock` appears from a dependency change without `--locked`: the next build fails on a stale lock, which is the intent.
- The first compile has many errors: each push of the loop costs one public run; no local build exists (1 GB of RAM). After 5 runs the engineer stops and the plan is revisited.
- A fork's pull request runs the bootstrap workflow: it does not run (same-repo pull requests only), so a fork cannot produce the artifact the engineer commits.
- The first push to GHCR makes a private package: the tick cannot pull it until the owner switches it to public.
- A watched repo has no `.factory-ci.toml`: the plan skips it silently.
- The Bitwarden server URL variable does not start with `https://`: the tick refuses it before any download or call.
- The Environments do not exist when the tick first runs: GitHub creates them without a branch rule and without secrets, and the plan fails its guard. The README says to create them first and restrict them to `main`.
- A log secret is set in the plan Environment but missing in `factory-ci-docker`: the docker job refuses with "log credentials incomplete" (an `error` status with one retry).
- The private monorepo entry and no log credentials: the plan refuses the repo and fails the tick.
- A fork's pull request runs the selftest: it gets no secrets and `contents: read` only.
- A workflow dispatched from a branch other than `main`: the Environment refuses the job, `plan` does not run, and `verdict` ends red. No secret is read.
- Bitwarden is down and no candidate row can use a secret: the plan makes no store call and the tick runs (FR-038).
- A blob key of 64 characters that is not lower-case hex: every guard refuses it (FR-036).
- A plan or run job fails or is cancelled: the heartbeat pings `/fail` and the check alerts at once; ticks that stop make it late (FR-034).
- `workflow_dispatch` token for the scheduler leaks: it can only start ticks; the tick still reads secrets only on `main`.

## Requirements *(mandatory)*

### Functional Requirements

**Self-consistency**

- **FR-001**: No tracked file outside `specs/`, `.specify/` and `.claude/` MUST contain a bare "monorepo" (only the full name `factory-monorepo`), name the central repo "B", or tell the owner to make a separate public copy. A shell test MUST enforce it.
- **FR-002**: `.github/factory-ci-repos.toml` MUST list `sudtanj/factory-monorepo` and `sudtanj/infrastructure`, and pass `rust-ci-runner plan --repos ... --check`. Neither entry sets `allow_docker`.
- **FR-003**: Every `runs-on` in every workflow under `.github/workflows/` MUST be `ubuntu-24.04`. The workflow lint MUST fail a copy that uses `ubuntu-latest` or any other label, for every workflow file in the directory.
- **FR-004**: The runner image MUST stay `ghcr.io/sudtanj/rust-ci-runner`, published by the release workflow from this repo. The README MUST say the package must be public. When the tick cannot pull it, the fetch step MUST print one fixed line saying so.
- **FR-005**: Docs, comments, scripts and lint rules MUST use this repo's real paths and say "this repo", not "B" or the monorepo.

**First compile**

- **FR-006**: `rust-ci-runner/Cargo.lock` MUST be committed, and every `cargo build`, `test`, `clippy` and `fetch` call in the Makefile, the selftest and release workflows and the pipeline files MUST use `--locked` where the subcommand accepts it.
- **FR-007**: `cargo fmt --check` MUST pass, and `rust-ci-runner/Cargo.toml` MUST hold `rust-version = "1.90"`, the value of the toolchain pin (D1).
- **FR-008**: The selftest workflow MUST pass on the pull request that implements this feature, covering a fast `cargo check --locked --all-targets` job, build, format check, clippy, tests, the fixture pipeline, the watch-list check, the workflow lint and the shell tests. The compile-fix loop has a budget of 5 runs; after that the plan is revisited.
- **FR-009**: The lock file and the format patch MUST come from a temporary workflow `factory-ci-bootstrap.yml`, because nothing can run cargo locally. It MUST run only for same-repo pull requests; run `cargo generate-lockfile` only when no lock exists, with `rust-version` already set and `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback`; format with `cargo fmt || true`; limit the patch to `rust-ci-runner/*.rs`; upload only `rust-ci-runner/Cargo.lock` and `fmt.patch` (no hidden files, 3-day retention, `if: always()`) with the already pinned `actions/upload-artifact`. The engineer MUST download only from the run of the same pull request, apply with `git apply --include='rust-ci-runner/*'`, and read the diff before committing. The workflow file MUST be deleted before merge.

**Bootstrap secrets**

- **FR-010**: Log credentials MUST come from the Environment secrets `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, set in `factory-ci-plan`, `factory-ci` and `factory-ci-docker`. `factory-ci-docker` holds its own `FACTORY_CI_LOG_TOKEN` (a separate Grafana access policy token, so the docker Environment can be cut off alone); URL and user are the same values. No script MUST read them from Bitwarden.
- **FR-011**: All three set means shipping is on, none means off, and a partial set MUST fail the tick at the start of the plan (before any GitHub read), naming the missing names. A job given a partial set MUST refuse before it runs a step.
- **FR-012**: Each log credential MUST be masked before use, and a value with a trimmed line under 4 characters MUST be refused. The three values MUST NOT reach any step container.
- **FR-013**: Pipeline secrets MUST come from Bitwarden only, with no GitHub-secret fallback. A job MUST make a Bitwarden call only when its pipeline lists at least one secret. An idle tick makes no Bitwarden call, and the plan's store check follows FR-038.
- **FR-014**: The three `FACTORY_CI_LOG_*` names and `BWS_ACCESS_TOKEN` stay reserved: a pipeline that lists them is rejected before any container starts.
- **FR-015**: No bootstrap secret value, no private repo name detail, branch, SHA, project path or secret name MUST appear in public logs, summaries, statuses, artifacts or outputs (002 FR-059). A test with sentinel values MUST prove it for the plan and job scripts.
- **FR-016**: The README MUST say the `FACTORY_CI_TOKEN` fine-grained token covers exactly the watched repos and holds only Contents read, Commit statuses read and write, and Metadata read. The runner uses one token for the clone and the statuses; splitting it is deferred (plan, Open risks).

**Pipeline files**

- **FR-017**: factory-monorepo's build, lint and test MUST run only as runner pipeline steps from `.factory-ci.toml` files: one at its root for repo-wide checks, one in each top-level project folder. No Actions workflow, here or there, builds or tests that repo. A watched repo with no `.factory-ci.toml` is skipped by the plan with no status and no job, and the README says so.
- **FR-018**: The pipeline format MUST stay TOML in `.factory-ci.toml`.
- **FR-019**: This repo MUST hold `rust-ci-runner/.factory-ci.toml` (format, lint and `cargo test --locked`) and a root `.factory-ci.toml` (shell tests and workflow lint). The root step's image MUST provide `git` and trust the mounted checkout (`git config --global --add safe.directory '*'`), or the docs test MUST use a fixed file list that excludes `target/`; the `own-pipelines` job proves it.
- **FR-020**: Only the selftest and release workflows MAY run cargo directly, because no runner exists yet to build the first one. The README, the header of each of those two workflows and `plan.md` MUST say so, and a lint rule MUST fail any other workflow that runs `cargo`.
- **FR-021**: Before merge, a selftest job `own-pipelines` MUST run both pipeline files with the freshly built runner (built there or passed as an artifact), with `FACTORY_CI_SHA`, `FACTORY_CI_EVENT` and `FACTORY_CI_BRANCH` set as the fixture step sets them and a separate `CARGO_TARGET_DIR`, so a broken pipeline file never reaches the first tick.

**README and constitution**

- **FR-022**: The root README Setup MUST list every secret and variable with the Environment or scope that holds it, and the manual steps, in the order that works:
  - Repository rules: a `main` ruleset (pull request required, 0 approvals, the selftest jobs as the only required status checks, no force push, no deletion, empty bypass list); no CODEOWNERS file and no code-owner review; a tag ruleset limiting who creates `rust-ci-runner-v*`; Actions settings (approval for all outside collaborators, read-only default workflow token, run retention 7 to 14 days). The README says why: GitHub never lets an author approve their own pull request, only the owner has write access, so only the owner can merge, and the selftest is the gate.
  - Environments: `factory-ci-plan`, `factory-ci`, `factory-ci-docker` limited to `main` with admin bypass off; `factory-ci-release` (no secrets) limited to `main` and the tag pattern `rust-ci-runner-v*`.
  - Accounts: Bitwarden machine accounts (read-only, one project each, token expiry set); a Grafana access policy with `logs:write` only, a usage alert on ingest, one token for the plan and `factory-ci` and a separate one for `factory-ci-docker`, rotated with the move; fine-grained GitHub tokens as in FR-016; blob keys from `openssl rand -hex 32`, rotated with the tokens.
  - After Setup, `gh secret list` at repo and org level shows none of the bootstrap names: they live only in Environments.
  - GHCR: the first push is private; switch it to public in the UI; check with `gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility`; confirm the release digest came from a run on `main`.
  - Scheduler: cron-job.org, `POST` to the dispatch endpoint with headers `Accept: application/vnd.github+json`, `Authorization: Bearer <token>` and `X-GitHub-Api-Version: 2022-11-28`, body `{"ref":"main"}`, expect 204, failure notification on, fine-grained token with Actions write on this repo only, a renewal reminder at day 75.
  - Heartbeat: healthchecks.io with a grace of 60 minutes or more, pinged at `/fail` when the plan failed or the tick is red (FR-034). The README says a red tick or a failed plan pings `/fail` and alerts at once, and that a late check means ticks stopped.
  - What a watched repo without `.factory-ci.toml` gets (nothing), and that the old Bitwarden `FACTORY_CI_LOG_*` keys are deleted only after quickstart checks 1 to 5 and 8 pass and some days of green ticks.
  - Rollback: pause the scheduler first, revert the lock and `--locked` together, and write down the previous `RUNNER_IMAGE_DIGEST`; the README lists what a revert does not restore (Environment secrets, the public package, Bitwarden contents, the scheduler token, the Grafana policy).
  - Notes: watch `ubuntu-24.04` deprecation notices; rerun the mirror after a Docker Hub rate limit; keep this repo public or lengthen the tick period (cost).
- **FR-023**: Every `secrets.NAME` and `vars.NAME` a workflow reads (except `GITHUB_TOKEN`) MUST appear in the README Setup tables and every name there MUST be read by a workflow. Every `environment:` name in the workflows MUST appear in the README and the reverse. A shell test MUST enforce both ways.
- **FR-024**: `rust-ci-runner/README.md` MUST point to the root README for setup and keep only runner behaviour, commands and the pipeline format.
- **FR-025**: 002 tasks T045 and T081 MUST be done by this feature, T046, T054 and T065 MUST be tracked here as blocked on Setup, and 002's `tasks.md` MUST point to the 003 task for each.
- **FR-026**: The constitution MUST change by Governance PATCH 1.0.1 so that "repository secrets and variables" also covers this repo's Environment secrets and variables.

- **FR-027**: No workflow in this repo MUST use the `pull_request_target` trigger, in block, list (`on: [pull_request_target]`) or map (`on: {pull_request_target: ...}`) form. The workflow lint MUST fail a copy that does, for every workflow file in the directory. A public repo's fork pull requests then never run with secrets.

**Selftest, release and tick hardening**

- **FR-028**: The selftest MUST build the release binaries for both musl targets with the release workflow's compiler and linker settings and build the image with `docker buildx build` without pushing it. The release workflow MUST run `rustup toolchain install` before `make build`. The release workflow gets no `workflow_dispatch` trigger.
- **FR-029**: The selftest MUST run on every pull request (no `paths:` filter) and on `workflow_dispatch`, have `concurrency: {group: selftest-${{ github.ref }}, cancel-in-progress: true}`, use `persist-credentials: false` on every checkout, and never use `secrets.`, `environment:`, `workflow_run` or `id-token`. The lint MUST fail a copy that does. The selftest MUST lint the whole `.github/workflows/` directory with `FACTORY_CI_LINT_STRICT=1`.
- **FR-030**: Tests that need a real container engine MUST carry `#[ignore]`. The runner pipeline runs `cargo test --locked`; the selftest host job adds `-- --include-ignored`.
- **FR-031**: `FACTORY_CI_BWS_SERVER_URL`, when set, MUST start with `https://`; the tick MUST refuse any other value before it downloads or calls anything.
- **FR-032**: A shell test MUST scan the committed docs (README files, `research.md`, `quickstart.md`) for private identifiers: no run URL of a private repo, no Grafana hostname, no token or key pattern. The live-check results template in `research.md` says so.
- **FR-033**: The release `publish` job and the image mirror job MUST run in the Environment `factory-ci-release`. The mirror job MUST run only when `github.ref == 'refs/heads/main'`.

**Review round 2**

- **FR-034**: The tick `verdict` job MUST ping `<FACTORY_CI_HEARTBEAT_URL>/fail` when the plan result is not `success`, when `fail_tick` is `1`, or when a `run` or `run-docker` result is `failure` or `cancelled`, and the plain URL otherwise. It MUST ping only on `main`, only when the URL is set, with a 10-second limit, and a ping error MUST NOT fail the job. The README Setup step 11 MUST say that a red tick or a failed plan pings `/fail` and alerts at once, and that a late check means ticks stopped. A docs-test row MUST assert that `/fail` appears in the README. Contract: `contracts/tick-workflow.md`.
- **FR-035**: The workflow lint MUST stay line-based and MUST add these rules, each with a mutation row in `--self-test`:
  - The tick's `on:` block has `workflow_dispatch:` as its only key. Any other key indented by 2 spaces under `on:`, quoted or not, fails, and so does a flow-style `on:` in the tick.
  - The lint loop covers `*.yml` and `*.yaml`.
  - The 40-hex `uses:` pin rule and the `permissions` present rule run on every workflow file.
  - The tick's `environment:` value per job is from an allowlist: `plan` and `verdict` use `factory-ci-plan`, `run` uses `factory-ci`, `run-docker` uses `factory-ci-docker`.
  - The main guard regex is end-anchored: `if: github.ref == 'refs/heads/main'( && .*)?$`, with no `||`.
  - The selftest rule also rejects `secrets: inherit` and `secrets[`.
- **FR-036**: Every guard on `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER` (the tick guard steps, `fci_blob_encrypt` and `fci_blob_decrypt`, and the scripts that call them) MUST require `^[0-9a-f]{64}$`, not only a length of 32 or more. The README already says the keys come from `openssl rand -hex 32`. Shell tests MUST cover a short key, a 64-character non-hex key, an upper-case hex key and a valid key.
- **FR-037**: The plan script MUST call the plan binary for phase A with `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` removed from its environment. Phase A (`plan::candidates`) only reads and never ships logs (checked in `rust-ci-runner/src/plan.rs`); the log line is sent from phase C (`plan::finish`), so the credentials are removed for phase A only. A test MUST prove phase A sees none of the three names and phase C sees all three.
- **FR-038**: The plan MUST check the Bitwarden store (`fci_list`) only when at least one candidate row can use a secret: its `secrets_ok` is true and it has shared secret names (`secrets` not empty) or an owner id (`repo_id` not empty). A Bitwarden outage then does not fail a tick whose rows list no secret. The purpose of the check stays (D1): a revoked or wrong token fails one tick early instead of one `error` status per item.
- **FR-039**: The first paragraph of the root `README.md`, the preamble of `.specify/memory/constitution.md` and the GitHub repository description MUST say that this repository is the open-source (MIT) home of sudtanj's infrastructure and DevOps tools, and that it holds the factory CI runner and every GitHub Actions workflow today.
- **FR-040**: The root `README.md` MUST say, right after its first paragraph, that the tools are tailored to the owner's repositories and needs, may not fit every use case, and come with no promise of support or stable interfaces.
- **FR-041**: The root `README.md` MUST say the blob keys are 64 lower-case hex characters from `openssl rand -hex 32` everywhere it mentions their form, and MUST NOT contain "32 characters". A docs-test row MUST fail on that phrase.
- **FR-042**: The lint `permissions` rule MUST accept only `permissions: {}` or a block mapping (the key alone on its line, followed by indented `scope: read|write|none` lines) at workflow level in every workflow file, and MUST reject `write-all` and `read-all` at workflow level and job level. Mutation rows for both values.
- **FR-043**: The lint `blob-key-length` rule MUST require a 64-hex guard for each of `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER` in the plan job. A mutation row deletes the `_DOCKER` guard.
- **FR-044**: The lint `uses:` pin and `runs-on:` rules MUST also read keys with optional quotes and a leading `{` or `- {`. Mutation rows for one flow-style and one quoted form. A full YAML parse stays deferred.
- **FR-045**: The lint MUST apply the `main` guard check to every scanned workflow with a job that has `environment: factory-ci-release`. Accepted guards: the `if: github.ref == 'refs/heads/main'( && .*)?$` form, or, for a workflow triggered only by a tag push, the `verify` job check `git merge-base --is-ancestor "$GITHUB_SHA" origin/main` (the form `rust-ci-runner-release.yml` uses today, T051). A mutation row copies the mirror workflow under another name without the guard and must fail `main-guard`.
- **FR-046**: When `FACTORY_CI_BLOB_KEY_DOCKER` is set and equal to `FACTORY_CI_BLOB_KEY`, the tick plan guard step and `factory-ci-plan.sh` MUST fail with a message naming the two variables, never their values. An unset docker key passes. The README Setup secrets step MUST say the two keys differ.

### Key Entities

- **Environment**: a GitHub Environment of this repo. `factory-ci-plan`, `factory-ci` and `factory-ci-docker` are limited to `main` and hold bootstrap secrets. `factory-ci-release` holds none and is limited to `main` and the release tag pattern.
- **Bootstrap secret**: an Actions secret the runner needs before it can read Bitwarden: tokens, blob keys, log credentials, heartbeat URL.
- **Repository variable**: a non-secret Actions variable: image digests, Bitwarden project IDs, status link.
- **Pipeline file**: a `.factory-ci.toml` at a repo root or in a top-level folder.
- **Watch entry**: one `[[watch]]` table in `.github/factory-ci-repos.toml`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The selftest workflow is green on the pull request, and the pull request holds `Cargo.lock`.
- **SC-002**: The stale-wording test, the `ubuntu-24.04` lint and the README-versus-workflows test pass in the shell job, and each fails on a mutated copy.
- **SC-003**: After Setup, one manual tick posts a `factory-ci` status on `sudtanj/infrastructure` within 15 minutes. The same holds for `sudtanj/factory-monorepo` once its PR 2 (the root `.factory-ci.toml`) is merged; until then the plan skips that repo and the check waits.
- **SC-004**: Once factory-monorepo PR 2 is merged, a line a monorepo step prints is searchable in Loki within 30 seconds, and a signed-out reader of the public run log finds none of: a log credential, a branch, a SHA, a project path, a secret name.
- **SC-005**: With no Bitwarden token set, a pipeline that lists no secret still runs and ships logs.
- **SC-006**: Setup takes the owner one pass through the README, with no step missing (checked by the quickstart).

## Open Questions

None block the plan. Each has a default the plan uses.

- **OQ-4 (variables scope)**: Repository variables or per-Environment variables? Default: repository variables, because none is secret.
- **OQ-5 (selftest duplicates)**: Drop the selftest's shell and lint jobs once the runner runs the same checks? Default: keep them until the runner has passed 10 ticks; a later feature removes them.

Closed by the lead (review round 1): OQ-1 scheduler is cron-job.org; OQ-2 `allow_docker` is off on both entries (tests that need an engine are `#[ignore]`d, FR-030); OQ-3 is two Loki tokens, one for `factory-ci-docker` (FR-010); OQ-6 is selftest jobs only as required checks (D3).

## Assumptions

- This repo is public, Actions are enabled, and `main` is protected (pull request required with 0 approvals, no force push, no deletion). Only the owner has write access, so only the owner can merge; the selftest is the gate (R2-2).
- Free Actions minutes apply to public repos, so a 5-minute tick costs nothing (002 A25).
- No local `cargo`, `docker`, `make` or test run exists: the VM has 1 GB of RAM. CI is the first compiler.
- The engineer can read public run logs and download artifacts with `gh`.
- factory-monorepo's pipeline files are written in that repo under its own constitution (v1.2.0, Principle VII). This feature records the contract and the watch entry only.
- The package visibility switch and every account step are manual and stay in the README.
- `rust-version = "1.90"` follows the toolchain pin. The owner asked for the field in 002 T081; the earlier value 1.82 was never checked against the dependencies.
- factory-monorepo's root `.factory-ci.toml` is on its PR 2 (branch `001-repo-skeleton-impl`), not on `main`. Until it merges the runner skips that repo.
- One fine-grained token serves the clone and the statuses; splitting it needs a runner change and is deferred.
