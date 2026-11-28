# Implementation Plan: Scheduled runs, a strict queue, saved state and debug artifacts for the ci command

**Branch**: `011-ci-state-queue` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/011-ci-state-queue/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran (a shell was available). `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before this plan and `/speckit-analyze` after `tasks.md`. The spec edits made while planning (Clarifications "plan calls", FR-024, FR-028, FR-042, FR-043, FR-049, Assumptions) change the spec, so `/speckit-analyze` must run before `/speckit-implement`. No devops or security assessment was handed over for 011; the section below records that and lists the review notes the plan folded in itself.

## Summary

011 builds on 010 (step secrets, runner 0.3.0, merged on `main` before any 011 code starts). It adds five things to the `ci` command and ships them as runner 0.4.0:

1. A strict FIFO queue per state. A ticket is a release asset in the private repo that owns the pipeline. Asset names are unique per run, the asset id gives a stable order, a label carries the run id and the last renewal in GitHub server time.
2. Encrypted state and artifacts. ChaCha20-Poly1305 in 64 KiB chunks through `ring` (already in `Cargo.lock` through rustls), key from HKDF-SHA256 with a per-file salt.
3. Debug bundles. A plain tar of the declared paths, scanned for every known secret form (plain, base64, lines, inside zips through `miniz_oxide`), then encrypted and uploaded.
4. A vault read. The pinned Bitwarden CLI (`bw`, a Node bundle run by Alpine's `nodejs`) unlocks inside the queue turn, `bw list items` returns the login items, the runner keeps only the declared fields and wipes the data dir before any step starts.
5. A schedule. One 15-minute cron, a `due` command that prints entry indexes, and `ci --schedule FILE --entry N`, so no repo name reaches the public log.

6. Step-scoped state. The state file exists in the checkout only while the one step that declares it (`state = true`) runs. No other step can run beside it, and none sees it (D8).

Everything else (watch list, Loki, fixed public grammar, redactor, engine) stays as in 008 and 010.

## Technical Context

**Language/Version**: Rust 2021, `rust:1.90` in the pipeline, musl static binary in the release.

**Primary Dependencies**: new direct dependencies, all but two already in `Cargo.lock`: `ring` 0.17.14 (locked, via rustls), `memchr` 2.8.3 (locked), `libc` 0.2.191 (locked), and two new lock entries `miniz_oxide` 0.8.9 and `adler2` 2.0.1. See Complexity Tracking. Image: `nodejs` (Alpine 3.23, 24.22.3) and the pinned `bw` npm bundle.

**Storage**: GitHub releases of the owning private repo (queue, state, artifacts). Local: files under `--work` and a `bw` data dir under `/tmp` (tmpfs). No database.

**Testing**: `cargo test --locked --lib` in the `rust-ci-runner` pipeline, with the new mock-server groups in their own processes; `factory-ci-selftest.yml` compiles, builds the image for both architectures and runs shell checks. Queue, state and artifact logic runs against an in-memory `Releases` fake, so only two test modules start HTTP servers.

**Target Platform**: `ghcr.io/sudtanj/rust-ci-runner` linux/amd64 and arm64, started by `factory-monorepo-ci.yml` on a GitHub-hosted VM.

**Project Type**: CLI, one binary.

**Performance Goals**: queue poll 20 s, renew 60 s, claim settle 3 s. State at most 10 MB, bundle at most 100 MB, memory bounded by one 64 KiB chunk plus the scan window.

**Constraints**: the dev VM has 1 GB and no cargo, docker or builds. In the runner container `/tmp` is a 64 MB tmpfs and the root is read-only (`factory-monorepo-ci.yml`, `--read-only --tmpfs /tmp:rw,size=64m`), so bundles and ciphertext go under `--work`, not `/tmp`. A step container allows 512 processes (`engine.rs`, `--pids-limit 512`).

**Scale/Scope**: at most 10 schedule entries, 3 tickets per state, 2 state assets, 10 bundles per project, 50 vault fields per pipeline.

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Runner, schedule file and workflow stay in this repo. |
| II Secrets source | Pass. The workflow gains no GitHub secret. The six new settings and every vault value come from Bitwarden inside the runner. The README secret list does not grow. |
| III Public logs | Pass by design. Ten new fixed lines, no name or count. Details go to Loki as step `ci`. A hostile fixture searches every sink. |
| IV Simplicity | Pass with four justified additions (Complexity Tracking): `miniz_oxide`, `libc` for signals, Node in the image, a hand-written tar. |
| V Tested | Pass. Every FR maps to a test below; tests are written first (T002 to T012). |
| VI Spec-driven | Pass for plan and tasks; `/speckit-analyze` runs next. |

Re-check after design: unchanged. Principle II needs one reading. The runner holds the vault credentials, the Bitwarden project holds them, and no GitHub secret is added; that is the owner's decision of 2026-11-28.

## Architecture decisions

Evidence for each is in [research.md](research.md). The alternatives rejected are listed here.

### D1 The atomic queue primitive: release assets in a per-state queue release

Chosen: a release `ci-queue-<state>` (prerelease, never `latest`) in the owning repo. A ticket is one asset named `t-<runid>` with a one-byte body and label `<runid>.<server epoch>`.

- Atomic create. GitHub documents that uploading an asset with the same filename as an existing one is an error and the old file must be deleted first. A rerun of one run id therefore replaces its own ticket (delete, upload) and two live runs never share a name.
- Order. The asset id is a server-assigned integer, so every reader sees one total order. FIFO is the ascending id. Two simultaneous creates get an order the store decides and both readers see the same one (FR-010). No client clock takes part.
- Server time (FR-013). Each API answer carries a `Date` header. A holder writes `<runid>.<epoch of the last Date header it saw>` as the label on every renewal (`PATCH /releases/assets/{id}`, `label` is a documented field). A reader judges age as its own latest `Date` minus the epoch in the label. Nothing depends on how `updated_at` moves, and no runner clock is read.
- Capacity (FR-011). A run lists, drops stale tickets, refuses at 3 live ones without registering, otherwise registers and lists again. A ticket at position 4 or later by id deletes itself and ends with `queue is full`. Four simultaneous starts end with exactly one refusal, never with four running.
- Turn. A run may start when its ticket has the lowest id among live tickets. It waits 3 s (settle), lists again, and starts only if it is still first. A ticket that shows up late with a lower id sends it back to waiting. This narrows the read-after-write window; it does not close it (R15).
- Stale removal. A waiter deletes any live-looking ticket whose label epoch is 600 s or more behind its own latest `Date`. Deleting twice is fine (404 counts as gone). Each removal goes to Loki as step `ci`.
- Fence (FR-015). Before a save and again after the new asset is stored, the run checks its own ticket still exists with its own label prefix. If it is gone after the upload, the run deletes the asset it just wrote and ends `state save failed`.

Alternatives rejected:
- Git refs (`POST /git/refs`, 422 when it exists). The create is atomic, but a ref has no timestamp and no payload, so renewal and the run id would need a second object, and a name reused after a stale break confuses an old holder with its successor.
- Issue comments. Ids are ordered, but writing a comment needs the issues permission. The spec fixes the token at contents write, and comments notify the owner.
- Contents API on a queue file (compare-and-swap through the blob sha). It is atomic and gives a real list, but every renewal is a commit on a branch, 45 or more per run, with push events.
- Releases as tickets (`tag_name` unique). Creates a git tag per ticket, has no clean renewal field, and `created_at` has one-second resolution.
- The workflow `concurrency` group. It cancels the older pending run, which spec Clarifications already rejected.

### D2 Crypto: `ring`, ChaCha20-Poly1305 in chunks, HKDF-SHA256

Chosen: `ring` 0.17.14, already locked as a dependency of `rustls`. Key: HKDF-SHA256 with a fresh 16-byte salt per file, the key setting as input, and `info` = `factory-ci/v1/state` or `factory-ci/v1/artifact`. Cipher: ChaCha20-Poly1305 (constant time in software on both architectures). Format `FCI1`, see [contracts/asset-format.md](contracts/asset-format.md): header (magic, kind, salt) used as associated data, then chunks of 64 KiB plaintext plus a 16-byte tag, nonce = chunk counter plus a final-chunk flag (STREAM construction, so truncation, reordering and trailing bytes all fail).

Why this shape. `ring` has no streaming AEAD, and the spec asks for stream reads, so chunking keeps memory at one chunk for a 100 MB bundle. Domain separation in `info` means the state key cannot open an artifact even if the owner set both keys equal (FR-020 also refuses equal keys). HKDF, not a slow KDF, because the keys are machine-made (`openssl rand -base64 33`, 44 characters) and FR-020 enforces 32 characters or more; the README says so.

Decrypt for the agent: `rust-ci-runner decrypt-artifact` reads ciphertext on stdin and the key from `FACTORY_CI_ARTIFACT_KEY` in the environment, writes the tar to stdout. The release workflow already publishes `rust-ci-runner-x86_64-linux` and `-aarch64-linux` as assets of this public repo (`rust-ci-runner-release.yml`, step "release assets"), so the agent needs `gh` and `tar`, not docker. There is no `decrypt-state` command: the agent never needs one.

Alternatives rejected:
- The `age` crate or `rage`: many new crates and a passphrase mode that needs a terminal.
- `openssl enc`: no authenticated mode on the command line.
- A new crate such as `chacha20poly1305`: adds lock entries for something `ring` already does.
- AES-GCM: needs hardware support for speed and safety on some hosts; no benefit here.
- PBKDF2: fine for passwords, slower, and the unit tests would pay for it. Recorded as an open point (O3).

### D3 `bw` in the image: Node bundle, not the glibc binary

Chosen: the `bitwarden-cli-2026.10.1-npm-build.zip` asset of release `cli-v2026.10.1` (sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3`, 5.1 MB, verified against a download), pinned by `ADD --checksum` like `bws`. It holds `bw.js`, two chunk files, a wasm file and locales: 19 MB unpacked, no native module. It needs Node 22 or newer (`engines`); Alpine 3.23 ships `nodejs` 24.22.3. A two-line `/usr/local/bin/bw` shell script runs `node /opt/bw/bw.js "$@"`. The same bundle runs on amd64 and arm64, so the Dockerfile needs no per-architecture stage.

Rejected: the standalone `bw-oss-linux-2026.10.1.zip` (sha256 `53384064...6738`). It is a 143 MB ELF linked against glibc (`INTERP /lib64/ld-linux-x86-64.so.2`, needs `libstdc++`, `libgcc_s`, `libc.so.6`), so it needs a glibc shim on Alpine and costs about twice the size. Measured on the downloaded file with `readelf`.

Image size. Alpine's package index lists `nodejs` at 52.3 MB installed plus roughly 15 MB of libraries (`icu-libs`, `libstdc++`, `ada-libs`, `brotli-libs`, `c-ares`, `nghttp2-libs`, `simdjson`, `sqlite-libs`, `zstd-libs`). The estimate is +70 MB on an image of roughly 110 MB. Nobody can build here, so the PR records the measured size from the selftest build (T023).

Wipe (FR-046). Each `bw` child gets an empty environment plus `PATH`, `HOME=/tmp` and exactly what that call needs: `BW_CLIENTID` and `BW_CLIENTSECRET` for `login --apikey`, `BW_PASSWORD` for `unlock --passwordenv BW_PASSWORD --raw`, `BW_SESSION` for `list items`, and `BITWARDENCLI_APPDATA_DIR=/tmp/vault-<random>` (a 0700 directory on the tmpfs) for all. The sequence is login, unlock, `bw list items`, `bw logout`, then the runner removes the directory and checks it is gone. The runner lists all items and filters by exact name in process, so no item name ever appears in argv (the spec wants names only in the environment path, and `bw get item` has no way to take one). A failed step in the sequence still runs the cleanup. The first step starts only after cleanup succeeded.

### D4 One list of runner keys

`store::RUNNER_KEYS: [&str; 10]` replaces 010's four-name list (010 put it in `store.rs`). The first four are the settings read at start (`BOOT_KEYS`). The others are `FACTORY_CI_STATE_TOKEN`, `FACTORY_CI_STATE_KEY`, `FACTORY_CI_ARTIFACT_KEY`, `FACTORY_CI_VAULT_CLIENTID`, `FACTORY_CI_VAULT_CLIENTSECRET` and `FACTORY_CI_VAULT_PASSWORD`. Users of the list:

- `pipeline::valid_secret_name` refuses each name, in any case (a step `secrets` list and a vault `env` name both go through it).
- `watch::parse` refuses each name in an entry `secrets` list, through the same function.
- `store::fetch_named` (010) refuses to look up any of them.
- `store::fetch_runner_keys` (new) looks up only names from the list and refuses anything else.
- `log.rs` takes the secret-key names of its `secret <KEY>:` faults from the list (010 did the same).

A table test walks the list and asserts that every consumer refuses every name, so adding an eleventh name to the list closes all five places at once.

### D5 Schedule

- Cron `2,17,32,47 * * * *`. GitHub documents that the shortest interval is 5 minutes, that the event can be delayed under high load (the start of every hour is named), and that queued jobs may be dropped. Offsetting by two minutes leaves 13 minutes of slack inside FR-003's 15-minute window.
- `rust-ci-runner due --schedule FILE [--now-minute N]` prints a JSON array of due indexes (`[0,2]`, `[]`) and nothing else. A bad file prints `schedule file refused` on stderr and exits 1. `plan --check` gains `--schedule FILE` and lists every problem, with no token and no network.
- The workflow `due` job runs on every event. On `schedule` it runs `due` in the image; on `workflow_dispatch` it writes `[-1]` without a container. The `ci` job takes its matrix from that array, named `ci <index>`; `-1` means the manual inputs. The job passes `--schedule /etc/factory-ci/schedule.toml --entry <index>` and no repo, branch or project, so the public log shows an index only (FR-001). `ci` accepts either the old triple or `--schedule` plus `--entry`, never both.
- 60-day rule. GitHub documents that scheduled workflows in a public repo are disabled after 60 days without repository activity. Mitigation inside this feature: the `due` job runs `git log -1 --format=%ct` on the checkout it already has and prints a `::warning::` annotation when the last commit on `main` is older than 50 days, and the README tells the owner to push any commit before day 60. No third-party keepalive action, no write permission. The warning is visible in the Actions UI only (R6).

### D6 Where the tests run

New modules, grouped by whether they start servers:

- No server, threads end: `seal_tests`, `schedule_tests`, `scan_tests`, `bundle_tests`, `state_tests`, `vault_tests` (fake `bw` script, no server), `queue_tests` (race tests over the in-memory `Releases` fake, at most 6 threads per test, all joined). They run in the existing first command.
- Servers: `releases_http_tests` (one `tiny_http` per test, `Drop` calls `unblock()` as `ci_tests` does) and `ci_state_tests` (`ci::run` with stub `bw`, `docker`, `git`, a Loki mock and the fake `Releases` injected through `Seams`). Each gets its own process group in `rust-ci-runner/.factory-ci.toml`, and the first command gains `--skip releases_http_tests:: --skip ci_state_tests::` (`--skip ci_tests::` does not match `ci_state_tests::`, so the skip is needed). With 010's split of `ci_tests::step_secrets::`, the unit step runs six commands.

### D7 Order inside `run_checkout` (extends 010)

1. Load the pipeline. Parse faults, including `state` and `artifacts` shape, vault shape, runner-key names and `BW_`, `BITWARDEN` names, are `pipeline file refused`.
2. Docker rules (010) plus: a step with `docker = true` and a `vault` list is refused (`pipeline file refused`, FR-042).
3. State and artifacts allowed: entry has `state = true`, repo is private (from the identity call, which now returns `private`), else `state not allowed`. No ticket, no container.
4. Allowlists: 010's `unlisted` (`step secret not allowed`), then vault items against `vault_items` (`vault item not allowed`). No lookup yet.
5. Ref gate (010's `secrets_allowed_on`) for any of: step secrets, vault, state, artifacts. Failure: `repository or ref not in the watch list`.
6. Runner keys for what the pipeline needs: state needs `FACTORY_CI_STATE_TOKEN` and `FACTORY_CI_STATE_KEY`, artifacts need the token and `FACTORY_CI_ARTIFACT_KEY`. One `fetch_runner_keys` call, only when needed. Failure: `state store not configured` (FR-020), before any ticket.
7. 010's `fetch_named` for step secrets (`step secret unavailable`), then `log.add_secrets`.
8. `state` declared: register and wait (`queue is full`, `queue wait timed out`). Inside the turn: fetch and decrypt the state into memory (`state restore failed`), add its strings to the log, vault read (`vault unavailable`), add values to the log. The file is not written yet.
9. `run_pipeline` with the step hook of D8: the file is written just before the `state = true` step and read and removed just after it.
10. After the pipeline: chown the declared artifact paths (root helper container), build, scan, encrypt and upload artifacts when due, save the captured state bytes when the pipeline passed, release the ticket, remove the checkout.
11. Any exit path removes the ticket: a drop guard covers return and panic, a watcher thread covers SIGINT and SIGTERM.

A pipeline with a vault declaration and no `[state]` reads the vault at step 8 without a ticket. Steps 3 to 6 each print their own fixed line first-match, as in 010.

### D8 State reaches one step only (from the 002 threat model)

Chosen: a step key `state = true` and a step hook in the engine.

- Validation (`pipeline.rs`, at parse). `[state]` needs exactly one step with `state = true`; no `[state]` means no such step. Every other step must be a transitive dependency or a transitive dependent of the state step, so the engine's scheduler (`max_parallel` 4 in `job.rs`) can never run one beside it. A pipeline that breaks this is `pipeline file refused`.
- Hook (`engine.rs`). `RunCtx` gains `hook: Option<&dyn StepHook>`; `run_step` calls `before(name)` ahead of `docker run` and `after(name, &State)` behind it, on the step's own thread, before the step counts as finished. Descendants start only after `after` returned, so the removal always precedes the next step.
- `before`, for the state step: write the in-memory plaintext at the declared path in the checkout, mode 0600, owner the runner user (parent folders created, a symlink anywhere on the path refused). No state yet (first run): write nothing, so the step sees no file (FR-021).
- `after`, for the state step, pass or fail: run the root helper (`chown -R` and `chmod -R u+rX` of that path to the workspace owner; no `a+r`), read the file (symlink refused, over 10 MB refused, empty or missing means nothing to save), keep the bytes in memory, remove the file from the checkout. If the removal fails the run fails (`runner fault`) and no later step starts.
- The declaring step should end with `chown <workspace owner> <file>` (the workspace owner is the uid and gid of the checkout folder). The helper is the backstop, so a forgotten `chown` does not fail the save. Neither path ever makes a copy world-readable.
- Save. The captured bytes seal from memory under the state key after a passing pipeline (FR-024, FR-026). Nothing is written to disk in plaintext other than the one file inside the state step's window. The strings of FR-050 come from the restored bytes before the step and from the captured bytes after it.
- Vault values. Already per step: `run_step` exports only the env names of that step's own `secrets` and `vault`. A test asserts a sibling step's environment holds none.

Alternatives rejected: a bind mount of a runner-private file onto the declared path (a first run would show an empty file where FR-021 promises none, a step that renames the file fails with `EBUSY`, and Docker creates root-owned placeholders in the checkout); restoring into the checkout before the pipeline starts (every step, including `npm install`, would read the cookies, which is the 002 finding); a per-step copy of the whole workspace (large and slow).

## Files

### Added (rust-ci-runner/src unless noted)

| File | Holds |
|---|---|
| `seal.rs` | `Kind`, `seal`, `open`, `SealError`. Format in contracts. |
| `schedule.rs` | `Entry`, `parse`, `read`, `due`, `minute_now`. |
| `releases.rs` | `trait Releases`, `Asset`, `HttpReleases`, `ReleaseError`. Only code that talks to api.github.com with the state token. |
| `queue.rs` | `Timing`, `with_turn`, `Fence`, `QueueError`, `is_stale`, `wait_decision`. |
| `signal.rs` | `install()` (SIGINT and SIGTERM set a flag), `pending()`. The only `unsafe` in the crate, a `libc::sigaction` call. |
| `state.rs` | `restore`, `save`, retention of 2. |
| `bundle.rs` | `build` (tar with pax `path` records, regular files only, 100 MB cap), `upload_artifact`, retention of 10. |
| `scan.rs` | `scan(tar_file, needles)`: raw search, zip entries to depth 2, limits of FR-049. |
| `vault.rs` | `read`, `VaultError` (positions and counts only). The only new file that names the vendor, so `tests/no_vendor.rs` allows it. |
| `stateful.rs` | The post-checkout phase of step 8 to 10 (restore, vault, pipeline with the D8 hook, artifacts, save). Keeps `ci.rs` readable. |
| `*_tests.rs` | One per module above, plus `ci_state_tests.rs`, `releases_http_tests.rs`. |
| `.github/factory-ci-schedule.toml` | Valid and empty at first; header comment documents keys. |
| `specs/011-ci-state-queue/{research.md,data-model.md,contracts/*,quickstart.md,tasks.md}` | Spec artifacts. |

### Changed

| File | Change |
|---|---|
| `store.rs` | `RUNNER_KEYS` grows to 10, `BOOT_KEYS` is the first 4, add `fetch_runner_keys` and `RunnerValues` (no `Debug` value). Key rules: at least 32 characters for the two keys, the two differ. |
| `watch.rs` | `ENTRY_KEYS` gains `state` (bool, default false) and `vault_items` (list of exact names; each 1 to 100 characters with no control character; at most `MAX_LIST`). `state` needs a numeric `id`. Runner-key names in `secrets` refuse the file (already true by prefix; now by list too). |
| `pipeline.rs` | `Step.vault: Vec<VaultLogin>`; `Pipeline.state`, `Pipeline.artifacts` (optional pipeline-wide tables); validation of FR-017, FR-018, FR-042; `valid_secret_name` uses `RUNNER_KEYS`. Accessors `vault_wants`, `vault_items`. |
| `redact.rs`, `log.rs` | `Redactor::needles()`, `Log::scan_set()`; `FAULTS` 16 to 25; the runner line `waiting for the queue` in `public_form`; `SECRET_KEYS` from `RUNNER_KEYS`. |
| `engine.rs` | `run_step` also exports each `step.vault` env name from the secrets map (`-e NAME`, value in the child environment, as secrets today). New `StepHook` trait (`before`, `after`) in `RunCtx`, called around `docker run` (D8). New `Engine::own_paths(image, parent, uid, gid, names)`: one `docker run --rm --network none --user 0 --entrypoint sh -v <ws>:/w <image> -c 'chown -R "$0:$1" -- "$@" && chmod -R u+rX -- "$@"'` with uid, gid and the `/w/<path>` list as arguments (never `a+r`), same pattern as `remove_dir`. |
| `ci.rs` | `Seams` gains `bw`, `uploads_base`, `hub: Option<Arc<dyn Releases>>`, `timing`, `exit`, `vault_dir`; `Limits` gains `hub`, `upload`, `vault`. `parse_args` accepts `--schedule FILE --entry N` instead of the triple. `identity` returns `private`. `job()` builds the runner-key closure next to 010's `fetch`. `run_checkout` follows D7 and calls `stateful.rs`. |
| `main.rs` | Subcommands `due`, `decrypt-artifact`; `plan --check` accepts `--schedule`; `run_ci` installs the signal handler; `USAGE` text. |
| `lib.rs` | New `pub mod` and `#[cfg(test)] mod` lines (T001). |
| `util.rs` | `parse_http_date` (IMF-fixdate to epoch, for the `Date` header). |
| `tests/no_vendor.rs` | Allow `vault.rs`. |
| `Dockerfile` | `nodejs`, the pinned `bw` bundle, `bw` wrapper, `RUN bw --version` smoke. |
| `Cargo.toml`, `Cargo.lock` | 0.4.0; dependencies of Complexity Tracking; two new lock entries, by hand. |
| `.factory-ci.toml` | Two new process groups and the `--skip` additions. |
| `.github/workflows/factory-monorepo-ci.yml` | D5 shape, concurrency block removed, 190 minutes, schedule mounted read-only. |
| `.github/scripts/test-factory-monorepo-ci.sh` | Updated checks (see T012). |
| `.github/factory-ci-repos.toml` | Header comment; `state` and `vault_items` for `sudtanj/factory-monorepo` at T032 only. |
| `.github/workflows/factory-ci-selftest.yml` | One step: `plan --check --schedule`. |
| `README.md`, `rust-ci-runner/README.md` | Last task. |

### Not changed, and why

- `ship.rs`: Loki delivery is untouched; queue and artifact lines use `log.out("ci", ...)`.
- `job.rs`: `run_pipeline` keeps its signature. The vault and state values travel in the existing `secrets` map and `step.secrets` plus `step.vault` names.
- The tick workflow and its scripts: untouched (Non-goals).

## Data and API contracts

- [contracts/config-files.md](contracts/config-files.md): schedule file, watch keys, pipeline tables, vault declaration, `ci` and `due` command lines, public lines.
- [contracts/release-store.md](contracts/release-store.md): release tags, asset names, labels, the exact GitHub calls, ticket protocol, retention.
- [contracts/asset-format.md](contracts/asset-format.md): the `FCI1` byte layout and the decrypt command.
- [data-model.md](data-model.md): entities, validation rules, state transitions.

## Engineer briefs

Order and parallelism. T001 first, alone (skeleton, dependencies, lock). T002 to T012 (tests first) run in parallel, one file each. T013 to T023 run in parallel after T001 and their own tests, except that T017 comes before T018, T019 and T021 comes after T015 and T017. T024 needs T013 to T023. T025 needs T015, T016. T026 needs T024. T027 can start after T012 but merges after T029. T028 to T034 are sequential and happen after the code is on `main`. T034 is last. Each brief is also a line in `tasks.md`; IDs match.

Precondition for everything: 010 is merged to `main` and 0.3.0 is released with its pin in place. Branch from that `main`. Names used below (`fetch_named`, `StepSecrets`, `Log::add_secrets`, `secrets_allowed_on`, `RUNNER_KEYS`, `FAULTS` at 16) are 010's.

### T001 Skeleton, dependencies, lock
- Goal: every new module exists and compiles empty, so later tasks only fill files.
- Files: `rust-ci-runner/src/lib.rs`, new empty modules and test files listed under Added, `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`.
- Acceptance: `lib.rs` declares `seal`, `schedule`, `releases`, `queue`, `signal`, `state`, `bundle`, `scan`, `vault`, `stateful` and the test modules `seal_tests`, `schedule_tests`, `releases_http_tests`, `queue_tests`, `state_tests`, `bundle_tests`, `scan_tests`, `vault_tests`, `ci_state_tests`. `Cargo.toml` adds `ring = { version = "0.17", default-features = false, features = ["alloc"] }`, `memchr = "2"`, `libc = "0.2"`, `miniz_oxide = { version = "0.8", default-features = false, features = ["with-alloc"] }`. `Cargo.lock`: add `"libc"`, `"memchr"`, `"miniz_oxide"`, `"ring"` to the dependencies of `rust-ci-runner` in alphabetical position; add `[[package]]` entries `adler2` 2.0.1 (checksum `320119579fcad9c21884f5c4861d16174d0e06250625266f50fe6898340abefa`, no dependencies) and `miniz_oxide` 0.8.9 (checksum `1fa76a2c86f704bdb222d66965fb3d63269ce38518b83cb0575fca855ebb6316`, dependency `adler2`). Version stays 0.3.0 here.
- Constraints: no cargo on this VM, so check the two checksums against `https://index.crates.io/mi/ni/miniz_oxide` and `https://index.crates.io/ad/le/adler2` before committing. `--locked` fails on any mismatch, and the selftest build is the proof.

### T013 Runner keys and watch keys (FR-020, FR-034, FR-043)
- Goal: ten runner keys, one list, two new watch keys.
- Files: `rust-ci-runner/src/store.rs`, `rust-ci-runner/src/watch.rs`.
- Acceptance: `RUNNER_KEYS` has the ten names of D4 and `BOOT_KEYS` is its first four; `store::fetch` still needs exactly those four. `fetch_runner_keys(bws, token, server, limit, names)` runs one `bws secret list` with the same retry and server check, returns the requested names only, refuses any name outside `RUNNER_KEYS`, applies the 010 value rule, requires 32 characters or more for the state and artifact keys and refuses equal state and artifact keys (a missing, duplicated or short value is `Count` or `Unusable`; equal keys are `Unusable`). Errors and `Debug` show names and counts only. `watch::parse` accepts `state` (bool; needs `id`) and `vault_items` (list of strings, each 1 to 100 characters, no control character, no duplicate, at most 20) and refuses the whole file on any problem; a runner-key name in `secrets` refuses the file. `WatchEntry` gains `state: bool` and `vault_items: Vec<String>`.
- Constraints: `watch.rs` must not mention the vendor. Take the key names from `store::RUNNER_KEYS`.

### T014 Pipeline declarations (FR-017, FR-018, FR-042)
- Goal: parse and validate `[state]`, `[artifacts]` and step `vault` entries.
- Files: `rust-ci-runner/src/pipeline.rs`.
- Acceptance: `[state]` takes `name` and `path`; `[artifacts]` takes `paths` and `when` (`failure` default, or `always`); a step takes `vault = [{ item, field, env }]`; unknown keys are errors everywhere; a second `[state]` is a TOML error. State name 1 to 64 characters of `[A-Za-z0-9_-]`. A path (state or artifact) is refused when empty, absolute, holds `..`, NUL or a newline, or a list holds more than 20. An artifact path that equals, contains or lies under the state path is refused by component, so `.ci-state` and `.ci-state-2` are both accepted. A vault `env` name is refused when `valid_secret_name` refuses it (this covers `FACTORY_CI*`, `BW_*`, `BITWARDEN*` and `RUNNER_KEYS`), when it repeats inside a step across `env`, `secrets` and `vault`, or when two steps map one env name to different item and field pairs. A step has at most 20 vault entries and the pipeline at most 50. A step with `docker = true` and a vault list is refused. `Step` gains `state: bool` (key `state`); `[state]` needs exactly one such step and none otherwise; every other step must be a transitive dependency or dependent of it (use the existing `depends_on` graph, as `cycle` does), else refused. Every name in a step's `env` map, `secrets` list and vault `env` is refused when, upper-cased, it starts with `BW_`, `BWS_`, `BITWARDENCLI` or `FACTORY_CI` or equals a `RUNNER_KEYS` name (010 covers `secrets` already; extend the check to `env` and vault names). Grep the fixtures and `rust-ci-runner/.factory-ci.toml` for `env` keys with those prefixes before committing. `Pipeline::vault_wants()` returns each distinct `(item, field)` once, `vault_items()` each item once. Symlinks and checkout containment are checked later, at run time, by `stateful.rs`.
- Constraints: `pipeline.rs` may name the vendor (it does today). `valid_secret_name` consults `RUNNER_KEYS` in addition to its prefixes.

### T015 Seal (FR-027)
- Goal: `seal`, `open` and the `decrypt-artifact` core.
- Files: `rust-ci-runner/src/seal.rs`.
- Acceptance: the layout of contracts/asset-format.md; `seal(kind, key, reader, writer)` and `open(kind, key, reader, writer)` stream in 64 KiB chunks; a fresh salt per call from `getrandom`; `open` with the other kind's key, a changed byte, a truncated file (including cut at a chunk boundary), a reordered chunk, trailing bytes or a wrong magic returns an error, and the last verified chunk is the only plaintext released before an error. Empty plaintext round-trips as one final chunk. A 10 MB plaintext round-trips.
- Constraints: `ring::aead` and `ring::hkdf` only; no `unsafe`; keys never logged; `SealError` carries no key or plaintext.

### T016 Schedule (FR-001 to FR-004)
- Goal: parse the schedule file and say which entries are due.
- Files: `rust-ci-runner/src/schedule.rs`.
- Acceptance: contracts/config-files.md. `parse` returns every problem and refuses the whole file for an unknown or repeated key, a missing `repo`, `branch` or `interval_minutes`, a bad repo, branch or project, an interval that is not a multiple of 15 or lies outside 15 to 10080, a duplicate `(repo case-folded, branch, project)`, or more than 10 entries; an empty file is valid. `due(entries, minute)` returns the indexes whose `minute % interval < 15`; 14 is due, 15 is not; interval 15 is always due; `minute_now` is UTC epoch minutes.
- Constraints: no network, no clock inside `due`. Branch and project use the same rules as `ci`'s `parse_args`; reuse `folder_name_ok`.

### T017 Releases client and server time (FR-036)
- Goal: the only code that calls GitHub with the state token.
- Files: `rust-ci-runner/src/releases.rs`, `rust-ci-runner/src/util.rs`.
- Acceptance: the calls of contracts/release-store.md behind `trait Releases`. Every call returns the server time from the `Date` header (`util::parse_http_date`, strict IMF-fixdate, anything else is an error). Every call has a time limit and one retry on no answer, 429 or 5xx; 4xx is final. The token is sent only as `Authorization: Bearer`, never in a URL, argv or log line, and `Debug` of the client prints no token. Asset download follows the redirect by hand: ask with `Accept: application/octet-stream` and no redirects, then fetch `Location` with a fresh request that carries no `Authorization`. Upload streams a reader with an explicit `Content-Length` to `uploads.github.com`. `create_asset` returns `ReleaseError::Exists` on 422 `already_exists`. `ensure_release` handles a lost race (422, then fetch by tag). Response bodies and the token never reach an error text.
- Constraints: no write with any other token; no clone. Limits come from `Seams`.

### T018 Queue and signals (FR-006 to FR-016)
- Goal: strict FIFO with fence, stale rule and guaranteed ticket removal.
- Files: `rust-ci-runner/src/queue.rs`, `rust-ci-runner/src/signal.rs`.
- Acceptance: the protocol of D1 and contracts/release-store.md. `with_turn` registers (replace own ticket on `Exists`), prints `waiting for the queue` once through `log.event("runner", ...)` while it waits, polls every `timing.poll`, renews every `timing.renew` from a scoped thread, removes stale tickets with a Loki line each, claims after the settle re-list, runs the closure, stops the renewer and deletes the ticket on every path, including a panic in the closure (catch, delete, resume). `is_stale(now, stamp)` is true at 600 s and false at 599 s. `wait_decision(elapsed, max, first)` keeps waiting at 119 minutes and gives up at 120. A missing own ticket while waiting re-registers (after a 60 s grace for read lag) and counts toward the same 120 minutes. `Fence::held()` re-checks the ticket against the store. The signal watcher thread deletes the ticket, calls `timing.exit(1)` and nothing else. Renewal failures never stop the closure.
- Constraints: no queue text reaches stdout or stderr except `waiting for the queue` and the three fault lines; positions, ages and names go to Loki only. Timing values come from `Timing` so tests use milliseconds. `signal.rs` holds one `unsafe` block with a comment of why; the handler only stores an atomic.

### T019 Bundle and scan (FR-028 to FR-032, FR-049, FR-050)
- Goal: build the tar, refuse any secret, upload encrypted.
- Files: `rust-ci-runner/src/bundle.rs`, `rust-ci-runner/src/scan.rs`.
- Acceptance: `build` walks each declared path under the checkout with `symlink_metadata`: a regular file is stored, a folder is walked (depth 64), a symlink or special file is skipped with a Loki line, a missing path is skipped with a Loki line. Entries are ustar with a pax `path` record when the name is over 100 bytes, mode 0644, mtime 0. Over 100 MB (100 MB exactly passes) uploads nothing and logs the size. `scan(tar, needles)` searches every tar member and the whole file with `memchr::memmem` and carries `max_needle - 1` bytes across chunk edges; for each member that starts with `PK\x03\x04` it reads the central directory itself and inflates each entry with `miniz_oxide::inflate::stream` (64 KiB output buffer), to depth 2 (a zip inside a zip inside the tar is depth 2, one more is refused), at most 10,000 entries and 500 MB uncompressed in total, an encrypted entry (flag bit 0), zip64, a bad CRC or a bad central directory is refused. A member that starts with the magic bytes of gzip (`1f 8b`), bzip2 (`BZh`), xz (`fd 37 7a 58 5a 00`), zstd (`28 b5 2f fd`) or 7z (`37 7a bc af 27 1c`) is refused. Nested zips load into memory only up to 64 MB, else refused. The result is `Ok` or one `Refused` carrying a count of causes, no name, offset or value. The caller prints `artifact upload refused`, exits 1 and uploads nothing. Upload name `artifact-<runid>-<8 hex>.bin`; after it, keep the 10 newest `artifact-*` assets of the release. The bundle file and its ciphertext live under the run's directory in `--work`.
- Constraints: scan and encrypt read the same tar file; do not re-read the checkout. Needles come from `Log::scan_set()`.

### T020 Vault read (FR-042 to FR-047)
- Goal: read only the declared fields and leave nothing behind.
- Files: `rust-ci-runner/src/vault.rs`.
- Acceptance: the call sequence of D3 with a time limit of 60 s per call and one retry of the whole sequence on a timeout or crash, never after a rejected credential (`invalid_client`, 400, 401, 403 or `incorrect` in stderr). `read(bw, creds, server, limit, parent, wants)` returns `(item, field) -> value`. An item must match exactly one login-type item by exact bytes; zero or two is `Unavailable`. Fields: `username`, `password`, `uri` (the first uri) or a custom field by exact name; a missing, empty or NUL-holding value, or a trimmed line under 4 characters (exactly 4 passes), is `Unavailable`. TOTP and attachments are not read. The data dir is created 0700 under `parent` with a random name; after the sequence, success or not, the runner runs `logout`, removes the directory, and fails with `Unavailable` if it still exists. `VaultError` and every log line carry positions and counts only. The session key goes into the log's redactor.
- Constraints: no item or field name in argv, in a file, in a log line or in `Debug`. The three credentials go only to the call that needs them. The fake `bw` in tests is a shell script that records argv and environment.

### T021 State (FR-021 to FR-027, FR-050)
- Goal: restore before the first step, save after a pass.
- Files: `rust-ci-runner/src/state.rs`.
- Acceptance: `restore` picks the highest-id `state-*.bin` asset with API state `uploaded`, downloads it into a file under `--work`, opens it with the state key into memory (at most 10 MB; the ciphertext file is removed at once) and returns the bytes or `None`. No asset: `None`. Any failure (download, authentication, decrypt, size) is `Err`; it never falls back to an older asset. `StateHook` (the D8 `StepHook`) writes the bytes at the declared path inside the checkout with mode 0600 just before the `state = true` step (parent folders created, a symlink on the path refused, nothing written for `None`), and after that step runs `Engine::own_paths`, reads the file (a symlink, an empty or missing file or one over 10 MB is no capture; 10 MB exactly passes), keeps the bytes in memory and removes the file from the checkout, failing the run if the removal fails. `save` takes the captured bytes, a missing capture is no save (Loki line), seals under the state key with a fresh salt, uploads `state-<runid>-<8 hex>.bin`, checks the stored size equals the ciphertext length, re-checks the fence, deletes the new asset and fails if the ticket vanished, then deletes older assets down to the 2 newest. `state_strings(bytes)` returns, for JSON, each string under a key named `value` of 16 or more characters, and for non-JSON the whole content when under 1 MB, for the redactor and the scan.
- Constraints: the state file is a credential: never an artifact path (T014 refuses it), never logged, never visible to a step other than the declaring one, never world-readable, never written to disk in plaintext outside the checkout window. Ciphertext streams in chunks; the plaintext is held in memory (10 MB at most).

### T022 Log, redactor, engine (FR-035, FR-037, FR-044)
- Goal: the public grammar, scan set and vault env export.
- Files: `rust-ci-runner/src/redact.rs`, `rust-ci-runner/src/log.rs`, `rust-ci-runner/src/engine.rs`.
- Acceptance: `Redactor::needles()` returns every form it masks; `Log::scan_set()` returns them. `FAULTS` holds the nine new strings of FR-037 (25 in all) and `public_form` accepts `[runner] waiting for the queue` on stdout. The secret-key faults use `RUNNER_KEYS`. `run_step` exports the env names of `step.vault` exactly as it does `step.secrets` (`-e NAME`, value in the child environment, a missing value is skipped). `StepHook` (`before`, `after`) is called on the step's thread around `docker run` and before the step is marked finished, so a descendant never starts first. New `Engine::own_paths` runs the helper of Files (`chown` and `chmod u+rX` to the workspace owner, never `a+r`), returns `Err` text with no path in a public line, and takes the image of the first step.
- Constraints: `redact.rs` and `log.rs` must not mention the vendor beyond today's allowance. Nothing prints a value, a path or a count publicly.

### T023 Image (FR-044, FR-038)
- Goal: `bw` runs in the image, pinned.
- Files: `rust-ci-runner/Dockerfile`.
- Acceptance: a throwaway stage downloads the zip with `ADD --checksum=sha256:43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3 https://github.com/bitwarden/clients/releases/download/cli-v2026.10.1/bitwarden-cli-2026.10.1-npm-build.zip`, unpacks it with the throwaway `unzip`, and the final image copies the files to `/opt/bw`; `apk add --no-cache nodejs` joins the existing `apk add`; `/usr/local/bin/bw` is an executable `#!/bin/sh` script `exec node /opt/bw/bw.js "$@"`; a `RUN bw --version` runs on each architecture (QEMU in the selftest). The header comment names the version, the sha and why not the standalone binary. The release smoke step runs `bw --version` for amd64 and arm64.
- Constraints: also edit `.github/workflows/rust-ci-runner-release.yml` smoke commands only to add `bw --version`. Record the image size before and after in the PR. If `bw --version` fails under Node 24, stop and report; do not swap to the glibc binary without a plan change.

### T024 Wire `ci` (FR-001, FR-002, FR-006 to FR-009, FR-019 to FR-050)
- Goal: `ci` runs D7.
- Files: `rust-ci-runner/src/ci.rs`, `rust-ci-runner/src/stateful.rs`.
- Acceptance: the order of D7, every fault with its exact line, exit code 1, no container on a refusal. `parse_args` takes `--schedule FILE --entry N` (index into the file; bad file or index is `bad arguments`) or the old triple, never both; the entry's repo, branch and project then flow through the same watch-list check. `identity` returns the repo's `private` flag; a public repo with `[state]` or `[artifacts]` is `state not allowed`. `stateful.rs` runs restore into memory, vault, the pipeline with the `StateHook`, `own_paths` on the artifact paths, artifacts, state save, in that order, adds restored and new state strings to the log, and removes the ticket through `with_turn`. The artifact scan refusal makes the exit code 1 and does not stop the state save. Artifact bundling runs when the pipeline did not pass and a step started (`Success` or `Failed` state), or `when = "always"`. `Seams.production()` points `bw` at `/usr/local/bin/bw` and vault data at `/tmp`; tests inject the fake store and stubs.
- Constraints: the state token, keys and vault credentials reach only `releases.rs`, `seal.rs` and `vault.rs`. `run_checkout` gets closures, not tokens (as 010). No `set_var`.

### T025 `main.rs` (FR-001, FR-033)
- Goal: `due`, `decrypt-artifact`, `plan --schedule`, signal install.
- Files: `rust-ci-runner/src/main.rs`.
- Acceptance: `due --schedule FILE [--now-minute N]` prints the JSON array and exits 0, or prints `schedule file refused` on stderr and exits 1, with no network and no token. `plan --repos F --schedule S --check` validates both and prints every schedule problem. `decrypt-artifact` reads only `FACTORY_CI_ARTIFACT_KEY` from the environment, streams stdin to stdout through `seal::open`, exits non-zero on any error and prints one generic line. `run_ci` calls `signal::install()` first. `USAGE` lists the new commands.
- Constraints: `decrypt-artifact` must refuse a missing or short key and never echo it.

### T026 Version and test groups (FR-038)
- Goal: 0.4.0 and the new process groups.
- Files: `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`, `rust-ci-runner/.factory-ci.toml`.
- Acceptance: both say `0.4.0` for `rust-ci-runner`. The unit step's first command gains `--skip releases_http_tests:: --skip ci_state_tests::`; two commands run `cargo test --locked --lib releases_http_tests::` and `cargo test --locked --lib ci_state_tests::`; the header comment says why.
- Constraints: by hand; touch no other lock entry. If the pipeline hits the process limit, add `-- --test-threads=4` to the server groups.

### T027 Workflow and schedule file (FR-039, FR-040)
- Goal: the schedule trigger and the queue-friendly workflow.
- Files: `.github/workflows/factory-monorepo-ci.yml`, `.github/factory-ci-schedule.toml`, `.github/factory-ci-repos.toml` (header comment only), `.github/workflows/factory-ci-selftest.yml`.
- Acceptance: `on` gains `schedule: - cron: "2,17,32,47 * * * *"`. A `due` job (`if: github.ref == 'refs/heads/main'`, permissions `contents: read` and `packages: read`, timeout 5, no Environment, no Bitwarden token) checks out, logs in to ghcr, and writes `entries=[-1]` for `workflow_dispatch` or the output of the `due` command for `schedule`, plus the 50-day warning. The `ci` job needs `due`, runs `if: github.ref == 'refs/heads/main' && needs.due.outputs.entries != '[]'`, uses `strategy.matrix.entry` from `fromJSON`, `fail-fast: false`, `name: ci ${{ matrix.entry }}`, `timeout-minutes: 190`, and has one `docker run` that mounts the schedule file read-only next to the watch list and passes either the manual triple or `--schedule ... --entry ...` through a bash array. The `concurrency` block is gone and the header says why. The workflow holds no new secret and no new input. The schedule file is valid and empty. The selftest runs `plan --repos ... --schedule ... --check`.
- Constraints: every `${{ }}` stays out of `run:` blocks (values go through `env:`); every `uses:` stays pinned; do not merge before T029.

### T028 Gate and tag (FR-038)
- Goal: a published 0.4.0 image and digest.
- Files: none (tag only).
- Acceptance: T001 to T027 are on `main` (T027 may follow after T029, see order); `make lint` and the selftest are green; the `rust-ci-runner` pipeline passed on `main` and its run id and full SHA equal the SHA to tag; `rust-ci-runner-release.yml` finished, smoke step included; the digest is verified with `docker pull`.
- Constraints: the owner or lead pushes the tag; never move a tag.

### T029 Pin (FR-038)
- Goal: the workflow runs 0.4.0.
- Files: `.github/workflows/factory-monorepo-ci.yml` (the `RUNNER_IMAGE` line only).
- Acceptance: the line reads `ghcr.io/sudtanj/rust-ci-runner:0.4.0@sha256:<64 hex>` and nothing else changes; the commit goes to a branch first so `factory-ci-selftest` (it ignores pushes to `main`) runs, then fast-forward `main`.
- Constraints: separate reviewed commit after T028; no placeholder digest.

### T030 No-secret canary
- Goal: 0.4.0 works with nothing new configured.
- Files: none.
- Acceptance: `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner` ends in `[runner] pipeline passed`, the log shows only grammar lines, and no queue or state line appears. T027's workflow lands and a manual dispatch passes the same way (`ci -1`).
- Constraints: on failure revert the pin before anything else.

### T031 Owner settings and API canary (FR-020, FR-045)
- Goal: the owner's settings exist and the two GitHub behaviours the queue relies on are proven.
- Files: none.
- Acceptance: the owner creates the Bitwarden entries for the six settings (token, two keys, three vault credentials), the dedicated account in an organization limited to one collection, and the fine-grained token for the private repos. Then the checks of quickstart.md section 2 pass on a throwaway private repo: duplicate asset name is refused with 422, asset ids rise, a label `PATCH` works, the `Date` header is present, immutable releases are off, and `bw list items` works with the dedicated account. Nobody prints a value.
- Constraints: values are created and read only by the owner. No agent sees a credential.

### T032 Watch entry and first schedule entry (FR-040)
- Goal: only `sudtanj/factory-monorepo` may keep state and read vault items.
- Files: `.github/factory-ci-repos.toml`, `.github/factory-ci-schedule.toml`.
- Acceptance: the `sudtanj/factory-monorepo` entry gains `state = true` and `vault_items = [...]` (names from the owner); the `sudtanj/infrastructure` entry gains neither; the schedule file gains the entry for the Playwright project only when its pipeline has landed; `plan --check` passes.
- Constraints: names only, no value. The repo is private and its `branches` are explicit (the 010 gate refuses a bare `*`).

### T033 First real run (SC-001, SC-003, SC-009)
- Goal: the Playwright project runs with restored state, a vault login and a readable failure.
- Files: none.
- Acceptance: quickstart.md section 3 passes: first run starts without state and saves it, the second restores it, a forced failure uploads an artifact that decrypts with the documented command, three starts of one state do not overlap and a fourth ends `queue is full`.
- Constraints: only after T029, T031 and T032.

### T034 Docs (FR-041), last
- Goal: both READMEs and the repo description match.
- Files: `README.md`, `rust-ci-runner/README.md`, the repository description through `gh repo edit`.
- Acceptance: they state the six new settings and the project guidance ("holds only the ten keys"), the collection-limited account, the `ci` step list, the ten fault lines, the decrypt command, the agent kit and what an agent must never hold, the vault declaration, the schedule file and cron, the queue numbers, the removed concurrency group, the 60-day rule, the immutable-releases requirement and the risks. No value appears.
- Constraints: stop-slop applies; the README bootstrap order stays true.

## Test plan (qa writes it first)

Unit tests sit in the `#[cfg(test)]` modules named below. `FakeReleases` (in `fake.rs`, test-only) is an in-memory `Releases`: monotonic asset ids, a settable server clock, duplicate-name refusal, per-call fault injection, a "lag" switch that hides new assets from the next N lists, and a call log. Every test that needs a clock sets the server time and never sleeps. Use unique canary values per secret and search every sink (`W::bootstrap()` of `ci_tests.rs` shows the method).

| ID | File | Test | FR / story |
|---|---|---|---|
| T002 | `store_tests.rs` | `runner_keys_has_ten_names_and_boot_keys_is_the_first_four`; `fetch_runner_keys_returns_requested_only`; `..._refuses_a_name_outside_the_list`; `..._key_of_32_chars_passes_31_fails`; `..._equal_state_and_artifact_keys_fail`; `..._missing_or_duplicate_is_count`; `..._errors_and_debug_show_no_value` | FR-020, FR-034 |
| | `secret_names_tests.rs`, `watch_tests.rs` | `every_runner_key_is_refused_by_every_consumer_in_any_case` (table over `RUNNER_KEYS`: `valid_secret_name`, `watch::parse` secrets, `fetch_named`, vault `env`, log fault names); `state_needs_a_numeric_id`; `state_defaults_false`; `vault_items_rules` (empty, 101 chars, control char, duplicate, 21 items refuse the file whole; 100 chars passes) | FR-034, FR-019, FR-043 |
| T003 | `seal_tests.rs` | `round_trip_empty_one_chunk_exact_chunk_and_10mb`; `wrong_kind_key_fails_both_ways`; `equal_keys_still_fail_across_kinds`; `flipped_byte_fails` (header, each chunk, tag); `truncation_at_chunk_boundary_fails`; `reordered_chunks_fail`; `trailing_bytes_fail`; `bad_magic_fails`; `two_seals_of_one_input_differ`; `error_text_holds_no_key_or_plaintext` | FR-027, SC-008 |
| T004 | `schedule_tests.rs` | `due_at_minute_14_15_and_interval_15`; `due_across_hour_day_and_dst_dates`; `ten_entries_pass_eleven_fail`; `interval_boundaries_15_and_10080_pass_14_16_10081_fail`; `unknown_repeated_missing_keys_refuse_whole_file_and_list_every_problem`; `duplicate_entry_is_case_folded`; `empty_file_is_valid`; `entry_index_out_of_range` | FR-001 to FR-005 |
| T005 | `pipeline.rs` tests | `state_table_rules` (name 64 passes 65 fails, bad chars, empty, absolute, `..`, NUL, newline, 20 paths pass 21 fail, `.ci-state` and `.ci-state-2` both pass, artifact equal, containing or under state refused); `exactly_one_step_declares_state`; `state_step_must_be_ordered_with_every_other_step` (parallel sibling refused, ancestors and descendants pass); `artifacts_when_default_failure_and_only_two_values`; `vault_entry_shape` (exact keys, unknown key, 20 per step pass 21 fail, 50 per pipeline pass 51 fail); `vault_env_names_refused_in_any_case` (`BW_`, `BWS_`, `BITWARDENCLI`, `FACTORY_CI`, lower case, runner keys, repeats across `env`, `secrets`, vault; one name mapped to two fields); `env_keys_with_reserved_prefixes_are_refused`; `docker_step_with_vault_is_refused`; `vault_wants_dedupes`; `second_state_table_is_a_toml_error` | FR-017, FR-018, FR-042 |
| T006 | `releases_http_tests.rs` (own process group) | `upload_sends_bearer_and_content_length_and_no_token_in_url`; `duplicate_name_422_is_exists`; `retry_once_on_429_5xx_and_no_answer_but_not_on_4xx`; `download_follows_redirect_without_authorization`; `date_header_is_parsed_and_garbage_is_an_error`; `ensure_release_handles_a_lost_race`; `token_never_in_error_text_or_debug`; `state_token_never_used_for_a_clone` | FR-036 |
| | `util.rs` tests | `http_date_parses_imf_fixdate_and_rejects_other_formats` | FR-013 |
| T007 | `queue_tests.rs` (fake, no server) | `fifo_order_is_asset_id_order`; `second_waits_and_prints_one_line_once`; `turn_passes_only_after_release`; `stale_at_600s_not_at_599s`; `stale_ticket_removed_by_next_waiter_with_loki_line`; `renewal_keeps_a_slow_holder_alive`; `third_ticket_accepted_fourth_refused_without_registering`; `four_simultaneous_registrations_yield_exactly_one_refusal` (4 threads); `two_simultaneous_registrations_get_one_stable_order` (loop 50 times); `late_lower_id_ticket_sends_the_claimer_back` (lag switch); `wait_119_minutes_continues_120_gives_up_and_removes_ticket`; `every_exit_path_removes_the_ticket` (pass, fail, panic, refusal, signal); `own_ticket_gone_before_save_writes_nothing`; `own_ticket_gone_after_upload_deletes_the_new_asset`; `renewal_failure_does_not_stop_the_closure`; `rerun_of_the_same_run_id_replaces_its_ticket`; `server_clock_skew_does_not_matter` (the runner clock is never read); `no_public_text_but_the_fixed_lines`; `store_down_gives_unavailable`; `two_waiters_deleting_one_stale_ticket_is_fine` | FR-006 to FR-016, SC-002 |
| T008 | `bundle_tests.rs` | `folder_is_bundled_whole_missing_path_skipped`; `symlink_and_special_files_skipped`; `long_names_use_pax`; `bundle_cap_passes_at_the_limit_and_refuses_one_byte_over` (cap injected, no 100 MB file written); `retention_keeps_10_deletes_the_oldest_on_the_11th`; `upload_failure_leaves_exit_code_to_the_caller`; `state_path_never_enters_a_bundle` | FR-028 to FR-032 |
| | `scan_tests.rs` | `sentinel_in_plain_base64_and_one_line_of_multiline_is_refused` (every Redactor form, in a plain member, in a zip entry, in a nested zip at depth 2); `depth_3_zip_is_refused`; `clean_bundle_passes`; `4_char_value_found_3_char_ignored`; `value_across_a_chunk_edge_is_found`; `encrypted_entry_zip64_bad_crc_over_10000_entries_over_500mb_are_refused` (limits injected); `gzip_bzip2_xz_zstd_7z_magic_is_refused`; `refusal_reports_a_count_only` | FR-049, SC-008 |
| T009 | `state_tests.rs` | `first_run_has_no_state_and_no_file`; `restore_picks_highest_id_uploaded_asset`; `restore_fails_on_wrong_key_truncated_tampered_and_never_falls_back`; `save_seals_with_fresh_nonce_and_keeps_the_two_newest`; `crash_between_upload_and_delete_leaves_two_and_newest_wins`; `save_refuses_symlink_empty_and_over_10mb` (10 MB passes); `failed_pipeline_never_changes_the_store` (newest asset byte-identical); `save_failure_keeps_previous_state`; `store_holds_only_ciphertext`; `fence_checked_before_and_after_upload`; `json_value_strings_16_chars_added_15_not`; `non_json_under_1mb_added_whole`; `state_file_exists_only_during_the_declaring_step` (hook order: absent before, mode 0600 inside, gone after); `no_other_step_sees_the_file`; `file_is_never_world_readable`; `removal_failure_stops_the_run` | FR-021 to FR-027, FR-050, SC-005 |
| T010 | `vault_tests.rs` (fake `bw` script) | `sequence_is_login_unlock_list_logout`; `each_call_gets_only_its_own_variables` (login: id and secret; unlock: password; list: session); `no_item_or_field_name_in_argv`; `reads_each_declared_field_once`; `item_matching_zero_or_two_logins_is_unavailable`; `case_difference_does_not_match`; `missing_empty_nul_and_3_char_line_are_unavailable_4_passes`; `data_dir_is_gone_and_0700_before_return`; `failed_logout_or_removal_is_unavailable`; `retry_once_on_timeout_not_on_rejected_credential`; `errors_and_logs_hold_positions_and_counts_only`; `unicode_space_and_equals_in_item_names_match_by_bytes` | FR-044 to FR-047, SC-007 |
| T011 | `ci_state_tests.rs` (own process group) | `counter_pipeline_sees_0_1_2_across_three_runs` (store holds only ciphertext); `three_runs_of_one_state_never_overlap_and_a_fourth_is_full`; `scheduled_entry_runs_through_the_watch_checks_and_prints_no_name` (`--schedule`, `--entry`); `entry_outside_the_watch_list_exits_1_with_the_watch_line`; `state_not_allowed_for_public_repo_and_entry_without_state_true` (no ticket, no container); `ref_gate_refuses_state_vault_and_artifacts_off_main`; `missing_state_token_or_key_ends_before_a_ticket`; `restore_failure_starts_no_container_and_releases_the_ticket`; `vault_item_not_allowed_never_unlocks`; `vault_unavailable_starts_no_container`; `sibling_step_holds_no_vault_value_and_no_state_file`; `step_dumping_env_and_files_sees_only_its_declared_vars_and_no_bw_data`; `failing_step_uploads_one_artifact_and_loki_names_it`; `passing_run_with_when_failure_uploads_nothing`; `scan_refusal_fails_a_passing_run_and_still_saves_state`; `artifact_upload_failure_keeps_the_exit_code`; `hostile_fixture_leaks_nothing` (canaries for all ten settings, state strings, vault values; search stdout, stderr, every child argv and child variable set, every Loki body, every asset); `every_stderr_line_is_in_the_fixed_list` (25 entries); `stdout_grammar_adds_exactly_waiting_for_the_queue`; `sigterm_while_waiting_and_while_running_removes_the_ticket` (injected exit); `no_state_pipeline_registers_nothing`; `existing_pipelines_run_unchanged`; `bw_and_bws_call_counts` | all stories, FR-001 to FR-050 |
| T012 | `.github/scripts/test-factory-monorepo-ci.sh` | updated checks: `schedule` trigger with the cron; `due` and `ci` jobs; one `docker run` still; no `concurrency` block; `timeout-minutes: 190`; matrix from `fromJSON`; schedule file mounted `:ro` (4 mounts); `--schedule` and `--entry` in the args; no `${{` in `run:`; still only `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN` as secrets; the `due` job has no Environment and no Bitwarden token; main guard on both jobs; header explains the missing concurrency group | FR-039 |
| | `tests/no_vendor.rs` | allow `vault.rs`; keep the rest | Principle IV |

Selftest additions: `plan --repos ... --schedule ... --check` (a step in `factory-ci-selftest.yml`), and the image build with `RUN bw --version` on both architectures.

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| args | `--schedule` with `--repo`, bad index | `parse_args` | `bad arguments` | `entry_index_out_of_range` |
| due | bad schedule file | `schedule::parse` | `schedule file refused`, nothing starts | `unknown_repeated_missing_keys...` |
| state allowed | public repo, entry lacks `state = true` | identity `private`, entry flag | `state not allowed`, no ticket | `state_not_allowed_...` |
| ref gate | state, vault or artifacts off an allowed ref | `secrets_allowed_on` | `repository or ref not in the watch list` | `ref_gate_refuses_...` |
| runner keys | missing, duplicate, short, equal | `fetch_runner_keys` | `state store not configured`, before a ticket | T002 |
| register | queue full | list, position after registering | `queue is full`, ticket withdrawn | `third_ticket_accepted_...` |
| register | store down, token expired | `Unavailable` | `state store not configured`, cause to Loki (O4) | `store_down_gives_unavailable` |
| wait | holder dies | stale rule at 600 s | next waiter removes the ticket | `stale_ticket_removed...` |
| wait | 120 minutes | `wait_decision` | `queue wait timed out`, ticket removed | `wait_119_...` |
| wait | own ticket vanishes | list | re-register after a 60 s grace | `late_lower_id...` |
| claim | late lower id appears | settle re-list | back to waiting | `late_lower_id...` |
| restore | download, auth, decrypt fail | `open` error | `state restore failed`, no container, ticket released | `restore_fails_on_wrong_key...` |
| vault | `bw` hangs, rejected, 0 or 2 matches, bad value | `vault::read` | `vault unavailable`, no container, dir removed | T010 |
| vault | logout or dir removal fails | check after cleanup | `vault unavailable`, no container | `failed_logout...` |
| state step | file removal fails after the step | `after` hook | `runner fault`, no later step | `removal_failure_stops_the_run` |
| artifacts | secret found, unscannable file | `scan` | `artifact upload refused`, exit 1, no upload | T008 |
| artifacts | upload fails | `HttpReleases` | Loki line, exit code unchanged | `artifact_upload_failure...` |
| save | upload, size or fence fails | `state::save` | `state save failed`, new asset deleted, old state kept | `fence_checked_...` |
| exit | SIGINT, SIGTERM, panic | watcher thread, drop guard | ticket deleted, exit 1 | `every_exit_path...` |
| exit | ticket delete fails | next waiter | stale rule frees it in 10 minutes | `stale_ticket_removed...` |
| image | `bw` does not run on Node 24 or an architecture | `RUN bw --version` in the build | build fails; no release | T023 |
| rollout | pin bumped before the digest exists | `docker pull` fails | run fails at pull; the 0.3.0 pin works until then | order of T028, T029 |
| rollout | `[state]` pipeline before keys and entry exist | fixed lines | no container | quickstart |
| schedule | 60 days without activity | GitHub | scheduled runs stop | `::warning::` at 50 days, README |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. Two runs of one state overlap. Causes: a lagging list hides an older ticket, or a stale break of a live holder. Prevented: settle re-list before the claim, stale break only after 600 s of server time, fence before and after the upload, deleting the new asset when the ticket vanished, race tests with a lag switch, and the 10-minute figure documented. R15 stays open.
2. A secret lands in an artifact, a log or the store. Prevented: the scan uses every Redactor form on the exact tar that is encrypted, refuses what it cannot open (encrypted zip, depth, compressed magic), state never enters a bundle (path rule, and the file leaves the checkout right after its step), two keys with domain separation, and the hostile fixture searches every sink and every asset.
3. The vault credentials or session leak, or the wipe does not happen. Prevented: each `bw` call gets only its own variables, no name in argv, the data dir is on tmpfs, removed and checked before any step, the run fails if it cannot confirm, and the step test dumps its variables and files.
4. The release breaks: `Cargo.lock` disagrees with `Cargo.toml` (two hand-written entries), `bw` does not run on Alpine, or the workflow change meets an old image. Prevented: checksums copied from the crates.io index, `--locked` in the selftest build, `RUN bw --version` in both architecture builds, and the rollout order (pin first, workflow after).
5. The queue starves or burns the API: renewals count against the secondary limit of 80 content-generating requests per minute and 500 per hour, or a stuck ticket blocks a state. Prevented: 60 s renewals cost about 60 requests per hour per live ticket (three tickets, 180 per hour), one retry only, the stale rule, `queue is full` at three, a Loki line for each break. Ten busy states at once would pass 500 per hour (O5).

## Invariants (qa turns these into property tests)

- I1: for any interleaving of registrations, removals and stale breaks, at most one run holds the turn, and holders run in ascending asset id.
- I2: a ticket is gone after the run ends, whatever the exit path, or the stale rule frees it within 600 s.
- I3: a run whose ticket is gone before the save writes no asset; gone after the upload, it leaves no new asset.
- I4: the stored newest state is unchanged by any run whose pipeline did not pass.
- I5: `open(kind, key, seal(kind, key, x)) == x`; for any other key or kind, any one-byte change or truncation, `open` fails and releases no plaintext beyond the last verified chunk.
- I6: no byte of stdout, stderr, any child argv or child variable, any Loki body or any asset holds a known secret form, except inside ciphertext.
- I7: stdout lines belong to the 008 grammar plus `waiting for the queue`; stderr lines belong to `FAULTS` (25).
- I8: `due(entries, m)` is a pure function of its inputs; an entry with interval k is due in exactly one quarter hour of every k minutes.
- I9: every name in `RUNNER_KEYS` is refused by every consumer, in any letter case.
- I10: the state file exists in the checkout only between `before` and `after` of the one state step, and no other step starts or runs in that window.
- I11: a vault value reaches only the steps that declared it; after the vault read no `bw` process, session or data dir exists.
- I12: a bundle that holds a known secret form is never encrypted or uploaded.
- I13: the state key opens no artifact and the artifact key opens no state asset.
- I14: no file written by the runner for state or artifacts is world-readable.

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: handled. Empty and maximum: 0 and 10 schedule entries, 64-char names, 20 paths, 20 vault fields, 10 MB and 100 MB bounds, each with an n and n+1 test (T004, T005, T008, T009). Duplicates: duplicate schedule entry, same item twice, same run id twice (`rerun_of_the_same_run_id_replaces_its_ticket`). Unicode, NUL, newline: path and name rules, item names by bytes (T005, T010). Malformed: tampered or truncated asset (T003, T009); a `bw` list that is not JSON is `vault unavailable`.
- State: handled. First run (`first_run_has_no_state_and_no_file`). Older format: none exists; the magic `FCI1` leaves room for a version. Crash mid-operation: stale rule, two assets after a crash and the newest wins, a partial upload has API state `starter` and is ignored. Partial failure: table above, each row with a test. Retry: ticket replace, unique asset names, idempotent delete (404 is gone).
- Time and concurrency: handled. Two callers: races in T007. Timeouts: every GitHub and `bw` call (T006, T010). Clock: age from the server `Date`, never the runner clock; schedule in UTC epoch minutes (`due_across_hour_day_and_dst_dates`). Expiry at the boundary: 599 and 600 s, 119 and 120 minutes, minute 14 and 15. Ordering: stable total order by asset id; request order is approximate before registration (R8).
- Resources: handled with limits. Disk: bundles and ciphertext go under `--work`, not the 64 MB `/tmp`; `bw` data is tiny. Large input: chunked crypto, streamed scan, caps of 10 MB, 100 MB, 500 MB, 10,000 entries. Slow or garbage dependency: one retry, then a fixed fault. Growth: 3 tickets, 2 state assets, 10 bundles, 10 schedule entries. Memory: 64 KiB chunks, a 64 KiB scan buffer, nested zips at most 64 MB, state at most 10 MB in memory.
- Permissions and trust: handled. The state token reaches `releases.rs` only; keys reach `seal.rs`; vault credentials reach `vault.rs`, each call with its own variables. The pipeline author cannot name a release, a repo, a setting or a vault credential, cannot widen the entry (a reviewed file on `main`), and cannot see the state file outside its one step. The agent holds the artifact key, a Loki token and a read token, nothing else (T034). Client values: pipeline names and paths are validated; the schedule entry comes from the reviewed file; dispatch inputs never select an entry.
- UI: N/A. No UI; the surface is the fixed stdout and stderr lines (FR-037, I7).

## Rollout and rollback

Rollout (the lead's order):

1. T001 to T026 land on `main` through a pull request linked to this folder; the selftest and `make lint` pass; the `rust-ci-runner` pipeline passes on `main`.
2. T028: tag `rust-ci-runner-v0.4.0` on that commit; read the digest.
3. T029: pin commit on a branch (the selftest runs there), then fast-forward `main`.
4. T027's workflow change lands after the pin (the old image does not know `--schedule`).
5. T030: no-secret canary and a manual dispatch.
6. T031: owner settings, then the API canary on a throwaway repo.
7. T032: watch entry and first schedule entry. T033: the factory-monorepo pipeline.
8. T034: docs, last.

Rollback, in this order:

1. Revert T027 (the workflow returns to the manual trigger and the concurrency block), then revert the pin (0.3.0). Neither needs a data change.
2. Remove `state = true` and `vault_items` from the entry and the entries from the schedule file.
3. State, queue and artifact releases stay in the private repo. They are encrypted or empty; the owner may delete them.
4. The release workflow moved `:latest` to 0.4.0. Moving it back needs owner approval (`docker buildx imagetools create --tag ghcr.io/sudtanj/rust-ci-runner:latest ghcr.io/sudtanj/rust-ci-runner@<0.3.0 digest>`). Nothing here pulls `latest`.
5. Fixes ship as 0.4.1. Never move or delete the 0.4.0 tag.
6. Rotate the six settings in Bitwarden if a leak is suspected, and revoke the state token.

## Devops assessment

None was handed over for 011. The notes below are the EM's own, folded into the steps. Items marked 010 repeat a finding on 010.

| Note | Handling | Where |
|---|---|---|
| The image gains Node (about +70 MB estimated) and a pinned bundle | Checksum pin, smoke `bw --version`, measured size in the PR | D3, T023 |
| `/tmp` is a 64 MB tmpfs and the root is read-only | Bundles and ciphertext under `--work`; the `bw` data dir on `/tmp` | Technical Context, T019, T020 |
| New workflow jobs and a matrix | One `docker run` stays; the `due` job has no secret and no Environment; checks in the shell test | T027, T012 |
| Pin and workflow order (010 S6) | Pin first on a branch (the selftest ignores `main`), workflow after | Rollout |
| Process limit of the step container (010 S3) | Server tests in their own process groups, race tests use at most 6 joined threads | D6, T026 |
| API rate | Renewal cost computed; the 500 per hour secondary limit documented | Pre-mortem 5, O5 |
| Immutable releases | Must be off on the private repo; the API canary checks it | T031 |
| Schedule delay and the 60-day switch-off | Cron offset, 13 minutes of slack, a warning at 50 days | D5 |
| Rollback of `:latest` (010 S4) | Owner approval step | Rollback |
| One list of runner keys (010 N6) | Ten names, five consumers | D4 |

## Security assessment

None was handed over for 011. The notes below are the EM's own, plus the 002 threat-model items the coordinator sent.

| Note | Handling | Where |
|---|---|---|
| State reaches only the declaring step; install, lint and test run third-party code as root | `state = true`, ordering rule, hook writes before and removes after, mode 0600, `chown` by the step with a runner backstop, no world-readable copy | D8, T005, T014, T021, T022 |
| Vault and state names collide with runner names | Case-insensitive refusal of `BW_`, `BWS_`, `BITWARDENCLI`, `FACTORY_CI` and `RUNNER_KEYS` in `env`, `secrets` and vault `env` | T014 |
| The ref gate covers more than step secrets | Vault, state and artifacts pass `secrets_allowed_on` | D7 step 5, T024 |
| Vault values and credentials | Per-call variables, no names in argv, tmpfs data dir, wipe before step one, positions and counts only in logs | D3, T020 |
| One list of runner keys | Ten names, five consumers, table test | D4, T002 |
| Artifacts hold secrets | Scan every Redactor form on the encrypted tar, zip to depth 2, refuse compressed formats and unreadable files, separate artifact key | T019 |
| Key hygiene | HKDF per file, domain separation, 32 characters or more, keys differ | D2, T013 |
| Public leak surface | Ten fixed lines, queue details to Loki only | T022, T011 |
| The agent kit | Artifact key, Loki read token, read-only GitHub token, documented | T034 |
| A writer with the state token can roll state back | Accepted: the token is a trust root (R1); the owner limits it to private repos with `state = true` | spec R1 |

## Open risks

- R15 (new): GitHub list-after-create may lag. The settle re-list narrows it and cannot close it. If the quickstart canary shows lag, raise the settle time or poll twice.
- O1: the plan relies on three GitHub behaviours not proven here: a duplicate asset name is a 422, `PATCH` accepts `label`, and asset ids rise. The first is documented; the other two are checked in T031 before the first real run. If `label` updates fail, the fallback is a plan change (observer-side monotonic age).
- O2: `bw list items` returns every item the identity sees. A personal vault would put every item in memory during the read. The collection-limited account (R12) is the control; the plan does not enforce it.
- O3: HKDF assumes machine-made keys. A human-chosen 32-character key is weaker. The README says to use `openssl rand -base64 33`. PBKDF2 is the alternative if the owner prefers it.
- O4: a store outage at registration has no fixed line of its own in FR-037. The plan prints `state store not configured` for it and logs the cause; the owner may want a tenth line, which changes the spec.
- O5: the secondary limit of 500 content-generating requests per hour caps sustained live tickets at about eight. Ten busy states would hit it.
- O6: scanning `FACTORY_CI_LOG_USER` (numeric) and `FACTORY_CI_LOG_URL` can refuse a bundle that holds those digits by chance (R11). If the first real failure shows it, drop identifiers from the scan set by a spec change.
- O7: a state step that never writes the file leaves nothing captured, so the old state stays. A step that replaces the file by rename is fine, because the hook reads the path after the step.
- O8: `nodejs` comes from Alpine 3.23 and is pinned only by the base image digest and the index. A later index could ship a newer major; the smoke `bw --version` catches a break at build time.
- O9: the agent needs the runner binary to decrypt; the public release assets give it and the doc says so. An agent with no `gh` is outside this plan.
- O10: the 002 pipeline must order its steps so the state step has no parallel sibling (install, lint, test before it or after it). The refusal is a fixed line and a Loki detail.

## Project Structure

### Documentation (this feature)

```text
specs/011-ci-state-queue/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── config-files.md
│   ├── release-store.md
│   └── asset-format.md
├── checklists/requirements.md
└── tasks.md
```

### Source Code (repository root)

```text
rust-ci-runner/
├── Dockerfile
├── Cargo.toml, Cargo.lock
├── .factory-ci.toml
├── src/  seal, schedule, releases, queue, signal, state, bundle, scan, vault, stateful (new);
│         store, watch, pipeline, redact, log, engine, ci, main, util, lib, fake (changed);
│         one *_tests.rs per new module
└── tests/no_vendor.rs
.github/
├── factory-ci-schedule.toml (new)
├── factory-ci-repos.toml
├── scripts/test-factory-monorepo-ci.sh
└── workflows/factory-monorepo-ci.yml, factory-ci-selftest.yml, rust-ci-runner-release.yml
```

**Structure Decision**: the existing layout. New logic in new single-purpose modules under `rust-ci-runner/src/`, no new crate, no new directory.

## Complexity Tracking

| Addition | Why needed | Simpler alternative rejected because |
|---|---|---|
| `miniz_oxide` (+ `adler2`), two new lock entries | FR-049 requires scanning zip entries (Playwright `trace.zip`); inflate is the one thing std lacks | Writing inflate by hand is security code to maintain; shelling out to `unzip` adds a binary, text parsing of hostile names and no depth control |
| `libc` direct dependency (already locked) | FR-009 needs SIGINT and SIGTERM handling; std has none | A stale rule alone leaves a cancelled run's ticket for 10 minutes; the spec demands removal on cancel |
| Node and the `bw` bundle in the image (about +70 MB) | The owner chose Bitwarden Password Manager for vault logins; only the CLI reads it | The standalone binary is 143 MB, glibc-linked and needs a shim; a direct API client would reimplement the vault crypto |
| A hand-written tar writer and reader | The bundle needs one archive that the scanner and the decryptor both read | A `tar` crate adds a dependency for about 80 lines; zip needs a CRC table and a compressor |
| `ring` and `memchr` as direct dependencies (already locked) | AEAD and HKDF; fast substring search | New crates for the same primitives |
