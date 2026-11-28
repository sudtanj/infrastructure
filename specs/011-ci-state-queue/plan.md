# Implementation Plan: A strict queue, saved state and debug artifacts for the ci command

**Branch**: `011-ci-state-queue` | **Date**: 2026-11-28 (amended the same day by the owner rule "Amend 011": no workflow YAML and no shell script changes) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/011-ci-state-queue/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before this plan and `/speckit-analyze` after `tasks.md`; the owner rule is recorded as a dated clarification in spec.md and `/speckit-analyze` ran on this revision. The constitution stays at 1.2.0; 011 does not use the schedule allowance it grants.

## Summary

011 builds on 010 (step secrets). It ships in one runner release, 0.4.0, and every part is Rust, `Cargo` files or toml.

**0.4.0**: a strict FIFO queue per state (release assets in a separate private store repo, one ticket per run, no cap), encrypted state restored into one named step only, encrypted debug artifacts behind a secret scan, all 13 runner key names reserved, and the vault declarations parsed and refused with `vault unavailable`. The runner finds its own attempt number and image by asking docker about its own container (D19), so the unchanged workflow does not have to pass them.

**Owner rule of 2026-11-28 (binding)**: no file under `.github/workflows/` or `.github/scripts/` changes. The tick workflow stays and keeps its outside dispatcher (feature 012 relies on it). The schedule, the workflow edits, the pin bump, the vault read and `bw` image, the alert-rule file and the agent script are out of scope (table below).

Everything else (watch list, Loki, fixed public grammar, redactor, engine) stays as in 008 and 010. Interim website logins use 010 step secrets.

## Technical Context

**Language/Version**: Rust 2021, `rust:1.90` in the pipeline, musl static binary in the release.

**Primary Dependencies**: `ring` 0.17.14, `memchr` 2.8.3 and `libc` 0.2.191 (all already locked; direct dependencies add one line each to the root package), and two new lock entries, `miniz_oxide` 0.8.9 and `adler2` 2.0.1. The image gains nothing (no Node, no `bw`).

**Storage**: GitHub releases of a separate private store repo that holds no code. Local files under `--work`. No database.

**Testing**: `cargo test --locked --lib` in the `rust-ci-runner` pipeline with new mock-server groups in their own processes; the unchanged `factory-ci-selftest.yml` compiles and builds the image on the branch. Queue, state and bundle logic runs against an in-memory `Releases` fake.

**Target Platform**: `ghcr.io/sudtanj/rust-ci-runner`, linux/amd64 and arm64, started by the unchanged `factory-monorepo-ci.yml` on a GitHub-hosted VM (45-minute job, concurrency group kept, image pin kept).

**Project Type**: CLI, one binary.

**Performance Goals**: poll 20 s, renew 60 s, settle 3 s. State at most 10 MB, bundle at most 100 MB, memory bounded by a 64 KiB chunk plus the scan window.

**Constraints**: the dev VM has 1 GB and no cargo, docker or builds. The runner container has a read-only root and a 64 MB `/tmp` tmpfs (`factory-monorepo-ci.yml`), so bundles and ciphertext go under `--work`. A step container allows 512 processes (`engine.rs`). The job limit is 45 minutes, and the workflow passes `-e GITHUB_RUN_ID` and not `GITHUB_RUN_ATTEMPT` or the image name.

**Scale/Scope**: 2 state assets, 10 bundles per project, 50 vault declarations per pipeline (checked, refused), no ticket cap, a 5-minute wait.

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

Rule: no file under `.github/workflows/` and none under `.github/scripts/` may change. Only Rust in `rust-ci-runner/`, `.factory-ci.toml` files and other toml files change. The owner chose "Amend 011" and kept the tick (see the clarification in spec.md).

| Earlier 011 item | Now | Reason |
|---|---|---|
| Cron lines, `case`, entry numbers 1 to 3, `--entry` (D5, T010, T023, T030) | Out of scope. Entry is the constant `0` in names. | needs `factory-monorepo-ci.yml` and a shell test |
| Workflow: no `concurrency`, 210 minutes, `-e` flags, Environment (D18, T023) | Out of scope. The group, 45 minutes and the pin stay. Numbers in D16 follow the 45-minute limit. | workflow edit |
| Pin to 0.4.0 (T025), canaries and first real run (T026, T029, T031) | Out of scope. The image is released and not pinned. | the pin is a workflow line |
| Watch keys on `factory-monorepo` (T030) | Out of scope. | the pinned `ci` runner and the tick's `plan` refuse unknown keys |
| Retire the tick (T022, D17) | Reversed: the tick stays. The tick path ignores `[state]` and `[artifacts]` (FR-062). | owner decision, feature 012 |
| Vault read, `bw` image, 0.4.1 (D3, T032 to T037) | Out of scope. Declarations are checked and refused. | `Dockerfile` and release workflow edit |
| Alert-rule file (D10, T027) | Out of scope as a file. The owner builds the rules in Grafana from `contracts/loki-lines.md`. | YAML file |
| Agent script (D15, T028) | Out of scope. `decrypt-artifact` and one README command remain. | shell script |
| Attempt number and runner image from the workflow | Read from docker by the runner (D19). | workflow cannot pass them |

Kept in full: D1, D2, D4, D6, D8, D9, D11 to D14, the 13 key names, the 9 fault lines, the Loki lines, the scan, the store hygiene.

## Architecture decisions

Evidence for each is in [research.md](research.md). The numbering D1 to D18 is stable for the briefs.

### D1 Queue: release assets in a store repo, unlimited, strict order

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

### D3 `bw` (out of scope)

The vault read needs Node and the `bw` bundle in `rust-ci-runner/Dockerfile` and a smoke step in `rust-ci-runner-release.yml`. The owner rule allows neither, so D3 is not built. The evidence stays in research.md (R-C) for the day the rule is lifted. 0.4.0 ends a validated vault declaration with `[runner] vault unavailable` before any ticket or container.

### D4 One list of runner keys, one `bws` call

`store::RUNNER_KEYS: [&str; 13]` replaces 010's `REQUIRED_KEYS`; `REQUIRED_KEYS` becomes its first four (read at start). The others are held from the same single `bws secret list` call: `fetch_with` keeps their rows next to the allowlisted step rows in `Held`, and `Held::take`-style accessors return them on need. Consumers: `valid_secret_name`, `watch::parse`, `Held::take` (refuses runner keys by name), the runner-key accessor, and the `log.rs` fault names. A table test walks the list; membership asserts, never counts. New data goes into `Args`, not new parameters.

### D5 Schedule (out of scope)

No cron lines, no `case`, no `--entry`. The tick keeps its outside dispatcher. The `<entry>` field of ticket and asset names is always `0`.

### D6 Where the tests run

No-server modules (`seal_tests`, `scan_tests`, `bundle_tests`, `state_tests`, `queue_tests` with at most 6 joined threads) run in the first command. `releases_http_tests` and `ci_state_tests` start servers and get their own process groups in `.factory-ci.toml`, with matching `--skip` flags (`--skip ci_tests::` does not match `ci_state_tests::`).

### D7 Order inside `run_checkout` (extends 010)

1. Load the pipeline: parse faults (state, artifacts, vault shape, reserved names, project and tag rules) are `pipeline file refused`.
2. Docker rules (010) plus: a vault list with `docker = true`, and any `docker = true` in a pipeline with `[state]`, are refused.
3. State or artifacts declared: the entry needs `state = true` and the pipeline repo must be private (identity call returns `private`), else `state not allowed`.
4. Allowlists: 010's `unlisted` (`step secret not allowed`), then vault items against `vault_items` (`vault item not allowed`); a validated vault declaration ends `vault unavailable` here, before any ticket.
5. Ref gate (`secrets_allowed_on`) for step secrets, vault, state and artifacts.
6. Runner keys from `Held`: state token, store repo, both keys (checked, different); any miss is `state store not configured`. The runner reads its own attempt and image (D19). `GET /repos/<store>` must answer `private: true`. Before step 6 reads anything and before any store call or any line that can carry a value (the lines of steps 1 to 5 are fixed text), `log.add_secrets` receives the state token, the state and artifact keys, the store repo name and `base64("x-access-token:" + token)` (FR-061). A 5xx on registration is retried inside the wait budget.
7. 010's `Held::take` for step secrets, then `log.add_secrets`.
8. `[state]`: register and wait (`queue wait expired`). Inside the turn: restore into memory (`state restore failed`), add its strings to the log.
9. `run_pipeline` with the step hook of D8 and the deadline of D16.
10. After: chown the artifact paths (runner image helper), build, scan, encrypt, upload when due, save the captured state when the pipeline passed (the `after` hook of D8 has already run `state_strings` on the bytes it read and `add_secrets`, so a cookie issued during the run is masked and scanned before any artifact is built), release the ticket, remove the checkout.

### D8 State reaches one step only

A step key `state = true` marks the one step. `[state]` needs exactly one such step, and every other step must be a transitive dependency or dependent of it, so none runs beside it. `RunCtx` gains a `StepHook` (`before`, `after`) called on the step's thread around `docker run`, before the step counts as finished. `before` writes the in-memory plaintext at the declared path (mode 0600, canonical path inside the checkout, `O_NOFOLLOW`; nothing for a first run). `after`, pass or fail, runs the chown helper, reads the file by the rules of D11, keeps the bytes in memory, and removes the file; a removal failure is `runner fault`. The declaring step should end with `chown` to the workspace owner; the helper is the backstop (`chown -hR` and `chmod u+rX`, never `a+r`) and runs the runner's own pinned image (found by D19), not the step image. The state step runs as root like every step; the README says so and tells the owner not to place a state step after an install step that runs package scripts. Rejected: a bind mount of a runner-private file (an empty file where FR-021 promises none; `EBUSY` on rename), a pipeline-wide restore (the 002 finding), a per-step workspace copy.

### D9 Store repo

A separate private repo that holds no code and no workflows, one initial README commit, no tag rulesets that block creation, immutable releases off. Its `owner/name` is `FACTORY_CI_STATE_REPO` (Bitwarden). `FACTORY_CI_STATE_TOKEN` is scoped to it only; the agent's read token to it plus Loki. Tags, names and regexes are in the release-store contract; the `+` delimiter and the `-` root project make the tag mapping injective, with a collision test.

### D10 Monitoring

[contracts/loki-lines.md](contracts/loki-lines.md) fixes every Loki line, each with a test. The alert-rule file is out of scope (YAML). The owner builds these Grafana rules by hand, no-data set to Alerting: a missing `pipeline passed` for `sudtanj/factory-monorepo`; any fault line over 1 h; `queue wait expired`; `queue renewal failed`; any `overlap`; more than one `restore none` in 24 h; more than two `restore failed` in 24 h; any `stale break`; `rate limited` more than 3 times in 1 h. The README carries a runbook for `state restore failed` (delete the newest state asset, or all of them to start empty).

### D11 Filesystem trust

The state hook and `bundle::build` canonicalise the declared path, require `starts_with(canonical checkout)`, `lstat`, open with `O_NOFOLLOW|O_NONBLOCK`, and judge size by bytes read. Before `seal`, the runner scans the state plaintext for every known secret form; a hit is `state save failed`. A test points the state path through `/proc/self` and a FIFO.

### D12 Names and maps

A name in both a `secrets` list and a vault `env` is refused pipeline-wide. A vault value map would be separate from step secrets (out of scope with the vault read). Entries with `state` or `vault_items` need an explicit `secret_branches = ["main"]` (`plan --check` refuses otherwise). Vault item names in the public watch file are opaque (`ci-login-1`).

### D13 Scan scope

The scan set is `Log::secret_forms()` minus the numeric `FACTORY_CI_LOG_USER` (an identifier that matches digits by chance) (010's `Redactor::forms`: the value, three base64 alignments, URL-safe base64, percent-encoding, JSON escape, hex, trimmed lines) plus the FR-050 state strings. The scan reads every zip member, `trace.network` included. Members are allowed only as text, JSON, zip (stored or deflate), PNG or JPEG, judged by content; anything else, and any `.html` member, is refused (the Playwright HTML reporter stays off; 002 uses its own reporter). Tar names with `..`, a leading `/` or a control character are refused. A canary on one real Playwright login-failure trace from the 002 fixture runs before the first real run.

### D14 Store hygiene

Fixed hosts, redirects off, download `Location` only https on `*.githubusercontent.com`, name and size checks before a download, pagination with a 300-asset fail-closed bound, one retry of a restore on a 404.

### D15 Agent kit (script out of scope)

No `open-artifact.sh`. The agent kit is the artifact key, a Loki read token and a read-only GitHub token. The README gives the download (`gh release download`) and `rust-ci-runner decrypt-artifact` commands, says the runner binary comes from the pinned image, and tells the reader to treat the decrypted content as untrusted data (strip control characters, extract with `tar -x --no-same-owner` into a new empty directory).

### D16 Deadline

The workflow's `timeout-minutes` is 45 and stays. `JOB_LIMIT` is a Rust constant of 45 minutes next to the budget code in `ci.rs`; a test reads `.github/workflows/factory-monorepo-ci.yml` (read only, through `CARGO_MANIFEST_DIR`) and asserts the `timeout-minutes` of the `ci` job equals it. A later workflow edit then turns the test red.

`budget_end` is read through `Seams.timing` (tests set it; production leaves the variable unset): the value of `FACTORY_CI_BUDGET_END` if set, else runner start plus 35 minutes (the limit minus 10 for start-up and cleanup). Pipeline deadline `= min(now + 30 min, budget_end - 15 min)`; the 15 minutes are 5 for the artifact scan and upload, 5 for the save and 5 for cleanup. The wait ends at `budget_end - 30 min` (5 restore, a 10-minute minimum pipeline window, the 15-minute reserve), which is 5 minutes after the start. A hold is at most 40 minutes. The README documents the 5-minute wait and the minimum pipeline window of 10 minutes.

### D17 Rollback and the tick

Rollback reverts the Rust commits. No workflow, pin or watch file changed, so no other step exists. The tick workflow `factory-ci-tick.yml`, its scripts and the `plan` subcommand are not touched (FR-060). The tick path (`job`, `local`) runs a pipeline that declares `[state]` or `[artifacts]` as before 011, with no store call (FR-062).

### D18 Environment (out of scope)

No `environment:` expression and no second Environment: nothing in the workflow changes and no scheduled run exists.

### D19 The runner finds its own attempt and image

The unchanged workflow starts the container as `--name factory-ci-runner-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT`, passes `-e GITHUB_RUN_ID` and mounts `/var/run/docker.sock`. `ci.rs` resolves two values once, before any store call, with one `docker inspect --format '{{.Name}} {{.Config.Image}}' <hostname>` through the engine seam:

- attempt: the digits after the last `-` of the container name when the name is `/factory-ci-runner-<digits>-<digits>`; else `GITHUB_RUN_ATTEMPT` when it is all digits; else `1`;
- image: `FACTORY_CI_RUNNER_IMAGE` when set; else `.Config.Image`. An empty image, or a failed `inspect`, ends a run that declares `[state]` or `[artifacts]` with `[runner] runner fault`, before any ticket, because the chown helper needs the image. A run with neither declaration never calls `inspect`.

The hostname is the container id under the workflow's `docker run` (no `--hostname`). Tests set both environment variables, and a fake engine answers `inspect`. Rejected: reading `/proc/self/cgroup` (version-dependent), guessing the image from a constant (it moves with every pin), a second `docker run` flag (workflow edit).

## Files

### Added (`rust-ci-runner/src` unless noted)

`seal.rs`, `releases.rs` (the only code that calls api.github.com with the state token), `queue.rs`, `signal.rs` (the one `unsafe`: a `libc::sigaction` call that stores an atomic), `state.rs`, `bundle.rs`, `scan.rs`, `stateful.rs` (post-checkout phase), one `*_tests.rs` per module, `ci_state_tests.rs`, `releases_http_tests.rs`, and the spec artifacts. No `vault.rs`, no `alerts/`, no `scripts/`.

### Changed

| File | Change |
|---|---|
| `store.rs` | `RUNNER_KEYS` (13) replaces `REQUIRED_KEYS`, which becomes its first four. `fetch_with` also keeps the rows of the nine other runner keys in `Held`; accessors return them on need with the 010 value rule; key format and equality checks. |
| `watch.rs` | Keys `state`, `vault_items`; both need an explicit `secret_branches = ["main"]`; runner-key names in `secrets` refuse the file. |
| `pipeline.rs` | `[state]`, `[artifacts]`, `Step.state`, `Step.vault`, the rules of data-model.md, `valid_secret_name` uses `RUNNER_KEYS` and refuses the four prefixes in `env`, `secrets` and vault `env`. |
| `redact.rs`, `log.rs` | Nothing new in the redactor (010's `forms`, `secret_forms`, `longest_secret` serve the scan); `FAULTS` gains nine lines and `public_form` the queue line; membership asserts. |
| `engine.rs`, `job.rs` | `StepHook` in `RunCtx`; `Engine::kill_run(prefix)`; `RunExtras` also carries the cancel flag; `job.rs` `run_pipeline` takes `RunExtras` (deadline and hook); `Engine::own_paths` (runner image, `chown -hR`, `chmod u+rX`); `Engine::inspect_self` (D19); `job` and `local` ignore `[state]` and `[artifacts]` and refuse a vault declaration (FR-062, FR-045). |
| `ci.rs` | `Args` gains the store settings and the budget (no `entry`); `Seams` gains `uploads_base`, `hub`, `timing`, `exit`, `inspect` (D19); `JOB_LIMIT` (D16); identity returns `private`; D7 order. New data goes into `Args`, not new parameters. |
| `main.rs` | `decrypt-artifact`; `run_ci` installs the signal handler; usage text. |
| `util.rs` | `parse_http_date`. |
| `.factory-ci.toml` | Two process groups and the `--skip` flags. |
| `Cargo.toml`, `Cargo.lock` | 0.4.0; `ring`, `memchr`, `libc`, `miniz_oxide`; two new lock entries by hand. |
| `README.md`, `rust-ci-runner/README.md` | Docs last. |

### Not changed

`ship.rs` keeps its interface. No file under `.github/` changes, and neither does `rust-ci-runner/Dockerfile` (owner rule). `job.rs` changes minimally: `run_pipeline` builds `RunCtx` itself with a fixed 30-minute deadline, so it gains one `RunExtras { deadline: Instant, hook: Option<&dyn StepHook> }` argument (it already carries `#[allow(clippy::too_many_arguments)]`); `local` and `job` pass the old 30-minute deadline and no hook.

## Data and API contracts

[contracts/config-files.md](contracts/config-files.md), [contracts/release-store.md](contracts/release-store.md), [contracts/asset-format.md](contracts/asset-format.md), [contracts/loki-lines.md](contracts/loki-lines.md), [data-model.md](data-model.md).

## Engineer briefs

Precondition: 010 is merged to `main` and released (0.3.0). Names used here are 010's real ones (`wt-infra-010`, `specs/010-ci-step-secrets/tasks.md`): `REQUIRED_KEYS`, `fetch_with`, `Fetched`, `Held::take`, `StepSecrets`, `NamedError { Count { index, found }, Unusable { index } }`, `Redactor::add`, `Redactor::forms`, `Log::add_secrets`, `Log::secret_forms`, `Log::longest_secret`, `WatchEntry::secrets_allowed_on`, `Args`. If a name differs after 010 lands, follow 010. Do not assume a `FAULTS` length.

Order. T001 first, alone (skeleton, dependencies, lock, and the `Releases` trait with its types). T002 to T004 and T009 (tests first) run in parallel after T001; T005 (the fake and the HTTP tests) runs after T001, and T006, T007, T008 run after T005 because they use `FakeReleases`. T011 to T021 (code) run in parallel after T001 and their own tests, except: T014 before T015, T016, T017; T018 before T015 and T017 (the cancel flag, the kill seam and the hook); T013 before T017 and T020; T015 before T020 (the signal install); T019 after T011 to T018; T021 after T019 and T020. T024 (gate and tag) follows the checkpoint. T038 (docs) is last. T010, T022, T023 and T025 to T037 are dropped by the owner rule (table below).

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
- Also: `Engine::inspect_self` runs `docker inspect --format '{{.Name}} {{.Config.Image}}'` on the hostname (D19) and returns the name and image; `job` and `local` ignore `[state]`, `[artifacts]` and `state = true`, and end a validated vault declaration with `vault unavailable` (FR-062, FR-045).
- Constraints: `log.rs` and `engine.rs` already may name the vendor; nothing else changes.

### T019 Wire `ci` (FR-005, FR-006 to FR-009, FR-019 to FR-027, FR-045, FR-049 to FR-055, FR-059, FR-061)
- Goal: `ci` runs D7, resolves its attempt and image (D19), and the job-limit constant holds.
- Files: `ci.rs`, `stateful.rs`.
- Acceptance: the order of D7, each fault with its exact line, exit code 1, no container on a refusal. `ci` has no `--entry` flag; names carry the entry `0`. `identity` returns `private`. The deadline of D16 with `JOB_LIMIT` 45 minutes. D19: attempt and image from `inspect_self`, environment overrides win, `runner fault` before any ticket when the image is missing in a stateful run, no `inspect` call in a stateless run. `stateful.rs` runs restore, the pipeline with the hook, `own_paths`, artifacts, save, release. A scan refusal makes exit 1 and does not stop the state save. Artifacts run when the pipeline did not pass and a step started, or `when = "always"`. A validated vault declaration ends `vault unavailable` before a ticket and a container.
- Also: before any line that can carry a value (steps 1 to 5 print fixed text) and before any store call, call `log.add_secrets` with the state token, the state and artifact keys, the store repo name and `base64("x-access-token:" + token)` (FR-061); read the budget end through `Seams.timing`, not `std::env`; pass the cancel flag to the renewal thread. A test reads `.github/workflows/factory-monorepo-ci.yml` through `CARGO_MANIFEST_DIR` (read only) and asserts its `timeout-minutes` equals `JOB_LIMIT`; if the pipeline checkout does not hold that file, report it and keep the README line as the only guard.
- Constraints: state token and keys reach only `releases.rs` and `seal.rs`. New data goes into `Args`. No `set_var`. No file under `.github/` is written.

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

### T024 Gate and tag (FR-038)
- Goal: a published 0.4.0 image and digest.
- Files: none.
- Acceptance: T001 to T021 are on `main`, `make lint` and the unchanged selftest pass, the `rust-ci-runner` pipeline passed on that commit (run id and SHA equal), the existing release workflow finished with its smoke step, the digest is verified by `docker pull`, and the digest is recorded in the README release checklist line.
- Constraints: the owner or lead pushes the tag; never move a tag. Nothing pins the digest (owner rule).

### Dropped by the owner rule

IDs stay reserved so references hold. None of these is built.

| ID | Was | Why dropped |
|---|---|---|
| T010 | shell-test updates for cron lines | script edit |
| T022 | retire the tick | reversed, the tick stays |
| T023 | rewrite `factory-monorepo-ci.yml` | workflow edit |
| T025 | pin 0.4.0 in the workflow | workflow edit |
| T026 | no-secret canary through the new pin | needs the pin |
| T027 | alert-rule YAML | YAML file, owner builds rules in Grafana |
| T028 | `open-artifact.sh` | shell script |
| T029 | owner actions and release canary on the real store | needs the pin to run a real `ci`; the owner actions stay listed below for the day the rule lifts |
| T030 | watch entry and first cron line | pinned runner and tick `plan` refuse the keys; cron is a workflow edit |
| T031 | first real run | needs the pin and T030 |
| T032 to T037 | vault tests, read, image, wiring, 0.4.1 release, owner vault actions | Dockerfile and workflow edit |

### T038 Docs (FR-041), last
- Goal: both READMEs and the repo description match.
- Files: `README.md`, `rust-ci-runner/README.md`, `gh repo edit`.
- Acceptance: they state what FR-041 lists, including the release checklist line (the image digest), the `state restore failed` runbook, the S5 wording (a state step should not follow an install step that runs package scripts), that the state step runs as root, that `queue wait expired` is the one runner-side drop and the wait is 5 minutes, that the concurrency group stays, that the tick stays, what 011 leaves out and why (schedule, vault read, pin, alert and agent files), the Grafana rules to build by hand, the `decrypt-artifact` command and the agent kit, and the accepted risks. No value appears.
- Also: the README says that traces from logged-in runs may be refused by the scan and that this is by design; that a burst of renewals at the secondary rate limit can trigger `queue renewal failed`, which an alert covers; that a late winner has a minimum pipeline window of 10 minutes; and that a step on reviewed `main` code could print a transformed secret (accepted).
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
| T006 | `queue_tests.rs` (fake, at most 6 joined threads) | `fifo_is_asset_id_order`; `there_is_no_ticket_cap` (6 waiters all run in order); `second_waits_and_prints_one_line_once`; `stale_at_600s_not_at_599s`; `stale_ticket_removed_by_the_next_waiter_with_a_loki_line`; `ticket_pattern_not_uploaded_younger_than_120s_is_live_and_120s_is_not`; `other_kind_not_uploaded_is_deleted_only_after_360s` (360 kept, 361 deleted); `two_failed_uploads_keep_the_last_good_state`; `forged_label_negative_age_overflow_missing_are_stale`; `label_120s_ahead_passes_121s_is_stale`; `junk_assets_are_ignored`; `late_lower_id_ticket_sends_the_claimer_back`; `slow_upload_with_a_lower_id_is_seen_at_renewal_and_logged_as_overlap`; `wait_expires_at_budget_minus_30_and_removes_the_ticket`; `budget_defaults_to_start_plus_35_minutes`; `deadline_is_the_smaller_of_30_minutes_and_budget_minus_15`; `matrix_siblings_with_one_run_id_do_not_replace_each_other`; `rerun_of_one_run_id_replaces_its_ticket`; `renewal_never_pauses_on_a_rate_limit`; `renewals_failing_for_600s_end_the_run_with_queue_renewal_failed`; `every_exit_path_removes_the_ticket`; `sigint_and_sigterm_delete_the_ticket_within_5s_and_skip_the_flush` (two tests); `own_ticket_gone_before_save_writes_nothing`; `own_ticket_gone_after_upload_deletes_the_new_asset`; `exit_delete_has_a_5s_limit_and_no_retry`; `server_clock_skew_does_not_matter`; `two_waiters_deleting_one_stale_ticket_is_fine` | FR-006 to FR-016, FR-052 to FR-054 |
| T006 (critic 2) | `queue_tests.rs` | `renewal_failure_kills_the_containers_then_deletes_the_ticket` (`FakeKiller` and `FakeReleases` share one event log: the kill and the wait for exit come before the ticket delete; a step still running blocks the delete); `cancel_flag_stops_the_step_loop`; `registration_5xx_is_retried_inside_the_wait_budget` | FR-014, FR-054 |
| T003 (critic 2) | `seal_tests.rs`, `ci_state_tests.rs` | `decrypt_artifact_reads_the_key_from_the_environment_only`; `decrypt_artifact_never_echoes_the_key`; `decrypt_artifact_error_prints_one_generic_stderr_line`; `decrypt_artifact_truncated_stream_exits_non_zero`; `run_ci_installs_the_signal_handler_first` | FR-033, FR-009 |
| T008 (critic 2) | `state_tests.rs` | `cookie_set_during_the_run_in_a_trace_refuses_the_artifact` (after hook adds the strings before bundling); `own_paths_argv_and_environment_hold_no_secret` (`--network none`, no `-e`, `--entrypoint chown`) | FR-050, FR-055 |
| T009 (critic 2) | `ci_state_tests.rs` | `canaries_include_the_base64_and_x_access_token_forms_of_the_state_token` (state token, both keys, store repo name; before the first log line and the first store call); `vault_declaration_refused_before_a_ticket`; `budget_end_comes_from_seams_timing`; `store_repo_shape_is_checked`; `loki_strings` as one list; `trace_after_login_with_restored_state_is_refused_unless_scrubbed` | FR-020, FR-061, FR-049 |
| T007 | `bundle_tests.rs`, `scan_tests.rs` | `folder_bundled_whole_missing_path_skipped`; `symlink_and_special_files_skipped`; `path_through_a_symlinked_parent_is_refused` (canonical `starts_with`); `fifo_at_the_path_does_not_block`; `size_is_by_bytes_read`; `tar_names_with_dotdot_absolute_or_control_are_refused`; `long_names_use_pax`; `bundle_cap_at_the_limit_passes_one_over_refuses` (cap injected); `retention_keeps_10_uploaded`; `upload_failure_leaves_the_exit_code`; `sentinel_in_plain_base64_and_one_line_of_multiline_is_refused` (every `secret_forms` form, in a plain member, in `trace.network` inside a zip, in a nested zip at depth 2); `state_strings_join_the_scan_set` (FR-050); `depth_3_zip_is_refused`; `html_member_is_refused`; `member_that_is_not_text_json_zip_png_jpeg_is_refused`; `zip_method_other_than_stored_or_deflate_is_refused`; `encrypted_entry_zip64_bad_crc_over_10000_entries_over_500mb_refused` (limits injected); `value_across_a_chunk_edge_is_found`; `4_char_found_3_char_ignored`; `refusal_reports_a_count_only`; `clean_bundle_passes` | FR-028 to FR-032, FR-049, FR-050, FR-055, FR-058 |
| T008 | `state_tests.rs` | `first_run_has_no_state_and_no_file`; `restore_picks_the_highest_id_uploaded_asset`; `manual_run_asset_restores`; `restore_fails_on_wrong_key_truncated_tampered_and_never_falls_back`; `restore_checks_name_and_size_before_download`; `save_fresh_nonce_keeps_the_two_newest_uploaded`; `crash_between_upload_and_delete_leaves_two_and_newest_wins`; `save_refuses_symlink_empty_over_10mb` (10 MB passes); `failed_pipeline_never_changes_the_store`; `save_failure_keeps_previous_state`; `store_holds_only_ciphertext`; `state_plaintext_holding_a_known_secret_is_not_saved`; `state_path_through_proc_self_captures_nothing`; `json_value_strings_16_chars_added_15_not`; `non_json_under_1mb_added_whole`; `state_file_exists_only_during_the_declaring_step` (absent before, 0600 inside, gone after); `no_other_step_sees_the_file`; `file_is_never_world_readable`; `removal_failure_stops_the_run`; `own_paths_runs_the_runner_image_with_chown_hr` | FR-021 to FR-027, FR-050, FR-055, SC-005 |
| T009 | `ci_state_tests.rs` (own group) | `counter_pipeline_sees_0_1_2_across_three_runs`; `four_runs_of_one_state_never_overlap_and_all_run`; `entry_flag_does_not_exist` (`--entry` is `bad arguments`); `repo_outside_the_watch_list_exits_1`; `state_not_allowed_for_public_repo_and_entry_without_state_true`; `ref_gate_refuses_state_vault_and_artifacts_off_main`; `missing_store_settings_end_before_a_ticket`; `restore_failure_starts_no_container_and_releases_the_ticket`; `validated_vault_declaration_ends_vault_unavailable`; `vault_item_not_allowed_never_reads`; `failing_step_uploads_one_artifact_and_loki_names_it`; `passing_run_with_when_failure_uploads_nothing`; `scan_refusal_fails_a_passing_run_and_still_saves_state`; `artifact_upload_failure_keeps_the_exit_code`; `hostile_fixture_leaks_nothing` (canaries for all thirteen runner keys, state strings; stdout, stderr, every child argv and variable set, every Loki body, every asset); `stderr_lines_are_all_in_faults_and_the_nine_are_members`; `stdout_adds_only_waiting_for_the_queue`; `no_state_pipeline_registers_nothing`; `existing_pipelines_run_unchanged`; `one_bws_call_per_run`; `every_loki_string_of_the_contract_is_emitted` | all stories, FR-029 to FR-031, FR-048, FR-051 to FR-060 |
| T009 (owner rule) | `ci_state_tests.rs`, `job_tests.rs` | `attempt_comes_from_the_container_name`; `attempt_falls_back_to_the_environment_then_1`; `image_comes_from_inspect_and_the_environment_wins`; `failed_inspect_or_empty_image_ends_runner_fault_before_a_ticket_in_a_stateful_run`; `stateless_run_never_calls_inspect`; `job_limit_equals_the_workflow_timeout` (reads the workflow read-only); `tick_path_runs_a_state_pipeline_without_store_calls_restore_save_or_bundle`; `tick_path_state_step_sees_no_file`; `tick_path_refuses_a_vault_declaration_with_vault_unavailable` | FR-005, FR-039, FR-045, FR-062, D19 |

### 0.4.1 (out of scope)

The vault read, the `bw` image and their tests (old T032 to T035) are not built. The only vault tests are in the 0.4.0 table above (declaration check, allowlist, refusal in `ci`, `job` and `local`).

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| args | `--entry` or any unknown flag | `parse_args` | `bad arguments` | T009 |
| state allowed | public repo, entry lacks `state = true` | identity `private`, flag | `state not allowed`, no ticket | T009 |
| ref gate | state, vault or artifacts off an allowed ref | `secrets_allowed_on` | watch-list line | T009 |
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
| vault | any validated vault declaration (`ci`, `job`, `local`) | pipeline check | `vault unavailable`, no ticket, no container | T009 |
| own container | `docker inspect` fails, name does not parse, image empty | `inspect_self` | attempt falls back; a stateful run ends `runner fault` before a ticket | T009 |
| rollout | someone adds `state` keys to the watch file before the pinned runner knows them | the pinned `ci` runner and the tick's `plan` refuse unknown keys | every run of that entry fails | not done in 011 (FR-040); R5 |
| job limit | someone edits `timeout-minutes` of the workflow | Rust test of the constant | red test, owner updates `JOB_LIMIT` | T009 |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. Two runs of one state overlap. Causes: a lagging list hides an older ticket, a slow upload, a stale break of a live holder. Prevented: in-flight rule (120 s), settle re-list, overlap log at every renewal, 600 s stale rule on server time, the fence before and after the save, race tests with a lag switch, and the 200-trial canary with two racing runners. O12 stays open.
2. A secret lands in an artifact, a log or the store. Prevented: the scan reads every zip member (`trace.network` included) against all `secret_forms` plus the state strings, refuses unreadable formats and `.html` members, the state never enters a bundle, two keys with domain separation, the real-trace canary, the hostile fixture over every sink.
3. The state file reaches a step that must not see it, or a runner file leaves through a symlink. Prevented: one declaring step with an ordering rule, a hook that removes the file, canonical path checks with `O_NOFOLLOW|O_NONBLOCK`, a pre-seal scan, a `/proc/self` test.
4. The release breaks: the hand-edited lock disagrees, or the new runner meets a workflow it does not expect (45-minute limit, no attempt variable, group kept). Prevented: T001 alone on a branch under `--locked`, D19 (own attempt and image), the job-limit constant and its test, the budget numbers of D16, and no pin or workflow change at all.
5. Runs wait too long or are dropped: a long holder makes a waiter expire, GitHub's group cancels a pending run, or a ticket starves. Prevented: a 5-minute wait ends in `queue wait expired` with an alert the owner builds in Grafana, renewals never pause, the group is documented (R4) and nothing pretends the queue recovers a cancelled run.

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
- I16: out of scope with the vault read.
- I17: one `bws` call per run.
- I18: `JOB_LIMIT` equals the `timeout-minutes` of `factory-monorepo-ci.yml`; the wait end, the deadline and the budget end derive from it only.
- I19: the tick path (`job`, `local`) makes no store call and writes no state or artifact, for any pipeline.
- I20: no file under `.github/` or `rust-ci-runner/Dockerfile` differs from `main` on the feature branch.

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: handled. Empty and maximum: 64-char names, 20 paths, 20 vault fields, 10 MB and 100 MB bounds, n and n+1 tests (T004, T007, T008). Duplicates: same item twice, same run id twice. Unicode, NUL, newline: path and name rules, item names by bytes (T004). Malformed: tampered or truncated asset (T003, T008).
- State: handled. First run (`first_run_has_no_state_and_no_file`). Older format: none; the magic leaves room for a version. Crash mid-operation: stale rule, two assets after a crash, the newest wins, a half upload is not `uploaded` and is deleted after 120 s. Partial failure: the table above. Retry: a rerun replaces its ticket, asset names are unique, deletes are idempotent (404 is gone).
- Time and concurrency: handled. Races in T006. Timeouts on every GitHub call. Clock: age from the server `Date`. Boundaries: 119 and 120 s, 599 and 600 s, label 120 and 121 s ahead. A duplicated dispatch queues and runs twice, inside the limits of the concurrency group. Late and out-of-order events: stable order by asset id.
- Resources: handled with limits. Disk: bundles and ciphertext under `--work`, not the 64 MB `/tmp`. Large input: chunked crypto, streamed scan, caps of 10 MB, 100 MB, 500 MB, 10,000 entries. Slow or garbage dependency: one retry, a bounded pause, then a fixed fault. Growth: 2 state assets, 10 bundles, at most 300 assets listed; the queue itself has no cap, so tickets grow only with waiting runs and each is deleted at its run's end. Memory: 64 KiB chunks, a 64 KiB scan buffer, nested zips at most 64 MB, state at most 10 MB.
- Permissions and trust: handled. The state token reaches `releases.rs` only (scoped to the store repo); keys reach `seal.rs`; nothing reads vault credentials. The pipeline author cannot name a release, a repo, a setting or a credential, cannot widen the entry (a reviewed file on `main`), and cannot see the state file outside its one step. The agent holds the artifact key, a Loki token and a read token for the store repo, nothing else.
- UI: N/A. No UI; the surface is the fixed stdout and stderr lines.

## Rollout and rollback

Rollout:

1. T001 lands alone on a branch (proves the lock). T002 to T021 land on `main` through a pull request linked to this folder; the unchanged selftest and `make lint` pass; the `rust-ci-runner` pipeline passes on `main`.
2. T024: tag `rust-ci-runner-v0.4.0`; read the digest.
3. T038: docs.
4. Nothing else. The pin, the watch file and the workflow do not change, so no real run uses 0.4.0 until the owner lifts the rule or moves the pin by hand. When that happens the order is: pin (its own reviewed commit, on a branch first so the selftest runs), Bitwarden keys and the store repo, then the watch entry keys, then the pipeline (R9). The old canaries (quickstart sections 2 and 3) run at that point.

Rollback: revert the Rust commits. The released image stays in the registry and is not pinned; nothing here pulls `latest`. If the owner has moved the pin later, the order is: remove the watch keys first (or in the same commit as the pin revert), then revert the pin, because a pinned runner without 0.4.0 refuses unknown keys. Queue, state and artifact releases stay in the store repo; they are encrypted or empty. Fixes ship as the next patch version; never move or delete a tag. Rotate the nine new settings if a leak is suspected and revoke the state token.

## Owner actions

Done by the owner, never by an agent, when the rule is lifted or the pin moves. No value is written in any file.

1. Create the store repo: private, no code, no workflows, no tag rulesets that block creation, immutable releases off, branch protection on its default branch (administrators included), and one initial README commit.
2. Create a fine-grained token scoped to the store repo only (contents write); put its expiry on a calendar.
3. Create Bitwarden entries for the four state settings: `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_STATE_REPO`. The two keys come from `openssl rand -base64 33`, different values. The five vault names stay unset.
4. The debug agent gets only the artifact key, a Loki read token limited to this tenant and step `ci`, and a GitHub read token scoped to the store repo. No write tools; egress to GitHub and Loki only.
5. In `sudtanj/factory-monorepo`: branch protection on `main` (reviews, no direct pushes, administrators included); a non-secret fixture for failure drills; the 002 pipeline keeps the Playwright HTML reporter off.
6. Build the Grafana alert rules by hand from `contracts/loki-lines.md` and attach a contact point.
7. Calendar item: state token expiry.
8. Run the release canary (quickstart section 2), including the real Playwright failure trace, before the first real run.
9. Decide whether to lift the owner rule for the deferred items: pin, schedule, vault read, alert and agent files.

## Devops assessment

## Devops assessment

Findings from the lead's devops review of 11d2ef0 and the critic review of faa835f, with their handling after the critic decisions.

Amendment of 2026-11-28 (owner rule): these rows change. Cron slack, concurrency cap, the 190-minute margin, the reviewer on `factory-ci`, `due`, image size and the arm64 smoke no longer apply (no schedule, no workflow edit, no Node). Tick still deployed: kept on purpose, the tick is not touched. Rollback order: now a Rust revert. Monitoring: Loki contract and tests stay, the alert file moves to Grafana by hand. Budget: 45-minute limit, budget 35 minutes, wait ends at budget minus 30 (D16).

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
| Staged scope | One release, 0.4.0, no Node; the vault read is out of scope | Summary |

## Security assessment

Amendment of 2026-11-28 (owner rule): the `bw list` row and the vault parts of the agent-kit and allowed-branch rows are deferred with the vault read. The agent kit is the README commands, not a script; the pinned-script control is gone, so the README says the runner binary comes from the pinned image and artifacts are untrusted data. The tick path never holds store settings (FR-062).

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
- O2: `bw list items` and the collection-limited account belong to the deferred vault read.
- O3: HKDF assumes machine-made keys; the format check and the README (`openssl rand -base64 33`) are the control. PBKDF2 is the alternative.
- O4: a store outage at registration prints `state store not configured` and logs the cause.
- O5: the secondary limit of 500 content-generating requests per hour caps sustained live tickets at about eight.
- O6: scanning `FACTORY_CI_LOG_USER` (numeric) can refuse a bundle that holds those digits by chance (R11).
- O7: a state step that never writes the file leaves nothing captured, so the old state stays.
- O8: no Node in the image (deferred with the vault read).
- O9: the agent opens bundles with `decrypt-artifact` from the pinned image; there is no pinned-hash script.
- O10: the 002 pipeline must order its steps so the state step has no parallel sibling.
- O11: a token expiry shows only as `state store not configured`; the alert rules and a calendar item guard it.
- O12: a canary of 200 trials proves read lag is rare, not absent; overlapping runs still cannot corrupt state, because each save checks its fence.
- O13: a ticket that is broken and registered again goes to the back; accepted residual (FR-011).
- O14: a duplicated tick runs the Playwright login twice in sequence; accepted ("every run must run").
- O15: no cron line exists, so no repo, branch or project of a schedule is public.
- O16: 011 ships inert. The image is released and not pinned, and the watch file cannot carry `state`, so no real run uses the queue or state until the owner lifts the rule or moves the pin (spec R5).
- O17: the concurrency group (kept) already serializes runs of one repo, ref and project, so the queue is mostly a guard for cases the group does not see; the owner may drop US2 with no effect on state and artifacts.
- O18: D19 relies on the container name and the docker socket of the unchanged workflow; a change there stops stateful runs with `runner fault` and turns the job-limit test red only if `timeout-minutes` changes, not the name. The name parser test documents the format.
- O19: the Rust-only rule is read as binding workflows, scripts, YAML and the Dockerfile; the READMEs still change (FR-041).

## Project Structure

```text
specs/011-ci-state-queue/  spec.md plan.md research.md data-model.md quickstart.md tasks.md
  contracts/ config-files.md release-store.md asset-format.md loki-lines.md
  checklists/requirements.md
rust-ci-runner/  Cargo.toml Cargo.lock .factory-ci.toml
  src/  seal releases queue signal state bundle scan stateful (new), store watch pipeline redact log engine job ci main util lib fake (changed), one *_tests.rs per new module
  tests/no_vendor.rs (unchanged)
```

No file under `.github/` and no `Dockerfile` changes.

**Structure Decision**: the existing layout; new logic in single-purpose modules, no new crate, no new directory.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| `miniz_oxide` (+ `adler2`), two new lock entries | FR-049 scans zip entries (`trace.zip`); inflate is what std lacks | hand-written inflate is security code to maintain; shelling out to `unzip` adds a binary and text parsing of hostile names |
| `libc` direct dependency (already locked) | FR-009 needs SIGINT and SIGTERM handling | a stale rule alone leaves a cancelled run's ticket for 10 minutes |
| `ring`, `memchr` direct dependencies (already locked) | AEAD, HKDF, fast substring search | new crates for the same primitives |
| A hand-written tar writer and reader | one archive that the scanner and the decryptor both read | a `tar` crate adds a dependency for about 80 lines |
| A separate store repo | a code-free repo limits the token | a shared repo exposes code to a content-write token |
