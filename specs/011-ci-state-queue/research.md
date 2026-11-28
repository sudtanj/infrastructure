# Research: ci-state-queue

Each entry gives the decision, the evidence and what was rejected. Evidence was read on 2026-11-28 from the code in this repo, from `gh api` reads of public GitHub data, and from GitHub's own documentation pages. Nothing was built or run.

## R-A The atomic primitive for the queue

**Decision**: release assets in a per-state release `ci-queue-<state>`. Asset name `t-<runid>`, label `<runid>.<server epoch>`, order by asset id.

**Evidence**:
- GitHub's "Upload a release asset" page (docs.github.com/en/rest/releases/assets) says: "If you upload an asset with the same filename as another uploaded asset, you'll receive an error and must delete the old file before you can re-upload the new asset." That is the atomic create.
- The same page lists `label` as a parameter of the upload ("An alternate short description of the asset") and the "Update a release asset" endpoint takes `label` and `state` ("Users with push access to the repository can edit a release asset"). Contents write is enough; no issues permission is needed.
- Every REST answer carries a `Date` header, so the runner can read server time with no extra call. The runner clock is never used (FR-013).
- Assets carry integer ids. A single integer order seen identically by all readers gives FR-010. Strict real-time order for creates that overlap is whatever the store decides, which is what FR-010 asks for.
- Secondary rate limit, from "Rate limits for the REST API": no more than 80 content-generating requests per minute and 500 per hour. Renewal at 60 s costs 60 per hour per live ticket.
- Immutable releases (docs "Immutable releases"): assets cannot be modified or deleted once a release is published with the setting on. The queue and the state need to delete assets, so the setting must be off on the private repo.

**Race cases walked**: two creates of different names (both succeed, ids order them); two creates of one name (one 422); register while the list lags (settle re-list sends the claimer back); a fourth run registering while three exist (post-register position check withdraws it); a stale break of a live holder (fence before and after the upload); two waiters deleting one stale ticket (404 is fine); a rerun of one run id (delete then upload replaces its own ticket); a crash between upload and delete of state (two assets, newest wins).

**Rejected**: git refs (no timestamp or payload; a reused name hides an old holder), issue comments (needs the issues permission and notifies), a queue file through the Contents API (a commit per renewal, 45 or more per run), releases as tickets (a tag per ticket, one-second `created_at`), the workflow concurrency group (cancels the older pending run).

**Open**: that `PATCH` accepts `label` and that ids rise is checked in quickstart section 2 before the first real run.

## R-B The cipher, the key derivation and the file format

**Decision**: `ring` 0.17.14, ChaCha20-Poly1305, HKDF-SHA256 with a 16-byte random salt per file, 64 KiB chunks with a STREAM nonce.

**Evidence**:
- `rust-ci-runner/Cargo.lock` lists `ring` 0.17.14 as a package (dependency of `rustls`, which `ureq` uses). A direct dependency adds one line to the root package's dependency list and no new package. The release workflow already builds `ring` with `musl-gcc` for both architectures (`rust-ci-runner-release.yml`, comment on `ring`), so the C part is proven in the toolchain.
- `ring::aead` has no streaming interface, and the spec says state and bundle files are read in streams (Edge Cases, Resources). Chunking at 64 KiB bounds memory at one chunk for a 100 MB bundle.
- The keys are machine-made (`openssl rand -base64 33`, 44 characters) and FR-020 requires 32 or more, so HKDF is enough. A slow KDF would only slow the tests.
- `info` strings `factory-ci/v1/state` and `factory-ci/v1/artifact` mean one key never opens the other kind's file, even when the owner sets the same value twice. FR-020 refuses equal values too.
- The `age` crate, `rage`, `openssl enc`, and an extra AEAD crate were looked at and rejected (no authenticated `openssl enc`; age needs a terminal for a passphrase and many crates).
- For the agent: the release workflow publishes `rust-ci-runner-x86_64-linux` and `-aarch64-linux` as assets of this public repo ("release assets" step), so `decrypt-artifact` needs `gh` and `tar` only.

## R-C `bw` in the image

**Decision**: the npm-build bundle of release `cli-v2026.10.1` of `bitwarden/clients`, run by Alpine's `nodejs`.

**Evidence** (from `gh api repos/bitwarden/clients/releases/tags/cli-v2026.10.1` and downloads checked with `sha256sum`):
- `bitwarden-cli-2026.10.1-npm-build.zip`: 5,129,489 bytes, sha256 `43401c3e27cb009a28035dc8f9bea43ae78880e0549490762542f55af0e6b2e3` (matches the download). Contents: `bw.js`, `344.js`, `685.js`, a wasm module, `locales/`. 19 MB unpacked, no native module. The bundle embeds `engines` `node >=22`.
- `bw-oss-linux-2026.10.1.zip`: 45,802,496 bytes, sha256 `53384064abe7555039281bc2e6fe51846368ba60db9b7ba89de183409d7f6738`. `unzip -l` shows one 143,321,507-byte file. `file` and `readelf` show a dynamically linked x86-64 ELF with interpreter `/lib64/ld-linux-x86-64.so.2` and `NEEDED` `libstdc++.so.6`, `libgcc_s.so.1`, `libc.so.6`, `libm`, `libdl`, `libpthread`; symbol versions up to `GLIBC_2.28`. Alpine uses musl, so it needs a glibc shim.
- Alpine 3.23 `main` index (`APKINDEX`): `nodejs` 24.22.3-r0, installed size 52,297,024 bytes; libraries `icu-libs` 4.8 MB, `libstdc++` 2.8 MB, `ada-libs` 0.7 MB, `brotli-libs` 1.0 MB, `c-ares` 0.2 MB, `nghttp2-libs`, `simdjson` 0.2 MB, `sqlite-libs` 1.6 MB, `zstd-libs` 0.7 MB. Node 24 satisfies `>=22`.
- The Dockerfile already pins `bws` with `ADD --checksum` and uses a throwaway `unzip` stage; the same pattern fits. The bundle is architecture-independent, so no per-architecture stage.
- Size: +70 MB estimated for Node and libraries, against 143 MB for the standalone binary. The PR records the real number from the selftest build.

**Wipe**: login with `BW_CLIENTID` and `BW_CLIENTSECRET` (`bw login --apikey`), `bw unlock --passwordenv BW_PASSWORD --raw`, `BW_SESSION` for `bw list items`, then `bw logout`, remove `BITWARDENCLI_APPDATA_DIR`, verify it is gone. `bw list items` plus an exact-name filter in process keeps item names out of argv. Command names and flags are from Bitwarden's CLI help as I know it; the fake `bw` of T010 fixes the contract and quickstart section 2 proves it with the real CLI.

**Rejected**: the glibc standalone binary (size, shim), `npm install -g @bitwarden/cli` (unpinned transitive dependencies at build time), the Bitwarden web API directly (reimplements vault decryption).

## R-D One runner-only key list

**Decision**: `store::RUNNER_KEYS` with ten names; `BOOT_KEYS` is the first four.

**Evidence**: the 010 plan (`origin/010-ci-step-secrets`, "Single list of runner keys (devops N6)") puts `RUNNER_KEYS: [&str; 4]` in `store.rs` and makes `valid_secret_name`, `fetch_named` and the `log.rs` fault names use it. 011 grows the same constant to ten, so the five consumers close together. `pipeline.rs` `valid_secret_name` already reserves `FACTORY_CI`, `BWS_`; 010 adds `BW_` and `BITWARDEN`.

## R-E Schedule

**Evidence** (docs.github.com, "Events that trigger workflows", `schedule`): the shortest interval is once every 5 minutes; scheduled workflows run on the latest commit of the default branch; the event "can be delayed during periods of high loads ... High load times include the start of every hour. If the load is sufficiently high enough, some queued jobs may be dropped"; "In a public repository, scheduled workflows are automatically disabled when no repository activity has occurred in 60 days."

**Decision**: cron `2,17,32,47 * * * *`, so each tick sits 2 minutes into its quarter hour with 13 minutes of slack. `due` is a pure function of the epoch minute. The 60-day risk gets a `::warning::` in the `due` job at 50 days since the last commit on `main`, plus a README instruction. No third-party keepalive action, no write permission.

## R-F Where the tests run

**Evidence**: `rust-ci-runner/.factory-ci.toml` runs `cargo test --locked --lib` in groups: everything but `ship_tests::`, `job_tests::`, `ci_tests::`, then each of those alone, because mock servers hold threads and a step container has `--pids-limit 512` (`engine.rs`). 010 adds a `ci_tests::step_secrets::` split. `ci_tests.rs` `Hub::drop` calls `server.unblock()`. `--skip ci_tests::` does not match `ci_state_tests::`, so new server modules need their own `--skip` and process groups. The in-memory `FakeReleases` keeps queue, state and bundle tests off HTTP, so race tests use at most 6 joined threads and run in the first group.

## R-G Findings that changed the design late

- The 002 threat model (coordinator, 2026-11-28): install, lint and test run third-party code as root and must never see the state file. This produced D8 (one declaring step, ordering rule, hook, mode 0600, chown by the step with a runner backstop) and replaced the earlier plan of a pipeline-wide restore and a world-readable `chmod`.
- The 010 plan on the remote branch moved on after the first read (ref gate through `secrets_allowed_on`, `StepSecrets`, the `fetch` closure, `Redactor::add` with more forms). The 011 plan follows the later revision.
