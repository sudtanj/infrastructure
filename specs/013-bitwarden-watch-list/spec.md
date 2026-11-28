# Feature Specification: Bitwarden watch list and a 5-minute schedule for the factory CI tick

**Feature Branch**: `013-bitwarden-watch-list`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "One general CI flow: the rust-ci-runner image scans every repo listed in a Bitwarden secret, finds each repo's `.factory-ci.toml` and runs it on push (and, through feature 011, on schedule). The watch list moves from the committed public file `.github/factory-ci-repos.toml` to the Bitwarden key `FACTORY_CI_REPOS`. Bitwarden is the only source. The tick gets a GitHub `schedule:` trigger every 5 minutes."

## Who this is for

The owner, who keeps private repositories under the factory CI and does not want their names in a public repository. No AI agent ever sees a Bitwarden credential or a secret value; tests use fakes.

## Problem

- The list of watched repositories sits in `.github/factory-ci-repos.toml`, in a public repo. Anyone can read the name of every repo the owner watches, private ones included.
- The tick (`factory-ci-tick.yml`) only runs when something dispatches it. The owner wants GitHub to start it every 5 minutes by itself.
- A repo name is printed today in public places that were harmless while the list was public: the tick's step summary (`- item N: <repo>`, `problems:` lines that start with `<repo>:`, the rate-limit list) and the messages of `watch::parse` (an entry's label is its repo name; some messages quote a branch glob, a secret name or a value). Once the list is confidential these lines leak it.
- Git history keeps every name ever committed. This feature cannot hide the past (see Out of Scope).

## Announcement

The list of watched repositories now lives in the Bitwarden secret `FACTORY_CI_REPOS`, in the same project as the other factory CI keys. It uses the same `[[watch]]` format as before. The committed `.github/factory-ci-repos.toml` shrinks to a comment-only stub and lists nothing. The tick starts itself every 5 minutes. The public log and the step summary name a watch entry by its number only.

## Clarifications

### Session 2026-11-28 (owner decisions, brief)

- Q: Where does the list live? -> A: Bitwarden key `FACTORY_CI_REPOS`, project of `FACTORY_CI_BWS_PROJECT`, read with `BWS_ACCESS_TOKEN`. Same `[[watch]]` format and validation as the committed file.
- Q: Is the file still a source? -> A: No. Bitwarden is the only source.
- Q: Is the schedule a workflow edit? -> A: Yes, `schedule:` with `cron: "*/5 * * * *"` in `factory-ci-tick.yml`, keeping `workflow_dispatch`.
- Q: May the list be read in the run jobs? -> A: No. Only the plan job and the manual `ci` command read it.
- Q: Which files may change? -> A (updated by the owner during planning): the five workflow YAML files (`factory-ci-tick`, `factory-ci-selftest`, `rust-ci-runner-release`, `factory-monorepo-ci`, `factory-ci-mirror-images`) and Rust. Every `.github/scripts/*.sh` stays frozen.

### Session 2026-11-28 (planning decisions, no owner question left)

- Q: Stub or delete the file? -> A: Keep a comment-only stub. Deleting needs `.github/scripts/test-factory-monorepo-ci.sh` to change: it requires exactly one `docker run`, exactly three `-v` mounts and the mount text `/etc/factory-ci/repos.toml:ro` in `factory-monorepo-ci.yml`. Without the file, Docker would create a directory at the mount source on the VM. The stub also keeps the frozen `--repos .github/factory-ci-repos.toml` argument of `factory-ci-plan.sh` valid in a checkout, and keeps `plan --check` working in the selftest.
- Q: How does the tick's plan runner get the list, when `factory-ci-plan.sh` strips `BWS_ACCESS_TOKEN` from every runner call and cannot change? -> A: A new plan-job step runs a new runner subcommand, `watch-list`, before `plan`. It reads Bitwarden with the token and writes the validated text over the stub path in the VM's checkout (mode 0600). `factory-ci-plan.sh` then reads that file as it does today, with no token. A final `always()` step deletes the file. The VM is discarded after the job. The file is never uploaded.
- Q: How does the manual `ci` command get the list, when its workflow is pinned by shell tests to one `docker run`? -> A: `ci` reads `FACTORY_CI_REPOS` in-process, from the one `bws` call it already makes for its four settings. The mounted stub is ignored, but the argument stays.
- Q: Why does `.github/scripts/factory-ci-lint-workflow.sh` not need an edit for the schedule? -> A: Its rule `no-schedule` fails on any `schedule:`. The selftest YAML (editable) lints a copy of the workflows directory from which the one cron block was removed, and a separate step asserts the block is exactly one cron line `*/5 * * * *`. The lint script and its self-test stay as they are.
- Q: When does the cron line land? -> A: In its own last pull request, after one by-hand tick has read the list from Bitwarden. Merging it starts ticks at once.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The tick reads the watch list from Bitwarden (Priority: P1)

The owner stores the list in `FACTORY_CI_REPOS`. A tick, started by hand or by the schedule, plans exactly the repos in that value.

**Why this priority**: it is the point of the feature. Without it repo names stay public.

**Independent Test**: runner tests drive `watch-list` with a fake `bws` that returns a secret list, then `plan` phase A over the written file; the candidates come from the Bitwarden value.

**Acceptance Scenarios**:

1. **Given** a Bitwarden project holding a valid `FACTORY_CI_REPOS` and a stub in the checkout, **When** the `watch-list` step and phase A run, **Then** phase A plans the entries of the Bitwarden value.
2. **Given** the same project, **When** the run jobs start, **Then** none of them holds `BWS_ACCESS_TOKEN` through a new path, reads `FACTORY_CI_REPOS` or receives its text (the workflow lint and a YAML check prove it).
3. **Given** `FACTORY_CI_REPOS` holds the two `[[watch]]` tables of today's file, **When** a tick runs before and after the stub lands, **Then** the planned rows are identical.

---

### User Story 2 - A bad or missing list stops the tick and leaks nothing (Priority: P1)

The key is missing, duplicated, empty, malformed or unreachable. The tick fails closed: it posts no status, starts no run job and says what is wrong without naming any repo.

**Why this priority**: a wrong list must never plan a wrong set of repos, and its error text sits in a public log.

**Independent Test**: unit tests with a fake `bws` for each failure, and a hostile list whose repo names, branches, secret names and values are marker strings. The markers never appear in stdout, stderr, the summary or any file left behind.

**Acceptance Scenarios**:

1. **Given** no `FACTORY_CI_REPOS` key, **When** `watch-list` runs, **Then** it exits 1, leaves no output file and prints `watch list: key FACTORY_CI_REPOS not found`.
2. **Given** two keys with that name, an empty or blank value, a value over 64 KiB, or a value with no `[[watch]]` table, **When** `watch-list` runs, **Then** it exits 1 with one fixed line each and no output file.
3. **Given** a value that fails validation, **When** `watch-list` runs, **Then** every error line names only `entry N` or `line N` and a field, never a repo, branch, secret name or value.
4. **Given** the secret store is down or rejects the token, **When** `watch-list` runs, **Then** it exits 1 with `secret store unreachable` or `secret store rejected the access token`.
5. **Given** the `watch-list` step failed, **When** the verdict job runs, **Then** the tick is red, the `plan` step never ran and no commit status was posted.
6. **Given** a stub with no entries reaches phase A (the step was skipped), **When** phase A runs, **Then** it exits 1 with `watch list has no entries` and posts nothing.

---

### User Story 3 - The public output names an entry by number only (Priority: P1)

The step summary and the plan's error lines drop repo names. An entry is `entry N`, N counted from 1 in the order of the Bitwarden value.

**Why this priority**: moving the list out of git is useless if the summary prints it back.

**Independent Test**: a tick over a fake list with marker repo names; the summary text, problem lines and stderr contain no marker.

**Acceptance Scenarios**:

1. **Given** a tick that plans two items, **When** the summary is written, **Then** it reads `- item 0` and `- item 1` and names no repo.
2. **Given** an API problem for the second entry, **When** the summary is written, **Then** the line starts `entry 2:`.
3. **Given** a rate limit, **When** the summary lists unchecked repos, **Then** it lists entry numbers.
4. **Given** an API error text that quotes a repo name, **When** it becomes a problem line, **Then** the name is masked.

---

### User Story 4 - The manual `ci` command takes the list from Bitwarden (Priority: P2)

`ci` reads `FACTORY_CI_REPOS` in the one `bws` call that already brings its four settings, and checks the repo and ref against it. A repo that is not listed gets the old fixed line.

**Why this priority**: the manual workflow stays usable; the tick does not depend on it.

**Independent Test**: `ci_tests` with a fake `bws` serving the list and the four settings; the existing file-based tests keep running through a test-only seam.

**Acceptance Scenarios**:

1. **Given** a listed repo and ref, **When** `ci` runs, **Then** it proceeds exactly as before, with one `bws` call.
2. **Given** an unlisted repo, **When** `ci` runs, **Then** it exits 1 and prints only `[runner] repository or ref not in the watch list`.
3. **Given** no `BWS_ACCESS_TOKEN` (the release smoke test), **When** `ci` runs, **Then** it exits 1 and prints exactly that same line.
4. **Given** the key is missing, duplicated, empty or malformed, **When** `ci` runs, **Then** it exits 1 with the fixed line `[runner] watch list unusable`.

---

### User Story 5 - The tick starts itself every 5 minutes (Priority: P2)

GitHub starts the tick on a cron schedule. Ticks stay serial.

**Why this priority**: it is the owner's second request, and it does not depend on the list.

**Independent Test**: the selftest asserts the file holds `workflow_dispatch:` and `schedule:` with the single line `- cron: "*/5 * * * *"`, and runs the unchanged lint and its self-test on the copy without that block.

**Acceptance Scenarios**:

1. **Given** the tick file, **When** the selftest runs, **Then** exactly one `cron:` line exists and equals `*/5 * * * *`.
2. **Given** a second cron line or `pull_request_target:`, **When** the selftest runs, **Then** it fails.
3. **Given** a scheduled event on `main`, **When** the `plan` job starts, **Then** the existing `if: github.ref == 'refs/heads/main'` is true.

---

### User Story 6 - The committed file is a stub and says so (Priority: P3)

`.github/factory-ci-repos.toml` holds comments only. A test fails if a `[[watch]]` table is committed there again.

**Why this priority**: it keeps the frozen path valid and stops a regression that would publish the list.

**Independent Test**: `tests/repo_files.rs` parses the committed file and asserts zero entries.

**Acceptance Scenarios**:

1. **Given** the stub, **When** `plan --repos .github/factory-ci-repos.toml --check` runs without Bitwarden, **Then** it prints `0 entries ok` and exits 0.
2. **Given** a file with one `[[watch]]` table, **When** `repo_files` runs, **Then** it fails.

---

### User Story 7 - The owner checks a list before saving it (Priority: P3)

The owner pipes the list into `plan --check --repos -` on their own machine and sees every problem at once, in the same number-only form.

**Why this priority**: a typo in the Bitwarden value stops every tick. A pre-check is cheaper than a red tick.

**Independent Test**: a CLI test feeds stdin to `plan --check --repos -`.

**Acceptance Scenarios**:

1. **Given** a valid list on stdin, **When** the command runs, **Then** it prints `N entries ok` and exits 0.
2. **Given** an invalid list, **When** the command runs, **Then** it prints `entry N: ...` lines and exits 1.

---

### Edge Cases

- The key is absent, appears twice, is empty or whitespace, is over 64 KiB, has no table, has a TOML syntax error, or holds 101 tables.
- Bitwarden answers 401 or 403, times out, or returns something that is not a JSON list.
- Two ticks overlap (cron, the outside dispatcher, a by-hand dispatch).
- GitHub delays or skips a scheduled run, or disables the schedule after 60 days without repo activity.
- The list changes between the `watch-list` step and phase A of one tick: phase A reads the file the step wrote, so one tick sees one version. Phase C never reads the list.
- A value contains Unicode, CRLF line endings or a NUL.
- A failed fetch leaves a stale file (the stub, or an earlier list) that phase A would read.
- A repo is removed from the list: its old statuses stay; nothing is posted again.
- A Bitwarden secret value has a size limit (see Assumptions).

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The runner MUST read the watch list from the Bitwarden key `FACTORY_CI_REPOS` and from nowhere else, in two places only: the `watch-list` subcommand (tick) and the `ci` command (manual). `job`, `status`, `run` and `plan` phase C MUST NOT read it.
- **FR-002**: The value MUST use the existing `[[watch]]` format and validation (`watch::parse`): same keys, same limits (64 KiB, 100 entries, 20 items per list), same unknown-key refusal.
- **FR-003**: `watch-list` MUST fail closed (exit 1, no output file, an existing output file removed) when the key is missing, appears more than once, is empty or blank, is over 64 KiB, yields zero entries, fails validation, or the secret store cannot be read. Each case prints one fixed line.
- **FR-004**: Any text produced about the list (stdout, stderr, the summary file, a candidates file, a status, a job output, an artifact, a panic message) MUST NOT contain a repo name, branch or glob, secret name, id value or any other part of the list's content. It MAY contain the key name `FACTORY_CI_REPOS`, `entry N`, `line N`, a field name from a fixed set, a count and a fixed reason.
- **FR-005**: The plan summary and the plan's problem lines MUST name an entry as `entry N` and an item as `item N`. The rate-limit list MUST list entry numbers. The names read from the list MUST also join the redactor so an API error that quotes one is masked.
- **FR-006**: `watch-list` and `ci` MUST read `BWS_ACCESS_TOKEN` once and remove it from the process environment before any thread or child starts, and MUST add it to their redactor. The plan step and phase A MUST NOT receive it.
- **FR-007**: The tick's file MUST be written with mode 0600 at `.github/factory-ci-repos.toml` in the VM checkout, removed by a final `if: always()` step, and never uploaded.
- **FR-008**: `ci` MUST read the list from the same `bws` call that brings its four settings. An unlisted repo or ref, and an empty token, MUST print only `[runner] repository or ref not in the watch list` and exit 1. A missing, duplicate, empty or malformed list MUST print only `[runner] watch list unusable` and exit 1.
- **FR-009**: `plan` phase A MUST refuse a list with zero entries (`watch list has no entries`, exit 1, no candidates file). `plan --repos FILE --check` MUST still accept a file with zero entries and print `0 entries ok`. `plan --check --repos -` MUST read the list from stdin with the number-only error form of FR-004. Neither calls the network.
- **FR-010**: `.github/factory-ci-repos.toml` MUST contain comments only, and a test MUST fail when it contains a `[[watch]]` table. The comments name no repo.
- **FR-011**: `factory-ci-tick.yml` MUST gain `schedule:` with `- cron: "*/5 * * * *"`, keep `workflow_dispatch:`, gain a `watch-list` step in the `plan` job (after `install bws`, before `plan`) and a final cleanup step. No other change to its jobs.
- **FR-012**: The selftest MUST assert exactly one cron line equal to `*/5 * * * *`, and MUST run the unchanged workflow lint and its self-test on a copy without the schedule block.
- **FR-013**: `watch-list` MUST pass the project given by `--project` (the variable `FACTORY_CI_BWS_PROJECT`) to `bws secret list`, limit each attempt to 60 s, retry once on a transient failure and never on 401 or 403.
- **FR-014**: The constitution line naming the committed file MUST change to name the Bitwarden key, with a MINOR version bump and a Sync Impact note. The root README and `rust-ci-runner/README.md` MUST describe the new source, the stub, the schedule and its limits. The README update and the GitHub repo description are the last task.
- **FR-015**: The rollout order MUST be: key in Bitwarden, then runner release and digest, then the stub, the `watch-list` step and the manual workflow pin, then the cron line. At no step does the tick watch nothing.

### Key Entities

- **Watch list**: the TOML text in `FACTORY_CI_REPOS`. Entries as in `watch::WatchEntry`, each with a 1-based `number`.
- **Stub file**: `.github/factory-ci-repos.toml`, comments only in git; on the tick VM, the list for the length of one job.
- **List error**: a fixed message, optionally with `entry N` or `line N` and a field name.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After the stub lands, a search of the public repo's working tree and of the next 20 Actions logs and summaries finds zero names of watched private repos.
- **SC-002**: The first tick that reads the list from Bitwarden plans the same rows as the committed file did.
- **SC-003**: For each of the 8 failure cases of FR-003 and FR-009 a test shows exit 1, no output file, no status posted and no marker string in any output.
- **SC-004**: After one week the Actions history shows a tick at least every 15 minutes in a 24-hour window (best effort, see Assumptions).
- **SC-005**: The unit and integration suites, the selftest and the workflow lint self-test pass on the branch before the stub merges.

## Assumptions

- GitHub cron is best effort: runs may start late or be skipped under load, and GitHub disables scheduled workflows after 60 days without repo activity. The README says so. 5 minutes is the smallest interval GitHub accepts.
- The existing concurrency group `factory-ci-tick` with `cancel-in-progress: false` keeps ticks serial. A newer pending tick replaces an older pending one. The scheduled run, a by-hand dispatch and the outside dispatcher can coexist; a double tick is idempotent because statuses are posted before the run.
- `if: github.ref == 'refs/heads/main'` is true for schedule events, which always run on the default branch.
- `FACTORY_CI_BWS_PROJECT` names the project that holds the key. In `ci`, which has no project argument today, the machine-account token scopes the call, and a second project holding the key makes the count 2, so the call fails closed.
- Bitwarden Secrets Manager limits the size of a secret value (believed to be 3,500 characters; not verified). Two entries use about 150 characters, so about 25 entries fit. A longer list needs a second key (out of scope).
- The `allow_docker` repos are listed in the same value. The Docker project token never reads the list.
- Every tick now makes one Bitwarden call, even an idle one (the README sentence "An idle tick makes no Bitwarden call" changes).
- Feature 012 merges first. Feature 011 is docs only (plan.md "Merge order").
- The manual workflow input `repo` is typed by the owner and shows on the public run page; the owner should not start a private repo that way. The README says so.

## Out of Scope

- A second key for lists over the size limit; reading the list in run jobs; any new watch key; encrypting the plan summary; moving the heartbeat.
- Removing names already in git history. The owner must rename or accept them; rewriting history is not planned.
- Any edit to `.github/scripts/`.
