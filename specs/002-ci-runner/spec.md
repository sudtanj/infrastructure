# Feature Specification: Central Interval CI Runner

**Feature Branch**: `002-ci-runner`

**Created**: 2026-11-26

**Status**: Draft

**Input**: User description: "lets just make it will run in interval call by external caller to execute the runner that will check every repo listed and run whats need to be run this custom rust-ci-runner on each project on each repo, please make it into the spec kit". Earlier requirements still hold: a runner of our own, written in Rust, with a TOML step graph that runs steps in containers in parallel, runnable locally, with no vendor lock-in in the pipeline format.

## Superseded in part by 003

`specs/003-infrastructure-home/spec.md` changes these items of this spec. Read the 003 spec for the current behaviour.

| 002 item | Now |
| --- | --- |
| A1 (central repo B is the monorepo or a public copy) | B is this repo. The "separate public copy" step is gone. |
| FR-029, FR-040 last sentence, US8 scenario 3 (log credentials in the Bitwarden default project) | They come from Environment secrets (003 FR-010). |
| FR-045 (the plan checks the store and that the three log values are all present or all absent) | The store check stays. The all-or-none check on the log values moves to the start of the plan and reads the environment (003 FR-011). |
| Open tasks T045, T046, T054, T065, T081 | Done or tracked in 003 (FR-025): T045 and T081 by 003 T005 and T004; T046, T054 and T065 by 003 T032 to T034. |

## Clarifications

### Session 2026-11-27

- Q: Should B be public or private, and if private, what tick interval? → A: B is a public repo with a 5-minute tick. Log shipping is required for private watched repos, and runs fail closed without it (FR-049, A25).
- Q: Should a transient runner fault (`error` status) stay final for its commit? → A: An `error` older than 30 minutes counts as not done, once only: at most one retry per commit and context (A6c, FR-009).
- Q: Is it acceptable that a public B shows private repo names, branches, SHAs and project paths? → A: Job names show only `item N`; the tick summary lists repo names without branches or SHAs (FR-058).
- Q: How is the owner alerted when ticks stop? → A: Both the Loki heartbeat line and a generic HTTPS ping to a URL held in a B variable, each used only when configured (FR-056).
- Q: Which separator for repo-scoped Bitwarden keys if `/` is refused? → A: `/` stays first choice and `:` is the fallback, held in `FCI_KEY_SEP`; `__` is dropped because two repos could collide on it (A24).

### Session 2026-11-27 (lead decisions after the critic review)

- D-a: Nothing private may reach B's public log or job outputs: the matrix carries only an index and an opaque encrypted blob, decrypted inside the job's script; step names and events of private repos go only to the log store (FR-059, FR-058, FR-030).
- D-b: GitHub secrets are no longer a source of pipeline secrets. Bitwarden is the only secret store; B holds only bootstrap secrets. Without `BWS_ACCESS_TOKEN`, a step that lists a secret ends in `error` (FR-040, FR-043, A19, A20).
- D-c: No per-repo Bitwarden project. Prefixed keys `owner/name<SEP>NAME` and the listed shared keys live in the default project; the docker account and project stay (2 of the free plan's 3 projects) (FR-040, FR-047, A21).
- D-d: `secret_branches` (default: the repo's default branch); image-pull and transient clone failures are `error`; heartbeat sent from the last job of the tick; prefixed keys need the entry's `id`; a persistent repo error fails the tick only in the first 10 minutes of a UTC hour; an idle tick makes no Bitwarden call; the A6c retry is marked " (retry)" in the status description (FR-004, FR-009, FR-017, FR-045, FR-052, FR-056, FR-060, A6c, A12).
- D-e: status calls are paged; a truncated tree falls back to per-folder calls; `item N` is unique across `run` and `run-docker`; runners are pinned to `ubuntu-24.04`; rollout uses a separate public copy of B; the cap-drop list is part of FR-054.

### Review round 1, 2026-11-27 (lead decisions)

- R2: when branches share a commit, the item uses the default branch's name if present and has secrets if any of those branches is a secret branch (FR-060).
- R3: an `allow_docker` entry with no docker token is a problem and gets no item; there is no fallback to the plain token (FR-047).
- R5, R6: docker rows use their own blob key, `FACTORY_CI_BLOB_KEY_DOCKER`; both blob keys are at least 32 characters (A20).
- R7, R8: control characters other than tab are replaced before output (FR-036); the Loki URL must be `https`, except plain `http` to loopback (FR-028).
- R10: failing to start the container engine ends the item in `error` (FR-017).
- R1, R9, R11 to R16 are implementation fixes covered by T066 to T081 and `research.md` D12 (Review round 1).
- Accepted as residual risks, not fixed: more reserved names (only store administrators create requestable names), repo names in the step summary (FR-058), the privileged dind sidecar (documented opt-in), Environment branch policy (verified live in T065), the upstream `alpine/git` tag (pinned by digest at mirror time), no container memory limit, an empty repo-scoped value falling through to the shared key.

## User Scenarios & Testing *(mandatory)*

The user is one developer who owns several GitHub repositories (the watched repos, "A") and keeps
this repository, or a public copy of it, as the central repository ("B"). B holds the runner and
a list of watched repos. The developer wants every project in every watched repo built and
tested with the `rust-ci-runner` step graph, with the result shown as a commit status on the
watched repo, and without adding a workflow file or a token to any watched repo.

An external scheduler (cron-job.org, or a cron line on an Orange Pi running `curl`) starts B's
workflow on an interval. Each start is one tick. A tick looks at every listed repo, works out
what has no result yet, runs it, and reports.

### User Story 1 - A tick runs a changed project and reports its status (Priority: P1)

The developer pushes a commit that changes files in a project of a watched repo. At the next
tick, B finds the commit, sees the project has no status and its files changed, marks it
pending, runs the project's pipeline in a container job, and sets the final status on the
commit, with a link to the user's log location when a link template is set (User Story 11).

**Why this priority**: This is the whole product.

**Independent Test**: List one repo with one project. Push a passing commit and a failing
commit. Trigger a tick after each. Check the statuses and logs.

**Acceptance Scenarios**:

1. **Given** a listed repo with a project whose files changed on a branch tip, **When** a tick
   runs, **Then** the commit gets status `pending` for that project's context, then `success`
   when every step exits 0, with a link to the log location when a link template is set.
2. **Given** a step that exits non-zero or times out, **When** the job ends, **Then** the status is
   `failure` with a one-line reason naming the step. A step that cannot start because its image
   cannot be pulled ends the item in `error` instead, which is retryable once (A6c).
3. **Given** a runner-side fault (clone fails, image pull fails, runner crashes after `pending`),
   **When** the job ends, **Then** the status is `error` with a one-line reason, never left `pending`.
4. **Given** a commit that already has a status for the project's context, **When** a tick
   runs, **Then** the project is not run again for that commit.
5. **Given** a `.factory-ci.toml` that is invalid or a symlink, **When** the project is
   reached, **Then** no step runs and the context gets `error` with the reason (for example
   "step build depends on unknown step compile").

---

### User Story 2 - An unchanged project is skipped with success (Priority: P1)

A commit touches only project `go-api` in a repo that also holds `rust-cli`. `rust-cli` has no
status on that commit. B finds the nearest first-parent ancestor with a `success` status for
`rust-cli`, sees none of its files changed since, and posts `success` "unchanged since
<short sha>" without starting a container.

**Why this priority**: Without it, a repo of N projects runs N pipelines on every commit, and
the free build minutes and the single status per commit stop being useful.

**Independent Test**: In a repo with two projects, change one. Check that only that project
ran and the other shows "unchanged since <short sha>".

**Acceptance Scenarios**:

1. **Given** a project with no status on the tip, and a first-parent ancestor within 50 commits
   that has a `success` status for the context and no changed files in the project since,
   **When** a tick runs, **Then** the tip gets `success` "unchanged since <short sha>" and no
   job starts for it.
2. **Given** the nearest ancestor with a status for the context has status `failure`, `error`
   or a stale `pending`, **When** a tick runs, **Then** the project runs, so "unchanged" never
   hides a broken state.
3. **Given** no ancestor with a status within 50 commits, or the history walk fails, **When** a
   tick runs, **Then** the project runs.
4. **Given** a commit with 0 changed files, **When** a tick runs, **Then** every project with a
   usable ancestor status is marked unchanged.

---

### User Story 3 - Several projects in one repo (Priority: P1)

A repo holds a project at its root and projects in top-level folders (for example
`rust-ci-runner/`, `go-invoices/`), each with its own `.factory-ci.toml`. Each project is
tracked, run and reported on its own.

**Why this priority**: This repo's own layout is the constitution's project-per-folder layout,
so the runner must serve it.

**Independent Test**: A repo with a root project and two folder projects. Change one folder.
Check three contexts on the commit: one ran, two unchanged.

**Acceptance Scenarios**:

1. **Given** a commit whose tree has `.factory-ci.toml` at the root and in two top-level
   folders, **When** a tick runs, **Then** the commit gets three independent contexts:
   `factory-ci` and `factory-ci/<folder>` for each folder.
2. **Given** a project whose job fails, **When** the other projects of the same commit are
   unaffected, **Then** they still run and report on their own.
3. **Given** a project, **When** its job runs, **Then** it sees only its own directory as the
   working directory, and the pipeline file read is that directory's file.
4. **Given** a `.factory-ci.toml` nested deeper than one folder, **When** a tick runs, **Then**
   it is not a project and is ignored.

---

### User Story 4 - Several repos in one tick (Priority: P1)

The watch list holds several repos. One tick checks every one and queues work from all of
them into the same run, within the caps.

**Why this priority**: The user wants one interval call to cover everything listed.

**Independent Test**: List three repos, push to two, trigger one tick, and check both pushed
repos get statuses and the third gets none.

**Acceptance Scenarios**:

1. **Given** three listed repos with new commits in two, **When** one tick runs, **Then** both
   get their work planned and run, and the third is checked and costs no job.
2. **Given** more items to run than the per-tick cap, **When** the tick plans, **Then** it runs
   up to the cap, marks only those pending, and leaves the rest to the next tick.
3. **Given** a repo that is not in the watch list, **When** any tick runs, **Then** B never
   checks it out, never lists its branches and never posts a status on it.

---

### User Story 5 - Overlapping ticks do not double-run (Priority: P1)

The scheduler fires every 5 minutes. A tick that runs a 20-minute pipeline is still going
when the next tick arrives.

**Why this priority**: A double run wastes minutes and posts conflicting statuses.

**Independent Test**: Trigger two ticks 10 seconds apart on a repo with a slow project. Check
the project ran once.

**Acceptance Scenarios**:

1. **Given** a tick in progress, **When** another tick is dispatched, **Then** the second waits
   and never runs at the same time, and the tick in progress is not cancelled.
2. **Given** items marked `pending` by an earlier tick, **When** a later tick plans, **Then**
   those items are treated as in progress and are not planned again.
3. **Given** a `pending` status older than the stale limit with no result posted, **When** a
   tick plans, **Then** it is treated as not done and the project is planned again.
4. **Given** several ticks dispatched while one runs, **When** the running tick ends, **Then**
   at most one waiting tick runs next, and no work is lost because every tick recomputes from
   the commit statuses.

---

### User Story 6 - An unreachable repo or a token without access does not block the others (Priority: P1)

One listed repo is renamed, deleted, or outside the token's grant. The tick reports it and
still handles every other repo.

**Why this priority**: The list will grow; one bad entry must not stop all CI.

**Independent Test**: List one valid repo and one name that does not exist. Trigger a tick.
Check the valid repo got its statuses and the tick finished marked failed with the bad name
in its summary.

**Acceptance Scenarios**:

1. **Given** a listed repo that returns not-found or forbidden, **When** a tick runs, **Then**
   that repo is skipped with its name and the HTTP reason in the tick summary, and every other
   repo is processed.
2. **Given** at least one repo failed to check, **When** the tick finishes its other work,
   **Then** the summary lists it on every tick, and the tick ends in failure when it starts in the
   first 10 minutes of a UTC hour (A12), so GitHub notifies the owner without a mail every tick.
3. **Given** a repo that fails inside one project (checkout fails), **When** the other
   projects of the repo are fine, **Then** only that project reports `error`.
4. **Given** a rate limit response from GitHub, **When** a tick plans, **Then** it stops
   calling that API, keeps the work already planned, lists the repos not checked in the summary
   and ends in failure.

---

### User Story 7 - Steps run as a dependency graph of containers (Priority: P1)

Unchanged from the earlier spec. A developer writes `.factory-ci.toml` in the project
directory. Each step has a name, an image and commands, and may depend on other steps. Steps
without unfinished dependencies run in parallel. All steps of a job share one checkout.

**Why this priority**: The step graph is the pipeline format and the reason for a runner of our
own.

**Independent Test**: A pipeline with `lint` and `test` (no dependencies) and `build` (depends
on both): `lint` and `test` overlap, `build` starts after both succeed, each in its own image.

**Acceptance Scenarios**:

1. **Given** steps `lint` and `test` with no dependencies, **When** the job starts, **Then**
   both start without waiting for each other, up to the parallel step limit.
2. **Given** step `build` depending on `lint` and `test`, **When** both succeed, **Then**
   `build` starts; **When** either fails, **Then** `build` is skipped and marked skipped.
3. **Given** a pipeline with a dependency cycle, a duplicate step name, an unknown dependency,
   a step with no image or no commands, or an unknown key, **When** it is loaded, **Then** the
   job is rejected before any container starts and every problem is named.
4. **Given** a step without `docker = true`, **When** it runs, **Then** its container sees only
   the checkout, the job's environment variables and the secrets the step lists, with no
   `DOCKER_HOST` and no engine socket.
5. **Given** a step with `docker = true` in a repo whose list entry sets `allow_docker = true`,
   **When** it runs, **Then** it can run `docker build` and `docker run` against the job's own
   Docker daemon, which is removed when the job ends.
6. **Given** a step with `docker = true` in a repo without `allow_docker = true`, **When** the
   pipeline is loaded, **Then** the job is rejected before any container starts with "step
   <name> needs docker but repo does not allow it".
7. **Given** a step that lists a secret its repo may not read (FR-040), or a reserved
   secret name, **When** the pipeline is loaded, **Then** the job is rejected before any
   container starts and names the secret.

---

### User Story 8 - A developer runs the same pipeline locally (Priority: P2)

Unchanged. `rust-ci-runner run [--dir PATH] [STEP]` runs a pipeline file in a local directory
with the same graph rules and executor, takes secrets from environment variables, and exits
non-zero on failure. It needs no list file, token or network access to GitHub.

**Why this priority**: It protects against lock-in, but the interval runner delivers value
first.

**Independent Test**: In a scratch directory with a pipeline file, run the local command and
compare step order and exit status with a tick's job for the same files.

**Acceptance Scenarios**:

1. **Given** a directory with a valid pipeline file, **When** the developer runs the local
   command, **Then** the exit status is 0 only if every step succeeded.
2. **Given** a step that lists a secret missing from the environment, **When** run locally,
   **Then** the run fails before any container starts and names the secret.
3. **Given** a step name argument, **When** it runs, **Then** only that step and its
   dependencies run.
4. **Given** a step with `docker = true`, **When** run locally, **Then** it gets a per-run
   Docker daemon with no opt-in.

---

### User Story 9 - Adding a repo by editing the list file (Priority: P2)

The developer adds an entry to the watch list file in B, grants `FACTORY_CI_TOKEN` access to
that repo, and merges. The next tick covers it. Nothing changes in the new repo.

**Why this priority**: This is the whole onboarding path and the reason A needs no workflow.

**Independent Test**: Add a repo entry on a branch of B, merge, trigger a tick, and check a
status appears on the repo's branch tip.

**Acceptance Scenarios**:

1. **Given** a new entry with a repo name, **When** it is merged and a tick runs, **Then** the
   repo's matching branch tips are checked and projects with no usable history run once.
2. **Given** an entry with a branch glob, shared secret names and `allow_docker`, **When** a tick
   runs, **Then** only matching branches are checked, only the repo's own prefixed keys plus the listed shared names can be read by
   that repo's pipelines, and docker steps are allowed only if `allow_docker` is true.
3. **Given** a list file with an unknown key, a malformed repo name, a duplicate repo or a
   reserved secret name, **When** a tick starts, **Then** nothing is checked or run, the tick
   fails and the summary names every problem.
4. **Given** an entry is removed, **When** the next tick runs, **Then** that repo is never
   touched again, and its old statuses stay as they are.

---

### User Story 10 - Releases are tagged and pulled by digest (Priority: P3)

Unchanged from what is built. A tag builds static x86_64 and aarch64 binaries as release assets
and a multi-arch GHCR image holding only the binary. B's tick pulls the runner image by a
digest the owner sets.

**Why this priority**: Already built. It stays in scope so the tick uses a pinned binary.

**Independent Test**: Push a tag, check the assets and image, and check a tick fails when the
digest variable is unset.

**Acceptance Scenarios**:

1. **Given** a `rust-ci-runner-v*` tag, **When** the release workflow runs, **Then** it
   publishes both binaries and a multi-arch image, and prints the image digest.
2. **Given** the digest variable is unset, **When** a tick starts, **Then** it fails before
   checking any repo, with no fallback build.

---

### User Story 11 - Step logs go to the user's own log server, not to the Actions log (Priority: P1)

B is public, so anything a step prints into the GitHub Actions log is readable by anyone. The
developer points the runner at a Loki-compatible log store they control (Grafana Cloud's free
tier, a self-hosted Loki, or another store that speaks the same push API). Every line a step
prints goes there. The Actions log shows only step names, durations, results and the run ID for a public repo, and nothing for a private one.
The commit status links to the developer's own log view.

**Why this priority**: Without it the user must pick between public logs and a private B, and
the user ruled out both vendor lock-in and exposing logs.

**Independent Test**: Configure a log destination, run a project whose step prints a known
line and a known secret value, and check the line is searchable in the log store, the secret
is not, and the Actions log holds neither.

**Acceptance Scenarios**:

1. **Given** log shipping is configured, **When** a job runs, **Then** the output of every
   step (stdout and stderr), the clone and the docker sidecar reaches the log store, and none
   of it is written to the Actions log.
2. **Given** log shipping is configured, **When** a job runs for a public repo, **Then** the Actions
   log shows only step names, durations, results and the run ID; for a private repo it shows none of
   them (FR-059).
3. **Given** a link template with a `{run_id}` placeholder, **When** a job posts its status,
   **Then** the status link is the template with the job's run ID filled in, not the Actions
   run URL. **Given** no template, **Then** the status has no link.
4. **Given** a step prints a value that the job fetched, **When** the line is
   shipped, **Then** the value is replaced before it leaves the runner.
5. **Given** the log store is down or slow, **When** a batch fails with 429 or a 5xx, **Then**
   the runner retries with backoff for up to 60 seconds, drops the batch if it still fails,
   and the step result is unchanged.
6. **Given** any batch was dropped, **When** the job posts its final status, **Then** the
   description ends with " (logs not delivered)".
7. **Given** the credential is rejected (401 or 403), **When** the first batch fails, **Then**
   the runner stops shipping for that job at once, with no retries, and the job continues and
   reports as in scenario 6.
8. **Given** `rust-ci-runner run` with no log configuration, **When** run locally, **Then**
   output prints to stdout as before. **Given** the configuration is set, **Then** it ships
   and also prints.
9. **Given** the flag `--quiet-logs` without a log configuration, **When** the command starts,
   **Then** it exits non-zero before running anything, with the reason.

---

### User Story 12 - Pipeline secrets come from Bitwarden Secrets Manager (Priority: P2)

The developer keeps pipeline secrets and the log-shipping credentials in Bitwarden Secrets
Manager and nowhere in GitHub. B holds a few bootstrap GitHub secrets (tokens, a blob key, a
heartbeat URL) and no pipeline secret, so GitHub's limit of 100 repository secrets never
applies. The runner itself still reads plain environment variables. One fetch step in B's
workflow fills them, so replacing Bitwarden means replacing that step and nothing else.

Secrets work by default: a project lists the secrets it needs in its own `.factory-ci.toml`
(on the steps that use them), and the job fetches exactly those names and nothing else.
Adding a secret for one repo means adding it in Bitwarden under that repo's key prefix and
naming it on the step. B's watch list does not change, apart from the repo's `id`.

**Why this priority**: The user wants secrets managed outside GitHub with no lock-in.

**Independent Test**: Put one secret in the default Bitwarden project under the key
`<owner>/<repo><SEP>DEPLOY_KEY`, list `DEPLOY_KEY` on one step of a project in that repo (whose
watch entry records its `id`), and leave the entry without a `secrets` list. Run a tick on the
repo's default branch. Check that step sees the value, a sibling step without the name does not,
and neither the Actions log nor the shipped logs show it.

**Acceptance Scenarios**:

1. **Given** `BWS_ACCESS_TOKEN` is set in B, **When** a job starts, **Then** the job reads the
   project's pipeline, collects the secret names its steps list, fetches only those names
   from Bitwarden and exports them as environment variables before any step container starts.
   Each step container receives only the names that step lists.
2. **Given** a step lists `DEPLOY_KEY` in repo `owner/app` whose entry records an `id`,
   **When** the job fetches, **Then** the value comes from the first match, in the default
   project, of the key `owner/app<SEP>DEPLOY_KEY`, then the key `DEPLOY_KEY` only if the
   watch entry lists it in `secrets`. A key under another repo's prefix, or a shared key the
   entry does not list, is never fetched. A repo whose entry has no `id` can read only the
   shared keys its entry lists.
3. **Given** log shipping is configured through Bitwarden, **When** a job starts, **Then** the
   three `FACTORY_CI_LOG_*` values come from the default project.
4. **Given** fetched values, **When** the fetch step ends, **Then** every value is in the
   runner's redaction set and in GitHub's masking before any other step runs.
5. **Given** `BWS_ACCESS_TOKEN` is not set, **When** a step lists a secret, **Then** the item
   ends in `error` with the reason "no secret store configured" and no step runs. A pipeline that
   lists no secret runs as usual.
6. **Given** a name exists in the default project both as `owner/app<SEP>NAME` and as `NAME`, and
   the entry lists `NAME`, **When** the job fetches, **Then** the prefixed key is used.
7. **Given** Bitwarden is unreachable or rate limited past the retry window, **When** a job
   fetches, **Then** the item ends in `error` with the reason "secret store unreachable" and
   no step runs.
8. **Given** a name a step lists is found in no allowed key, **When** a job fetches,
   **Then** the item ends in `error` naming the missing secret (the name only) and no step
   runs.
9. **Given** a step lists a name that exists in Bitwarden only under another repo's prefix,
   **When** the job fetches, **Then** the item ends in `error` naming the missing secret and
   the other repo's value is never read.
10. **Given** `BWS_ACCESS_TOKEN` is revoked or expired and there is something to run, **When** the
    tick checks the store, **Then** the tick fails before any status is posted, so the next tick
    retries. A tick with nothing to run makes no Bitwarden call.
11. **Given** an item on a branch not listed in the entry's `secret_branches` (default: the repo's
    default branch), **When** a step lists a secret, **Then** the item ends in `error` ("secrets
    not allowed on this branch") and no value is fetched.

---

### Edge Cases

- **Secret store down, slow or rate limited**: the fetch retries with backoff for up to 60 s,
  then the item ends in `error` "secret store unreachable". No step starts with only some of
  its secrets. All names arrive, or none are used.
- **Secret store down at the pre-post check**: after the tick has computed what to run and before it
  posts any status, it checks the token and the store. On failure it posts nothing and fails the
  tick, because an `error` status can be retried only once (A6c) and an outage must not cost a
  commit its run. A tick with nothing to run makes no store call.
- **Token revoked or expired (401, 403)**: no retry. At the pre-post check it fails the tick with the
  reason "secret store rejected the token". If it happens inside a job, that item ends in
  `error` with the same reason.
- **Listed name missing from every allowed key**: `error` for that item, naming the secret.
  The value of no other secret is shown. A name in the watch entry's `secrets` that no step
  of this project lists is not fetched and is not an error.
- **Invalid pipeline**: the job rejects it before the fetch, so no secret is read for it.
- **Two projects in one repo**: both read the same repo prefix. Each job fetches only the
  names its own pipeline lists. Separating two projects of one repo is out of scope.
- **Multi-line secret value (a PEM key, for example)**: the value reaches the step intact,
  newlines included. Redaction and GitHub masking register the whole value and each line of
  at least 4 characters separately, so printing one line is still caught.
- **A Bitwarden key not reachable by the repo** (another repo's prefix, or a shared key the
  watch entry does not list): never fetched or exported.
- **Name that is not a valid environment variable name** in a step's `secrets`: rejected
  with the pipeline, before the fetch.
- **No secret store configured** (`BWS_ACCESS_TOKEN` unset): a step that lists a secret ends the
  item in `error` "no secret store configured". There is no GitHub secret fallback.
- **Item on a branch outside `secret_branches`**: a step that lists a secret ends the item in `error`;
  steps that list none run.
- **Prefixed key for an entry with no `id`**: not read. Only the shared keys the entry lists are.
  starts, as a reserved name.
- **Log endpoint down for the whole job**: every batch is retried up to 60 s then dropped.
  The job result, status and runtime of the other steps are unaffected, apart from the 60 s
  retries, which never block a step: shipping runs beside the steps.
- **Wrong credential (401, 403)**: shipping stops for the job on the first rejection. No retry
  storm. Other jobs of the tick try again on their own, so a fixed credential works at the next
  tick.
- **Rejected batch (400, for example a timestamp the store refuses)**: not retried. The batch
  is dropped and counts as not delivered.
- **Very long line**: split into pieces of at most 16 KB, in order, with no loss. Nothing is
  truncated.
- **Binary or non-UTF-8 output**: converted lossily, invalid bytes replaced with the Unicode
  replacement character. The job does not fail.
- **Clock skew**: each line's timestamp comes from the runner's clock when the line is read. A
  runner clock the store rejects as too old or too far ahead yields a 400 and a dropped batch.
- **A step prints a secret**: redacted before it is queued for shipping. The same rule as for
  any output path.
- **Status description over GitHub's length limit**: the reason is shortened first. The
  " (logs not delivered)" suffix is kept.
- **Job ends with unsent lines**: the runner flushes before it posts the final status, for up
  to 60 s. Lines still unsent are dropped.
- **Log volume over the per-run cap**: output beyond the cap is not shipped, and the last
  shipped line says so.
- **Only some of the three log secrets set**: treated as a configuration error. The tick fails
  before any repo is checked, naming the missing name. With none set, shipping is off and step
  output goes to the Actions log as before.
- **A pipeline lists `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` or `FACTORY_CI_LOG_TOKEN`**:
  rejected before any container starts, as a reserved name.
- **Force-push**: the tip is a new commit. Its history is walked from the new tip. Statuses on
  the abandoned commit are ignored. If the rewrite leaves no ancestor with a status within 50
  commits, the project runs.
- **New branch**: its tip is checked like any other. If it branched from a commit that has
  statuses, unchanged projects get "unchanged since <short sha>" and only changed projects
  run. If the tip is a commit another branch already finished, nothing runs.
- **Deleted branch**: no longer listed, so nothing runs. A `pending` left on its tip is
  ignored.
- **Branch tips that share a commit**: the commit is planned once per project.
- **Commit with 0 changed files**: see User Story 2, scenario 4. A first-parent walk from a
  merge commit compares the merge commit's tree, so changes merged in count.
- **Project directory added**: no ancestor status for the new context, so it runs.
- **Project directory removed, or `.factory-ci.toml` deleted**: it is no longer a project at
  that commit, so nothing runs and no status is posted for it. Old statuses stay.
- **Project directory renamed**: a new context, so it runs. The old context stops being posted.
- **More items than the cap**: the tick runs up to the item cap, ordered by oldest commit
  first so nothing starves, and the summary reports how many wait. A pending status is posted
  only for the items that run.
- **Invalid `.factory-ci.toml`**: `error` on that project's context only. See User Story 1,
  scenario 5. More than 100 steps or a file over 256 KB is invalid.
- **GitHub API rate limit**: see User Story 6, scenario 4. A rate-limited tick leaves no
  `pending` for unplanned work.
- **Two ticks racing on the same item despite the concurrency group**: the second finds
  `pending` and skips. A stale `pending` older than the limit is retried.
- **A job crashes or is cancelled after `pending`**: the job's final step posts `error`. If
  even that fails, the stale rule makes the item run again after the limit.
- **Same-repo pull request**: its head is a branch tip, so it runs as that branch. The merge
  result of the PR is not tested. Fork pull requests are not branches of the repo, so they
  never run.
- **A step prints gigabytes of output**: the log is cut at a size limit with a marker.
- **A step hangs**: it is killed at its own timeout if set, otherwise at the job timeout.
  Unstarted steps are skipped.
- **A secret value appears in output**: it is replaced in log lines, statuses and errors when
  the whole value appears.
- **Step names, images and commands**: passed to the engine as separate arguments, never
  through a host shell.
- **Watched repo's `.factory-ci.toml` tries to read a secret of another repo**:
  rejected before any container starts.
- **Status per commit and context caps at 1000 updates on GitHub**: a project at one commit
  reaches at most 4 updates, because the retry budget is one (A6c): pending, an `error` or `failure`, one retry's pending marked " (retry)", and its final. A stale pending that has used the budget is closed with one `error`, which is also at most 4.

## Requirements *(mandatory)*

### Functional Requirements

**Tick and trigger**

- **FR-001**: B MUST provide a workflow that an external caller starts through the GitHub
  `workflow_dispatch` API, and MUST NOT depend on GitHub's `schedule` trigger. A tick MUST work
  when started by `curl` with no other client.
- **FR-002**: The caller's token MUST be able to start B's workflow only (Actions write on B).
  `FACTORY_CI_TOKEN` MUST hold Contents read and Commit statuses read and write on the listed
  repos only. Both tokens MUST live only in B. Neither MUST reach any step, any log, any status
  or any watched repo.
- **FR-003**: The workflow MUST use one concurrency group for all ticks and MUST NOT cancel a
  tick in progress.

**Watch list**

- **FR-004**: B MUST hold a versioned list file naming the watched repos. Each entry MUST have
  the repo as `owner/name`, and MAY have its numeric repo `id` (FR-052), a branch glob list (default: every branch),
  `secret_branches` (FR-060), the shared secret names that repo's pipelines may read in addition to
  its own prefixed keys (default: none, see FR-040), and `allow_docker` (default false).
- **FR-005**: The tick MUST validate the whole list before any other work. It MUST reject
  unknown keys, a malformed repo name, a duplicate repo, a malformed glob, and reserved secret
  names (including `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN` and
  `BWS_ACCESS_TOKEN`),
  name every problem, and run nothing when any exists.
- **FR-006**: B MUST NOT list branches of, check out, or post a status on any repo not in the
  list.

**Planning**

- **FR-007**: For each listed repo, the tick MUST take the tip commit of every branch matching
  the filter. Same-repo pull requests are covered by their head branch. Fork pull requests
  MUST never run. A deleted branch MUST run nothing.
- **FR-008**: For each tip, the tick MUST find the projects. A project is a directory holding
  a `.factory-ci.toml`: the repo root, or a top-level folder. Deeper files MUST be ignored.
  Each project MUST have its own commit status context, `factory-ci/<project-path>` for a
  folder and `factory-ci` for the root.
- **FR-009**: A project at a commit MUST be treated as done if that commit already has a
  status for its context, unless that status is `pending` and older than the stale limit, or is `error` and older than
  30 minutes, and its description does not yet end with " (retry)" (the retry budget of A6c is then unused).
- **FR-010**: Otherwise the tick MUST walk first-parent ancestors for at most 50 commits to
  the nearest one that has a status for the context. If that status is `success` and no file
  of the project changed between it and the tip, the project is unchanged. In every other
  case, including no ancestor found or any failure of the walk, the project MUST run. The root
  project's files are every file outside the folders that are themselves projects.
- **FR-011**: For an unchanged project the tick MUST post `success` with the description
  "unchanged since <short sha>" and MUST NOT start a job for it.
- **FR-012**: The tick MUST compute the list of (repo, sha, project) items to run, order it by
  oldest commit first within a repo (repos interleaved, FR-055), take at most the per-tick item cap, and post `pending` for exactly
  those items before any job starts. Items beyond the cap MUST wait for a later tick.
- **FR-013**: The tick MUST process each repo independently. An error on one repo (not found,
  forbidden, network) MUST be recorded with the repo name and the reason and MUST NOT stop
  other repos. The summary lists every such error on every tick; the tick finishes as failed
  only when it starts in the first 10 minutes of a UTC hour (A12), so a persistent error does not
  fail a tick every 5 minutes.
- **FR-014**: On a GitHub API rate limit, the tick MUST stop calling the API, keep the work
  already planned and run it, list the unchecked repos in the summary, and finish as failed
  under the same hourly rule.

**Execution**

- **FR-015**: The planned items MUST run as a matrix of jobs, one item per job, each in
  isolation from the others, using the same pipeline executor as `rust-ci-runner run`, started through its `job` command (cloning, secret checks, log shipping and the final status need more than the local command). The matrix MUST have a max-parallel
  cap and the per-tick item cap.
- **FR-016**: A job MUST check out the repo at the exact commit with `FACTORY_CI_TOKEN`, leave
  no git credentials in the checkout, and use the project's directory as the working directory.
- **FR-017**: A job MUST post a final status (`success`, `failure` or `error`) for its context
  with the link described in FR-031, even if the pipeline fails or the job is cancelled, and
  on failure MUST name the first failed step. A step that cannot start because its image cannot be
  pulled, a failure to start the container engine, and a transient clone fault, MUST end the item in `error` (retryable, A6c), not `failure`.
- **FR-018**: A job MUST receive only the secrets its pipeline's steps list and that its repo
  may read (FR-040, FR-060), and a step MUST receive only the secrets it lists. A listed secret the
  repo may not read, or a reserved name, MUST fail the job before any container starts. No token of B, and none of the three log
  shipping secrets, MUST reach a step.
- **FR-019**: The pipeline file MUST define named steps, each with an image and commands, and
  optionally `depends_on`, `env`, `secrets`, `timeout` and `docker`. An invalid file MUST be
  rejected before any container starts with every problem named, and the project's context
  MUST get `error`. A symlinked pipeline file, more than 100 steps, or a file over 256 KB MUST
  be invalid.
- **FR-020**: The runner MUST run each step in a new container with the checkout mounted as the
  working directory, start a step only when its dependencies succeeded, run independent steps
  in parallel up to a limit (default 4), and mark a step skipped when a dependency failed or
  was skipped. The host engine socket MUST NOT be mounted into any container.
- **FR-021**: A step MAY set `docker = true`. Only such steps get engine access, and only
  job summary. An error message MUST NOT contain a request path (repo name or SHA): a failed final status
  post reports only that it was not delivered and the HTTP code.
  repo's entry sets `allow_docker = true`. The sidecar, network and volumes MUST be removed
  when the job ends.
- **FR-022**: The runner MUST pass the commit SHA, the previous SHA (the ancestor used for the
  change check, when known), the branch and the event type (`push`) to every step as
  environment variables.
- **FR-023**: The runner MUST enforce per-step timeouts, a job timeout (default 30 minutes)
  and a log size limit, and MUST remove every container, network and volume of a job when it
  ends.
- **FR-024**: The runner MUST NOT write secrets or tokens into logs, statuses, errors or the
  job summary.

**Local use and release**

- **FR-025**: The same binary MUST provide `run [--dir PATH] [STEP]` for local use with the
  same step rules and executor. Secrets come from environment variables. It exits non-zero on
  failure. With no log configuration it prints step output to stdout. With the configuration
  set it ships and also prints (FR-037).
- **FR-026**: The release workflow MUST publish static x86_64 and aarch64 binaries as release
  assets and a multi-arch image containing only the binary, on a tag. A tick MUST pull the
  runner image by a digest held in B's configuration and MUST fail before any other work when
  it is unset.

**Platform**

- **FR-027**: Only GitHub-hosted watched repos are supported.

**Log shipping**

- **FR-028**: The runner MUST ship step logs over the Loki HTTP push API (`POST
  <url>/loki/api/v1/push`, JSON body) with basic auth. Changing provider MUST need only a new
  URL and credential, with no code change. The URL MUST be `https`, except plain `http` to `127.0.0.1`, `localhost` or
  `[::1]` (the test fake uses it); any other scheme or host is refused.
- **FR-029**: Shipping is configured by three secrets held in the secret store's default project: `FACTORY_CI_LOG_URL`,
  `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`. The names are reserved: a pipeline MUST NOT
  list them (rejected before any container starts), and they MUST NOT reach any step
  container. All three set means shipping is on, none set means off, and a partial set MUST
  fail the tick before any repo is checked, naming the missing name.
- **FR-030**: When shipping is on in a tick, the output of every step (stdout and stderr), the
  clone and the docker sidecar MUST NOT be written to the GitHub Actions log. For a public repo
  that log MAY show step names, durations, results and the run ID; for a private repo it MUST show
  none of them (FR-059). The runner MUST be started with `--quiet-logs` (and `--private` for a
  private repo) in the workflow to get this, decided after the log credentials are fetched.
  `--quiet-logs` without a complete log configuration MUST exit non-zero before running anything.
- **FR-031**: The commit status link MUST be a configurable template with a `{run_id}`
  placeholder (for example a Grafana Explore URL), filled in with the job's run ID. When
  shipping is off the link is the Actions run, and when shipping is on and no template is set
  the status MUST have no link. The `pending` status posted by the plan carries no link; only a job's final
  status does, because the run ID the link needs is assigned after the `pending` post.
- **FR-032**: Redaction MUST run before a line is queued for shipping, so no
  fetched secret value leaves the runner.
- **FR-033**: Each shipped stream MUST carry only the labels `repo`, `project`, `step` and
  `stream` (`stdout` or `stderr`). The commit sha, branch and run ID MUST go in each line's
  structured metadata or a fixed line prefix, not in labels. Timestamps MUST come from the
  runner's clock.
- **FR-034**: The runner MUST batch lines (target: every 1 second or 1 MB, whichever first),
  retry a batch with backoff on 429 and 5xx for up to 60 seconds, then drop it. A 401 or 403
  MUST stop shipping for that job at once, and any other 4xx MUST drop the batch without a
  retry. Shipping MUST run beside the steps and MUST NOT change any step or job result. The
  runner MUST flush remaining lines for up to 60 seconds before the final status is posted.
  Dropped lines MUST NOT be printed to the Actions log.
- **FR-035**: If any batch was dropped, the final status description MUST end with " (logs not
  delivered)", and the reason MUST be shortened before that suffix is.
- **FR-036**: A line longer than 16 KB MUST be split into ordered pieces of at most 16 KB.
  Output that is not valid UTF-8 MUST be converted lossily. The per-run log volume MUST stay
  capped as before, with a last shipped line saying the cap was hit. Control characters other than tab (including
  `\r`) MUST be replaced before a line is printed or shipped.
- **FR-037**: Local `rust-ci-runner run` MUST print to stdout when no log configuration is
  set. With it set, it MUST ship and also print, unless `--quiet-logs` is given.

**Secret sources**

- **FR-038**: The runner binary MUST read secrets from environment variables only. It MUST
  contain no code specific to any secret vendor. Replacing the secret store MUST change only
  the fetch step in B's workflow.
- **FR-039**: When `BWS_ACCESS_TOKEN` is set in B, one fetch step in B's workflow MUST fill the
  environment variables for each job from Bitwarden Secrets Manager before any step
  container starts. The names to fetch MUST come from the `secrets` lists of the project's
  validated pipeline; the runner prints them, so the fetch step does not parse the pipeline
  itself. Nothing else is fetched.
- **FR-040**: For a listed name `NAME` in repo `owner/name`, the fetch MUST take the first match, in the default
  project named in B's configuration, of: (1) key `owner/name<SEP>NAME`, when the watch entry records an `id`;
  (2) key `NAME`, only when the watch entry lists `NAME` in `secrets`. For a repo with `allow_docker`, the default
  project is the docker project (FR-047). Keys under another repo's prefix MUST never be read. The default project
  MUST also hold the `FACTORY_CI_LOG_*` values. There are no per-repo projects, because the free plan allows only
  3 projects (A23).
- **FR-041**: `BWS_ACCESS_TOKEN` MUST be a reserved name: a pipeline MUST NOT list it, and it
  MUST NOT reach any step container. The fetch step MUST write only the names the work
  item's pipeline lists.
- **FR-042**: The fetch step MUST add every fetched value, and for a multi-line value each line
  of at least 4 characters, to the runner's redaction and to GitHub masking before any other
  step runs.
- **FR-043**: When `BWS_ACCESS_TOKEN` is not set, a step that lists a secret MUST end the item in `error` with
  the reason "no secret store configured", and no step starts. A pipeline that lists no secret runs as usual.
  GitHub secrets are not a source of pipeline secrets.
- **FR-044**: If the secret store is unreachable, rate limited past a 60 s retry window, or
  a listed name matches no allowed source, the item MUST end in `error` with the reason
  "secret store unreachable" or the missing name, and MUST start no step. A 401 or 403 MUST
  not be retried.
- **FR-045**: After the tick has computed what to run, and before it posts any status, it MUST check that the
  token and the store work and that the three log values are all present or all absent. A tick with nothing to
  run MUST make no secret-store call. On failure it MUST post no status, run nothing and fail the tick with the
  reason.
- **FR-046**: `rust-ci-runner run` locally MUST need no Bitwarden or other secret store. Its
  secrets come from the developer's environment.

**Hardening (review findings)**

- **FR-047**: Work items of repos with `allow_docker = true` MUST run in a job separate from all other
  items. That job MUST use its own Bitwarden machine account and project, which hold only those repos'
  keys, and its own fine-grained token limited to those repos. It MUST NOT receive B's GitHub secrets
  as a set or the other jobs' tokens, and MUST NOT run on the plain token: an `allow_docker` entry whose docker
  token is missing is a problem and gets no item. GitHub secrets are never a source of pipeline secrets, for any repo.
- **FR-048**: B's secrets MUST live in GitHub Environments whose deployment branch policy allows only
  `main`. Every job that uses a secret MUST declare its environment and run only when the workflow
  ref is `main`. A dispatch of another ref MUST receive no secret and end red.
- **FR-049**: When B is public and no log store resolves for a job, the tick MUST NOT plan or run a
  private repo, and the runner MUST never print step output of a private repo. Quiet mode MUST be decided
  from the resolved log configuration, not from the environment at the start of the fetch step.
- **FR-050**: Only commit statuses created by the user of `FACTORY_CI_TOKEN` count when the tick looks for
  a status. A status by anyone else is treated as absent.
- **FR-051**: Text from a repo (step names, secret names, TOML errors) that reaches a status description,
  summary or reason MUST be restricted to `[A-Za-z0-9 _.:/-]`, shortened, and never start with `::`.
- **FR-052**: A watch entry MAY record the numeric repo `id`. When it does, the tick MUST check it against
  GitHub before planning and skip the repo on a mismatch. The repo-prefixed Bitwarden keys (FR-040 source 1)
  require an entry that records an `id`; without one, only the shared keys the entry lists are readable.
- **FR-053**: A secret value with any trimmed line shorter than 4 characters MUST be refused (the item ends
  in `error`, the name only). The runner and the fetch step MUST also mask the base64 form of each value and
  of the clone's Basic authorization header.
- **FR-054**: Step containers MUST NOT be able to reach `169.254.169.254` and `168.63.129.16`, and MUST drop
  the capabilities `NET_RAW`, `MKNOD` and `SETFCAP`.
- **FR-055**: Status writes MUST be spaced at least 750 ms apart and limited to 60 per tick; the rest wait
  for the next tick. A secondary-rate-limit answer counts as a rate limit. When the item cap applies, items
  from different repos MUST be interleaved so one repo cannot take all of it. On a rate-limit answer a write waits
  30 s once, then the remaining writes wait for the next tick.
- **FR-056**: At the end of each tick, a job that always runs MUST report that the tick ran through every
  heartbeat channel that is configured: one HTTPS request to the URL held in B's `FACTORY_CI_HEARTBEAT_URL`
  when that secret is set, and one heartbeat line to the log store when shipping is on and the tick fetched the
  log credentials. Each channel is used only when configured. A failed report MUST NOT fail the tick. The URL is
  treated as a secret (it grants the right to ping). The owner sets the alert grace above the longest tick.
- **FR-057**: The runner MUST refuse a clone image or docker sidecar image that is not pinned by digest.
- **FR-058**: A job's name in the Actions UI MUST show only `item N`, where N is unique across the `run` and
  `run-docker` jobs of the tick, and the tick summary MUST list repo names without branches or SHAs, so a
  public B shows the least about private repos.
- **FR-059**: Nothing private MUST reach B's public log or job outputs. The matrix MUST carry only the item
  index and an opaque blob, encrypted with a key held in a masked secret and decrypted inside the job's script,
  so no step `env:`, job name or job output shows a branch, SHA, project path, secret name or run ID (a public
  repo's job log lines may still show the run ID, FR-030). For a private repo, step
  names, durations and results MUST go only to the log store, never to the Actions log. A job started without a
  log store for a private repo MUST fail closed before running anything.
- **FR-060**: A watch entry MAY set `secret_branches` (globs; default: the repo's default branch). Items on
  other branches MUST get no secrets: a step that lists a secret ends the item in `error`. When several
  branches point at one commit, the item takes the default branch's name if it is among them, and has secrets
  if any of those branches matches `secret_branches`.

### Removed

These parts of the earlier design are cut. Plan and tasks drop them.

- The long-running `serve` mode: the polling daemon, the local state store, the `/health`
  endpoint, the logs HTTP server, and GitLab and Gitea status posting.
- The push-triggered dispatch from A: `factory-ci-dispatch.yml` and `FACTORY_CI_DISPATCH`.
- The reusable workflow `factory-ci.yml`.

### Key Entities

- **Watch list**: the versioned file in B. Holds entries.
- **Watch entry**: repo (`owner/name`), optional repo `id`, optional branch globs, optional `secret_branches`,
  optional shared secret names, `allow_docker`.
- **Tick**: one start of B's workflow by the caller. Reads the list, plans, runs, reports.
- **Project**: a directory in a repo holding `.factory-ci.toml`: the root or a top-level
  folder. Has a path and a status context.
- **Work item**: (repo, sha, project). Planned when the project is not done and changed.
- **Status context**: `factory-ci` (root) or `factory-ci/<project-path>`. The commit statuses
  on the watched repo are the only record of what ran.
- **Pipeline**: the `.factory-ci.toml` of a project: steps forming a directed acyclic graph.
- **Step**: name, image, commands, dependencies, environment, requested secret names,
  optional timeout and docker flag; state (pending, running, success, failure, skipped).
- **Job**: one execution of one work item in isolation. Has a log and a final status.
- **Secret source**: Bitwarden Secrets Manager (via one token in B), default project only.
  Repo-scoped keys (`owner/name<SEP>NAME`), shared keys, and the log shipping keys live in it.
- **Log destination**: URL, user and token of a Loki-compatible store, plus an optional link
  template with `{run_id}`. Held only in B.
- **Heartbeat**: the report that a tick ran: a log line and/or an HTTPS ping, each sent only when configured, so a missing report shows a stopped tick (FR-056).
- **Log line**: text, runner timestamp, labels (`repo`, `project`, `step`, `stream`), and
  metadata (sha, branch, run ID).

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: A user adds a repo to the list and sees its first status within one caller
  interval plus 10 minutes, with no change in the repo and no token added to it.
- **SC-002**: In a repo of 5 projects where a commit changes 1, exactly 1 pipeline runs and the
  other 4 get "unchanged since <short sha>" in the same tick.
- **SC-003**: Over 20 ticks fired in pairs 10 seconds apart on a test repo, no work item runs
  twice.
- **SC-004**: With one unreachable repo among 5 listed, the other 4 get all their statuses in
  the same tick, and the tick ends failed with the bad repo named in the summary.
- **SC-005**: An idle tick (nothing new) over 10 repos with 5 branches each uses under 150
  GitHub API requests, so a 5-minute interval stays under the hourly limit.
- **SC-006**: Zero secrets or tokens appear in any log, status or summary across the test
  suite, which feeds known secret values through every output path.
- **SC-007**: A pipeline that passes with the local command passes in a tick for the same
  files and images, and the reverse, apart from secrets.
- **SC-008**: A job killed after `pending` leaves its context `error` within 5 minutes, or
  runs again within the stale limit.
- **SC-009**: With log shipping configured, 0 lines of step output appear in the Actions log
  across the test suite, which prints known markers from stdout, stderr, the clone and a
  docker step.
- **SC-010**: A line a step prints is searchable in the user's log store within 30 seconds
  after the step finishes, when the store is reachable.
- **SC-011**: With the log store unreachable, every job in a tick still ends with the same
  step results and a status ending " (logs not delivered)", and no job takes more than 60
  seconds longer than it would with shipping off.

## Assumptions

Each assumption below is a decision the lead made or a default chosen here. The user can
overturn any of them.

- **A1 (central repo)**: B is this monorepo or a public copy. It holds the runner and the
  watch list. Watched repos need no workflow file and no token of their own.
- **A2 (trigger)**: An external scheduler calls B's workflow on an interval through the
  `workflow_dispatch` API, for example cron-job.org or a cron line with `curl` on an Orange Pi.
  GitHub's `schedule` trigger is not relied on, because it runs late and is disabled after
  60 days of inactivity on public repos. Suggested interval: 5 minutes.
- **A3 (list file)**: Watched repos and their settings live in a versioned file in B: repo,
  optional branch glob, allowed secret names, `allow_docker`. Only listed repos are ever
  checked out. A malformed list stops the whole tick, so a typo in a secret name list is never
  half applied (default chosen here).
- **A4 (what is checked)**: Every tick checks the tip of every branch that matches the filter.
  Same-repo pull requests are covered as branch tips. Fork pull requests never run. The merge
  result of a pull request is not tested.
- **A5 (projects)**: A project is a directory with `.factory-ci.toml`, at the repo root or a
  top-level folder. Contexts are `factory-ci` and `factory-ci/<project-path>`. The root
  project's files are everything outside folders that are projects (default chosen here).
- **A6 (stateless planning)**: Commit statuses on the watched repo are the only record. A
  project is done at a commit with any status for its context. Otherwise it runs only if its
  files changed since the nearest first-parent ancestor with a status, within 50 commits.
  No ancestor or a failed walk means run. Unchanged means `success` "unchanged since <short
  sha>" with no run. `pending` is posted before running. One concurrency group, no cancel.
- **A6a (default chosen here, overturns part of A6)**: The ancestor's status must be
  `success` to skip. A `failure`, `error` or stale `pending` ancestor means run, because a
  skipped project would otherwise show success over a broken state.
- **A6b (default chosen here, amended 2026-11-27)**: A `pending` status older than 2 hours counts as no
  status. Without this a crashed job blocks its commit forever. The replan uses the retry budget of A6c.
- **A6c (amended 2026-11-27)**: `success` and `failure` are final for that commit and context. A retry budget
  of one covers each commit and context: an `error` older than 30 minutes, or a `pending` older than 2 hours,
  counts as not done once. The retry's `pending` has the description "queued (retry)" and its final
  description ends with " (retry)". A status whose description already ends with " (retry)" has used the
  budget: an old `error` then stays final, and an old `pending` is closed by the tick with an `error` "job did
  not finish (retry)". So a transient runner fault gets a second run and a repeated fault stays red. There is
  no manual re-run input in this version. A new commit starts a new run.
- **A7 (execution)**: One planning job computes the items. A matrix of jobs runs each in
  isolation. Defaults chosen here: max-parallel 4, at most 20 items per tick, oldest commit
  first. The plan posts `pending` (no link); each job then posts `success`, `failure` or `error`, with the link from FR-031.
- **A8 (step model)**: Unchanged. TOML step graph as a DAG, parallel containers, `docker =
  true` with a per-job `docker:dind` sidecar only where `allow_docker` is set, secrets limited
  to the names its steps list and the repo may read, reserved names refused. `rust-ci-runner run [--dir] [STEP]` stays.
  Steps share one checkout; there are no artifacts, caches, service containers or per-step
  statuses. A docker step builds from the context sent to its daemon.
- **A9 (cuts)**: See Removed. Only GitHub-hosted watched repos are supported.
- **A10 (tokens)**: Two tokens, both stored only in B: the caller's token (Actions write on B
  only) and `FACTORY_CI_TOKEN` (Contents read, Commit statuses read and write on the listed
  repos). When log shipping is off, Actions logs are public if B is public. With shipping on
  (A15), step output is not in them. The user accepted the GitHub terms-of-service risk for
  running this service on GitHub Actions.
- **A11 (releases)**: Tagged, multi-arch, GHCR image pulled by digest, as already built.
- **A12 (default chosen here, amended 2026-11-27)**: A tick with a persistent repo error or rate limit is
  reported in its summary on every tick and finishes as failed only when it starts in the first 10 minutes of
  a UTC hour, so GitHub's failure notice reaches the owner at most about twice an hour instead of every 5
  minutes. A fatal error (invalid list, bad log configuration) fails every tick. No other alerting is built
  beyond the heartbeat (FR-056).
- **A13 (default chosen here)**: Anyone who can push to a listed repo can run code on B's
  runners and read that repo's secrets. The user lists only repos they trust, and
  keeps each repo's secret set short.
- **A14 (default chosen here)**: Pipeline steps run on GitHub-hosted runners, so step images
  must exist for that runner's architecture (x86_64).
- **A15 (log shipping, lead decision)**: The runner pushes step logs over the Loki HTTP push
  API with basic auth. Grafana Cloud's free Loki, a self-hosted Loki and other compatible
  stores serve it, so switching means a new URL and credential. Considered and not done here:
  OTLP logs. Three reserved secrets in B hold the destination. When shipping is on, step
  output stays out of the Actions log and the status link comes from a template with
  `{run_id}`. Labels are `repo`, `project`, `step`, `stream` to protect store cardinality and
  free-tier limits. The sha, branch and run ID travel as line metadata.
- **A16 (free tier)**: Volume stays within the provider's free-tier limits, and the runner caps
  per-run volume. This spec states no ingestion or retention figures, because none are
  cited. The user checks their provider's current limits.
- **A17 (default chosen here)**: Log delivery never decides a CI result. A dropped batch is
  lost for good: no spool on disk, no print to the Actions log. A split long line cuts at
  16 KB. The 1 s and 1 MB batch sizes and the 60 s retry window are starting values that the
  plan may tune.
- **A18 (default chosen here)**: Pipeline step errors that the runner itself prints before any
  step starts (an invalid pipeline file, for example) go through the same shipping and quiet
  path as step output. The status description still carries a short reason, and it is public
  on the commit.
- **A19 (secret store, lead decision, amended 2026-11-27)**: Pipeline secrets and the log-shipping
  credentials come from Bitwarden Secrets Manager only. B holds a small set of bootstrap GitHub
  secrets (the access tokens, the blob key, the heartbeat URL) and no pipeline secret. The runner
  binary stays vendor-free (FR-038). A Bitwarden secret key is `owner/name<SEP>NAME` for a repo's own
  secret, or `NAME` for a shared secret the watch entry lists, both in the default project. The
  pipeline's step `secrets` lists decide what is fetched; the watch entry's `secrets` list only
  grants the shared keys (FR-040). Without `BWS_ACCESS_TOKEN`, a step that lists a secret ends the
  item in `error`.
- **A24 (clarified 2026-11-27)**: Repo-scoped keys use the prefix `owner/name/`. The separator is held in the
  constant `FCI_KEY_SEP`, first choice `/`. The plan MUST confirm Bitwarden accepts `/` in a key; if not, the
  separator becomes `:`, which appears in neither a repo name nor a variable name, so two repos can never produce the
  same key. The trust boundary is B's fetch step, not Bitwarden: one machine
  account can read the whole default project, and B's code decides which keys a repo gets.
  Anyone who can push a pipeline to a watched repo can read every key under that repo's
  prefix and the shared names its entry lists, so per-repo prefixes keep one repo's
  committers away from another repo's secrets.
- **A20 (default chosen here, amended 2026-11-27)**: The bootstrap secrets stay GitHub secrets in B:
  `FACTORY_CI_TOKEN`, `FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN`, `BWS_ACCESS_TOKEN_DOCKER`, the blob
  keys `FACTORY_CI_BLOB_KEY` (plain rows) and `FACTORY_CI_BLOB_KEY_DOCKER` (docker rows, each at least 32
  characters), and `FACTORY_CI_HEARTBEAT_URL`. The planning job needs the tokens before any fetch, so they
  cannot live in the store they unlock. The caller's token lives in the scheduler.
- **A21 (default chosen here)**: A machine-account token has read access only, to the default project
  (the docker project for the docker account). The fetch step's client is pinned by version or digest
  like the runner image. The free plan allows 3 projects and 3 machine accounts; this design uses 2
  projects (default, docker) and 2 accounts.
- **A22 (default chosen here)**: A multi-line value is redacted as a whole and line by line for
  lines of 4 characters or more. A value with a shorter line is refused (FR-053), since redacting a line such as
  `}` would mangle all output, and leaving it alone would leak. The runner catches a secret only when a whole redacted piece
  appears in a line.
- **A23 (Bitwarden facts, checked 2026-11-26)**: The free Secrets Manager plan is cloud only
  and allows 2 users, 3 projects and 3 machine accounts, with unlimited secrets
  (bitwarden.com/help/secrets-manager-plans). Self-hosting Secrets Manager needs an Enterprise
  license; Vaultwarden does not support it and does not plan to (vaultwarden discussions #5483
  and #5702). API rate limits are not documented, so the fetch retries with backoff on 429. A
  machine account can be limited to "Can read" on chosen projects. The fetch uses the `bws`
  CLI (`bws secret list <PROJECT_ID> -o json`, token in `BWS_ACCESS_TOKEN`) from a pinned
  `bitwarden/sdk-sm` release, checked against its published SHA-256 checksums. Lock-in stays
  low because the runner reads only environment variables (FR-038): leaving Bitwarden means
  replacing one workflow step. A free, self-hosted
  Bitwarden secret store is not available.
- **A25 (clarified 2026-11-27, cost)**: B is a public repository and the scheduler fires every 5 minutes. An idle
  5-minute tick bills about 2 minutes, which a public repo does not pay for. Because B is public, step output of a
  private watched repo never reaches the Actions log: log shipping is required for private watched repos and a job
  without it fails closed (FR-049). A private B is not a supported configuration of this version.
- **A26 (default chosen here, Grafana free)**: 50 GB a month and 14-day retention. A status link to a log
  stops working after 14 days. A usage alert at 70% is the owner's job.
- **A27 (known limit, accepted 2026-11-27)**: If `FACTORY_CI_TOKEN_DOCKER` is removed while a Docker repo already has a
  stale `pending` status, nothing closes that status until a tick runs with the Docker token again: without it, phase C
  drops that repo's "unchanged" and closing-error posts (FR-047 forbids using the plain token for Docker repos). The
  status stays `pending` until then.
