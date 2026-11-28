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

Pipelines run by `factory-monorepo-ci` can now run on a schedule, remember their state and leave evidence when they fail. Add a `cron` line and its `case` branch to `factory-monorepo-ci.yml` (a repo, a branch, a project, an entry number from 1 to 3), and GitHub starts it for you. Declare a `[state]` file in the pipeline and mark the one step that uses it with `state = true`: the runner restores the last good copy just before that step and saves the new one after a passing run. Runs that share a state wait in a strict queue and run one after another in the order they asked, with a fixed `waiting for the queue` line in the public log. When a run fails, the runner scans the traces and screenshots the pipeline declared for every secret it knows, encrypts them and uploads them to a release on a private store repo. Loki names the release and asset. An agent reads Loki, opens the asset with a pinned script, and reads it. State and artifacts are encrypted with two different keys from Bitwarden, and the runner holds the write token itself: steps can never request any of them. A step that needs a website login declares the vault item and fields (runner 0.4.1; until then a login comes from a step secret). Runner 0.4.0 carries the queue, state, artifacts and the schedule.

## Clarifications

### Session 2026-11-28 (owner decisions)

- Q: Do runs start on a schedule? → A: Yes, besides manual dispatch. Schedule entries (repo, branch, project, interval) live in reviewed config in this repo, never in dispatch inputs. Scheduled runs go through the same watch-list checks as manual ones.
- Q: Is GitHub's concurrency group enough to serialize runs of a state? → A: No, it cancels older pending runs. The queue is strict FIFO: runs that share a state never overlap, and each requested run runs, in request order, one after another. The spec fixes the stale-lock rule, the maximum wait and the public line.
- Q: Where does state live? → A: In a release tag of a private store repo that holds no code (named by `FACTORY_CI_STATE_REPO`; see the decisions session below). The runner restores it before the pipeline and saves it after.
- Q: Is state encrypted? → A: Yes. Anyone with read access to the private repo can download release assets, so the runner encrypts state before upload with a key from Bitwarden Secrets Manager. A failed run never overwrites the last good state. The first run, with no state, starts empty.
- Q: Are debug artifacts kept? → A: Yes, encrypted the same way, uploaded to a release on the private repo on failure (and on every run when the pipeline asks). Loki gets the release and asset name, the public log gets nothing. An agent reads Loki with a read token and downloads and decrypts with `gh`. Retention is bounded so the release does not grow forever.
- Q: Which tokens? → A: Two new runner settings read from Bitwarden: `FACTORY_CI_STATE_TOKEN` (fine-grained, contents write, scoped to the store repo only) and `FACTORY_CI_STATE_KEY` (the encryption key). Like the four existing settings, steps may never request them. `FACTORY_CI_TOKEN` stays read-only.

### Session 2026-11-28 (owner decisions, second round)

- Q: Who may touch a Bitwarden credential? → A: The runner only. No AI agent ever sees one, and a pipeline script receives only what it needs.
- Q: How does a step get a website login from the Bitwarden vault (Password Manager)? → A: The runner holds the vault unlock credentials as runner settings in Bitwarden Secrets Manager, which steps may never request. The pipeline declares which vault login items and which fields a step needs. The runner unlocks the vault, reads only those, passes only those field values as step environment variables, adds them to the Redactor, then locks or logs out and removes the `bw` session and data directory before the step starts. A step never gets a session key or an unlock credential. Declared item names are allowlisted per repo in the watch list, the same way 010 allowlists secret names. Recommended: give the Bitwarden identity access to one collection that holds only those items.
- Q: One encryption key for state and artifacts? → A: No, two. Artifacts use their own key, `FACTORY_CI_ARTIFACT_KEY`, different from `FACTORY_CI_STATE_KEY`. An agent gets the artifact key and a Loki read token and nothing else: never the state key, the state token, the Bitwarden access token or a vault credential. State (cookies) counts as a credential and is never part of a debug artifact. This replaces the earlier planning call of one key.
- Q: What stops a secret from landing in an artifact? → A: Before encrypting and uploading, the runner scans the bundle for every redacted value (exact, base64, each line, and inside zip entries for Playwright traces). A match refuses the upload with a fixed fault and keeps the run failed. Loki output stays redacted as before.
- Q: May the runner's own logs name vault items, fields or values? → A: No. Logs carry positions and counts only, for the vault.

### Session 2026-11-28 (planning calls, no owner answer needed; override any of them)

- Q: Where does the schedule live? → A: In the workflow file, one `cron` line per entry (at most 3), with a `case` on `github.event.schedule` that maps each string to a repo, a branch, a project and an entry number. There is no schedule file and no `due` command. A cron line cannot hold per-entry intervals, so each entry has its own line. The workflow file is reviewed on `main` and public, like the watch list.
- Q: What does "request order" mean? → A: The order in which runs register a queue ticket. The runner learns the state name from the fetched pipeline file, so registration happens after the fetch and the pipeline checks, a few seconds to a minute after the dispatch. Two starts within that gap can swap places.
- Q: Which pipelines queue? → A: Those that declare a `[state]`, keyed by the pipeline repo, project and state name inside the store repo. Pipelines with no state do not queue, and the workflow's concurrency group is removed because it cancels pending runs. Two stateless runs of one project can now overlap.
- Q: Which repos may keep state? → A: Only a watch entry with `state = true`, and only a private repo. The entry is a reviewed file on `main`, so a branch cannot widen it. A public repo cannot opt in, because its release assets would be public.
- Q: Stale lock? → A: Every ticket carries a renewal time that its run refreshes every 60 s while it waits and while it runs. A ticket not renewed for 10 minutes is stale and the next waiter removes it. A run that finds its own ticket gone never writes state.
- Q: Maximum wait and queue size? → A: None. Every requested run waits for its turn (FR-011). The only drop is a run that cannot win a turn before its job budget (FR-012, line `queue wait expired`, with an alert). The workflow job limit rises from 45 to 210 minutes. A hold is at most 45 minutes: 5 restore, 5 vault read, 30 pipeline, 5 save.
- Q: Names of the vault settings? → A: `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD`. The runner hands them to the `bw` child as `BW_CLIENTID`, `BW_CLIENTSECRET` and `BW_PASSWORD`, in that child only. The `FACTORY_CI` prefix is already refused in every `secrets` list, so no new reserved-name rule is needed.
- Q: Where is the vault allowlist? → A: A new watch key `vault_items`, a list of exact item names, default none. A pipeline names the item and the field; the reviewed watch entry decides which items a repo may read. Fields are not allowlisted: the item is the boundary.
- Q: Which fields can a step read? → A: `username`, `password`, `uri` and custom fields, by exact name. The TOTP code and attachments are out of scope.
- Q: Does an artifact refusal fail a passing run? → A: Yes. A secret inside a declared artifact path is a leak in the making, so the run ends red even with `when = "always"` on a pass.
- Q: Retention? → A: State release: the 2 newest assets. Artifact release: the 10 newest bundles per project.
- Q: Are the new Bitwarden keys required? → A: Only when a run needs them. State needs the state token and the state key, artifacts need the state token and the artifact key, and a vault declaration needs the three vault settings. A run that declares none of them needs none, so existing runs keep working before the owner creates the keys.

### Session 2026-11-28 (plan calls that settle gaps found by the 002 plan)

- Q: What does a vault declaration look like in `.factory-ci.toml`? → A: A step key `vault`, a list of inline tables with exactly `item`, `field` and `env`: `vault = [{ item = "playwright-login", field = "username", env = "E2E_USER" }]`. Unknown keys inside an entry are errors. `[state]` and `[artifacts]` stay pipeline-wide.
- Q: Which steps see the state file? → A: Only the one step that declares it. A step key `state = true` marks it; `[state]` requires exactly one such step and no other step may carry the key. The runner decrypts the state into its own memory, writes the file at the declared path in the checkout with mode 0600 immediately before that step starts, and after that step ends (pass or fail) reads the file and removes it from the checkout before any other step starts. Install, lint and test steps, which run third-party code as root, never see it. Every other step must be a transitive dependency or dependent of the state step, so none can run beside it (`[state]` with a step that could run in parallel is `pipeline file refused`). The vault values already reach only the step that declares them (FR-044).
- Q: A step writes the state file as root with mode 0600. Does the save fail? → A: No, and the file is never made world-readable. The declaring step should end with a `chown` of the file to the workspace owner. As a backstop the runner runs one helper container (root, no network, the first step's image) that runs `chown -R` and `chmod -R u+rX` on the declared state and artifact paths to the workspace owner, never `a+r`, before it reads them. A file the runner still cannot read fails the save with `state save failed`. No world-readable copy of the state exists on the runner or in a bundle.
- Q: Which names may a step set? → A: The names in `env`, `secrets` and vault `env` entries are refused, in any letter case, when they start with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` (010 already refuses the first, second and last for `secrets`; 011 extends all four to `env` and to vault names), or when they equal a runner key.
- Q: May an artifact path be a folder? → A: Yes. A folder is bundled whole, recursively, regular files only (symlinks and special files are skipped, with a Loki line). A path that does not exist is skipped with a Loki line. The state path is a file only.
- Q: How does a step whose vault values are withheld end? → A: It never starts, and neither does any other step. There is no per-step skip: an item the entry does not allow ends the whole run with `[runner] vault item not allowed`, and a vault that cannot be read ends it with `[runner] vault unavailable`, both before any container, as 010 does for `step secret not allowed`. 010 also gates secrets by ref (`WatchEntry::secrets_allowed_on`, line `repository or ref not in the watch list`, whole run). Vault declarations, `[state]` and `[artifacts]` pass the same gate, so a ref the gate refuses never reads the vault, never touches a release and never gets a ticket.
- Q: Does a scan refusal also stop the state save? → A: No. The state save depends only on the pipeline having passed (FR-024, FR-026). A refused artifact fails the run with exit code 1 and leaves the save alone.
- Q: Which files can the scan not read? → A: Besides the list in FR-049, any file that is not text, JSON, zip (stored or deflate), PNG or JPEG. The scan cannot read those, so the upload is refused.
- Q: How does a scheduled run name its target? → A: The workflow `case` sets the repo, ref, project and entry number and passes `--repo`, `--ref`, `--project` and `--entry <n>` to `ci`; a manual run is entry 0. The cron lines are chosen by the owner, with a minute off the top of the hour where GitHub reports delays.

### Session 2026-11-28 (lead decisions after the devops assessment and the security threat model; they supersede any earlier line that disagrees)

- Q: Where do queue, state and artifacts live? → A: In a separate private repo that holds no code and no workflows (the store repo). Its `owner/name` is a new runner setting `FACTORY_CI_STATE_REPO` read from Bitwarden, so it never appears in this public repo. `FACTORY_CI_STATE_TOKEN` is scoped to that repo only (contents write). The agent's GitHub read token is scoped to that repo, and its Loki token to this tenant. A release in the store repo is still a release tag on a private repo. The pipeline's own repo must still be private and have `state = true` (FR-019).  Release tags are `ci-queue-<owner>_<name>-<project>-<state>`, `ci-state-<owner>_<name>-<project>-<state>` and `ci-artifacts-<owner>_<name>-<project>`, where `<owner>_<name>` is the pipeline repo and `<project>` is `root` for `.` (other dots become `_`). A tag longer than 200 bytes is `pipeline file refused`. The tag is never taken from the pipeline file.
- Q: Ticket names, starters and labels? → A: `t-<runid>-<attempt>-<entry>` (`GITHUB_RUN_ATTEMPT` is passed in; `-` for the entry of a manual run). An asset that is not `uploaded` and is younger than 120 s by `created_at` counts as live (see the critic session). Only assets whose name matches the ticket regex of contracts/release-store.md count as tickets. A label that is missing, malformed, or has an epoch more than 120 s ahead of the reader's latest `Date`, is stale. The holder re-lists on every renewal and logs an `overlap` line when a lower-id live ticket exists. The release canary runs 200 trials and two racing runners and passes before the first real run.
- Q: Duplicate scheduled ticks? → A: A duplicated tick runs twice, one after the other, through the queue. That matches "every requested run must run". No marker asset exists.
- Q: Timeouts and limits? → A: Job limit 210 minutes. The runner takes the budget end as its own start plus 200 minutes, unless the environment sets `FACTORY_CI_BUDGET_END` (tests), and waits until the budget end minus 45 minutes. The pipeline deadline is the smaller of 30 minutes from its start and the budget end minus 20 minutes. A 403, 429 or `x-ratelimit-remaining: 0` pauses store calls other than renewals until `retry-after`, `x-ratelimit-reset` or 60 s, at most 120 s; a renewal never pauses, and renewals failing for 600 s end the run with `queue renewal failed`. Mutations are at least 1 s apart. Any 4xx on a create triggers a re-list and a decision. A full list page is followed by pagination, or the run fails closed. The exit-time ticket delete has a 5 s limit and no retry.
- Q: What do the Loki lines look like? → A: They are fixed strings listed in `contracts/loki-lines.md`, each with a test. Grafana alert rules over them are written in `rust-ci-runner/alerts/factory-ci.yaml` with no-data set to Alerting. The README tells the owner to push a commit before day 55.
- Q: Which keys exist? → A: Thirteen runner keys: the four settings read at start, `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID`, `FACTORY_CI_VAULT_COLLECTIONID`. The vault reads pass `--organizationid` and `--collectionid` and `BW_NOINTERACTION=true`, and a list that returns any item outside the allowlist is `vault unavailable`.
- Q: What does the runner trust on disk and in the store? → A: A declared path is canonicalised and must start with the canonical checkout; it is opened with `O_NOFOLLOW|O_NONBLOCK`; size is judged by bytes read. Before the seal, the runner scans the state plaintext for every known secret and refuses the save (`state save failed`) on a match. Release listings are checked for name pattern and size before a download, and followed by pagination. Hosts are fixed (`api.github.com`, `uploads.github.com`), redirects are off, the download host must be `*.githubusercontent.com` over https.
- Q: What may an artifact contain? → A: Text, JSON, zip (stored or deflate entries), PNG and JPEG. Anything else is refused, and any member named `*.html` is refused (the Playwright HTML report stays off). Every zip member is read, `trace.network` included. Tar member names with `..`, a leading `/` or a control character are refused when building the bundle.
- Q: Which entries may use state or the vault? → A: Only entries with `secret_branches = ["main"]` written explicitly (no pattern), enforced by `plan --check` and the 010 gate. A pipeline with `[state]` may not have a step with `docker = true`. Vault item names in the public watch file are opaque (`ci-login-1`, not the site).
- Q: Which helper image chowns? → A: The runner's own pinned image (passed in as `FACTORY_CI_RUNNER_IMAGE`), not the first step's image.
- Q: Retired parts? → A: The `due` job, the schedule file, the slot marker, the pinned `due` binary, the queue cap of 3 and the 120-minute wait cap are gone (critic review decisions). The tick workflow `factory-ci-tick.yml` is deleted in 011 (FR-060).
- Q: Rollback order? → A: One commit removes `state`, `secret_branches` additions for state, `vault_items` and the schedule entries and reverts the pin, or the removal comes first. The 0.3.0 runner refuses unknown watch keys.

### Session 2026-11-28 (critic review decisions; they supersede any earlier line that disagrees)

- Q: Release tags and asset names? → A: Tags are `ci-queue+<owner>_<name>+<project>+<state>`, `ci-state+<owner>_<name>+<project>+<state>` and `ci-artifacts+<owner>_<name>+<project>`. The delimiter `+` sits outside the alphabets of owner, name, project and state; the root project `.` is written as `-`, which cannot be a folder name; dots in a project are kept. The entry number in an asset name is always digits (manual run 0, cron entries 1 to 3), and every asset kind has one regex in contracts/release-store.md: ticket `t-<digits>-<digits>-<digits>`, state `state-<digits>-<digits>-<digits>-<8 hex>.bin`, artifact `artifact-<digits>-<digits>-<digits>-<8 hex>.bin`. A run id that is not digits is replaced by a random decimal number.
- Q: What counts as an asset in flight? → A: Any asset that is not `uploaded` and is younger than 120 s by `created_at`. An older non-`uploaded` asset of any name is deleted. Retention counts only `uploaded` assets, so two failed uploads never push the last good state out.
- Q: Is a request ever dropped? → A: One way only: a run that cannot win a turn before its budget ends with `queue wait expired` and fires an alert. No queue cap, no wait cap. A ticket that was broken and registered again goes to the back; accepted residual.
- Q: Which release carries what? → A: 0.4.0: queue, state, artifacts, schedule, all 13 key names reserved, vault declarations parsed and refused with `vault unavailable`. 0.4.1: the vault read and the `bw` image. Until then logins come from 010 step secrets.
- Q: The `bw` server? → A: The US cloud, documented. With `FACTORY_CI_BWS_SERVER_URL` set and a vault declaration, the run ends `vault unavailable`. No new key.
- Q: The signal path? → A: GitHub gives about 10 s after a cancel (7.5 s between SIGINT and SIGTERM, then 2.5 s). The handler deletes the ticket first, within 5 s, and skips the Loki flush; SIGINT and SIGTERM have separate tests.
- Q: Which Environment may start a scheduled run? → A: An open owner decision recorded in the plan (a required reviewer on `factory-ci` would hold a scheduled run). Recommended: a second Environment `factory-ci-schedule`, limited to `main`, no reviewer, holding the same token.
- Q: The constitution? → A: Amended to 1.2.0: the exception for `factory-monorepo-ci` covers its own schedule.
- Q: The state step runs as which user? → A: Root, like every step; the README says so and tells the owner not to place a state step after an install step that runs package scripts.

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

### User Story 2 - Runs of one state never overlap and every run gets its turn (Priority: P1)

A scheduled run and a manual run start together, for the same state. Both must run, one after the other, in order.

**Why this priority**: Two runs that share a state would restore the same cookies and overwrite each other's save, or log in twice from two places.

**Independent Test**: Mock store. Start four `ci` processes for one state. Assert the pipeline windows do not overlap and the order equals the ticket order. A fifth waits as well; only a run that cannot get a turn before its job budget ends with `queue wait expired`.

**Acceptance Scenarios**:

1. **Given** run A holds the turn, **When** run B and C register, **Then** B starts after A ends and C after B ends, and each prints `[runner] waiting for the queue` once.
2. **Given** A's runner is killed, **When** 10 minutes pass without renewal, **Then** B removes A's ticket and starts.
3. **Given** the turns ahead of a waiter use up its job budget (the budget end minus 45 minutes), **When** that point passes, **Then** it removes its own ticket, prints `[runner] queue wait expired`, exits 1 and starts no step. This is the one accepted way a requested run does not run, and a Grafana alert fires on it.

---

### User Story 3 - The schedule starts the pipelines (Priority: P1)

The owner adds a `cron` line to `factory-monorepo-ci.yml` and a line in its `case` that maps that cron string to a repo, a branch, a project and an entry number from 1 to 3. The workflow starts the run with no dispatch.

**Why this priority**: The owner wants the browser test to run unattended.

**Independent Test**: The shell test of the workflow (`test-factory-monorepo-ci.sh`) asserts at most 3 `cron` lines, one `case` branch for each, an entry number for each, a failing default branch for an unknown schedule string, and the same `docker run`. A fixture `ci` run with `--entry 2` and a repo outside the watch list exits 1 with the watch-list line.

**Acceptance Scenarios**:

1. **Given** a `cron` line with its `case` branch, **When** GitHub fires it on `main`, **Then** the job runs `ci` for that branch's repo, ref and project with `--entry <n>`, and no other line runs.
2. **Given** a mapped repo or branch the watch list does not allow, **When** its cron fires, **Then** `ci` exits 1 with `[runner] repository or ref not in the watch list`.

---

### User Story 4 - A failed run leaves evidence an agent can read (Priority: P1)

The Playwright step fails. The runner bundles `test-results/` (the Playwright HTML report stays off, see FR-049), scans every member of the bundle including `trace.network`, encrypts it and uploads it. Loki says which release and asset. The agent opens it with the documented script and reads the trace.

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

A failed trace holds the typed password in `trace.network` inside `trace.zip`. The runner reads every zip member, including that one, finds the password before encryption and uploads nothing.

**Why this priority**: The artifact key goes to an agent. Anything inside the bundle, the agent can read.

**Independent Test**: A fixture that writes a zip whose inner entry, named `trace.network`, holds a canary in plain, in base64 and as one line of a multi-line value, and a state-file value of the FR-050 kind. Run each. No upload happens, the run fails, and clean content uploads. A bundle with an `.html` member is refused. A real Playwright login-failure trace from the 002 fixture passes the scan once and is refused when the typed password is seeded as a known value (canary of T031). A bundle that lists the state file path is refused at the pipeline check.

**Acceptance Scenarios**:

1. **Given** a declared artifact holds a known secret value in a zip entry, **When** the run ends, **Then** nothing is uploaded, stderr shows `[runner] artifact upload refused` and the exit code is 1.
2. **Given** an artifact with no known value in it, **When** the run ends, **Then** it is encrypted with the artifact key and uploaded, and the state key cannot open it.
3. **Given** an artifact path that is, contains or lies under the state path, **When** the pipeline is read, **Then** the run ends with `[runner] pipeline file refused`.

### Edge Cases

Answers to the edge-case checklist. "Handled" names the requirement.

- Inputs. State or artifact path absolute, with `..`, through a symlink, or outside the project: pipeline refused (FR-018). Two `[state]` tables: TOML error, refused. State name with `/`, uppercase, Unicode, NUL, over 64 characters: refused (FR-018). State file empty, over 10 MB, or missing after a pass: no save, `state save failed` for the oversized one, a Loki line for the missing one (FR-024). Artifact path that matches nothing: skipped, a Loki line. Bundle over 100 MB: not uploaded, a Loki line (FR-032). A fourth `cron` line, a `cron` line without its `case` branch, an unknown schedule string: the shell test and the job refuse (FR-003, FR-004). A project name with `..`, ending in `.` or `.lock`, used with `[state]` or `[artifacts]`: pipeline refused (the release tag would be invalid).
- State. First run with no state starts empty (FR-021). A crash between upload and delete leaves two assets, and the newest wins (FR-025). A truncated or tampered asset, or the wrong key: restore fails and the run stops, with state untouched (FR-022). Older asset format: no older version exists. A retry is safe: ticket and asset names carry the run id, so a rerun of the same run id replaces its own ticket and saves one new asset.
- Partial failure. Restore fails: no step starts, the ticket is released. Pipeline fails: state kept, artifacts uploaded, ticket released. Save fails after a pass: the run fails, the ticket is released, the old state stays (FR-026). Artifact upload fails: Loki line, exit code unchanged (FR-031). Release of the ticket fails: the stale rule frees it within 10 minutes (FR-014).
- Time and concurrency. Two runs register together: the store decides the order, and the order is stable (FR-010). Every GitHub call has a time limit and one retry on no answer or 5xx; a 403 or 429 pauses all calls (FR-036). Clock skew between runners: ticket age comes from the GitHub server time, never the runner clock (FR-013). Scheduled tick delayed or skipped: no catch-up, so a run can be missed or doubled (R5). An interval boundary crossing a month or daylight saving change: intervals count UTC minutes since the epoch, so neither matters.
- Resources. Waiters poll every 20 s, so each waiter costs about 3 calls a minute against the token's 5,000 an hour. Releases hold at most 2 state assets and 10 artifacts per project. A waiting job holds one hosted VM until its turn or its budget end (R7).
- Trust. Who can name a state: the pipeline author, inside the repo's own releases, only when the reviewed watch entry says `state = true` and the repo is private. Who can reach the token: the runner process only. What the pipeline author cannot do: write another repo's release, read a setting or a vault credential, widen an entry, choose a release tag outside the fixed naming (FR-002, FR-019, FR-034, FR-042). What the agent cannot do: open state, because it holds neither the state key nor the token (FR-027, FR-048).
- Vault and scan inputs. A vault item that matches zero or more than one login in the allowed collection, a missing field, an empty field, a value with NUL or with a trimmed line under 4 characters, or a locked or revoked identity: `vault unavailable`, no container (FR-045). Two steps declaring the same environment variable for different fields, or one that also appears in `env` or `secrets`: pipeline refused (FR-042). An item name with Unicode, spaces or `=`: allowed, matched by exact bytes, and passed only through the environment, never argv. A zip with an encrypted entry, nesting deeper than 2, an entry count over 10,000 or more than 500 MB uncompressed: not scannable, so no upload (FR-049). A value split across two lines or transformed beyond base64 is not found (R2, R11).
- Vault resources and failure. The `bw` call has a time limit and one retry (FR-036). A `bw` that hangs or crashes leaves its data directory, which the runner removes anyway (FR-046). A vault that lock or logout fails on ends the run (FR-046).
- UI. No UI. The surface is the fixed stdout and stderr lines (FR-037).

## Requirements *(mandatory)*

Each requirement is an acceptance criterion in the form WHEN trigger THE SYSTEM SHALL response. Tags: `[error]` is a failure path, `[boundary]` is an edge of an allowed range. Each maps to at least one unit test under `cargo test --lib` (Principle V); workflow and file-format checks run in `factory-ci-selftest`.

### Functional Requirements

**Schedule**

- **FR-001**: WHEN a `cron` line of `factory-monorepo-ci.yml` fires on `main` THE SYSTEM SHALL map its `github.event.schedule` string through a `case` in the workflow to a repo, a branch, a project and an entry number (1 to 3) and start one `ci` run for it through the same `ci` command and watch-list checks as a manual run (`--entry <n>`; a manual run is entry 0). The repo, branch and project of a mapped target sit in the workflow file, which is public and reviewed on `main`, like the watch list. The job name carries the entry number.
- **FR-002**: WHEN a mapped run starts THE SYSTEM SHALL run it only if the watch list has an entry for its repo with a numeric `id` whose `branches` match the branch. A cron mapping grants nothing the watch list does not.
- **FR-003**: THE SYSTEM SHALL keep at most 3 `cron` lines in the workflow, each with its own `case` branch, and an unknown `github.event.schedule` string SHALL fail the job before any container starts. A duplicated tick runs a second time through the queue, after the first. `[boundary]` 3 lines are accepted, 4 are refused by the shell test.
- **FR-004** `[error]`: WHEN the shell test reads the workflow THE SYSTEM SHALL fail on a fourth `cron` line, a `cron` line with no `case` branch, a `case` branch with no `cron` line, an entry number outside 1 to 3, a repeated entry number, or a mapped value that is not a plain word. No schedule file exists.
- **FR-005**: WHEN a run starts by manual dispatch THE SYSTEM SHALL behave as before, with the same inputs. Dispatch inputs never select a schedule entry.

**Queue**

- **FR-006**: WHEN a pipeline declares a `[state]` and passes the file, docker and secret checks THE SYSTEM SHALL register one ticket in the queue of that state in the store repo, before restoring state or starting any container. A pipeline with no `[state]` registers nothing.
- **FR-007**: WHEN a ticket is first in line THE SYSTEM SHALL let its run restore, run and save. WHEN it is not THE SYSTEM SHALL start no container and touch no state, poll every 20 s, and print `[runner] waiting for the queue` once to stdout.
- **FR-008**: WHEN several tickets exist THE SYSTEM SHALL run them in registration order, one at a time. The turn passes only after the previous run released its ticket or its ticket went stale. No ticket is skipped, reordered or dropped except by FR-011, FR-012 and FR-014.
- **FR-009**: WHEN a run ends, whatever the result (pass, fail, refusal after registering, panic, SIGTERM, SIGINT) THE SYSTEM SHALL remove its ticket before exit. A cancelled GitHub job sends SIGTERM and the runner gets its grace period to do so.
- **FR-010**: WHEN two runs register at the same moment THE SYSTEM SHALL give them one stable order decided by the store, the same for both.
- **FR-011**: THE SYSTEM SHALL NOT cap the number of tickets of a state. Every requested run waits for its turn. A broken ticket that is deleted and registered again goes to the back; that is an accepted residual.
- **FR-012** `[error]`: WHEN a run has not won its turn by `FACTORY_CI_BUDGET_END` minus 45 minutes (restore 5, pipeline 30, save 5, and 5 of margin) THE SYSTEM SHALL remove its ticket, print `[runner] queue wait expired`, exit 1 and start no container, and the alert rules SHALL fire on it. The budget end is the runner start plus 200 minutes unless the environment sets `FACTORY_CI_BUDGET_END` (tests); the job limit is 210 minutes. `[boundary]` One second before the point the run keeps waiting.
- **FR-013**: WHEN a run keeps a ticket THE SYSTEM SHALL renew it every 60 s, waiting or running, using the store's server time to judge age, never the runner clock.
- **FR-014** `[error]`: WHEN a ticket has gone 10 minutes without renewal THE SYSTEM SHALL treat it as stale, let the next waiter remove it and log the removal to Loki as step `ci`. `[boundary]` 9 minutes 59 seconds is not stale, 10 minutes is. A holder whose renewals fail does not stop its pipeline.
- **FR-015** `[error]`: WHEN a run finds its own ticket gone before it saves state THE SYSTEM SHALL write nothing, end the run with exit code 1 and `[runner] state save failed`, and log the cause to Loki.
- **FR-016**: WHEN waiting runs show anything publicly THE SYSTEM SHALL show only the line of FR-007. Positions, ticket names, ages, state names and repo names go to Loki as step `ci` only.

**Declaring state and artifacts**

- **FR-017**: THE SYSTEM SHALL accept two optional tables in `.factory-ci.toml`: `[state]` with `name` and `path`, and `[artifacts]` with `paths` (a list) and `when` (`"failure"` or `"always"`, default `"failure"`), and the step key `state = true`, carried by exactly one step when `[state]` exists and by none otherwise. Unknown keys stay errors.
- **FR-018** `[error]`: WHEN `name` is not 1 to 64 characters of `[A-Za-z0-9_-]`, or `path` or an artifact path is empty, absolute, contains `..`, a NUL or a newline, or resolves through a symlink or outside the project folder, or a list holds more than 20 paths, or an artifact path equals, contains or lies under the state path, or `[state]` has no step with `state = true` or more than one, or another step is neither a transitive dependency nor a dependent of the state step (it could run beside it), or a name in `env`, `secrets` or a vault `env` starts, in any letter case, with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` THE SYSTEM SHALL end the run with `[runner] pipeline file refused` and start no container. `[boundary]` A 64-character name and 20 paths are accepted, 65 and 21 are not. A sibling path that only shares a prefix with the state path (`.ci-state` and `.ci-state-2`) is accepted.
- **FR-019** `[error]`: WHEN a pipeline declares `[state]` or `[artifacts]` and the repo's watch entry lacks `state = true`, or the repo is public (judged from the identity check the runner already makes) THE SYSTEM SHALL end the run with `[runner] state not allowed`, register no ticket and start no container. The release a run writes to is fixed by the repo's own name and the state name or project, never taken from the file. `plan --check` accepts `state` only with a numeric `id`.

**State**

- **FR-020** `[error]`: WHEN a pipeline declares `[state]` and `FACTORY_CI_STATE_TOKEN` or `FACTORY_CI_STATE_KEY` is missing from Bitwarden (zero matches), or declares `[artifacts]` and `FACTORY_CI_STATE_TOKEN` or `FACTORY_CI_ARTIFACT_KEY` is missing, THE SYSTEM SHALL end the run with `[runner] state store not configured` before registering a ticket. Two matches, an empty value, a value with CR, LF or NUL, leading or trailing whitespace, or under 4 characters, are refused as for the four existing settings. A pipeline with `[state]` or `[artifacts]` also needs `FACTORY_CI_STATE_REPO` (the store repo, `owner/name`), and both keys must exist and differ, whichever of the two the pipeline uses: a missing key or equal values end the run the same way. A key (`FACTORY_CI_STATE_KEY` or `FACTORY_CI_ARTIFACT_KEY`) must be standard base64 or hex text that decodes to 32 bytes or more, with at least 8 distinct byte values in the decoded bytes. `[boundary]` A key that decodes to 32 bytes is accepted and 31 are refused.
- **FR-021**: WHEN the runner is the first in line and the state release has no asset THE SYSTEM SHALL start the pipeline with no state file at the declared path. This is the only case that starts empty.
- **FR-022** `[error]`: WHEN the newest state asset cannot be downloaded, authenticated or decrypted (wrong key, truncated, tampered, unreadable) THE SYSTEM SHALL end the run with `[runner] state restore failed`, start no container, change nothing in the store and release its ticket. It never falls back to an older asset or an empty state.
- **FR-023**: WHEN restore succeeds THE SYSTEM SHALL write the decrypted file at the declared path inside the checkout, mode 0600, immediately before the one step that declares it (`state = true`) starts, and remove it as soon as that step ends. No other step starts while it exists, and none sees it. Only the declared file is restored.
- **FR-024**: WHEN the pipeline passed THE SYSTEM SHALL read the declared file from the checkout and save it, encrypted with a fresh random nonce, as a new asset, before it removes the checkout. WHEN the file is missing THE SYSTEM SHALL save nothing and log a line to Loki. The runner reads the file right after the declaring step ends, after a root helper container has chowned the declared paths to the workspace owner (Clarifications, plan calls); a file it still cannot read is `state save failed`. It removes the file from the checkout at that moment and keeps the bytes in memory until the save. `[boundary]` A file of exactly 10 MB is saved. One of 10 MB plus 1 byte is refused (`[runner] state save failed`). A symlink at the path is refused.
- **FR-025**: WHEN a save uploads a new asset THE SYSTEM SHALL delete older assets only after the new one is stored and its size matches, and keep the 2 newest. WHEN restore sees several assets THE SYSTEM SHALL pick the newest by the store's order, not by name alone.
- **FR-026** `[error]`: WHEN the pipeline failed, was skipped in part, or hit a runner fault THE SYSTEM SHALL save no state. WHEN the pipeline passed and the upload or the size check fails THE SYSTEM SHALL end the run with `[runner] state save failed`, keep the previous state and release its ticket.
- **FR-027** `[error]`: THE SYSTEM SHALL encrypt every state file with an authenticated cipher under `FACTORY_CI_STATE_KEY` only, and every artifact bundle under `FACTORY_CI_ARTIFACT_KEY` only, so a changed byte fails decryption and one key never opens the other's assets. WHEN an asset is opened with the wrong one of the two keys THE SYSTEM SHALL fail authentication and return no plaintext. The state file is a credential: its content never enters an artifact (FR-018, FR-049, FR-050). No plaintext of either leaves the runner process or the checkout.

**Artifacts**

- **FR-028**: WHEN the pipeline did not pass and at least one step started, or `when = "always"`, THE SYSTEM SHALL bundle the declared paths that exist (a folder whole and recursive, a missing path skipped with a Loki line), scan the bundle (FR-049), encrypt it under the artifact key and upload it as one asset to the artifact release of the run's project in the repo that owns the pipeline. It runs after the pipeline, before the checkout is removed, and whether or not a step failed. It needs no ticket.
- **FR-029**: WHEN an upload completes THE SYSTEM SHALL log the release tag and the asset name to Loki as step `ci`, with the run id, and print nothing about them publicly.
- **FR-030**: WHEN an artifact upload completes THE SYSTEM SHALL keep the 10 newest bundles of that project's release and delete the rest. `[boundary]` With 10 bundles nothing is deleted. The 11th upload deletes the oldest.
- **FR-031** `[error]`: WHEN an artifact bundle or its upload fails THE SYSTEM SHALL log the cause to Loki as step `ci` and leave the run's exit code as the pipeline decided. It never turns a failure into a pass or a pass into a failure. The one exception is a refusal by the secret scan (FR-049), which ends the run with exit code 1.
- **FR-032** `[boundary]`: WHEN a bundle would exceed 100 MB THE SYSTEM SHALL upload nothing and log the size to Loki. 100 MB exactly is uploaded.
- **FR-033**: THE SYSTEM SHALL document one command line that downloads an artifact asset with `gh release download` and decrypts it, in `rust-ci-runner/README.md`. The key comes from the environment or standard input, never argv, and the doc names it by its Bitwarden name only. The same section states what an agent may hold: the artifact key, a Loki read token, and a GitHub token with contents read on the one private repo. It names what an agent must never hold (FR-048).

**Vault logins**

- **FR-042** `[error]`: WHEN a step declares a vault login THE SYSTEM SHALL accept only `vault = [{ item = "...", field = "...", env = "..." }]` on the step, entries with exactly those three string keys (item name, field name and environment variable name), and SHALL end the run with `[runner] pipeline file refused` before any vault call when the environment variable name breaks the 010 name rules, starts with `FACTORY_CI`, `BW_` or `BITWARDENCLI`, repeats in the step (across `env`, `secrets` and vault entries), or the step has `docker = true`. `[boundary]` A step may declare 20 fields and the pipeline 50; 21 and 51 are refused.
- **FR-043** `[error]`: WHEN any step names a vault item that is not listed in `vault_items` of the run's watch entry (including when the entry has no such key) THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] vault item not allowed`, unlock no vault, start no container and read no value. Items match by exact name. There is no per-step skip: the whole run ends. `[boundary]` An item that differs only by case is not allowed. `plan --check` reports a `vault_items` entry that is empty, longer than 100 characters, or holds a control character, and refuses the file whole.
- **FR-044**: WHEN every declared item is allowed THE SYSTEM SHALL read all declared fields once, before the first container starts, and pass each value only to steps that declared it, through the environment of the engine child process, never argv. WHEN several steps declare the same item and field THE SYSTEM SHALL read it once.
- **FR-045** `[error]`: WHEN a declared item matches no login or more than one in what the identity can see, a declared field is missing, empty or holds a NUL, a value has a trimmed line under 4 characters, or the vault cannot be unlocked THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] vault unavailable`, and start no container. `[boundary]` A trimmed line of exactly 4 characters is accepted, 3 is refused. The vault settings missing from Bitwarden (`FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`) end the run the same way.
- **FR-046** `[error]`: WHEN the vault reads end, whether they succeeded or failed THE SYSTEM SHALL lock or log out of the vault and remove the `bw` session and data directory before any step starts, and SHALL give no `bw` child or step a session key, an unlock credential or the data directory path. WHEN lock, logout or removal fails THE SYSTEM SHALL end the run with `[runner] vault unavailable` and start no container.
- **FR-047**: THE SYSTEM SHALL add every vault value to the Redactor before any line that could hold it is written, with its base64 forms and each line of 4 or more characters, as for step secrets. The runner's own logs, stdout, stderr and Loki detail lines for the vault carry positions and counts only (for example `declared item 2 of 3`), never an item name, a field name or a value.
- **FR-048**: THE SYSTEM SHALL keep these out of reach of any agent and any step: the vault credentials, `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY` and the Bitwarden access token. An agent's whole kit is `FACTORY_CI_ARTIFACT_KEY`, a Loki read token and a read-only GitHub token; no artifact, Loki line or public line may carry anything else.

**Artifact scan**

- **FR-049** `[error]`: WHEN a bundle is ready and before it is encrypted THE SYSTEM SHALL scan every file in it for each value the Redactor holds (the runner settings, step secrets, vault values, the Bitwarden access token, the FR-050 state strings), each in its exact form, in the forms the Redactor knows (`Log::secret_forms`), and as each line of 4 or more characters of a multi-line value, and SHALL scan the contents of every zip entry, `trace.network` included, to a nesting depth of 2. WHEN a value matches, or a file cannot be scanned (an encrypted entry, depth over 2, more than 10,000 entries, more than 500 MB uncompressed, an unreadable file, a file that is not text, JSON, a zip with stored or deflate entries, PNG or JPEG, a member named `*.html`) THE SYSTEM SHALL upload nothing, print exactly `[runner] artifact upload refused`, end the run with exit code 1 and log to Loki only a position and a count, never the value, the matching file name or an offset. `[boundary]` A value of exactly 4 characters on one line is scanned for; a nested zip at depth 2 is scanned and one at depth 3 refuses the upload.
- **FR-050**: WHEN the state file is JSON THE SYSTEM SHALL add each string it holds under a key named `value` of 16 or more characters (the cookie and local storage values of a Playwright storageState) to the Redactor and to the scan set, and WHEN it is not JSON THE SYSTEM SHALL add its whole content when that is under 1 MB. `[boundary]` A 16-character cookie value is added, a 15-character one is not.

**Settings and containment**

- **FR-034** `[error]`: WHEN a watch entry lists any of `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID` or `FACTORY_CI_VAULT_COLLECTIONID` in `secrets` THE SYSTEM SHALL refuse the whole watch file. WHEN a pipeline lists any of them in `secrets`, or as a vault environment variable name, THE SYSTEM SHALL end the run with `[runner] pipeline file refused`. None of the nine values reaches a step, a step container argument, an engine child other than the one call that needs it, or a log body. The test list of 010 FR-004 grows from four names to thirteen, and 011 supersedes that list. Every name in a `secrets` list and every vault `env` name must be disjoint across the whole pipeline; a name in both is `pipeline file refused`.
- **FR-035**: THE SYSTEM SHALL mask all nine new values, their base64 forms and the base64 of `x-access-token:<token>` in every output path, as it does for the four settings, before any line that could hold them is written.
- **FR-036**: WHEN the runner calls `bw` THE SYSTEM SHALL give the call a time limit (60 s) and one retry, never after a rejected credential. WHEN the runner calls the GitHub API for the queue, state or artifacts THE SYSTEM SHALL use `FACTORY_CI_STATE_TOKEN` with a time limit, one retry on no answer or 5xx and a pause on 403 or 429, and never use `FACTORY_CI_TOKEN` for a write or `FACTORY_CI_STATE_TOKEN` for a clone. The token never appears in argv, a file or a git config.

**Public output**

- **FR-037**: WHEN the run prints THE SYSTEM SHALL keep stdout to the closed grammar of 008, extended by exactly one line, `[runner] waiting for the queue`, and stderr to the closed fault list, extended by exactly these lines: `queue wait expired`, `queue renewal failed`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`. No repo, branch, project, state, release, asset, vault item, vault field or secret name, and no count, position or age, appears in any public line. The lines join the closed list in `log.rs` and its test.

**Rollout**

- **FR-038**: THE SYSTEM SHALL release in two steps. Runner 0.4.0 carries the queue, state, artifacts and the schedule, reserves all 13 runner key names, parses and validates the vault declarations and `vault_items`, and ends any validated vault declaration with `[runner] vault unavailable` before a container starts. Runner 0.4.1 adds the vault read (FR-044 to FR-047, FR-056) and the `bw` image. Each release: `Cargo.toml` and `Cargo.lock` agree, tag `rust-ci-runner-v<version>` on a commit on `main`, then `RUNNER_IMAGE` changes in a separate reviewed commit to the new digest, as the README's bootstrap order says. The release smoke step still passes. Until 0.4.1, a login comes from 010 step secrets.
- **FR-039**: THE SYSTEM SHALL change `factory-monorepo-ci.yml` so that it adds up to 3 `schedule` cron lines with the `case` of FR-001, removes the `concurrency` block, raises `timeout-minutes` of the run job to 210, keeps `if: github.ref == 'refs/heads/main'`, holds no new GitHub secret, and keeps the single `docker run` for the run job. It passes `-e GITHUB_RUN_ATTEMPT` and `-e FACTORY_CI_RUNNER_IMAGE`. The job uses the Environment `factory-ci`, or `factory-ci-schedule` for scheduled starts when the owner chooses the second Environment (plan, owner decisions). The header comment explains why the concurrency group is gone. The tick workflow `factory-ci-tick.yml` is deleted in 011 (FR-060).
- **FR-040**: THE SYSTEM SHALL add `state = true`, `secret_branches = ["main"]` and (from 0.4.1) the `vault_items` of the Playwright login to the `sudtanj/factory-monorepo` entry only, and add the first cron line and its `case` branch for the Playwright project when its pipeline lands, and update the header comments of `.github/factory-ci-repos.toml`. The `sudtanj/infrastructure` entry gets none of them.
- **FR-041**: THE SYSTEM SHALL update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description to match: the nine new settings and the Bitwarden project guidance ("holds only the thirteen keys"), the collection-limited account (0.4.1), the store repo, the `ci` step list, the fault list, the script that opens an artifact and the agent kit, the vault declaration (0.4.1), the cron lines and their `case`, the queue behaviour and the one accepted drop, the removed concurrency group, the retired tick and its settings, a runbook for `state restore failed` (delete the newest state asset, or all of them to start empty), a release checklist line naming the pins that move per release (image digest, agent script sha), and the risks. This is the last task in `tasks.md`.

### Key Entities

- **Cron entry**: one `cron` line and its `case` branch in `factory-monorepo-ci.yml`: repo, branch, project, entry number 1 to 3. Reviewed on `main`.
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

- **SC-001**: Over the first 7 days of an hourly cron line, at least 90% of runs after the first start with a restored state, and zero pairs of runs of one state overlap. Measured from Loki `ci` lines (restore result, run start and end times) per state.
- **SC-002**: Zero requested runs for one state are lost to anything other than `queue wait expired`, `queue renewal failed` and a missed tick. Measured by comparing requested runs (Actions run list) with ticket lines in Loki. Any `queue wait expired` in the week is read and either explained or the numbers of FR-012 change.
- **SC-003**: For every failed run that declared artifacts, an agent finds the release and asset in Loki and opens the bundle with the documented command in under 5 minutes, on its first try. Measured by one trial by the owner after release.
- **SC-004**: Across a hostile fixture (canary state, token and key, artifact with a canary inside, step listing the settings), zero lines of stdout, stderr, child argv and Loki bodies hold a canary, no release asset holds a plaintext canary, and zero public lines hold a repo, state, release or asset name. Measured by the unit and fixture tests.
- **SC-005**: A failed run never changes the stored state: a test that fails the pipeline after a write to the state file finds the newest asset byte-identical to before.
- **SC-006**: `cargo test --lib` passes in the `rust-ci-runner` pipeline with tests for FR-001 to FR-050, and the release publishes a 0.4.0 digest that the workflow pin matches.
- **SC-007**: In a fixture run with a fake `bw`, the step's environment and visible files hold the declared variables and zero vault credentials, session keys or `bw` data. Zero runner log lines, public or Loki, hold a vault item name, field name or value. Measured by the fixture test and a search of every output.
- **SC-008**: Across a hostile artifact fixture (each known value in plain, base64, one line of a multi-line value, inside a zip and inside a nested zip), 100% of bundles are refused with no upload, and a clean bundle uploads. The state key opens no artifact and the artifact key opens no state asset. Measured by the unit tests.
- **SC-009**: After one debugging trial, the agent's kit holds the artifact key, a Loki read token and a read-only GitHub token, and the owner confirms it never held any other credential. Measured by the owner's review of what the agent was given.

## Assumptions

- The owner creates the store repo and a fine-grained token scoped to it (contents write), the two keys (each `openssl rand -base64 33`, decoding to 32 bytes or more, different from each other), and the Bitwarden Secrets Manager entries for all of them and for the three vault settings. No code or doc holds a value. Anyone with write access to the Bitwarden project can read them all: that is already a trust root (008).
- The vault credentials unlock a Bitwarden Password Manager account. A Bitwarden API key belongs to a whole account, so limiting it to one collection takes a separate account that is a member of an organization with access to that collection only. The owner creates it; the spec recommends it (R12) and cannot enforce it.
- The `bw` command line client is not in the runner image today. The plan adds it pinned by SHA-256 like `bws`, or picks another way to read a vault, and justifies the size and the new dependency in Complexity Tracking. The `bw` call is a child process with a cleared environment holding only what it needs.
- The agent holds its own GitHub credentials for `gh`. The owner gives it a token with contents read on the one private repo. That token can download the encrypted state asset, which is useless without the state key.
- GitHub's store gives an atomic create that fails when the name exists (a ref or a release tag), server timestamps and release asset upload and delete. The plan picks the mechanism for tickets and renewals; this spec fixes only the behaviour. A queue operation that cannot be made atomic on GitHub is a blocker for the plan, not a reason to loosen FR-008.
- The runner has no encryption dependency today (`Cargo.toml` lists `getrandom`, `serde`, `serde_json`, `toml`, `ureq`). The plan chooses an authenticated cipher that the owner or an agent can open with a documented command, and justifies any new crate or image tool in Complexity Tracking.
- The state file is small (cookies, tens of KB). A directory as state, and a file over 10 MB, are out of scope.
- The step container runs as root. The state file is written at mode 0600 and the runner reads it after a root helper container chowns the declared paths to the workspace owner (Clarifications, plan calls), so a step may keep 0600 and no copy is world-readable.
- Scheduled workflows run on the default branch and GitHub may delay or skip them, so a run can be missed or doubled; a doubled tick queues and runs twice.
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
- **R3, an artifact holds secrets**: a Playwright trace records typed input and network traffic, so it can hold the login and cookies. Encryption protects the release from readers of the repo, not from whoever holds the artifact key, and the agent holds it. Mitigation: the scan of FR-049 reads every member including `trace.network` and refuses a bundle that holds any value the runner knows (the FR-050 state strings included); `.html` members and unreadable formats are refused; the artifact key differs from the state key; the state never enters a bundle. 002 keeps the HTML reporter off and scrubs its trace. A canary on a real failure trace runs before the first real run. The scan cannot see a value it does not know (R11).
- **R4, concurrency group removed**: two stateless runs of one project can overlap, and no cancel of pending runs happens any more. Mitigation: documented in the README. The Playwright project declares state, so it queues, and a duplicated tick runs twice in sequence.
- **R5, schedule jitter**: GitHub delays and skips scheduled runs, so a run can be missed. A doubled tick runs twice through the queue. Mitigation: the queue; SC-001 measures it.
- **R6, schedule switched off**: a public repo with no activity for 60 days stops scheduled runs without a message. Mitigation: the README tells the owner to push a commit before day 55, and an alert rule fires on a missing `pipeline passed`, no-data set to Alerting.
- **R7, long waits cost a hosted VM**: a waiting job holds a runner until its turn or its budget. The repo is public, so minutes are free, but the concurrent-job limit applies; with no ticket cap, many waiters hold many VMs. Mitigation: 3 cron lines at most, and `queue wait expired` ends a run that cannot be served.
- **R8, request order is approximate**: registration happens after the fetch, so near-simultaneous starts can swap (Clarifications). Mitigation: accepted. Strict order holds from registration on.
- **R9, rollout order**: a pin bump before the digest fails the pull. A `[state]` pipeline before `state = true`, the token and the key exist fails with a fixed line. FR-038 and FR-040 set the order: release, pin, watch entry and Bitwarden keys, then the pipeline.
- **R10, one state per name per repo**: two pipelines of one repo that pick one state name share one queue and one file. Mitigation: pick distinct names.
- **R11, the scan is exact**: it finds known values in plain, in the base64 forms the Redactor knows and by line. A value a page renders differently (URL-encoded, split in a screenshot as pixels) passes. A screenshot of a page that shows a secret is an image, and no scan reads it. Mitigation: keep `when = "failure"`, declare only the paths that are needed, and treat artifacts as sensitive. A false positive (a 4-character value that matches ordinary text) blocks an upload and fails the run; the 4-character floor is the same as for masking.
- **R12, the vault credentials are a master key**: `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD` unlock the account. Anyone who can read them from Bitwarden Secrets Manager, or reach the runner process memory, can read every item that account sees. The runner holds them only in memory and in one `bw` child, but the child's environment is readable by anything running as the same user in the container, as with the existing access token. Mitigation: a dedicated Bitwarden account in an organization that can see only the collection holding the allowlisted items, not the owner's personal vault; the `vault_items` allowlist; no step ever sees the credentials.
- **R13, allowlist is by item, not field**: a pipeline that may read an item may read every field of it. Mitigation: keep one item per purpose and put only the fields it needs in it.
- **R14, a vault read takes the turn**: reads happen inside the queue turn, so a slow vault holds the next run. Mitigation: 60-second limit, one retry, and the 5-minute share of the 45-minute hold.

## Requirements from the lead decisions (FR-051 to FR-059)

These formalise the decisions session so each has an identifier and a test. They supersede any older line that disagrees.

- **FR-051**: THE SYSTEM SHALL keep queue, state and artifacts in the store repo named by `FACTORY_CI_STATE_REPO`, use a token scoped to that repo only, require `private: true` from `GET /repos/<store>` before any ticket, fix the hosts to `api.github.com` and `uploads.github.com`, turn redirects off, and accept a download `Location` only over https on `*.githubusercontent.com`. The pipeline file never sets a tag or a repo.
- **FR-052**: WHEN a run waits THE SYSTEM SHALL cap the wait only by the budget of FR-012, and SHALL compute the pipeline deadline as the smaller of 30 minutes from the pipeline start and `budget end` minus 20 minutes (5 for the artifact scan and upload, 5 for the save, 5 for a vault retry, 5 for cleanup).
- **FR-053** `[error]`: WHEN the runner lists tickets THE SYSTEM SHALL count only assets whose name matches the ticket pattern of contracts/release-store.md, count any asset that is not `uploaded` and is younger than 120 s by `created_at` as in flight (live), delete a non-`uploaded` asset of any name that is older, treat a ticket whose label is missing, malformed or more than 120 s ahead of the reader's latest `Date` as stale, and log `overlap` at a renewal that sees a live ticket with a lower id. Retention counts only `uploaded` assets. `[boundary]` A label epoch 120 s ahead is accepted, 121 s is stale; an asset 119 s old is in flight, 120 s is not.
- **FR-054**: WHEN the runner calls the store THE SYSTEM SHALL pause all calls except renewals on a 403, a 429 or `x-ratelimit-remaining: 0` until `retry-after`, `x-ratelimit-reset` or 60 s, never longer than 120 s, space mutations 1 s apart, treat any 4xx on a create as re-list and decide, delete a half-created own-name asset after a timed-out upload and retry once, follow pagination and fail closed above 300 assets, check asset name and size before a download, retry a restore once on a 404, and give the exit-time ticket delete 5 s and no retry. A renewal never pauses: WHEN renewals fail for 600 s THE SYSTEM SHALL delete the ticket, stop the run, print `[runner] queue renewal failed` and exit 1.
- **FR-060**: THE SYSTEM SHALL delete `.github/workflows/factory-ci-tick.yml` and remove its mentions, and the owner settings only it used, from the READMEs, and update the selftest and shell tests that name it. The `plan` subcommand and the scripts it used stay in 011 unless the selftest needs them; their removal is left to a later cleanup feature. Rollback keeps the tick deleted.
- **FR-055** `[error]`: WHEN the runner reads a declared state or artifact path THE SYSTEM SHALL canonicalise it, require it to start with the canonical checkout, open it with `O_NOFOLLOW|O_NONBLOCK`, judge size by bytes read, and scan the state plaintext for every known secret form before sealing; a hit is `[runner] state save failed`. The state step runs as root; the README says so.
- **FR-056** `[error]` (0.4.1): WHEN the runner reads the vault THE SYSTEM SHALL pass `--organizationid` and `--collectionid` from `FACTORY_CI_VAULT_ORGID` and `FACTORY_CI_VAULT_COLLECTIONID` and `BW_NOINTERACTION=true`, keep vault values in a map keyed by `(item, field)` separate from step secrets, and end with `[runner] vault unavailable` when an item outside the allowlist comes back. The `bw` server is the US cloud; WHEN `FACTORY_CI_BWS_SERVER_URL` is set and a pipeline declares a vault entry THE SYSTEM SHALL end with `[runner] vault unavailable`.
- **FR-057**: THE SYSTEM SHALL emit exactly the Loki strings of `contracts/loki-lines.md`, test each, and ship alert rules in `rust-ci-runner/alerts/factory-ci.yaml` with no-data set to Alerting, and the README SHALL tell the owner to push a commit before day 55.
- **FR-058** `[error]`: WHEN a bundle is built THE SYSTEM SHALL refuse tar member names with `..`, a leading `/` or a control character, and WHEN it is scanned THE SYSTEM SHALL accept only text, JSON, zip with stored or deflate entries, PNG and JPEG, refuse any member named `*.html`, and read every zip member including `trace.network`. The Playwright HTML reporter must stay off in the pipeline (002 uses its own reporter).
- **FR-059** `[error]`: WHEN a watch entry has `state` or `vault_items` without `secret_branches = ["main"]` written explicitly THE SYSTEM SHALL refuse the whole watch file; WHEN a pipeline has `[state]` and any step with `docker = true` THE SYSTEM SHALL end with `[runner] pipeline file refused`.
