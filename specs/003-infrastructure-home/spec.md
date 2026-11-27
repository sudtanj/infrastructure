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

- Clarify pass: the coverage scan found no question that the brief did not answer. Six non-blocking questions are recorded as OQ-1 to OQ-6 with defaults.

### What this feature changes in 002

| 002 item | Now |
| --- | --- |
| A1 (central repo B is the monorepo or a public copy) | B is this repo. The "separate public copy" step is gone. |
| FR-029, FR-040 last sentence, US8 scenario 3 (log credentials in the Bitwarden default project) | They come from Environment secrets (FR-010). |
| FR-045 (the plan checks the store and that the three log values are all present or all absent) | The store check stays. The all-or-none check on the log values moves to the start of the plan and reads the environment (FR-011). |
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

The crate has never compiled. The pull request for this feature triggers `factory-ci-selftest.yml` (it changes `rust-ci-runner/**`). That run is the first compile. The same pull request commits `Cargo.lock`, builds with `--locked`, formats the code and declares `rust-version = "1.82"`.

**Why this priority**: Nothing else ships while the code does not build. 002 T045 and T081 waited for this.

**Independent Test**: The selftest workflow is green on the pull request: build, format check, clippy, unit and integration tests, fixture pipeline, watch-list check, workflow lint and shell tests.

**Acceptance Scenarios**:

1. **Given** no `Cargo.lock` in the repo and no local cargo, **When** a temporary selftest job runs `cargo generate-lockfile` and `cargo fmt`, **Then** it uploads `Cargo.lock` and a format patch as an artifact, and the engineer commits both.
2. **Given** `Cargo.lock` committed, **When** the Makefile, the selftest and the release workflow run cargo, **Then** each call carries `--locked`, and a stale lock fails the build.
3. **Given** the temporary job is no longer needed, **When** the lock and the patch are committed, **Then** the job is removed in the same pull request.
4. **Given** a compile or test failure in the first run, **When** the engineer reads the public run log, **Then** the fix lands at its source, never by loosening a test, and the loop repeats until the run is green.
5. **Given** `rust-ci-runner/Cargo.toml`, **When** it is read, **Then** it holds `rust-version = "1.82"`.

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

---

### User Story 5 - The runner tests itself (Priority: P2)

Once running, the runner builds, lints and tests itself through `rust-ci-runner/.factory-ci.toml`, and this repo's scripts and workflows through a root `.factory-ci.toml`. The selftest and release workflows keep running cargo directly: no runner exists yet to build the first one.

**Why this priority**: Principle V wants every check in the runner's own pipeline or this repo's workflows. The bootstrap exception is named so nobody copies it.

**Independent Test**: The selftest runs both pipeline files with the freshly built runner (`rust-ci-runner run --dir ...`) before merge.

**Acceptance Scenarios**:

1. **Given** `rust-ci-runner/.factory-ci.toml`, **When** the runner runs it, **Then** format, clippy and the tests that need no Docker pass with `--locked`.
2. **Given** the root `.factory-ci.toml`, **When** the runner runs it, **Then** the shell tests and the workflow lint pass.
3. **Given** the README, **When** the owner looks for the bootstrap exception, **Then** it says: the selftest and release workflows run cargo directly, and no other workflow may.

---

### User Story 6 - The README says what to do by hand (Priority: P2)

The README Setup section lists which secrets and variables go in which Environment, and the manual steps: create the Environments, set the secrets, make the GHCR packages public, set up the scheduler that calls the tick, create the Bitwarden machine accounts.

**Why this priority**: A public repo shows its whole setup (Principle II). The owner cannot script these steps, so the list must be exact.

**Independent Test**: A shell test compares the names the workflows read with the names the README lists, both ways.

**Acceptance Scenarios**:

1. **Given** the workflows, **When** every `secrets.NAME` and `vars.NAME` is extracted (except `GITHUB_TOKEN`), **Then** each appears in the README Setup tables with its Environment, and each README name is read by a workflow.
2. **Given** a workflow change that reads a new secret, **When** the README is not updated, **Then** the test fails.
3. **Given** the Setup section, **When** the owner follows it from top to bottom, **Then** the order works: Environments, Bitwarden, Grafana, tokens, secrets, merge, release, make public, variables, mirror, scheduler, first tick.

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
- The first compile has many errors: each push of the loop costs one public run; no local build exists (1 GB of RAM).
- The Environments do not exist when the tick first runs: GitHub creates them without a branch rule and without secrets, and the plan fails its guard. The README says to create them first and restrict them to `main`.
- A log secret is set in the plan Environment but missing in `factory-ci-docker`: the docker job refuses with "log credentials incomplete" (an `error` status with one retry).
- The private monorepo entry and no log credentials: the plan refuses the repo and fails the tick.
- A fork's pull request runs the selftest: it gets no secrets and `contents: read` only.
- A workflow dispatched from a branch other than `main` gets no Environment secrets, and the `plan` job is skipped.
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
- **FR-007**: `cargo fmt --check` MUST pass, and `rust-ci-runner/Cargo.toml` MUST hold `rust-version = "1.82"`.
- **FR-008**: The selftest workflow MUST pass on the pull request that implements this feature, covering build, format check, clippy, tests, the fixture pipeline, the watch-list check, the workflow lint and the shell tests.
- **FR-009**: The lock file and the format patch MUST come from a temporary selftest job that uploads them as an artifact, because nothing can run cargo locally. The job MUST be removed before merge.

**Bootstrap secrets**

- **FR-010**: Log credentials MUST come from the Environment secrets `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, set in `factory-ci-plan`, `factory-ci` and `factory-ci-docker`. No script MUST read them from Bitwarden.
- **FR-011**: All three set means shipping is on, none means off, and a partial set MUST fail the tick at the start of the plan (before any GitHub read), naming the missing names. A job given a partial set MUST refuse before it runs a step.
- **FR-012**: Each log credential MUST be masked before use, and a value with a trimmed line under 4 characters MUST be refused. The three values MUST NOT reach any step container.
- **FR-013**: Pipeline secrets MUST come from Bitwarden only, with no GitHub-secret fallback. A job MUST make a Bitwarden call only when its pipeline lists at least one secret. An idle tick makes no Bitwarden call.
- **FR-014**: The three `FACTORY_CI_LOG_*` names and `BWS_ACCESS_TOKEN` stay reserved: a pipeline that lists them is rejected before any container starts.
- **FR-015**: No bootstrap secret value, no private repo name detail, branch, SHA, project path or secret name MUST appear in public logs, summaries, statuses, artifacts or outputs (002 FR-059). A test with sentinel values MUST prove it for the plan and job scripts.
- **FR-016**: The README MUST say the `FACTORY_CI_TOKEN` fine-grained token covers both watched repos (Contents read, Commit statuses read and write, Metadata read).

**Pipeline files**

- **FR-017**: factory-monorepo's build, lint and test MUST run only as runner pipeline steps from `.factory-ci.toml` files: one at its root for repo-wide checks, one in each top-level project folder. No Actions workflow, here or there, builds or tests that repo.
- **FR-018**: The pipeline format MUST stay TOML in `.factory-ci.toml`.
- **FR-019**: This repo MUST hold `rust-ci-runner/.factory-ci.toml` (format, lint and the tests that need no Docker) and a root `.factory-ci.toml` (shell tests and workflow lint).
- **FR-020**: Only the selftest and release workflows MAY run cargo directly, because no runner exists yet to build the first one. The README, the header of each of those two workflows and `plan.md` MUST say so, and a lint rule MUST fail any other workflow that runs `cargo`.
- **FR-021**: Before merge, the selftest MUST run both pipeline files with the freshly built runner, so a broken pipeline file never reaches the first tick.

**README and constitution**

- **FR-022**: The root README Setup MUST list every secret and variable with the Environment or scope that holds it, and the manual steps: create the three Environments limited to `main`, set the secrets, make the runner image and mirrored image packages public, set up the external scheduler for the tick, create the Bitwarden machine accounts and projects, create the Grafana Cloud token and the fine-grained GitHub tokens.
- **FR-023**: Every `secrets.NAME` and `vars.NAME` a workflow reads (except `GITHUB_TOKEN`) MUST appear in the README Setup tables and every name there MUST be read by a workflow. A shell test MUST enforce both ways.
- **FR-024**: `rust-ci-runner/README.md` MUST point to the root README for setup and keep only runner behaviour, commands and the pipeline format.
- **FR-025**: 002 tasks T045 and T081 MUST be done by this feature, T046, T054 and T065 MUST be tracked here as blocked on Setup, and 002's `tasks.md` MUST point to the 003 task for each.
- **FR-026**: The constitution MUST change by Governance PATCH 1.0.1 so that "repository secrets and variables" also covers this repo's Environment secrets and variables.

- **FR-027**: No workflow in this repo MUST use the `pull_request_target` trigger. The workflow lint MUST fail a copy that does, for every workflow file in the directory. A public repo's fork pull requests then never run with secrets.

### Key Entities

- **Environment**: a GitHub Environment of this repo (`factory-ci-plan`, `factory-ci`, `factory-ci-docker`), limited to `main`, holding bootstrap secrets.
- **Bootstrap secret**: an Actions secret the runner needs before it can read Bitwarden: tokens, blob keys, log credentials, heartbeat URL.
- **Repository variable**: a non-secret Actions variable: image digests, Bitwarden project IDs, status link.
- **Pipeline file**: a `.factory-ci.toml` at a repo root or in a top-level folder.
- **Watch entry**: one `[[watch]]` table in `.github/factory-ci-repos.toml`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The selftest workflow is green on the pull request, and the pull request holds `Cargo.lock`.
- **SC-002**: The stale-wording test, the `ubuntu-24.04` lint and the README-versus-workflows test pass in the shell job, and each fails on a mutated copy.
- **SC-003**: After Setup, one manual tick posts a `factory-ci` status on `sudtanj/infrastructure` and on `sudtanj/factory-monorepo` within 15 minutes.
- **SC-004**: A line a monorepo step prints is searchable in Loki within 30 seconds, and a signed-out reader of the public run log finds none of: a log credential, a branch, a SHA, a project path, a secret name.
- **SC-005**: With no Bitwarden token set, a pipeline that lists no secret still runs and ships logs.
- **SC-006**: Setup takes the owner one pass through the README, with no step missing (checked by the quickstart).

## Open Questions

None block the plan. Each has a default the plan uses.

- **OQ-1 (scheduler)**: Which external scheduler calls the tick every 5 minutes? Default: cron-job.org, a plain HTTPS POST with a fine-grained token (Actions write, this repo only). The owner may pick another; nothing in this repo depends on the choice.
- **OQ-2 (docker for this repo)**: Does this repo's own entry get `allow_docker`? Default: no. The integration tests that need Docker stay in the selftest workflow; the pipeline file runs the rest.
- **OQ-3 (log token per Environment)**: Same Loki credentials in all three Environments, or a separate token for `factory-ci-docker`? Default: the same values, because the docker Environment already holds root-on-host trust. A separate token is a README note.
- **OQ-4 (variables scope)**: Repository variables or per-Environment variables? Default: repository variables, because none is secret.
- **OQ-5 (selftest duplicates)**: Drop the selftest's shell and lint jobs once the runner runs the same checks? Default: keep them until the runner has passed 10 ticks; a later feature removes them.
- **OQ-6 (required checks)**: Which statuses does `main` require? Default: the selftest check, plus `factory-ci` and `factory-ci/rust-ci-runner` after the first green tick.

## Assumptions

- This repo is public, Actions are enabled, and `main` is protected (pull request required, no force push).
- Free Actions minutes apply to public repos, so a 5-minute tick costs nothing (002 A25).
- No local `cargo`, `docker`, `make` or test run exists: the VM has 1 GB of RAM. CI is the first compiler.
- The engineer can read public run logs and download artifacts with `gh`.
- factory-monorepo's pipeline files are written in that repo under its own constitution (v1.2.0, Principle VII). This feature records the contract and the watch entry only.
- The package visibility switch and every account step are manual and stay in the README.
