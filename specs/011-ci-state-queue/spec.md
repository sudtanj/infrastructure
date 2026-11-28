# Feature Specification: A strict queue, saved state and debug artifacts for the ci command

**Feature Branch**: `011-ci-state-queue`

**Created**: 2026-11-28

**Status**: Draft, amended 2026-11-28 by the owner rule "Amend 011" (no workflow YAML and no shell script changes; see Clarifications and "Out of scope after the owner rule")

**Input**: User description: "Run the `ci` pipelines on a schedule from reviewed config, queue runs that share a state strictly first in, first out, restore and save a pipeline's state (a Playwright storageState file) in an encrypted release on the private repo that owns the pipeline, and upload encrypted debug artifacts to that repo on failure. Builds on 010 (step secrets). Owner addition of the same day: the runner, never a step or an AI agent, unlocks the Bitwarden vault and hands a step only the login fields it declared; artifacts use their own key and are scanned for secrets before upload."

## Who this is for

The owner, who runs `factory-monorepo-ci` and writes the `.factory-ci.toml` files of the watched repos, and the AI agent the owner sends to debug a failed run. The first user is the Playwright browser-test project (spec 002 in `sudtanj/factory-monorepo`), which logs in to the owner's own sites with human-like input. A standing rule of the owner binds the whole feature: no AI agent ever sees a Bitwarden credential, and a pipeline step receives only the values it declared. The agent gets two things only: the artifact key and a Loki read token.

## Problem

`ci` starts only by hand, forgets everything between runs, and cannot say why a browser test failed. Evidence:

- `.github/workflows/factory-monorepo-ci.yml` has `workflow_dispatch` as its only trigger. The concurrency group is `factory-monorepo-ci-<repo>-<ref>-<project>` with `cancel-in-progress: false`. GitHub keeps one running and one pending run per group and cancels an older pending run when a third arrives, so a queue of three loses a run. The owner rule of 2026-11-28 keeps this file as it is, so the group, the 45-minute job limit and the image pin stay.
- `rust-ci-runner/src/ci.rs` fetches the branch into a new directory under `--work`, runs the pipeline and removes the directory (`remove_ws`). Nothing survives. A browser test that logs in with a username and password every run looks like a bot to the target site and may trip captchas or lockouts. It needs its cookies back.
- The public Actions log shows only fixed lines (`log.rs`). The step output that explains a failure goes to Loki, and a Playwright failure is mostly a trace and screenshots, which Loki lines cannot carry. An agent has nothing to read.
- A GitHub release asset on a private repo is downloadable by everyone with read access to that repo, so a cookie file stored there unencrypted is a session theft waiting for a read-only collaborator or a leaked read token.
- `FACTORY_CI_TOKEN` is read-only by design (spec 008). Writing a release needs a second token with contents write.

## Announcement

Pipelines run by `factory-monorepo-ci` can now remember their state and leave evidence when they fail. Declare a `[state]` file in the pipeline and mark the one step that uses it with `state = true`: the runner restores the last good copy just before that step and saves the new one after a passing run. Runs that share a state wait in a strict queue and run one after another in the order they asked, with a fixed `waiting for the queue` line in the public log. When a run fails, the runner scans the traces and screenshots the pipeline declared for every secret it knows, encrypts them and uploads them to a release on a private store repo. Loki names the release and asset. An agent reads Loki, downloads the asset and decrypts it with `rust-ci-runner decrypt-artifact`. State and artifacts are encrypted with two different keys from Bitwarden, and the runner holds the write token itself: steps can never request any of them. All of this is Rust and toml in `rust-ci-runner/`. The owner rule of 2026-11-28 forbids any change to a workflow or a script, so the schedule (cron lines), the vault read, the image pin and the alert and agent files are out of scope (see "Out of scope after the owner rule"). The tick workflow stays as it is and keeps its outside dispatcher. Runner 0.4.0 is built and released, and nothing pins it until the owner lifts the rule.

## Clarifications

### Session 2026-11-28 (owner rule "Amend 011"; it supersedes every earlier line that disagrees)

- Q: What did the owner decide? → A: No file under `.github/workflows/` and no file under `.github/scripts/` may change, ever. Only Rust code in `rust-ci-runner/`, `.factory-ci.toml` files and other toml files change. The owner chose "Amend 011": the tick workflow `.github/workflows/factory-ci-tick.yml` stays exactly as it is and keeps being dispatched by the outside scheduler; 011 adds no `cron` line to `factory-monorepo-ci.yml` and does not delete the tick. Feature 012 (Docker images) relies on the tick.
- Q: What stays in scope? → A: The queue (FR-006 to FR-016), state (FR-017 to FR-027), artifacts and the scan (FR-028 to FR-033, FR-049, FR-050, FR-055, FR-058), the thirteen reserved key names, the runner-only settings, the vault declaration check and its refusal `vault unavailable`, the public-line grammar and the Loki lines. All of it is Rust, plus `.factory-ci.toml` and `Cargo` files.
- Q: What is out of scope? → A: Everything that needs a workflow, a script, a Dockerfile or a YAML file: the schedule (US3, FR-001 to FR-004), the workflow edits (FR-039), the pin bump to 0.4.0 and every real run that follows it, the watch-file keys and first cron line (FR-040), the vault read and the `bw` image (US6, FR-044, FR-046, FR-047, FR-056), the alert-rule file and the agent script (parts of FR-057 and FR-033), the tick retirement (FR-060, reversed) and the Environment decision. Each has its reason in "Out of scope after the owner rule". The owner may bring any of them back by lifting the rule.
- Q: How does a run learn its attempt number and its own image if the workflow cannot pass `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_RUNNER_IMAGE`? → A: The unchanged workflow names the runner container `factory-ci-runner-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT` and mounts the docker socket. The runner asks docker once about its own container (`docker inspect` with the container id from the hostname) and reads the name suffix and `.Config.Image`. `GITHUB_RUN_ATTEMPT` and `FACTORY_CI_RUNNER_IMAGE`, when set, win (tests and hand runs). The attempt falls back to `1`, and a missing image ends a stateful run with `[runner] runner fault` before any ticket, because the chown helper cannot start without it.
- Q: Is there an entry number? → A: No flag. `ci` takes no `--entry`, because only the workflow could pass it. The `<entry>` field of asset names stays in the format, always `0`, so the formats need no second version when a schedule returns.
- Q: Which numbers apply with a 45-minute job and the concurrency group kept? → A: The job limit is 45 minutes (a Rust constant that a test compares with `timeout-minutes` of the workflow file, read only). The budget end is the runner start plus 35 minutes. The pipeline deadline is the smaller of 30 minutes from its start and the budget end minus 15 minutes (5 scan and upload, 5 save, 5 cleanup). A waiter gives up at the budget end minus 30 minutes, which is 5 minutes after its start (5 restore, a 10-minute minimum pipeline window, the 15-minute reserve). So a run behind another waits at most 5 minutes, then ends with `queue wait expired`.
- Q: What does the queue still guarantee? → A: Runs that share a state never overlap and run in registration order. "Every requested run runs" no longer holds: the concurrency group stays, keeps one running and one pending run per repo, ref and project, and cancels an older pending run when a third arrives. Because the group already serializes those runs, the queue mostly guards the cases the group does not see (a rerun path, a hand-started container, a later removal of the group). It stays because the owner chose to keep 011; it can be dropped with no effect on state and artifacts.
- Q: What do the tick path (`job`, `local`) and `plan` do with the new keys? → A: `job` and `local` run a pipeline that declares `[state]` or `[artifacts]` as before 011: no ticket, no restore, no save, no bundle, no store call. The tick never holds the store settings (its script passes only declared step secrets). A vault declaration is refused there as in `ci` (`vault unavailable`). `plan` is untouched.
- Q: Do the READMEs still change? → A: Yes. The standing owner rule says README, `rust-ci-runner/README.md` and the repository description stay current. The Rust-only rule is read as binding workflows, scripts, YAML and Dockerfile. The report flags this reading.

### Session 2026-11-28 (owner decisions)

- Q: Do runs start on a schedule? → A: Out of scope after the owner rule of 2026-11-28 (no workflow change). Manual dispatch stays the only trigger of `factory-monorepo-ci.yml`. The tick keeps its own outside dispatcher.
- Q: Is GitHub's concurrency group enough to serialize runs of a state? → A: No, it cancels older pending runs. The queue is strict FIFO: runs that share a state never overlap, and each requested run runs, in request order, one after another. The spec fixes the stale-lock rule, the wait budget (no queue cap, no fixed wait cap) and the public line.
- Q: Where does state live? → A: In a release tag of a private store repo that holds no code (named by `FACTORY_CI_STATE_REPO`; see the decisions session below). The runner restores it before the pipeline and saves it after.
- Q: Is state encrypted? → A: Yes. Anyone with read access to the private repo can download release assets, so the runner encrypts state before upload with a key from Bitwarden Secrets Manager. A failed run never overwrites the last good state. The first run, with no state, starts empty.
- Q: Are debug artifacts kept? → A: Yes, encrypted the same way, uploaded to a release on the private repo on failure (and on every run when the pipeline asks). Loki gets the release and asset name, the public log gets nothing. An agent reads Loki with a read token and downloads and decrypts with `gh`. Retention is bounded so the release does not grow forever.
- Q: Which tokens? → A: Two new runner settings read from Bitwarden: `FACTORY_CI_STATE_TOKEN` (fine-grained, contents write, scoped to the store repo only) and `FACTORY_CI_STATE_KEY` (the encryption key). Like the four existing settings, steps may never request them. `FACTORY_CI_TOKEN` stays read-only.

### Session 2026-11-28 (owner decisions, second round)

- Q: Who may touch a Bitwarden credential? → A: The runner only. No AI agent ever sees one, and a pipeline script receives only what it needs.
- Q: How does a step get a website login from the Bitwarden vault (Password Manager)? → A: (Read path out of scope after the owner rule; only the declaration check and `vault unavailable` ship.) The runner holds the vault unlock credentials as runner settings in Bitwarden Secrets Manager, which steps may never request. The pipeline declares which vault login items and which fields a step needs. The runner unlocks the vault, reads only those, passes only those field values as step environment variables, adds them to the Redactor, then locks or logs out and removes the `bw` session and data directory before the step starts. A step never gets a session key or an unlock credential. Declared item names are allowlisted per repo in the watch list, the same way 010 allowlists secret names. Recommended: give the Bitwarden identity access to one collection that holds only those items.
- Q: One encryption key for state and artifacts? → A: No, two. Artifacts use their own key, `FACTORY_CI_ARTIFACT_KEY`, different from `FACTORY_CI_STATE_KEY`. An agent gets the artifact key and a Loki read token and nothing else: never the state key, the state token, the Bitwarden access token or a vault credential. State (cookies) counts as a credential and is never part of a debug artifact. This replaces the earlier planning call of one key.
- Q: What stops a secret from landing in an artifact? → A: Before encrypting and uploading, the runner scans the bundle for every redacted value (exact, base64, each line, and inside zip entries for Playwright traces). A match refuses the upload with a fixed fault and keeps the run failed. Loki output stays redacted as before.
- Q: May the runner's own logs name vault items, fields or values? → A: No. Logs carry positions and counts only, for the vault.

### Session 2026-11-28 (planning calls, no owner answer needed; override any of them)

- Q: Where does the schedule live? → A: Nowhere in 011. The cron lines, the `case` and the entry numbers are out of scope.
- Q: What does "request order" mean? → A: The order in which runs register a queue ticket. The runner learns the state name from the fetched pipeline file, so registration happens after the fetch and the pipeline checks, a few seconds to a minute after the dispatch. Two starts within that gap can swap places.
- Q: Which pipelines queue? → A: Those that declare a `[state]`, keyed by the pipeline repo, project and state name inside the store repo. Pipelines with no state do not queue. The workflow's concurrency group stays, so it still cancels an older pending run when a third arrives.
- Q: Which repos may keep state? → A: Only a watch entry with `state = true`, and only a private repo. The entry is a reviewed file on `main`, so a branch cannot widen it. A public repo cannot opt in, because its release assets would be public.
- Q: Stale lock? → A: Every ticket carries a renewal time that its run refreshes every 60 s while it waits and while it runs. A ticket not renewed for 10 minutes is stale and the next waiter removes it. A run that finds its own ticket gone never writes state.
- Q: Maximum wait and queue size? → A: The runner caps no queue. A run waits until the budget end minus 30 minutes (5 minutes after its start) and then ends with `queue wait expired`, with an alert the owner builds by hand in Grafana. The workflow job limit stays at 45 minutes. A hold is at most 40 minutes: 5 restore, 30 pipeline, 5 save.
- Q: Names of the vault settings? → A: `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD`. The runner hands them to the `bw` child as `BW_CLIENTID`, `BW_CLIENTSECRET` and `BW_PASSWORD`, in that child only. The `FACTORY_CI` prefix is already refused in every `secrets` list, so no new reserved-name rule is needed.
- Q: Where is the vault allowlist? → A: A new watch key `vault_items`, a list of exact item names, default none. A pipeline names the item and the field; the reviewed watch entry decides which items a repo may read. Fields are not allowlisted: the item is the boundary.
- Q: Which fields can a step read? → A: `username`, `password`, `uri` and custom fields, by exact name. The TOTP code and attachments are out of scope.
- Q: Does an artifact refusal fail a passing run? → A: Yes. A secret inside a declared artifact path is a leak in the making, so the run ends red even with `when = "always"` on a pass.
- Q: Retention? → A: State release: the 2 newest assets. Artifact release: the 10 newest bundles per project.
- Q: Are the new Bitwarden keys required? → A: Only when a run needs them. State and artifacts need the state token, the store repo and both keys (the two keys must differ); a vault declaration needs none, because 011 only refuses it. A run that declares none of them needs none, so existing runs keep working before the owner creates the keys.

### Session 2026-11-28 (plan calls that settle gaps found by the 002 plan)

- Q: What does a vault declaration look like in `.factory-ci.toml`? → A: A step key `vault`, a list of inline tables with exactly `item`, `field` and `env`: `vault = [{ item = "playwright-login", field = "username", env = "E2E_USER" }]`. Unknown keys inside an entry are errors. `[state]` and `[artifacts]` stay pipeline-wide.
- Q: Which steps see the state file? → A: Only the one step that declares it. A step key `state = true` marks it; `[state]` requires exactly one such step and no other step may carry the key. The runner decrypts the state into its own memory, writes the file at the declared path in the checkout with mode 0600 immediately before that step starts, and after that step ends (pass or fail) reads the file and removes it from the checkout before any other step starts. Install, lint and test steps, which run third-party code as root, never see it. Every other step must be a transitive dependency or dependent of the state step, so none can run beside it (`[state]` with a step that could run in parallel is `pipeline file refused`). The vault values already reach only the step that declares them (FR-044).
- Q: A step writes the state file as root with mode 0600. Does the save fail? → A: No, and the file is never made world-readable. The declaring step should end with a `chown` of the file to the workspace owner. As a backstop the runner runs one helper container from the runner's own pinned image (`--network none`, no `-e`, an explicit `--entrypoint chown`) that runs `chown -hR` and then `chmod -R u+rX` on the declared state and artifact paths to the workspace owner, never `a+r`, before it reads them. A file the runner still cannot read fails the save with `state save failed`. No world-readable copy of the state exists on the runner or in a bundle.
- Q: Which names may a step set? → A: The names in `env`, `secrets` and vault `env` entries are refused, in any letter case, when they start with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` (010 already refuses the first, second and last for `secrets`; 011 extends all four to `env` and to vault names), or when they equal a runner key.
- Q: May an artifact path be a folder? → A: Yes. A folder is bundled whole, recursively, regular files only (symlinks and special files are skipped, with a Loki line). A path that does not exist is skipped with a Loki line. The state path is a file only.
- Q: How does a step whose vault values are withheld end? → A: It never starts, and neither does any other step. There is no per-step skip: an item the entry does not allow ends the whole run with `[runner] vault item not allowed`, and a vault that cannot be read ends it with `[runner] vault unavailable`, both before any container, as 010 does for `step secret not allowed`. 010 also gates secrets by ref (`WatchEntry::secrets_allowed_on`, line `repository or ref not in the watch list`, whole run). Vault declarations, `[state]` and `[artifacts]` pass the same gate, so a ref the gate refuses never reads the vault, never touches a release and never gets a ticket.
- Q: Does a scan refusal also stop the state save? → A: No. The state save depends only on the pipeline having passed (FR-024, FR-026). A refused artifact fails the run with exit code 1 and leaves the save alone.
- Q: Which files can the scan not read? → A: Besides the list in FR-049, any file that is not text, JSON, zip (stored or deflate), PNG or JPEG. The scan cannot read those, so the upload is refused.
- Q: How does a run name its target? → A: By the dispatch inputs `repo`, `ref` and `project`, as before. There are no scheduled runs.

### Session 2026-11-28 (lead decisions after the devops assessment and the security threat model; they supersede any earlier line that disagrees)

- Q: Where do queue, state and artifacts live? → A: In a separate private repo that holds no code and no workflows (the store repo). Its `owner/name` is a new runner setting `FACTORY_CI_STATE_REPO` read from Bitwarden, so it never appears in this public repo. `FACTORY_CI_STATE_TOKEN` is scoped to that repo only (contents write). The agent's GitHub read token is scoped to that repo, and its Loki token to this tenant. A release in the store repo is still a release tag on a private repo. The pipeline's own repo must still be private and have `state = true` (FR-019).  Release tags are `ci-queue-<owner>_<name>-<project>-<state>`, `ci-state-<owner>_<name>-<project>-<state>` and `ci-artifacts-<owner>_<name>-<project>`, where `<owner>_<name>` is the pipeline repo and `<project>` is `root` for `.` (other dots become `_`). A tag longer than 200 bytes is `pipeline file refused`. The tag is never taken from the pipeline file.
- Q: Ticket names, starters and labels? → A: `t-<runid>-<attempt>-0` (the attempt comes from the runner's own container name, see the owner rule above; the entry is always `0`). A non-`uploaded` asset that matches the ticket regex and is younger than 120 s by `created_at` counts as a live ticket; a non-`uploaded` asset of any other kind is deleted only once it is older than 360 s (the 300 s upload limit plus margin). Only assets whose name matches the ticket regex of contracts/release-store.md count as tickets. A label that is missing, malformed, or has an epoch more than 120 s ahead of the reader's latest `Date`, is stale. The holder re-lists on every renewal and logs an `overlap` line when a lower-id live ticket exists. The release canary (200 trials, two racing runners) is deferred with the pin.
- Q: Duplicate starts? → A: A duplicated dispatch runs twice, one after the other, through the queue (inside the limits of the concurrency group). No marker asset exists.
- Q: Timeouts and limits? → A: Job limit 45 minutes. The runner takes the budget end as its own start plus 35 minutes, unless the environment sets `FACTORY_CI_BUDGET_END` (tests), and waits until the budget end minus 30 minutes. The pipeline deadline is the smaller of 30 minutes from its start and the budget end minus 15 minutes. A 403, 429 or `x-ratelimit-remaining: 0` pauses store calls other than renewals until `retry-after`, `x-ratelimit-reset` or 60 s, at most 120 s; a renewal never pauses, and renewals failing for 600 s end the run with `queue renewal failed`. Mutations are at least 1 s apart. Any 4xx on a create triggers a re-list and a decision. A full list page is followed by pagination, or the run fails closed. The exit-time ticket delete has a 5 s limit and no retry.
- Q: What do the Loki lines look like? → A: They are fixed strings listed in `contracts/loki-lines.md`, each with a test. The owner builds the Grafana alert rules by hand from that list (no-data set to Alerting); no alert file is added to the repo.
- Q: Which keys exist? → A: Thirteen runner keys: the four settings read at start, `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID`, `FACTORY_CI_VAULT_COLLECTIONID`. The vault reads pass `--organizationid` and `--collectionid` and `BW_NOINTERACTION=true`, and a list that returns any item outside the allowlist is `vault unavailable`.
- Q: What does the runner trust on disk and in the store? → A: A declared path is canonicalised and must start with the canonical checkout; it is opened with `O_NOFOLLOW|O_NONBLOCK`; size is judged by bytes read. Before the seal, the runner scans the state plaintext for every known secret and refuses the save (`state save failed`) on a match. Release listings are checked for name pattern and size before a download, and followed by pagination. Hosts are fixed (`api.github.com`, `uploads.github.com`), redirects are off, the download host must be `*.githubusercontent.com` over https.
- Q: What may an artifact contain? → A: Text, JSON, zip (stored or deflate entries), PNG and JPEG. Anything else is refused, and any member named `*.html` is refused (the Playwright HTML report stays off). Every zip member is read, `trace.network` included. Tar member names with `..`, a leading `/` or a control character are refused when building the bundle.
- Q: Which entries may use state or the vault? → A: Only entries with `secret_branches = ["main"]` written explicitly (no pattern), enforced by `plan --check` and the 010 gate. A pipeline with `[state]` may not have a step with `docker = true`. Vault item names in the public watch file are opaque (`ci-login-1`, not the site).
- Q: Which helper image chowns? → A: The runner's own pinned image, found by `docker inspect` of the runner's own container (or `FACTORY_CI_RUNNER_IMAGE` when set), not the first step's image.
- Q: Retired parts? → A: The `due` job, the schedule file, the slot marker, the pinned `due` binary, the queue cap of 3 and the 120-minute wait cap are gone (critic review decisions). The tick workflow `factory-ci-tick.yml` is not retired: it stays as it is (owner rule).
- Q: Rollback order? → A: Revert the Rust commits. No workflow, pin or watch file changed, so nothing else moves. The 0.4.0 image stays in the registry unpinned.

### Session 2026-11-28 (critic review decisions; they supersede any earlier line that disagrees)

- Q: Release tags and asset names? → A: Tags are `ci-queue+<owner>_<name>+<project>+<state>`, `ci-state+<owner>_<name>+<project>+<state>` and `ci-artifacts+<owner>_<name>+<project>`. The delimiter `+` sits outside the alphabets of owner, name, project and state; the root project `.` is written as `-`, which cannot be a folder name; dots in a project are kept. The entry number in an asset name is always the digit `0`, and every asset kind has one regex in contracts/release-store.md: ticket `t-<digits>-<digits>-<digits>`, state `state-<digits>-<digits>-<digits>-<8 hex>.bin`, artifact `artifact-<digits>-<digits>-<digits>-<8 hex>.bin`. A run id that is not digits is replaced by a random decimal number.
- Q: What counts as an asset in flight? → A: A non-`uploaded` asset that matches the ticket regex and is younger than 120 s by `created_at` is a live ticket. A non-`uploaded` asset of any other kind (state, artifact, anything else) is deleted only when it is older than 360 s, so it outlives a 300 s upload. Retention counts only `uploaded` assets, so two failed uploads never push the last good state out.
- Q: Is a request ever dropped? → A: One way only: a run that cannot win a turn before its budget ends with `queue wait expired` and fires an alert. No queue cap, no wait cap. A ticket that was broken and registered again goes to the back; accepted residual.
- Q: Which release carries what? → A: One release, 0.4.0: queue, state, artifacts, all 13 key names reserved, vault declarations parsed and refused with `vault unavailable`. There is no 0.4.1 in 011: the vault read and the `bw` image are out of scope. Logins come from 010 step secrets.
- Q: The `bw` server? → A: Not applicable in 011 (no vault read).
- Q: The signal path? → A: GitHub gives about 10 s after a cancel (7.5 s between SIGINT and SIGTERM, then 2.5 s). The handler deletes the ticket first, within 5 s, and skips the Loki flush; SIGINT and SIGTERM have separate tests.
- Q: Which Environment may start a scheduled run? → A: Not applicable: no scheduled run exists in 011, and the workflow file stays as it is.
- Q: The constitution? → A: 1.2.0 already allows a schedule in `factory-monorepo-ci`; 011 does not use that allowance. No constitution change.
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

### User Story 2 - Runs of one state never overlap and run in order (Priority: P1)

Two runs start together for the same state. Both run, one after the other, in order, as far as GitHub starts both.

**Why this priority**: Two runs that share a state would restore the same cookies and overwrite each other's save, or log in twice from two places.

**Independent Test**: Mock store. Start four `ci` processes for one state. Assert the pipeline windows do not overlap and the order equals the ticket order. A fifth waits as well; only a run that cannot get a turn before its wait limit ends with `queue wait expired`. The test runs the queue on a fake store with test timing, so the 5-minute wait of production does not limit it.

**Acceptance Scenarios**:

1. **Given** run A holds the turn, **When** run B and C register, **Then** B starts after A ends and C after B ends, and each prints `[runner] waiting for the queue` once.
2. **Given** A's runner is killed, **When** 10 minutes pass without renewal, **Then** B removes A's ticket and starts.
3. **Given** the turns ahead of a waiter use up its wait limit (the budget end minus 30 minutes), **When** that point passes, **Then** it removes its own ticket, prints `[runner] queue wait expired`, exits 1 and starts no step. This is the one drop the runner makes; GitHub's concurrency group, which stays, can also cancel a pending run before the runner starts. The owner's Grafana alert on this line is built by hand.

---

### User Story 3 - The schedule starts the pipelines (out of scope)

Dropped by the owner rule of 2026-11-28. A `cron` line, its `case` and the job name need edits to `factory-monorepo-ci.yml` and `test-factory-monorepo-ci.sh`, which may not change. The tick keeps its outside dispatcher and stays as it is. A later feature may bring the schedule back when the rule is lifted; the `<entry>` field of the asset names is reserved for it.

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

### User Story 6 - A step gets a website login and never a vault credential (out of scope, only the refusal ships)

The vault read needs `bw` in the runner image (a `Dockerfile` change) and a smoke step in `rust-ci-runner-release.yml`, and the owner rule allows neither. What ships: a `vault` declaration is parsed and validated (FR-042), the entry's `vault_items` allowlist is checked (FR-043), and a validated declaration ends the run with `[runner] vault unavailable` before any ticket or container (FR-045). Logins come from 010 step secrets.

**Independent Test**: A fixture pipeline with a validated `vault` entry ends `[runner] vault unavailable` with no container and no ticket. An item the entry does not allow ends `[runner] vault item not allowed` first.

**Acceptance Scenarios**:

1. **Given** a pipeline names an item that the entry does not allow, **When** `ci` reads the pipeline, **Then** the run ends with `[runner] vault item not allowed` and no container starts.
2. **Given** an allowed item in a valid declaration, **When** `ci` reads the pipeline, **Then** the run ends with `[runner] vault unavailable`, registers no ticket and starts no container.

---

### User Story 7 - An artifact never carries a secret to an agent (Priority: P1)

A failed trace holds the typed password in `trace.network` inside `trace.zip`. The runner reads every zip member, including that one, finds the password before encryption and uploads nothing.

**Why this priority**: The artifact key goes to an agent. Anything inside the bundle, the agent can read.

**Independent Test**: A fixture that writes a zip whose inner entry, named `trace.network`, holds a canary in plain, in base64 and as one line of a multi-line value, and a state-file value of the FR-050 kind. Run each. No upload happens, the run fails, and clean content uploads. A bundle with an `.html` member is refused. A real Playwright login-failure trace from the 002 fixture passes the scan once and is refused when the typed password is seeded as a known value (an owner canary, deferred with the pin). A trace taken after a login with restored state is refused unless the 002 scrub removed the cookies; that refusal is by design and the README says so. A cookie set during the run that shows up in a trace is refused (FR-050). A bundle that lists the state file path is refused at the pipeline check.

**Acceptance Scenarios**:

1. **Given** a declared artifact holds a known secret value in a zip entry, **When** the run ends, **Then** nothing is uploaded, stderr shows `[runner] artifact upload refused` and the exit code is 1.
2. **Given** an artifact with no known value in it, **When** the run ends, **Then** it is encrypted with the artifact key and uploaded, and the state key cannot open it.
3. **Given** an artifact path that is, contains or lies under the state path, **When** the pipeline is read, **Then** the run ends with `[runner] pipeline file refused`.

### Edge Cases

Answers to the edge-case checklist. "Handled" names the requirement.

- Inputs. State or artifact path absolute, with `..`, through a symlink, or outside the project: pipeline refused (FR-018). Two `[state]` tables: TOML error, refused. State name with `/`, uppercase, Unicode, NUL, over 64 characters: refused (FR-018). State file empty, over 10 MB, or missing after a pass: no save, `state save failed` for the oversized one, a Loki line for the missing one (FR-024). Artifact path that matches nothing: skipped, a Loki line. Bundle over 100 MB: not uploaded, a Loki line (FR-032). A project name with `..`, ending in `.` or `.lock`, used with `[state]` or `[artifacts]`: pipeline refused (the release tag would be invalid).
- State. First run with no state starts empty (FR-021). A crash between upload and delete leaves two assets, and the newest wins (FR-025). A truncated or tampered asset, or the wrong key: restore fails and the run stops, with state untouched (FR-022). Older asset format: no older version exists. A retry is safe: ticket and asset names carry the run id, so a rerun of the same run id replaces its own ticket and saves one new asset.
- Partial failure. Restore fails: no step starts, the ticket is released. Pipeline fails: state kept, artifacts uploaded, ticket released. Save fails after a pass: the run fails, the ticket is released, the old state stays (FR-026). Artifact upload fails: Loki line, exit code unchanged (FR-031). Release of the ticket fails: the stale rule frees it within 10 minutes (FR-014).
- Time and concurrency. Two runs register together: the store decides the order, and the order is stable (FR-010). Every GitHub call has a time limit and one retry on no answer or 5xx; a 403 or 429 pauses all calls (FR-036). Clock skew between runners: ticket age comes from the GitHub server time, never the runner clock (FR-013). A cancelled pending run (the concurrency group stays): not recoverable by the runner (R4). A job that hits the 45-minute limit: the budget end sits 10 minutes before it, so the runner ends first.
- Resources. Waiters poll every 20 s, so each waiter costs about 3 calls a minute against the token's 5,000 an hour. Releases hold at most 2 state assets and 10 artifacts per project. A waiting job holds one hosted VM for at most 5 minutes of waiting (R7).
- Trust. Who can name a state: the pipeline author, inside the repo's own releases, only when the reviewed watch entry says `state = true` with `secret_branches = ["main"]` and the repo is private, in a store repo that holds no code. Who can reach the token: the runner process only. What the pipeline author cannot do: write another repo's release, read a setting or a vault credential, widen an entry, choose a release tag outside the fixed naming (FR-019, FR-034, FR-042). What the agent cannot do: open state, because it holds neither the state key nor the token (FR-027, FR-048).
- Vault and scan inputs. Any validated vault declaration: `vault unavailable`, no container (FR-045); the read cases (zero or two logins, empty field, locked identity) are out of scope with the read. Two steps declaring the same environment variable for different fields, or one that also appears in `env` or `secrets`: pipeline refused (FR-042). An item name with Unicode, spaces or `=`: allowed, matched by exact bytes, and passed only through the environment, never argv. A zip with an encrypted entry, nesting deeper than 2, an entry count over 10,000 or more than 500 MB uncompressed: not scannable, so no upload (FR-049). A value split across two lines or transformed beyond base64 is not found (R2, R11).
- Vault resources and failure. None: the runner never calls `bw` in 011.
- UI. No UI. The surface is the fixed stdout and stderr lines (FR-037).

## Requirements *(mandatory)*

Each requirement is an acceptance criterion in the form WHEN trigger THE SYSTEM SHALL response. Tags: `[error]` is a failure path, `[boundary]` is an edge of an allowed range. Each maps to at least one unit test under `cargo test --lib` (Principle V); file-format checks run in the `rust-ci-runner` pipeline. No workflow or script check changes.

### Functional Requirements

**Schedule (out of scope)**

- **FR-001 to FR-004**: dropped by the owner rule. They described `cron` lines, a `case` on `github.event.schedule`, a limit of 3 lines and a shell test of the workflow. Every one needs an edit to `factory-monorepo-ci.yml` or `test-factory-monorepo-ci.sh`. The IDs stay unused so later references hold.
- **FR-005**: WHEN a run starts by manual dispatch THE SYSTEM SHALL behave as before, with the same inputs and the same command line. `ci` takes no `--entry` flag; every ticket and asset name carries the entry `0`.

**Queue**

- **FR-006**: WHEN a pipeline declares a `[state]` and passes the file, docker and secret checks THE SYSTEM SHALL register one ticket in the queue of that state in the store repo, before restoring state or starting any container. A pipeline with no `[state]` registers nothing.
- **FR-007**: WHEN a ticket is first in line THE SYSTEM SHALL let its run restore, run and save. WHEN it is not THE SYSTEM SHALL start no container and touch no state, poll every 20 s, and print `[runner] waiting for the queue` once to stdout.
- **FR-008**: WHEN several tickets exist THE SYSTEM SHALL run them in registration order, one at a time. The turn passes only after the previous run released its ticket or its ticket went stale. No ticket is skipped, reordered or dropped except by FR-011, FR-012 and FR-014.
- **FR-009**: WHEN a run ends, whatever the result (pass, fail, refusal after registering, panic, SIGTERM, SIGINT) THE SYSTEM SHALL remove its ticket before exit. A cancelled GitHub job sends SIGTERM and the runner gets its grace period to do so.
- **FR-010**: WHEN two runs register at the same moment THE SYSTEM SHALL give them one stable order decided by the store, the same for both.
- **FR-011**: THE SYSTEM SHALL NOT cap the number of tickets of a state; a run that registers waits for its turn. A broken ticket that is deleted and registered again goes to the back; that is an accepted residual. The runner cannot recover a run that GitHub's concurrency group (kept) cancels before the runner starts.
- **FR-012** `[error]`: WHEN a run has not won its turn by the budget end minus 30 minutes (5 restore, 10 minimum pipeline window, 15 reserve) THE SYSTEM SHALL remove its ticket, print `[runner] queue wait expired`, exit 1 and start no container. The budget end is the runner start plus 35 minutes unless the environment sets `FACTORY_CI_BUDGET_END` (tests); the job limit is 45 minutes and equals `timeout-minutes` of the workflow (a test reads the file). `[boundary]` One second before the point the run keeps waiting.
- **FR-013**: WHEN a run keeps a ticket THE SYSTEM SHALL renew it every 60 s, waiting or running, using the store's server time to judge age, never the runner clock.
- **FR-014** `[error]`: WHEN a ticket has gone 10 minutes without renewal THE SYSTEM SHALL treat it as stale, let the next waiter remove it and log the removal to Loki as step `ci`. `[boundary]` 9 minutes 59 seconds is not stale, 10 minutes is. A holder whose renewals have failed for 600 s stops its run (FR-054).
- **FR-015** `[error]`: WHEN a run finds its own ticket gone before it saves state THE SYSTEM SHALL write nothing, end the run with exit code 1 and `[runner] state save failed`, and log the cause to Loki.
- **FR-016**: WHEN waiting runs show anything publicly THE SYSTEM SHALL show only the line of FR-007. Positions, ticket names, ages, state names and repo names go to Loki as step `ci` only.

**Declaring state and artifacts**

- **FR-017**: THE SYSTEM SHALL accept two optional tables in `.factory-ci.toml`: `[state]` with `name` and `path`, and `[artifacts]` with `paths` (a list) and `when` (`"failure"` or `"always"`, default `"failure"`), and the step key `state = true`, carried by exactly one step when `[state]` exists and by none otherwise. Unknown keys stay errors.
- **FR-018** `[error]`: WHEN `name` is not 1 to 64 characters of `[A-Za-z0-9_-]`, or `path` or an artifact path is empty, absolute, contains `..`, a NUL or a newline, or resolves through a symlink or outside the project folder, or a list holds more than 20 paths, or an artifact path equals, contains or lies under the state path, or `[state]` has no step with `state = true` or more than one, or another step is neither a transitive dependency nor a dependent of the state step (it could run beside it), or a name in `env`, `secrets` or a vault `env` starts, in any letter case, with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` THE SYSTEM SHALL end the run with `[runner] pipeline file refused` and start no container. `[boundary]` A 64-character name and 20 paths are accepted, 65 and 21 are not. A sibling path that only shares a prefix with the state path (`.ci-state` and `.ci-state-2`) is accepted.
- **FR-019** `[error]`: WHEN a pipeline declares `[state]` or `[artifacts]` and the repo's watch entry lacks `state = true`, or the repo is public (judged from the identity check the runner already makes) THE SYSTEM SHALL end the run with `[runner] state not allowed`, register no ticket and start no container. The release a run writes to is fixed by the repo's own name and the state name or project, never taken from the file. `plan --check` accepts `state` only with a numeric `id`.

**State**

- **FR-020** `[error]`: WHEN a pipeline declares `[state]` and `FACTORY_CI_STATE_TOKEN` or `FACTORY_CI_STATE_KEY` is missing from Bitwarden (zero matches), or declares `[artifacts]` and `FACTORY_CI_STATE_TOKEN` or `FACTORY_CI_ARTIFACT_KEY` is missing, THE SYSTEM SHALL end the run with `[runner] state store not configured` before registering a ticket. Two matches, an empty value, a value with CR, LF or NUL, leading or trailing whitespace, or under 4 characters, are refused as for the four existing settings. A pipeline with `[state]` or `[artifacts]` also needs `FACTORY_CI_STATE_REPO` (the store repo, `owner/name`), and both keys must exist and differ, whichever of the two the pipeline uses: a missing key or equal values end the run the same way. A key (`FACTORY_CI_STATE_KEY` or `FACTORY_CI_ARTIFACT_KEY`) must be standard base64 or hex text that decodes to 32 bytes or more, with at least 8 distinct byte values in the decoded bytes. `[boundary]` A key that decodes to 32 bytes is accepted and 31 are refused. `FACTORY_CI_STATE_REPO` must have the shape `owner/name` (the 010 repo-name rule); anything else is `state store not configured`. A store 5xx during registration is retried inside the wait budget and does not print that line.
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

- **FR-042** `[error]`: WHEN a step declares a vault login THE SYSTEM SHALL accept only `vault = [{ item = "...", field = "...", env = "..." }]` on the step, entries with exactly those three string keys (item name, field name and environment variable name), and SHALL end the run with `[runner] pipeline file refused` when the environment variable name breaks the 010 name rules, starts with `FACTORY_CI`, `BW_` or `BITWARDENCLI`, repeats in the step (across `env`, `secrets` and vault entries), or the step has `docker = true`. `[boundary]` A step may declare 20 fields and the pipeline 50; 21 and 51 are refused. The declaration is only checked: nothing reads a vault (FR-045).
- **FR-043** `[error]`: WHEN any step names a vault item that is not listed in `vault_items` of the run's watch entry (including when the entry has no such key) THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] vault item not allowed`, unlock no vault, start no container and read no value. Items match by exact name. There is no per-step skip: the whole run ends. `[boundary]` An item that differs only by case is not allowed. `plan --check` reports a `vault_items` entry that is empty, longer than 100 characters, or holds a control character, and refuses the file whole.
- **FR-044**: out of scope (the vault read needs `bw` in the image). Dropped by the owner rule.
- **FR-045** `[error]`: WHEN a pipeline holds a vault declaration that passed FR-042 and FR-043 THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] vault unavailable`, register no ticket and start no container. This is the whole vault behaviour of 011; it holds in `ci`, `job` and `local`.
- **FR-046**: out of scope with the vault read. Dropped by the owner rule.
- **FR-047**: out of scope with the vault read (no vault value exists to redact). Dropped by the owner rule.
- **FR-048**: THE SYSTEM SHALL keep these out of reach of any agent and any step: the vault credentials, `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY` and the Bitwarden access token. An agent's whole kit is `FACTORY_CI_ARTIFACT_KEY`, a Loki read token and a read-only GitHub token; no artifact, Loki line or public line may carry anything else.

**Artifact scan**

- **FR-049** `[error]`: WHEN a bundle is ready and before it is encrypted THE SYSTEM SHALL scan every file in it for each value the Redactor holds (the runner settings, step secrets, vault values, the Bitwarden access token, the FR-050 state strings, but not the numeric `FACTORY_CI_LOG_USER`, which is an identifier and matches digits by chance), each in its exact form, in the forms the Redactor knows (`Log::secret_forms`), and as each line of 4 or more characters of a multi-line value, and SHALL scan the contents of every zip entry, `trace.network` included, to a nesting depth of 2. WHEN a value matches, or a file cannot be scanned (an encrypted entry, depth over 2, more than 10,000 entries, more than 500 MB uncompressed, an unreadable file, a file that is not text, JSON, a zip with stored or deflate entries, PNG or JPEG, a member named `*.html`) THE SYSTEM SHALL upload nothing, print exactly `[runner] artifact upload refused`, end the run with exit code 1 and log to Loki only a position and a count, never the value, the matching file name or an offset. `[boundary]` A value of exactly 4 characters on one line is scanned for; a nested zip at depth 2 is scanned and one at depth 3 refuses the upload.
- **FR-050**: WHEN the state file is JSON THE SYSTEM SHALL add each string it holds under a key named `value` of 16 or more characters (the cookie and local storage values of a Playwright storageState) to the Redactor and to the scan set, and WHEN it is not JSON THE SYSTEM SHALL add its whole content when that is under 1 MB. It does this twice: for the restored bytes before the state step starts, and for the bytes the `after` hook reads when the state step ends, before any artifact is built, so a cookie issued during the run is masked and scanned too. `[boundary]` A 16-character cookie value is added, a 15-character one is not.

**Settings and containment**

- **FR-034** `[error]`: WHEN a watch entry lists any of `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID` or `FACTORY_CI_VAULT_COLLECTIONID` in `secrets` THE SYSTEM SHALL refuse the whole watch file. WHEN a pipeline lists any of them in `secrets`, or as a vault environment variable name, THE SYSTEM SHALL end the run with `[runner] pipeline file refused`. None of the nine values reaches a step, a step container argument, an engine child other than the one call that needs it, or a log body. The test list of 010 FR-004 grows from four names to thirteen, and 011 supersedes that list. Every name in a `secrets` list and every vault `env` name must be disjoint across the whole pipeline; a name in both is `pipeline file refused`.
- **FR-035**: THE SYSTEM SHALL mask all nine new values, their base64 forms and the base64 of `x-access-token:<token>` in every output path, as it does for the four settings, before any line that could hold them is written.
- **FR-036**: WHEN the runner calls the GitHub API for the queue, state or artifacts THE SYSTEM SHALL use `FACTORY_CI_STATE_TOKEN` with a time limit, one retry on no answer or 5xx and a pause on 403 or 429, and never use `FACTORY_CI_TOKEN` for a write or `FACTORY_CI_STATE_TOKEN` for a clone. The token never appears in argv, a file or a git config. The runner makes no `bw` call.

**Public output**

- **FR-037**: WHEN the run prints THE SYSTEM SHALL keep stdout to the closed grammar of 008, extended by exactly one line, `[runner] waiting for the queue`, and stderr to the closed fault list, extended by exactly these lines: `queue wait expired`, `queue renewal failed`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`. No repo, branch, project, state, release, asset, vault item, vault field or secret name, and no count, position or age, appears in any public line. The lines join the closed list in `log.rs` and its test.
- **FR-061**: BEFORE any line that can carry a value and before any store call THE SYSTEM SHALL call `log.add_secrets` (the lines of steps 1 to 5 of the run order are fixed text and carry none) with the state token, the state key, the artifact key, the store repo name, `base64("x-access-token:" + state token)`, so no later line, Loki body, error text or asset can hold them. A test seeds each, plus the `x-access-token:` form, as canaries.

**Rollout**

- **FR-038**: THE SYSTEM SHALL release once, as 0.4.0, which carries the queue, state and artifacts, reserves all 13 runner key names, parses and validates the vault declarations and `vault_items`, and ends any validated vault declaration with `[runner] vault unavailable` before it registers a ticket or starts a container. `Cargo.toml` and `Cargo.lock` agree, and the tag `rust-ci-runner-v0.4.0` goes on a commit on `main`; the existing release workflow builds the image and its smoke step passes. `RUNNER_IMAGE` in `factory-monorepo-ci.yml` does not change (owner rule): the image is released and not pinned, and the owner moves the pin when the rule is lifted. No 0.4.1 exists in 011.
- **FR-039**: THE SYSTEM SHALL NOT change `factory-monorepo-ci.yml`, `factory-ci-tick.yml`, any other file under `.github/workflows/`, or any file under `.github/scripts/`. A Rust test reads `factory-monorepo-ci.yml` read-only and asserts that `timeout-minutes` equals the job-limit constant, so a later workflow edit shows up as a red test.
- **FR-040**: out of scope. Adding `state`, `secret_branches` and `vault_items` to `.github/factory-ci-repos.toml` would make every pinned runner that still reads that file refuse it: the `ci` runner pinned in the workflow (which cannot move) and the tick's `plan` (the runner of the `RUNNER_IMAGE_DIGEST` variable) until each runs 0.4.0. The keys, the first cron line and the file header wait until the pin moves.
- **FR-041**: THE SYSTEM SHALL update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description to match: the nine new settings and the Bitwarden project guidance ("holds only the thirteen keys"), the store repo, the `ci` step list, the fault list, the `decrypt-artifact` command and the agent kit, the vault declaration and its refusal, the queue behaviour (5-minute wait, the one drop, the concurrency group that stays), a runbook for `state restore failed` (delete the newest state asset, or all of them to start empty), a release checklist line (image digest), what 011 leaves out and why (the schedule, the vault read, the pin), and the risks. The tick and its settings stay described as they are. This is the last task in `tasks.md`.

### Key Entities

- **Cron entry**: out of scope; the entry number is the constant `0` in asset names.
- **Watch entry**: gains `state` (default false) and `vault_items` (default none). Reviewed on `main` of B.
- **Vault login**: a step's declaration of one item, one field and the environment variable that would receive it. In 011 it is checked and refused (FR-042, FR-045); no value is read.
- **State**: one named file of a pipeline, stored encrypted in a release of the owning private repo, newest 2 kept.
- **Ticket**: a queue place of one run for one state, with a registration order and a renewal time. No cap per state.
- **Artifact bundle**: one encrypted archive of a run's declared paths, in the project's artifact release, newest 10 kept.
- **Runner settings**: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`, now also `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID` and `FACTORY_CI_VAULT_COLLECTIONID` (thirteen in all, all held from the one `bws` call). Read by the runner, never handed to a step or an agent. The five vault names are reserved only; nothing reads them in 011.
- **Agent kit**: the artifact key, a Loki read token and a read-only GitHub token. Nothing else.
- **Public line**: a stdout or stderr line from the closed grammar, now with one more stdout line and nine more fault lines.

## Success Criteria *(mandatory)*

### Hypothesis

If the runner keeps the browser session between runs and serializes the runs that share it, the Playwright project can log in once and not on every run, and an agent can diagnose a failure without the owner copying anything. We would be wrong if sessions expire faster than the runs come, if waiters time out, or if agents still ask the owner for more than the artifact. The first two cannot be measured until the pin moves (out of scope).

### Measurable Outcomes

- **SC-001**: deferred. It needs a real schedule and the pin, both out of scope. The test-side equivalent is the three-run counter of the Independent Test of User Story 1 (the step sees 0, 1, 2) and the four-run no-overlap test of User Story 2.
- **SC-002**: In the queue tests over the fake store, zero registered runs of one state are lost to anything other than `queue wait expired` and `queue renewal failed`, and no two overlap. Production loss by the concurrency group is out of scope. Measured by `queue_tests`.
- **SC-003**: deferred with the pin. The test-side equivalent: a failed fixture run uploads one asset, Loki names it, and `decrypt-artifact` with the artifact key returns the file (User Story 4).
- **SC-004**: Across a hostile fixture (canary state, token and key, artifact with a canary inside, step listing the settings), zero lines of stdout, stderr, child argv and Loki bodies hold a canary, no release asset holds a plaintext canary, and zero public lines hold a repo, state, release or asset name. Measured by the unit and fixture tests.
- **SC-005**: A failed run never changes the stored state: a test that fails the pipeline after a write to the state file finds the newest asset byte-identical to before.
- **SC-006**: `cargo test --lib` passes in the `rust-ci-runner` pipeline with tests for the in-scope requirements, and the release publishes a 0.4.0 digest. The workflow pin does not move (FR-038).
- **SC-007**: out of scope with the vault read.
- **SC-008**: Across a hostile artifact fixture (each known value in plain, base64, one line of a multi-line value, inside a zip and inside a nested zip), 100% of bundles are refused with no upload, and a clean bundle uploads. The state key opens no artifact and the artifact key opens no state asset. Measured by the unit tests.
- **SC-009**: deferred with the pin. The agent kit is documented (FR-033, FR-048) and the owner reviews it at the first real use.

## Out of scope after the owner rule

The rule of 2026-11-28 allows only Rust in `rust-ci-runner/`, `.factory-ci.toml` and other toml files to change. Each row is a 011 goal that needs a file outside that set, so it is not done and no workaround stands in for it.

| Goal | What it needed | Why it is out |
|---|---|---|
| Schedule: US3, FR-001 to FR-004, `--entry` | `schedule:` and a `case` in `factory-monorepo-ci.yml`, a rewrite of `test-factory-monorepo-ci.sh` | workflow and script edits |
| Workflow changes, FR-039 | drop `concurrency`, `timeout-minutes` 210, `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_RUNNER_IMAGE`, an Environment | workflow edit |
| Pin to 0.4.0, quickstart sections 2 and 3, SC-001, SC-003, SC-009 as measured outcomes | the `RUNNER_IMAGE` line of `factory-monorepo-ci.yml` | workflow edit; without the pin no real run uses 0.4.0 |
| Watch keys and first cron line, FR-040 | `state`, `secret_branches` in `.github/factory-ci-repos.toml` | toml is allowed, but the pinned `ci` runner and the tick's `plan` refuse unknown keys until they run 0.4.0, so adding them breaks both |
| Every requested run runs | removing the concurrency group | the group stays and cancels an older pending run |
| Waits past 5 minutes | a 210-minute job | the 45-minute limit stays |
| Vault read, US6, FR-044, FR-046, FR-047, FR-056, SC-007, 0.4.1 | Node and `bw` in `rust-ci-runner/Dockerfile`, a smoke step in `rust-ci-runner-release.yml` | Dockerfile and workflow edit |
| Alert-rule file, FR-057 (file part) | `rust-ci-runner/alerts/factory-ci.yaml` | YAML file; the owner builds the rules in Grafana |
| Agent script `open-artifact.sh`, FR-033 (script part) | a new shell script | shell script; `decrypt-artifact` and the README command remain |
| Shell-test updates | `test-factory-monorepo-ci.sh`, `factory-ci-lint-workflow.sh` | script edits; nothing changes in the workflow, so nothing needs a new check |
| Tick retirement, old FR-060 | delete `factory-ci-tick.yml`, edit the selftest workflow and scripts | reversed: the tick stays |
| Environment decision (D18) | an `environment:` expression in the workflow | workflow edit |

## Assumptions

- The owner creates the store repo and a fine-grained token scoped to it (contents write), the two keys (each `openssl rand -base64 33`, decoding to 32 bytes or more, different from each other), and the Bitwarden Secrets Manager entries for the four state settings. The vault settings stay unset. No code or doc holds a value. Anyone with write access to the Bitwarden project can read them all: that is already a trust root (008).
- The vault credentials and the collection-limited account belong to the deferred vault read (R12).
- The `bw` command line client is not in the runner image and 011 does not add it (a `Dockerfile` change is out of scope).
- The agent holds its own GitHub credentials for `gh`. The owner gives it a token with contents read on the one private repo. That token can download the encrypted state asset, which is useless without the state key.
- GitHub's store gives an atomic create that fails when the name exists (a ref or a release tag), server timestamps and release asset upload and delete. The plan picks the mechanism for tickets and renewals; this spec fixes only the behaviour. A queue operation that cannot be made atomic on GitHub is a blocker for the plan, not a reason to loosen FR-008.
- The runner has no encryption dependency today (`Cargo.toml` lists `getrandom`, `serde`, `serde_json`, `toml`, `ureq`). The plan chooses an authenticated cipher that the owner or an agent can open with a documented command, and justifies any new crate or image tool in Complexity Tracking.
- The state file is small (cookies, tens of KB). A directory as state, and a file over 10 MB, are out of scope.
- The step container runs as root. The state file is written at mode 0600 and the runner reads it after a root helper container chowns the declared paths to the workspace owner (Clarifications, plan calls), so a step may keep 0600 and no copy is world-readable.
- No scheduled workflow exists in 011. The tick keeps its outside dispatcher.
- The pin of `factory-monorepo-ci.yml` stays on the image it names today. Anything released in 011 is inert until the owner moves it.
- `sudtanj/infrastructure` and any public repo get no `state = true`.
- Existing entries and pipelines with no `[state]` and no `[artifacts]` behave as before. The concurrency group, which stays, still serializes them (R4).

## Non-goals

- Queues for stateless pipelines, priorities, cancelling a queued run, and fair scheduling across states.
- A schedule of any kind (out of scope).
- State as a directory, state shared between repos, state versions beyond the newest 2, rollback to an older state, and any merge of two states.
- Key rotation and re-encryption of old assets.
- Rewriting or masking text inside a bundle. The scan only refuses (FR-049).
- Scanning values the runner does not know: a secret that never passed through Bitwarden, a transformed value, or one split across lines (R2, R11).
- The vault TOTP code, attachments, secure notes beyond named fields, writing to the vault, more than one vault identity, and any per-field allowlist.
- Managing the agent's credentials, or checking what an agent was given. The spec fixes the kit and documents it (FR-033, FR-048).
- Streaming artifacts to Loki, viewing them in a browser, and any download tooling beyond the `decrypt-artifact` command and one documented `gh` command.
- Docker steps with state, `--allow-docker`, and any change to the tick workflow and its scripts.
- Any edit of a workflow, a script, a Dockerfile or a YAML file (the owner rule), and with it the schedule, the vault read, the pin bump, the alert-rule file and the agent script.
- A cap on step output, and any change to how the four existing settings are read beyond adding the others.

## Risks

- **R1, a write token on the runner**: `FACTORY_CI_STATE_TOKEN` can rewrite releases of every repo it covers. A pipeline cannot read it (FR-034), but a bug in the runner could leak it. Mitigation: fine-grained, only private repos with `state = true`, 90-day expiry like the others, masked, and never in a child environment.
- **R2, the queue is a lock on a public API**: a ticket that cannot be removed (token expired, API down) blocks the state until staleness frees it, and a stale break of a run that is alive but cut off from GitHub lets two runs overlap. Mitigation: 10-minute stale rule, the fence of FR-015 so the loser never writes state, and Loki lines for every break.
- **R3, an artifact holds secrets**: a Playwright trace records typed input and network traffic, so it can hold the login and cookies. Encryption protects the release from readers of the repo, not from whoever holds the artifact key, and the agent holds it. Mitigation: the scan of FR-049 reads every member including `trace.network` and refuses a bundle that holds any value the runner knows (the FR-050 state strings included); `.html` members and unreadable formats are refused; the artifact key differs from the state key; the state never enters a bundle. 002 keeps the HTML reporter off and scrubs its trace. A canary on a real failure trace runs before the first real run. The scan cannot see a value it does not know (R11).
- **R4, the concurrency group stays**: GitHub keeps one running and one pending run per repo, ref and project and cancels an older pending run when a third arrives, so a requested run can be lost before the runner starts. Mitigation: documented in the README. The queue behind the group adds no recovery for that case.
- **R5, 011 ships inert**: the image is released and not pinned (the pin is a workflow line), the watch file cannot carry `state` (the pinned runner and the tick's `plan` refuse unknown keys), and so no real run uses the queue or state. Mitigation: the report and README say so; the owner lifts the rule or moves the pin by hand. The risk is wasted work if the rule stays.
- **R6, the runner finds its own attempt and image by `docker inspect`**: it relies on the container name set by the unchanged workflow and on the docker socket mount. A renamed container or a missing socket stops a stateful run (`runner fault`, no ticket). Mitigation: a test of the name parser and a test of the failure, and the Rust test of the job limit that watches the same file.
- **R7, waits are short**: a waiting job holds a runner for at most 5 minutes before `queue wait expired`; a run behind a long holder is dropped. Mitigation: the queue guards only what the concurrency group does not already serialize, and the drop is an alert built by hand.
- **R8, request order is approximate**: registration happens after the fetch, so near-simultaneous starts can swap (Clarifications). Mitigation: accepted. Strict order holds from registration on.
- **R9, rollout order**: the tag needs no workflow change and nothing pins it. When the owner later moves the pin, the order is: release, then pin, then Bitwarden keys and the watch entry (the pinned runner refuses unknown keys before the pin moves), then the pipeline. A `[state]` pipeline before `state = true`, the token and the key exist fails with a fixed line.
- **R10, one state per name per repo**: two pipelines of one repo that pick one state name share one queue and one file. Mitigation: pick distinct names.
- **R11, the scan is exact**: it finds known values in plain, in the base64 forms the Redactor knows and by line. A value a page renders differently (URL-encoded, split in a screenshot as pixels) passes. A screenshot of a page that shows a secret is an image, and no scan reads it. Mitigation: keep `when = "failure"`, declare only the paths that are needed, and treat artifacts as sensitive. A false positive (a 4-character value that matches ordinary text) blocks an upload and fails the run; the 4-character floor is the same as for masking.
- **R12, the vault credentials are a master key** (deferred with the vault read): the five names are reserved and nothing reads them.
- **R13, allowlist is by item, not field** (deferred with the vault read).
- **R14, a vault read takes the turn** (deferred with the vault read).
- **R16, a reviewed step can still print a transformed secret**: the redactor masks exact, base64, URL-encoded, JSON-escaped, hex and per-line forms. A step on reviewed `main` code that prints a value in another form (reversed, split, XOR-ed) is not masked. Accepted: the pipeline file and its steps are reviewed, `secret_branches` is `["main"]`, and the owner is told. A late winner also has a minimum pipeline window of 10 minutes (FR-052).

## Requirements from the lead decisions (FR-051 to FR-059)

These formalise the decisions session so each has an identifier and a test. They supersede any older line that disagrees.

- **FR-051**: THE SYSTEM SHALL keep queue, state and artifacts in the store repo named by `FACTORY_CI_STATE_REPO`, use a token scoped to that repo only, require `private: true` from `GET /repos/<store>` before any ticket, fix the hosts to `api.github.com` and `uploads.github.com`, turn redirects off, and accept a download `Location` only over https on `*.githubusercontent.com`. The pipeline file never sets a tag or a repo.
- **FR-052**: WHEN a run waits THE SYSTEM SHALL cap the wait only by the limit of FR-012, and SHALL compute the pipeline deadline as the smaller of 30 minutes from the pipeline start and `budget end` minus 15 minutes (5 for the artifact scan and upload, 5 for the save, 5 for cleanup). The wait ends at `budget end` minus 30 minutes, so the latest winner has 25 minutes before the 15-minute reserve; after restore (5) that leaves a minimum pipeline window of 10 minutes (documented in the README). The budget end is read through `Seams.timing`, not straight from the environment.
- **FR-053** `[error]`: WHEN the runner lists tickets THE SYSTEM SHALL count only assets whose name matches the ticket pattern of contracts/release-store.md, count a non-`uploaded` asset that matches the ticket regex and is younger than 120 s by `created_at` as a live ticket, delete a non-`uploaded` asset of any other kind only once it is older than 360 s, treat a ticket whose label is missing, malformed or more than 120 s ahead of the reader's latest `Date` as stale, and log `overlap` at a renewal that sees a live ticket with a lower id. Retention counts only `uploaded` assets. `[boundary]` A label epoch 120 s ahead is accepted, 121 s is stale; a ticket-pattern asset 119 s old is live, 120 s is not; another kind at 360 s is kept, at 361 s deleted.
- **FR-054**: WHEN the runner calls the store THE SYSTEM SHALL pause all calls except renewals on a 403, a 429 or `x-ratelimit-remaining: 0` until `retry-after`, `x-ratelimit-reset` or 60 s, never longer than 120 s, space mutations 1 s apart, treat any 4xx on a create as re-list and decide, retry a 5xx on registration inside the wait budget, delete a half-created own-name asset after a timed-out upload and retry once, follow pagination and fail closed above 300 assets, check asset name and size before a download, retry a restore once on a 404, and give the exit-time ticket delete 5 s and no retry. A renewal never pauses: WHEN renewals have failed for 600 s THE SYSTEM SHALL set a cancel flag, kill the run's step containers by their container-name prefix (`docker kill`), wait for them to exit, and only then delete the ticket, print `[runner] queue renewal failed` and exit 1. A burst of renewals at the secondary rate limit can trigger it; an alert covers it.
- **FR-060**: THE SYSTEM SHALL leave `.github/workflows/factory-ci-tick.yml`, its scripts, its settings and the `plan` subcommand untouched. The outside scheduler keeps dispatching the tick; feature 012 relies on it. (The earlier FR-060 deleted the tick; the owner rule reverses it.)
- **FR-055** `[error]`: WHEN the runner reads a declared state or artifact path THE SYSTEM SHALL canonicalise it, require it to start with the canonical checkout, open it with `O_NOFOLLOW|O_NONBLOCK`, judge size by bytes read, and scan the state plaintext for every known secret form before sealing; a hit is `[runner] state save failed`. The state step runs as root; the README says so.
- **FR-056**: out of scope with the vault read. Dropped by the owner rule.
- **FR-057**: THE SYSTEM SHALL emit exactly the Loki strings of `contracts/loki-lines.md` and test each. The alert rules are out of scope as a file (no YAML); the owner builds them in Grafana from those strings, no-data set to Alerting, and the README names them.
- **FR-058** `[error]`: WHEN a bundle is built THE SYSTEM SHALL refuse tar member names with `..`, a leading `/` or a control character, and WHEN it is scanned THE SYSTEM SHALL accept only text, JSON, zip with stored or deflate entries, PNG and JPEG, refuse any member named `*.html`, and read every zip member including `trace.network`. The Playwright HTML reporter must stay off in the pipeline (002 uses its own reporter).
- **FR-062**: WHEN the tick path (`job`, `local`) runs a pipeline that declares `[state]`, `[artifacts]` or `state = true` THE SYSTEM SHALL run it as before 011: no ticket, no restore, no save, no bundle and no store call, and the declared state step sees no file. The tick never holds the store settings. A vault declaration is refused there with `[runner] vault unavailable`, as in `ci` (FR-045).
- **FR-059** `[error]`: WHEN a watch entry has `state` or `vault_items` without `secret_branches = ["main"]` written explicitly THE SYSTEM SHALL refuse the whole watch file; WHEN a pipeline has `[state]` and any step with `docker = true` THE SYSTEM SHALL end with `[runner] pipeline file refused`.
