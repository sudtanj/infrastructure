# Feature Specification: Bitwarden watch list, a 5-minute schedule, and a clean public repo

**Feature Branch**: `013-bitwarden-watch-list`

**Created**: 2026-11-28

**Status**: Draft (revision 2, after the owner's decisions and three plan reviews of 2026-11-28)

**Input**: User description: "One general CI flow: the rust-ci-runner image scans every repo listed in a Bitwarden secret, finds each repo's `.factory-ci.toml` and runs it on push (and, through feature 011, on schedule). The watch list moves from the committed public file `.github/factory-ci-repos.toml` to the Bitwarden key `FACTORY_CI_REPOS`. Bitwarden is the only source. The tick gets a GitHub `schedule:` trigger every 5 minutes. Names of private repos leave the public repo, history included."

## Naming in this spec

This spec is public. It writes no private repo name, only the old file names of the manual workflow and its test where an engineer needs the path before T030 renames them (T030 scrubs those too). `PRIVATE_REPO` means the owner's private monorepo that sits in the watch list. `OLD_MANUAL` means the manual workflow file and its shell test, named after that repo. `NEW_MANUAL` is `factory-ci-manual` (the workflow file `.github/workflows/factory-ci-manual.yml`, its shell test `.github/scripts/test-factory-ci-manual.sh`). The exact strings are derived with git commands in `contracts/history-rewrite.md`, not written here.

## Who this is for

The owner, who keeps private repositories under the factory CI and does not want their names in a public repository. No AI agent ever sees a Bitwarden credential or a secret value; tests use fakes.

## Problem

- The list of watched repositories sits in `.github/factory-ci-repos.toml`, in a public repo. Anyone can read the name of every repo the owner watches, private ones included.
- Other files name `PRIVATE_REPO` too: the root README, the default of the manual workflow's `repo` input, the file names of the manual workflow and its test, older specs.
- The tick only runs when something dispatches it. The owner wants GitHub to start it every 5 minutes.
- Public output prints list content today: the plan summary (`- item N: <repo>`, `<repo>: <why>`, the rate-limit list), `main.rs` (`<repo>: FACTORY_CI_TOKEN_DOCKER is not set`), `plan.rs` (`id N in the list, but the API says M`), and `watch::parse` messages (entry labels are repo names; some quote globs, secret names and values).
- Under a `schedule` event `github.event.repository` is empty, so `B_PRIVATE` is empty and `main.rs` treats an empty value as "the runner repo is private" (`!= "false"`), which turns off the private-repo log-store guard. That is unsafe once a cron runs the tick.
- The heartbeat pings after a failed plan, so a bad list does not silence the dead-man alarm.
- Git history keeps every name ever committed.

## Announcement

The list of watched repositories lives in the Bitwarden secret `FACTORY_CI_REPOS`, in the project of `FACTORY_CI_BWS_PROJECT`, in the same `[[watch]]` format as before. The committed `.github/factory-ci-repos.toml` is deleted. The tick starts itself every 5 minutes and pings its heartbeat only after a good plan. The public log and summary name a watch entry by its number. The manual workflow is renamed `factory-ci-manual`. After the owner's go, the public history is rewritten to remove the private names.

## Clarifications

### Session 2026-11-28 (owner decisions, brief)

- Q: Where does the list live? -> A: Bitwarden key `FACTORY_CI_REPOS`, project of `FACTORY_CI_BWS_PROJECT`, read with `BWS_ACCESS_TOKEN`; same `[[watch]]` format and validation. Bitwarden is the only source; the committed file stops listing repos.
- Q: Is the schedule a workflow edit? -> A: Yes, `schedule:` with `cron: "*/5 * * * *"` in `factory-ci-tick.yml`, keeping `workflow_dispatch`.
- Q: May the list be read in the run jobs? -> A: No. Only the plan job and the manual `ci` command read it.

### Session 2026-11-28 (owner decisions, revision 2)

- Q: Which files may change? -> A: The five workflow YAML files and, now, `.github/scripts/*.sh`.
- Q: Stub or delete the file? -> A: Delete `.github/factory-ci-repos.toml` outright. No stub. (Earlier planning kept a stub only because the manual workflow's shell test pinned its mount; scripts are editable now, so the mount goes.)
- Q: How does the plan runner get the list? -> A: Design B. The plan script reads `FACTORY_CI_REPOS` with the existing shell helpers `fci_list` and `fci_key_value` (`factory-ci-secrets.sh`), writes the value to a private temp file made with `mktemp` (mode 0600, created exclusively), passes that file to the runner with `--repos`, and removes it. The runner never receives the Bitwarden token, which keeps the script's rule that the runner needs no store token. Rejected: letting phase A read Bitwarden (puts the token and the parsers of third-party repo data in one process; adds a store path to the runner for the tick; duplicates what the shell helpers do with a tested retry and 401/403 handling). The `watch-list` subcommand of revision 1 is dropped.
- Q: How does the manual `ci` command get the list? -> A: In-process, from the one `bws` call it already makes, using `FACTORY_CI_BWS_PROJECT`. The manual workflow loses the `--repos` argument and the mount. `ci` keeps `--repos FILE` only as a development and test option; when it is absent the source is Bitwarden.
- Q: How is the cron allowed by the lint? -> A: Edit the lint rule `no-schedule`: a `schedule:` is allowed only as exactly one `- cron: "*/5 * * * *"` line. The stripped-copy workaround is dropped.
- Q: Is the manual workflow renamed? -> A: Yes, to `factory-ci-manual`, because its file name reveals `PRIVATE_REPO`. Its tests that pin the name are updated.
- Q: Does the owner want the public history rewritten? -> A: Yes. It is the very last task: irreversible, a force-push, run by the lead only after a fresh explicit owner go, and only after 012, 011 and 013 have merged and no other branch is open. The current tree is scrubbed first in a normal commit.
- Q: Which cron offset? -> A: `*/5`, as the owner said. A skewed minute list such as `3-58/5` avoids the top-of-the-hour load; recorded as an option only.

### Session 2026-11-28 (security review findings, all accepted)

- H1: The Environments `factory-ci-plan`, `factory-ci`, `factory-ci-docker` and the one the verdict job uses get a deployment branch rule of `main` only and no required reviewer (a reviewer would stall every cron run). The Bitwarden token, both GitHub tokens and the blob keys exist as Environment secrets only. A branch ruleset protects `.github/**` and `rust-ci-runner/**`.
- M1: no public text names a repo, id or branch from the list, including `main.rs`'s `FACTORY_CI_TOKEN_DOCKER is not set` line and `plan.rs`'s id line. A permanent hostile-list test covers every phase, including an `allow_docker` entry with no docker token. Feature 011 must pass it.
- M2: `B_PRIVATE` is private only when it equals `true`; empty or anything else counts as public, which is the fail-closed side (`fail_closed = repo.private && !b_private`).
- M3: the phase C redactor is seeded from the candidate rows. A value under 4 characters is never masked, so messages never quote list text.
- M4: the repo owner must equal `GITHUB_REPOSITORY_OWNER`; `id` is required when `allow_docker` is true or `secrets` is non-empty; a bare `*` or `**` in `secret_branches` is refused. The owner makes both GitHub tokens fine-grained and scoped to the intended repos.
- M5: the heartbeat is sent only when the plan succeeded and `fail_tick` is not 1.
- L1: every file that holds list text is made with an exclusive create at mode 0600 and removed after use (`mktemp`).
- L2: `FACTORY_CI_BWS_PROJECT` reaches `ci`.
- L3, L4: the README lists what stays visible in public (counts, item numbers, run timing, the presence of `allow_docker` rows, the manual workflow input).

### Session 2026-11-28 (devops and critic findings, all accepted)

- The Bitwarden value size limit is unverified. The only source found is the SDK's input validation (read through DeepWiki), which caps a value at 25,000 characters; the docs give no number. The owner confirms it (T002). The earlier guess of 3,500 is dropped. The parser's own 64 KiB and 100-entry caps stay.
- The cutover is split: first the fetch lands with the committed file still present and the owner compares rows; the file is deleted later in its own pull request, after the owner re-checks that the Bitwarden value equals the committed file (feature 012's follow-up adds `allow_docker` and the Docker Hub secret names to the infrastructure entry).
- T016 records the previous `RUNNER_IMAGE_DIGEST` before overwriting it. Rolling it back below 0.5.0 makes the public summary print repo names again and drops the M1 to M4 rules; it is allowed only together with `gh workflow disable factory-ci-tick`.
- Schedule monitoring counts Canceled runs (concurrency replacements) apart from Failed runs. The README documents the failure email per tick and the stop command `gh workflow disable factory-ci-tick`. The account that merges the cron line is the owner's, because GitHub notifies that account.
- `plan --check` and phase A agree: both refuse a list with zero entries.
- Feature 011 contradicts this spec in several places (no cron line, tick unchanged, a second `bws` call in `ci`); its documents are reconciled in the same session.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The tick reads the watch list from Bitwarden (Priority: P1)

The owner stores the list in `FACTORY_CI_REPOS`. A tick, by hand or by the schedule, plans exactly the repos in that value.

**Why this priority**: it is the point of the feature.

**Independent Test**: the shell tests run `factory-ci-plan.sh` against a fake `bws` that serves a secret list; the runner stub receives `--repos` with a 0600 file that holds the Bitwarden value.

**Acceptance Scenarios**:

1. **Given** a project holding a valid `FACTORY_CI_REPOS`, **When** the plan script runs, **Then** phase A receives that value in a mode-0600 file and the file is gone when the script ends.
2. **Given** the same project, **When** the run jobs start, **Then** none of them holds the list, reads the key or receives its text (the workflow lint and a selftest check prove it).
3. **Given** a value equal to the committed file, **When** a tick runs before and after the cutover, **Then** the planned rows are identical.
4. **Given** the plan runner runs, **When** its environment is read, **Then** it holds no Bitwarden token.

---

### User Story 2 - A bad or missing list stops the tick and leaks nothing (Priority: P1)

The key is missing, duplicated, empty, malformed or unreachable. The tick fails closed: it posts no status, starts no run job, and says what is wrong without naming any repo.

**Why this priority**: a wrong list must never plan a wrong set of repos, and its error text is public.

**Independent Test**: shell tests with a fake `bws` for each failure; Rust tests with a hostile list whose repo names, branches, secret names and values are marker strings. No marker appears in stdout, stderr, the summary or any file left behind.

**Acceptance Scenarios**:

1. **Given** no `FACTORY_CI_REPOS` row, **When** the script runs, **Then** it exits 1, the summary says `watch list key FACTORY_CI_REPOS not found` and phase A never runs.
2. **Given** two rows with that key, or an empty value, **When** the script runs, **Then** it exits 1 with one fixed line each.
3. **Given** a value that fails validation, **When** phase A runs, **Then** every error line names only `entry N` or `line N` and a field, never a repo, branch, secret name or value.
4. **Given** the store is down or rejects the token, **When** the script runs, **Then** it exits 1 with the existing `secret store unreachable` or `secret store rejected the token`.
5. **Given** `BWS_ACCESS_TOKEN` is empty, **When** the script runs, **Then** it exits 1 with `BWS_ACCESS_TOKEN is not set`.
6. **Given** a list with zero entries, **When** phase A or `plan --check` runs, **Then** it exits 1 with `watch list has no entries`.
7. **Given** any of these failures, **When** the verdict job runs, **Then** the tick is red, no status was posted and the heartbeat was not sent.

---

### User Story 3 - The public output names an entry by number only (Priority: P1)

The plan summary and the plan's error lines drop repo names. An entry is `entry N`, N counted from 1 in the order of the Bitwarden value; an item is `item N`.

**Why this priority**: moving the list out of git is useless if the summary prints it back.

**Independent Test**: the permanent hostile-list test (`plan_hostile_tests`) over `plan_a`, `plan_c`, `--check` and `ci`.

**Acceptance Scenarios**:

1. **Given** a tick that plans two items, **When** the summary is written, **Then** it reads `- item 0` and `- item 1` and names no repo.
2. **Given** an API problem for the second entry, **When** the summary is written, **Then** the line starts `entry 2:`.
3. **Given** a rate limit, **When** the summary lists unchecked repos, **Then** it lists entry numbers.
4. **Given** an `allow_docker` entry and no docker token, **When** phase A runs, **Then** the problem line is `entry N: FACTORY_CI_TOKEN_DOCKER is not set`.
5. **Given** an id mismatch, **When** the problem is written, **Then** it carries no id.
6. **Given** an API error text that quotes a repo name, **When** it becomes a problem line, **Then** the name is masked in phase A and in phase C.

---

### User Story 4 - The manual `ci` command takes the list from Bitwarden (Priority: P2)

`ci` reads `FACTORY_CI_REPOS` in the one `bws` call that already brings its four settings, scoped by `FACTORY_CI_BWS_PROJECT`, and checks the repo and ref against it. A repo that is not listed gets the old fixed line.

**Why this priority**: the manual workflow stays usable; the tick does not depend on it.

**Independent Test**: `ci_tests` with a fake `bws` serving the four settings and the list; the existing file-based tests keep working through `--repos FILE`.

**Acceptance Scenarios**:

1. **Given** a listed repo and ref, **When** `ci` runs without `--repos`, **Then** it proceeds as before with one `bws` call.
2. **Given** an unlisted repo, **When** `ci` runs, **Then** it exits 1 and prints only `[runner] repository or ref not in the watch list`.
3. **Given** no token (the release smoke test), **When** `ci` runs, **Then** it prints exactly that line and exits 1.
4. **Given** the key is missing, duplicate, empty or malformed, **When** `ci` runs, **Then** it exits 1 with `[runner] watch list unusable`.
5. **Given** an entry whose owner differs from `GITHUB_REPOSITORY_OWNER`, **When** `ci` runs, **Then** the list is unusable.

---

### User Story 5 - The tick starts itself every 5 minutes and pings only after a good plan (Priority: P2)

GitHub starts the tick on a cron schedule. Ticks stay serial. The heartbeat is sent only after a successful plan.

**Why this priority**: it is the owner's second request; the heartbeat rule closes a silent-failure gap.

**Independent Test**: the lint (rule edited) accepts exactly one cron line `*/5 * * * *` and rejects a second line or `pull_request_target:`; a lint mutation proves it; a selftest step asserts the plan step has no `continue-on-error` and no `if:`; a lint rule asserts the heartbeat step has no `if: always()`.

**Acceptance Scenarios**:

1. **Given** the tick file, **When** the lint runs, **Then** `workflow_dispatch:` is present and `schedule:` holds exactly one cron line equal to `*/5 * * * *`.
2. **Given** a second cron line, **When** the lint runs, **Then** it fails.
3. **Given** a scheduled event on `main`, **When** the `plan` job starts, **Then** the existing `if: github.ref == 'refs/heads/main'` is true.
4. **Given** a failed plan or `fail_tick=1`, **When** the verdict job ends, **Then** no heartbeat request is made.
5. **Given** an empty `B_PRIVATE` (every scheduled run), **When** a private watched repo is planned and no log store is configured, **Then** the row is refused.

---

### User Story 6 - The committed file is gone and stays gone (Priority: P3)

`.github/factory-ci-repos.toml` does not exist. A test fails if it comes back.

**Why this priority**: it stops a regression that would publish the list.

**Independent Test**: `tests/repo_files.rs` asserts the path is absent. The selftest runs `plan --check` on a fixture with fake names.

**Acceptance Scenarios**:

1. **Given** the tree, **When** `repo_files` runs, **Then** the file is absent and no `*.toml` outside `rust-ci-runner/tests/fixtures` holds a `[[watch]]` table.
2. **Given** the example fixture, **When** the selftest runs `plan --repos <fixture> --check`, **Then** it prints `N entries ok`.

---

### User Story 7 - The owner checks a list before saving it (Priority: P3)

The owner pipes the list into `plan --repos - --check` on their own machine and sees every problem at once, in the number-only form, plus the entry count.

**Why this priority**: a typo in the Bitwarden value stops every tick; after every edit the owner also checks the count.

**Independent Test**: CLI tests feed stdin to `plan --repos - --check`.

**Acceptance Scenarios**:

1. **Given** a valid list on stdin, **When** the command runs, **Then** it prints `N entries ok` and exits 0.
2. **Given** an invalid list, **When** the command runs, **Then** it prints `entry N: ...` lines and exits 1.
3. **Given** the owner's rules (owner, id, `secret_branches`), **When** the list breaks one, **Then** the line names the entry and the rule.

---

### User Story 8 - The public repo carries no private name (Priority: P3)

The current tree is scrubbed in a normal commit, old run logs that print names are deleted, and the history is rewritten after the owner's explicit go.

**Why this priority**: the owner decided it; it is irreversible, so it is gated and last.

**Independent Test**: a grep for the derived strings over the tree (before the rewrite) and over a fresh mirror clone (after it).

**Acceptance Scenarios**:

1. **Given** the scrub commit, **When** the grep runs on the tree, **Then** it finds none of the strings and the manual workflow has its new name.
2. **Given** the rewrite ran, **When** the grep runs on a fresh mirror clone of every ref, **Then** it finds none.
3. **Given** the tick runs after the rewrite, **When** the walk reads the new tips, **Then** it plans one run per project at each watched tip and not one per commit.

---

### Edge Cases

- The key is absent, appears twice, is empty or whitespace, is over 64 KiB, has no table, has a TOML syntax error, holds 101 tables.
- Bitwarden answers 401 or 403, times out, or returns something that is not a JSON list.
- Two ticks overlap (cron, the outside dispatcher, a by-hand dispatch); the cron doubles the cadence while the outside dispatcher still runs.
- GitHub delays or skips a scheduled run, or disables the schedule after 60 days without repo activity.
- A repo owner differs from `GITHUB_REPOSITORY_OWNER`; `allow_docker` without `id`; `secret_branches = ["*"]`.
- A value contains Unicode, CRLF line endings or a NUL.
- `B_PRIVATE` empty, `true`, `false` or garbage.
- A repo is removed from the list: its old statuses stay; nothing is posted again.
- The history rewrite changes every SHA of the public repo, which is itself a watched repo.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The list MUST come from the Bitwarden key `FACTORY_CI_REPOS` and from nowhere else in production. It is read in two places only: the plan script (through `fci_list` and `fci_key_value`) and the `ci` command (through `store.rs`). `job`, `status`, `run`, plan phase C and every run job MUST NOT read it.
- **FR-002**: The value MUST use the existing `[[watch]]` format and validation, with the extra rules of FR-006: 64 KiB, 100 entries, 20 items per list, unknown keys refused.
- **FR-003**: The plan script MUST fail closed (exit 1, no phase A, no status, no output) when the token or project is missing, the store cannot be read, the key is missing, appears more than once, is empty, or is over 64 KiB. The text it writes to the summary MUST be fixed.
- **FR-004**: Any text produced about the list (stdout, stderr, the summary, a candidates file, a status, a job output, an artifact, a panic message) MUST NOT contain a repo name, id, branch, glob, secret name or any other list content. It MAY contain `FACTORY_CI_REPOS`, `entry N`, `item N`, `line N`, a fixed field name, a count and a fixed reason. `watch::parse` itself MUST produce only such messages.
- **FR-005**: The plan summary and problem lines MUST name entries as `entry N` and items as `item N`; the rate-limit list MUST list entry numbers; every `format!` that touches `e.repo`, `e.id` or `e.branches` in `main.rs`, `plan.rs`, `walk.rs` and `projects.rs` MUST be checked and fixed. A permanent hostile-list test MUST cover `plan --check`, phase A (including an `allow_docker` entry with no docker token), phase C and `ci`, and MUST be named in feature 011's tasks as a gate.
- **FR-006**: `watch::parse` MUST refuse: a repo owner that differs from `GITHUB_REPOSITORY_OWNER` (checked by `plan` phase A and by `ci` with the store source, case-insensitive; an unset variable refuses); `allow_docker = true` or a non-empty `secrets` without `id`; a `secret_branches` item that is `*` or `**`; a list with zero entries.
- **FR-007**: The Bitwarden token MUST NOT reach the plan runner, phase C, a run job or a step. The `ci` command reads it once and removes it from its environment.
- **FR-008**: Every file that holds list text MUST be created exclusively at mode 0600 and removed on every exit path. The plan script MUST discard every row of the secret list except `FACTORY_CI_REPOS` before running the runner.
- **FR-009**: `ci` MUST read the list from the same `bws` call as its four settings, scoped by `FACTORY_CI_BWS_PROJECT`. An unlisted repo or ref, and an empty token, MUST print only `[runner] repository or ref not in the watch list`; a missing, duplicate, empty, malformed or rule-breaking list MUST print only `[runner] watch list unusable`; one `bws` call per run.
- **FR-010**: `B_PRIVATE` MUST count as private only when it equals `true`.
- **FR-011**: The tick MUST gain `schedule:` with exactly one `- cron: "*/5 * * * *"`, keep `workflow_dispatch:`, and its heartbeat step MUST run only when the plan succeeded and `fail_tick` is not 1 (no `if: always()` on that step). The lint MUST allow exactly that cron line, reject any other schedule content and require the heartbeat condition. The selftest MUST assert that the `plan` step has no `continue-on-error` and no `if:`.
- **FR-012**: `plan --repos FILE --check` and `plan --repos - --check` MUST validate with the same parser and the same rules (owner rule excepted when `GITHUB_REPOSITORY_OWNER` is unset), print `N entries ok`, refuse zero entries, and make no network call.
- **FR-013**: `.github/factory-ci-repos.toml` MUST be deleted, and a test MUST fail if it exists or if another committed `.toml` outside the test fixtures holds a `[[watch]]` table.
- **FR-014**: The manual workflow MUST be renamed to `factory-ci-manual` (file, `name:`, its shell test, every pin and reference), lose the `--repos` argument and the mount, drop the default of its `repo` input, and receive `FACTORY_CI_BWS_PROJECT` and `GITHUB_REPOSITORY_OWNER`.
- **FR-015**: The current tree MUST be scrubbed of the private names in a normal commit before the history rewrite. The rewrite MUST follow `contracts/history-rewrite.md` and MUST NOT run without a fresh explicit owner go.
- **FR-016**: The constitution line naming the committed file MUST change, with a MINOR bump and a Sync Impact note. The root README and `rust-ci-runner/README.md` MUST describe the new source, the schedule and its limits, the heartbeat rule, the stop command, the accepted public leaks (L3, L4) and the rollback caution; the README update and the GitHub repo description are the last task before the rewrite.
- **FR-017**: The rollout MUST be: key in Bitwarden; runner release and digest (previous digest recorded); script, lint and heartbeat change with the committed file still present, rows compared; file deletion and manual workflow cutover in their own pull request after the owner re-checks the value; cron in its own pull request; tree scrub; docs; rewrite. At no step does the tick watch nothing.

### Key Entities

- **Watch list**: the TOML text in `FACTORY_CI_REPOS`. Entries as in `watch::WatchEntry`, each with a 1-based `number`.
- **List file**: a mode-0600 temp file on the tick VM for the length of phase A.
- **Scrub strings**: derived in `contracts/history-rewrite.md`.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: Before the rewrite: after the file is deleted and the tree is scrubbed, a case-insensitive search of the working tree for every scrub string finds zero hits, and the next 20 tick runs and manual runs show none in their logs and summaries. After the rewrite: the same search over a fresh mirror clone of every branch and tag, over all commit messages and over all paths finds zero hits, and the old run logs that printed names are deleted.
- **SC-002**: The first tick that reads the list from Bitwarden plans the same rows as the committed file did.
- **SC-003**: For each failure case of FR-003, FR-006 and FR-009 a test shows exit 1, no output file, no status posted and no marker string in any output.
- **SC-004**: After one week the Actions history shows a successful tick at least every 15 minutes in a 24-hour window; Canceled runs are counted apart from Failed.
- **SC-005**: Unit and integration suites, the shell tests, the workflow lint and its self-test, and the selftest pass on each branch before it merges.
- **SC-006**: After the rewrite the first tick plans exactly one row per project of each watched tip of the rewritten repo and no other row.

## Assumptions

- GitHub cron is best effort: runs may start late or be skipped under load, and GitHub disables scheduled workflows after 60 days without repo activity. `github.ref == 'refs/heads/main'` is true for schedule events. The concurrency group `factory-ci-tick` with `cancel-in-progress: false` serialises ticks and keeps at most one pending, so a replaced pending run shows as Canceled.
- Every tick now makes one Bitwarden call (two when it has work).
- `FACTORY_CI_BWS_PROJECT` names the project that holds the key and the other factory CI keys; the machine account is read-only and scoped to that one project.
- The Bitwarden value limit is unverified (25,000 characters is the only figure found, from SDK validation); the parser's caps are tighter.
- The `allow_docker` repos are listed in the same value; the Docker project token never reads it.
- Feature 012 merges first. Feature 011 is docs only and is reconciled with this spec (plan.md "Merge order and conflicts").
- The manual workflow input `repo` is typed by the owner and shows on the public run page.
- Write access to the Bitwarden project equals full CI compromise; the owner controls it.

## Out of Scope

- A second key for larger lists; reading the list in run jobs; new watch keys; encrypting the plan summary.
- Removing names from other people's forks or from caches GitHub holds: GitHub Support does that (the request is drafted, not sent).
- Any change to the history of other repositories.
