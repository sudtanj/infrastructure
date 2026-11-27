# Feature Specification: Infrastructure Home

**Feature Branch**: `003-infrastructure-home`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "The factory CI runner moved from the private factory-monorepo into the public sudtanj/infrastructure repo (main is 6fdfb1a). Make the repo self-consistent after the move, put every GitHub Actions workflow for sudtanj's repos here, run factory-monorepo's checks only as runner pipeline steps, inject the first secrets (Bitwarden, Grafana and Loki) from GitHub Actions secrets, do the first compile that 002 never had, pin ubuntu-latest to ubuntu-24.04, and tell the owner in the README exactly what to set by hand."

## Clarifications

### Session 2026-11-27

Answers come from the lead's brief. Nothing blocked on the user; open questions are listed under Open Questions.

- Q: Where do factory-monorepo's build, lint and test checks run? → A: Only as runner pipeline steps declared in `.factory-ci.toml` files in that repo: one at the root for repo-wide checks, one in each top-level project folder. That repo holds no GitHub Actions workflow (its constitution v1.2.0, Principle VII). Every workflow for sudtanj's repos lives here (FR-017).
- Q: Where do the first secrets come from? → A (round 4 replaces rounds 1 to 3): one GitHub Environment secret, `BWS_ACCESS_TOKEN` in `factory-ci`. Every other secret and setting, the log credentials included, comes from one Bitwarden project that its machine account reads (FR-047, FR-048). Pipeline secrets stay in Bitwarden only, with no GitHub-secret fallback (002 decision D-b, FR-013).
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
- D7 (superseded, round 4): two Loki tokens. There is one set of log credentials, in Bitwarden (FR-010).
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
- R2-14 (withdrawn, see FR-043): The `blob-key-length` rule passed when one of the two guards was deleted from the plan job. It now needs both blob-key variable names guarded (FR-043).
- R2-15: Flow-style (`- {uses: x@v4}`) and quoted (`"uses": x@v4`, `'runs-on': x`) keys slipped past the pin and runner rules. The line regex now reads both. A full YAML parse stays deferred (FR-044).
- R2-16: The `main` guard check for release jobs read only the mirror workflow by name. It now covers every workflow with a job in `factory-ci-release`; the release workflow's publish job keeps its tag-ref guard (FR-045).
- R2-17 (withdrawn, see FR-046): Nothing checked that `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER` differ. The plan guard step and `factory-ci-plan.sh` fail when both are set and equal (FR-046).
- R2-18: The label "security L1" named two findings. The deferred one (a test that fetched file content never reaches the summary `problems` text) is now "security L7". QA's residual risk is recorded in `plan.md` Open risks as R-13: a pipeline that names owner-scoped secrets on a row with empty `repo_id` skips the early store check; the job still refuses. No code change.

### Review round 4, 2026-11-27 (owner decision: one Bitwarden token, "option A")

Answers come from the owner's decision and the lead's brief. Nothing blocks on the owner; the only choices left to me are marked "chosen" with a reason.

- R2-19: The only GitHub Actions secret is `BWS_ACCESS_TOKEN`, an Environment secret of `factory-ci` (deployment branches `main` only, admin bypass off). The only GitHub variable is the optional `FACTORY_CI_BWS_SERVER_URL`, needed before Bitwarden can be reached. `factory-ci-release` stays and holds no secret. Everything else moves into one Bitwarden project that one machine account reads (FR-047, FR-048).
- R2-20: Removed with no replacement: `FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY_DOCKER`, `FACTORY_CI_BWS_PROJECT`, `FACTORY_CI_BWS_PROJECT_DOCKER`, the Environments `factory-ci-docker` and `factory-ci-plan`, and every guard, test row, lint rule and README line that existed only for them (FR-049).
- R2-21 (chosen): `plan` and `verdict` move into `factory-ci`. I looked for a reason they need their own Environment and found none: both held only secrets that now come from Bitwarden, and the Environment's only job is the `main` branch rule, which `factory-ci` already has (FR-058).
- R2-22 (chosen): The `run-docker` job, `matrix_docker`, `count_docker` and the second blob key go away. The only differences between `run` and `run-docker` were the credentials. Docker-enabled items now run in `run` and still get `--allow-docker` from their row. The owner accepted that such a step runs in a job that can read every Bitwarden secret (FR-054).
- R2-23 (chosen): One new script, `.github/scripts/factory-ci-load.sh`, runs one `bws secret list` per job, masks every value it takes, and writes only the names that job needs to `$GITHUB_ENV`. A fixed allowlist of names lives in the script. The `run` job makes a second list call only when its pipeline lists secrets (FR-050 to FR-053).
- R2-24 (chosen): `GITHUB_ENV` is the hand-off, so no `uses:` step may follow the load step in a tick job. A value with a newline or CR is refused. `RUNNER_IMAGE_DIGEST` is exported as `FCI_RUNNER_IMAGE_DIGEST` because GitHub reserves the `RUNNER_` prefix for its own variables and I cannot test whether `GITHUB_ENV` refuses it (FR-050, FR-052).
- R2-25 (chosen): The plan's own store check (FR-038) is gone. Its load call is the check: a revoked token fails the tick at `plan` with the store message. Every tick now calls Bitwarden at least twice (plan and verdict), so FR-013's "an idle tick makes no Bitwarden call" no longer holds (FR-056).
- R2-26 (accepted risk, owner): Bitwarden down means `verdict` cannot read the heartbeat URL, so no `/fail` ping goes out and only the healthchecks.io late alert fires. The `verdict` install and load steps are `continue-on-error`, so a Bitwarden fault there cannot turn a green tick red (FR-057). Moot when `FACTORY_CI_HEARTBEAT_URL` is unset, which is the setting for manual runs (R2-38).
- R2-27 (chosen): Constitution 1.2.0, MINOR. Principle II keeps its purpose and gains two rules (one GitHub secret, everything else in Bitwarden), Principle III gains one (mask every fetched value). No principle is removed. The change is not a PATCH because it redefines which values may sit in GitHub, and not a MAJOR because no principle goes and the effect narrows the allowed GitHub channel. Done in this step with `CLAUDE.md` and `AGENTS.md` (FR-061).
- R2-28: The runner stops reading `FACTORY_CI_TOKEN_DOCKER` and stops splitting rows by `allow_docker` (FR-055). The only runner change, found by `grep -rn "_DOCKER" rust-ci-runner`.
- Clarify pass (round 4): the scan over inputs, state, timing, trust and the README found no open question for the owner. Choices made without the owner are R2-21 to R2-25 and R2-27, each with its reason; the owner can reverse any of them before implementation starts.

### Review round 4 results, 2026-11-27 (critic NO-GO with one blocker, security with four findings; all adopted unless noted)

- R2-29 (critic B-1, blocker): `install bws` put `bws` in `$RUNNER_TEMP/bin` but never on `$GITHUB_PATH`, and `load secrets` runs before `fetch runner`, so `load secrets` could not find `bws`; in `verdict` it never could, and `continue-on-error` hid it, so no ping would ever go out. Fix: `install bws` appends `$RUNNER_TEMP/bin` to `$GITHUB_PATH` in every job (FR-063).
- R2-30 (M-1): the removed Environment name `factory-ci-plan` is a prefix of the script name `factory-ci-plan.sh`. Every check, grep and brief anchors the name so a following `.`, letter, digit, `_` or `-` does not match, and a row proves the script name passes (FR-049).
- R2-31 (M-2, scheduler steps removed by R2-38): one migration order everywhere: create the Bitwarden project, machine account, `BWS_ACCESS_TOKEN` and the keys that already exist; merge; tag the release, take the digest, run the mirror; set the remaining keys; run one manual tick; delete the retired items after the manual runs stay green. The owner has already created `factory-ci` (main only, admin bypass off) with `BWS_ACCESS_TOKEN`, and deleted `factory-ci-plan` (FR-060).
- R2-32 (M-3): `rust-ci-runner-release.yml` and `factory-ci-mirror-images.yml` tell the owner to set repository variables. They now say "add the Bitwarden secret KEY", with a docs-test row (FR-064, T069).
- R2-33 (M-4): a revoked or expired token most likely answers `400 invalid_client`, not 401 or 403. The reject pattern covers `400`, `401`, `403` and `invalid_client`; T066 records the real text (FR-050).
- R2-34 (M-5): the soft `verdict` steps get `timeout-minutes: 2`, the `verdict` load step sets `FACTORY_CI_FETCH_WINDOW=20`, and the job timeout is 10 minutes (FR-057).
- R2-35 (withdrawn by R2-38): the idea to move the idle ping into `plan` and make `verdict` call Bitwarden only when items ran. `FR-051`, `FR-056` and `FR-057` keep the first round 4 design. The `fci_list` retry jitter stays because it costs one line.
- R2-36 (minors m-1 to m-8): a selftest step proves the `$GITHUB_ENV` hand-off with a stub `bws`, including a probe of a `RUNNER_` name (FR-065, T068); whitespace and trailing-newline values are refused (FR-050); the README warns against `ACTIONS_STEP_DEBUG` (FR-060); test values have the real shape of each key; the checkout post step also inherits `$GITHUB_ENV`, which is recorded as accepted; T054 to T056 land in one commit; W1 and the lint skip comment lines; the README and docs test cover `gh secret list --org` and "Selected branches: main".
- R2-37 (security F1 to F8): live checks of the `factory-ci` Environment and the repo and org secret lists before the first tick and again at T066 and T067 (FR-066, T070); every tick job, `verdict` included, has the main-only guard (FR-066); the docker risk is stated in full and an `allow_docker = true` entry needs a risk-marker comment (FR-067); clone and dind references must start with `ghcr.io/<owner>/`, and the committed-file alternative is a decision pending the owner (FR-068, research D32); a "token leaked" runbook (FR-060); derived forms are masked, `jq` and `bws` stderr go to `/dev/null`, one `printf` writes `$GITHUB_ENV` after every check (FR-050); the three `install bws` copies are byte-identical, fetch one archive, use `curl --proto '=https' --tlsv1.2` (FR-069).
- Rejected: none. One adaptation: the image-prefix guard derives the owner from `github.repository_owner` like `fetch runner` does, so a fork keeps working (the value for this repo is `ghcr.io/sudtanj/`).
- R2-38 (owner decision, after the round 4 reviews): Ticks run only when the owner starts `factory-ci-tick` by hand from the Actions tab. There is no cron-job.org scheduler and no interval. The workflow is already `workflow_dispatch`-only, so its trigger does not change. Consequences: (a) the scheduler token, the cron-job.org setup, its 75-day reminder and "pause the scheduler" leave Setup; the smaller change is to keep the procedure in the README as a step labelled "Optional: run on a schedule" and say to skip it, instead of deleting it and its docs-test rows; migration and rollback steps lose their scheduler lines; (b) the heartbeat (healthchecks.io, `FACTORY_CI_HEARTBEAT_URL`) has no meaning without a schedule: the existing optional code stays exactly as it is, the README says to leave the key unset for manual runs, R-15 is moot when it is unset, and the M-6 rework (R2-35) is withdrawn; (c) the Bitwarden rate-limit risk R-14 drops to a handful of manual runs; the retry jitter stays because it is one line; (d) FR-051, FR-056 and FR-057 are as in the first round 4 design plus the M-5 timeouts.

### What this feature changes in 002

| 002 item | Now |
| --- | --- |
| Round 4: `factory-ci-docker`, `factory-ci-plan`, `run-docker`, the docker token, key and project (002 FR-029, FR-047 R3, tick contract, data model) | Gone. One `run` job, one `factory-ci` Environment, one token. See R2-19 to R2-22. |
| A1 (central repo B is the monorepo or a public copy) | B is this repo. The "separate public copy" step is gone. |
| FR-029, FR-040 last sentence, US8 scenario 3 (log credentials in the Bitwarden default project) | They stay in Bitwarden, in the one project (FR-010, round 4). Round 2 had moved them to Environment secrets; round 4 reverses that. |
| FR-045 (the plan checks the store and that the three log values are all present or all absent) | The plan has no separate store check; its load step is the check (FR-056). The all-or-none check on the log values stays at the start of the plan (FR-011). |
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

### User Story 3 - One Bitwarden token on GitHub, everything else in Bitwarden (Priority: P1)

The owner sets one GitHub Actions secret, `BWS_ACCESS_TOKEN`, in the Environment `factory-ci`. Every other secret and setting the runner needs sits in one Bitwarden project that the token's machine account reads. Each tick job fetches what it needs with one `bws secret list`, masks it, and passes it on.

**Why this priority**: Round 3 left ten secrets in three Environments and two Bitwarden projects. Each one is a copy to rotate and a place to leak from. One token is one thing to guard on GitHub.

**Independent Test**: Shell tests with a fake `bws` and sentinel values: all names present, a required one missing, an optional one missing, the log trio partial, a duplicate key, short and multi-line values, a missing or rejected token, and no sentinel in any output except `::add-mask::` lines.

**Acceptance Scenarios**:

1. **Given** a Bitwarden project holding every required name, **When** a tick job runs `factory-ci-load.sh` with its name list, **Then** exactly one `bws secret list` runs, each value is masked before anything else uses it, and only the listed names reach `$GITHUB_ENV`.
2. **Given** `BWS_ACCESS_TOKEN` unset or empty, **When** a tick job loads, **Then** it stops with one fixed line that names the Environment, and no `bws` call is made.
3. **Given** a required name missing from the project, **When** the job loads, **Then** it stops and names the missing key (a bootstrap key is public in the README, so naming it is safe). An optional name missing is skipped.
4. **Given** one or two of the three `FACTORY_CI_LOG_*` values present, **When** the plan or a job starts, **Then** it fails before any repo is checked and names the missing keys (FR-011, unchanged).
5. **Given** a pipeline that lists no secret, **When** its `run` job runs, **Then** the job makes no second Bitwarden call.
6. **Given** a pipeline that lists a secret, **When** the `run` job runs, **Then** the value comes from the same project through a second `bws secret list` and from nowhere else; with the token rejected the item ends in `error`.
7. **Given** a value with a line under 4 characters, a newline, CR or NUL anywhere, or leading or trailing whitespace, **When** any job loads it, **Then** the job refuses it and prints no value.
8. **Given** sentinel values in every Bitwarden name, **When** the load, plan and job scripts run, **Then** no sentinel appears in stdout, stderr, the step summary or the step outputs, except as `::add-mask::` lines.
9. **Given** a pipeline step that lists `FACTORY_CI_TOKEN`, `RUNNER_IMAGE_DIGEST` or any other Bitwarden bootstrap name, **When** the runner reads the file, **Then** it rejects the name before any container starts.

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

The README Setup section has a table of GitHub Actions items and a table of Bitwarden secrets, and the manual steps: repository rules, create the Environment, create the Bitwarden project and machine account, fill in the secrets, make the GHCR packages public, set up the scheduler that calls the tick, and the rollback.

**Why this priority**: A public repo shows its whole setup (Principle II). The owner cannot script these steps, so the list must be exact.

**Independent Test**: A shell test compares the names the workflows read with the names the README lists, both ways.

**Acceptance Scenarios**:

1. **Given** the workflows, **When** every `secrets.NAME` and `vars.NAME` is extracted (except `GITHUB_TOKEN`), **Then** each appears in the README Setup tables with its Environment, and each README name is read by a workflow.
2. **Given** a workflow change that reads a new secret, **When** the README is not updated, **Then** the test fails.
3. **Given** the Setup section, **When** the owner follows it from top to bottom, **Then** the order works: repository rules, the Environment, the Bitwarden project and machine account, the tokens, the first Bitwarden secrets, merge, release, make public, mirror (the image names go into Bitwarden), scheduler, first tick.
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
- The Environment `factory-ci` does not exist when the tick first runs: GitHub creates it without a branch rule and without the token, and the first job stops with the fixed "BWS_ACCESS_TOKEN is not set" line. The README says to create it first and restrict it to `main`.
- The log trio in Bitwarden is partial (one or two of three): the plan fails at its start and names the missing keys; a job that gets a partial set refuses with "log credentials incomplete".
- The private monorepo entry and no log credentials: the plan refuses the repo and fails the tick.
- A fork's pull request runs the selftest: it gets no secrets and `contents: read` only.
- A workflow dispatched from a branch other than `main`: every tick job, `verdict` included, is skipped by its main-only `if:`; no Environment is entered and no secret is read (FR-058).
- Bitwarden is down: `plan` stops at its load step with "secret store unreachable" after the fetch window, and `verdict` is red from the plan result. A fault that starts later: `run` jobs end red without posting a status (the `pending` statuses go stale and are replanned), and the soft `verdict` steps give up after 2 minutes, so no `/fail` ping goes out. The owner accepted this (R2-26); it is moot when the heartbeat URL is unset.
- A blob key rotated in Bitwarden while a tick runs: `run` jobs cannot decrypt their row and end red with no status; the next tick replans the stale `pending` items. The README says to rotate it between ticks.
- A blob key of 64 characters that is not lower-case hex: every guard refuses it (FR-036). There is one blob key.
- A plan or run job fails or is cancelled: the heartbeat pings `/fail` and the check alerts at once; ticks that stop make it late (FR-034), unless Bitwarden is the cause (R2-26).
- The tick has no scheduler by default (R2-38): the owner starts `factory-ci-tick` by hand. If the owner adds a scheduler, its `workflow_dispatch` token can only start ticks; the tick still reads secrets only on `main`.

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

- **FR-010** (round 4): Log credentials MUST come from the Bitwarden keys `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, fetched by `factory-ci-load.sh` (FR-050). GitHub holds none of them, and there is no second Loki token for a docker job.
- **FR-011**: All three set means shipping is on, none means off, and a partial set MUST fail the tick at the start of the plan (before any GitHub read), naming the missing names. A job given a partial set MUST refuse before it runs a step.
- **FR-012**: Each log credential MUST be masked before use, and a value with a trimmed line under 4 characters MUST be refused. The three values MUST NOT reach any step container.
- **FR-013** (round 4): Pipeline secrets MUST come from Bitwarden only, with no GitHub-secret fallback. The `run` job MUST make its pipeline-secret `bws secret list` call only when its pipeline lists at least one secret (FR-053). Whether a tick calls Bitwarden at all is FR-056.
- **FR-014**: The three `FACTORY_CI_LOG_*` names and `BWS_ACCESS_TOKEN` stay reserved: a pipeline that lists them is rejected before any container starts. Round 4: the runner's reserved prefixes `FACTORY_CI`, `RUNNER_`, `BWS_` and `FCI_` already cover every Bitwarden bootstrap name, and a test row names each one (FR-053).
- **FR-015**: No bootstrap secret value, no private repo name detail, branch, SHA, project path or secret name MUST appear in public logs, summaries, statuses, artifacts or outputs (002 FR-059). A test with sentinel values MUST prove it for the plan and job scripts.
- **FR-016** (round 4): The README MUST say `FACTORY_CI_TOKEN` is a fine-grained personal access token with "Only select repositories" set to the watched repos and these permissions only: Contents Read-only, Commit statuses Read and write, Metadata Read-only. Expiry 90 days. The runner uses one token for the clone and the statuses; splitting it is deferred (plan, Open risks). Only if the owner runs a schedule (R2-38): the scheduler token is a separate fine-grained token on `sudtanj/infrastructure` only with Actions Read and write; it lives only in cron-job.org.

**Pipeline files**

- **FR-017**: factory-monorepo's build, lint and test MUST run only as runner pipeline steps from `.factory-ci.toml` files: one at its root for repo-wide checks, one in each top-level project folder. No Actions workflow, here or there, builds or tests that repo. A watched repo with no `.factory-ci.toml` is skipped by the plan with no status and no job, and the README says so.
- **FR-018**: The pipeline format MUST stay TOML in `.factory-ci.toml`.
- **FR-019**: This repo MUST hold `rust-ci-runner/.factory-ci.toml` (format, lint and `cargo test --locked`) and a root `.factory-ci.toml` (shell tests and workflow lint). The root step's image MUST provide `git` and trust the mounted checkout (`git config --global --add safe.directory '*'`), or the docs test MUST use a fixed file list that excludes `target/`; the `own-pipelines` job proves it.
- **FR-020**: Only the selftest and release workflows MAY run cargo directly, because no runner exists yet to build the first one. The README, the header of each of those two workflows and `plan.md` MUST say so, and a lint rule MUST fail any other workflow that runs `cargo`.
- **FR-021**: Before merge, a selftest job `own-pipelines` MUST run both pipeline files with the freshly built runner (built there or passed as an artifact), with `FACTORY_CI_SHA`, `FACTORY_CI_EVENT` and `FACTORY_CI_BRANCH` set as the fixture step sets them and a separate `CARGO_TARGET_DIR`, so a broken pipeline file never reaches the first tick.

**README and constitution**

- **FR-022**: The root README Setup MUST list every secret and variable with the Environment or scope that holds it, and the manual steps, in the order that works:
  - Repository rules: a `main` ruleset (pull request required, 0 approvals, the selftest jobs as the only required status checks, no force push, no deletion, empty bypass list); no CODEOWNERS file and no code-owner review; a tag ruleset limiting who creates `rust-ci-runner-v*`; Actions settings (approval for all outside collaborators, read-only default workflow token, run retention 7 to 14 days). The README says why: GitHub never lets an author approve their own pull request, only the owner has write access, so only the owner can merge, and the selftest is the gate.
  - Environments (round 4): `factory-ci` limited to `main` with admin bypass off, holding only `BWS_ACCESS_TOKEN`; `factory-ci-release` (no secret) limited to `main` and the tag pattern `rust-ci-runner-v*`. No other Environment.
  - Accounts (round 4): one Bitwarden project and one machine account with Can read on it, token expiry set; the fine-grained tokens of FR-016; one blob key from `openssl rand -hex 32`; a Grafana access policy with `logs:write` only and a usage alert on ingest. All rotate before day 80.
  - After Setup (round 4), `gh secret list` at repo and org level shows no secret at all, and `gh secret list --env factory-ci` shows exactly `BWS_ACCESS_TOKEN`.
  - GHCR: the first push is private; switch it to public in the UI; check with `gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility`; confirm the release digest came from a run on `main`.
  - Optional, round 4 (R2-38): run on a schedule. Scheduler: cron-job.org, `POST` to the dispatch endpoint with headers `Accept: application/vnd.github+json`, `Authorization: Bearer <token>` and `X-GitHub-Api-Version: 2022-11-28`, body `{"ref":"main"}`, expect 204, failure notification on, fine-grained token with Actions Read and write on this repo only, a renewal reminder at day 75.
  - Optional, round 4: heartbeat. Heartbeat: healthchecks.io with a grace of 60 minutes or more, pinged at `/fail` when the plan failed or the tick is red (FR-034). The README says a red tick or a failed plan pings `/fail` and alerts at once, and that a late check means ticks stopped.
  - What a watched repo without `.factory-ci.toml` gets (nothing). Round 4: the old "delete the log keys last" step is gone, because the same Bitwarden keys are now the source; a migration note tells the owner to delete the old Environments and secrets.
  - Rollback: pause the scheduler first if one runs, revert the lock and `--locked` together, and write down the previous `RUNNER_IMAGE_DIGEST`; the README lists what a revert does not restore (Environment secrets, the public package, Bitwarden contents, the scheduler token, the Grafana policy).
  - Notes: watch `ubuntu-24.04` deprecation notices; rerun the mirror after a Docker Hub rate limit; keep this repo public or lengthen the tick period (cost).
- **FR-023** (round 4): Every `secrets.NAME` and `vars.NAME` a workflow reads (except `GITHUB_TOKEN`) MUST appear in the README table of GitHub Actions items, and every name there MUST be read by a workflow. Every name a tick job passes to `factory-ci-load.sh` MUST appear in the README table of Bitwarden secrets, and every name there MUST be passed by some job. Every `environment:` name in the workflows MUST appear in the README and the reverse. A shell test MUST enforce all three both ways.
- **FR-024**: `rust-ci-runner/README.md` MUST point to the root README for setup and keep only runner behaviour, commands and the pipeline format.
- **FR-025**: 002 tasks T045 and T081 MUST be done by this feature, T046, T054 and T065 MUST be tracked here as blocked on Setup, and 002's `tasks.md` MUST point to the 003 task for each.
- **FR-026**: The constitution MUST change by Governance PATCH 1.0.1 so that "repository secrets and variables" also covers this repo's Environment secrets and variables.

- **FR-027**: No workflow in this repo MUST use the `pull_request_target` trigger, in block, list (`on: [pull_request_target]`) or map (`on: {pull_request_target: ...}`) form. The workflow lint MUST fail a copy that does, for every workflow file in the directory. A public repo's fork pull requests then never run with secrets.

**Selftest, release and tick hardening**

- **FR-028**: The selftest MUST build the release binaries for both musl targets with the release workflow's compiler and linker settings and build the image with `docker buildx build` without pushing it. The release workflow MUST run `rustup toolchain install` before `make build`. The release workflow gets no `workflow_dispatch` trigger.
- **FR-029**: The selftest MUST run on every pull request (no `paths:` filter) and on `workflow_dispatch`, have `concurrency: {group: selftest-${{ github.ref }}, cancel-in-progress: true}`, use `persist-credentials: false` on every checkout, and never use `secrets.`, `environment:`, `workflow_run` or `id-token`. The lint MUST fail a copy that does. The selftest MUST lint the whole `.github/workflows/` directory with `FACTORY_CI_LINT_STRICT=1`.
- **FR-030**: Tests that need a real container engine MUST carry `#[ignore]`. The runner pipeline runs `cargo test --locked`; the selftest host job adds `-- --include-ignored`.
- **FR-031** (round 4): `FACTORY_CI_BWS_SERVER_URL`, when set, MUST start with `https://`; the `install bws` step of each tick job MUST refuse any other value before it downloads or calls anything. The plan and job scripts no longer repeat the check, because they never run without that step.
- **FR-032**: A shell test MUST scan the committed docs (README files, `research.md`, `quickstart.md`) for private identifiers: no run URL of a private repo, no Grafana hostname, no token or key pattern. The live-check results template in `research.md` says so.
- **FR-033**: The release `publish` job and the image mirror job MUST run in the Environment `factory-ci-release`. The mirror job MUST run only when `github.ref == 'refs/heads/main'`.

**Review round 2**

- **FR-034**: The tick `verdict` job MUST ping `<FACTORY_CI_HEARTBEAT_URL>/fail` when the plan result is not `success`, when `fail_tick` is `1`, or when the `run` result is `failure` or `cancelled`, and the plain URL otherwise. It MUST ping only on `main`, only when the URL is set, with a 10-second limit, and a ping error MUST NOT fail the job. The README Setup step 11 MUST say that a red tick or a failed plan pings `/fail` and alerts at once, and that a late check means ticks stopped. A docs-test row MUST assert that `/fail` appears in the README. Contract: `contracts/tick-workflow.md`.
- **FR-035**: The workflow lint MUST stay line-based and MUST add these rules, each with a mutation row in `--self-test`:
  - The tick's `on:` block has `workflow_dispatch:` as its only key. Any other key indented by 2 spaces under `on:`, quoted or not, fails, and so does a flow-style `on:` in the tick.
  - The lint loop covers `*.yml` and `*.yaml`.
  - The 40-hex `uses:` pin rule and the `permissions` present rule run on every workflow file.
  - The tick's `environment:` value per job is from an allowlist: `plan`, `run` and `verdict` use `factory-ci` (round 4, FR-058).
  - The main guard regex is end-anchored: `if: github.ref == 'refs/heads/main'( && .*)?$`, with no `||`.
  - The selftest rule also rejects `secrets: inherit` and `secrets[`.
- **FR-036**: Every guard on `FACTORY_CI_BLOB_KEY` (the only blob key since round 4) (the tick guard steps, `fci_blob_encrypt` and `fci_blob_decrypt`, and the scripts that call them) MUST require `^[0-9a-f]{64}$`, not only a length of 32 or more. The README already says the keys come from `openssl rand -hex 32`. Shell tests MUST cover a short key, a 64-character non-hex key, an upper-case hex key and a valid key.
- **FR-037**: The plan script MUST call the plan binary for phase A with `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` removed from its environment. Phase A (`plan::candidates`) only reads and never ships logs (checked in `rust-ci-runner/src/plan.rs`); the log line is sent from phase C (`plan::finish`), so the credentials are removed for phase A only. A test MUST prove phase A sees none of the three names and phase C sees all three.
- **FR-038** (withdrawn, round 4, FR-056): The plan no longer has its own store check. Its load step is the check.
- **FR-039**: The first paragraph of the root `README.md`, the preamble of `.specify/memory/constitution.md` and the GitHub repository description MUST say that this repository is the open-source (MIT) home of sudtanj's infrastructure and DevOps tools, and that it holds the factory CI runner and every GitHub Actions workflow today.
- **FR-040**: The root `README.md` MUST say, right after its first paragraph, that the tools are tailored to the owner's repositories and needs, may not fit every use case, and come with no promise of support or stable interfaces.
- **FR-041**: The root `README.md` MUST say the blob keys are 64 lower-case hex characters from `openssl rand -hex 32` everywhere it mentions their form, and MUST NOT contain "32 characters". A docs-test row MUST fail on that phrase.
- **FR-042**: The lint `permissions` rule MUST accept only `permissions: {}` or a block mapping (the key alone on its line, followed by indented `scope: read|write|none` lines) at workflow level in every workflow file, and MUST reject `write-all` and `read-all` at workflow level and job level. Mutation rows for both values.
- **FR-043** (withdrawn, owner scope change: blob keys are redesigned separately): The lint `blob-key-length` rule MUST require a 64-hex guard for each of `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER` in the plan job. A mutation row deletes the `_DOCKER` guard.
- **FR-044**: The lint `uses:` pin and `runs-on:` rules MUST also read keys with optional quotes and a leading `{` or `- {`. Mutation rows for one flow-style and one quoted form. A full YAML parse stays deferred.
- **FR-045**: The lint MUST apply the `main` guard check to every scanned workflow with a job that has `environment: factory-ci-release`. Accepted guards: the `if: github.ref == 'refs/heads/main'( && .*)?$` form, or, for a workflow triggered only by a tag push, the `verify` job check `git merge-base --is-ancestor "$GITHUB_SHA" origin/main` (the form `rust-ci-runner-release.yml` uses today, T051). A mutation row copies the mirror workflow under another name without the guard and must fail `main-guard`.
- **FR-046** (withdrawn, same reason): When `FACTORY_CI_BLOB_KEY_DOCKER` is set and equal to `FACTORY_CI_BLOB_KEY`, the tick plan guard step and `factory-ci-plan.sh` MUST fail with a message naming the two variables, never their values. An unset docker key passes. The README Setup secrets step MUST say the two keys differ.

**Review round 4: one Bitwarden token**

- **FR-047**: GitHub MUST hold exactly these items for the workflows: the Environment secret `BWS_ACCESS_TOKEN` in `factory-ci`; the optional repository variable `FACTORY_CI_BWS_SERVER_URL`; the Environments `factory-ci` and `factory-ci-release`, the second with no secret. No workflow MUST read any other `secrets.NAME` (except `GITHUB_TOKEN`) or `vars.NAME`. A lint rule MUST fail a copy that does, with a mutation row.
- **FR-048**: Bitwarden MUST hold, in one project that one machine account reads: required `FACTORY_CI_TOKEN`, `FACTORY_CI_BLOB_KEY` (one key, 64 lower-case hex, FR-036), `RUNNER_IMAGE_DIGEST` (`sha256:<64 hex>`), `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE` (pinned `@sha256`); the log trio `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, all or none (FR-011); optional `FACTORY_CI_LOG_LINK` and `FACTORY_CI_HEARTBEAT_URL`; and the pipeline secrets named in `.factory-ci.toml` steps. No project ID is configured: the machine account sees one project and `bws secret list` runs without an ID.
- **FR-049**: No tracked file outside `specs/` and `.claude/` MUST name `FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY_DOCKER`, `FACTORY_CI_BWS_PROJECT`, `FACTORY_CI_BWS_PROJECT_DOCKER`, `factory-ci-docker`, `factory-ci-plan`, `run-docker`, `matrix_docker` or `count_docker`. The pattern for `factory-ci-plan` is anchored (`factory-ci-plan` not followed by a letter, digit, `.`, `_` or `-`), because the script `factory-ci-plan.sh` is a live file; a docs-test row proves the script name does not trip it. A docs-test check MUST enforce it (patterns built from parts, so the test never holds the words). Every guard, test row, lint rule and README line that existed only for them MUST go.
- **FR-050**: A new script `.github/scripts/factory-ci-load.sh NAME...` MUST fetch values with one `bws secret list -o json` (through `fci_list`, with its retry window, jittered backoff and a no-retry rule for a rejected token), look each requested name up by exact key, and then in this order: (1) mask every requested value found with `::add-mask::` using `fci_mask`, plus the derived forms: the URL-encoded value when it differs, and the base64 of `FACTORY_CI_LOG_USER:FACTORY_CI_LOG_TOKEN` when both are loaded; (2) refuse a value with a line under 4 characters, a newline or CR anywhere, a NUL, or leading or trailing whitespace (checked on the JSON value, because `$(...)` strips a trailing newline); (3) build every `NAME=value` line first, then append them to `$GITHUB_ENV` with one `printf '%s'`, only for the requested names. `RUNNER_IMAGE_DIGEST` is written as `FCI_RUNNER_IMAGE_DIGEST`. A name outside the allowlist MUST be refused. A missing required name MUST fail with its key (never a value). A missing optional name is skipped. Two secrets with one key MUST fail as ambiguous. A token that is unset MUST fail with one fixed line naming `BWS_ACCESS_TOKEN` and the Environment `factory-ci`, with no `bws` call. A rejected token (`bws` stderr with `400`, `401`, `403`, `invalid_client`, `unauthorized`, `forbidden` or `invalid access token`) MUST fail at once with "secret store rejected the token". The script sends `jq` and `bws` stderr to `/dev/null` or a private file it never prints, prints only `::add-mask::` lines and fixed error text, never echoes a value, and never uses `set -x`.
- **FR-051**: Each tick job MUST load only the names it needs. `plan`: `RUNNER_IMAGE_DIGEST`, `FACTORY_CI_BLOB_KEY`, `FACTORY_CI_TOKEN`, the log trio, `FACTORY_CI_LOG_LINK`. `run`: those plus `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE`. `verdict`: `FACTORY_CI_HEARTBEAT_URL` only. A shell test MUST compare each job's load line with this table.
- **FR-052**: In each tick job the steps MUST run in this order: checkout, `install bws`, `load secrets`, then repo scripts only. No `uses:` step MAY follow the load step, because every later step inherits `$GITHUB_ENV`. `secrets.BWS_ACCESS_TOKEN` MUST be mapped only in the `load secrets` steps and in the `run` job's `job` step. A lint rule MUST enforce both, with mutation rows. The checkout post step also inherits `$GITHUB_ENV`; the checkout is pinned to a commit and uses `persist-credentials: false`, and the owner accepts this (R-22).
- **FR-053**: The `run` job's `job` step MUST call `bws secret list` a second time only when the pipeline lists at least one secret, and MUST refuse with "no secret store configured" when it lists one and the token is unset (unchanged). Every Bitwarden bootstrap name MUST be refused as a pipeline secret name by the runner; a test row names each of them.
- **FR-054**: There MUST be one `run` job for every item. The `run-docker` job, `matrix_docker`, `count_docker`, the docker blob key and the docker token MUST NOT exist. The plan outputs are `matrix`, `count` and `fail_tick`. `allow_docker` rows still pass `--allow-docker` to the job. `verdict` needs `plan` and `run`. The lint reads one `max-parallel: 4`.
- **FR-055**: The runner MUST use one GitHub token. `rust-ci-runner/src/main.rs` and `plan.rs` MUST NOT read `FACTORY_CI_TOKEN_DOCKER`, hold a second API client, skip docker entries or emit the "FACTORY_CI_TOKEN_DOCKER is not set" problem, and `Final` MUST hold one `matrix`. The tests that covered the removed behaviour (`tests/cli.rs` T071 and T082 blocks, `plan_hardening_tests.rs`, `plan_tests.rs`) MUST change to match, and `rust-ci-runner/README.md` MUST say the same.
- **FR-056**: `factory-ci-plan.sh` MUST make no `bws` call. The plan's load step is the store check: a rejected token stops the tick at `plan` with "secret store rejected the token", and an unreachable store with "secret store unreachable". Every tick makes one Bitwarden call in `plan` and one in `verdict`, plus one per `run` item whose pipeline lists a secret. Ticks start only by hand (R2-38), so this is a handful of calls a day.
- **FR-057**: The `verdict` job MUST run `install bws` and `load secrets` first, both with `continue-on-error: true` and `timeout-minutes: 2`, the load step with `FACTORY_CI_FETCH_WINDOW=20`, the job with `timeout-minutes: 10`, so a Bitwarden fault never changes the verdict or holds the concurrency slot. With no heartbeat URL loaded, the heartbeat step pings nothing. The ping rules of FR-034 stay in the workflow step, unchanged. The job needs `contents: read` and a checkout, because the load script lives in the repo.
- **FR-058**: The jobs `plan`, `run` and `verdict` MUST use the Environment `factory-ci`. The lint `tick-environment` rule allows only that value. Every tick job, `verdict` included, MUST have an end-anchored main-only guard; `verdict` uses `if: github.ref == 'refs/heads/main' && always()`, so a dispatch from another branch skips every job and reads no secret.
- **FR-059**: A test with sentinel values in every Bitwarden name MUST prove that `factory-ci-load.sh`, `factory-ci-plan.sh` and `factory-ci-job.sh` print no sentinel except as `::add-mask::` lines, that the `$GITHUB_ENV` file holds only the requested names, and that every mask line precedes the first `$GITHUB_ENV` write.
- **FR-060**: The root README Setup MUST have two separate tables. The first lists the GitHub Actions items of FR-047, each with its type and scope: the Environment `factory-ci` (deployment branches `main`), the Environment secret `BWS_ACCESS_TOKEN`, the optional repository variable `FACTORY_CI_BWS_SERVER_URL`, and the Environment `factory-ci-release` (no secret). The second lists every Bitwarden secret of FR-048 with its name, what it is, how to create it and when to set it. It MUST describe `FACTORY_CI_TOKEN` and the scheduler token as FR-016 says, update the troubleshooting table and the rollback notes, replace the "no bootstrap secret at repo level" step with the check of FR-022 (round 4), include a migration note for the retired Environments, secrets and variables, and say a blob key is rotated between ticks. The docs test enforces FR-023 both ways against the two tables. It MUST also: give the migration order of R2-31 without scheduler steps (the owner has already created `factory-ci` and `BWS_ACCESS_TOKEN`); say "Deployment branches: Selected branches, `main`" and show `gh secret list`, `gh secret list --org <org>` and `gh secret list --env factory-ci`; say never to enable `ACTIONS_STEP_DEBUG` or `ACTIONS_RUNNER_DEBUG` on this repo; say a docker step can get root on the runner and so can read `BWS_ACCESS_TOKEN` itself, hence every Bitwarden secret and every watched repo, and that the owner rotates the token after any untrusted docker run; list the real stderr of a revoked token in the troubleshooting table; and hold a "token leaked" runbook: revoke the machine-account token, issue a new one with an expiry of 90 days or less, rotate every value in the project (the GitHub PAT, the blob key, the Loki token, the heartbeat URL, the image references) and every pipeline secret, then delete or review old run logs (retention 7 to 14 days) for unmasked derived forms. The scheduler step and the heartbeat step are labelled optional (R2-38) and the README says to leave `FACTORY_CI_HEARTBEAT_URL` unset for manual runs.
- **FR-061**: The constitution MUST be version 1.2.0 with Principle II, Principle III and the Repository Structure bullet as amended in this round, and `CLAUDE.md` and `AGENTS.md` MUST say the same in one sentence. Done in this step.
- **FR-062**: The last task of `tasks.md` MUST update the root README, `rust-ci-runner/README.md` and the GitHub repository description after merge, or state why not (constitution, Development Workflow).
- **FR-063**: The `install bws` step of every tick job MUST append `$RUNNER_TEMP/bin` to `$GITHUB_PATH`, so `load secrets` and later steps find `bws`. A hardening row (W9) MUST fail on a workflow copy where any `install bws` lacks it.
- **FR-064**: `rust-ci-runner-release.yml` and `factory-ci-mirror-images.yml` MUST tell the owner to add the Bitwarden secret `KEY` (`RUNNER_IMAGE_DIGEST`, `FACTORY_CI_CLONE_IMAGE`, `FACTORY_CI_DIND_IMAGE`), not to set a repository variable, in their step summaries and header comments. A docs-test row MUST fail on the words "repository variable" in either file.
- **FR-065**: The selftest `shell` job MUST include two steps that prove the `$GITHUB_ENV` hand-off: step one runs `factory-ci-load.sh` against a stub `bws` with a literal fake token and writes a probe variable with a `RUNNER_` name to `$GITHUB_ENV`; step two checks that `FCI_RUNNER_IMAGE_DIGEST` and the other loaded names reached it, and prints whether the `RUNNER_` probe arrived, failing only on the first. The steps use no `secrets.` and no Environment.
- **FR-066**: Before the first tick and again at T066 and T067 the engineer MUST check live: `gh api repos/sudtanj/infrastructure/environments/factory-ci` shows `deployment_branch_policy.custom_branch_policies` true, `protected_branches` false and `can_admins_bypass` false; its branch policies are exactly one, named `main`; `gh secret list` and `gh secret list --org` show nothing; and the `main` ruleset matches the README. The lint main-guard rule MUST cover `plan`, `run` and `verdict`.
- **FR-067**: A hardening row MUST fail when an entry in `.github/factory-ci-repos.toml` sets `allow_docker = true` without a comment line `# RISK-ACCEPTED: a docker step can read every Bitwarden secret` directly above its `[[watch]]` line. Neither entry sets it today.
- **FR-068**: The `run` job's guard step MUST refuse a `FACTORY_CI_CLONE_IMAGE` or `FACTORY_CI_DIND_IMAGE` that does not start with `ghcr.io/<owner>/` (owner from `github.repository_owner`, lower-cased, as `fetch runner` derives it), besides the `@sha256` pin. A lint rule MUST require that guard and a hardening row MUST cover it. Whether these references stay in Bitwarden or move to a committed file is open (research D32).
- **FR-069**: The `install bws` step MUST download only the archive (no separate checksum file), with `curl --proto '=https' --tlsv1.2`, and compare its SHA-256 with `BWS_SHA256`. The `run:` bodies of the three `install bws` steps MUST be byte-identical, and a lint rule MUST fail when they differ.

### Key Entities

- **Environment**: a GitHub Environment of this repo. `factory-ci` is limited to `main`, holds `BWS_ACCESS_TOKEN` and serves the `plan`, `run` and `verdict` jobs. `factory-ci-release` holds no secret and is limited to `main` and the release tag pattern.
- **Bitwarden secret**: a key and value in the one Bitwarden project the machine account reads: the GitHub token, the blob key, the log credentials, the heartbeat URL, the pinned image references and the pipeline secrets.
- **Repository variable**: only `FACTORY_CI_BWS_SERVER_URL`, optional.
- **Pipeline file**: a `.factory-ci.toml` at a repo root or in a top-level folder.
- **Watch entry**: one `[[watch]]` table in `.github/factory-ci-repos.toml`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: The selftest workflow is green on the pull request, and the pull request holds `Cargo.lock`.
- **SC-002**: The stale-wording test, the `ubuntu-24.04` lint and the README-versus-workflows test pass in the shell job, and each fails on a mutated copy.
- **SC-003**: After Setup, one manual tick posts a `factory-ci` status on `sudtanj/infrastructure` within 15 minutes. The same holds for `sudtanj/factory-monorepo` once its PR 2 (the root `.factory-ci.toml`) is merged; until then the plan skips that repo and the check waits.
- **SC-004**: Once factory-monorepo PR 2 is merged, a line a monorepo step prints is searchable in Loki within 30 seconds, and a signed-out reader of the public run log finds none of: a log credential, a branch, a SHA, a project path, a secret name.
- **SC-005**: With `BWS_ACCESS_TOKEN` unset, the first job stops within one step with one fixed line that names the Environment. With Bitwarden reachable and a pipeline that lists no secret, the `run` job makes one Bitwarden call, not two.
- **SC-007**: `gh secret list --env factory-ci` shows exactly `BWS_ACCESS_TOKEN`, `gh secret list` shows nothing, and the repository variables are `FACTORY_CI_BWS_SERVER_URL` or none.
- **SC-006**: Setup takes the owner one pass through the README, with no step missing (checked by the quickstart).

## Open Questions

None block the plan. Each has a default the plan uses.

- **OQ-4** (closed, round 4): the only variable is `FACTORY_CI_BWS_SERVER_URL`, a repository variable.
- **OQ-5 (selftest duplicates)**: Drop the selftest's shell and lint jobs once the runner runs the same checks? Default: keep them until the runner has passed 10 ticks; a later feature removes them.

Closed by the lead (review round 1): OQ-1 scheduler is cron-job.org (optional since R2-38: ticks start by hand); OQ-2 `allow_docker` is off on both entries (tests that need an engine are `#[ignore]`d, FR-030); OQ-3 was two Loki tokens (superseded by round 4: one set); OQ-6 is selftest jobs only as required checks (D3).

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
- Ticks start only by hand from the Actions tab (R2-38). A scheduler and a heartbeat are optional.
