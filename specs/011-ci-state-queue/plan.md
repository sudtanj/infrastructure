# Implementation Plan: A strict queue, saved state and debug artifacts for the ci command

**Branch**: `011-ci-state-queue` | **Date**: 2026-11-28 (amended the same day twice: the owner rule "Amend 011", then the owner's tick decisions) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/011-ci-state-queue/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before this plan and `/speckit-analyze` after `tasks.md`; the owner rule is recorded as a dated clarification in spec.md and `/speckit-analyze` ran on this revision. The constitution stays at 1.2.0; 011 does not use the schedule allowance it grants.

## Summary

011 builds on 010 (step secrets). Every part is Rust, `Cargo` files, toml, the runner `Dockerfile`, one YAML rule file and one shell script under `rust-ci-runner/`; nothing under `.github/workflows/` or `.github/scripts/` changes.

**Owner tick decisions (2026-11-28, binding; they supersede every older line)**. Scheduled pipelines return. The existing tick, dispatched about every 5 minutes, runs a project's `[schedule]` slot when it is due and unclaimed, coalesces missed slots into one run and never runs a slot twice. The run state is a ledger of claim assets in a release of a private store repo. The browser `[state]` cookies, the queue and the debug artifacts move from the manual `ci` command to the tick `job`. The manual `ci` workflow keeps its frozen 0.3.0 pin and refuses the new declarations. On the tick only branch `main` of an entry with `secret_branches = ["main"]` gets declared step secrets, state, queue, artifacts, vault values and the claim token.

**How it works**. `plan` (read-only token) reads each scheduled project's pipeline blob on `main` and the ledger, and emits a scheduled row for a due slot (D21, D22). The row reaches `job` through the frozen script as a normal item; `prev_sha == sha` marks it. The run job's `job` prepare mode lists the runner-only names in the names file, the frozen `factory-ci-job.sh` resolves them from the per-repo Bitwarden keys and exports them (D21, D23), and the final `job` takes them once and removes them from its process. Inside the queue turn the job creates the claim (atomic by duplicate name), restores state, runs the pipeline with the step hook, saves state, uploads artifacts on failure, finishes the claim and posts at most one status to `factory-ci-schedule[/<project>]`.

**Releases**. 0.4.0: schedule, ledger, queue, state and artifacts on the tick, all 13 key names reserved, vault declarations parsed and refused with `vault unavailable`. 0.4.1: the vault read (`bw` from the pinned bundle, run by Alpine's `nodejs`). The binary reaches the tick with `gh variable set RUNNER_IMAGE_DIGEST` (D20). 012 merges first (D25).

Everything else (watch list, Loki, fixed public grammar, redactor, engine) stays as in 008 and 010. Interim website logins use 010 step secrets, which the tick now passes on `main`.

## Technical Context

**Language/Version**: Rust 2021, `rust:1.90` in the pipeline, musl static binary in the release.

**Primary Dependencies**: `ring` 0.17.14, `memchr` 2.8.3 and `libc` 0.2.191 (all already locked; direct dependencies add one line each to the root package), and two new lock entries, `miniz_oxide` 0.8.9 and `adler2` 2.0.1. 0.4.1 adds `nodejs` and the `bw` bundle to the image.

**Storage**: GitHub releases of a separate private store repo that holds no code. Local files under `--work`. No database.

**Testing**: `cargo test --locked --lib` in the `rust-ci-runner` pipeline with new mock-server groups in their own processes; the unchanged `factory-ci-selftest.yml` compiles and builds the image on the branch. Queue, state and bundle logic runs against an in-memory `Releases` fake.

**Target Platform**: `ghcr.io/sudtanj/rust-ci-runner`, linux/amd64 and arm64. The tick copies the binary out of the image and runs it on the GitHub-hosted VM (`plan` and `job`); the manual `factory-monorepo-ci.yml` still runs the 0.3.0 image in a container.

**Constraints**: the dev VM has 1 GB and no cargo, docker or builds. The tick `run` job has a 45-minute limit and the runner runs on the VM, not in a container (so `/tmp` is the VM's and bundles still go under the job's work folder). A step container allows 512 processes (`engine.rs`). The frozen job script passes only the declared names and the log credentials to the real run.

**Project Type**: CLI, one binary.

**Performance Goals**: poll 20 s, renew 60 s, settle 3 s. State at most 10 MB, bundle at most 100 MB, memory bounded by a 64 KiB chunk plus the scan window.


**Scale/Scope**: 2 state assets, 10 bundles per project, 50 vault fields per pipeline, no ticket cap, a 5-minute wait.

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Runner and workflow stay here. |
| II Secrets source | Pass. The workflow is unchanged and gains no secret. Every new setting comes from Bitwarden inside the runner. |
| III Public logs | Pass by design. Nine new fixed lines, no name or count. Details go to Loki as step `ci`. No public line names a cron entry, because none exists. |
| IV Simplicity | Pass with justified additions (Complexity Tracking). The critic round removed the schedule file, `due`, the slot marker and the ticket cap; the owner rule removed the schedule, the vault read, the alert file and the agent script. |
| V Tested | Pass. Every FR maps to a test below; tests come first. |
| VI Spec-driven | Pass. Spec, plan, tasks, clarify and analyze all ran on this revision. |

## Owner rule of 2026-11-28 and what it changes

Rule: no file under `.github/workflows/` and none under `.github/scripts/` may change. Correction of the same day: `rust-ci-runner/Dockerfile` and other files outside those folders may change; the tick reads `vars.RUNNER_IMAGE_DIGEST`. The owner's tick decisions (spec, last Clarifications session) bring the schedule, state, queue and artifacts back by routing them through the frozen tick.

| Earlier 011 item | Now | Reason |
|---|---|---|
| Schedule: cron lines, `case`, entry numbers, `--entry` (D5, T010, T023, T030) | Schedule is back as a `[schedule]` table evaluated by `plan` (D5, D21, D22). Cron lines, `case` and `--entry` stay out; the entry is the constant `0`. | needs no workflow edit |
| Workflow: no `concurrency`, 210 minutes, `-e` flags, Environment (D18, T023) | Out. The tick's group, its 45-minute `run` limit and its Environment `factory-ci` apply (D16, D18). | workflow edit |
| Pin of the manual workflow, canaries, first real run (T025, T026, T029, T031) | Pin stays out. Canaries and first real run (T026, T029, T031) run through the tick after the bump. | the tick is live through a variable |
| Tick runner | The variable moves the tick to 0.4.x (T039, D20). | no file changes |
| Watch keys `state`, `vault_items`, first cron line (T030) | The two keys are dropped for good (the pinned 0.3.0 refuses unknown watch keys, `watch.rs:12-19`, `:168`). T030 returns as one toml pull request: `secret_branches = ["main"]` on the `factory-monorepo` entry (0.3.0 parses it, `watch.rs:16`). | D23 |
| Retire the tick (T022, D17) | Reversed: the tick stays and carries the features. | owner decision, feature 012 |
| Vault read, `bw` image, 0.4.1 (D3, T032 to T036) | In scope. Vault owner actions (T037) run through the tick on `main`. | the `Dockerfile` is not covered by the ban |
| Alert-rule file (D10, T027), agent script (D15, T028) | In scope. | not under the banned folders |
| Attempt number and runner image (D19) | Tick runner: `GITHUB_RUN_ATTEMPT`, the row's run id, and the clone image for the helper (D19). | the tick runner runs on the VM, not in a container |
| State, queue, artifacts, vault through `ci` | Out. `ci` refuses the declarations (D24). | frozen pin |

Kept in full: D1 (with tick ids), D2, D4, D6, D8, D9, D11 to D14, the 13 key names, the 9 fault lines (as status texts on the tick), the Loki lines, the scan, the store hygiene.

## Architecture decisions

Evidence for each is in [research.md](research.md). The numbering D1 to D18 is stable for the briefs.

### D1 Queue: release assets in a store repo, unlimited, strict order
Tick note (owner tick decisions): the queue runs inside the tick `job`. `<runid>` in ticket, state and artifact names is the row's `--run-id` (16 hex) and `<attempt>` is `GITHUB_RUN_ATTEMPT` (D19); the regexes of contracts/release-store.md follow. Ticks are serial and a project has one row per tick, so two waiters meet only on a tick rerun (spec R17); the queue stays by the owner's word. The signal handler, installed by `job`, deletes the ticket and then the claim (D21).


A release `ci-queue+<repo>+<project>+<state>` in the store repo (prerelease, never `latest`). A ticket is one asset `t-<runid>-<attempt>-<entry>` (one byte) with label `<runid>.<epoch>`. The full protocol is [contracts/release-store.md](contracts/release-store.md). Points that matter:

- Atomic create: GitHub documents that uploading an asset with an existing filename is an error. Order: the asset id, one integer order for every reader (FR-010). Time: the `Date` header of the answers, never the runner clock (FR-013). A label epoch is the last `Date` the holder saw.
- No ticket cap. A run waits until the budget end minus 30 minutes (5 minutes after its start, D16), then `queue wait expired`. The budget end is the runner start plus 35 minutes unless `FACTORY_CI_BUDGET_END` is set (tests). The one runner-side drop is `queue wait expired`, which the owner alerts on by hand in Grafana. The workflow's concurrency group stays, so GitHub serializes runs of one repo, ref and project and may cancel an older pending run first; the queue adds no recovery for that. A ticket that is deleted and registered again goes to the back (accepted residual).
- Live means `uploaded`, or a non-`uploaded` asset that matches the ticket regex and is younger than 120 s. A non-`uploaded` asset of any other kind is deleted only once it is older than 360 s (the 300 s upload limit plus margin). Retention counts only `uploaded` assets, so two failed uploads never evict the last good state. Stale means a missing, malformed, future (more than 120 s ahead) or 600 s old label. The holder re-lists on each renewal and logs `overlap` when a lower id is live. A canary of 200 trials and two racing runners proves the lag assumptions before the first real run.
- Renewals never pause on a rate limit. Renewals failing for 600 s end the run in this order: set the cancel flag in `RunExtras`, kill the run's step containers by their name prefix (`docker kill`), wait for them to exit, then delete the ticket, print `queue renewal failed` and exit 1; the ticket is never deleted while a step still runs. Other calls pause for `retry-after`, `x-ratelimit-reset` or 60 s, at most 120 s; mutations are 1 s apart.
- Fence: the ticket is checked before and after a state upload; if it vanished after, the new asset is deleted and the run ends `state save failed`.
- Exit paths: a drop guard covers return and panic; a SIGINT/SIGTERM watcher deletes the ticket first, within 5 s, skips the Loki flush, and exits 1. GitHub gives about 10 s after a cancel (7.5 s between the signals plus 2.5 s). Separate tests for each signal.

Rejected: git refs (no payload or timestamp, a reused name hides an old holder), issue comments (needs the issues permission), a queue file through the Contents API (a commit per renewal), releases as tickets (a tag per ticket), the workflow `concurrency` group as the only serializer (it cancels the older pending run; it stays in place anyway because the workflow may not change), a slot-marker asset (a duplicated dispatch runs twice in sequence instead).

### D2 Crypto: `ring`, ChaCha20-Poly1305 in 64 KiB chunks, HKDF

`ring` 0.17.14 is already in `Cargo.lock` (via rustls). HKDF-SHA256 with a fresh 16-byte salt per file and `info` `factory-ci/v1/state` or `/artifact`; input is the decoded bytes of the key (base64 or hex text decoding to 32 bytes or more, 8 or more distinct byte values). STREAM chunks with a final flag; `open` reads one byte ahead, so a plaintext of exactly 64 KiB has one valid encoding. Layout in [contracts/asset-format.md](contracts/asset-format.md). `decrypt-artifact` reads the key from `FACTORY_CI_ARTIFACT_KEY`. Rejected: `age`, `openssl enc`, an extra AEAD crate, AES-GCM, PBKDF2 (open point O3).

### D3 `bw` (0.4.1 only)

The npm-build bundle of `bitwarden/clients` release `cli-v2026.10.1` (sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3`, 5.1 MB, matches a download) run by Alpine 3.23 `nodejs` 24.22.3 (installed 52.3 MB plus about 15 MB of libraries, index read today) behind a two-line `/usr/local/bin/bw`. The standalone binary is 143 MB, glibc-linked (`readelf` shows interpreter `/lib64/ld-linux-x86-64.so.2` and `libstdc++`, `libc.so.6`); rejected. Estimated image growth is about +86 MB; the PR records the measured size and the pull time. `RUN bw --version` on amd64 blocks; the arm64 run under QEMU does not.

Sequence per run: `login --apikey` (only `BW_CLIENTID`, `BW_CLIENTSECRET`), `unlock --passwordenv BW_PASSWORD --raw`, `list items --organizationid --collectionid` (only `BW_SESSION`, `BW_NOINTERACTION=true`), `logout`, then remove the data dir (a 0700 folder on the tmpfs) and check it is gone, all before the first container. The runner lists items and filters by exact name, so no item name is in argv. An item outside the allowlist in the answer is `vault unavailable`. The `bw` server is the US cloud; with `FACTORY_CI_BWS_SERVER_URL` set and a vault declaration the run ends `vault unavailable`. No new key.

### D4 One list of runner keys, one `bws` call

On the tick the runner never calls `bws`; the same names arrive as variables (D7) and `take_runner_env` applies the same value rule. The single-call rule (I17) holds for `ci` only.
`store::RUNNER_KEYS: [&str; 13]` replaces 010's `REQUIRED_KEYS`; `REQUIRED_KEYS` becomes its first four (read at start). The others are held from the same single `bws secret list` call: `fetch_with` keeps their rows next to the allowlisted step rows in `Held`, and `Held::take`-style accessors return them on need. Consumers: `valid_secret_name`, `watch::parse`, `Held::take` (refuses runner keys by name), the runner-key accessor, and the `log.rs` fault names. A table test walks the list; membership asserts, never counts. New data goes into `Args`, not new parameters.

### D5 Schedule: `[schedule]` with an interval, UTC, epoch-aligned slots

Q2 of the brief. A table in `.factory-ci.toml`:

```toml
[schedule]
every  = "6h"    # ^[1-9][0-9]{0,4}[mhd]$, 15m to 31d
offset = "3h"    # optional, ^(0|[1-9][0-9]{0,4}[mhd])$, smaller than every
```

Interval, not cron: no parser, no time zone, no daylight saving, one formula. `due(now) = now - ((now - offset) mod every)` in seconds since the Unix epoch, UTC; `every = "1d"`, `offset = "3h"` is 03:00 UTC daily. The floor is 15 minutes: the tick runs about every 5 minutes, so a 5-minute schedule fires on nearly every tick and fights the queue, and the store allows 500 content writes an hour (research R-A: one run costs about 10 to 20 writes). The owner asked for at least 5 minutes; 15 satisfies it. Unknown keys, a bad unit, `0m` for `every`, a value out of range, `offset >= every`, a second table: refused (FR-001). `schedule.rs` is pure (parse and `due`), with no clock inside; `now` is an argument. Rejected: cron (a parser, daylight saving to support or refuse), a per-weekday list (no need yet).

### D6 Where the tests run

No-server modules (`seal_tests`, `scan_tests`, `bundle_tests`, `state_tests`, `queue_tests` with at most 6 joined threads, `vault_tests`) run in the first command. `releases_http_tests` and `tick_state_tests` start servers and get their own process groups in `.factory-ci.toml`, with matching `--skip` flags (`--skip ci_tests::` does not match `tick_state_tests::`).

### D7 Order inside the tick `job` (extends 010 and 012)

The tick `run` item is two process calls of the same binary (frozen `factory-ci-job.sh:55` and `:117`): prepare mode, then the real run.

Prepare mode (a cleared environment with the clone token only, `job.sh:47-58`): clone, load the pipeline (a parse fault is the reason line), run the 010 and 012 checks, then write the names file: the names the pipeline declared (010 route, only when the branch is `main`; on another branch a declared step secret keeps `NO_JOB_SECRETS`), plus, only when the branch is `main` and the pipeline needs the store (D23), `FACTORY_CI_STATE_TOKEN` and per need `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, and in 0.4.1 the five vault names. 012's two Docker Hub names join for image rows. The script then demands `secrets_ok` (`job.sh:68-71`), resolves each name from `<repo><SEP><NAME>` (`secrets.sh:71`), masks and exports it (`job.sh:98-101`), unsets the bws token and execs (`job.sh:115-117`).

Real run (`job`, no `--names-file`):

1. Flags and target checks (`target()`), `require_pinned` clone image, `log_config`.
2. Intake: take the runner-only variables from the process once, then remove them from it, before any thread or child exists (the way `run_ci` removes `BWS_ACCESS_TOKEN`, `main.rs`). A missing or malformed value, equal keys, or a `FACTORY_CI_STATE_REPO` that differs from the derived name: the status reason `state store not configured`, no ticket. `log.add_secrets` gets the token, both keys, the derived repo name and `base64("x-access-token:" + token)` before any line that can carry a value or any store call (FR-061, FR-066).
3. Checkout and pipeline checks as before (`attempt`).
4. Scheduled row (`prev_sha == sha`): `due` from the pipeline's `[schedule]` and the store's `Date`; nothing due, or the claim exists: end silently with exit 0 and no status.
5. `[state]`: register in the queue and wait (`queue wait expired` is the reason; the slot stays due because no claim exists yet); inside the turn restore into memory (`state restore failed`).
6. Scheduled row: create the claim (a 4xx ends without a run).
7. `run_pipeline` with `RunExtras` (deadline, step hook, cancel flag).
8. After: helper `chown` and `chmod` (clone image), build, scan, encrypt, upload artifacts when due, save the captured state when the pipeline passed, finish the claim (label), sweep, release the ticket, remove the checkout, post the final status (push: the usual context; scheduled: D21 rules).

A row without the store (not `main`, or a pipeline with no store need) runs steps 1, 3, 7 and the usual post, exactly as before 011.

Test seams: `stateful.rs` holds `StatefulSeams { timing, exit, uploads_base, releases }` (the old `Seams` of `ci.rs` is not extended). Production builds it from the process; tests inject a fake clock, a recording exit, the upload base and `FakeReleases`.

### D8 State reaches one step only

A step key `state = true` marks the one step. `[state]` needs exactly one such step, and every other step must be a transitive dependency or dependent of it, so none runs beside it. `RunCtx` gains a `StepHook` (`before`, `after`) called on the step's thread around `docker run`, before the step counts as finished. `before` writes the in-memory plaintext at the declared path (mode 0600, canonical path inside the checkout, `O_NOFOLLOW`; nothing for a first run). `after`, pass or fail, runs the chown helper, reads the file by the rules of D11, keeps the bytes in memory, and removes the file; a removal failure is `runner fault`. The declaring step should end with `chown` to the workspace owner; the helper is the backstop (`chown -hR` and `chmod u+rX`, never `a+r`) and runs the runner's own pinned image (found by D19), not the step image. The state step runs as root like every step; the README says so and tells the owner not to place a state step after an install step that runs package scripts. Rejected: a bind mount of a runner-private file (an empty file where FR-021 promises none; `EBUSY` on rename), a pipeline-wide restore (the 002 finding), a per-step workspace copy.

### D9 Store repo

A separate private repo that holds no code and no workflows, one initial README commit, no tag rulesets that block creation, immutable releases off. Its `owner/name` is `FACTORY_CI_STATE_REPO` (Bitwarden). `FACTORY_CI_STATE_TOKEN` is scoped to it only; the agent's read token to it plus Loki. Tags, names and regexes are in the release-store contract; the `+` delimiter and the `-` root project make the tag mapping injective, with a collision test.

### D10 Monitoring

[contracts/loki-lines.md](contracts/loki-lines.md) fixes every Loki line, each with a test. `rust-ci-runner/alerts/factory-ci.yaml` holds the Grafana rules, no-data set to Alerting: a missing `pipeline passed` for `sudtanj/factory-monorepo` over 24 hours; any fault line over 1 h; `queue wait expired`; `queue renewal failed`; any `overlap`; more than one `restore none` in 24 h; more than two `restore failed` in 24 h; any `stale break`; `rate limited` more than 3 times in 1 h. A Rust test checks that every string in the file is in the contract or in `FAULTS`. The README carries a runbook for `state restore failed` (delete the newest state asset, or all of them to start empty). Ledger rules: `claim created` missing for a scheduled project over twice its `every` (no data is Alerting), any `schedule ledger unreadable`, any `claim stale break`, any `claim finish failed`.

### D11 Filesystem trust

The state hook and `bundle::build` canonicalise the declared path, require `starts_with(canonical checkout)`, `lstat`, open with `O_NOFOLLOW|O_NONBLOCK`, and judge size by bytes read. Before `seal`, the runner scans the state plaintext for every known secret form; a hit is `state save failed`. A test points the state path through `/proc/self` and a FIFO.

### D12 Names and maps

A name in both a `secrets` list and a vault `env` is refused pipeline-wide. Vault values live in a separate `(item, field)` map (0.4.1). The gate is the literal `secret_branches = ["main"]` (D23); no `state` or `vault_items` key exists. Vault item names in the public watch file are opaque (`ci-login-1`).

### D13 Scan scope

The scan set is `Log::secret_forms()` minus the numeric `FACTORY_CI_LOG_USER` (an identifier that matches digits by chance) (010's `Redactor::forms`: the value, three base64 alignments, URL-safe base64, percent-encoding, JSON escape, hex, trimmed lines) plus the FR-050 state strings. The scan reads every zip member, `trace.network` included. Members are allowed only as text, JSON, zip (stored or deflate), PNG or JPEG, judged by content; anything else, and any `.html` member, is refused (the Playwright HTML reporter stays off; 002 uses its own reporter). Tar names with `..`, a leading `/` or a control character are refused. A canary on one real Playwright login-failure trace from the 002 fixture runs before the first real run.

### D14 Store hygiene

Fixed hosts, redirects off, download `Location` only https on `*.githubusercontent.com`, name and size checks before a download, pagination with a 300-asset fail-closed bound, one retry of a restore on a 404.

### D15 Agent kit

`rust-ci-runner/scripts/open-artifact.sh` downloads the runner binary of a release and runs it only if its sha256 equals the value pinned in the script, decrypts, checks the exit status, extracts with `tar -x --no-same-owner` into a new empty directory, and prints files stripped of control characters and ANSI, size-capped, under an untrusted-data header. The README names the pins that move per release (image digest, script sha) in a release checklist line.

### D16 Deadline

Both workflows the runner can run in have a 45-minute job limit: `factory-monorepo-ci.yml` (unchanged) and the tick's `run` job (`factory-ci-tick.yml:117`). `JOB_LIMIT` is a Rust constant of 45 minutes next to the budget code; a test reads both workflow files read-only (through `CARGO_MANIFEST_DIR`) and asserts each `timeout-minutes` equals it, so a later edit turns the test red. If the checkout does not hold a file, the test reports it and the README line stays the only guard.

`budget_end` is read through `StatefulSeams.timing` (tests set it; production leaves the variable unset): the value of `FACTORY_CI_BUDGET_END` if set, else the runner start plus 35 minutes (the limit minus 10 for start-up and cleanup). On the tick the runner starts after the checkout, the runner fetch and the bws install (about 1 to 2 minutes), so the 10-minute margin is real. Pipeline deadline `= min(now + 30 min, budget_end - 15 min)`; the 15 minutes are 5 for the artifact scan and upload, 5 for the save and 5 for cleanup. The queue wait ends at `budget_end - 30 min` (5 restore, a 10-minute minimum pipeline window, the 15-minute reserve), which is 5 minutes after the start. A hold is at most 40 minutes. 012's image rows keep their own `JOB_DEADLINE` (40 minutes); the stateful budget applies to rows that use the store. A pipeline that reads the vault (0.4.1) spends up to 5 more minutes before its first step, so a late winner's window can drop to 5 minutes. The claim is written at turn time, so a live claim is never older than 40 minutes; the stale rule uses 3600 s (D21).

### D17 Rollback and the tick

Rollback goes in reverse of the rollout (Rollout section): remove the project toml keys (`[schedule]`, `[state]`, `[artifacts]`, `vault`, `state = true`) first, because 0.3.0 refuses them; then set the recorded old digest (`gh variable set RUNNER_IMAGE_DIGEST --body <old>`); then revert the Rust and `Dockerfile` commits; the `secret_branches` toml change may stay (0.3.0 parses it). The tick workflow `factory-ci-tick.yml`, its scripts and its settings are not touched (FR-060). The ledger and state releases stay in the store repo; they hold 1-byte assets and ciphertext.

### D18 Environment (answered, no change)

Scheduled runs use the tick `run` job's Environment `factory-ci` (`factory-ci-tick.yml:118`) like push runs; nothing in the workflow changes. The owner checks that the Environment allows deployments from `main` and has no required reviewer (a reviewer would hold every scheduled run); this is already true for push rows on `main`. Scheduled rows never use `run-docker` (spec: `allow_docker = false` only).

### D19 The tick runner's attempt, run id and helper image

The tick copies the binary out of the image (`factory-ci-tick.yml:152-160`) and runs it on the VM, so there is no runner container to `docker inspect`. The earlier D19 (container name, `docker inspect`) applies to no path any more and `Engine::inspect_self` is dropped. Instead:

- attempt: `GITHUB_RUN_ATTEMPT` (set by Actions, inherited by the `exec` at `job.sh:117`) when all digits, else `1`;
- run id for ticket, state, artifact and claim names: the row's `--run-id` (16 hex, set by `plan` phase C in `finish`, `plan.rs:414-450`, unique per row), so matrix siblings of one workflow run never share a name; the regexes of contracts/release-store.md take `[0-9a-f]{16}` where they took digits;
- helper image for the `chown -hR` and `chmod -R u+rX` backstop (D8): `FACTORY_CI_CLONE_IMAGE`, already pinned and passed to the `job` step (`factory-ci-tick.yml:200`, `require_pinned` in `job.rs`). The helper runs with `--network none`, no `-e` and `--entrypoint chown` then `--entrypoint chmod`. A clone image that cannot run them ends the run with `runner fault` before any ticket; a test pins the entrypoints and the quickstart checks the real image (O21).

Rejected: reading `/proc/self/cgroup`, guessing an image constant (it moves with the pin), a second variable (the frozen YAML cannot pass it).

### D20 Shipping through the tick

`.github/workflows/factory-ci-tick.yml` runs `plan` and `job` from `ghcr.io/<owner>/rust-ci-runner@${{ vars.RUNNER_IMAGE_DIGEST }}` (`:37`, `:57`, `:133`, `:151`). `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<digest>` moves the tick to a new runner and needs no file edit. The manual `factory-monorepo-ci.yml` hardcodes its image, so it stays on 0.3.0.

Order matters because 0.3.0 refuses what 0.4.x accepts (spec, Clarifications):

1. 012 merges first (D25), then this feature's code on top.
2. Tag 0.4.0; verify the digest; the tick-path tests already ran in CI.
3. T039: record the old digest, set the new one, watch two ticks: `fail_tick=0`, the same push statuses, no new public line, no scheduled row yet (no project declares a schedule).
4. Pull request T030: `secret_branches = ["main"]` on the `sudtanj/factory-monorepo` entry plus the updated comment block of `.github/factory-ci-repos.toml`. Safe under 0.3.0 (`watch.rs:16`, `:203-205`: parsed; the 0.3.0 `plan` ignores it at `plan.rs:310`; the frozen manual `ci` honours it for declared step secrets only). It merges after step 3, as 012 does with its watch edit (012 decision 10), so a rollback of the variable never meets a watch file it cannot read.
5. Owner actions (store repo, token scopes, Bitwarden keys, Environment check).
6. The canary on the real store (T029), then the project toml change in `factory-monorepo` (`[schedule]`, `[state]`, ...). From this moment the frozen manual `ci` cannot run that project: `pipeline.rs:10` and `:29` deny unknown fields.

What the bump gives: 0.4.x plans, runs and reports as before for every row that declares none of the new keys. 012 relies on the same tick; tell its owner before step 3 and use a quiet moment. Rollback: D17.

### D21 The ledger and the claim protocol (answers Q1, Q3, Q4 of the brief)

**Q1, who writes the claim.** The plan job cannot hand the runner a store write token. Evidence in the frozen files at `origin/main` 8660df5: the plan step's environment is `FACTORY_CI_TOKEN(_DOCKER)`, the two bws tokens, the blob keys and a few variables (`factory-ci-tick.yml:96-110`); `factory-ci-plan.sh:35-37` runs the plan runner with the bws tokens and blob keys removed; the only values taken from the bws list are the three log keys, named in code (`factory-ci-secrets.sh:84-100`, exported at `factory-ci-plan.sh:66`); the list itself stays in a shell variable. No other value crosses. The `run` job does have a route: `factory-ci-job.sh:55-66` (prepare mode lists names), `:68-71` (any name needs `secrets_ok`, else `refuse`), `:73-93` (one bws list, each name looked up as `<repo><SEP><NAME>` when the row has a repo id, `factory-ci-secrets.sh:71`, or as a shared name, `:72-75`), `:98-101` (mask and export), `:115-117` (unset the bws token, `exec` the runner). So the run job writes the claim, with `FACTORY_CI_STATE_TOKEN` delivered as a listed name. `plan` sets `secrets_ok` (D22); 012 changes the same line for image rows (D25). A shared name would need the entry's `secrets` list, which `watch.rs:209-215` forbids for runner keys; the per-repo route needs only the entry's numeric `id`, which `factory-monorepo` has.

**Release and names.** Store repo `<owner>/factory-ci-state` (derived in code from the watched repo's owner, used by both `plan` and `job`; `plan` cannot read Bitwarden and a new watch key is impossible, D23). Release `ci-sched+<owner>_<name>` of the watched repo, prerelease, never `latest`. One asset per claim, 1 byte, named `claim-<h16>-<slot>-<n>` where `<h16>` is the first 16 hex characters of SHA-256 over `project`, a NUL byte and `branch`, `<slot>` is the decimal epoch of the slot, and `<n>` is `0` or `1`. Regex `^claim-[0-9a-f]{16}-[0-9]{1,12}-[01]$`. Label `<o>.<epoch>.<runid>` (`^[rpfe]\.[0-9]+\.[0-9a-f]{16}$`): `r` running, `p` passed, `f` failed, `e` closed with an error. Only `uploaded` assets count.

**Plan side (read only).** After `GET /repos/<store>` answers `private: true`, list the release assets (paged, at most 300). For each scheduled project: `due` (D5) from the `Date` of the list answer; the newest claim of its `<h16>`. No claim at slot `>= due`, or the newest claim is stale, plans one row. Stale: label missing or malformed, epoch more than 120 s ahead of `Date`, or `r` with the epoch 3600 s or more behind `Date`. A stale `n = 0` is planned again as `n = 1`; a stale `n = 1` plans nothing and posts one `error` status. Any read failure plans nothing for that repo and reports one problem (fail closed: a blind plan would re-run every project on every tick).

**Job side (the only writer).** After the queue turn: recompute `due` from the `Date` of the store; list; apply the same stale rule; create `claim-<h16>-<due>-<n>` with label `r.<epoch>.<runid>`. A 4xx means another run owns it (list to confirm): end silently. Atomic because GitHub refuses a second upload with the same name (research R-A; O1 proves it on the real store). The claim is never created before the queue turn and never when the plan's slot is no longer due, so a run that never starts leaves nothing and the slot stays due. At the end: `PATCH` the label to `p.` or `f.` (3 tries, 1 s apart, else Loki `claim finish failed`), then the cleanup: delete, for the same `<h16>`, all but the 2 newest `uploaded` claims, and any claim of any `<h16>` whose label epoch is older than 90 days. SIGTERM and SIGINT delete the own claim within 5 s (the signal path of D1). A takeover of a stale claim needs no delete: `n = 1` is a different name.

**Why two ticks and parallel jobs stay safe (Q4).** Ticks never overlap: the group `factory-ci-tick` with `cancel-in-progress: false` (`factory-ci-tick.yml:9-11`) and the `verdict` job that needs every `run` job mean the next `plan` starts after the last `run` job of the previous tick ended. Within a tick there is at most one row per (repo, branch, project), and a claim has one creator who is its only writer. No shared document is read, modified and written, so no lost update can occur. Parallel jobs write disjoint assets; the sweep touches only its own `<h16>` plus assets older than 90 days, which no live run owns. A tick rerun and a manual dispatch meet the duplicate-name refusal. The ledger stays bounded: at most 2 `uploaded` claims per `<h16>`, a 90-day sweep for deleted branches and projects, at most 300 assets (fail closed above, so a runaway never grows silently), 1 byte each.

**Q3, statuses.** Scheduled rows are marked by `prev_sha == sha` (a push row's `prev_sha` is an ancestor from `walk.rs` or empty; a test pins both). Phase C (`plan.rs:414-450`) posts no `pending` for them and does not count them against `write_cap`. The job posts one final status to context `factory-ci-schedule` (root) or `factory-ci-schedule/<project>`. The push contexts are exactly `factory-ci` or `factory-ci/<folder>` (`projects.rs:22-26`), so no push context equals or starts with the scheduled one, and the push walk, which reads exact contexts (`walk.rs`), never sees it. The scheduled status posts only when the outcome differs from the previous claim's outcome of that `<h16>` (or none exists) and the latest own status of that context on the sha is 3600 s or more old (one `status` read, cached per run). A 422 (the cap of 1000 per sha and context) is logged and ignored. A repeated pass or a repeated fail posts nothing, so a 15-minute schedule on an unchanged sha posts at most 24 times a day and only on a flip. The scheduled run on an unchanged sha runs because `plan` decides it from the ledger and does not read the push statuses. Push wins: when a push row for the same (repo, branch, project) is in the tick, the scheduled row is dropped for that tick and runs on the next one.

Rejected: commit statuses as the ledger (the cap of 1000 per sha and context, no atomic create), one shared JSON asset (read-modify-write races across parallel jobs), a pending status on the push context (it overwrites the push result and burns its cap), a plan job that writes (no write token reaches it), a row per scheduled project on every tick with the check in the job (a hosted VM per project per tick for nothing).

### D22 Plan changes

`plan.rs`, `github.rs`, `fake.rs`, `main.rs` (no new flags; the frozen scripts call `plan` by the same flags).

- `Api` gains `file(repo, sha, path) -> Result<Option<Vec<u8>>, ApiError>` (the contents blob of one `.factory-ci.toml`, 64 KiB limit) and a read-only ledger reader behind a small trait in `ledger.rs` (`list(store, tag) -> (Date, Vec<Asset>)`) that uses `FACTORY_CI_TOKEN` over `api.github.com`, redirects off, 30 s limit. The plan never calls `releases.rs` (the state-token client).
- `read_repo`: after `projects::discover` for the tip of `main`, for an entry with `allow_docker = false`, `secret_branches` exactly `["main"]` and a private repo (`repo.private`), each non-`docker/` project: read its pipeline blob (at most 1 per project per tick, at most 40 per tick, over that the repo is `unchecked`), parse only `[schedule]` with `schedule.rs`, read the ledger once per repo, decide (D21). Invalid schedule: a problem line and one `error` status on the scheduled context when its latest own status is not already that error.
- `Item` gains nothing serialisable that the frozen script reads; scheduled rows set `prev_sha = sha`, `secrets_ok = true` and `retry = false`. `secrets_ok` for every row becomes `store_allowed(entry, branch, repo.private) || image_rule` where `store_allowed` is the D23 rule (the `image_rule` is 012's expression at `plan.rs:310` after its S2).
- `finish`: scheduled rows skip the `pending` post and the write cap; `order` places scheduled rows after push rows; a scheduled row whose (repo, branch, project) has a push row is dropped; the summary counts `schedule ignored`, with no names.
- Plan Loki line (the existing heartbeat) gains counts only: `schedule due <n> rows`, `schedule ledger unreadable`, `schedule ignored <n>`.

### D23 The gate: `main` and a literal `secret_branches`, no new watch keys

Q for Decision 2. `WatchEntry::store_allowed(branch, private)` is true when `private`, `branch == "main"` and `secret_branches == Some(["main"])` exactly (one literal item; a glob or a longer list does not count; this differs on purpose from `secrets_allowed_on`, `watch.rs:39-43`, which keeps globs for the manual `ci`). `plan` uses it for `secrets_ok` and for schedules. Prepare mode lists runner-only names only for `branch == "main"` (the same literal, repeated in the runner because prepare has no entry; `secrets_ok` is the entry-level check at `job.sh:69`). Consequences: a `main` row of an entry without the literal gets `secrets_ok = false`; if its pipeline needs the store the frozen script refuses the item (`secrets not allowed on this branch`), a loud signal; a non-`main` row lists nothing and runs without state, queue, artifacts, vault and schedule, and declared step secrets keep `NO_JOB_SECRETS`.

The 0.3.0 check the brief asks for: the key `secret_branches` is already in `ENTRY_KEYS` (`watch.rs:12-19`, `:16`) and parsed at `:203-205`, so 0.3.0 does not refuse it. 0.3.0 `plan` ignores it (`plan.rs:310` is `false`), and the frozen manual `ci` honours it for declared step secrets (010). Keys that 0.3.0 does refuse: every other new watch key (`:168`, "unknown key") and every new pipeline key (`pipeline.rs:10`, `:29`, `deny_unknown_fields`). Therefore `state` and `vault_items` are dropped, and the toml pull request for `secret_branches` merges after the variable bump (D20), the 012 pattern (012 plan decision 10, `wt-infra-012/specs/012-docker-images/plan.md:23`).

### D24 The manual `ci` refuses the new declarations

`ci.rs` gains one check after `load_pipeline`: `[state]`, `[artifacts]`, `[schedule]`, `state = true` or a vault list ends `[runner] state not allowed` before any store call or container (FR-069). The earlier D7 wiring of `ci` (queue, restore, store identity, vault) is not built. `Seams`, `Args` growth and `tick_state_tests` for `ci` are dropped; the shared post-checkout code lives in `stateful.rs`, called by `job`.

### D25 Merge order with 012 (Q5)

012 first. Its `plan.rs` change is the `secrets_ok` and `allow_docker` lines (`plan.rs:307-310`, `wt-infra-012` commit cd36b5f and 6631734), its `job.rs` change is `target()` (`project_path_ok`), `secret_names` (writes the two Docker Hub names for image rows, `NO_JOB_SECRETS` still first), `attempt` (shared `JOB_DEADLINE` of 40 minutes, `publish`) and `token_values`; its `engine.rs` change adds the publish helpers; its `projects.rs` change adds `docker/<name>` discovery and `context`. 011 touches the same four files: `plan.rs` (rows, `secrets_ok` expression, phase C), `job.rs` (`secret_names`, `attempt`, `RunExtras`), `engine.rs` (`StepHook`, `kill_run`, `own_paths`) and, if needed, `projects.rs` (a `schedule_context` function next to `context`). Rules: 011 code tasks start from `main` after 012 merged; the `secrets_ok` expression becomes `store_allowed || image_rule`; the names file is the union of 010, 012 and 011 names; image rows never get schedule or store names; 011's deadlines apply to store rows and 012's `JOB_DEADLINE` to image rows. The 011 documents (spec, plan, tasks) merge on their own now. If 012 slips, 011's T001 to T018 that do not touch those four files (seal, releases, queue, bundle, scan, state, store, pipeline) may proceed on a branch from current `main`, and T019, T043, T044 wait.

## Files

### Added (`rust-ci-runner/src` unless noted)

`seal.rs`, `releases.rs` (the only code that calls api.github.com with the state token), `queue.rs`, `signal.rs` (the one `unsafe`: a `libc::sigaction` call that stores an atomic), `state.rs`, `bundle.rs`, `scan.rs`, `stateful.rs` (post-checkout phase, called by the tick `job`), `schedule.rs` (parse and `due`, pure), `ledger.rs` (claim names, labels, stale rule, create, finish, sweep, and the read-only plan reader), `vault.rs` (0.4.1, the only new file that names the vendor), one `*_tests.rs` per module (`schedule_tests.rs`, `ledger_tests.rs`, `plan_schedule_tests.rs` included), `tick_state_tests.rs` (tick `job` integration, own process group), `releases_http_tests.rs`, `rust-ci-runner/alerts/factory-ci.yaml`, `rust-ci-runner/scripts/open-artifact.sh` with `rust-ci-runner/scripts/test-open-artifact.sh`, and the spec artifacts.

### Changed

| File | Change |
|---|---|
| `store.rs` | `RUNNER_KEYS` (13) replaces `REQUIRED_KEYS`, which becomes its first four. A runner-key reader for the tick (`take_runner_env`: take from the process once, remove, check shape, key format and equality). The `ci` bws read keeps its 010 shape (the nine other rows are held for reserved-name checks only). |
| `watch.rs` | `WatchEntry::store_allowed(branch, private)` (D23). No new key. Runner-key names in `secrets` refuse the file. |
| `pipeline.rs` | `[state]`, `[artifacts]`, `[schedule]`, `Step.state`, `Step.vault`, the rules of data-model.md, `valid_secret_name` uses `RUNNER_KEYS` and refuses the four prefixes in `env`, `secrets` and vault `env`. |
| `plan.rs`, `github.rs`, `fake.rs` | D22: `Api::file`, due rows, `secrets_ok`, scheduled rows without `pending`, order, summary counts. |
| `redact.rs`, `log.rs` | `FAULTS` gains nine lines as status reason texts and `public_form` the queue line (membership asserts); nothing new in the redactor. |
| `engine.rs`, `job.rs` | `StepHook` in `RunCtx`; `Engine::kill_run(prefix)`; `Engine::own_paths` (clone image, `chown -hR`, `chmod u+rX`); `RunExtras { deadline, hook, cancel }` for `run_pipeline`; `job.rs` prepare writes the names (D7), the real run takes the runner-only variables once, then the stateful phase (D7); `local` ignores the new keys. |
| `ci.rs` | One refusal of the new declarations (D24). Nothing else. |
| `main.rs` | `decrypt-artifact`; `job` installs the signal handler; usage text. |
| `util.rs` | `parse_http_date`. |
| `projects.rs` | `schedule_context(project)` next to `context` (after 012). |
| `.factory-ci.toml` | Process groups and the `--skip` flags. |
| `Cargo.toml`, `Cargo.lock` | 0.4.0 (then 0.4.1); `ring`, `memchr`, `libc`, `miniz_oxide`; two new lock entries by hand. |
| `rust-ci-runner/tests/no_vendor.rs` | Allow `vault.rs` (0.4.1). |
| `rust-ci-runner/Dockerfile` | 0.4.1: Node, the `bw` bundle, wrapper, `RUN bw --version`. |
| `.github/factory-ci-repos.toml` | T030, its own pull request after the bump: `secret_branches = ["main"]` on the `factory-monorepo` entry and the comment block. |
| `README.md`, `rust-ci-runner/README.md` | Docs last. |

### Not changed

`ship.rs` keeps its interface. No file under `.github/workflows/` or `.github/scripts/` changes (owner rule); `git diff --stat origin/main -- .github/workflows .github/scripts` is empty before every commit. The only `.github` file that changes is `.github/factory-ci-repos.toml` (T030).

## Data and API contracts

[contracts/config-files.md](contracts/config-files.md), [contracts/release-store.md](contracts/release-store.md), [contracts/asset-format.md](contracts/asset-format.md), [contracts/loki-lines.md](contracts/loki-lines.md), [data-model.md](data-model.md).

## Engineer briefs

Precondition: 010 is merged to `main` and released (0.3.0, in the tick today), and 012 is merged to `main` (D25). Implementation branches start from that `main`, not from this documentation branch (it is behind `main`). Names used here are 010's and 012's real ones: `REQUIRED_KEYS`, `fetch_with`, `Held::take`, `StepSecrets`, `NamedError { Count { index, found }, Unusable { index } }`, `Redactor::add`, `Redactor::forms`, `Log::add_secrets`, `Log::secret_forms`, `Log::longest_secret`, `WatchEntry::secrets_allowed_on`, `Args`, and from 012 `image::publishes`, `projects::project_path_ok`, `secret_names`, `JOB_DEADLINE`. If a name differs after they land, follow them. Do not assume a `FAULTS` length.

Order. T001 first, alone (skeleton, dependencies, lock, and the `Releases` trait with its types). T002 to T004, T009 and T040 (tests first, 0.4.0) run in parallel after T001; T005 (the fake and the HTTP tests) runs after T001, and T006, T007, T008 run after T005 because they use `FakeReleases`. T043 (plan tests) runs after T005 and T040. Code: T011 to T014, T018 and T041 in parallel after T001 and their own tests; T042 after T014 and T041; T015, T016, T017 as before (T014 before T015, T016, T017; T018 before T015 and T017; T013 before T017 and T020); T044 (plan) after T042, T043 and T011; T045 (`ci` refusal) after T012; T019 (wire the tick `job`) after T011 to T018, T042 and the 012 merge; T020 after T013 and T015; T021 after T019, T020, T044 and T045. T024 (gate and tag) follows the checkpoint; T026, T029, T030, T031, T039 follow T024 in the order of D20; T027 and T028 after T024. Phase 5 (0.4.1, T032 to T036) starts after T024: T032 and T034 in parallel; T033 after T032; T035 after T032 to T034; T036 after T035; T037 after T036; T039 repeats after T036. T038 (docs) is last. T010, T022, T023 and T025 are dropped (table below).

### T001 Skeleton, dependencies, lock
- Goal: new empty modules compile; dependencies and lock proved alone; the `Releases` trait exists for the tests.
- Files: `lib.rs`, new empty files, `Cargo.toml`, `Cargo.lock`.
- Acceptance: `lib.rs` declares `seal`, `releases`, `queue`, `signal`, `state`, `bundle`, `scan`, `stateful`, `schedule`, `ledger`, and test modules `seal_tests`, `releases_http_tests`, `queue_tests`, `state_tests`, `bundle_tests`, `scan_tests`, `schedule_tests`, `ledger_tests`, `plan_schedule_tests`, `tick_state_tests` (`vault` and `vault_tests` come in T033). `Cargo.toml` adds `ring = { version = "0.17", default-features = false, features = ["alloc"] }`, `memchr = "2"`, `libc = "0.2"`, `miniz_oxide = { version = "0.8", default-features = false, features = ["with-alloc"] }`. `Cargo.lock`: the four names join the root package dependencies alphabetically; new entries `adler2` 2.0.1 (checksum `320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa`) and `miniz_oxide` 0.8.9 (checksum `1fa76a2c86f704bdb222d66965fb3d63269ce38518b83cb0575fca855ebb6316`, dependency `adler2`).
- Also: `engine.rs` declares `trait Killer { fn kill_run(&self, prefix: &str) -> Result<(), String>; }` (the kill seam; `Engine` implements it in T018, T005 provides a recording `FakeKiller` that shares an event log with `FakeReleases`). `releases.rs` declares `trait Releases` and its types (`Asset`, `ReleaseError`, the list and renew shapes) with unimplemented bodies, so T005 to T008 compile against it; T014 fills in `HttpReleases`.
- Constraints: no cargo here; check both checksums against `https://index.crates.io/mi/ni/miniz_oxide` and `https://index.crates.io/ad/le/adler2`. T001 lands alone on a branch first so the selftest proves `--locked`.

### T011 Runner keys and the store gate (FR-019, FR-020, FR-034, FR-059, FR-066, FR-067)
- Goal: thirteen runner keys, one list, the intake of the tick job, the literal gate.
- Files: `store.rs`, `watch.rs`.
- Acceptance: `RUNNER_KEYS` has the thirteen names; `REQUIRED_KEYS` is its first four and `fetch`, `parse_list` behave as before. `take_runner_env(&mut env_source)` reads the listed runner-only variables once, removes them from the process, and applies the 010 value rule (not empty, no CR, LF or NUL, no leading or trailing whitespace, at least 4 characters); a key is base64 or hex text decoding to 32 bytes or more with 8 distinct byte values; the two keys differ whenever both are needed; a missing or bad value is `state store not configured`. `FACTORY_CI_STATE_REPO`, when present, must have the shape `owner/name` and equal the derived `<owner>/factory-ci-state`. `Held::take` refuses any runner-key name. `WatchEntry::store_allowed(branch, private)` is true only for `private`, `branch == "main"` and `secret_branches == Some(["main"])` exactly; a glob (`ma*`), a longer list and `None` are false; `secrets_allowed_on` is unchanged. `watch::parse` adds no key (a test asserts `state` and `vault_items` stay unknown keys), and runner-key names in `secrets` refuse the file.
- Constraints: no vendor words in `watch.rs`. Membership asserts, never counts. No second `bws` call in `ci`.

### T012 Pipeline declarations (FR-017, FR-018, FR-042, FR-059)
- Goal: parse and validate `[state]`, `[artifacts]`, `state = true`, `vault`.
- Files: `pipeline.rs`.
- Acceptance: the rules of data-model.md (the `[schedule]` table through `schedule::parse`, FR-001), including: exactly one `state = true` step, ordering with every other step, artifact paths by component against the state path, name checks (`BW_`, `BWS_`, `BITWARDENCLI`, `FACTORY_CI`, runner keys) in `env`, `secrets` and vault `env`, a name in both a `secrets` list and a vault `env`, vault limits (20 and 50), `docker = true` with a vault list or with `[state]`, a project with `..` or ending `.` or `.lock` with `[state]` or `[artifacts]`, a tag over 200 bytes. `vault_wants()` returns each `(item, field)` once. Grep the fixtures and `rust-ci-runner/.factory-ci.toml` for `env` keys with the four prefixes.
- Constraints: `pipeline.rs` may name the vendor. Symlink and containment checks happen at run time (D11).

### T013 Seal (FR-027)
- Goal: `seal` and `open`.
- Files: `seal.rs`.
- Acceptance: contracts/asset-format.md; 64 KiB chunks; a fresh salt per call; `open` reads one byte ahead; wrong key or kind, a changed byte, truncation (including at a chunk boundary), reordered chunks, trailing bytes and a bad magic all fail and release plaintext only for verified chunks; empty plaintext and 10 MB round-trip; input is the decoded key bytes.
- Constraints: `ring::aead` and `ring::hkdf` only; no `unsafe`; errors hold no key or plaintext.

### T014 Releases client (FR-013, FR-036, FR-051, FR-054)
- Goal: the only code that calls GitHub with the state token.
- Files: `releases.rs`, `util.rs`.
- Acceptance: the calls of contracts/release-store.md behind `trait Releases`; `parse_http_date` (strict IMF-fixdate); fixed hosts, redirects off, download `Location` check, name and size checks before download, pagination with a 300-asset fail-closed bound, rate-limit pause (at most 120 s, renewals exempt), 1 s spacing of mutations, a 5 s no-retry delete, any 4xx on create returns `Conflict`, a half-created own-name asset is deleted after a timeout, one retry of a download on a 404, the store identity call requires `private: true`. The token is sent only as `Authorization: Bearer` and never appears in a URL, argv, error text or `Debug`.
- Constraints: `GITHUB_API_URL` is never read; no clone with this token.

### T015 Queue and signals (FR-006 to FR-016, FR-052, FR-053, FR-054)
- Goal: strict FIFO, no cap, fence, guaranteed removal. After T014 and T018.
- Files: `queue.rs`, `signal.rs`.
- Acceptance: the protocol of the release-store contract. `with_turn` registers (replacing a same-name ticket of a rerun), prints `waiting for the queue` once, polls every 20 s, renews every 60 s from a scoped thread, claims after the 3 s settle re-list, stops at the budget (`queue wait expired`), runs the closure, and deletes the ticket on every path including a panic. Live and stale rules, the regexes and the label rule are pure functions with boundary tests (119 and 120 s, 599 and 600 s, label 120 and 121 s ahead, u64 overflow, missing label). A renewal never pauses; renewals failing for 600 s stop the run in this order: set the cancel flag, kill the run's step containers through the `Killer` seam, wait for them to exit, then delete the ticket, print `queue renewal failed` and exit 1. The signal handler deletes the ticket first within 5 s, skips the Loki flush, and exits 1 through `StatefulSeams.exit`; SIGINT and SIGTERM are tested separately. `Fence::held()` re-checks the ticket.
- Also: renewals failing for 600 s set the `Arc<AtomicBool>` cancel flag of `RunExtras`, kill the run's step containers by their name prefix through the `Killer` seam (`Engine::kill_run` runs `docker kill`), wait for them to exit, and only then delete the ticket, print `queue renewal failed` and exit 1. A 5xx on registration is retried inside the wait budget. A non-`uploaded` ticket-pattern asset under 120 s is live; another kind is deleted only after 360 s. The SIGINT/SIGTERM path uses the same flag.
- Constraints: no queue text on stdout or stderr except the fixed lines; timing values come from `Timing` so tests use milliseconds; one `unsafe` block with a comment.

### T016 Bundle and scan (FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058)
- Goal: build the tar, refuse any secret, upload encrypted.
- Files: `bundle.rs`, `scan.rs`.
- Acceptance: `build` canonicalises each declared path, requires `starts_with` the canonical checkout, `lstat`s, opens with `O_NOFOLLOW|O_NONBLOCK`, judges size by bytes read; walks folders (depth 64); skips symlinks, special files and missing paths with a Loki line; refuses tar names with `..`, a leading `/` or a control character; ustar plus pax `path` for names over 100 bytes; cap 100 MB (exactly passes). `scan` reads the tar file: raw search with `memchr::memmem` (carry `max_needle - 1` bytes), every zip member read with `miniz_oxide::inflate::stream` (64 KiB buffer) to depth 2, `trace.network` included, limits of FR-049; members allowed only as text, JSON, zip (stored or deflate), PNG or JPEG; `.html` members refused. Needles are `Log::secret_forms()` plus the FR-050 state strings. One `Refused` with a count. Upload name by the artifact regex; keep the 10 newest `uploaded` assets.
- Constraints: scan and encrypt read the same tar file under `--work`; do not re-read the checkout.

### T017 State (FR-021 to FR-027, FR-050, FR-055)
- Goal: restore into memory, hook, save.
- Files: `state.rs`.
- Acceptance: `restore` picks the highest id among `uploaded` assets matching the state regex, checks name and size first, downloads, opens into memory (10 MB), returns bytes or `None`; any failure is `Err` and never falls back. `StateHook` writes at the declared path before the `state = true` step (mode 0600, canonical path inside the checkout, `O_NOFOLLOW`, nothing for `None`) and after it runs `own_paths`, reads the file by the D11 rules, keeps the bytes in memory and removes the file (failure is `runner fault`). `save` seals with a fresh salt after a scan of the plaintext for every known secret form, uploads, checks the stored size, re-checks the fence, deletes the new asset if the ticket vanished, keeps the 2 newest `uploaded`, and a missing capture is no save. `state_strings(bytes)` for FR-050. A manual-run (entry 0) asset restores.
- Also: the `after` hook runs `state_strings` on the bytes it read and calls `log.add_secrets` before any artifact is built, so a cookie issued during the run is masked and scanned (FR-050). After T018.
- Constraints: the state is a credential: never an artifact path, never logged, never world-readable, never on disk in plaintext outside the checkout window.

### T018 Log and engine (FR-035, FR-037, FR-072)
- Goal: the public grammar and the hook.
- Files: `log.rs`, `engine.rs`, `job.rs` (`RunExtras`: deadline, hook and cancel flag into `RunCtx`; `local` passes the old 30 minutes and no hook).
- Acceptance: `FAULTS` gains the nine lines of FR-037 and `public_form` accepts the queue line (membership asserts); on the tick the same texts are the reason texts of the commit status (FR-072). `StepHook` is called on the step's thread around `docker run`, before the step is marked finished. `Engine::own_paths(image, paths)` runs the given image (the clone image on the tick) with `--network none`, no `-e`, `--entrypoint chown -hR` then `--entrypoint chmod -R u+rX`, never `a+r`, with arguments, not a shell string built from names; a test asserts its argv and environment hold no secret. `RunExtras` carries `cancel: Arc<AtomicBool>`; the step loop checks it and stops starting steps. `Engine::kill_run(prefix)` runs `docker kill` on the run's containers and waits.
- Also: `job` and `local` ignore the new keys for rows that are not allowed the store (FR-062, FR-067); `local` always ignores them; a vault declaration there ends `vault unavailable`. No `inspect_self` (D19 dropped it).
- Constraints: `log.rs` and `engine.rs` already may name the vendor; nothing else changes.

### T019 Wire the tick `job` (FR-006 to FR-009, FR-019 to FR-027, FR-045, FR-049 to FR-055, FR-059, FR-061, FR-066 to FR-070)
- Goal: `job` prepare writes the names, the real run takes them once, and the order of D7 holds.
- Files: `job.rs`, `stateful.rs`.
- Acceptance: prepare mode writes, in this order, the names the pipeline declared (only on `main`; other branches keep `NO_JOB_SECRETS`), 012's names for image rows, and for a `main` row whose pipeline needs the store `FACTORY_CI_STATE_TOKEN` plus `FACTORY_CI_STATE_KEY` for `[state]`, `FACTORY_CI_ARTIFACT_KEY` for `[artifacts]` (and in 0.4.1 the five vault names for `vault`), nothing else, none on another branch, and a pipeline with no store need gets no runner-only name; a scheduled row (`prev == sha`) needs the token. The real run follows D7 steps 1 to 8: intake through `take_runner_env` before any thread, `log.add_secrets` before any value-bearing line or store call, derived store repo and `private: true` check, the budget of D16 through `StatefulSeams.timing`, `JOB_LIMIT` 45 minutes, attempt from `GITHUB_RUN_ATTEMPT` and ids from `--run-id` (D19), the queue turn, claim, restore, hook, `own_paths` on the clone image, bundle, scan, upload, save, claim finish and sweep, ticket release, final status. A scan refusal makes the item `error` and does not stop the state save. Artifacts run when the pipeline did not pass and a step started, or `when = "always"`. A scheduled run ends silently with exit 0 and no status when nothing is due or the claim create is refused; a scheduled run's status follows FR-004. In 0.4.0 a validated vault declaration ends `vault unavailable` before a ticket; 0.4.1 reads it (T035). A row that is not allowed the store runs exactly as before 011 (a test compares the `RunCtx` and the posts against the 0.3.0 behaviour).
- Also: the test that reads both workflow files for `timeout-minutes` (FR-039); the signal handler deletes the ticket and the claim within 5 s.
- Constraints: state token and keys reach only `releases.rs`, `ledger.rs` and `seal.rs`; they are never given to an engine child (`ENGINE_ENV` stays seven names, `engine.rs:21`). No `set_var`. No file under `.github/workflows/` or `.github/scripts/` is written. After T011 to T018, T042 and the 012 merge.

### T020 `main.rs` (FR-033)
- Goal: `decrypt-artifact` and the signal handler.
- Files: `main.rs`.
- Acceptance: `decrypt-artifact` reads only `FACTORY_CI_ARTIFACT_KEY`, streams stdin to stdout through `seal::open`, exits non-zero with one generic line on any error; the `job` entry (not `run_ci`) calls `signal::install()` before anything else; usage text lists the command.
- Also: after T013 and T015. Tests (T003, T009): the key is read from the environment only (a key in argv or stdin is ignored), is never echoed, any error prints one generic stderr line, a truncated stream exits non-zero, and `signal::install()` runs first in `job`.
- Constraints: never echo the key.

### T021 Version and groups (FR-038)
- Goal: 0.4.0 and the process groups.
- Files: `Cargo.toml`, `Cargo.lock`, `.factory-ci.toml`.
- Acceptance: both say `0.4.0` for `rust-ci-runner`; the first command gains `--skip releases_http_tests:: --skip tick_state_tests::` and two commands run those groups; the header says why.
- Constraints: by hand; no other lock entry. After T019, T020, T044, T045.

### T024 Gate and tag (FR-038)
- Goal: a published 0.4.0 image and digest.
- Files: none.
- Acceptance: T001 to T021 (T040 to T045 included) are on `main` after 012, `make lint` and the unchanged selftest pass, the `rust-ci-runner` pipeline passed on that commit (run id and SHA equal), the existing release run finished, the digest is verified by `docker pull`, and the sha256 of `rust-ci-runner-x86_64-linux` is recorded.
- Constraints: the owner or lead pushes the tag; never move a tag. Nothing pins the digest in the manual workflow.

### T027 Alert rules (FR-057)
- Goal: a silent system never reads as healthy.
- Files: `rust-ci-runner/alerts/factory-ci.yaml`, `rust-ci-runner/src/tick_state_tests.rs`.
- Acceptance: the rules of D10 with no-data Alerting; every string matches contracts/loki-lines.md or `FAULTS`; a Rust test reads the file through `CARGO_MANIFEST_DIR` and the contract and asserts each string is in both; no webhook URL.
- Constraints: names and labels only; the file is not under `.github/`. After T024.

### T028 Agent script (FR-033, D15)
- Goal: one hardened command for the debug agent.
- Files: `rust-ci-runner/scripts/open-artifact.sh`, `rust-ci-runner/scripts/test-open-artifact.sh`, `rust-ci-runner/.factory-ci.toml` (one step that runs the test).
- Acceptance: as D15, plus a shell test with a fake binary of a wrong sha, an empty-directory extraction and control-character stripping.
- Constraints: shellcheck-clean; never prints the key; both scripts live under `rust-ci-runner/scripts/`, never under `.github/scripts/`. After T024.

### Phase 5, runner 0.4.1 (vault)

### T032 Vault tests (FR-042 to FR-047, FR-056, SC-007)
- Goal: tests first for the vault read.
- Files: `vault_tests.rs` (fake `bw` script), additions to `tick_state_tests.rs`, `lib.rs` (`vault`, `vault_tests`).
- Acceptance: the tests listed under "Test plan, 0.4.1".

### T033 Vault read (FR-044 to FR-047, FR-056)
- Goal: read only the declared fields and leave nothing behind.
- Files: `vault.rs`, `tests/no_vendor.rs`.
- Acceptance: D3's sequence with 60 s per call and one retry of the whole sequence on a timeout or crash, never after a rejected credential; exactly-one login item per name by exact bytes; fields `username`, `password`, `uri`, custom; values with NUL or a trimmed line under 4 characters (4 passes) are `Unavailable`; data dir 0700 removed and checked; items outside the allowlist, a set `FACTORY_CI_BWS_SERVER_URL`, or any failure end `vault unavailable`; errors and logs hold positions and counts only; the session key joins the redactor.
- Constraints: no item or field name in argv, a file or a log; each call gets only its own variables.

### T034 Image (FR-044)
- Goal: `bw` runs in the image, pinned.
- Files: `rust-ci-runner/Dockerfile` only. The release workflow is not edited, so there is no smoke step.
- Acceptance: `ADD --checksum=sha256:43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3` of `bitwarden-cli-2026.10.1-npm-build.zip` in a throwaway stage; files copied to `/opt/bw`; `apk add nodejs`; the `bw` wrapper; `ARG TARGETARCH` and `RUN bw --version || [ "$TARGETARCH" != amd64 ]`, so the amd64 build fails on a broken `bw` and the arm64 build under QEMU does not; the unchanged selftest builds the image on the branch and proves it; measured size and pull time in the PR.
- Constraints: if `bw --version` fails under Node 24, stop and report; do not switch to the glibc binary without a plan change. Do not edit any workflow.

### T035 Wire the vault and release 0.4.1
- Goal: replace the 0.4.0 refusal with the read in `ci`.
- Files: `ci.rs`, `stateful.rs`, `engine.rs`, `Cargo.toml`, `Cargo.lock`.
- Acceptance: a validated declaration reads the vault inside the queue turn (or before the pipeline when there is no `[state]`), values go into the separate `(item, field)` map, `run_step` exports each step's own entries from it, the log learns the values before any line, version `0.4.1`; `job` and `local` keep refusing a vault declaration (FR-062).
- Constraints: a sibling step never gets a value.

### T036 Tag 0.4.1
- Goal: a published 0.4.1 image and digest.
- Files: `rust-ci-runner/scripts/open-artifact.sh` (the pinned sha256 of the new binary, in its own commit after the tag).
- Acceptance: as T024 for 0.4.1; nothing pins it in the manual workflow; the agent script's pinned sha256 matches the 0.4.1 binary.

### T039 Tick variable (FR-038, D20)
- Goal: the tick runs the new binary; no project uses a new key yet.
- Files: none. The change is the repository variable `RUNNER_IMAGE_DIGEST`.
- Acceptance: the owner (or an agent the owner approves in writing for this run) first records the current value (`gh variable get RUNNER_IMAGE_DIGEST`, a digest, not a secret) in the pull request; after T024 sets `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<digest>` to the digest verified in T024; waits for or dispatches two ticks; expects `plan` to report `fail_tick=0`, the same commit statuses (`factory-ci`, `factory-ci/<project>`) as before the bump, no scheduled row, and no new public line. On any failed status: `gh variable set RUNNER_IMAGE_DIGEST --body <recorded old digest>` and report. The task repeats with the 0.4.1 digest after T036.
- Constraints: a variable change only; no file edit; the digest must belong to a published `rust-ci-runner-v0.4.x` tag; feature 012 relies on the tick, so tell its owner before the bump; no secret printed.

### T040 Schedule and ledger tests (FR-001 to FR-004, FR-063, FR-064)
- Goal: tests first for the pure parts.
- Files: `schedule_tests.rs`, `ledger_tests.rs`, `lib.rs` (declares `schedule`, `ledger` and both test modules; T001 may declare them empty).
- Acceptance: the tests under "Test plan, schedule and ledger".

### T041 `schedule.rs` (FR-001, FR-002)
- Goal: parse `[schedule]` and compute the due slot.
- Files: `schedule.rs`.
- Acceptance: `parse(&toml::Table) -> Result<Option<Schedule>, String>` accepts only `every` and `offset` with the regexes of FR-001, range 900 s to 2,678,400 s, `offset < every`; `due(&Schedule, now: u64) -> u64` is `now - ((now - offset) mod every)`; a `now` below `offset` yields `None`. Boundaries: `15m`, `31d`, `14m`, `32d`, `0m`, `01h`, `1H`, `1.5h`, `offset == every`, `offset = "0"`, a third key, a non-string value, two tables.
- Constraints: no clock, no I/O, no `unsafe`; the module name is not vendor-specific.

### T042 `ledger.rs` (FR-003, FR-063, FR-064, FR-065)
- Goal: the claim protocol over `Releases` and the read-only plan reader.
- Files: `ledger.rs`.
- Acceptance: `h16(project, branch)`; `claim_name`; `label` build and parse (`^[rpfe]\.[0-9]+\.[0-9a-f]{16}$`); `stale(label, now)` (`r` at 3600 s, a missing, malformed or future label (more than 120 s) is stale; 3599 s is not); `decide(assets, h16, due, now) -> Plan { Run{n}, Skip, Close }`; `claim(rel, ...)` creates the asset and treats any 4xx as lost after one list; `finish(rel, ...)` 3 tries 1 s apart; `sweep` keeps the 2 newest `uploaded` per `<h16>` and deletes claims older than 90 days; the plan reader lists at most 300 assets (fail closed above), checks `private: true` first, uses `FACTORY_CI_TOKEN`, a 30 s limit, redirects off, and returns the `Date`. Never a read-modify-write of one asset.
- Constraints: the write functions take `&dyn Releases` (state token); the plan reader takes the read-only client. The two never share a token type. No name or token in any error text.

### T043 Plan schedule tests (FR-002, FR-064 to FR-068)
- Goal: tests first for `plan`.
- Files: `plan_schedule_tests.rs`, `plan_hardening_tests.rs` (extend; keep 012's `!r.secrets_ok` fixture free of `docker/` and `main` store rows).
- Acceptance: the tests under "Test plan, plan".

### T044 `plan` changes (FR-002, FR-064 to FR-068, D22)
- Goal: scheduled rows, `secrets_ok`, no `pending` for them.
- Files: `plan.rs`, `github.rs`, `fake.rs`, `projects.rs` (`schedule_context`).
- Acceptance: D22. `Api::file` with a 64 KiB limit; at most 1 blob read per project and 40 per tick; ledger read once per repo; scheduled rows carry `prev_sha == sha`, `secrets_ok = true`; phase C skips their `pending` and the write cap; `order` puts them after push rows; a push row for the same project drops the scheduled row; ignored schedules (other branch, entry, `allow_docker`, `docker/`, public repo) are counted in the summary without names; an invalid schedule gives a problem and one `error` status only when the latest own status of the context differs; a ledger fault gives no row and one problem. `secrets_ok` for every row is `store_allowed || image_rule`. A push row never has `prev_sha == sha` (assert in `finish`).
- Constraints: the plan never uses the state token or `releases.rs`; `walk.rs` does not change; after T042, T043, T011 and the 012 merge.

### T045 `ci` refusal (FR-069, D24)
- Goal: the manual `ci` refuses the new declarations.
- Files: `ci.rs`, `ci_tests.rs`.
- Acceptance: a pipeline with `[state]`, `[artifacts]`, `[schedule]`, `state = true` or a vault list ends `[runner] state not allowed` before any store call or container; a plain pipeline behaves as before.
- Constraints: small; no new `Args` or `Seams` field.

### T026 Canary through the tick (deferred until T039 and T030; owner, or an agent the owner approves)
- Goal: prove the no-secret path and the write-token route on the live tick.
- Acceptance: with a throwaway fixture project on `main` of a scratch private repo (watch entry with the literal `secret_branches`), one tick lists the names, resolves `<repo><SEP>FACTORY_CI_STATE_TOKEN`, claims a slot, runs, finishes the claim and posts one status; stdout of the job step, stderr and every Loki body hold none of the canaries; a second tick runs nothing for that slot.

### T029 Owner actions and release canary on the real store (after T039; owner)
- Goal: the quickstart section 2 table, run through the tick path.
- Acceptance: every line of the table passes, including the 200-trial listing, two racing jobs for one slot (20 times, one create wins) and the real Playwright trace scan.

### T030 `secret_branches` pull request (after T039)
- Goal: `secret_branches = ["main"]` on the `sudtanj/factory-monorepo` entry of `.github/factory-ci-repos.toml`, and the comment block updated (the key now also unlocks the tick's store on `main`).
- Acceptance: `plan --repos .github/factory-ci-repos.toml --check` passes with the 0.4.x runner and with 0.3.0; the repo-files test of 010 and 012 accepts the new shape; nothing else in the file changes.
- Constraints: merges after the variable bump, never before (D20).

### T031 First real runs (after T029 and the project toml change)
- Goal: SC-001, SC-003 and SC-009 measured.
- Acceptance: quickstart section 3.

### T037 Vault owner actions and first vault run (after T036)
- Goal: the dedicated account, collection and a canary item; one tick run on `main` reads it.
- Acceptance: quickstart section 3 step 5.

### Dropped by the owner rule

IDs stay reserved so references hold. None of these is built.

| ID | Was | Why dropped |
|---|---|---|
| T010 | shell-test updates for cron lines | `.github/scripts` edit |
| T022 | retire the tick | reversed, the tick stays |
| T023 | rewrite `factory-monorepo-ci.yml` | workflow edit |
| T025 | pin the release in the manual workflow | workflow edit |

### T038 Docs (FR-041), last
- Goal: both READMEs and the repo description match.
- Files: `README.md`, `rust-ci-runner/README.md`, `gh repo edit`.
- Acceptance: they state what FR-041 lists, including the release checklist line (image digest, agent script sha, the tick variable), the `state restore failed` runbook, the S5 wording (a state step should not follow an install step that runs package scripts), that the state step runs as root, that `queue wait expired` is the one runner-side drop and the wait is 5 minutes, that the concurrency group and the tick stay, what 011 leaves out and why (schedule, manual pin, watch keys), the Grafana rules, the `decrypt-artifact` command and the agent kit, and the accepted risks. No value appears.
- Also: the README says that traces from logged-in runs may be refused by the scan and that this is by design; that a burst of renewals at the secondary rate limit can trigger `queue renewal failed`, which an alert covers; that a late winner has a minimum pipeline window of 10 minutes; and that a step on reviewed `main` code could print a transformed secret (accepted).
- Constraints: stop-slop applies.

## Test plan (qa writes it first)

`FakeReleases` (test-only, in `fake.rs`) is an in-memory `Releases`: monotonic ids, a settable server clock, duplicate-name refusal, per-call fault injection, a lag switch, an "in flight" switch, a call log. Tests set the clock and never sleep. Use unique canaries per secret and search every sink (`W::bootstrap()` in `ci_tests.rs` shows the method). Membership asserts, never counts, for `RUNNER_KEYS` and `FAULTS`.

### 0.4.0

| ID | File | Tests | FR |
|---|---|---|---|
| T002 | `store_tests.rs`, `secret_names_tests.rs`, `watch_tests.rs` | `runner_keys_hold_the_thirteen_names_and_required_keys_are_the_first_four`; `fetch_with_keeps_runner_key_rows_in_the_one_call` (stub counts `bws` calls: one); `held_refuses_a_runner_key_name`; `key_decoding_to_31_bytes_fails_32_passes_low_variety_fails`; `equal_state_and_artifact_keys_fail_in_a_state_only_run`; `missing_store_repo_or_token_fails`; `every_runner_key_is_refused_by_every_consumer_in_any_case`; `store_allowed_needs_main_literal_private`; `state_and_vault_items_stay_unknown_watch_keys`; `take_runner_env_removes_variables` | FR-019, FR-020, FR-034, FR-043, FR-059 |
| T003 | `seal_tests.rs` | `round_trip_empty_one_chunk_exact_chunk_and_10mb`; `plaintext_of_exactly_64kib_has_one_encoding`; `wrong_kind_key_fails_both_ways`; `flipped_byte_fails`; `truncation_at_chunk_boundary_fails`; `reordered_chunks_fail`; `trailing_bytes_fail`; `bad_magic_fails`; `two_seals_differ`; `error_text_holds_no_key_or_plaintext` | FR-027 |
| T004 | `pipeline.rs` tests | `state_table_rules` (name 64 passes 65 fails, bad characters, paths, 20 and 21, `.ci-state` and `.ci-state-2` pass, artifact equal, containing, under refused); `exactly_one_step_declares_state`; `state_step_must_be_ordered_with_every_other_step`; `artifacts_when_values`; `vault_entry_shape_and_limits`; `reserved_prefixes_refused_in_env_secrets_and_vault_env_in_any_case`; `name_in_secrets_and_vault_env_is_refused`; `docker_step_with_vault_or_with_state_is_refused`; `project_with_dotdot_dot_end_or_lock_is_refused_for_state`; `tag_over_200_bytes_is_refused`; `tag_mapping_is_injective` (collision table); `second_state_table_is_a_toml_error` | FR-017, FR-018, FR-042, FR-059 |
| T005 | `releases_http_tests.rs` (own group), `util.rs` tests | `upload_sends_bearer_and_content_length_and_no_token_in_url`; `any_4xx_on_create_is_conflict`; `retry_once_on_5xx`; `pause_on_403_429_at_most_120s_renewals_exempt`; `mutations_are_1s_apart`; `download_redirect_host_must_be_githubusercontent_https`; `download_checks_name_and_size_first`; `restore_retries_once_on_404`; `half_created_own_asset_is_deleted_after_a_timeout_then_one_retry`; `full_page_paginates_and_over_300_fails_closed`; `store_must_be_private`; `date_header_parse`; `token_never_in_error_text_or_debug` | FR-013, FR-036, FR-051, FR-054 |
| T006 | `queue_tests.rs` (fake, at most 6 joined threads) | `fifo_is_asset_id_order`; `there_is_no_ticket_cap` (6 waiters all run in order); `second_waits_and_prints_one_line_once`; `stale_at_600s_not_at_599s`; `stale_ticket_removed_by_the_next_waiter_with_a_loki_line`; `ticket_pattern_not_uploaded_younger_than_120s_is_live_and_120s_is_not`; `other_kind_not_uploaded_is_deleted_only_after_360s` (360 kept, 361 deleted); `two_failed_uploads_keep_the_last_good_state`; `forged_label_negative_age_overflow_missing_are_stale`; `label_120s_ahead_passes_121s_is_stale`; `junk_assets_are_ignored`; `late_lower_id_ticket_sends_the_claimer_back`; `slow_upload_with_a_lower_id_is_seen_at_renewal_and_logged_as_overlap`; `wait_expires_at_budget_minus_30_and_removes_the_ticket`; `budget_defaults_to_start_plus_35_minutes`; `deadline_is_the_smaller_of_30_minutes_and_budget_minus_15`; `matrix_siblings_with_one_run_id_do_not_replace_each_other`; `rerun_of_one_run_id_replaces_its_ticket`; `renewal_never_pauses_on_a_rate_limit`; `renewals_failing_for_600s_end_the_run_with_queue_renewal_failed`; `every_exit_path_removes_the_ticket`; `sigint_and_sigterm_delete_the_ticket_within_5s_and_skip_the_flush` (two tests); `own_ticket_gone_before_save_writes_nothing`; `own_ticket_gone_after_upload_deletes_the_new_asset`; `exit_delete_has_a_5s_limit_and_no_retry`; `server_clock_skew_does_not_matter`; `two_waiters_deleting_one_stale_ticket_is_fine` | FR-006 to FR-016, FR-052 to FR-054 |
| T006 (critic 2) | `queue_tests.rs` | `renewal_failure_kills_the_containers_then_deletes_the_ticket` (`FakeKiller` and `FakeReleases` share one event log: the kill and the wait for exit come before the ticket delete; a step still running blocks the delete); `cancel_flag_stops_the_step_loop`; `registration_5xx_is_retried_inside_the_wait_budget` | FR-014, FR-054 |
| T003 (critic 2) | `seal_tests.rs`, `tick_state_tests.rs` | `decrypt_artifact_reads_the_key_from_the_environment_only`; `decrypt_artifact_never_echoes_the_key`; `decrypt_artifact_error_prints_one_generic_stderr_line`; `decrypt_artifact_truncated_stream_exits_non_zero`; `run_ci_installs_the_signal_handler_first` | FR-033, FR-009 |
| T008 (critic 2) | `state_tests.rs` | `cookie_set_during_the_run_in_a_trace_refuses_the_artifact` (after hook adds the strings before bundling); `own_paths_argv_and_environment_hold_no_secret` (`--network none`, no `-e`, `--entrypoint chown`) | FR-050, FR-055 |
| T009 (critic 2) | `tick_state_tests.rs` | `canaries_include_the_base64_and_x_access_token_forms_of_the_state_token` (state token, both keys, derived store repo name; before the first log line and the first store call); `vault_declaration_refused_before_a_ticket`; `budget_end_comes_from_seams_timing`; `store_repo_must_equal_the_derived_name`; `loki_strings` as one list; `trace_after_login_with_restored_state_is_refused_unless_scrubbed` | FR-020, FR-061, FR-049 |
| T007 | `bundle_tests.rs`, `scan_tests.rs` | `folder_bundled_whole_missing_path_skipped`; `symlink_and_special_files_skipped`; `path_through_a_symlinked_parent_is_refused` (canonical `starts_with`); `fifo_at_the_path_does_not_block`; `size_is_by_bytes_read`; `tar_names_with_dotdot_absolute_or_control_are_refused`; `long_names_use_pax`; `bundle_cap_at_the_limit_passes_one_over_refuses` (cap injected); `retention_keeps_10_uploaded`; `upload_failure_leaves_the_exit_code`; `sentinel_in_plain_base64_and_one_line_of_multiline_is_refused` (every `secret_forms` form, in a plain member, in `trace.network` inside a zip, in a nested zip at depth 2); `state_strings_join_the_scan_set` (FR-050); `depth_3_zip_is_refused`; `html_member_is_refused`; `member_that_is_not_text_json_zip_png_jpeg_is_refused`; `zip_method_other_than_stored_or_deflate_is_refused`; `encrypted_entry_zip64_bad_crc_over_10000_entries_over_500mb_refused` (limits injected); `value_across_a_chunk_edge_is_found`; `4_char_found_3_char_ignored`; `refusal_reports_a_count_only`; `clean_bundle_passes` | FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058 |
| T008 | `state_tests.rs` | `first_run_has_no_state_and_no_file`; `restore_picks_the_highest_id_uploaded_asset`; `manual_run_asset_restores`; `restore_fails_on_wrong_key_truncated_tampered_and_never_falls_back`; `restore_checks_name_and_size_before_download`; `save_fresh_nonce_keeps_the_two_newest_uploaded`; `crash_between_upload_and_delete_leaves_two_and_newest_wins`; `save_refuses_symlink_empty_over_10mb` (10 MB passes); `failed_pipeline_never_changes_the_store`; `save_failure_keeps_previous_state`; `store_holds_only_ciphertext`; `state_plaintext_holding_a_known_secret_is_not_saved`; `state_path_through_proc_self_captures_nothing`; `json_value_strings_16_chars_added_15_not`; `non_json_under_1mb_added_whole`; `state_file_exists_only_during_the_declaring_step` (absent before, 0600 inside, gone after); `no_other_step_sees_the_file`; `file_is_never_world_readable`; `removal_failure_stops_the_run`; `own_paths_runs_the_runner_image_with_chown_hr` | FR-021 to FR-027, FR-050, FR-055, SC-005 |
| T009 | `tick_state_tests.rs` (own group; a fake engine and a fake store drive `job`) | `counter_pipeline_sees_0_1_2_across_three_runs`; `four_runs_of_one_state_never_overlap_and_all_run`; `non_main_branch_runs_without_state_queue_artifacts_vault_and_schedule`; `main_row_without_secrets_ok_is_refused_by_the_script_contract` (names listed, `secrets_ok` false is the script's refusal, modelled by a harness that mirrors `job.sh:68-71`); `declared_step_secret_on_a_non_main_branch_keeps_no_job_secrets`; `public_repo_gets_no_store`; `missing_store_settings_end_before_a_ticket`; `restore_failure_starts_no_container_and_releases_the_ticket`; `validated_vault_declaration_ends_vault_unavailable`; `failing_step_uploads_one_artifact_and_loki_names_it`; `passing_run_with_when_failure_uploads_nothing`; `scan_refusal_fails_a_passing_run_and_still_saves_state`; `artifact_upload_failure_keeps_the_status`; `hostile_fixture_leaks_nothing` (canaries for all thirteen runner keys, state strings; job stdout and stderr, every child argv and variable set, every Loki body, every asset, every status text); `status_texts_are_all_in_faults_and_the_nine_are_members`; `job_stdout_is_empty_under_quiet_logs`; `no_state_pipeline_registers_nothing`; `existing_pipelines_run_unchanged`; `every_loki_string_of_the_contract_is_emitted`; `intake_removes_the_runner_variables_before_any_child` (the child environment and `/proc/self/environ` of the fake engine hold none) | all stories, FR-029 to FR-031, FR-048, FR-051 to FR-067 |
| T009 (tick) | `tick_state_tests.rs`, `job_tests.rs` | `attempt_comes_from_github_run_attempt_digits_else_1`; `ids_come_from_the_row_run_id_and_matrix_siblings_differ`; `helper_image_is_the_clone_image_and_its_argv_holds_no_secret`; `clone_image_without_chown_ends_runner_fault_before_a_ticket`; `job_limit_equals_both_workflow_timeouts` (reads `factory-monorepo-ci.yml` and the tick `run` job, read only); `prepare_names_table` (main with state, with artifacts, scheduled, plain, non-main, image row from 012: exact sorted lists); `scheduled_run_with_nothing_due_exits_0_without_status`; `scheduled_run_creates_the_claim_after_the_turn_and_finishes_it`; `lost_claim_create_ends_silently`; `cancel_deletes_the_claim_within_5s` | FR-003, FR-039, FR-062, FR-066, FR-067, FR-070 |
| T045 | `ci_tests.rs` | `ci_refuses_state_artifacts_schedule_and_vault_declarations_before_any_call`; `ci_runs_a_plain_pipeline_as_before` | FR-069 |

### Schedule, ledger and plan

| ID | File | Tests | FR |
|---|---|---|---|
| T040 | `schedule_tests.rs` | `every_range_15m_to_31d_and_the_off_by_one_cases`; `bad_units_leading_zero_decimal_case_and_empty_are_refused`; `offset_must_be_below_every`; `unknown_key_non_string_and_second_table_are_refused`; `due_is_epoch_aligned_utc` (`6h` gives 00, 06, 12, 18; `1d` with `3h` gives 03:00); `due_at_the_slot_second_and_one_second_before`; `now_below_offset_has_no_slot`; `due_is_a_pure_function_of_now` (property: `due <= now`, `now - due < every`, `due mod every == offset mod every`) | FR-001, FR-002 |
| T040 | `ledger_tests.rs` | `claim_name_regex_and_h16_are_stable`; `h16_differs_for_project_branch_pairs_that_concatenate_alike`; `label_roundtrip_and_malformed_labels_are_stale`; `stale_at_3600s_not_at_3599s`; `label_120s_ahead_ok_121s_stale`; `decide_run_when_no_claim`; `decide_skip_for_claim_at_or_above_due_running_passed_or_failed`; `decide_coalesces_three_missed_slots_into_one`; `decide_stale_n0_gives_run_n1_and_stale_n1_gives_close`; `forged_names_and_labels_are_ignored`; `two_creates_of_one_name_one_wins` (FakeReleases); `finish_retries_three_times_then_logs`; `sweep_keeps_two_newest_per_h16_and_deletes_over_90_days`; `sweep_never_touches_another_h16_younger_than_90_days`; `over_300_assets_fail_closed`; `not_uploaded_claims_do_not_count`; `store_must_be_private_before_any_read`; `parallel_jobs_write_disjoint_assets_and_no_asset_is_rewritten_by_two_writers` (up to 6 threads) | FR-003, FR-063, FR-064, FR-065 |
| T043 | `plan_schedule_tests.rs` | `ten_tick_walk_two_slots_dead_run_and_repeated_tick` (fake `Api` clock and `FakeReleases`); `never_ran_schedule_is_due_at_the_next_tick`; `unchanged_sha_with_a_passing_push_status_still_runs`; `scheduled_row_has_prev_equal_sha_and_push_row_never_does`; `no_pending_for_scheduled_rows_and_no_write_cap_use`; `push_row_wins_and_the_scheduled_row_is_dropped_for_the_tick`; `scheduled_context_never_equals_or_prefixes_a_push_context` (table over `.`, `a`, `schedule`, `docker/x`); `status_posts_only_on_a_flip_and_one_hour_apart`; `status_422_is_ignored`; `ignored_schedules_other_branch_other_entry_allow_docker_docker_project_public_repo_counted_without_names`; `invalid_schedule_posts_one_error_once_per_sha`; `ledger_read_failure_gives_no_row_and_one_problem`; `store_not_private_or_missing_gives_no_row`; `blob_reads_are_capped_at_one_per_project_and_40_per_tick`; `secrets_ok_is_store_allowed_or_image_rule` (table: literal `["main"]`, glob, longer list, none, public repo, non-main, image row); `plan_check_accepts_secret_branches_main_and_refuses_state_and_vault_items_keys` | FR-002, FR-064 to FR-068 |

### 0.4.1

| ID | File | Tests | FR |
|---|---|---|---|
| T032 | `vault_tests.rs` (fake `bw`) | `sequence_is_login_unlock_list_logout`; `each_call_gets_only_its_own_variables`; `no_item_or_field_name_in_argv`; `list_runs_with_org_and_collection_ids_and_no_interaction`; `reads_each_declared_field_once`; `item_matching_zero_or_two_logins_is_unavailable`; `item_outside_the_allowlist_is_unavailable`; `case_difference_does_not_match`; `missing_empty_nul_and_3_char_line_unavailable_4_passes`; `data_dir_is_0700_and_gone_before_return`; `failed_logout_or_removal_is_unavailable`; `retry_once_on_timeout_not_on_rejected_credential`; `bws_server_url_set_ends_unavailable`; `errors_and_logs_hold_positions_and_counts_only`; `unicode_space_and_equals_in_item_names_match_by_bytes` | FR-044 to FR-047, FR-056 |
| T032 | `tick_state_tests.rs` | `vault_values_reach_only_the_declaring_step`; `sibling_step_environment_holds_no_vault_value_or_state_file`; `step_dumping_env_and_files_sees_only_declared_vars_and_no_bw_data`; `vault_unavailable_starts_no_container`; `vault_values_join_the_log_before_any_line`; `vault_values_are_in_the_scan_set` | US6, SC-007 |

### Alerts and agent script

| ID | File | Tests | FR |
|---|---|---|---|
| T027 | `tick_state_tests.rs` | `alert_rules_cite_only_strings_of_the_loki_contract_or_faults`; `every_fault_and_queue_string_has_a_rule` | FR-057 |
| T028 | `scripts/test-open-artifact.sh` | `wrong_sha_binary_is_not_run`; `extraction_goes_to_a_new_empty_directory`; `control_characters_are_stripped`; `key_never_printed` | FR-033 |

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| runner keys | missing, duplicate, bad format, equal | `Held` accessors | `state store not configured`, before a ticket | T002 |
| store | not private, not reachable, huge or oddly named assets | identity call, name and size checks | `state store not configured` or `state restore failed` | T005 |
| store | rate limited | headers | calls pause (renewals do not), Loki `rate limited` | T005, T006 |
| queue | holder dies | stale rule at 600 s | next waiter removes the ticket | T006 |
| queue | forged or malformed label | label rule | stale, deleted | T006 |
| queue | invisible in-flight ticket with a lower id | not `uploaded` under 120 s counts live; renewal re-list | claim waits; `overlap` logged | T006 |
| queue | turns ahead use the wait limit | budget minus 30 minutes (5 minutes after start) | `queue wait expired`, owner alert | T006 |
| queue | renewals fail for 600 s | renewal thread | cancel flag set, containers killed and stopped, then ticket deleted, `queue renewal failed`, exit 1 | T006 |
| restore | download, auth, decrypt, size | `open`, checks | `state restore failed`, no container, ticket released | T008 |
| state step | file removal fails | `after` hook | `runner fault`, no later step | T008 |
| state | declared path leaves the checkout, FIFO, plaintext holds a secret | canonical path, `O_NONBLOCK`, scan | no capture, `state save failed` | T008 |
| artifacts | secret found, unscannable file, `.html` member | `scan` | `artifact upload refused`, exit 1, no upload | T007 |
| artifacts | upload fails | `HttpReleases` | Loki line, exit code unchanged | T009 |
| save | upload, size or fence fails | `state::save` | `state save failed`, new asset deleted, old state kept | T008 |
| exit | SIGINT, SIGTERM, panic | watcher, drop guard | ticket deleted first within 5 s, exit 1 | T006 |
| exit | ticket delete fails | next waiter | stale rule frees it in 10 minutes | T006 |
| vault (0.4.0) | any validated vault declaration (`ci`, `job`, `local`) | pipeline check | `vault unavailable`, no ticket, no container | T009 |
| vault (0.4.1) | `bw` hangs, rejected, 0 or 2 matches, outside item, server URL set | `vault::read` | `vault unavailable`, no container, dir removed | T032 |
| image (0.4.1) | `bw` does not run on Node 24 | `RUN bw --version` in the Dockerfile | build fails; no release | T034 |
| job limit | someone edits `timeout-minutes` of either workflow | Rust test of the constant against both files | red test, owner updates `JOB_LIMIT` | T009 |
| tick bump | the new binary breaks `job` or `plan` | the first tick's statuses and `fail_tick` | `gh variable set RUNNER_IMAGE_DIGEST` with the recorded old digest | T039 |
| plan ledger | store missing, not private, no read scope, rate limited, over 300 assets, odd names | `ledger::read` | no scheduled row for that repo, one problem without names, push rows unaffected | T043 |
| plan blob | pipeline blob unreadable, over 64 KiB, bad `[schedule]` | `Api::file`, `schedule::parse` | no row; invalid schedule gives one `error` status per sha | T043 |
| claim | two jobs, one slot (rerun, dispatch) | duplicate-name 4xx | the loser ends silently, no run, no status | T040, T009 |
| claim | job dies before the claim | no asset | slot stays due, runs next tick | T043 |
| claim | job dies after the claim | label `r` for 3600 s | one retry as `n = 1`, then `error` and closed | T040, T043 |
| claim | cancelled job | SIGTERM handler | claim deleted within 5 s, slot due next tick | T009 |
| claim | label write fails after a pass | 3 tries | Loki `claim finish failed`; stale rule re-runs once (R19) | T040 |
| claim | store token missing or wrong in Bitwarden | `fci_lookup` in the frozen script | item refused with a sanitized reason, slot stays due | T026 |
| claim | `secrets_ok` false on a `main` row that needs the store | `job.sh:69` | item refused, loud status | T009 |
| intake | runner-only variable left in the process | test of children | removed before any thread or child | T009 |
| statuses | scheduled status over the cap (422) | HTTP 422 | ignored, Loki line | T043 |
| push and schedule | both due for one project | `plan` | push row runs, scheduled row waits one tick | T043 |
| helper | clone image lacks `chown` or `chmod` | `own_paths` | `runner fault`, no ticket | T009, T029 |
| 0.3.0 | a project toml gets a new key before the bump | `deny_unknown_fields` | the project's rows fail `pipeline file refused` until the bump | D20 order |
| manual `ci` | project uses the new keys | pinned 0.3.0 refuses; 0.4.x `ci` refuses | `ci` cannot run that project (accepted, R5) | T045 |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. A slot runs twice or every tick. Causes: the plan reads a stale or partial ledger, the claim is not created before the run, a clock differs between plan and job, the label write fails. Prevented: the claim is a duplicate-name create after the queue turn, `plan` fails closed on any ledger fault (never a blind plan), `due` comes from the store's `Date` in both places and the job recomputes it, the stale rule allows exactly one retry, tests walk ten ticks, a rerun and two racing jobs (T040, T043, T009, T029).
2. A credential reaches a step, a log or an agent. Causes: the store token or a key stays in the runner process environment and reaches a child, the redactor learns it too late, a name is listed on a branch that must not get it, the plan job gets a write token. Prevented: names listed only for `main` with `secrets_ok`, intake removes the variables before any child, `ENGINE_ENV` stays seven names, `log.add_secrets` runs before any line or store call, the scan reads every zip member, the hostile fixture covers all thirteen keys over every sink (T009, T019, T016).
3. The state file reaches a step that must not see it, or a runner file leaves through a symlink. Prevented: one declaring step with an ordering rule, a hook that removes the file, canonical path checks with `O_NOFOLLOW|O_NONBLOCK`, a pre-seal scan, a `/proc/self` test.
4. The release breaks the live tick or 012. Causes: the new binary fails `plan` or `job` for rows with no new key, the toml keys land before the bump (0.3.0 refuses them), the `secret_branches` change lands before the bump, 012 and 011 collide in `plan.rs` and `job.rs`. Prevented: the D20 order (merge 012, tag, bump, watch two ticks, then the `secret_branches` pull request, then owner actions, then project keys), the tick-path tests, one-command rollback, D25.
5. Runs wait, stall or burn the store budget. Causes: a push row and a scheduled row meet in the queue, a 15-minute schedule across many projects exceeds 500 writes an hour, the Environment holds scheduled runs for approval, the ledger grows. Prevented: push wins and the scheduled row waits one tick, the floor of 15 minutes, the owner's Environment check, 2 claims per `<h16>`, a 90-day sweep and a 300-asset fail-closed bound, the alert rules (D10).

## Invariants (qa turns these into property tests)

- I1: for any interleaving of registrations, removals and stale breaks, at most one run holds the turn, and holders run in ascending asset id.
- I2: a ticket is gone after the run ends, whatever the exit path, or the stale rule frees it within 600 s.
- I3: a run whose ticket is gone before the save writes no asset; gone after the upload, it leaves no new asset. The ticket is deleted only after the run's step containers have stopped.
- I4: the newest `uploaded` state is unchanged by any run whose pipeline did not pass, and by any number of failed uploads.
- I5: `open(kind, key, seal(kind, key, x)) == x`; any other key or kind, any one-byte change or truncation fails and releases no plaintext beyond the last verified chunk.
- I6: no byte of stdout, stderr, any child argv or variable, any Loki body or any asset holds a known secret form, except inside ciphertext.
- I7: stdout lines belong to the 008 grammar plus `waiting for the queue`; stderr lines belong to `FAULTS`, of which the nine new lines are members.
- I8: every name in `RUNNER_KEYS` is refused by every consumer, in any letter case.
- I9: the state file exists in the checkout only between `before` and `after` of the one state step, and no other step runs in that window.
- I10: a run never reads or bundles a path that canonicalises outside the checkout; state plaintext never reaches `seal` while it holds a known secret.
- I11: a ticket with a missing, malformed or future label, or not matching the ticket regex, never holds the turn.
- I12: a bundle that holds a known secret form, or any `.html` member, is never encrypted or uploaded.
- I13: the state key opens no artifact and the artifact key opens no state asset.
- I14: no file written by the runner for state or artifacts is world-readable.
- I15: the tag mapping `(repo, project, state)` to a tag is injective.
- I16 (0.4.1): a vault value reaches only the step that declared its `(item, field)`; after the read no `bw` process, session or data dir exists.
- I17: one `bws` call per run.
- I18: `JOB_LIMIT` equals the `timeout-minutes` of `factory-monorepo-ci.yml` and of the tick `run` job; the wait end, the deadline and the budget end derive from it only.
- I19: a row that is not allowed the store (not `main`, not the literal `secret_branches`, public repo) makes no store call and writes no state, claim or artifact, for any pipeline.
- I20: no file under `.github/workflows/` or `.github/scripts/` differs from `main` on any 011 branch.
- I21: a claim name is created at most once; for one `<h16>` and slot at most two claims ever exist (`n` 0 and 1).
- I22: `plan` never plans a row for a slot whose newest claim is at or above `due` and not stale; with an unreadable ledger it plans no scheduled row.
- I23: `due(now)` satisfies `due <= now < due + every` and `due = offset (mod every)`.
- I24: a scheduled row has `prev_sha == sha`; a push row never does.
- I25: the scheduled status context equals no push context and is a prefix of none.
- I26: the plan job and the plan runner never hold the state token, a key or a vault credential.
- I27: after intake, no runner-only variable exists in the job process or in any child.
- I28: no ledger asset is rewritten by a writer other than its creator.

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: handled. Empty and maximum: 64-char names, 20 paths, 20 vault fields, 10 MB and 100 MB bounds, `every` 15m and 31d, 300 assets, n and n+1 tests (T004, T007, T008, T040). Duplicates: the same claim twice (one wins), the same run id twice. Unicode, NUL, newline: path and name rules, `<h16>` hashes project and branch so odd branch names never reach an asset name, item names by bytes (T004, T032, T040). Malformed: tampered or truncated asset (T003, T008), a forged label or name (ledger tests), a bad `[schedule]` (T040), a `bw` list that is not JSON.
- State: handled. First run (`first_run_has_no_state_and_no_file`, `never_ran_schedule_is_due_at_the_next_tick`). Older version: 0.3.0 refuses the new keys, the D20 order keeps them out until the bump; the magic leaves room for an asset version. Crash mid-operation: a stale queue ticket (600 s), a stale claim (3600 s, one retry), two assets after a crash with the newest winning, a half upload is not `uploaded`. Partial failure: the failure-mode table. Retry: a rerun replaces its ticket, claim and asset names are unique, deletes are idempotent (404 is gone).
- Time and concurrency: handled. Races in T006 and T040 (parallel jobs write disjoint assets). Timeouts on every GitHub and `bw` call. Clock: ages and `due` from the store's `Date`; the job recomputes `due`. Boundaries: 119 and 120 s, 599 and 600 s, 3599 and 3600 s, label 120 and 121 s ahead, the slot second. Ticks are serial (`factory-ci-tick.yml:9-11`); a tick rerun meets the duplicate-name refusal. Late and out-of-order events: stable order by asset id; a slot older than a newer claim is never run.
- Resources: handled with limits. Disk: bundles and ciphertext under `--work`. Large input: chunked crypto, streamed scan, caps of 10 MB, 100 MB, 500 MB, 10,000 entries, 64 KiB pipeline blob. Slow or garbage dependency: one retry, a bounded pause, then a fixed fault; a ledger fault means no scheduled row. Growth: 2 state assets, 10 bundles, 2 claims per `<h16>`, a 90-day sweep, at most 300 assets listed (fail closed). Store writes: about 10 to 20 per run against 500 an hour, so the 15-minute floor. Memory: 64 KiB chunks and scan buffer.
- Permissions and trust: handled. The state token reaches `releases.rs` and `ledger.rs` only; keys reach `seal.rs`; vault credentials reach `vault.rs` (0.4.1). The plan holds only the read-only token. The pipeline author cannot name a release, a repo, a setting or a credential, cannot widen the entry (a reviewed file on `main`) and cannot see the state file outside its one step. A branch other than `main` gets nothing. The agent holds the artifact key, a Loki token and a read token for the store, nothing else.
- UI: N/A. No UI; the surface is commit status texts and Loki lines.

## Rollout and rollback

Rollout (D20, D25), each step after the one before:

1. 012 merges to `main`. 011 tick-path code lands on `main` through a pull request linked to this folder, starting with T001 alone on a branch (it proves the hand-edited lock under `--locked`); the unchanged selftest and `make lint` pass; the `rust-ci-runner` pipeline passes on `main`.
2. T024: tag `rust-ci-runner-v0.4.0`; read the digest.
3. T027, T028 (alert rules, agent script).
4. T039: record the old tick digest, set the new one, watch two ticks. No project declares a new key yet.
5. T030: the `secret_branches` pull request for the `factory-monorepo` entry (safe under 0.3.0 and 0.4.x).
6. Owner actions (below), then T029 (canary on the real store, through the tick) and T026.
7. The project toml change in `factory-monorepo` (`[schedule]`, `[state]`, `[artifacts]`; the 002 pipeline). From here the frozen manual `ci` cannot run that project.
8. T031 first real runs. Phase 5: T032 to T035, T036 (tag 0.4.1, update the agent script's pinned sha), T039 again with the 0.4.1 digest, T037.
9. T038: docs.

Rollback, in reverse: remove the project toml keys; set the recorded old digest back (`gh variable set RUNNER_IMAGE_DIGEST --body <old>`) and confirm one tick passes; then revert the Rust and `Dockerfile` commits. The `secret_branches` change may stay (0.3.0 parses it; the manual `ci` then honours it for declared step secrets as 010 intends) or be reverted in its own pull request. The released images stay in the registry; nothing pulls `latest`. Ledger, queue and state releases stay in the store repo (1-byte assets, ciphertext); delete them by hand if wanted. The release workflow moves `:latest` on each tag; moving it back needs owner approval (`docker buildx imagetools create --tag ghcr.io/sudtanj/rust-ci-runner:latest ghcr.io/sudtanj/rust-ci-runner@<previous digest>`). Fixes ship as the next patch version; never move or delete a tag. Rotate the runner-only settings if a leak is suspected and revoke the state token.

## Owner actions

Done by the owner, never by an agent (T039 may be run by an agent the owner approves). No value is written in any file.

1. Create `<owner>/factory-ci-state`: private, no code, no workflows, no tag rulesets that block creation, immutable releases off, branch protection on its default branch, one initial README commit.
2. Create a fine-grained token scoped to the store repo only (contents write); put its expiry on a calendar. This is `FACTORY_CI_STATE_TOKEN`.
3. Add the store repo to the contents read scope of `FACTORY_CI_TOKEN` (the plan reads the ledger with it). Read only.
4. In the plain lane's Bitwarden project (`FACTORY_CI_BWS_PROJECT`) add, per watched repo, the keys `<repo><SEP>FACTORY_CI_STATE_TOKEN`, `<repo><SEP>FACTORY_CI_STATE_KEY` and `<repo><SEP>FACTORY_CI_ARTIFACT_KEY` (the two keys from `openssl rand -base64 33`, different values). `<SEP>` is `/` unless the store refused it (`factory-ci-secrets.sh:11`). Before 0.4.1 is used, the five vault keys the same way, and the dedicated Bitwarden account in an organization that sees one collection, with opaque item names (`ci-login-1`).
5. Check the Environment `factory-ci`: it allows deployments from `main` and has no required reviewer.
6. In `sudtanj/factory-monorepo`: branch protection on `main` (reviews, no direct pushes, administrators included); the 002 pipeline keeps the Playwright HTML reporter off.
7. The debug agent gets only the artifact key, a Loki read token limited to this tenant and step `ci`, and a GitHub read token scoped to the store repo. No write tools; egress to GitHub and Loki only.
8. Import `rust-ci-runner/alerts/factory-ci.yaml` into Grafana and attach a contact point.
9. Calendar items: state token expiry, a review of each Bitwarden CLI release.
10. Tell feature 012's owner before T039; keep the old digest.

## Devops assessment

Findings from the lead's devops review of 11d2ef0 and the critic review of faa835f, with their handling after the critic decisions.

Amendment of 2026-11-28 (owner rule, its correction and the tick decisions): cron slack, the concurrency cap and the 190-minute margin do not apply (no cron line, no workflow edit). The tick now carries the features, so these rows are new or changed: the write-token route, the plan read scope, the tick order of rollout, the Environment check, store write budget, ledger size and the 012 merge order. Image size and the arm64 smoke still apply to 0.4.1 (`RUN bw --version`; amd64 blocks, arm64 does not). Tick still deployed on purpose and moved by the repository variable (T039). Rollback order: toml keys, then the old digest, then the commits. Monitoring: Loki contract, tests and the alert file stay. Budget: 45-minute limit on both workflows, budget 35 minutes, wait ends at budget minus 30 (D16).

| New finding | Handling | Where |
|---|---|---|
| The plan job cannot hand a write token to the runner | The run job writes the claim; the token travels by the frozen names-file route (`factory-ci-job.sh:55-117`) | D21, D7 |
| `FACTORY_CI_TOKEN` must read the store repo | Owner action 3, read scope only; a read fault means no scheduled row | D21, Owner actions |
| Store write budget (500 content writes an hour) | 15-minute floor, one create and one patch per run, 2 claims per `<h16>` | D5, D21 |
| Ledger growth | 2 per `<h16>`, 90-day sweep, 300-asset fail-closed bound | D21 |
| 0.3.0 refuses the new pipeline keys; the manual `ci` stays on 0.3.0 | D20 order; README states that the manual `ci` cannot run such a project | D20, R5 |
| Scheduled runs held by the Environment | Owner check; same Environment as push rows | D18 |
| 012 and 011 edit the same four files | 012 merges first; the `secrets_ok` expression and names file are unions | D25 |
| Both tick lanes | scheduled rows use the plain lane only; `run-docker` is untouched | D22 |

| Finding | Handling | Where |
|---|---|---|
| Rollback order breaks every run | Removal first or in one commit with the pin revert; quickstart 4 | D17, Rollout |
| Two runs can hold the turn (in-flight tickets) | Not-`uploaded` under 120 s is live; overlap log; 200-trial canary with racing runners | D1, T006, T029 |
| No monitoring | Loki contract, a test per string, runbook; the owner builds the alert rules in Grafana | D10 |
| Matrix siblings share a run id | Ticket `t-<runid>-<attempt>-<entry>`; no matrix remains, the name still carries the attempt | D1 |
| Cron slack, double fire | Per-entry cron lines off the hour; a double fire queues and runs twice (accepted) | D5 |
| 190 minutes has no margin | 210; budget of 200 minutes; wait ends at budget minus 45 | D16 |
| Rate limits | Pause for `retry-after`, reset or 60 s, at most 120 s; renewals never pause; mutations 1 s apart | D1 |
| Duplicate-name status, one page | Any 4xx re-lists; pagination; fail closed above 300 | D1, D14 |
| Signal timing | 5 s limit, no retry; handler deletes the ticket first | D1 |
| `due` pulls the image | `due` is gone | D5 |
| Image size understated | About +86 MB estimated for 0.4.1; measured size and pull time in the PR; 0.4.0 adds none | D3 |
| arm64 smoke under QEMU | Non-blocking arm64, blocking amd64 | D3, T034 |
| Concurrency cap | At most 3 cron lines; with no ticket cap, waiting VMs are R7 | D5, spec R7 |
| Tick still deployed | Kept on purpose after the owner rule: the tick is not touched (FR-060) | D17 |
| Reviewer on `factory-ci` holds scheduled runs | Owner decision with a recommendation | D18 |
| Staged scope | 0.4.0 without Node; 0.4.1 adds the vault | Summary |

## Security assessment

Amendment of 2026-11-28 (owner rule, its correction and the tick decisions): every earlier row stays, because the vault read, the agent script and the alert file are in scope and the features now run on the tick. The tick path holds the store token, keys and vault credentials only in the run job's final `job` process, only for `main` rows of an entry with the literal `secret_branches = ["main"]`; the new rows follow the table.

| New finding | Handling | Where |
|---|---|---|
| The store write token reaches the run job's environment | Listed only for `main` rows with `secrets_ok`; masked by the frozen script and the redactor; taken once and removed before any thread or child; `ENGINE_ENV` stays seven names; never in argv, a file or a git config | D7, D23, T011, T019 |
| The plan job and plan runner must never hold a write token | They do not (`factory-ci-plan.sh:35-37`, `:66`); the plan reads with `FACTORY_CI_TOKEN`, read scope only | D21, D22, I26 |
| A branch's pipeline file asks for secrets, state or the claim token | Names are listed by the runner by rule, not by the pipeline; only `main`; a glob in `secret_branches` does not count; declared step secrets on other branches keep `NO_JOB_SECRETS` | D23 |
| A forged ledger asset or label by anyone with write access to the store | Name and label regexes; malformed or future labels are stale; the retry cap bounds the harm to two extra runs of one slot | D21 |
| A hostile branch name or project in a claim name | `<h16>` is a hash; names never carry them | D21 |
| A schedule used to run code on a timer from a feature branch | Schedules honoured on `main` of a literal-`secret_branches` entry only | D5, D23 |
| The store repo name in public source | Accepted (R21); the token is the secret | R21 |
| The manual `ci` run with new declarations | Refused before any call | D24 |

| Finding | Handling | Where |
|---|---|---|
| Symlink read can reach the Bitwarden token | Canonical path, `starts_with`, `O_NOFOLLOW|O_NONBLOCK`, size by bytes read, pre-seal scan, `/proc/self` test | D11, T007, T008, T016, T017 |
| Secrets map collision with a vault env name | Separate vault map; name in both refused | D12, T004, T012, T035 |
| Key equality check skipped | Both keys required and compared whenever state or artifacts exist | D4, T002, T011 |
| Forged ticket label locks the queue | Label rule and ticket regex; overflow test | D1, T006, T015 |
| Token on the code repo | Separate store repo, token and agent token scoped to it | D9 |
| Weak keys | Base64 or hex decoding to 32 bytes or more with 8 distinct bytes | D2, T011 |
| Scan gaps | `secret_forms` plus FR-050 strings; format allowlist; every zip member read; `.html` refused; real-trace canary | D13, T007, T016, T029 |
| Allowed branch plus vault | `secret_branches = ["main"]` required; non-secret fixture | D12, T011, T030 |
| Poisoned workspace reaches the state step | Accepted risk with controls (README); `docker = true` refused with `[state]`; no state step after a package-script install | D8, T038 |
| `bw list` loads every item | Org and collection ids, `BW_NOINTERACTION`, items outside the allowlist fail closed (0.4.1) | D3, T033 |
| Restore trusts the release | Regexes, name and size checks, pagination | D14, T014, T017 |
| Agent kit | Pinned script sha, hardened extract, untrusted header, Loki limited to step `ci` | D15, T028 |
| Redirects and hosts | Fixed hosts, redirects off, `*.githubusercontent.com` | D14, T014 |
| chown helper image | Runner image, `chown -hR` | D8, T018 |
| Opaque names, project in tag | `ci-login-1`; tag `ci-state+<repo>+<project>+<state>` | D9, D12 |
| N3, N4, N9, N7 | Accepted; README states them | T038 |

## Open risks

- O1: three GitHub behaviours are not proven here: a duplicate asset name is a 4xx, `PATCH` accepts `label`, asset ids rise. The canary checks them before the first real run. A failing label update is a plan change (observer-side monotonic age).
- O2: `bw list items` returns every item the account sees in its collection; the collection-limited account is an owner action, and an outside item fails closed (0.4.1).
- O3: HKDF assumes machine-made keys; the format check and the README (`openssl rand -base64 33`) are the control. PBKDF2 is the alternative.
- O4: a store outage at registration prints `state store not configured` and logs the cause.
- O5: the secondary limit of 500 content-generating requests per hour caps sustained live tickets at about eight.
- O6: scanning `FACTORY_CI_LOG_USER` (numeric) can refuse a bundle that holds those digits by chance (R11).
- O7: a state step that never writes the file leaves nothing captured, so the old state stays.
- O8: `nodejs` comes from Alpine 3.23's index (0.4.1); `bw --version` at build time catches a break.
- O9: the agent opens bundles with the pinned-hash script; the hash is written at T024.
- O10: the 002 pipeline must order its steps so the state step has no parallel sibling.
- O11: a token expiry shows only as `state store not configured`; the alert rules and a calendar item guard it.
- O12: a canary of 200 trials proves read lag is rare, not absent; overlapping runs still cannot corrupt state, because each save checks its fence.
- O13: a ticket that is broken and registered again goes to the back; accepted residual (FR-011).
- O14: a duplicated tick runs the Playwright login twice in sequence; accepted ("every run must run").
- O15: no cron line exists, so no repo, branch or project of a schedule is public.
- O16: the pinned manual `ci` cannot run a project that uses the new keys (spec R5). The owner accepted the frozen pin; the README says so. The features themselves run on the tick, so they are no longer inert.
- O17: the queue is seldom needed on the tick (serial ticks, one row per project per tick). It stays by the owner's word.
- O18: the tick runner has no container, so D19 changed: `GITHUB_RUN_ATTEMPT`, the row's run id and the clone image as helper. A clone image without `chown` or `chmod` ends stateful runs with `runner fault` (checked at T029).
- O19: the ban is read as covering `.github/workflows/` and `.github/scripts/` only; the `Dockerfile`, the alert YAML, the agent script and `.github/factory-ci-repos.toml` are in scope, and the READMEs still change (FR-041).
- O20: T039 changes a live path that feature 012 also uses; the old digest is recorded first, the rollback is one command, and 012 merges first (D25).
- O21: three owner calls have defaults and may be reversed: the store name `<owner>/factory-ci-state` is derived in code (the plan cannot read Bitwarden and a watch key is impossible); schedules run on `main` only (the claim needs the token that only `main` rows get); the manual `ci` cannot run a project that uses the new keys.
- O22: the plan reads one blob per scheduled project per tick; the cap of 40 per tick and the 5,000-an-hour limit bound the cost, and a cap hit makes the repo `unchecked` for that tick.
- O23: a failed label write after a pass re-runs the slot once after 3600 s (R19).
- O24: a `main` row whose entry lacks the literal `secret_branches` and whose pipeline needs the store is refused by the frozen script, not run without state; this is the loud choice.

## Project Structure

```text
specs/011-ci-state-queue/  spec.md plan.md research.md data-model.md quickstart.md tasks.md
  contracts/ config-files.md release-store.md asset-format.md loki-lines.md
  checklists/requirements.md
rust-ci-runner/  Dockerfile Cargo.toml Cargo.lock .factory-ci.toml
  src/  seal releases queue signal state bundle scan stateful schedule ledger (new), vault (0.4.1), store watch pipeline redact log engine job plan github projects ci main util lib fake (changed), one *_tests.rs per new module
  alerts/factory-ci.yaml (new)  scripts/open-artifact.sh test-open-artifact.sh (new)  tests/no_vendor.rs
```

No file under `.github/workflows/` or `.github/scripts/` changes.

**Structure Decision**: the existing layout; new logic in single-purpose modules, no new crate, no new directory.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| `miniz_oxide` (+ `adler2`), two new lock entries | FR-049 scans zip entries (`trace.zip`); inflate is what std lacks | hand-written inflate is security code to maintain; shelling out to `unzip` adds a binary and text parsing of hostile names |
| `libc` direct dependency (already locked) | FR-009 needs SIGINT and SIGTERM handling | a stale rule alone leaves a cancelled run's ticket for 10 minutes |
| `ring`, `memchr` direct dependencies (already locked) | AEAD, HKDF, fast substring search | new crates for the same primitives |
| A hand-written tar writer and reader | one archive that the scanner and the decryptor both read | a `tar` crate adds a dependency for about 80 lines |
| Node and the `bw` bundle in the image (0.4.1, about +86 MB) | the owner chose Bitwarden Password Manager for vault logins; only the CLI reads it | the standalone binary is 143 MB, glibc-linked; a direct API client reimplements vault crypto |
| A separate store repo, an alert-rule file and an agent script | a code-free repo limits the token; alerts catch a silent stop; the script pins what the agent runs | a shared repo exposes code to a content-write token; no alerts hides a disabled schedule; a bare command lets a swapped binary steal the key |
