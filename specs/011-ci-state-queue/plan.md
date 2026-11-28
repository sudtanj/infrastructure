# Implementation Plan: Scheduled runs, a strict queue, saved state and debug artifacts for the ci command

**Branch**: `011-ci-state-queue` | **Date**: 2026-11-28 (rewritten after the devops assessment, the security threat model and the critic review) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/011-ci-state-queue/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before this plan and `/speckit-analyze` after `tasks.md`; both ran on this revision (see the report in the commit message and the hand-over). The constitution was amended to 1.2.0 in a separate commit (Principle II covers the workflow's own schedule).

## Summary

011 builds on 010 (step secrets). It ships in two runner releases.

**0.4.0**: a strict FIFO queue per state (release assets in a separate private store repo, one ticket per run, no cap), encrypted state restored into one named step only, encrypted debug artifacts behind a secret scan, up to three cron lines in `factory-monorepo-ci.yml` mapped by a `case`, the retired tick workflow, and all 13 runner key names reserved with the vault declarations parsed and refused.

**0.4.1**: the vault read (`bw` from the pinned Bitwarden CLI bundle, run by Alpine's `nodejs`), the `vault_items` read path, FR-042 to FR-047 and FR-056.

Everything else (watch list, Loki, fixed public grammar, redactor, engine) stays as in 008 and 010. Interim website logins use 010 step secrets until 0.4.1.

## Technical Context

**Language/Version**: Rust 2021, `rust:1.90` in the pipeline, musl static binary in the release.

**Primary Dependencies**: `ring` 0.17.14, `memchr` 2.8.3 and `libc` 0.2.191 (all already locked; direct dependencies add one line each to the root package), and two new lock entries, `miniz_oxide` 0.8.9 and `adler2` 2.0.1. 0.4.1 adds `nodejs` and the `bw` bundle to the image.

**Storage**: GitHub releases of a separate private store repo that holds no code. Local files under `--work`. No database.

**Testing**: `cargo test --locked --lib` in the `rust-ci-runner` pipeline with new mock-server groups in their own processes; `factory-ci-selftest.yml` compiles and builds the image. Queue, state and bundle logic runs against an in-memory `Releases` fake.

**Target Platform**: `ghcr.io/sudtanj/rust-ci-runner`, linux/amd64 and arm64, started by `factory-monorepo-ci.yml` on a GitHub-hosted VM.

**Project Type**: CLI, one binary.

**Performance Goals**: poll 20 s, renew 60 s, settle 3 s. State at most 10 MB, bundle at most 100 MB, memory bounded by a 64 KiB chunk plus the scan window.

**Constraints**: the dev VM has 1 GB and no cargo, docker or builds. The runner container has a read-only root and a 64 MB `/tmp` tmpfs (`factory-monorepo-ci.yml`), so bundles and ciphertext go under `--work`. A step container allows 512 processes (`engine.rs`).

**Scale/Scope**: 3 cron lines, 2 state assets, 10 bundles per project, 50 vault fields per pipeline, no ticket cap.

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Runner and workflow stay here. |
| II Secrets source | Pass under 1.2.0. The workflow gains no GitHub secret and may hold only the Bitwarden token when started by hand or by its schedule. Every new setting comes from Bitwarden inside the runner. |
| III Public logs | Pass by design. Nine new fixed lines, no name or count. Details go to Loki as step `ci`. One recorded exception: the repo, ref and project of a cron entry sit in the `run:` text of the workflow, which GitHub prints (FR-001, FR-037); they are already public in the watch list and the dispatch defaults, and the owner is told. |
| IV Simplicity | Pass with justified additions (Complexity Tracking). The critic round removed the schedule file, `due`, the slot marker and the ticket cap. |
| V Tested | Pass. Every FR maps to a test below; tests come first. |
| VI Spec-driven | Pass. Spec, plan, tasks, clarify and analyze all ran on this revision. |

## Architecture decisions

Evidence for each is in [research.md](research.md). The numbering D1 to D18 is stable for the briefs.

### D1 Queue: release assets in a store repo, unlimited, strict order

A release `ci-queue+<repo>+<project>+<state>` in the store repo (prerelease, never `latest`). A ticket is one asset `t-<runid>-<attempt>-<entry>` (one byte) with label `<runid>.<epoch>`. The full protocol is [contracts/release-store.md](contracts/release-store.md). Points that matter:

- Atomic create: GitHub documents that uploading an asset with an existing filename is an error. Order: the asset id, one integer order for every reader (FR-010). Time: the `Date` header of the answers, never the runner clock (FR-013). A label epoch is the last `Date` the holder saw.
- No ticket cap and no wait cap (the critic decision "every run must run"). A run waits until `FACTORY_CI_BUDGET_END` minus 45 minutes; the budget end is the runner start plus 200 minutes unless the variable is set. The one drop is `queue wait expired`, with an alert. A ticket that is deleted and registered again goes to the back (accepted residual).
- Live means `uploaded`, or a non-`uploaded` asset that matches the ticket regex and is younger than 120 s. A non-`uploaded` asset of any other kind is deleted only once it is older than 360 s (the 300 s upload limit plus margin). Retention counts only `uploaded` assets, so two failed uploads never evict the last good state. Stale means a missing, malformed, future (more than 120 s ahead) or 600 s old label. The holder re-lists on each renewal and logs `overlap` when a lower id is live. A canary of 200 trials and two racing runners proves the lag assumptions before the first real run.
- Renewals never pause on a rate limit. Renewals failing for 600 s end the run in this order: set the cancel flag in `RunExtras`, kill the run's step containers by their name prefix (`docker kill`), wait for them to exit, then delete the ticket, print `queue renewal failed` and exit 1; the ticket is never deleted while a step still runs. Other calls pause for `retry-after`, `x-ratelimit-reset` or 60 s, at most 120 s; mutations are 1 s apart.
- Fence: the ticket is checked before and after a state upload; if it vanished after, the new asset is deleted and the run ends `state save failed`.
- Exit paths: a drop guard covers return and panic; a SIGINT/SIGTERM watcher deletes the ticket first, within 5 s, skips the Loki flush, and exits 1. GitHub gives about 10 s after a cancel (7.5 s between the signals plus 2.5 s). Separate tests for each signal.

Rejected: git refs (no payload or timestamp, a reused name hides an old holder), issue comments (needs the issues permission), a queue file through the Contents API (a commit per renewal), releases as tickets (a tag per ticket), the workflow `concurrency` group (cancels the older pending run), a slot-marker asset (a duplicated tick runs twice in sequence instead).

### D2 Crypto: `ring`, ChaCha20-Poly1305 in 64 KiB chunks, HKDF

`ring` 0.17.14 is already in `Cargo.lock` (via rustls). HKDF-SHA256 with a fresh 16-byte salt per file and `info` `factory-ci/v1/state` or `/artifact`; input is the decoded bytes of the key (base64 or hex text decoding to 32 bytes or more, 8 or more distinct byte values). STREAM chunks with a final flag; `open` reads one byte ahead, so a plaintext of exactly 64 KiB has one valid encoding. Layout in [contracts/asset-format.md](contracts/asset-format.md). `decrypt-artifact` reads the key from `FACTORY_CI_ARTIFACT_KEY`. Rejected: `age`, `openssl enc`, an extra AEAD crate, AES-GCM, PBKDF2 (open point O3).

### D3 `bw` (0.4.1 only)

The npm-build bundle of `bitwarden/clients` release `cli-v2026.10.1` (sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3`, 5.1 MB, matches a download) run by Alpine 3.23 `nodejs` 24.22.3 (installed 52.3 MB plus about 15 MB of libraries, index read today) behind a two-line `/usr/local/bin/bw`. The standalone binary is 143 MB, glibc-linked (`readelf` shows interpreter `/lib64/ld-linux-x86-64.so.2` and `libstdc++`, `libc.so.6`); rejected. Estimated image growth is about +86 MB; the PR records the measured size and the pull time. `RUN bw --version` on amd64 blocks; the arm64 run under QEMU does not.

Sequence per run: `login --apikey` (only `BW_CLIENTID`, `BW_CLIENTSECRET`), `unlock --passwordenv BW_PASSWORD --raw`, `list items --organizationid --collectionid` (only `BW_SESSION`, `BW_NOINTERACTION=true`), `logout`, then remove the data dir (a 0700 folder on the tmpfs) and check it is gone, all before the first container. The runner lists items and filters by exact name, so no item name is in argv. An item outside the allowlist in the answer is `vault unavailable`. The `bw` server is the US cloud; with `FACTORY_CI_BWS_SERVER_URL` set and a vault declaration the run ends `vault unavailable`. No new key.

### D4 One list of runner keys, one `bws` call

`store::RUNNER_KEYS: [&str; 13]` replaces 010's `REQUIRED_KEYS`; `REQUIRED_KEYS` becomes its first four (read at start). The others are held from the same single `bws secret list` call: `fetch_with` keeps their rows next to the allowlisted step rows in `Held`, and `Held::take`-style accessors return them on need. Consumers: `valid_secret_name`, `watch::parse`, `Held::take` (refuses runner keys by name), the runner-key accessor, and the `log.rs` fault names. A table test walks the list; membership asserts, never counts. New data goes into `Args`, not new parameters.

### D5 Schedule: cron lines in the workflow

Up to 3 `cron:` lines in `factory-monorepo-ci.yml`; a short `case` on `github.event.schedule` (passed through `env:`) maps each to repo, ref, project and entry number 1 to 3; manual is entry 0; an unknown string fails the job. No schedule file, no `due`, no pinned binary, no `plan --schedule`. `ci` takes `--entry N` as a plain flag. The shell test enforces the shape. A duplicated tick runs twice in sequence through the queue. GitHub documents that scheduled workflows in a public repo are disabled after 60 days without activity, and that high load delays or drops ticks (the start of every hour is named); the owner picks a minute off the hour, the README says to push a commit before day 55, and an alert fires on a missing `pipeline passed`.

### D6 Where the tests run

No-server modules (`seal_tests`, `scan_tests`, `bundle_tests`, `state_tests`, `queue_tests` with at most 6 joined threads, `vault_tests`) run in the first command. `releases_http_tests` and `ci_state_tests` start servers and get their own process groups in `.factory-ci.toml`, with matching `--skip` flags (`--skip ci_tests::` does not match `ci_state_tests::`).

### D7 Order inside `run_checkout` (extends 010)

1. Load the pipeline: parse faults (state, artifacts, vault shape, reserved names, project and tag rules) are `pipeline file refused`.
2. Docker rules (010) plus: a vault list with `docker = true`, and any `docker = true` in a pipeline with `[state]`, are refused.
3. State or artifacts declared: the entry needs `state = true` and the pipeline repo must be private (identity call returns `private`), else `state not allowed`.
4. Allowlists: 010's `unlisted` (`step secret not allowed`), then vault items against `vault_items` (`vault item not allowed`); in 0.4.0 a validated vault declaration ends `vault unavailable` here, before any ticket.
5. Ref gate (`secrets_allowed_on`) for step secrets, vault, state and artifacts.
6. Runner keys from `Held`: state token, store repo, both keys (checked, different); any miss is `state store not configured`. `GET /repos/<store>` must answer `private: true`. Before step 6 reads anything and before any store call or any line that can carry a value (the lines of steps 1 to 5 are fixed text), `log.add_secrets` receives the state token, the state and artifact keys, the store repo name and `base64("x-access-token:" + token)` (and in 0.4.1 the vault credentials) (FR-061). A 5xx on registration is retried inside the wait budget.
7. 010's `Held::take` for step secrets, then `log.add_secrets`.
8. `[state]`: register and wait (`queue wait expired`). A 0.4.0 vault declaration was already refused at step 4, before any ticket. Inside the turn: restore into memory (`state restore failed`), add its strings to the log; 0.4.0: a vault declaration ends `vault unavailable` here; 0.4.1: the vault read.
9. `run_pipeline` with the step hook of D8 and the deadline of D16.
10. After: chown the artifact paths (runner image helper), build, scan, encrypt, upload when due, save the captured state when the pipeline passed (the `after` hook of D8 has already run `state_strings` on the bytes it read and `add_secrets`, so a cookie issued during the run is masked and scanned before any artifact is built), release the ticket, remove the checkout.

### D8 State reaches one step only

A step key `state = true` marks the one step. `[state]` needs exactly one such step, and every other step must be a transitive dependency or dependent of it, so none runs beside it. `RunCtx` gains a `StepHook` (`before`, `after`) called on the step's thread around `docker run`, before the step counts as finished. `before` writes the in-memory plaintext at the declared path (mode 0600, canonical path inside the checkout, `O_NOFOLLOW`; nothing for a first run). `after`, pass or fail, runs the chown helper, reads the file by the rules of D11, keeps the bytes in memory, and removes the file; a removal failure is `runner fault`. The declaring step should end with `chown` to the workspace owner; the helper is the backstop (`chown -hR` and `chmod u+rX`, never `a+r`) and runs the runner's own pinned image (`FACTORY_CI_RUNNER_IMAGE`), not the step image. The state step runs as root like every step; the README says so and tells the owner not to place a state step after an install step that runs package scripts. Rejected: a bind mount of a runner-private file (an empty file where FR-021 promises none; `EBUSY` on rename), a pipeline-wide restore (the 002 finding), a per-step workspace copy.

### D9 Store repo

A separate private repo that holds no code and no workflows, one initial README commit, no tag rulesets that block creation, immutable releases off. Its `owner/name` is `FACTORY_CI_STATE_REPO` (Bitwarden). `FACTORY_CI_STATE_TOKEN` is scoped to it only; the agent's read token to it plus Loki. Tags, names and regexes are in the release-store contract; the `+` delimiter and the `-` root project make the tag mapping injective, with a collision test.

### D10 Monitoring

[contracts/loki-lines.md](contracts/loki-lines.md) fixes every Loki line, each with a test. `rust-ci-runner/alerts/factory-ci.yaml` holds the Grafana rules, no-data set to Alerting: a missing `pipeline passed` for `sudtanj/factory-monorepo` over the cron interval plus 45 minutes; any fault line over 1 h; `queue wait expired`; `queue renewal failed`; any `overlap`; more than one `restore none` in 24 h; more than two `restore failed` in 24 h; any `stale break`; `rate limited` more than 3 times in 1 h. The README carries a runbook for `state restore failed` (delete the newest state asset, or all of them to start empty) and the day-55 note.

### D11 Filesystem trust

The state hook and `bundle::build` canonicalise the declared path, require `starts_with(canonical checkout)`, `lstat`, open with `O_NOFOLLOW|O_NONBLOCK`, and judge size by bytes read. Before `seal`, the runner scans the state plaintext for every known secret form; a hit is `state save failed`. A test points the state path through `/proc/self` and a FIFO.

### D12 Names and maps

A name in both a `secrets` list and a vault `env` is refused pipeline-wide. Vault values live in a separate `(item, field)` map (0.4.1). Entries with `state` or `vault_items` need an explicit `secret_branches = ["main"]` (`plan --check` refuses otherwise). Vault item names in the public watch file are opaque (`ci-login-1`).

### D13 Scan scope

The scan set is `Log::secret_forms()` minus the numeric `FACTORY_CI_LOG_USER` (an identifier that matches digits by chance) (010's `Redactor::forms`: the value, three base64 alignments, URL-safe base64, percent-encoding, JSON escape, hex, trimmed lines) plus the FR-050 state strings. The scan reads every zip member, `trace.network` included. Members are allowed only as text, JSON, zip (stored or deflate), PNG or JPEG, judged by content; anything else, and any `.html` member, is refused (the Playwright HTML reporter stays off; 002 uses its own reporter). Tar names with `..`, a leading `/` or a control character are refused. A canary on one real Playwright login-failure trace from the 002 fixture runs before the first real run.

### D14 Store hygiene

Fixed hosts, redirects off, download `Location` only https on `*.githubusercontent.com`, name and size checks before a download, pagination with a 300-asset fail-closed bound, one retry of a restore on a 404.

### D15 Agent kit

`rust-ci-runner/scripts/open-artifact.sh` downloads the runner binary of a release and runs it only if its sha256 equals the value pinned in the script, decrypts, checks the exit status, extracts with `tar -x --no-same-owner` into a new empty directory, and prints files stripped of control characters and ANSI, size-capped, under an untrusted-data header. The README names the pins that move per release (image digest, script sha) in a release checklist line.

### D16 Deadline

`budget_end` is read through `Seams.timing` (tests set it; production leaves the variable unset): the value of `FACTORY_CI_BUDGET_END` if set, else runner start plus 200 minutes (job limit 210 minus 10 for start-up and cleanup). Pipeline deadline `= min(now + 30 min, budget_end - 20 min)`; the 20 minutes are 5 for the artifact scan and upload, 5 for the save, 5 for a vault retry and 5 for cleanup. The wait ends at `budget_end - 45 min` (5 restore, 5 vault, 30 pipeline, 5 save). The latest winner has 25 minutes before the 20-minute reserve; after restore and vault that leaves a minimum pipeline window of 15 minutes, which the README documents.

### D17 Rollback order and the retired tick

Rollback removes `state`, `vault_items`, the added `secret_branches` and the cron lines with their `case` branches, in one commit with the pin revert or before it, because a 0.3.0 runner refuses unknown watch keys. The tick workflow `factory-ci-tick.yml` is deleted in 011 and stays deleted. The `plan` subcommand and the shell scripts only the tick used are left for a later cleanup feature unless the selftest still needs them (T022 greps and says which).

### D18 Environment

The README tells the owner to put the owner as required reviewer on `factory-ci`. A reviewer holds a scheduled run until someone approves it. Owner decision (see Owner actions): drop the reviewer, or put the scheduled job in a second Environment. Recommended: `factory-ci-schedule`, limited to `main`, no reviewer, holding the same token. The job then reads `environment: ${{ github.event_name == 'schedule' && 'factory-ci-schedule' || 'factory-ci' }}`.

## Files

### Added (`rust-ci-runner/src` unless noted)

`seal.rs`, `releases.rs` (the only code that calls api.github.com with the state token), `queue.rs`, `signal.rs` (the one `unsafe`: a `libc::sigaction` call that stores an atomic), `state.rs`, `bundle.rs`, `scan.rs`, `stateful.rs` (post-checkout phase), `vault.rs` (0.4.1, the only new file that names the vendor), one `*_tests.rs` per module, `ci_state_tests.rs`, `releases_http_tests.rs`, `rust-ci-runner/alerts/factory-ci.yaml`, `rust-ci-runner/scripts/open-artifact.sh`, and the spec artifacts.

### Changed

| File | Change |
|---|---|
| `store.rs` | `RUNNER_KEYS` (13) replaces `REQUIRED_KEYS`, which becomes its first four. `fetch_with` also keeps the rows of the nine other runner keys in `Held`; accessors return them on need with the 010 value rule; key format and equality checks. |
| `watch.rs` | Keys `state`, `vault_items`; both need an explicit `secret_branches = ["main"]`; runner-key names in `secrets` refuse the file. |
| `pipeline.rs` | `[state]`, `[artifacts]`, `Step.state`, `Step.vault`, the rules of data-model.md, `valid_secret_name` uses `RUNNER_KEYS` and refuses the four prefixes in `env`, `secrets` and vault `env`. |
| `redact.rs`, `log.rs` | Nothing new in the redactor (010's `forms`, `secret_forms`, `longest_secret` serve the scan); `FAULTS` gains nine lines and `public_form` the queue line; membership asserts. |
| `engine.rs`, `job.rs` | `StepHook` in `RunCtx`; `Engine::kill_run(prefix)`; `RunExtras` also carries the cancel flag; `job.rs` `run_pipeline` takes `RunExtras` (deadline and hook); `Engine::own_paths` (runner image, `chown -hR`, `chmod u+rX`); 0.4.1: vault env export from the separate map. |
| `ci.rs` | `Args` gains `entry`, the store settings, the budget; `Seams` gains `uploads_base`, `hub`, `timing`, `exit`, `runner_image`; identity returns `private`; D7 order. New data goes into `Args`, not new parameters. |
| `main.rs` | `decrypt-artifact`; `run_ci` installs the signal handler; usage text. |
| `util.rs` | `parse_http_date`. |
| `.factory-ci.toml` | Two process groups and the `--skip` flags. |
| `Cargo.toml`, `Cargo.lock` | 0.4.0 (then 0.4.1); `ring`, `memchr`, `libc`, `miniz_oxide`; two new lock entries by hand. |
| `.github/workflows/factory-monorepo-ci.yml` | D5 shape, 210 minutes, no `concurrency`, `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_RUNNER_IMAGE`, Environment per D18. |
| `.github/workflows/factory-ci-tick.yml` | Deleted. |
| `.github/workflows/factory-ci-selftest.yml`, `.github/scripts/*` | Drop references to the tick; keep what else uses them. |
| `.github/scripts/test-factory-monorepo-ci.sh` | New workflow checks. |
| `.github/factory-ci-repos.toml` | Header comment; entry keys at T030 only. |
| `rust-ci-runner/tests/no_vendor.rs` | Allow `vault.rs` (0.4.1). |
| `rust-ci-runner/Dockerfile` | 0.4.1: Node, the `bw` bundle, wrapper, smoke. |
| `README.md`, `rust-ci-runner/README.md`, `.specify/memory/constitution.md` | Docs last; constitution already amended. |

### Not changed

`ship.rs` keeps its interface. `job.rs` changes minimally: `run_pipeline` builds `RunCtx` itself with a fixed 30-minute deadline, so it gains one `RunExtras { deadline: Instant, hook: Option<&dyn StepHook> }` argument (it already carries `#[allow(clippy::too_many_arguments)]`); `local` and `job` pass the old 30-minute deadline and no hook.

## Data and API contracts

[contracts/config-files.md](contracts/config-files.md), [contracts/release-store.md](contracts/release-store.md), [contracts/asset-format.md](contracts/asset-format.md), [contracts/loki-lines.md](contracts/loki-lines.md), [data-model.md](data-model.md).

## Engineer briefs

Precondition: 010 is merged to `main` and released (0.3.0). Names used here are 010's real ones (`wt-infra-010`, `specs/010-ci-step-secrets/tasks.md`): `REQUIRED_KEYS`, `fetch_with`, `Fetched`, `Held::take`, `StepSecrets`, `NamedError { Count { index, found }, Unusable { index } }`, `Redactor::add`, `Redactor::forms`, `Log::add_secrets`, `Log::secret_forms`, `Log::longest_secret`, `WatchEntry::secrets_allowed_on`, `Args`. If a name differs after 010 lands, follow 010. Do not assume a `FAULTS` length.

Order. T001 first, alone (skeleton, dependencies, lock, and the `Releases` trait with its types). T002 to T004 and T009 to T010 (tests first, 0.4.0) run in parallel after T001; T005 (the fake and the HTTP tests) runs after T001, and T006, T007, T008 run after T005 because they use `FakeReleases`. T011 to T023 (code) run in parallel after T001 and their own tests, except: T014 before T015, T016, T017; T018 before T015 and T017 (the cancel flag, the kill seam and the hook); T013 before T017 and T020; T015 before T020 (the signal install); T019 after T011 to T018; T021 after T019 and T020; T022 runs after T010 (both edit `.github/scripts/*`) and finishes before T030; T023 merges after T025. T024 to T031 are sequential after the code is on `main`: T025 after T024; T026 after T025 and T023; T027 and T028 after T024; T029 after T022, T027 and T028; T030 after T022, T025, T026 and T029; T031 after T025 and T027 to T030. Phase 5 (0.4.1, T032 to T037) starts after T031. T038 (docs) is last.

### T001 Skeleton, dependencies, lock
- Goal: new empty modules compile; dependencies and lock proved alone; the `Releases` trait exists for the tests.
- Files: `lib.rs`, new empty files, `Cargo.toml`, `Cargo.lock`.
- Acceptance: `lib.rs` declares `seal`, `releases`, `queue`, `signal`, `state`, `bundle`, `scan`, `stateful`, and test modules `seal_tests`, `releases_http_tests`, `queue_tests`, `state_tests`, `bundle_tests`, `scan_tests`, `ci_state_tests` (`vault` and `vault_tests` come in T033). `Cargo.toml` adds `ring = { version = "0.17", default-features = false, features = ["alloc"] }`, `memchr = "2"`, `libc = "0.2"`, `miniz_oxide = { version = "0.8", default-features = false, features = ["with-alloc"] }`. `Cargo.lock`: the four names join the root package dependencies alphabetically; new entries `adler2` 2.0.1 (checksum `320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa`) and `miniz_oxide` 0.8.9 (checksum `1fa76a2c86f704bdb222d66965fb3d63269ce38518b83cb0575fca855ebb6316`, dependency `adler2`).
- Also: `engine.rs` declares `trait Killer { fn kill_run(&self, prefix: &str) -> Result<(), String>; }` (the kill seam; `Engine` implements it in T018, T005 provides a recording `FakeKiller` that shares an event log with `FakeReleases`). `releases.rs` declares `trait Releases` and its types (`Asset`, `ReleaseError`, the list and renew shapes) with unimplemented bodies, so T005 to T008 compile against it; T014 fills in `HttpReleases`.
- Constraints: no cargo here; check both checksums against `https://index.crates.io/mi/ni/miniz_oxide` and `https://index.crates.io/ad/le/adler2`. T001 lands alone on a branch first so the selftest proves `--locked`.

### T011 Runner keys and watch keys (FR-019, FR-020, FR-034, FR-043, FR-059)
- Goal: thirteen runner keys, one list, one `bws` call.
- Files: `store.rs`, `watch.rs`.
- Acceptance: `RUNNER_KEYS` has the thirteen names; `REQUIRED_KEYS` is its first four and `fetch`, `parse_list` behave as before. `fetch_with` keeps, next to 010's allowlisted rows, the rows of the nine other runner keys; accessors take them with the 010 value rule and report `NamedError` by index into the requested list. A run with `[state]` or `[artifacts]` needs the token, the store repo and both keys: a missing or duplicated value, a key that is not base64 or hex decoding to 32 bytes or more with 8 distinct byte values, or equal keys, is an error (`state store not configured`). `Held::take` refuses any runner-key name. `watch::parse` accepts `state` (bool, needs `id`) and `vault_items` (at most 20, each 1 to 100 characters, no control character, no duplicate), and refuses the file when either is present without `secret_branches = ["main"]` written explicitly; runner-key names in `secrets` refuse the file.
- Also: `FACTORY_CI_STATE_REPO` must have the shape `owner/name` (the 010 repo-name rule), else the accessor errors (`state store not configured`). `FACTORY_CI_BUDGET_END` is not read here (see T019).
- Constraints: no vendor words in `watch.rs`. Membership asserts, never counts. No second `bws` call.

### T012 Pipeline declarations (FR-017, FR-018, FR-042, FR-059)
- Goal: parse and validate `[state]`, `[artifacts]`, `state = true`, `vault`.
- Files: `pipeline.rs`.
- Acceptance: the rules of data-model.md, including: exactly one `state = true` step, ordering with every other step, artifact paths by component against the state path, name checks (`BW_`, `BWS_`, `BITWARDENCLI`, `FACTORY_CI`, runner keys) in `env`, `secrets` and vault `env`, a name in both a `secrets` list and a vault `env`, vault limits (20 and 50), `docker = true` with a vault list or with `[state]`, a project with `..` or ending `.` or `.lock` with `[state]` or `[artifacts]`, a tag over 200 bytes. `vault_wants()` returns each `(item, field)` once. Grep the fixtures and `rust-ci-runner/.factory-ci.toml` for `env` keys with the four prefixes.
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
- Acceptance: the protocol of the release-store contract. `with_turn` registers (replacing a same-name ticket of a rerun), prints `waiting for the queue` once, polls every 20 s, renews every 60 s from a scoped thread, claims after the 3 s settle re-list, stops at the budget (`queue wait expired`), runs the closure, and deletes the ticket on every path including a panic. Live and stale rules, the regexes and the label rule are pure functions with boundary tests (119 and 120 s, 599 and 600 s, label 120 and 121 s ahead, u64 overflow, missing label). A renewal never pauses; renewals failing for 600 s stop the run in this order: set the cancel flag, kill the run's step containers through the `Killer` seam, wait for them to exit, then delete the ticket, print `queue renewal failed` and exit 1. The signal handler deletes the ticket first within 5 s, skips the Loki flush, and exits 1 through `Seams.exit`; SIGINT and SIGTERM are tested separately. `Fence::held()` re-checks the ticket.
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

### T018 Log and engine (FR-035, FR-037)
- Goal: the public grammar and the hook.
- Files: `log.rs`, `engine.rs`, `job.rs` (`RunExtras`: deadline and hook into `RunCtx`; callers other than `ci` pass the old 30 minutes and no hook).
- Acceptance: `FAULTS` gains the nine lines of FR-037 and `public_form` accepts the queue line (membership asserts). `StepHook` is called on the step's thread around `docker run`, before the step is marked finished. `own_paths` runs the runner image with `chown -hR` then `chmod -R u+rX`, never `a+r`, with arguments, not a shell string built from names.
- Also: `RunExtras` carries `cancel: Arc<AtomicBool>`; the step loop checks it the way the signal path does and stops starting steps; `Engine` gets `kill_run(prefix)` that runs `docker kill` on the run's containers and waits. `own_paths` runs the helper with `--network none`, no `-e`, and an explicit `--entrypoint chown` (then a second call with `--entrypoint chmod`); a test asserts its argv and environment hold no secret.
- Constraints: `log.rs` and `engine.rs` already may name the vendor; nothing else changes.

### T019 Wire `ci` (FR-001, FR-002, FR-006 to FR-009, FR-019 to FR-027, FR-049 to FR-055, FR-059)
- Goal: `ci` runs D7.
- Files: `ci.rs`, `stateful.rs`.
- Acceptance: the order of D7, each fault with its exact line, exit code 1, no container on a refusal. `--entry N` (digits 0 to 3, default 0). `identity` returns `private`. The deadline of D16. `stateful.rs` runs restore, the (0.4.0) vault refusal, the pipeline with the hook, `own_paths`, artifacts, save, release. A scan refusal makes exit 1 and does not stop the state save. Artifacts run when the pipeline did not pass and a step started, or `when = "always"`. A validated vault declaration ends `vault unavailable` before a container (0.4.0).
- Also: before any line that can carry a value (steps 1 to 5 print fixed text) and before any store call, call `log.add_secrets` with the state token, the state and artifact keys, the store repo name, `base64("x-access-token:" + token)` (and in 0.4.1 the vault credentials) (FR-061); read the budget end through `Seams.timing`, not `std::env`; refuse a validated vault declaration (0.4.0) at D7 step 4, before a ticket; pass the cancel flag to the renewal thread.
- Constraints: state token, keys and vault credentials reach only `releases.rs`, `seal.rs` and `vault.rs`. New data goes into `Args`. No `set_var`.

### T020 `main.rs` (FR-033)
- Goal: `decrypt-artifact` and the signal handler.
- Files: `main.rs`.
- Acceptance: `decrypt-artifact` reads only `FACTORY_CI_ARTIFACT_KEY`, streams stdin to stdout through `seal::open`, exits non-zero with one generic line on any error; `run_ci` calls `signal::install()`; usage text lists the command.
- Also: after T013 and T015. Tests (T003, T009): the key is read from the environment only (a key in argv or stdin is ignored), is never echoed, any error prints one generic stderr line, a truncated stream exits non-zero, and `signal::install()` is called by `run_ci` before anything else.
- Constraints: never echo the key.

### T021 Version and groups (FR-038)
- Goal: 0.4.0 and the process groups.
- Files: `Cargo.toml`, `Cargo.lock`, `.factory-ci.toml`.
- Acceptance: both say `0.4.0` for `rust-ci-runner`; the first command gains `--skip releases_http_tests:: --skip ci_state_tests::` and two commands run those groups; the header says why.
- Constraints: by hand; no other lock entry.

### T022 Retire the tick (FR-060)
- Goal: `factory-ci-tick.yml` is gone before the watch file changes.
- Files: `.github/workflows/factory-ci-tick.yml` (delete), `.github/workflows/factory-ci-selftest.yml`, `.github/scripts/*`, `README.md`, `rust-ci-runner/README.md`.
- Acceptance: the workflow is deleted; the selftest and shell tests no longer name it; the grep that proves no reference remains covers `.github/**`, `README.md`, `rust-ci-runner/README.md` and `rust-ci-runner/**` only (specs are history and stay); `.github/scripts/factory-ci-lint-workflow.sh` drops its tick rules and its self-test is retargeted to the release workflow (`factory-ci-selftest.yml` lines for the lint and `--self-test`); README mentions and the owner settings only the tick used (the docker variants of the token and Bitwarden token, the blob keys, the heartbeat URL, the clone and dind image variables, `RUNNER_IMAGE_DIGEST`, the Environments `factory-ci-plan` and `factory-ci-docker`) leave the docs and are listed as owner deletions in the pull request. The `plan` subcommand and the scripts stay unless the selftest needs them; the task writes down which and leaves their removal to a later cleanup feature.
- Constraints: do not widen: no deletion of runner code. Rollback keeps the tick deleted.

### T023 Workflow (FR-001 to FR-004, FR-039)
- Goal: cron lines, `case`, 210 minutes.
- Files: `.github/workflows/factory-monorepo-ci.yml`, `.github/scripts/test-factory-monorepo-ci.sh` (rules in T010).
- Acceptance: contracts/config-files.md. The `docker run` stays single, passes `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_RUNNER_IMAGE`, no `concurrency`, no `due`, timeout 210, `environment` per D18, no new secret, every `uses:` pinned, no `${{` in `run:`.
- Also: T023 lands with zero `cron` lines and no `schedule:` key (so merging it starts nothing); T030 adds the first `cron` line, its `case` branch, the `schedule:` key and the header of `.github/factory-ci-repos.toml`. `FACTORY_CI_RUNNER_IMAGE` takes its value from `RUNNER_IMAGE`. The header comment explaining the removed concurrency group is checked by the shell test (T010).
- Constraints: merge after T025 (the old image does not know `--entry`).

### T024 Gate and tag (FR-038)
- Goal: a published 0.4.0 image and digest.
- Files: none.
- Acceptance: T001 to T022 are on `main`, `make lint` and the selftest pass, the `rust-ci-runner` pipeline passed on that commit (run id and SHA equal), the release run finished with its smoke step, the digest is verified by `docker pull`, and the sha256 of `rust-ci-runner-x86_64-linux` is recorded.
- Constraints: the owner or lead pushes the tag; never move a tag.

### T025 Pin (FR-038)
- Goal: the workflow runs 0.4.0.
- Files: `.github/workflows/factory-monorepo-ci.yml` (the `RUNNER_IMAGE` line).
- Acceptance: the line reads `ghcr.io/sudtanj/rust-ci-runner:0.4.0@sha256:<64 hex>` and nothing else changes; the commit goes to a branch first so the selftest runs, then fast-forward `main`.
- Constraints: separate reviewed commit; no placeholder digest.

### T026 No-secret canary
- Goal: 0.4.0 works with nothing new configured.
- Files: none.
- Acceptance: `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner` ends `[runner] pipeline passed` with grammar lines only; after T023 lands, one manual dispatch of the new workflow passes the same way (`ci 0`).
- Constraints: on failure revert the pin first.

### T027 Alert rules (FR-057)
- Goal: a silent system never reads as healthy.
- Files: `rust-ci-runner/alerts/factory-ci.yaml`.
- Acceptance: the rules of D10 with no-data Alerting; every string matches contracts/loki-lines.md or `FAULTS`; a shell check greps each string from both; no webhook URL.
- Constraints: names and labels only. After T024.

### T028 Agent script (FR-033, D15)
- Goal: one hardened command for the debug agent.
- Files: `rust-ci-runner/scripts/open-artifact.sh`.
- Acceptance: as D15, plus a shell test with a fake binary of a wrong sha, an empty-directory extraction and control-character stripping.
- Constraints: shellcheck-clean; never prints the key. After T024.

### T029 Owner actions and release canary (FR-020)
- Goal: the settings exist and the GitHub assumptions are proven.
- Files: none.
- Acceptance: the owner does Owner actions 1 to 9 (below), then quickstart section 2 passes on the real store repo, including the real Playwright failure trace scan.
- Also: after T022, T027 and T028. A canary trace taken after a login with restored state runs through the scan; it must be refused unless the 002 scrub removed the cookies. A refusal is by design, and the README says traces from logged-in runs may be refused.
- Constraints: values are created and read only by the owner; no agent sees a credential.

### T030 Watch entry and first cron line (FR-040)
- Goal: only `sudtanj/factory-monorepo` may keep state.
- Files: `.github/factory-ci-repos.toml`, `.github/workflows/factory-monorepo-ci.yml`.
- Acceptance: the `sudtanj/factory-monorepo` entry gains `state = true` and `secret_branches = ["main"]`; `sudtanj/infrastructure` gains neither; the first cron line and its `case` branch land when the Playwright pipeline has landed; `plan --check` passes. After T022 and T029.
- Also: after T022, T025, T026 and T029. This task owns the header of `.github/factory-ci-repos.toml`.
- Constraints: names only, no value.

### T031 First real run (SC-001 to SC-005, SC-009)
- Goal: the Playwright project runs with restored state and a readable failure.
- Files: none.
- Acceptance: quickstart section 3 steps 1 to 5 and 7 to 9 (step 8 is the cron check).
- Constraints: only after T025, T027, T028, T029, T030.

### Phase 5, runner 0.4.1 (vault)

### T032 Vault tests (FR-042 to FR-047, FR-056, SC-007)
- Goal: tests first for the vault read.
- Files: `vault_tests.rs` (fake `bw` script), additions to `ci_state_tests.rs`, `lib.rs` (`vault`, `vault_tests`).
- Acceptance: the tests listed under "Test plan, 0.4.1".

### T033 Vault read (FR-044 to FR-047, FR-056)
- Goal: read only the declared fields and leave nothing behind.
- Files: `vault.rs`, `tests/no_vendor.rs`.
- Acceptance: D3's sequence with 60 s per call and one retry of the whole sequence on a timeout or crash, never after a rejected credential; exactly-one login item per name by exact bytes; fields `username`, `password`, `uri`, custom; values with NUL or a trimmed line under 4 characters (4 passes) are `Unavailable`; data dir 0700 removed and checked; items outside the allowlist, a set `FACTORY_CI_BWS_SERVER_URL`, or any failure end `vault unavailable`; errors and logs hold positions and counts only; the session key joins the redactor.
- Constraints: no item or field name in argv, a file or a log; each call gets only its own variables.

### T034 Image (FR-044)
- Goal: `bw` runs in the image, pinned.
- Files: `rust-ci-runner/Dockerfile`, `.github/workflows/rust-ci-runner-release.yml` (smoke).
- Acceptance: `ADD --checksum=sha256:43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3` of `bitwarden-cli-2026.10.1-npm-build.zip` in a throwaway stage; files copied to `/opt/bw`; `apk add nodejs`; the `bw` wrapper; `RUN bw --version` per architecture; the amd64 smoke blocks, the arm64 smoke is non-blocking; measured size and pull time in the PR.
- Constraints: if `bw --version` fails under Node 24, stop and report; do not switch to the glibc binary without a plan change.

### T035 Wire the vault and release 0.4.1
- Goal: replace the 0.4.0 refusal with the read.
- Files: `ci.rs`, `stateful.rs`, `engine.rs`, `Cargo.toml`, `Cargo.lock`.
- Acceptance: a validated declaration reads the vault inside the queue turn (or before the pipeline when there is no `[state]`), values go into the separate `(item, field)` map, `run_step` exports each step's own entries from it, the log learns the values before any line, version `0.4.1`.
- Constraints: a sibling step never gets a value.

### T036 Tag, pin and canary 0.4.1
- Goal: a published 0.4.1 and a passing canary.
- Files: `.github/workflows/factory-monorepo-ci.yml` (the pin).
- Acceptance: as T024 to T026 for 0.4.1; the pin in its own commit.
- Also: update the pinned sha256 in `rust-ci-runner/scripts/open-artifact.sh` (T028) to the 0.4.1 binary.

### T037 Vault owner actions and first vault run
- Goal: the vault login works end to end.
- Files: `.github/factory-ci-repos.toml`.
- Acceptance: the owner creates the vault entries and the collection account; the entry gains `vault_items = ["ci-login-1"]`; the canary of quickstart section 2 (`bw` row) and section 3 step 6 pass.

### T038 Docs (FR-041), last
- Goal: both READMEs and the repo description match.
- Files: `README.md`, `rust-ci-runner/README.md`, `gh repo edit`.
- Acceptance: they state what FR-041 lists, including the release checklist line (pins that move per release: image digest, agent script sha), the `state restore failed` runbook, the day-55 note, the S5 wording (a state step should not follow an install step that runs package scripts), that the state step runs as root, that a full queue no longer exists and `queue wait expired` is the one drop, the retired tick, the Environment decision, and the accepted risks. No value appears.
- Also: the README says that traces from logged-in runs may be refused by the scan and that this is by design; that a burst of renewals at the secondary rate limit can trigger `queue renewal failed`, which an alert covers; that a late winner has a minimum pipeline window of 15 minutes; that a step on reviewed `main` code could print a transformed secret (accepted); and that the repo, ref and project of a cron entry appear in the workflow `run:` text (FR-001, FR-037).
- Constraints: stop-slop applies.

## Test plan (qa writes it first)

`FakeReleases` (test-only, in `fake.rs`) is an in-memory `Releases`: monotonic ids, a settable server clock, duplicate-name refusal, per-call fault injection, a lag switch, an "in flight" switch, a call log. Tests set the clock and never sleep. Use unique canaries per secret and search every sink (`W::bootstrap()` in `ci_tests.rs` shows the method). Membership asserts, never counts, for `RUNNER_KEYS` and `FAULTS`.

### 0.4.0

| ID | File | Tests | FR |
|---|---|---|---|
| T002 | `store_tests.rs`, `secret_names_tests.rs`, `watch_tests.rs` | `runner_keys_hold_the_thirteen_names_and_required_keys_are_the_first_four`; `fetch_with_keeps_runner_key_rows_in_the_one_call` (stub counts `bws` calls: one); `held_refuses_a_runner_key_name`; `key_decoding_to_31_bytes_fails_32_passes_low_variety_fails`; `equal_state_and_artifact_keys_fail_in_a_state_only_run`; `missing_store_repo_or_token_fails`; `every_runner_key_is_refused_by_every_consumer_in_any_case`; `state_and_vault_items_need_secret_branches_main`; `state_needs_a_numeric_id`; `vault_items_rules` | FR-019, FR-020, FR-034, FR-043, FR-059 |
| T003 | `seal_tests.rs` | `round_trip_empty_one_chunk_exact_chunk_and_10mb`; `plaintext_of_exactly_64kib_has_one_encoding`; `wrong_kind_key_fails_both_ways`; `flipped_byte_fails`; `truncation_at_chunk_boundary_fails`; `reordered_chunks_fail`; `trailing_bytes_fail`; `bad_magic_fails`; `two_seals_differ`; `error_text_holds_no_key_or_plaintext` | FR-027 |
| T004 | `pipeline.rs` tests | `state_table_rules` (name 64 passes 65 fails, bad characters, paths, 20 and 21, `.ci-state` and `.ci-state-2` pass, artifact equal, containing, under refused); `exactly_one_step_declares_state`; `state_step_must_be_ordered_with_every_other_step`; `artifacts_when_values`; `vault_entry_shape_and_limits`; `reserved_prefixes_refused_in_env_secrets_and_vault_env_in_any_case`; `name_in_secrets_and_vault_env_is_refused`; `docker_step_with_vault_or_with_state_is_refused`; `project_with_dotdot_dot_end_or_lock_is_refused_for_state`; `tag_over_200_bytes_is_refused`; `tag_mapping_is_injective` (collision table); `second_state_table_is_a_toml_error` | FR-017, FR-018, FR-042, FR-059 |
| T005 | `releases_http_tests.rs` (own group), `util.rs` tests | `upload_sends_bearer_and_content_length_and_no_token_in_url`; `any_4xx_on_create_is_conflict`; `retry_once_on_5xx`; `pause_on_403_429_at_most_120s_renewals_exempt`; `mutations_are_1s_apart`; `download_redirect_host_must_be_githubusercontent_https`; `download_checks_name_and_size_first`; `restore_retries_once_on_404`; `half_created_own_asset_is_deleted_after_a_timeout_then_one_retry`; `full_page_paginates_and_over_300_fails_closed`; `store_must_be_private`; `date_header_parse`; `token_never_in_error_text_or_debug` | FR-013, FR-036, FR-051, FR-054 |
| T006 | `queue_tests.rs` (fake, at most 6 joined threads) | `fifo_is_asset_id_order`; `there_is_no_ticket_cap` (6 waiters all run in order); `second_waits_and_prints_one_line_once`; `stale_at_600s_not_at_599s`; `stale_ticket_removed_by_the_next_waiter_with_a_loki_line`; `ticket_pattern_not_uploaded_younger_than_120s_is_live_and_120s_is_not`; `other_kind_not_uploaded_is_deleted_only_after_360s` (360 kept, 361 deleted); `two_failed_uploads_keep_the_last_good_state`; `forged_label_negative_age_overflow_missing_are_stale`; `label_120s_ahead_passes_121s_is_stale`; `junk_assets_are_ignored`; `late_lower_id_ticket_sends_the_claimer_back`; `slow_upload_with_a_lower_id_is_seen_at_renewal_and_logged_as_overlap`; `wait_expires_at_budget_minus_45_and_removes_the_ticket`; `budget_defaults_to_start_plus_200_minutes`; `deadline_is_the_smaller_of_30_minutes_and_budget_minus_20`; `matrix_siblings_with_one_run_id_do_not_replace_each_other`; `rerun_of_one_run_id_replaces_its_ticket`; `renewal_never_pauses_on_a_rate_limit`; `renewals_failing_for_600s_end_the_run_with_queue_renewal_failed`; `every_exit_path_removes_the_ticket`; `sigint_and_sigterm_delete_the_ticket_within_5s_and_skip_the_flush` (two tests); `own_ticket_gone_before_save_writes_nothing`; `own_ticket_gone_after_upload_deletes_the_new_asset`; `exit_delete_has_a_5s_limit_and_no_retry`; `server_clock_skew_does_not_matter`; `two_waiters_deleting_one_stale_ticket_is_fine` | FR-006 to FR-016, FR-052 to FR-054 |
| T006 (critic 2) | `queue_tests.rs` | `renewal_failure_kills_the_containers_then_deletes_the_ticket` (`FakeKiller` and `FakeReleases` share one event log: the kill and the wait for exit come before the ticket delete; a step still running blocks the delete); `cancel_flag_stops_the_step_loop`; `registration_5xx_is_retried_inside_the_wait_budget` | FR-014, FR-054 |
| T003 (critic 2) | `seal_tests.rs`, `ci_state_tests.rs` | `decrypt_artifact_reads_the_key_from_the_environment_only`; `decrypt_artifact_never_echoes_the_key`; `decrypt_artifact_error_prints_one_generic_stderr_line`; `decrypt_artifact_truncated_stream_exits_non_zero`; `run_ci_installs_the_signal_handler_first` | FR-033, FR-009 |
| T008 (critic 2) | `state_tests.rs` | `cookie_set_during_the_run_in_a_trace_refuses_the_artifact` (after hook adds the strings before bundling); `own_paths_argv_and_environment_hold_no_secret` (`--network none`, no `-e`, `--entrypoint chown`) | FR-050, FR-055 |
| T009 (critic 2) | `ci_state_tests.rs` | `canaries_include_the_base64_and_x_access_token_forms_of_the_state_token` (state token, both keys, store repo name; before the first log line and the first store call); `vault_declaration_refused_before_a_ticket_in_0_4_0`; `budget_end_comes_from_seams_timing`; `store_repo_shape_is_checked`; `loki_strings_0_4_0` and `loki_strings_0_4_1` as two lists; `trace_after_login_with_restored_state_is_refused_unless_scrubbed` | FR-020, FR-061, FR-049 |
| T007 | `bundle_tests.rs`, `scan_tests.rs` | `folder_bundled_whole_missing_path_skipped`; `symlink_and_special_files_skipped`; `path_through_a_symlinked_parent_is_refused` (canonical `starts_with`); `fifo_at_the_path_does_not_block`; `size_is_by_bytes_read`; `tar_names_with_dotdot_absolute_or_control_are_refused`; `long_names_use_pax`; `bundle_cap_at_the_limit_passes_one_over_refuses` (cap injected); `retention_keeps_10_uploaded`; `upload_failure_leaves_the_exit_code`; `sentinel_in_plain_base64_and_one_line_of_multiline_is_refused` (every `secret_forms` form, in a plain member, in `trace.network` inside a zip, in a nested zip at depth 2); `state_strings_join_the_scan_set` (FR-050); `depth_3_zip_is_refused`; `html_member_is_refused`; `member_that_is_not_text_json_zip_png_jpeg_is_refused`; `zip_method_other_than_stored_or_deflate_is_refused`; `encrypted_entry_zip64_bad_crc_over_10000_entries_over_500mb_refused` (limits injected); `value_across_a_chunk_edge_is_found`; `4_char_found_3_char_ignored`; `refusal_reports_a_count_only`; `clean_bundle_passes` | FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058 |
| T008 | `state_tests.rs` | `first_run_has_no_state_and_no_file`; `restore_picks_the_highest_id_uploaded_asset`; `manual_run_asset_restores`; `restore_fails_on_wrong_key_truncated_tampered_and_never_falls_back`; `restore_checks_name_and_size_before_download`; `save_fresh_nonce_keeps_the_two_newest_uploaded`; `crash_between_upload_and_delete_leaves_two_and_newest_wins`; `save_refuses_symlink_empty_over_10mb` (10 MB passes); `failed_pipeline_never_changes_the_store`; `save_failure_keeps_previous_state`; `store_holds_only_ciphertext`; `state_plaintext_holding_a_known_secret_is_not_saved`; `state_path_through_proc_self_captures_nothing`; `json_value_strings_16_chars_added_15_not`; `non_json_under_1mb_added_whole`; `state_file_exists_only_during_the_declaring_step` (absent before, 0600 inside, gone after); `no_other_step_sees_the_file`; `file_is_never_world_readable`; `removal_failure_stops_the_run`; `own_paths_runs_the_runner_image_with_chown_hr` | FR-021 to FR-027, FR-050, FR-055, SC-005 |
| T009 | `ci_state_tests.rs` (own group) | `counter_pipeline_sees_0_1_2_across_three_runs`; `four_runs_of_one_state_never_overlap_and_all_run`; `cron_entry_runs_through_the_watch_checks` (`--entry 2`); `entry_outside_the_watch_list_exits_1`; `entry_flag_rejects_non_digits_and_over_3`; `state_not_allowed_for_public_repo_and_entry_without_state_true`; `ref_gate_refuses_state_vault_and_artifacts_off_main`; `missing_store_settings_end_before_a_ticket`; `restore_failure_starts_no_container_and_releases_the_ticket`; `validated_vault_declaration_ends_vault_unavailable_in_0_4_0`; `vault_item_not_allowed_never_reads`; `failing_step_uploads_one_artifact_and_loki_names_it`; `passing_run_with_when_failure_uploads_nothing`; `scan_refusal_fails_a_passing_run_and_still_saves_state`; `artifact_upload_failure_keeps_the_exit_code`; `hostile_fixture_leaks_nothing` (canaries for all thirteen runner keys, state strings; stdout, stderr, every child argv and variable set, every Loki body, every asset); `stderr_lines_are_all_in_faults_and_the_nine_are_members`; `stdout_adds_only_waiting_for_the_queue`; `no_state_pipeline_registers_nothing`; `existing_pipelines_run_unchanged`; `one_bws_call_per_run`; `every_loki_string_of_the_contract_is_emitted` | all stories, FR-029 to FR-031, FR-048, FR-051 to FR-060 |
| T010 | `.github/scripts/test-factory-monorepo-ci.sh` | `at_most_3_cron_lines`; `every_cron_has_a_case_branch_and_back`; `entry_numbers_1_to_3_unique`; `unknown_schedule_fails`; `values_are_plain_words`; `one_docker_run`; `timeout_210`; `no_concurrency_block`; `passes_run_attempt_and_runner_image`; `no_dollar_brace_in_run_blocks`; `only_bws_token_and_github_token_as_secrets`; `main_guard`; `no_due_job_and_no_schedule_file`; `tick_workflow_is_gone_and_unreferenced` (grep scope `.github/**`, both READMEs and `rust-ci-runner/**`; specs excluded); `readme_names_no_tick_only_setting` | FR-001 to FR-004, FR-039, FR-060 |

### 0.4.1

| ID | File | Tests | FR |
|---|---|---|---|
| T032 | `vault_tests.rs` (fake `bw`) | `sequence_is_login_unlock_list_logout`; `each_call_gets_only_its_own_variables`; `no_item_or_field_name_in_argv`; `list_runs_with_org_and_collection_ids_and_no_interaction`; `reads_each_declared_field_once`; `item_matching_zero_or_two_logins_is_unavailable`; `item_outside_the_allowlist_is_unavailable`; `case_difference_does_not_match`; `missing_empty_nul_and_3_char_line_unavailable_4_passes`; `data_dir_is_0700_and_gone_before_return`; `failed_logout_or_removal_is_unavailable`; `retry_once_on_timeout_not_on_rejected_credential`; `bws_server_url_set_ends_unavailable`; `errors_and_logs_hold_positions_and_counts_only`; `unicode_space_and_equals_in_item_names_match_by_bytes` | FR-044 to FR-047, FR-056 |
| T032 | `ci_state_tests.rs` | `vault_values_reach_only_the_declaring_step`; `sibling_step_environment_holds_no_vault_value_or_state_file`; `step_dumping_env_and_files_sees_only_declared_vars_and_no_bw_data`; `vault_unavailable_starts_no_container`; `vault_values_join_the_log_before_any_line`; `vault_values_are_in_the_scan_set` | US6, SC-007 |

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| args | bad `--entry`, schedule string unknown | `parse_args`, workflow `case` | `bad arguments`, job fails | T009, T010 |
| state allowed | public repo, entry lacks `state = true` | identity `private`, flag | `state not allowed`, no ticket | T009 |
| ref gate | state, vault or artifacts off an allowed ref | `secrets_allowed_on` | watch-list line | T009 |
| runner keys | missing, duplicate, bad format, equal | `Held` accessors | `state store not configured`, before a ticket | T002 |
| store | not private, not reachable, huge or oddly named assets | identity call, name and size checks | `state store not configured` or `state restore failed` | T005 |
| store | rate limited | headers | calls pause (renewals do not), Loki `rate limited` | T005, T006 |
| queue | holder dies | stale rule at 600 s | next waiter removes the ticket | T006 |
| queue | forged or malformed label | label rule | stale, deleted | T006 |
| queue | invisible in-flight ticket with a lower id | not `uploaded` under 120 s counts live; renewal re-list | claim waits; `overlap` logged | T006 |
| queue | turns ahead use the budget | budget minus 45 minutes | `queue wait expired`, alert | T006 |
| queue | renewals fail for 600 s | renewal thread | cancel flag set, containers killed and stopped, then ticket deleted, `queue renewal failed`, exit 1 | T006 |
| restore | download, auth, decrypt, size | `open`, checks | `state restore failed`, no container, ticket released | T008 |
| state step | file removal fails | `after` hook | `runner fault`, no later step | T008 |
| state | declared path leaves the checkout, FIFO, plaintext holds a secret | canonical path, `O_NONBLOCK`, scan | no capture, `state save failed` | T008 |
| artifacts | secret found, unscannable file, `.html` member | `scan` | `artifact upload refused`, exit 1, no upload | T007 |
| artifacts | upload fails | `HttpReleases` | Loki line, exit code unchanged | T009 |
| save | upload, size or fence fails | `state::save` | `state save failed`, new asset deleted, old state kept | T008 |
| exit | SIGINT, SIGTERM, panic | watcher, drop guard | ticket deleted first within 5 s, exit 1 | T006 |
| exit | ticket delete fails | next waiter | stale rule frees it in 10 minutes | T006 |
| vault (0.4.1) | `bw` hangs, rejected, 0 or 2 matches, outside item, server URL set | `vault::read` | `vault unavailable`, no container, dir removed | T032 |
| image (0.4.1) | `bw` does not run on Node 24 | `RUN bw --version` | build fails; no release | T034 |
| rollout | pin before the digest exists | `docker pull` fails | run fails; the old pin works until then | order of T024, T025 |
| rollout | pin reverted before watch keys removed | 0.3.0 refuses unknown keys | every run fails | quickstart 4 |
| schedule | 60 days without activity | GitHub | runs stop | missing-`pipeline passed` alert, README |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. Two runs of one state overlap. Causes: a lagging list hides an older ticket, a slow upload, a stale break of a live holder. Prevented: in-flight rule (120 s), settle re-list, overlap log at every renewal, 600 s stale rule on server time, the fence before and after the save, race tests with a lag switch, and the 200-trial canary with two racing runners. O12 stays open.
2. A secret lands in an artifact, a log or the store. Prevented: the scan reads every zip member (`trace.network` included) against all `secret_forms` plus the state strings, refuses unreadable formats and `.html` members, the state never enters a bundle, two keys with domain separation, the real-trace canary, the hostile fixture over every sink.
3. The state file reaches a step that must not see it, or a runner file leaves through a symlink. Prevented: one declaring step with an ordering rule, a hook that removes the file, canonical path checks with `O_NOFOLLOW|O_NONBLOCK`, a pre-seal scan, a `/proc/self` test.
4. The release breaks: the hand-edited lock disagrees, the workflow meets an old image, or the cron runs against 0.3.0. Prevented: T001 alone on a branch under `--locked`, the pin (T025) before the workflow (T023), the shell test, the rollback order.
5. Runs wait forever or never start: a reviewer on the Environment holds scheduled runs, tickets starve, or a disabled schedule goes unseen. Prevented: the Environment decision (D18), `queue wait expired` with an alert, renewals that never pause, the absent-`pipeline passed` rule with no-data Alerting, the day-55 note.

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

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: handled. Empty and maximum: 0 and 3 cron lines, 64-char names, 20 paths, 20 vault fields, 10 MB and 100 MB bounds, n and n+1 tests (T004, T007, T008, T010). Duplicates: duplicate cron entry number, same item twice, same run id twice. Unicode, NUL, newline: path and name rules, item names by bytes (T004, T032). Malformed: tampered or truncated asset (T003, T008); a `bw` list that is not JSON is `vault unavailable`.
- State: handled. First run (`first_run_has_no_state_and_no_file`). Older format: none; the magic leaves room for a version. Crash mid-operation: stale rule, two assets after a crash, the newest wins, a half upload is not `uploaded` and is deleted after 120 s. Partial failure: the table above. Retry: a rerun replaces its ticket, asset names are unique, deletes are idempotent (404 is gone).
- Time and concurrency: handled. Races in T006. Timeouts on every GitHub and `bw` call. Clock: age from the server `Date`; cron in UTC. Boundaries: 119 and 120 s, 599 and 600 s, label 120 and 121 s ahead. A duplicated tick queues and runs twice. Late and out-of-order events: stable order by asset id.
- Resources: handled with limits. Disk: bundles and ciphertext under `--work`, not the 64 MB `/tmp`. Large input: chunked crypto, streamed scan, caps of 10 MB, 100 MB, 500 MB, 10,000 entries. Slow or garbage dependency: one retry, a bounded pause, then a fixed fault. Growth: 2 state assets, 10 bundles, at most 300 assets listed; the queue itself has no cap, so tickets grow only with waiting runs and each is deleted at its run's end. Memory: 64 KiB chunks, a 64 KiB scan buffer, nested zips at most 64 MB, state at most 10 MB.
- Permissions and trust: handled. The state token reaches `releases.rs` only (scoped to the store repo); keys reach `seal.rs`; vault credentials reach `vault.rs` (0.4.1). The pipeline author cannot name a release, a repo, a setting or a credential, cannot widen the entry or the cron (reviewed files on `main`), and cannot see the state file outside its one step. The agent holds the artifact key, a Loki token and a read token for the store repo, nothing else.
- UI: N/A. No UI; the surface is the fixed stdout and stderr lines.

## Rollout and rollback

Rollout:

1. T001 lands alone on a branch (proves the lock). T002 to T022 land on `main` through a pull request linked to this folder; the selftest and `make lint` pass; the `rust-ci-runner` pipeline passes on `main`.
2. T024: tag `rust-ci-runner-v0.4.0`; read the digest and the sha256 of the release binary.
3. T025: pin on a branch (the selftest runs there), then fast-forward `main`.
4. T023 (workflow) lands after the pin. T026: no-secret canary and a manual dispatch.
5. T027 and T028 (alerts, script). T029: owner actions and the release canary.
6. T030: watch entry and first cron line. T031: the factory-monorepo pipeline.
7. Phase 5: 0.4.1 (vault) with its own tag, pin, canary and owner actions, then T038 docs.

Rollback, in this order:

1. Remove `state`, `vault_items`, the added `secret_branches` and the cron lines with their `case` branches, in the same commit as the pin revert or before it (a 0.3.0 runner refuses unknown watch keys).
2. Revert T023 (the workflow returns to the manual trigger and the concurrency block), then revert the pin. The tick workflow stays deleted.
3. Queue, state and artifact releases stay in the store repo; they are encrypted or empty.
4. The release workflow moved `:latest`; moving it back needs owner approval (`docker buildx imagetools create --tag ghcr.io/sudtanj/rust-ci-runner:latest ghcr.io/sudtanj/rust-ci-runner@<previous digest>`). Nothing here pulls `latest`.
5. Fixes ship as the next patch version; never move or delete a tag.
6. Rotate the nine new settings if a leak is suspected and revoke the state token.

## Owner actions

Done by the owner, never by an agent. No value is written in any file.

1. Create the store repo: private, no code, no workflows, no tag rulesets that block creation, immutable releases off, branch protection on its default branch (administrators included), and one initial README commit.
2. Create a fine-grained token scoped to the store repo only (contents write); put its expiry on a calendar.
3. Create Bitwarden entries for the nine new settings: `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`, and (before 0.4.1) `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET`, `FACTORY_CI_VAULT_PASSWORD`, `FACTORY_CI_VAULT_ORGID`, `FACTORY_CI_VAULT_COLLECTIONID`. The two keys come from `openssl rand -base64 33`, different values.
4. Before 0.4.1: the dedicated Bitwarden account in an organization that sees one collection; login items there with opaque names (`ci-login-1`).
5. The debug agent gets only the artifact key, a Loki read token limited to this tenant and step `ci`, and a GitHub read token scoped to the store repo. No write tools; egress to GitHub and Loki only.
6. In `sudtanj/factory-monorepo`: branch protection on `main` (reviews, no direct pushes, administrators included); a non-secret fixture for failure drills; the 002 pipeline keeps the Playwright HTML reporter off.
7. Import `rust-ci-runner/alerts/factory-ci.yaml` into Grafana and attach a contact point.
8. Calendar items: state token expiry, a review of each Bitwarden CLI release, a commit before day 55 of repository inactivity.
9. Delete the settings and Environments only the retired tick used (listed in the T022 pull request).
10. Run the release canary (quickstart section 2), including the real Playwright failure trace, before the first real run.

### Owner decision: the Environment (D18)

The README tells the owner to require a reviewer on `factory-ci`. A scheduled run would wait for approval. Options: (a) drop the reviewer; (b) put the scheduled job in a second Environment holding the same token. **Recommended: (b), `factory-ci-schedule`, limited to `main`, no reviewer.** The factory-ci Environment must not have branch rules that exclude scheduled runs. Until the owner decides, T023 uses the `environment` expression of D18 and T030 does not start.

## Devops assessment

Findings from the lead's devops review of 11d2ef0 and the critic review of faa835f, with their handling after the critic decisions.

| Finding | Handling | Where |
|---|---|---|
| Rollback order breaks every run | Removal first or in one commit with the pin revert; quickstart 4 | D17, Rollout |
| Two runs can hold the turn (in-flight tickets) | Not-`uploaded` under 120 s is live; overlap log; 200-trial canary with racing runners | D1, T006, T029 |
| No monitoring | Loki contract, a test per string, alert rules, day-55 note, runbook | D10, T027 |
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
| Tick still deployed | Deleted in 011 (T022) | D17 |
| Reviewer on `factory-ci` holds scheduled runs | Owner decision with a recommendation | D18 |
| Staged scope | 0.4.0 without Node; 0.4.1 adds the vault | Summary |

## Security assessment

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
- O15: cron lines are public in the workflow file along with their repo, branch and project; those are already public in the watch list and the dispatch defaults.

## Project Structure

```text
specs/011-ci-state-queue/  spec.md plan.md research.md data-model.md quickstart.md tasks.md
  contracts/ config-files.md release-store.md asset-format.md loki-lines.md
  checklists/requirements.md
rust-ci-runner/  Dockerfile Cargo.toml Cargo.lock .factory-ci.toml
  src/  seal releases queue signal state bundle scan stateful (new), vault (0.4.1), store watch pipeline redact log engine ci main util lib fake (changed), one *_tests.rs per new module
  alerts/factory-ci.yaml (new)  scripts/open-artifact.sh (new)  tests/no_vendor.rs
.github/  workflows/factory-monorepo-ci.yml (changed), factory-ci-tick.yml (deleted), factory-ci-selftest.yml (changed), rust-ci-runner-release.yml (smoke)
          scripts/test-factory-monorepo-ci.sh  factory-ci-repos.toml
```

**Structure Decision**: the existing layout; new logic in single-purpose modules, no new crate, no new directory but `alerts/` and `scripts/` under the runner.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| `miniz_oxide` (+ `adler2`), two new lock entries | FR-049 scans zip entries (`trace.zip`); inflate is what std lacks | hand-written inflate is security code to maintain; shelling out to `unzip` adds a binary and text parsing of hostile names |
| `libc` direct dependency (already locked) | FR-009 needs SIGINT and SIGTERM handling | a stale rule alone leaves a cancelled run's ticket for 10 minutes |
| `ring`, `memchr` direct dependencies (already locked) | AEAD, HKDF, fast substring search | new crates for the same primitives |
| Node and the `bw` bundle in the image (0.4.1, about +86 MB) | the owner chose Bitwarden Password Manager for vault logins; only the CLI reads it | the standalone binary is 143 MB, glibc-linked; a direct API client reimplements vault crypto |
| A hand-written tar writer and reader | one archive that the scanner and the decryptor both read | a `tar` crate adds a dependency for about 80 lines |
| A separate store repo, an alert-rule file and an agent script | a code-free repo limits the token; alerts catch a silent stop; the script pins what the agent runs | a shared repo exposes code to a content-write token; no alerts hides a disabled schedule; a bare command lets a swapped binary steal the key |
| Retiring the tick (T022) | Principle II and the schedule; the tick held `BWS_ACCESS_TOKEN` in a second workflow | keeping two trigger paths with two Environments doubles the review surface |
