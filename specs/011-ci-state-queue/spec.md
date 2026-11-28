# Feature Specification: Scheduled runs, a strict queue, saved state and debug artifacts for the ci command

**Feature Branch**: `011-ci-state-queue`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "Run the `ci` pipelines on a schedule from reviewed config, queue runs that share a state strictly first in, first out, restore and save a pipeline's state (a Playwright storageState file) in an encrypted release on the private repo that owns the pipeline, and upload encrypted debug artifacts to that repo on failure. Builds on 010 (step secrets). Owner addition of the same day: the runner, never a step or an AI agent, unlocks the Bitwarden vault and hands a step only the login fields it declared; artifacts use their own key and are scanned for secrets before upload."

## Who this is for

The owner, who runs `factory-monorepo-ci` and writes the `.factory-ci.toml` files of the watched repos, and the AI agent the owner sends to debug a failed run. The first user is the Playwright browser-test project (spec 002 in `sudtanj/factory-monorepo`), which logs in to the owner's own sites with human-like input. A standing rule of the owner binds the whole feature: no AI agent ever sees a Bitwarden credential, and a pipeline step receives only the values it declared. The agent gets two things only: the artifact key and a Loki read token.

## Problem

`ci` starts only by hand, forgets everything between runs, and cannot say why a browser test failed. Evidence:

- `.github/workflows/factory-monorepo-ci.yml` has `workflow_dispatch` as its only trigger. The concurrency group is `factory-monorepo-ci-<repo>-<ref>-<project>` with `cancel-in-progress: false`. GitHub keeps one running and one pending run per group and cancels an older pending run when a third arrives, so a queue of three loses a run.
- `rust-ci-runner/src/ci.rs` fetches the branch into a new directory under `--work`, runs the pipeline and removes the directory (`remove_ws`). Nothing survives. A browser test that logs in with a username and password every run looks like a bot to the target site and may trip captchas or lockouts. It needs its cookies back.
- The public Actions log shows only fixed lines (`log.rs`). The step output that explains a failure goes to Loki, and a Playwright failure is mostly a trace and screenshots, which Loki lines cannot carry. An agent has nothing to read.
- A GitHub release asset on a private repo is downloadable by everyone with read access to that repo, so a cookie file stored there unencrypted is a session theft waiting for a read-only collaborator or a leaked read token.
- `FACTORY_CI_TOKEN` is read-only by design (spec 008). Writing a release needs a second token with contents write.

## Announcement

Pipelines run by `factory-monorepo-ci` can now run on a schedule, remember their state and leave evidence when they fail. Add an entry to `.github/factory-ci-schedule.toml` with a repo, a branch, a project and an interval, and the runner starts it for you. Declare a `[state]` file in the pipeline, and the runner restores the last good copy before the first step and saves the new one after a passing run. Runs that share a state wait in a strict queue and run one after another in the order they asked, with a fixed `waiting for the queue` line in the public log. When a run fails, the runner encrypts the traces and screenshots the pipeline declared and uploads them to a release on the private repo that owns the pipeline. Loki names the release and asset. An agent reads Loki, downloads the asset with `gh`, and decrypts it. State and artifacts are encrypted with two different keys from Bitwarden, and the runner holds the write token itself: steps can never request any of them. Before an artifact is encrypted, the runner scans it for every secret value it knows, and refuses to upload it if one is inside. A step that needs a website login declares the vault item and fields. The runner unlocks the vault, reads those fields, hands them to that step as environment variables, locks the vault and deletes its session before the step starts, so neither the step nor an agent ever holds a vault credential. Runner 0.4.0 carries the change.

## Clarifications

### Session 2026-11-28 (owner decisions)

- Q: Do runs start on a schedule? → A: Yes, besides manual dispatch. Schedule entries (repo, branch, project, interval) live in reviewed config in this repo, never in dispatch inputs. Scheduled runs go through the same watch-list checks as manual ones.
- Q: Is GitHub's concurrency group enough to serialize runs of a state? → A: No, it cancels older pending runs. The queue is strict FIFO: runs that share a state never overlap, and each requested run runs, in request order, one after another. The spec fixes the stale-lock rule, the maximum wait and the public line.
- Q: Where does state live? → A: In a release tag of the private repo that owns the pipeline. The runner restores it before the pipeline and saves it after.
- Q: Is state encrypted? → A: Yes. Anyone with read access to the private repo can download release assets, so the runner encrypts state before upload with a key from Bitwarden Secrets Manager. A failed run never overwrites the last good state. The first run, with no state, starts empty.
- Q: Are debug artifacts kept? → A: Yes, encrypted the same way, uploaded to a release on the private repo on failure (and on every run when the pipeline asks). Loki gets the release and asset name, the public log gets nothing. An agent reads Loki with a read token and downloads and decrypts with `gh`. Retention is bounded so the release does not grow forever.
- Q: Which tokens? → A: Two new runner settings read from Bitwarden: `FACTORY_CI_STATE_TOKEN` (fine-grained, contents write on the private repos that opt in) and `FACTORY_CI_STATE_KEY` (the encryption key). Like the four existing settings, steps may never request them. `FACTORY_CI_TOKEN` stays read-only.

### Session 2026-11-28 (owner decisions, second round)

- Q: Who may touch a Bitwarden credential? → A: The runner only. No AI agent ever sees one, and a pipeline script receives only what it needs.
- Q: How does a step get a website login from the Bitwarden vault (Password Manager)? → A: The runner holds the vault unlock credentials as runner settings in Bitwarden Secrets Manager, which steps may never request. The pipeline declares which vault login items and which fields a step needs. The runner unlocks the vault, reads only those, passes only those field values as step environment variables, adds them to the Redactor, then locks or logs out and removes the `bw` session and data directory before the step starts. A step never gets a session key or an unlock credential. Declared item names are allowlisted per repo in the watch list, the same way 010 allowlists secret names. Recommended: give the Bitwarden identity access to one collection that holds only those items.
- Q: One encryption key for state and artifacts? → A: No, two. Artifacts use their own key, `FACTORY_CI_ARTIFACT_KEY`, different from `FACTORY_CI_STATE_KEY`. An agent gets the artifact key and a Loki read token and nothing else: never the state key, the state token, the Bitwarden access token or a vault credential. State (cookies) counts as a credential and is never part of a debug artifact. This replaces the earlier planning call of one key.
- Q: What stops a secret from landing in an artifact? → A: Before encrypting and uploading, the runner scans the bundle for every redacted value (exact, base64, each line, and inside zip entries for Playwright traces). A match refuses the upload with a fixed fault and keeps the run failed. Loki output stays redacted as before.
- Q: May the runner's own logs name vault items, fields or values? → A: No. Logs carry positions and counts only, for the vault.

### Session 2026-11-28 (planning calls, no owner answer needed; override any of them)

- Q: Where does the schedule live? → A: A new file `.github/factory-ci-schedule.toml`, checked by a `--check` mode like the watch list. Not in `factory-ci-repos.toml`, because the by-hand tick reads that file and a schedule key would change what the tick plans. A cron line in the workflow cannot hold per-entry intervals, so the workflow fires one fixed 15-minute cron and the runner decides which entries are due.
- Q: What does "request order" mean? → A: The order in which runs register a queue ticket. The runner learns the state name from the fetched pipeline file, so registration happens after the fetch and the pipeline checks, a few seconds to a minute after the dispatch. Two starts within that gap can swap places.
- Q: Which pipelines queue? → A: Those that declare a `[state]`, keyed by the state name inside the private repo that owns the pipeline. Pipelines with no state do not queue, and the workflow's concurrency group is removed because it cancels pending runs. Two stateless runs of one project can now overlap.
- Q: Which repos may keep state? → A: Only a watch entry with `state = true`, and only a private repo. The entry is a reviewed file on `main`, so a branch cannot widen it. A public repo cannot opt in, because its release assets would be public.
- Q: Stale lock? → A: Every ticket carries a renewal time that its run refreshes every 60 s while it waits and while it runs. A ticket not renewed for 10 minutes is stale and the next waiter removes it. A run that finds its own ticket gone never writes state.
- Q: Maximum wait and queue size? → A: At most 3 tickets per state (one running, two waiting). A fourth request ends at once with `queue is full`. A waiter gives up after 120 minutes. The workflow job limit rises from 45 to 190 minutes (120 wait, 45 hold, 25 for fetch and cleanup). A hold is at most 45 minutes: 5 restore, 5 vault read, 30 pipeline, 5 save.
- Q: Names of the vault settings? → A: `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD`. The runner hands them to the `bw` child as `BW_CLIENTID`, `BW_CLIENTSECRET` and `BW_PASSWORD`, in that child only. The `FACTORY_CI` prefix is already refused in every `secrets` list, so no new reserved-name rule is needed.
- Q: Where is the vault allowlist? → A: A new watch key `vault_items`, a list of exact item names, default none. A pipeline names the item and the field; the reviewed watch entry decides which items a repo may read. Fields are not allowlisted: the item is the boundary.
- Q: Which fields can a step read? → A: `username`, `password`, `uri` and custom fields, by exact name. The TOTP code and attachments are out of scope.
- Q: Does an artifact refusal fail a passing run? → A: Yes. A secret inside a declared artifact path is a leak in the making, so the run ends red even with `when = "always"` on a pass.
- Q: Retention? → A: State release: the 2 newest assets. Artifact release: the 10 newest bundles per project.
- Q: Are the new Bitwarden keys required? → A: Only when a run needs them. State needs the state token and the state key, artifacts need the state token and the artifact key, and a vault declaration needs the three vault settings. A run that declares none of them needs none, so existing runs keep working before the owner creates the keys.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - The browser test keeps its login between runs (Priority: P1)

The owner declares a `[state]` file in the Playwright pipeline and sets `state = true` on the `sudtanj/factory-monorepo` watch entry. Run 1 starts with no state, logs in, passes and saves the cookie file. Run 2 restores it and skips the login.

**Why this priority**: It is the reason for the feature. Without a restored session every run logs in again.

**Independent Test**: A fixture pipeline whose step writes a counter into the state file. Mock the GitHub API and the key store. Run `ci` three times. The step sees 0, 1, 2. The mock holds only encrypted bytes, and none of the plaintext.

**Acceptance Scenarios**:

1. **Given** no state release exists, **When** `ci` runs a pipeline with `[state]`, **Then** the state file is absent at step start, the pipeline passes, and an encrypted asset exists afterwards.
2. **Given** a state release with one asset, **When** `ci` runs, **Then** the decrypted file is at the declared path before the first step starts.
3. **Given** the pipeline fails, **When** the run ends, **Then** the stored state is byte for byte the one before the run.

---

### User Story 2 - Runs of one state never overlap and never get lost (Priority: P1)

A scheduled run and a manual run start together, for the same state. Both must run, one after the other, in order.

**Why this priority**: Two runs that share a state would restore the same cookies and overwrite each other's save, or log in twice from two places.

**Independent Test**: Mock store. Start three `ci` processes for one state. Assert the pipeline windows do not overlap and the order equals the ticket order. Start a fourth: it ends with `queue is full`.

**Acceptance Scenarios**:

1. **Given** run A holds the turn, **When** run B and C register, **Then** B starts after A ends and C after B ends, and each prints `[runner] waiting for the queue` once.
2. **Given** A's runner is killed, **When** 10 minutes pass without renewal, **Then** B removes A's ticket and starts.
3. **Given** A holds the turn for longer than a waiter can wait, **When** the waiter reaches 120 minutes, **Then** it removes its own ticket, prints `[runner] queue wait timed out`, exits 1 and starts no step.

---

### User Story 3 - The schedule starts the pipelines (Priority: P1)

The owner adds an entry with an interval of 60 minutes. The runner starts the entry about once an hour with no dispatch.

**Why this priority**: The owner wants the browser test to run unattended.

**Independent Test**: The `due` check with an injected clock and a fixture schedule file. Assert which entries are due at which minute, that an entry for a repo outside the watch list produces a `ci` that exits 1 with the watch-list line, and that a bad file is refused whole.

**Acceptance Scenarios**:

1. **Given** an entry with `interval_minutes = 60`, **When** the 15-minute tick falls in the first quarter hour of an hour, **Then** that entry is due and the others are not.
2. **Given** an entry whose repo or branch the watch list does not allow, **When** it is due, **Then** `ci` exits 1 with `[runner] repository or ref not in the watch list`.

---

### User Story 4 - A failed run leaves evidence an agent can read (Priority: P1)

The Playwright step fails. The runner bundles `test-results/` and `playwright-report/`, encrypts the bundle and uploads it. Loki says which release and asset. The agent downloads it with `gh release download`, decrypts it with the documented command and reads the trace.

**Why this priority**: Without it the agent debugs from a pass/fail line.

**Independent Test**: A failing fixture step that writes a file under a declared path. Assert one encrypted asset exists, the Loki body names it, stdout and stderr hold only grammar lines, and decrypting with the artifact key (and not the state key) returns the file.

**Acceptance Scenarios**:

1. **Given** `when = "failure"` and a failing step, **When** the run ends, **Then** one asset is uploaded and Loki holds the release and asset names.
2. **Given** a passing run with `when = "failure"`, **When** the run ends, **Then** no asset is uploaded.
3. **Given** 10 bundles exist and an 11th is uploaded, **When** the upload completes, **Then** the oldest is deleted.

---

### User Story 5 - Nothing private leaks and steps cannot reach the new settings (Priority: P1)

**Why this priority**: Constitution Principle III, and the new token can rewrite a private repo's releases.

**Independent Test**: A step that lists each new setting, a watch entry that lists each, and canary values for all of them seeded in the mock store. Search stdout, stderr, every child argv and environment, and every Loki body.

**Acceptance Scenarios**:

1. **Given** a pipeline lists a new setting in `secrets`, **When** `ci` reads it, **Then** the run ends with `[runner] pipeline file refused` and starts no container.
2. **Given** a run that restores, saves and uploads, **When** it ends, **Then** no new setting value, nor its base64, appears anywhere public, in any step environment, or in any Loki body.

---

### User Story 6 - A step gets a website login and never a vault credential (Priority: P1)

The Playwright step declares the vault item `playwright-login` and its fields `username` and `password`, mapped to two environment variables. The watch entry lists the item. The runner unlocks the vault, reads the two fields, locks the vault, deletes the `bw` session and data directory, and only then starts the step.

**Why this priority**: The owner's rule: no AI agent ever sees a Bitwarden credential, and the script gets only what it needs. The step is code from a branch, and an agent may write it.

**Independent Test**: A fake `bw` that records its argv and environment and a fixture step that dumps its own environment and the file tree it can see. Assert the step holds exactly the declared variables, no `BW_*` name, no session key, no data directory, and the fake `bw` was asked for the one declared item only.

**Acceptance Scenarios**:

1. **Given** the entry allows the item and the vault holds it once, **When** `ci` runs, **Then** the step starts with the declared variables set, and the vault session and data directory are gone before it starts.
2. **Given** a step that declares no vault field, **When** it starts, **Then** its environment holds no vault value, even when a sibling step declared one.
3. **Given** a pipeline names an item that the entry does not allow, **When** `ci` reads the pipeline, **Then** the run ends with `[runner] vault item not allowed`, the vault is never unlocked and no container starts.

---

### User Story 7 - An artifact never carries a secret to an agent (Priority: P1)

A failed trace holds the typed password in a network log inside `trace.zip`. The runner finds it before encryption and uploads nothing.

**Why this priority**: The artifact key goes to an agent. Anything inside the bundle, the agent can read.

**Independent Test**: A fixture that writes a zip whose inner entry holds a canary in plain, in base64 and as one line of a multi-line value. Run each. No upload happens, the run fails, and clean content uploads. Also a bundle that lists the state file path is refused at the pipeline check.

**Acceptance Scenarios**:

1. **Given** a declared artifact holds a known secret value in a zip entry, **When** the run ends, **Then** nothing is uploaded, stderr shows `[runner] artifact upload refused` and the exit code is 1.
2. **Given** an artifact with no known value in it, **When** the run ends, **Then** it is encrypted with the artifact key and uploaded, and the state key cannot open it.
3. **Given** an artifact path that is, contains or lies under the state path, **When** the pipeline is read, **Then** the run ends with `[runner] pipeline file refused`.

### Edge Cases

Answers to the edge-case checklist. "Handled" names the requirement.

- Inputs. State or artifact path absolute, with `..`, through a symlink, or outside the project: pipeline refused (FR-018). Two `[state]` tables: TOML error, refused. State name with `/`, uppercase, Unicode, NUL, over 64 characters: refused (FR-018). State file empty, over 10 MB, or missing after a pass: no save, `state save failed` for the oversized one, a Loki line for the missing one (FR-024). Artifact path that matches nothing: skipped, a Loki line. Bundle over 100 MB: not uploaded, a Loki line (FR-032). Schedule entry with a repeated key, an unknown key, an interval that is not a multiple of 15 or is outside 15 to 10080, a duplicate entry, or more than 10 entries: the whole file is refused (FR-004).
- State. First run with no state starts empty (FR-021). A crash between upload and delete leaves two assets, and the newest wins (FR-025). A truncated or tampered asset, or the wrong key: restore fails and the run stops, with state untouched (FR-022). Older asset format: no older version exists. A retry is safe: ticket and asset names carry the run id, so a rerun of the same run id replaces its own ticket and saves one new asset.
- Partial failure. Restore fails: no step starts, the ticket is released. Pipeline fails: state kept, artifacts uploaded, ticket released. Save fails after a pass: the run fails, the ticket is released, the old state stays (FR-026). Artifact upload fails: Loki line, exit code unchanged (FR-031). Release of the ticket fails: the stale rule frees it within 10 minutes (FR-014).
- Time and concurrency. Two runs register together: the store decides the order, and the order is stable (FR-010). Every GitHub call has a time limit and one retry (FR-036). Clock skew between runners: ticket age comes from the GitHub server time, never the runner clock (FR-013). Scheduled tick delayed or skipped: no catch-up, so a run can be missed or doubled (R5). An interval boundary crossing a month or daylight saving change: intervals count UTC minutes since the epoch, so neither matters.
- Resources. Waiters poll every 20 s, so a full queue costs about 12 calls a minute against the token's 5,000 an hour. Releases hold at most 2 state assets and 10 artifacts per project, and a queue holds at most 3 tickets. A waiting job holds one hosted VM for up to 120 minutes. The runner reads state and bundle files in streams, not whole into memory (plan checks this).
- Trust. Who can name a state: the pipeline author, inside the repo's own releases, only when the reviewed watch entry says `state = true` and the repo is private. Who can reach the token: the runner process only. What the pipeline author cannot do: write another repo's release, read a setting or a vault credential, widen an entry, choose a release tag outside the fixed naming (FR-002, FR-019, FR-034, FR-042). What the agent cannot do: open state, because it holds neither the state key nor the token (FR-027, FR-048).
- Vault and scan inputs. A vault item that matches zero or more than one login in the allowed collection, a missing field, an empty field, a value with NUL or with a trimmed line under 4 characters, or a locked or revoked identity: `vault unavailable`, no container (FR-045). Two steps declaring the same environment variable for different fields, or one that also appears in `env` or `secrets`: pipeline refused (FR-042). An item name with Unicode, spaces or `=`: allowed, matched by exact bytes, and passed only through the environment, never argv. A zip with an encrypted entry, nesting deeper than 2, an entry count over 10,000 or more than 500 MB uncompressed: not scannable, so no upload (FR-049). A value split across two lines or transformed beyond base64 is not found (R2, R11).
- Vault resources and failure. The `bw` call has a time limit and one retry (FR-036). A `bw` that hangs or crashes leaves its data directory, which the runner removes anyway (FR-046). A vault that lock or logout fails on ends the run (FR-046).
- UI. No UI. The surface is the fixed stdout and stderr lines (FR-037).

## Requirements *(mandatory)*

Each requirement is an acceptance criterion in the form WHEN trigger THE SYSTEM SHALL response. Tags: `[error]` is a failure path, `[boundary]` is an edge of an allowed range. Each maps to at least one unit test under `cargo test --lib` (Principle V); workflow and file-format checks run in `factory-ci-selftest`.

### Functional Requirements

**Schedule**

- **FR-001**: WHEN the 15-minute cron of `factory-monorepo-ci.yml` fires on `main` THE SYSTEM SHALL read `.github/factory-ci-schedule.toml` from that same commit, work out which entries are due, and start one `ci` run per due entry through the same `ci` command and watch-list checks as a manual run. The public job name and log carry the entry's index only, never its repo, branch or project.
- **FR-002**: WHEN an entry is due THE SYSTEM SHALL run it only if the watch list has an entry for its repo with a numeric `id` whose `branches` match the entry's branch. A schedule entry grants nothing the watch list does not.
- **FR-003**: WHEN the minute count since the epoch (UTC) modulo an entry's `interval_minutes` is below 15 THE SYSTEM SHALL treat the entry as due at that tick. `[boundary]` A modulo of 14 is due, 15 is not. An interval of 15 is due at every tick.
- **FR-004** `[error]`: WHEN the schedule file has an unknown or repeated key, a missing `repo`, `branch` or `interval_minutes`, an interval that is not a multiple of 15 or lies outside 15 to 10080, a duplicate (repo, branch, project) entry, or more than 10 entries THE SYSTEM SHALL refuse the whole file, report every problem in `--check` mode with no token or network, and start nothing at a tick. `[boundary]` 10 entries are accepted, 11 are not. An empty file, or one with no entries, is valid and starts nothing.
- **FR-005**: WHEN a run starts by manual dispatch THE SYSTEM SHALL behave as before, with the same inputs. Dispatch inputs never select a schedule entry.

**Queue**

- **FR-006**: WHEN a pipeline declares a `[state]` and passes the file, docker and secret checks THE SYSTEM SHALL register one ticket in the queue of that state in the repo that owns the pipeline, before restoring state or starting any container. A pipeline with no `[state]` registers nothing.
- **FR-007**: WHEN a ticket is first in line THE SYSTEM SHALL let its run restore, run and save. WHEN it is not THE SYSTEM SHALL start no container and touch no state, poll every 20 s, and print `[runner] waiting for the queue` once to stdout.
- **FR-008**: WHEN several tickets exist THE SYSTEM SHALL run them in registration order, one at a time. The turn passes only after the previous run released its ticket or its ticket went stale. No ticket is skipped, reordered or dropped except by FR-011, FR-012 and FR-014.
- **FR-009**: WHEN a run ends, whatever the result (pass, fail, refusal after registering, panic, SIGTERM, SIGINT) THE SYSTEM SHALL remove its ticket before exit. A cancelled GitHub job sends SIGTERM and the runner gets its grace period to do so.
- **FR-010**: WHEN two runs register at the same moment THE SYSTEM SHALL give them one stable order decided by the store, the same for both.
- **FR-011** `[error]`: WHEN a state's queue already holds 3 tickets THE SYSTEM SHALL end the run at once with exit code 1 and `[runner] queue is full`, register no ticket and start no container. `[boundary]` A third ticket is accepted, a fourth is refused.
- **FR-012** `[error]`: WHEN a run has waited 120 minutes without its turn THE SYSTEM SHALL remove its ticket, print `[runner] queue wait timed out`, exit 1 and start no container. `[boundary]` At 119 minutes it keeps waiting. The wait does not count against the 30-minute pipeline deadline.
- **FR-013**: WHEN a run keeps a ticket THE SYSTEM SHALL renew it every 60 s, waiting or running, using the store's server time to judge age, never the runner clock.
- **FR-014** `[error]`: WHEN a ticket has gone 10 minutes without renewal THE SYSTEM SHALL treat it as stale, let the next waiter remove it and log the removal to Loki as step `ci`. `[boundary]` 9 minutes 59 seconds is not stale, 10 minutes is. A holder whose renewals fail does not stop its pipeline.
- **FR-015** `[error]`: WHEN a run finds its own ticket gone before it saves state THE SYSTEM SHALL write nothing, end the run with exit code 1 and `[runner] state save failed`, and log the cause to Loki.
- **FR-016**: WHEN waiting runs show anything publicly THE SYSTEM SHALL show only the line of FR-007. Positions, ticket names, ages, state names and repo names go to Loki as step `ci` only.

**Declaring state and artifacts**

- **FR-017**: THE SYSTEM SHALL accept two optional tables in `.factory-ci.toml`: `[state]` with `name` and `path`, and `[artifacts]` with `paths` (a list) and `when` (`"failure"` or `"always"`, default `"failure"`). Unknown keys stay errors.
- **FR-018** `[error]`: WHEN `name` is not 1 to 64 characters of `[A-Za-z0-9_-]`, or `path` or an artifact path is empty, absolute, contains `..`, a NUL or a newline, or resolves through a symlink or outside the project folder, or a list holds more than 20 paths, or an artifact path equals, contains or lies under the state path THE SYSTEM SHALL end the run with `[runner] pipeline file refused` and start no container. `[boundary]` A 64-character name and 20 paths are accepted, 65 and 21 are not. A sibling path that only shares a prefix with the state path (`.ci-state` and `.ci-state-2`) is accepted.
- **FR-019** `[error]`: WHEN a pipeline declares `[state]` or `[artifacts]` and the repo's watch entry lacks `state = true`, or the repo is public (judged from the identity check the runner already makes) THE SYSTEM SHALL end the run with `[runner] state not allowed`, register no ticket and start no container. The release a run writes to is fixed by the repo's own name and the state name or project, never taken from the file. `plan --check` accepts `state` only with a numeric `id`.

**State**

- **FR-020** `[error]`: WHEN a pipeline declares `[state]` and `FACTORY_CI_STATE_TOKEN` or `FACTORY_CI_STATE_KEY` is missing from Bitwarden (zero matches), or declares `[artifacts]` and `FACTORY_CI_STATE_TOKEN` or `FACTORY_CI_ARTIFACT_KEY` is missing, THE SYSTEM SHALL end the run with `[runner] state store not configured` before registering a ticket. Two matches, an empty value, a value with CR, LF or NUL, leading or trailing whitespace, or under 4 characters, are refused as for the four existing settings. A key (`FACTORY_CI_STATE_KEY` or `FACTORY_CI_ARTIFACT_KEY`) must be 32 characters or more, and the two must differ: equal values end the run the same way. `[boundary]` A key of 32 characters is accepted and 31 are refused.
- **FR-021**: WHEN the runner is the first in line and the state release has no asset THE SYSTEM SHALL start the pipeline with no state file at the declared path. This is the only case that starts empty.
- **FR-022** `[error]`: WHEN the newest state asset cannot be downloaded, authenticated or decrypted (wrong key, truncated, tampered, unreadable) THE SYSTEM SHALL end the run with `[runner] state restore failed`, start no container, change nothing in the store and release its ticket. It never falls back to an older asset or an empty state.
- **FR-023**: WHEN restore succeeds THE SYSTEM SHALL write the decrypted file at the declared path inside the checkout, readable by the step, before the first step starts. Only the declared file is restored.
- **FR-024**: WHEN the pipeline passed THE SYSTEM SHALL read the declared file from the checkout and save it, encrypted with a fresh random nonce, as a new asset, before it removes the checkout. WHEN the file is missing THE SYSTEM SHALL save nothing and log a line to Loki. `[boundary]` A file of exactly 10 MB is saved. One of 10 MB plus 1 byte is refused (`[runner] state save failed`). A symlink at the path is refused.
- **FR-025**: WHEN a save uploads a new asset THE SYSTEM SHALL delete older assets only after the new one is stored and its size matches, and keep the 2 newest. WHEN restore sees several assets THE SYSTEM SHALL pick the newest by the store's order, not by name alone.
- **FR-026** `[error]`: WHEN the pipeline failed, was skipped in part, or hit a runner fault THE SYSTEM SHALL save no state. WHEN the pipeline passed and the upload or the size check fails THE SYSTEM SHALL end the run with `[runner] state save failed`, keep the previous state and release its ticket.
- **FR-027** `[error]`: THE SYSTEM SHALL encrypt every state file with an authenticated cipher under `FACTORY_CI_STATE_KEY` only, and every artifact bundle under `FACTORY_CI_ARTIFACT_KEY` only, so a changed byte fails decryption and one key never opens the other's assets. WHEN an asset is opened with the wrong one of the two keys THE SYSTEM SHALL fail authentication and return no plaintext. The state file is a credential: its content never enters an artifact (FR-018, FR-049, FR-050). No plaintext of either leaves the runner process or the checkout.

**Artifacts**

- **FR-028**: WHEN the pipeline did not pass and at least one step started, or `when = "always"`, THE SYSTEM SHALL bundle the declared paths that exist, scan the bundle (FR-049), encrypt it under the artifact key and upload it as one asset to the artifact release of the run's project in the repo that owns the pipeline. It runs after the pipeline, before the checkout is removed, and whether or not a step failed. It needs no ticket.
- **FR-029**: WHEN an upload completes THE SYSTEM SHALL log the release tag and the asset name to Loki as step `ci`, with the run id, and print nothing about them publicly.
- **FR-030**: WHEN an artifact upload completes THE SYSTEM SHALL keep the 10 newest bundles of that project's release and delete the rest. `[boundary]` With 10 bundles nothing is deleted. The 11th upload deletes the oldest.
- **FR-031** `[error]`: WHEN an artifact bundle or its upload fails THE SYSTEM SHALL log the cause to Loki as step `ci` and leave the run's exit code as the pipeline decided. It never turns a failure into a pass or a pass into a failure. The one exception is a refusal by the secret scan (FR-049), which ends the run with exit code 1.
- **FR-032** `[boundary]`: WHEN a bundle would exceed 100 MB THE SYSTEM SHALL upload nothing and log the size to Loki. 100 MB exactly is uploaded.
- **FR-033**: THE SYSTEM SHALL document one command line that downloads an artifact asset with `gh release download` and decrypts it, in `rust-ci-runner/README.md`. The key comes from the environment or standard input, never argv, and the doc names it by its Bitwarden name only. The same section states what an agent may hold: the artifact key, a Loki read token, and a GitHub token with contents read on the one private repo. It names what an agent must never hold (FR-048).

**Vault logins**

- **FR-042** `[error]`: WHEN a step declares a vault login THE SYSTEM SHALL accept only entries of the form item name, field name and environment variable name, and SHALL end the run with `[runner] pipeline file refused` before any vault call when the environment variable name breaks the 010 name rules, starts with `FACTORY_CI`, `BW_` or `BITWARDENCLI`, repeats in the step (across `env`, `secrets` and vault entries), or the step has `docker = true`. `[boundary]` A step may declare 20 fields and the pipeline 50; 21 and 51 are refused.
- **FR-043** `[error]`: WHEN any step names a vault item that is not listed in `vault_items` of the run's watch entry (including when the entry has no such key) THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] vault item not allowed`, unlock no vault, start no container and read no value. Items match by exact name. `[boundary]` An item that differs only by case is not allowed. `plan --check` reports a `vault_items` entry that is empty, longer than 100 characters, or holds a control character, and refuses the file whole.
- **FR-044**: WHEN every declared item is allowed THE SYSTEM SHALL read all declared fields once, before the first container starts, and pass each value only to steps that declared it, through the environment of the engine child process, never argv. WHEN several steps declare the same item and field THE SYSTEM SHALL read it once.
- **FR-045** `[error]`: WHEN a declared item matches no login or more than one in what the identity can see, a declared field is missing, empty or holds a NUL, a value has a trimmed line under 4 characters, or the vault cannot be unlocked THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] vault unavailable`, and start no container. `[boundary]` A trimmed line of exactly 4 characters is accepted, 3 is refused. The vault settings missing from Bitwarden (`FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`) end the run the same way.
- **FR-046** `[error]`: WHEN the vault reads end, whether they succeeded or failed THE SYSTEM SHALL lock or log out of the vault and remove the `bw` session and data directory before any step starts, and SHALL give no `bw` child or step a session key, an unlock credential or the data directory path. WHEN lock, logout or removal fails THE SYSTEM SHALL end the run with `[runner] vault unavailable` and start no container.
- **FR-047**: THE SYSTEM SHALL add every vault value to the Redactor before any line that could hold it is written, with its base64 forms and each line of 4 or more characters, as for step secrets. The runner's own logs, stdout, stderr and Loki detail lines for the vault carry positions and counts only (for example `declared item 2 of 3`), never an item name, a field name or a value.
- **FR-048**: THE SYSTEM SHALL keep these out of reach of any agent and any step: the vault credentials, `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY` and the Bitwarden access token. An agent's whole kit is `FACTORY_CI_ARTIFACT_KEY`, a Loki read token and a read-only GitHub token; no artifact, Loki line or public line may carry anything else.

**Artifact scan**

- **FR-049** `[error]`: WHEN a bundle is ready and before it is encrypted THE SYSTEM SHALL scan every file in it for each value the Redactor holds (the runner settings, step secrets, vault values, the Bitwarden access token), each in its exact form, in the base64 forms the Redactor knows, and as each line of 4 or more characters of a multi-line value, and SHALL scan the contents of every zip entry, to a nesting depth of 2. WHEN a value matches, or a file cannot be scanned (an encrypted entry, depth over 2, more than 10,000 entries, more than 500 MB uncompressed, an unreadable file) THE SYSTEM SHALL upload nothing, print exactly `[runner] artifact upload refused`, end the run with exit code 1 and log to Loki only a position and a count, never the value, the matching file name or an offset. `[boundary]` A value of exactly 4 characters on one line is scanned for; a nested zip at depth 2 is scanned and one at depth 3 refuses the upload.
- **FR-050**: WHEN the state file is JSON THE SYSTEM SHALL add each string it holds under a key named `value` of 16 or more characters (the cookie and local storage values of a Playwright storageState) to the Redactor and to the scan set, and WHEN it is not JSON THE SYSTEM SHALL add its whole content when that is under 1 MB. `[boundary]` A 16-character cookie value is added, a 15-character one is not.

**Settings and containment**

- **FR-034** `[error]`: WHEN a watch entry lists any of `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` or `FACTORY_CI_VAULT_PASSWORD` in `secrets` THE SYSTEM SHALL refuse the whole watch file. WHEN a pipeline lists any of them in `secrets`, or as a vault environment variable name, THE SYSTEM SHALL end the run with `[runner] pipeline file refused`. None of the six values reaches a step, a step container argument, an engine child other than the one call that needs it, or a log body. The test list of 010 FR-004 grows from four names to ten, and 011 supersedes that list.
- **FR-035**: THE SYSTEM SHALL mask all six new values, their base64 forms and the base64 of `x-access-token:<token>` in every output path, as it does for the four settings, before any line that could hold them is written.
- **FR-036**: WHEN the runner calls `bw` THE SYSTEM SHALL give the call a time limit (60 s) and one retry, never after a rejected credential. WHEN the runner calls the GitHub API for the queue, state or artifacts THE SYSTEM SHALL use `FACTORY_CI_STATE_TOKEN` with a time limit and one retry on no answer, 429 or 5xx, and never use `FACTORY_CI_TOKEN` for a write or `FACTORY_CI_STATE_TOKEN` for a clone. The token never appears in argv, a file or a git config.

**Public output**

- **FR-037**: WHEN the run prints THE SYSTEM SHALL keep stdout to the closed grammar of 008, extended by exactly one line, `[runner] waiting for the queue`, and stderr to the closed fault list, extended by exactly these lines: `queue is full`, `queue wait timed out`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`. No repo, branch, project, state, release, asset, vault item, vault field or secret name, and no count, position or age, appears in any public line. The lines join the closed list in `log.rs` and its test.

**Rollout**

- **FR-038**: THE SYSTEM SHALL release the runner as 0.4.0 (`Cargo.toml` and `Cargo.lock` agree, tag `rust-ci-runner-v0.4.0` on a commit on `main`), then change `RUNNER_IMAGE` in a separate reviewed commit to the new digest, as the README's bootstrap order says. The release smoke step still passes.
- **FR-039**: THE SYSTEM SHALL change `factory-monorepo-ci.yml` so that it adds the 15-minute `schedule` trigger and a job that lists due entries, removes the `concurrency` block, raises `timeout-minutes` of the run job to 190, keeps `if: github.ref == 'refs/heads/main'`, holds no new GitHub secret, and keeps the single `docker run` for the run job. The header comment explains why the concurrency group is gone.
- **FR-040**: THE SYSTEM SHALL add `state = true` and the `vault_items` of the Playwright login to the `sudtanj/factory-monorepo` entry only, add the first schedule entry for the Playwright project when its pipeline lands, and update the header comments of `.github/factory-ci-repos.toml`. The `sudtanj/infrastructure` entry gets neither.
- **FR-041**: THE SYSTEM SHALL update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description to match: the six new settings and the Bitwarden project guidance ("holds only the four keys"), the recommendation of a Bitwarden identity limited to one collection, the `ci` step list, the fault list, the decrypt command and the agent kit, the vault declaration, the schedule file, the queue numbers, the removed concurrency group, and the risks. This is the last task in `tasks.md`.

### Key Entities

- **Schedule entry**: `repo`, `branch`, `project` (default `.`), `interval_minutes`, in `.github/factory-ci-schedule.toml`. Reviewed on `main` of B.
- **Watch entry**: gains `state` (default false) and `vault_items` (default none). Reviewed on `main` of B.
- **Vault login**: a step's declaration of one item, one field and the environment variable that receives it. The values live in a Bitwarden Password Manager vault and reach only that step.
- **State**: one named file of a pipeline, stored encrypted in a release of the owning private repo, newest 2 kept.
- **Ticket**: a queue place of one run for one state, with a registration order and a renewal time. At most 3 per state.
- **Artifact bundle**: one encrypted archive of a run's declared paths, in the project's artifact release, newest 10 kept.
- **Runner settings**: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`, now also `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD`. Read by the runner, never handed to a step or an agent. The last three are the vault unlock credentials, which never leave the one `bw` child.
- **Agent kit**: the artifact key, a Loki read token and a read-only GitHub token. Nothing else.
- **Public line**: a stdout or stderr line from the closed grammar, now with one more stdout line and nine more fault lines.

## Success Criteria *(mandatory)*

### Hypothesis

If the runner keeps the browser session between runs and serializes the runs that share it, the Playwright project can run unattended every hour without logging in each time and without two runs fighting over one session, and an agent can diagnose a failure without the owner copying anything. We would be wrong if sessions expire faster than the interval (every run logs in anyway), if the queue backs up so waiters time out, or if agents still ask the owner for more than the artifact.

### Measurable Outcomes

- **SC-001**: Over the first 7 days of a 60-minute schedule, at least 90% of runs after the first start with a restored state, and zero pairs of runs of one state overlap. Measured from Loki `ci` lines (restore result, run start and end times) per state.
- **SC-002**: Zero scheduled or manual runs for one state are lost to anything other than FR-011, FR-012 and a missed tick. Measured by comparing requested runs (Actions run list) with ticket lines in Loki. Any `queue wait timed out` or `queue is full` in the week is read and either explained or the numbers in FR-011 and FR-012 change.
- **SC-003**: For every failed run that declared artifacts, an agent finds the release and asset in Loki and opens the bundle with the documented command in under 5 minutes, on its first try. Measured by one trial by the owner after release.
- **SC-004**: Across a hostile fixture (canary state, token and key, artifact with a canary inside, step listing the settings), zero lines of stdout, stderr, child argv and Loki bodies hold a canary, no release asset holds a plaintext canary, and zero public lines hold a repo, state, release or asset name. Measured by the unit and fixture tests.
- **SC-005**: A failed run never changes the stored state: a test that fails the pipeline after a write to the state file finds the newest asset byte-identical to before.
- **SC-006**: `cargo test --lib` passes in the `rust-ci-runner` pipeline with tests for FR-001 to FR-050, and the release publishes a 0.4.0 digest that the workflow pin matches.
- **SC-007**: In a fixture run with a fake `bw`, the step's environment and visible files hold the declared variables and zero vault credentials, session keys or `bw` data. Zero runner log lines, public or Loki, hold a vault item name, field name or value. Measured by the fixture test and a search of every output.
- **SC-008**: Across a hostile artifact fixture (each known value in plain, base64, one line of a multi-line value, inside a zip and inside a nested zip), 100% of bundles are refused with no upload, and a clean bundle uploads. The state key opens no artifact and the artifact key opens no state asset. Measured by the unit tests.
- **SC-009**: After one debugging trial, the agent's kit holds the artifact key, a Loki read token and a read-only GitHub token, and the owner confirms it never held any other credential. Measured by the owner's review of what the agent was given.

## Assumptions

- The owner creates the fine-grained token (contents write, only the private repos with `state = true`), the two keys (each `openssl rand -base64 33`, 32 characters or more, different from each other), and the Bitwarden Secrets Manager entries for all of them and for the three vault settings. No code or doc holds a value. Anyone with write access to the Bitwarden project can read them all: that is already a trust root (008).
- The vault credentials unlock a Bitwarden Password Manager account. A Bitwarden API key belongs to a whole account, so limiting it to one collection takes a separate account that is a member of an organization with access to that collection only. The owner creates it; the spec recommends it (R12) and cannot enforce it.
- The `bw` command line client is not in the runner image today. The plan adds it pinned by SHA-256 like `bws`, or picks another way to read a vault, and justifies the size and the new dependency in Complexity Tracking. The `bw` call is a child process with a cleared environment holding only what it needs.
- The agent holds its own GitHub credentials for `gh`. The owner gives it a token with contents read on the one private repo. That token can download the encrypted state asset, which is useless without the state key.
- GitHub's store gives an atomic create that fails when the name exists (a ref or a release tag), server timestamps and release asset upload and delete. The plan picks the mechanism for tickets and renewals; this spec fixes only the behaviour. A queue operation that cannot be made atomic on GitHub is a blocker for the plan, not a reason to loosen FR-008.
- The runner has no encryption dependency today (`Cargo.toml` lists `getrandom`, `serde`, `serde_json`, `toml`, `ureq`). The plan chooses an authenticated cipher that the owner or an agent can open with a documented command, and justifies any new crate or image tool in Complexity Tracking.
- The state file is small (cookies, tens of KB). A directory as state, and a file over 10 MB, are out of scope.
- The step container runs as root and writes files the runner user can read. A file it writes with mode 0600 as root fails the save with `state save failed`; the pipeline author fixes the file mode.
- Scheduled workflows run on the default branch and GitHub may delay or skip them. The 15-minute window of FR-003 makes a missed or doubled tick possible. A doubled run queues, and runs a second time.
- GitHub disables scheduled workflows in a public repo after 60 days with no repository activity. The owner re-enables it by hand (R6).
- `sudtanj/infrastructure` and any public repo get no `state = true`.
- Existing entries and pipelines with no `[state]` and no `[artifacts]` behave as before, except that the concurrency group no longer serializes them (R4).

## Non-goals

- Queues for stateless pipelines, priorities, cancelling a queued run, and fair scheduling across states.
- Exactly-once schedules, catch-up of missed ticks, jitter, cron expressions, and per-entry timezones.
- State as a directory, state shared between repos, state versions beyond the newest 2, rollback to an older state, and any merge of two states.
- Key rotation and re-encryption of old assets.
- Rewriting or masking text inside a bundle. The scan only refuses (FR-049).
- Scanning values the runner does not know: a secret that never passed through Bitwarden, a transformed value, or one split across lines (R2, R11).
- The vault TOTP code, attachments, secure notes beyond named fields, writing to the vault, more than one vault identity, and any per-field allowlist.
- Managing the agent's credentials, or checking what an agent was given. The spec fixes the kit and documents it (FR-033, FR-048).
- Streaming artifacts to Loki, viewing them in a browser, and any download tooling beyond one documented `gh` command.
- Docker steps with state, `--allow-docker`, and any change to the tick workflow and its scripts.
- A cap on step output, and any change to how the four existing settings are read beyond adding the two.

## Risks

- **R1, a write token on the runner**: `FACTORY_CI_STATE_TOKEN` can rewrite releases of every repo it covers. A pipeline cannot read it (FR-034), but a bug in the runner could leak it. Mitigation: fine-grained, only private repos with `state = true`, 90-day expiry like the others, masked, and never in a child environment.
- **R2, the queue is a lock on a public API**: a ticket that cannot be removed (token expired, API down) blocks the state until staleness frees it, and a stale break of a run that is alive but cut off from GitHub lets two runs overlap. Mitigation: 10-minute stale rule, the fence of FR-015 so the loser never writes state, and Loki lines for every break.
- **R3, an artifact holds secrets**: a Playwright trace records typed input and network traffic, so it can hold the login and cookies. Encryption protects the release from readers of the repo, not from whoever holds the artifact key, and the agent holds it. Mitigation: the scan of FR-049 refuses a bundle that holds any value the runner knows, the artifact key differs from the state key so the agent cannot open state, and the state never enters a bundle (FR-018, FR-050). The scan cannot see a value it does not know (R11).
- **R4, concurrency group removed**: two stateless runs of one project can overlap, and no cancel of pending runs happens any more. Mitigation: documented in the README. The Playwright project declares state, so it queues.
- **R5, schedule jitter**: GitHub delays and skips scheduled runs, so a run can be missed or doubled. Mitigation: none beyond the queue. SC-001 measures it.
- **R6, schedule switched off**: a public repo with no activity for 60 days stops scheduled runs without a message. Mitigation: owner checks the Actions tab. A dead-man ping is not part of this spec.
- **R7, long waits cost a hosted VM**: a waiting job holds a runner for up to 120 minutes. The repo is public, so minutes are free, but the concurrent-job limit applies. Mitigation: queue cap of 3.
- **R8, request order is approximate**: registration happens after the fetch, so near-simultaneous starts can swap (Clarifications). Mitigation: accepted. Strict order holds from registration on.
- **R9, rollout order**: a pin bump before the digest fails the pull. A `[state]` pipeline before `state = true`, the token and the key exist fails with a fixed line. FR-038 and FR-040 set the order: release, pin, watch entry and Bitwarden keys, then the pipeline.
- **R10, one state per name per repo**: two pipelines of one repo that pick one state name share one queue and one file. Mitigation: pick distinct names.
- **R11, the scan is exact**: it finds known values in plain, in the base64 forms the Redactor knows and by line. A value a page renders differently (URL-encoded, split in a screenshot as pixels) passes. A screenshot of a page that shows a secret is an image, and no scan reads it. Mitigation: keep `when = "failure"`, declare only the paths that are needed, and treat artifacts as sensitive. A false positive (a 4-character value that matches ordinary text) blocks an upload and fails the run; the 4-character floor is the same as for masking.
- **R12, the vault credentials are a master key**: `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD` unlock the account. Anyone who can read them from Bitwarden Secrets Manager, or reach the runner process memory, can read every item that account sees. The runner holds them only in memory and in one `bw` child, but the child's environment is readable by anything running as the same user in the container, as with the existing access token. Mitigation: a dedicated Bitwarden account in an organization that can see only the collection holding the allowlisted items, not the owner's personal vault; the `vault_items` allowlist; no step ever sees the credentials.
- **R13, allowlist is by item, not field**: a pipeline that may read an item may read every field of it. Mitigation: keep one item per purpose and put only the fields it needs in it.
- **R14, a vault read takes the turn**: reads happen inside the queue turn, so a slow vault holds the next run. Mitigation: 60-second limit, one retry, and the 5-minute share of the 45-minute hold.
