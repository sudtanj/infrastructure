# Implementation Plan: Step secrets for the ci command

**Branch**: `010-ci-step-secrets` | **Date**: 2026-11-28 (third revision: devops assessment, security threat model, critic review) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/010-ci-step-secrets/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` and `setup-tasks.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before the plan and `/speckit-analyze` after `tasks.md`. The spec carries a clarification session and was amended three times (see its Clarifications). `/speckit-analyze` is still to run before `/speckit-implement`.

Order against feature 011: **010 merges before 011.** Both edit `rust-ci-runner/Cargo.toml` (0.3.0 here, 0.4.0 there), the `FAULTS` list in `log.rs`, `ci.rs` and both READMEs. 011 rebases onto 010's `Args`, `Held`, `Redactor::add`/`forms` and `Log::add_secrets`; it must not assume a fixed `FAULTS` length.

## Summary

`ci` gives a step the Bitwarden secrets it lists, when the run's watch entry allows the name and the ref may receive secrets. `job()` already knows the entry's allowlist before it makes its one `bws secret list` call, so `store::fetch_with` keeps the raw rows of the allowlisted keys (at most 20) next to the four runner values. After the fetch, `run_checkout` checks, in order: pipeline parse, docker rules, allowlist, ref gate. Then `Held::take` validates the requested values, one batch goes to the log's redactor, and the map goes to `run_pipeline`. Any failure ends the run before the first container with a fixed public line. The engine already passes values as `-e NAME` with the value in the child environment, and the name rule already refuses the four runner settings (prefix `FACTORY_CI`). New code: the keyed lookup in `store.rs`, pipeline and watch helpers, a stronger redactor, a public-line fix, a pump boundary fix and the `ci.rs` wiring. There is no second `bws` call.

## Technical Context

**Language/Version**: Rust 2021, toolchain from `rust-ci-runner/rust-toolchain.toml` (pipeline image `rust:1.90`)

**Primary Dependencies**: no new crate. `serde_json`, `toml`, `ureq`, std only. Percent-encoding and hex are a few lines; JSON escape uses `serde_json`.

**Storage**: none. No state between runs.

**Testing**: `cargo test --locked --lib` (unit, and `ci_tests.rs` driving `ci::run` with stub `bws`, `docker`, `git` and a `tiny_http` Loki), `cargo test --locked --test ...`. The `rust-ci-runner` pipeline runs them on `main` (`rust-ci-runner/.factory-ci.toml`, one step container with `--pids-limit 512`). `factory-ci-selftest.yml` runs on GitHub-hosted runners for pushes to every branch except `main` and for pull requests; its `list` job will run `cargo test --locked --lib` in full (T013), then `--test '*'`, `make lint` and `plan --check`.

**Target Platform**: the `ghcr.io/sudtanj/rust-ci-runner` image, linux/amd64 and arm64, started by `factory-monorepo-ci.yml`.

**Project Type**: CLI (one binary, `ci` subcommand).

**Performance Goals**: no new external call. A run makes exactly one `bws` call, as today.

**Constraints**: values only in the environment of the engine child, never argv; public stdout and stderr stay a closed grammar; names and counts to Loki only; this VM has 1 GB, so nobody builds or tests locally (git and file reads only).

**Scale/Scope**: at most 20 allowlisted names per entry (`watch.rs` `MAX_LIST`), at most 100 steps, values at most 32 KiB.

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Runner and watch file stay here. |
| II Secrets source | Pass. Pipeline secrets come from Bitwarden through the existing token. The workflow gains no secret and no input. No value is committed; docs name keys only. |
| III Public logs | Pass by design: two fixed lines, no name or count, redactor filled before any step, public lines built from fixed grammar. Hostile tests search every output for canaries. |
| IV Simplicity | Pass with the items in Complexity Tracking. No new crate, no new watch key, no new module, no second `bws` call. |
| V Tested | Pass with one justified exception, below. |
| VI Spec-driven | Pass for plan and tasks; `/speckit-analyze` runs next. |

Re-check after design: unchanged.

### Principle V exception (justified, recorded here as Governance requires)

Principle V requires every acceptance criterion to map to an automated test. These do not, and cannot, run as an automated test inside this repository:

| Item | Why no automated test | What replaces it |
|---|---|---|
| FR-014 (tag, release) | The tag is made after the code lands; the release workflow is the check (it compares tag to `Cargo.toml`, smoke-tests the image) | `tests/repo_files.rs::cargo_toml_and_lock_versions_match`; the release workflow's own checks |
| FR-015 (pin) | A digest exists only after the release | `.github/scripts/test-factory-monorepo-ci.sh` (pin shape), selftest job `shell` on the pin branch; T021 canary pulls the image |
| FR-016 (header comment, names) | Prose and owner choice | `tests/repo_files.rs` (no name in two entries, none on `sudtanj/infrastructure`); `plan --check` |
| FR-017 (docs) | Prose | `tests/repo_files.rs::readme_has_no_stale_secrets_sentence` (README no longer holds "Steps get no pipeline secrets here"), landed with the docs task |
| FR-019, SC-001, SC-004 | Need the real store, a real run and a week of use | T022 to T026 (manual runs with recorded results) |

The exception is limited to these items. Every other FR has a unit or `ci::run` test in the Test Plan.

## Architecture decision

Chosen: one `bws secret list` call, as today, with the allowlist passed in. `job()` finds the watch entry before it calls the store (`ci.rs`: `listed` then `store::fetch`), so `store::fetch_with(bws, token, server, limit, allowed)` parses the list once, in `store.rs` only, and returns:

- `Secrets`: the four runner values, validated as today (`parse_list` rules unchanged);
- `Held`: for each allowlisted name, the values of every row whose key equals it (zero, one or more), nothing else. All other rows and the output buffer are dropped inside `fetch_with`.

`Held::take(names)` validates lazily, only the names a step requests: a missing, duplicated or unusable allowlisted key never breaks a run that does not request it. Retention only: dropped `String`s are not zeroed (no zeroize crate is added); the claim is that nothing but the allowlisted rows outlives `fetch_with`. The whole list still passes through memory and the `bws` pipe, as it does today. `run_capped` caps stdout at 16 MiB; a bigger list is cut, fails to parse and ends as the existing `secret store unreachable`.

Why not a second call (the earlier plan): it added a 60 s call with its own retry, a token mix-up risk (`steps` receives `sec.token`, the GitHub clone token, while the lookup needs the Bitwarden token that only `job()` holds), per-call stub work and a closure to thread through `run_checkout`. The one call removes all of it. The cost is that values of allowlisted keys sit in memory from the first call on, before the pipeline is read; the spec states that (Assumptions, FR-002).

Alternatives rejected:
- A second filtered call: see above. `bws secret list` has no filter either way.
- A second `Log` with a bigger Redactor: resets the 10 MB limit and cut flag. `Log` holds its redactor in an `RwLock` and gets `add_secrets` instead.
- Per-key `bws secret get`: needs ids, which only the list gives.
- Resolve in the workflow with `factory-ci-secrets.sh`: the constitution keeps the workflow to the one access token.
- Repo-own keys `owner/name<SEP>NAME`: out of scope.

### Lookup interface v1 (for 011)

Names in, name-to-value map out: `Held::take(&self, names: &[String]) -> Result<StepSecrets, NamedError>`. `NamedError` is keyed by position in `names` (`Count { index, found }`, `Unusable { index }`) and carries no name, item or field. `ci.rs` turns an index into the requested name for the Loki detail (010 may log secret names; 011 must not log vault item or field names and can reuse the type). `StepSecrets` wraps the map; its `Debug` prints names only. Nothing that holds a token or a value derives `Debug`.

### Where the new data lives (clippy)

`steps` and `run_checkout` already take 7 arguments each; `make lint` runs `clippy -D warnings` and no `#[allow]` is added. So the new data goes into `Args`: `id: u64` (replaces the `want_id` parameter), `allowed: Vec<String>`, `held: Held`, `ref_may_hold_secrets: bool` (computed from the entry by `WatchEntry::secrets_allowed_on`). `parse_args` fills defaults; `job()` fills them after the entry and the fetch (`let mut a`). `steps(a, token, run_id, env, s, log)` has 6 parameters; `run_checkout` keeps 7. `Args` does not derive `Debug`.

### Order inside `run_checkout` (one place, so the order is testable)

1. Load the pipeline (`load_pipeline`). Any parse fault is `pipeline file refused`; this covers the four runner settings (prefix `FACTORY_CI`), `BW_`, `BITWARDEN` and every other reserved name.
2. Docker rules: `docker_problems(a.allow_docker)` as today, plus `docker_secret_problems`: refused when any step lists secrets and any step (same or other) has `docker = true`, whatever the flags. Line `docker steps are refused`.
3. `unlisted(&a.allowed)` not empty: names to Loki, fault `step secret not allowed`.
4. No names requested: skip the rest (today's path).
5. Ref gate: `a.ref_may_hold_secrets` false: fault `repository or ref not in the watch list`.
6. `a.held.take(&names)`; on error: names and counts to Loki (index mapped to name in `ci.rs`), fault `step secret unavailable`.
7. `log.add_secrets(values)`: one batch.
8. `run_pipeline(.., secrets, ..)`.

A step that breaks two rules prints the first line in this order.

### Ref gate

`WatchEntry::secrets_allowed_on(ref)`: `secret_branches` set means the ref must match one of them. Unset means `branches` must have no pattern made only of `*` and `?` characters (the default `["*"]`, an explicit `["*"]`, `?*` and `*?` all match any ref) and the ref must match `branches`.

### Value rule

A value is usable when all hold: at most 32 KiB; not blank; no control character other than `\n` and `\r\n` (lone `\r`, NUL, tab, `\x0b`, `\x0c`, U+0085, U+2028, U+2029 are refused); if it is single-line (no `\n` once trimmed), it has no leading or trailing whitespace, a trailing newline included; if it is multi-line, every non-blank trimmed line has 4 or more characters and a trailing newline and blank inner lines are allowed. `Held::take` returns the value unchanged.

### Redactor forms and bounds

`Redactor::add(values)` takes a batch, sorts and dedups once. For each value it always adds: the value, its plain base64 and the base64 of `x-access-token:<value>` (both as today, whatever the length, so `new(["pw"])` still masks) and each trimmed line of 4 or more characters. It adds these derived forms only when the form is 8 or more characters and at most 48 KiB: the stable middle of base64 at the three alignments (`k` in 0..3, `base64(v[k..k + 3 * ((n - k) / 3)])`, so a prefix, a trailing newline or wrapping does not hide the value), each also URL-safe and unpadded; percent-encoding of every non-unreserved byte; the JSON-string escape; lowercase hex. `Redactor::forms()` returns every form, and `Log::secret_forms()` exposes it (011 scans them). `Log::longest_secret()` is the longest form. Residual limits are in spec R2.

### Public line from fixed grammar

`Log::write` redacts the text, then `emit` builds the public line from the redacted piece through `public_form`. A secret `pass` turns `pipeline passed` into `pipeline ***ed`, which `public_form` then drops. Fix: for `event` calls (fixed text from the runner) the public line is computed from the raw text; the sink and the non-public streams still get the redacted text. `public_form` prints only fixed words and digits, so raw input cannot print a value.

### Pump boundary

`engine.rs` `pump` reads `take(65536).read_until(b'\n')`, and `Log::write` redacts each chunk alone, so a value across byte 65536 of one line escapes. Fix in `pump`: hold back the last `carry` bytes of each cut chunk and prepend them to the next, flush the remainder at end of stream, never cut inside a UTF-8 character. `carry` equals the longest form (`Log::longest_secret()`), at most 48 KiB for derived forms and 32 KiB for the value, so it is always below the 64 KiB chunk. Tests at the 32 KiB cap and one byte over.

### Required runner keys

`REQUIRED_KEYS: [&str; 4]` in `store.rs` (renamed from the private `KEYS`) is the one list of the four keys the runner must find. `log.rs` uses it for `is_fault` instead of its own `SECRET_KEYS`. There is no by-name refusal: `valid_secret_name` already refuses all four by the `FACTORY_CI` prefix (and `FACTORY_CI_TOKEN` exactly), and every runner-held key is named `FACTORY_CI_*`. 011 adds its own lists.

## Files

### Changed

| File | Change |
|---|---|
| `rust-ci-runner/src/store.rs` | `REQUIRED_KEYS` (rename). `fetch_with` and `parse_list_with` (the one call, keeps rows of allowlisted keys); `fetch` stays as a wrapper over `fetch_with(.., &[])` so existing tests and callers compile; `parse_list` stays. `Fetched { secrets, held }`, `Held` (manual `Debug`, `take`), `StepSecrets` (names-only `Debug`), `NamedError { Count { index, found }, Unusable { index } }`, `usable_secret`. |
| `rust-ci-runner/src/pipeline.rs` | `Pipeline::secret_names`, `unlisted`, `docker_secret_problems`. Reserve prefixes `BW_` and `BITWARDEN` (`BWS_` is already there). `resolve_secrets`, `docker_problems` stay (the tick uses them). |
| `rust-ci-runner/src/watch.rs` | `WatchEntry::secrets_allowed_on`. Parsing and defaults unchanged. No `bws` or `bitwarden` in the text. |
| `rust-ci-runner/src/redact.rs` | `Redactor::add` (batch), `forms`; `new` calls `add`. No `bws` or `bitwarden`. |
| `rust-ci-runner/src/log.rs` | `RwLock<Redactor>`, `add_secrets`, `secret_forms`, `longest_secret`; public line from raw event text; `FAULTS` gains `step secret not allowed` and `step secret unavailable`; `is_fault` uses `REQUIRED_KEYS`. |
| `rust-ci-runner/src/engine.rs` | `pump` carries the boundary tail. |
| `rust-ci-runner/src/job.rs` | `load_pipeline` becomes `pub(crate)`. |
| `rust-ci-runner/src/ci.rs` | `Args` gains `id`, `allowed`, `held`, `ref_may_hold_secrets`; `job()` finds the entry first, calls `fetch_with`, fills `Args`; `steps` and `run_checkout` follow the order above. |
| `rust-ci-runner/src/ci_tests.rs` | New `mod step_secrets` at the end. Change `declared_secret_fails_before_any_container` and the case table of `every_stderr_line_is_in_the_fixed_list`. |
| `rust-ci-runner/.factory-ci.toml` | Split the `ci_tests::` group; fix the header comment (T014). |
| `.github/workflows/factory-ci-selftest.yml` | `list` job: `cargo test --locked --lib` without `--no-run` (T013); at the pin task, add `.github/workflows/factory-monorepo-ci.yml` to both `paths` lists (T020). |
| unit test files | `store_tests.rs`, `watch_tests.rs`, `secret_names_tests.rs`, tests modules in `pipeline.rs`, `redact.rs`, `log.rs`, `engine.rs`. |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | `0.2.0` to `0.3.0`. |
| `.github/workflows/factory-monorepo-ci.yml` | The `RUNNER_IMAGE` line only. |
| `.github/factory-ci-repos.toml` | Header comment; names only if FR-016 says so. |
| `README.md` (line 55), `rust-ci-runner/README.md` | Last task. |

### Added

- `rust-ci-runner/tests/repo_files.rs`: reads `../.github/factory-ci-repos.toml` (no name in two entries, no `secrets` on `sudtanj/infrastructure`), compares the `Cargo.toml` and `Cargo.lock` versions and, with the docs task, asserts `../README.md` no longer holds "Steps get no pipeline secrets here". It needs the whole repo checkout (`../.github`, `../README.md`): it runs in the selftest (`--test '*'`) and fails under a plain `cargo test` where `../` is missing. It is not in the explicit integration list of `rust-ci-runner/.factory-ci.toml`, whose step mounts only the project directory.
- `ci-canary/.factory-ci.toml`: a one-step pipeline (`echo ok`) for the no-secret canary (T021). `--project ci-canary` passes `folder_name_ok`.
- Spec artifacts: `research.md`, `data-model.md`, `contracts/ci-secrets.md`, `quickstart.md`, `tasks.md`. No new file under `src/`.

### Not changed, and why

- `.github/scripts/test-factory-monorepo-ci.sh`: line 89 matches any `X.Y.Z@sha256:<64 hex>`; lines 56 to 58 assert only `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN`. It runs in the selftest job `shell`, which `factory-monorepo-ci.yml` changes do not trigger today (T020 fixes the paths).
- `engine.rs` `run_logged`: already passes `-e NAME` with the value in the child environment, only for the step's own names.
- `.github/scripts/factory-ci-secrets.sh` and the tick.

## Data and API contracts

See [contracts/ci-secrets.md](contracts/ci-secrets.md) and [data-model.md](data-model.md). No new flag, environment variable, workflow input or file format.

## Engineer briefs

Ordering. T001 to T007 (tests) run in parallel; T005 is one file. In Phase 2, T008 (`store.rs`) starts first because T011 uses `REQUIRED_KEYS`; T009, T010 and T013 to T016 do not depend on it and run in parallel with it; T011 starts after T008; T012 starts after T011 (it calls `Log::longest_secret`); T015 needs T008 to T012. T017 any time. Landing is T018, once, after T001 to T017 (see Rollout). T019 to T026 are sequential and later; T027 (docs) is last. IDs match `tasks.md`.

### T008 Keyed lookup (FR-005 to FR-007, FR-018)
- Goal: read the four runner values and the allowlisted rows with the one call.
- Files: `rust-ci-runner/src/store.rs`.
- Acceptance: `fetch_with(bws, token, server, limit, allowed)` runs the same call, child environment, server check and retry as `fetch` and returns `Fetched { secrets, held }`. `held` has, for each name in `allowed`, the values of all rows whose whole key equals it. A row without a string `key` or non-JSON output is `Unreachable`; a rejected token is `Rejected`; both as today. `Held::take(names)` returns a `StepSecrets` with each name, or `NamedError::Count { index, found }` when found is not 1, or `Unusable { index }` per the value rule. Lazy: names not taken are never validated. A name not in the allowlist gives `Count { index, found: 0 }`. `Display` is `secret #<index>: expected 1, found <n>` and `secret #<index>: unusable value`; `Debug` of `StepSecrets`, `Held` and `NamedError` shows no value and no name. `KEYS` becomes `REQUIRED_KEYS`. `fetch` and `parse_list` behave as before.
- Constraints: the value rule in "Value rule", including the 32 KiB cap and the single-line whitespace refusal. Values pass untouched. The output buffer and non-allowlisted rows are dropped before return. No `derive(Debug)` on anything that holds a value or token.

### T009 Pipeline helpers (FR-001 to FR-004, FR-007, FR-009)
- Goal: ask a parsed pipeline what it requests and what breaks the rules.
- Files: `rust-ci-runner/src/pipeline.rs`.
- Acceptance: `secret_names` returns each name once, sorted. `unlisted(allowed)` returns every `(step, name)` not byte-equal to an entry of `allowed`. `docker_secret_problems` returns a text when any step lists secrets and any step has `docker = true`, for one step and for two separate steps, whatever the flags. `valid_secret_name` also refuses the prefixes `BW_` and `BITWARDEN`, case-insensitive like the others.
- Constraints: leave `resolve_secrets` and `docker_problems` alone. No by-name refusal of the four keys (the prefix does it).

### T010 Ref gate (FR-010)
- Goal: secrets only reach refs the entry names on purpose.
- Files: `rust-ci-runner/src/watch.rs`.
- Acceptance: `secrets_allowed_on(ref)` as in "Ref gate". `?*`, `*?`, `**`, `*` and `?` (patterns made only of `*` and `?`) count as wild.
- Constraints: parsing and `WatchEntry` fields do not change. No `bws` or `bitwarden`.

### T011 Redactor and log (FR-011 to FR-013)
- Goal: a log learns values after it exists, masks the FR-011 forms, and prints public lines from fixed grammar.
- Files: `rust-ci-runner/src/redact.rs`, `rust-ci-runner/src/log.rs` (after T008).
- Acceptance: `Redactor::add(batch)` and `new` follow "Redactor forms and bounds"; `new(["pw"])` still masks `pw`; `add(a); add(b)` masks what `new(a + b)` masks; `forms()` lists every form. `Log::add_secrets(Vec<String>)`, `secret_forms()` and `longest_secret()` exist; text written before the call is unchanged, after it masked; size limit and cut flag keep their state. A secret `pass` leaves `[runner] pipeline passed` intact on stdout while Loki holds the redacted text. `FAULTS` contains the two new strings (tests assert membership, not a count); `fault` still drops other text; `is_fault` uses `REQUIRED_KEYS`.
- Constraints: no `bws` or `bitwarden` in `redact.rs`. A poisoned lock recovers with `into_inner`. No new crate.

### T012 Pump boundary (FR-013)
- Goal: a value across the 64 KB read cut is masked, and the carry stays bounded.
- Files: `rust-ci-runner/src/engine.rs` (after T011).
- Acceptance: `carry` = `Log::longest_secret()`; a line with the value across byte 65536 reaches the log masked; ordinary output keeps content and order; the tail is flushed at end of stream; a UTF-8 character is never cut; with a 32 KiB value and its derived forms no chunk is held back forever.
- Constraints: `pump` stays generic over `Read`; the carry comes from the caller.

### T013 Selftest runs the lib tests (S1)
- Goal: lib tests run before code reaches `main`.
- Files: `.github/workflows/factory-ci-selftest.yml`.
- Acceptance: step "type-check the unit tests" runs `cargo test --locked --lib` (no `--no-run`) and is renamed; the job timeout still fits (30 minutes). The runner is on GitHub-hosted runners under the build exception already noted in that file's header; the file still holds no secret.
- Constraints: nothing else in the workflow changes here.

### T014 Split the test group (devops S3)
- Goal: the new tests cannot exhaust the 512-process limit of the step container.
- Files: `rust-ci-runner/.factory-ci.toml`.
- Acceptance: the `unit` step runs `cargo test --locked --lib ci_tests:: -- --skip ci_tests::step_secrets::` and a separate `cargo test --locked --lib ci_tests::step_secrets::`; the first command (`--skip ci_tests::`) is unchanged; the header comment is corrected: each `hub` (ci_tests.rs) ends its accept thread in `Hub::drop` (`server.unblock()`), but 46 existing plus about 30 new tests and their reader threads share one process, so the split is a guard.
- Constraints: if the pipeline still hits the limit, add `-- --test-threads=4`.

### T015 Wire `ci` (FR-001 to FR-013, FR-018)
- Goal: `ci` runs a pipeline with its allowlisted secrets.
- Files: `rust-ci-runner/src/ci.rs`, `rust-ci-runner/src/job.rs` (`load_pipeline` visibility).
- Acceptance: `job()` finds the entry, then calls `store::fetch_with(&s.bws, token, server, s.limits.bws, &entry.secrets)` with the Bitwarden `token`; `Args` carries `id`, `allowed`, `held`, `ref_may_hold_secrets`; the order in "Order inside `run_checkout`" holds. Faults print exactly `[runner] step secret not allowed`, `[runner] step secret unavailable`, `[runner] repository or ref not in the watch list` (gate) and `[runner] docker steps are refused`; exit 1; no `docker run`, `network create` or `volume create` call. Detail lines (step `ci`) carry names and counts and no value; stdout and stderr carry none. `log.add_secrets` runs with all requested values, once, before `run_pipeline`. Each step sees only its own names. Every run makes exactly one `bws` call.
- Constraints: no `set_var`; the server URL comes through the `env` closure. No `#[allow(clippy::...)]`: `steps` has 6 parameters, `run_checkout` 7. `make lint` stays clean.

### T016 Version (FR-014)
- Goal: the crate says `0.3.0`.
- Files: `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`.
- Acceptance: both say `0.3.0` for package `rust-ci-runner`; two changed lines; `tests/repo_files.rs::cargo_toml_and_lock_versions_match` passes.
- Constraints: by hand; touch no other lock entry (`--locked`).

### T017 Canary pipeline (S2 gap)
- Goal: a tiny pipeline for the no-secret canary.
- Files: `ci-canary/.factory-ci.toml`.
- Acceptance: one step, image pinned by digest (an image the repo already pins, such as the one in `rust-ci-runner/Dockerfile`'s Alpine base, or another small image by digest), `commands = ["echo ok"]`, no `secrets`, no docker.
- Constraints: no value, no secret name. The tick may plan this project; the tick is not in use.

### T018 Land the code (S1)
- Goal: all code reaches `main` once, after a green selftest.
- Files: none (git only).
- Acceptance: T001 to T017 are committed together; the branch push runs `factory-ci-selftest.yml` (full lib tests via T013, integration tests, `make lint`, `plan --check`) and it is green; then the same commits are pushed to `main` as a fast-forward. Each commit message links `specs/010-ci-step-secrets/` in place of a PR (PR creation on this repo returns 403).
- Constraints: never force-push; no merge commits; the spec, plan and tasks commits stay on the branch history.

### T019 Tag gate (FR-014)
- Goal: a published 0.3.0 image from a verified commit.
- Files: none.
- Acceptance: the landing is on `main`; the selftest was green on the branch; the `rust-ci-runner` pipeline was dispatched on `main` and passed, and the task records that run's id and full SHA; the SHA equals `git rev-parse main` at tag time (re-run if `main` moved); tag `rust-ci-runner-v0.3.0` is on it; `rust-ci-runner-release.yml` finished, smoke step included; the digest is read from the release run summary and recorded. No `docker pull` on the agent VM: the canary (T021) pulls the image.
- Constraints: the owner or lead pushes the tag. Never move a tag; a fix ships as 0.3.1.

### T020 Pin (FR-015)
- Goal: the workflow runs 0.3.0, with its shell tests run.
- Files: `.github/workflows/factory-ci-selftest.yml`, `.github/workflows/factory-monorepo-ci.yml`.
- Acceptance: on a branch, commit 1 adds `.github/workflows/factory-monorepo-ci.yml` to both `paths` lists of the selftest (its glob `factory-ci*.yml` does not match `factory-monorepo-ci.yml`); commit 2 sets `RUNNER_IMAGE` to `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest from T019>`; both pushed together; the selftest job `shell` runs `.github/scripts/test-factory-monorepo-ci.sh` and is green; then fast-forward `main`. Nothing else changes. `RUNNER_IMAGE_DIGEST` (tick variable) is not bumped.
- Constraints: separate commit for the pin after the digest exists. No placeholder digest. The agent does not run the shell script; the selftest does.

### T021 No-secret canary (devops S5)
- Goal: prove 0.3.0 runs before any secret exists.
- Files: none.
- Acceptance: `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=ci-canary` on the new pin ends in `[runner] pipeline passed` (it pulls the image and runs a tiny pipeline, minutes not the 30-minute `rust-ci-runner` pipeline).
- Constraints: on failure revert the pin (Rollback) before going on.

### T022 Preconditions re-check (FR-019, spec R1)
- Goal: the access facts still hold before the first secret run.
- Files: none (record the result in the task).
- Acceptance: with read-only `gh api` using a token with admin scope on the repos (Environment settings and collaborators need it; the owner runs this): Environment `factory-ci` has custom-branch policy with exactly the policy `main` and no required reviewer (2026-11-28: so); the only collaborator of `sudtanj/infrastructure` and `sudtanj/factory-monorepo` is the owner; `factory-monorepo` has no `.github/dependabot.yml`, no Renovate config and no auto-merge on `main`; the `rust-ci-runner-v*` tag ruleset exists; the GHCR package is readable by the workflow; the machine account reads one project.
- Constraints: an agent runs no `bws` or `bw`, reads no `BWS_ACCESS_TOKEN` or Environment secret, creates or reads no Bitwarden key.

### T023 Watch entry (FR-016)
- Goal: the header comment is true; names only where needed.
- Files: `.github/factory-ci-repos.toml`.
- Acceptance: the comment says `ci` reads `secrets` as the per-repo allowlist, applies the FR-010 gate (`branches` explicit, no `*`, `?*`, or `secret_branches`), and ignores nothing now. The `sudtanj/factory-monorepo` entry gains only the non-login names a step needs (none today; the owner names them). `sudtanj/infrastructure` never gets `secrets`. `tests/repo_files.rs` and `plan --check` pass.
- Constraints: names only; secret names are public in this file.

### T024 Canary values run (FR-019, security S8)
- Goal: see what Loki shows for a value before a real value exists.
- Files: `.github/factory-ci-repos.toml` (a temporary canary name, removed in this task).
- Acceptance: the owner creates a Bitwarden key with a made-up value, lists its name in a step of the `factory-monorepo` pipeline on `main` (that repo's change) and in the watch entry; the step prints the value plain, as `echo "$V" | base64` and percent-encoded; the run ends in `pipeline passed` with only fixed lines in the Actions log; Loki (query in `quickstart.md`) shows `***` for all three; the name is then removed and the key deleted.
- Constraints: owner only for every Bitwarden action; an agent reads the Actions log and Loki only.

### T025 First real run (SC-001)
- Goal: a real step secret works end to end.
- Files: none.
- Acceptance: after T022 to T024 and when a real non-login step secret exists, `factory-monorepo-ci` on `main` ends in `[runner] pipeline passed`, with fixed lines in the Actions log and masked values in Loki. Until a secret exists the task waits; 010 is complete without it. Logins come from 011.
- Constraints: same Bitwarden rules as T024.

### T026 One-week check (SC-004)
- Goal: unexpected `step secret` faults are caught.
- Files: none.
- Acceptance: one week after T025, list the runs of `factory-monorepo-ci` (`gh run list`, needs only Actions read; T022 is the one that needs admin scope) and count the two new lines; an unexpected `step secret not allowed` is a README fix.
- Constraints: read-only.

### T027 Docs (FR-017), last
- Goal: both READMEs and the GitHub description match.
- Files: `README.md` (line 55, the "Steps get no pipeline secrets here" sentence), `rust-ci-runner/README.md` (the `ci` steps 2 and 5 where it "refuses a docker step or a declared secret", the minimal-output paragraph and fault list, the Secrets section, risks), `rust-ci-runner/tests/repo_files.rs` (adds `readme_has_no_stale_secrets_sentence`), repository description with `gh repo edit` (needs admin).
- Acceptance: states the allowlist (per repo, not per project), the lookup rule, the new lines, the always-refused names (four settings, `BW_`, `BITWARDEN`), no docker steps in a pipeline with secrets, the ref gate (`branches` explicit, and `*`, `?*` or `secret_branches = ["*"]` set on purpose open it), masking forms and limits, shared names across repos, `sudtanj/infrastructure` never gets `secrets`, secret names are public, steps share `/workspace` (pin images by digest, `npm ci --ignore-scripts`), the tick also reads the names, that `repo_files.rs` needs `../.github` and `../README.md`, the runbook line for `step secret unavailable` (check the key exists once with that exact name in the project; a failed `bws` call retries once with no backoff, so start the run again), and the rollback list. No value appears.
- Constraints: stop-slop applies.

## Test plan (qa writes it first)

Unit tests sit in the modules named. `ci::run` integration tests sit in `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs` (`use super::*;`, so the private `W` harness is reused). The `WATCH` constant has `branches = ["*"]` for `o/r`, which FR-010 refuses for secrets: tests overwrite `w.watch` (`repos.toml`) with an entry that has `branches = ["main"]` and `secrets = [...]`, and add store rows by pushing `(key, value)` into `w.secrets`; `W::bootstrap()` feeds those values into the leak searches. Use a unique canary per value. The `bws` stub stays one script: there is only one call.

| ID | File | Test | FR / story |
|---|---|---|---|
| T001 | `store_tests.rs` | `held_take_returns_values_by_exact_key`; `..._missing_key_is_found_0`; `..._duplicate_key_is_found_2`; `..._key_differing_in_case_does_not_match` | FR-001, FR-003, FR-005, FR-006 |
| | | `held_take_refuses_empty_blank_and_each_control_character` (NUL, tab, lone `\r`, `\x0b`, `\x0c`, U+0085, U+2028, U+2029) | FR-006 |
| | | `held_take_line_boundary` (3 fails, 4 passes, blank inner line passes, `\r\n` passes, trailing newline on a multi-line value passes) | FR-006, FR-007 |
| | | `held_take_refuses_surrounding_whitespace_on_a_single_line` (leading space, trailing space, trailing `\n`) | FR-006 |
| | | `held_take_size_cap` (32768 bytes passes, 32769 fails) | FR-006 |
| | | `held_take_passes_unicode_and_equals_untouched`; `held_take_validates_only_the_names_taken` (a bad allowlisted key not requested does not fail); `held_take_looks_up_a_repeated_name_once` | FR-007 |
| | | `fetch_with_keeps_only_allowlisted_rows_and_still_needs_the_four_runner_keys`; `fetch_with_empty_allowlist_equals_fetch` | FR-005 |
| | | `fetch_with_retries_once_and_a_rejected_token_is_final`; `..._refuses_a_foreign_server_url`; `..._child_env_is_cleared` | FR-005 |
| | | `debug_and_display_of_held_results_and_errors_hold_no_value_and_no_name` | FR-018 |
| T002 | `pipeline.rs` tests | `unlisted_*` (all allowed, each miss, empty allowlist, case difference, one allowed one listed); `secret_names_returns_each_name_once_sorted` | FR-001 to FR-003, FR-007 |
| | | `docker_secret_problems_flags_secrets_with_any_docker_step` (one step; two separate steps; neither; flags irrelevant) | FR-009 |
| | | `each_runner_setting_is_refused_in_a_step` | FR-004 |
| T003 | `watch_tests.rs` | `each_runner_setting_in_secrets_refuses_the_file` | FR-004 |
| | | `secrets_allowed_on_*`: `branches` omitted, `["*"]`, `["?*"]`, `["*?"]`, `["main"]`, `["release/*"]`, `secret_branches` matching, not matching, `["*"]` | FR-010 |
| | `secret_names_tests.rs` | rows for `BW_X`, `BITWARDEN`, `bitwarden_x` refused in a watch entry and a pipeline | FR-004 |
| T004 | `redact.rs` tests | `new_still_masks_a_two_character_value` (`new(["pw"])`) and the existing tests stay green | FR-011 |
| | | `add_masks_value_and_base64_forms` (`echo "$V" \| base64` with trailing newline, `printf 'x%s' "$V" \| base64`, wrapped at 76, URL-safe, unpadded) | FR-011, FR-013 |
| | | `add_masks_percent_json_and_hex_forms`; `add_batch_equals_new`; `add_skips_derived_forms_under_8_chars_and_over_48_kib`; `forms_lists_every_form` | FR-011 |
| | `log.rs` tests | `add_secrets_masks_later_lines_only`; `add_secrets_keeps_size_limit_state`; `a_secret_named_pass_cannot_mangle_a_public_line`; update `fault_accepts_only_the_fixed_list` to assert both new lines are members of `FAULTS` and that every member prints, with no length assert | FR-011, FR-012 |
| T005 | `ci_tests.rs` `step_secrets::` | `step_secrets_reach_only_the_step_that_lists_them` (`docker.calls` has `-e NAME` and no value; `docker.envs` of that run has it, the other run not; `bws_calls() == 1`) | US1, FR-001, FR-008 |
| | | `every_run_makes_one_bws_call` (with and without secrets, refusals included) | I8 |
| | | `an_allowlisted_key_with_a_bad_value_does_not_fail_a_run_that_does_not_request_it` | FR-005 |
| | | `unlisted_name_fails_before_any_container` (stderr exactly `[runner] step secret not allowed\n`, `started_containers() == 0`, name in a Loki body only) | FR-002, FR-012, US2 |
| | | `entry_without_secrets_key_refuses_a_requested_name`; `name_differing_in_case_is_not_allowed`; `single_allowed_name_runs` | FR-002, FR-003 |
| | | `runner_setting_in_the_pipeline_is_refused` (four rows) and `runner_setting_in_the_entry_refuses_the_watch_file` (four rows, `bws_calls() == 0`) | FR-004 |
| | | `unavailable_value_ends_the_run_before_any_container` (missing, duplicate, empty, blank, NUL, 3-char line, trailing newline single-line, over 32 KiB; `step secret unavailable`; zero containers; name and count in Loki) | FR-006, US3 |
| | | `four_char_value_runs`; `blank_inner_line_value_runs_and_masks_each_line` | FR-007, FR-011 |
| | | `two_steps_listing_one_name_get_one_value` | FR-007 |
| | | `docker_step_in_a_pipeline_with_secrets_is_refused` (two separate steps; `--allow-docker` and `allow_docker = true`; `docker steps are refused`; zero containers) | FR-009 |
| | | `secrets_with_branches_omitted_or_star_are_refused` (rows: omitted, `["*"]`, `["?*"]`; line `repository or ref not in the watch list`; zero containers) | FR-010 |
| | | `secret_branches_must_match_the_ref`; `explicit_branches_main_gets_secrets`; `a_pipeline_without_secrets_runs_on_branches_star` | FR-010 |
| | | `hostile_step_cannot_print_a_secret_anywhere` (plain, `echo "$V" \| base64`, `printf 'x%s'` base64, percent-encoded, inside a longer line; search stdout, stderr, `docker.calls`, `bws.calls`, `git.calls`, `hub.bodies()`; Loki holds `***`) | FR-011, FR-013, US4, SC-002 |
| | | `a_secret_named_pass_keeps_pipeline_passed_on_stdout` | FR-012 |
| | | `no_public_line_holds_a_secret_name_or_count` | FR-012 |
| | `ci_tests.rs` (existing group) | change `declared_secret_fails_before_any_container` to expect `step secret not allowed`; add two rows to `every_stderr_line_is_in_the_fixed_list` | FR-012 |
| T006 | `engine.rs` tests | `pump_masks_a_value_across_the_64k_cut`; `pump_masks_a_32_kib_value_across_the_cut` and `pump_with_a_value_one_byte_over_the_cap_is_never_given_to_it`; `pump_keeps_order_and_flushes_the_tail`; `pump_never_cuts_a_utf8_char` | FR-013 |
| T007 | `tests/repo_files.rs` | `no_secret_name_in_two_committed_entries`; `infrastructure_entry_has_no_secrets`; `cargo_toml_and_lock_versions_match` | FR-014, FR-016 |

SC-003 is the `started_containers()` assert in the refusal tests. SC-001, SC-004, FR-019 are manual (Principle V exception). `tests/no_vendor.rs` must pass: `bws` and `bitwarden` stay in `store.rs`, `ci.rs`, `pipeline.rs`, `log.rs`, `engine.rs`, `main.rs`, not in `redact.rs` or `watch.rs`.

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| docker rules | any docker step in a pipeline with secrets | `docker_secret_problems` | `docker steps are refused` | `docker_step_in_a_pipeline_with_secrets_is_refused` |
| allowlist | name not in entry, no `secrets` key, case difference | `unlisted` | `step secret not allowed` | `unlisted_name_fails_before_any_container` |
| ref gate | `branches` omitted, `*`, `?*`; ref not in `secret_branches` | `secrets_allowed_on` | `repository or ref not in the watch list` | `secrets_with_branches_omitted_or_star_are_refused` |
| pipeline load | runner setting, `BW_`, `BITWARDEN` name | `valid_secret_name` | `pipeline file refused` | `runner_setting_in_the_pipeline_is_refused` |
| watch read | runner setting in the entry | `watch::parse` | `repository or ref not in the watch list` | `runner_setting_in_the_entry_refuses_the_watch_file` |
| store call | `bws` down, timeout, rejected token, list over 16 MiB (retry once, no backoff) | `fetch_with` error | existing `secret store unreachable` or `rejected the access token` | existing `store_tests.rs` |
| lookup | key missing or duplicated; value unusable (control character, over 32 KiB, surrounding whitespace, short line) | `NamedError` | `step secret unavailable`, name and count to Loki | `unavailable_value_ends_the_run_before_any_container` |
| redaction | value in a listed form reaches a line | canary search | `***` in Loki | `hostile_step_cannot_print_a_secret_anywhere` |
| redaction | value across the 64 KB cut | pump test | masked | `pump_masks_a_value_across_the_64k_cut` |
| public line | value such as `pass` mangles a fixed line | test | line intact | `a_secret_named_pass_keeps_pipeline_passed_on_stdout` |
| run | huge value refused by the OS | cannot happen over 32 KiB | n/a | n/a |
| run | step transforms a value beyond FR-011 | not detected | unmasked in Loki (R2) | none; README |
| test run | 512-process limit hit by the test group | pipeline step fails | split group (T014) | the pipeline itself |
| landing | lib tests fail on the branch | selftest red | no fast-forward to `main` | selftest |
| release | tag SHA differs from the tested SHA | T019 check | no tag until re-run | T019 |
| rollout | pin before digest exists, or pin bad | pull fails; canary fails | revert the pin commit | T021 |
| rollout | first lookup fails for a missing key | `step secret unavailable` | owner creates the key with that exact name and starts again | quickstart |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. A value reaches Loki or stdout through a form the redactor missed or a boundary. Prevented: the FR-011 forms, the pump tail bounded below the chunk, one batch before `run_pipeline`, the canary run T024 before any real value, the hostile tests.
2. Secrets reach a branch nobody meant (default `branches = ["*"]`, `?*`, `secret_branches` ignored). Prevented: FR-010 gate with its tests.
3. A change to the first `bws` call breaks every run, with or without secrets, because `fetch_with` shares it. Prevented: `fetch` and `parse_list` keep their tests unchanged; `fetch_with` with an empty allowlist must equal `fetch` (test); selftest runs the lib tests before `main`; the no-secret canary T021.
4. The release or rollout breaks: lock and manifest disagree under `--locked`; the pin is bad; the tested SHA is not the tagged SHA; the pin gate never ran. Prevented: T016 touches two lines and `repo_files.rs` compares them; T019 records and compares SHAs; T020 adds the missing selftest path so job `shell` runs; T021 canary.
5. A public line leaks a name or count, or a value such as `pass` mangles the grammar. Prevented: `Log::fault` drops text outside `FAULTS`; details use `log.out`; public lines come from fixed grammar; tests for both.

## Invariants (qa turns these into property tests)

- I1: no container starts unless every requested name is in `entry.secrets`, has exactly one valid value, and the ref gate passed.
- I2: for any set of values and any listed form, no byte of stdout, stderr, any child argv or any Loki body holds the raw value or a form from FR-011, once the lookup finished.
- I3: a step's container receives exactly `step.secrets` as `-e NAME` and no other store key.
- I4: `unlisted(p, allowed)` is empty if and only if `secret_names(p)` is a subset of `allowed`; growing `allowed` never adds a miss.
- I5: whatever the allowlist, none of the four runner keys reaches a step environment.
- I6: stdout lines belong to the existing grammar and stderr lines to `FAULTS`, whatever the secret values are; no line holds a secret name.
- I7: `Redactor::add(a)` then `add(b)` masks the same text as `new(a + b)`; `new([v])` masks `v` for any non-empty `v`.
- I8: every run makes exactly one `bws` call.
- I9: the output of `pump` equals its input with masked forms replaced, whatever the chunk cuts and whatever value of at most 32 KiB.
- I10: `Held::take` never returns a value that breaks the value rule.

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: empty allowlist, duplicate names, case differences, empty, blank, control-character, whitespace-padded, short-line, blank-inner-line, over-cap, multi-line, Unicode and `=` values, one name in two steps, `branches` omitted or `?*`: each has a test row.
- State and order: no state kept; the lookup finishes before the first container; a crash leaves only the checkout the existing cleanup removes.
- Timing: no new call; the existing 60 s attempt and one retry without backoff apply.
- Concurrency: concurrent runs read the store independently; the workflow's queue group is unchanged.
- Resources: values at most 32 KiB, derived forms at most 48 KiB, pump carry below the 64 KiB chunk; a list over 16 MiB ends in `secret store unreachable`; the test group is split for the process limit.
- Permissions and trust: the watch file on `main` decides names and refs; the pipeline cannot widen it; runner settings never pass; names are public.
- Failure and rollback: every failure stops before a container; rollback list below.
- Output and UI: no UI; two new closed lines; public lines from fixed grammar.
- Compatibility: pipelines with no secrets behave as before; a secret-listing pipeline now ends in `step secret not allowed` instead of `pipeline file refused` (R6).

## Rollout and rollback

Landing. PR creation on `sudtanj/infrastructure` returns 403, so code lands by push: the whole code change (T001 to T017: tests, code, selftest edit, test-group split, version, canary pipeline) goes as one set of commits. First push it to the feature branch; `factory-ci-selftest.yml` runs on it (lint, the full lib tests with T013, the integration tests, `plan --check`) and must be green. Then fast-forward `main` with the same commits in one push. Each commit message links the feature folder `specs/010-ci-step-secrets/` in place of a PR link. Tests are written before code in order of authoring; the branch selftest is where that is checked (it is green only with both).

Rollout after landing:

1. Dispatch the `rust-ci-runner` pipeline on `main`; it must pass (T019 records run id and SHA).
2. Tag `rust-ci-runner-v0.3.0` on that SHA (T019). Read the digest from the release summary.
3. Pin to a branch (two commits, T020), selftest job `shell` green, fast-forward `main`.
4. No-secret canary on `ci-canary` (T021), which pulls the image.
5. Re-check the preconditions (T022).
6. Comment and conditional names (T023).
7. Canary values run (T024), then the first real run when a non-login secret exists (T025).
8. One-week check (T026). Docs last (T027).

Rollback, in this order:

1. Revert the pin commit (`factory-monorepo-ci.yml` returns to 0.2.0). 0.2.0 refuses step secrets.
2. `RUNNER_IMAGE_DIGEST` (the tick's variable) is not bumped for 0.3.0, so nothing to revert there.
3. The release workflow moved `:latest` to 0.3.0 (`rust-ci-runner-release.yml`, tag latest step). Moving it back needs owner approval: `docker buildx imagetools create --tag ghcr.io/sudtanj/rust-ci-runner:latest ghcr.io/sudtanj/rust-ci-runner@<0.2.0 digest>`, run by the owner. Nothing in this repo pulls `latest`.
4. Fixes ship as 0.3.1. Never move or delete the 0.3.0 tag.
5. Remove the names from the watch entry.
6. Rotate the values if a leak is suspected (the owner, in Bitwarden), and delete the canary key.

## Devops assessment

| Finding | Handling | Where |
|---|---|---|
| S1 `bws secret list` has no filter | Now one call (critic S9): `fetch_with` keeps allowlisted rows, drops the rest; 16 MiB cap gives the existing `secret store unreachable`; retention-only wording | Architecture decision, research D1, T008 |
| S2 wrong token for the lookup | Disappears with the single call: `fetch_with` runs where `token` (Bitwarden) is in scope. No closure, no equality test | Architecture decision, T015 |
| S3 `ci_tests` group and `--pids-limit 512` | Partly wrong (`Hub::drop` ends the accept thread); split anyway; header comment corrected | T014, research D7 |
| S4 rollback and `:latest` | Rollback list | Rollback |
| S5 no-secret canary | T021, with a tiny `ci-canary` pipeline instead of the 30-minute pipeline | T017, T021 |
| S6 selftest ignores pushes to `main` | Landing and pin go to a branch first; the selftest `paths` gain `factory-monorepo-ci.yml` (its glob does not match that name); job is `shell`, not `workflow` | T013, T020 |
| S7 tag gate | Run id and full SHA recorded, equal to tagged SHA, `make lint` in the selftest, digest from the release summary, no `docker pull` on the agent VM | T019 |
| Notes: no retry backoff, Loki query, GitHub preconditions, Chromium `/dev/shm` | Runbook line; query in quickstart; T022 (admin scope, owner); `/dev/shm` is `factory-monorepo` 002 | T027, quickstart, T022 |
| N6 one required-keys list; drop buffers; `RwLock` | `REQUIRED_KEYS` only, no by-name check; `RwLock` for 011 | Required runner keys, T011 |
| B1 Environment and collaborator facts | Recorded (2026-11-28, `gh api`): Environment `factory-ci` has custom-branch policy with exactly `main`, no required reviewer; the only collaborator of both repos is the owner. R1 is "owner-only write access, Environment limited to `main`"; T022 re-checks it and checks for Dependabot or Renovate auto-merge | spec R1, T022 |

## Security assessment

| Finding | Handling | Where |
|---|---|---|
| S1 default `branches = "*"`, `secret_branches` ignored | FR-010 gate before any lookup; `?*` and `*?` count as wild | Ref gate, T010, T015 |
| S2 dind is privileged | Any docker step in a pipeline with secrets is refused | FR-009, T009 |
| S3 token reaches `run_checkout`; `Debug` leaks | The lookup sits in `job()`; `Args` holds `Held` with names-free `Debug`; no `Debug` on holders; interface v1 | Lookup interface, T008, T015 |
| S4 redactor misses `echo "$V" \| base64` and other forms | Forms added with floors and caps; hostile tests use those commands | Redactor forms, T011, T004 |
| S5 pump chunk boundary | Carry equals the longest form, bounded by the 32 KiB value cap and 48 KiB form cap | Pump boundary, T012, T006 |
| S6 value rule and public-line mangling | Control characters refused, blank inner lines allowed, surrounding whitespace on single lines refused, public line from fixed grammar | Value rule, T008, T011 |
| S7 reserve `BW_`, `BITWARDEN` | `BWS_` already there; `BW_` and `BITWARDEN` added | T009, T003 |
| S8 canary run, rotation, agent rules | T024, rollback step 6, tasks.md rules | tasks.md |
| S9 no name in two entries; infrastructure never gets secrets | `tests/repo_files.rs`; README line | T007, T027 |
| N1 parse `bws` output only in `store.rs` | Done | T008 |
| N2 shared `/workspace` | In R1 and the README | spec R1, T027 |
| N3 error and `Debug` hold no value | Test | T001 |
| N4 names are public | Stated; no claim of name secrecy | spec, T023 |
| N6 allowlist is per repo | In the README | T027 |

## Critic review (third revision)

| Finding | Handling |
|---|---|
| B1 pin gate cannot run | `factory-monorepo-ci.yml` added to the selftest `paths` in T020 (two commits, one push); job name is `shell` everywhere |
| B2 8-character floor | Value and trimmed lines of 4 or more are always added; plain base64 stays unconditional; derived forms need 8 or more; test `new(["pw"])`; limits in spec R2 |
| B3 Principle V | Exception recorded above; cheap checks added (`repo_files.rs`) |
| S1 no test before `main` | T013 runs `--lib` in the selftest; landing by branch push then fast-forward; commits link the feature folder |
| S2 `[P]` dependencies | T008 first; T011 after T008; T012 after T011; others parallel |
| S3 clippy | New data in `Args`; `steps` 6 parameters; no `#[allow]` |
| S4 pump carry | 32 KiB value cap; 48 KiB derived-form cap; carry = longest form, below 64 KiB; tests at the cap |
| S5 trailing newline | Refused on single-line values; multi-line unchanged |
| S6 `docker pull` | Dropped; digest from the release summary; T021 pulls |
| S7 "dropped" overclaim | Reworded to retention only; no zeroize |
| S8 scope | 010 stays; US1 and Hypothesis fixed; consumer is any non-login secret such as an API key |
| S9 single call | Adopted |
| N1 to N6 | Position-keyed `NamedError`; one `REQUIRED_KEYS`, no by-name check; `forms()` and `Log::secret_forms`; membership asserts; one batch; 010 merges before 011 |
| Wrong claims | README location fixed in the spec; `Unusable` text widened in data-model; `.factory-ci.toml` header comment fixed (T014); `?*` rule; no per-call stub; SC-004 task T026; tiny canary pipeline; `repo_files.rs` needs `../` documented |

Not applied: none. Two points go beyond the lead's text and are flagged: derived forms over 48 KiB are not added (without it, the percent form of a 32 KiB value can reach 96 KiB, over the 64 KiB chunk), and the landing goes to the feature branch first so the selftest can run (the selftest ignores pushes to `main`).

## Open risks

- Tabs inside a value are refused (control character), as `store.rs` `usable` does today.
- `RwLock` and the public-line change touch `Log`, which `run`, `job` and `ci` share; `job_tests.rs` and `log.rs` tests must stay green.
- Held values of allowlisted keys sit in memory from the first call (spec Assumptions).
- The temporary canary name in T024 touches the `factory-monorepo` pipeline file, a change in that repo.
- 010 and 011 conflict in files; 010 merges first.
- T013 makes the selftest run the full lib tests once; if they exceed 30 minutes on a hosted runner, raise `timeout-minutes` in the same task.

## Project Structure

```text
specs/010-ci-step-secrets/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/ci-secrets.md
└── tasks.md

rust-ci-runner/src/   store.rs  pipeline.rs  watch.rs  redact.rs  log.rs  engine.rs  job.rs  ci.rs  (+ tests named above)
rust-ci-runner/tests/repo_files.rs
rust-ci-runner/.factory-ci.toml  Cargo.toml  Cargo.lock
ci-canary/.factory-ci.toml
.github/factory-ci-repos.toml  .github/workflows/factory-monorepo-ci.yml  .github/workflows/factory-ci-selftest.yml
README.md  rust-ci-runner/README.md
```

**Structure Decision**: one existing crate; no new module under `src/`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| `Log` redactor behind an `RwLock` | Secret names are known only after the log exists; values must join without a second log; feature 011 adds values later | A second `Log` resets the 10 MB limit and cut state |
| `StepSecrets` and `Held` with names-free `Debug` | A stray `{:?}` of the map or the held rows must not print values | Bare maps print their values |
| Extra redactor forms, caps and the pump tail | Security S4, S5: a step can print `echo "$V" \| base64`; a long line can straddle the read cut | Plain value and plain base64 miss both |
| Public line from raw event text | Security S6: a value such as `pass` breaks `pipeline passed` | Printing the redacted text does that |
| `ci-canary/.factory-ci.toml` | A canary that does not rerun the 30-minute runner pipeline | Reusing `rust-ci-runner` costs 30 minutes per canary |
