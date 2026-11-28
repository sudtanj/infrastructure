# Implementation Plan: Step secrets for the ci command

**Branch**: `010-ci-step-secrets` | **Date**: 2026-11-28 (revised after the devops assessment and the security threat model) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/010-ci-step-secrets/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` and `setup-tasks.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before the plan and `/speckit-analyze` after `tasks.md`. The spec carries a clarification session and was amended by the two assessments (FR-009, FR-010, FR-016, FR-018, FR-019 and the value rule changed or are new). `/speckit-analyze` is still to run before `/speckit-implement`.

## Summary

`ci` gives a step the Bitwarden secrets it lists, when the run's watch entry allows the name and the ref may receive secrets. After the fetch the runner checks, in order: pipeline parse, docker rules, allowlist, ref gate. Then it makes one more `bws secret list` call with the Bitwarden access token (not the GitHub clone token), keeps only the named keys, validates every value, drops the full list, adds the values to the log's redactor and hands the map to `run_pipeline`. Any failure ends the run before the first container with a fixed public line. The engine already passes values as `-e NAME` with the value in the child environment, and the name rules already refuse the four runner settings. New code: one lookup function, pipeline and watch helpers, a stronger redactor, a public-line fix, a pump boundary fix and the `ci.rs` wiring.

## Technical Context

**Language/Version**: Rust 2021, toolchain from `rust-ci-runner/rust-toolchain.toml` (pipeline image `rust:1.90`)

**Primary Dependencies**: no new crate. `serde_json`, `toml`, `ureq`, std only (JSON-string escaping uses `serde_json`, percent-encoding and hex are a few lines).

**Storage**: none. No state between runs.

**Testing**: `cargo test --locked --lib` (unit, and `ci_tests.rs` driving `ci::run` with stub `bws`, `docker`, `git` and a `tiny_http` Loki), `cargo test --locked --test ...` for the integration binaries. Both run in the `rust-ci-runner` pipeline (`rust-ci-runner/.factory-ci.toml`). `factory-ci-selftest.yml` runs `--lib --no-run`, `--test '*'`, `make lint` and `plan --check`.

**Target Platform**: the `ghcr.io/sudtanj/rust-ci-runner` image, linux/amd64 and arm64, started by `factory-monorepo-ci.yml`.

**Project Type**: CLI (one binary, `ci` subcommand).

**Performance Goals**: one extra `bws` call (60 s limit per attempt, one retry, no backoff) only when a step lists a secret. A pipeline with no secrets makes the same single call as today.

**Constraints**: values only in the environment of the engine child, never argv; public stdout and stderr stay a closed grammar; names and counts to Loki only; this VM has 1 GB, so nobody builds locally.

**Scale/Scope**: at most 20 allowlisted names per entry (`watch.rs` `MAX_LIST`), at most 100 steps.

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Runner and watch file stay here. |
| II Secrets source | Pass. Pipeline secrets come from Bitwarden through the existing token. The workflow gains no secret and no input. No value is committed; docs name keys only. |
| III Public logs | Pass by design: two fixed lines, no name or count, redactor filled before any step, public lines built from fixed grammar (FR-011, FR-012). The hostile tests search every output for canaries. |
| IV Simplicity | Pass with the items in Complexity Tracking. No new crate, no new watch key, no new module. |
| V Tested | Pass. Every FR maps to a test in the Test Plan; tests are written first. |
| VI Spec-driven | Pass for plan and tasks; `/speckit-analyze` runs next. |

Re-check after design: unchanged.

## Architecture decision

Chosen: a second `bws secret list` call inside `ci.rs`, after the fetch, only when a step lists a name and the docker, allowlist and ref checks passed.

`bws secret list` has no filter (devops S1). The second call fetches the whole project and `parse_named` keeps only the named keys. What this buys over reusing the first call: no value of any kind is looked up or kept on a refusal path (FR-002), a run with no step secrets makes no new call, and the process holds the whole list text only inside `fetch_named`: the buffer and every parsed row are dropped before it returns, so nothing but the requested values is alive when `run_pipeline` starts. `run_capped` caps stdout at 16 MiB (`engine.rs`, `read_capped(r, 16 << 20)`); a larger project list is cut, fails to parse and ends as `Store(Unreachable)`, so `step secret unavailable`. Parsing the `bws` output happens only in `store.rs`.

Alternatives rejected:
- Reuse the first call and keep the allowlisted rows. It holds values before the pipeline asked for them and ties value problems to names the pipeline may not request.
- A second `Log` with a bigger Redactor, as `job()` does for the first one. It resets the 10 MB limit and cut flag. `Log` instead holds its redactor in an `RwLock` and gets `add_secrets`, which also lets feature 011 add values after the log exists.
- Per-key `bws secret get`. It needs secret ids, which only the list gives.
- Resolve in the workflow with `factory-ci-secrets.sh`. The constitution keeps the workflow to the one access token, and the script cannot run inside the container.
- Repo-own keys `owner/name<SEP>NAME`. Out of scope.

### The lookup closure (security S3, devops S2)

In `job()` the variable `token` is the Bitwarden access token. `steps()` and `run_checkout` receive `sec.token`, which is `FACTORY_CI_TOKEN`, the GitHub clone token. The lookup must use the first. So `job()` builds, next to `server`:

```rust
let fetch = |names: &[String]| store::fetch_named(&s.bws, token, server.as_deref(), s.limits.bws, names);
```

and passes only `&fetch` (type `&dyn Fn(&[String]) -> Result<StepSecrets, NamedError>`) plus `allowed: &[String]` to `steps` and `run_checkout`. `run_checkout` never sees either token. `StepSecrets` is a newtype over `BTreeMap<String, String>` whose `Debug` prints names only; nothing that holds the token or a value derives `Debug`. Interface v1: names in, name-to-value map out (see contracts). A test asserts that the second `bws` call carries the same `BWS_ACCESS_TOKEN` as the first and never the clone token.

### Order inside `run_checkout` (one place, so the order is testable)

1. Load the pipeline from the checkout (`load_pipeline`). Any parse fault is `pipeline file refused`; this covers the four runner settings, `BW_` and `BITWARDEN` names and every other reserved name.
2. Docker rules: `docker_problems(a.allow_docker)` as today, plus the new `docker_secret_problems`: refused when any step lists secrets and any step (same or other) has `docker = true`, whatever the flags. Line: `docker steps are refused`.
3. `unlisted(allowed)` not empty: names to Loki, fault `step secret not allowed`, no `bws` call.
4. No names requested: skip the lookup (today's path, one `bws` call in the run).
5. Ref gate for secrets (`WatchEntry::secrets_allowed_on(ref)`): `secret_branches` set means the ref must match; unset means `branches` must be explicit and without a bare `*`. Failure: fault `repository or ref not in the watch list`.
6. `fetch` closure; on any error: names and counts to Loki, fault `step secret unavailable`.
7. `log.add_secrets(values)`.
8. `run_pipeline(.., secrets, ..)`.

Docker comes before the allowlist because `docker_problems` already ran there; the gate comes after the allowlist because it needs the requested set. A step that breaks two rules prints the first line in this order.

The "bare `*`" rule: a pattern made only of `*` characters. `WatchEntry.branches` defaults to `["*"]` when the key is absent, so omitted and explicit `["*"]` are the same and both refused. Patterns such as `release/*` pass.

### Public line from fixed grammar (security S6)

`Log::write` redacts the text, then emits; in public mode `emit` builds the line from the redacted piece through `public_form`. A secret such as `pass` turns `pipeline passed` into `pipeline ***ed`, which `public_form` then drops. Fix: `event` and `fault` callers pass fixed text, so the public line is computed from the raw text of an `event` call and printed; the sink and every non-public stream still get the redacted text. `public_form` already cuts a step reason after `:` and only prints digits and fixed words, so a raw input cannot print a value.

### Redactor forms (security S4)

`Redactor::add` masks, for each value `v` (bytes) with at least 4 characters: `v`; standard base64 of `v`; base64 of `x-access-token:<v>` (existing); the stable middle of base64 at the three alignments (for `k` in 0..3, `base64(v[k..k + 3 * ((n - k) / 3)])`, so a prefix, a trailing newline or wrapping does not hide it), each also URL-safe (`-_`) and unpadded; percent-encoding of every non-unreserved byte, upper- and lowercase hex digits; the JSON-string escape of `v` (inner part of `serde_json::to_string`); lowercase hex of `v`; each trimmed line of 4 or more characters. Forms shorter than 8 characters are not added (a short fragment would mask ordinary text); a form equal to the value is deduplicated. Residual: R2.

### Pump boundary (security S5)

`engine.rs` `pump` reads `take(65536).read_until(b'\n')` and calls `f` per chunk; `Log::write` redacts each chunk alone, so a value across byte 65536 of one long line escapes. Fix in `pump`: keep the last `L - 1` bytes of each cut chunk (`L` is the longest redactor form, from `Log::longest_secret()`) and prepend them to the next chunk instead of emitting them, flushing the remainder at end of stream. The test puts a canary across byte 65536 of one line.

### Single list of runner keys (devops N6)

`pub const RUNNER_KEYS: [&str; 4]` in `store.rs` (all four are named `FACTORY_CI_*`). `pipeline.rs` `valid_secret_name` refuses exactly these names (as well as the `FACTORY_CI` prefix), `fetch_named` refuses to return them, and `log.rs` uses the list instead of its own `SECRET_KEYS`.

## Files

### Changed

| File | Change |
|---|---|
| `rust-ci-runner/src/store.rs` | Add `RUNNER_KEYS` (replaces the private `KEYS`). Extract `list` from `fetch` (server check, two attempts); `fetch` calls it and parses as before. Add `fetch_named`, `parse_named`, `StepSecrets` (names-only `Debug`), `NamedError { Store(StoreError), Count(String, usize), Unusable(String) }` (derive `Debug`, holds no value) with `Display`, and `usable_secret`. `fetch_named` drops the output buffer and all rows before it returns. |
| `rust-ci-runner/src/pipeline.rs` | Add `Pipeline::secret_names`, `Pipeline::unlisted`, `Pipeline::docker_secret_problems` (any step with secrets and any docker step). Reserve the prefixes `BW_` and `BITWARDEN` (`BWS_` is already there); refuse `RUNNER_KEYS` by name. `resolve_secrets` and `docker_problems` stay (the tick uses them). |
| `rust-ci-runner/src/watch.rs` | Add `WatchEntry::secrets_allowed_on(&self, git_ref: &str) -> bool`. Parsing and defaults do not change. No mention of `bws` or `bitwarden`. |
| `rust-ci-runner/src/redact.rs` | `Redactor::add` with the forms above, plus `longest()`. `new` calls `add`. No mention of `bws` or `bitwarden`. |
| `rust-ci-runner/src/log.rs` | `RwLock<Redactor>`, `add_secrets`, `longest_secret`; public line from fixed grammar; `FAULTS` 14 to 16; use `RUNNER_KEYS`. |
| `rust-ci-runner/src/engine.rs` | `pump` carries the boundary tail. Nothing else. |
| `rust-ci-runner/src/job.rs` | `load_pipeline` becomes `pub(crate)`. |
| `rust-ci-runner/src/ci.rs` | Keep the matched `WatchEntry`, build the `fetch` closure, apply the order above. Clippy allows 7 arguments: pass `allowed`, the entry and `&fetch` in one small private struct without `Debug`. |
| `rust-ci-runner/src/ci_tests.rs` | New `mod step_secrets` at the end (see T005, T014). Change `declared_secret_fails_before_any_container` and the case table of `every_stderr_line_is_in_the_fixed_list`. |
| `rust-ci-runner/.factory-ci.toml` | Split the `ci_tests::` group (T014). |
| unit test files | `store_tests.rs`, `watch_tests.rs`, `secret_names_tests.rs`, tests modules in `pipeline.rs`, `redact.rs`, `log.rs`, `engine.rs`. |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | `0.2.0` to `0.3.0`. |
| `.github/workflows/factory-monorepo-ci.yml` | The `RUNNER_IMAGE` line only. |
| `.github/factory-ci-repos.toml` | Header comment; names only if FR-016 says so. |
| `README.md`, `rust-ci-runner/README.md` | Last task. |

### Added

`rust-ci-runner/tests/watch_file.rs` (reads `../.github/factory-ci-repos.toml` with `watch::read`: no name in two entries, no `secrets` on `sudtanj/infrastructure`). It runs in `factory-ci-selftest.yml` (`--test '*'`, full checkout). It is not added to the integration command of `.factory-ci.toml`: that pipeline mounts only the project directory, so `../.github` does not exist there. Spec artifacts: `research.md`, `data-model.md`, `contracts/ci-secrets.md`, `quickstart.md`, `tasks.md`. No new file under `src/`.

### Not changed, and why

- `watch.rs` parsing: `valid_secret_name` already refuses reserved names, `FACTORY_CI*` included.
- `.github/scripts/test-factory-monorepo-ci.sh`: line 89 matches any `X.Y.Z@sha256:<64 hex>`; lines 56 to 58 assert only `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN`. No edit. It runs in the selftest `workflow` job only on a branch push or pull request, which shapes T017.
- `engine.rs` `run_logged`: already passes `-e NAME` with the value in the child environment, only for the step's own names.
- `.github/scripts/factory-ci-secrets.sh` and the tick.

## Data and API contracts

See [contracts/ci-secrets.md](contracts/ci-secrets.md) (watch entry, pipeline step, lookup interface v1, public lines) and [data-model.md](data-model.md). No new flag, environment variable, workflow input or file format.

## Engineer briefs

Order. T001 to T007 (tests) run in parallel; T005 is one file. T008 to T012 run in parallel once their tests exist. T013 needs T008 to T012. T014 and T015 any time. T016 to T022 are sequential after the code is on `main`. T023 is last. IDs match `tasks.md`.

### T008 Redactor and log (FR-011, FR-012, FR-013)
- Goal: a log learns values after it exists, masks every form in FR-011, and prints public lines from fixed grammar.
- Files: `rust-ci-runner/src/redact.rs`, `rust-ci-runner/src/log.rs`.
- Acceptance: `Redactor::add` masks the forms in "Redactor forms"; `add(a); add(b)` masks what `new(a + b)` masks. `Log::add_secrets(&self, values)` and `Log::longest_secret()` exist; text written before the call is unchanged, after it masked; size limit and cut flag keep state. A secret `pass` leaves `[runner] pipeline passed` intact on stdout while Loki holds the redacted text. `FAULTS.len() == 16` with both new strings; `fault` still drops other text. `log.rs` uses `RUNNER_KEYS`.
- Constraints: no `bws` or `bitwarden` in `redact.rs`. A poisoned lock recovers with `into_inner`. No new crate. Forms under 8 characters are skipped.

### T009 Named lookup (FR-005 to FR-007, FR-018)
- Goal: read the values of named keys, all or an error, behind interface v1.
- Files: `rust-ci-runner/src/store.rs`.
- Acceptance: `fetch_named(bws, token, server, limit, names)` runs `bws secret list -o json` with the cleared child environment, token, server check and one retry as `fetch`. A name fails when its exact key matches 0 or 2 or more rows (`Count`), when it is in `RUNNER_KEYS`, or when its value is unusable (`Unusable`): empty or blank; any control character other than `\n` and `\r\n` (lone `\r`, NUL, tab, `\x0b`, `\x0c`, U+0085, U+2028, U+2029); any non-blank trimmed line under 4 characters. A blank inner line passes; exactly 4 passes, 3 fails. A row without a string `key` or non-JSON output is `Store(Unreachable)`; a rejected token is `Store(Rejected)`; output over 16 MiB is `Store(Unreachable)`. The output buffer and all rows are dropped before return. A name repeated in `names` is looked up once. `{:?}` and `Display` of `StepSecrets` and `NamedError` print no value. `fetch` and `parse_list` behave as before.
- Constraints: extract the shared retry and server check, do not copy. Values pass untouched (no trimming, no normalisation). No `derive(Debug)` on anything that holds a value or token.

### T010 Pipeline helpers (FR-001 to FR-004, FR-007, FR-009)
- Goal: ask a parsed pipeline what it requests and what breaks the rules.
- Files: `rust-ci-runner/src/pipeline.rs`.
- Acceptance: `secret_names` returns each name once, sorted. `unlisted(allowed)` returns every `(step, name)` not byte-equal to an entry of `allowed`. `docker_secret_problems` returns a text when any step lists secrets and any step has `docker = true`, with two separate steps and with one step, whatever the flags. `valid_secret_name` also refuses the prefixes `BW_` and `BITWARDEN` (case-insensitive, like the others) and every `RUNNER_KEYS` name; `BWS_` stays.
- Constraints: leave `resolve_secrets` and `docker_problems` alone.

### T011 Pump boundary (FR-013)
- Goal: a value across the 64 KB read cut is masked.
- Files: `rust-ci-runner/src/engine.rs`.
- Acceptance: with a redactor holding a value of length `n`, a line with the value across byte 65536 reaches the log masked; ordinary output is unchanged in content and order; the tail is flushed at end of stream; a UTF-8 character is never cut.
- Constraints: `pump` stays generic over `Read`; the longest-form length comes from the caller.

### T012 Ref gate (FR-010)
- Goal: secrets only reach refs the entry names on purpose.
- Files: `rust-ci-runner/src/watch.rs`.
- Acceptance: `secrets_allowed_on(ref)`: if `secret_branches` is set, true only when the ref matches one of them; otherwise true only when `branches` has no pattern made only of `*` (so the default and `["*"]` give false) and the ref matches `branches`.
- Constraints: parsing and the `WatchEntry` fields do not change. No `bws` or `bitwarden`.

### T013 Wire `ci` (FR-001 to FR-013, FR-018)
- Goal: `ci` runs a pipeline with its allowlisted secrets.
- Files: `rust-ci-runner/src/ci.rs`, `rust-ci-runner/src/job.rs` (`load_pipeline` visibility).
- Acceptance: the order in "Order inside `run_checkout`" holds. The `fetch` closure is built in `job()` from `token` (Bitwarden), `s.bws`, `server` and `s.limits.bws`; `run_checkout` never receives a token. Faults print exactly `[runner] step secret not allowed`, `[runner] step secret unavailable`, `[runner] repository or ref not in the watch list` (gate), `[runner] docker steps are refused`; exit 1; no `docker run`, `network create` or `volume create` call. Not allowed and gate failures: no second `bws` call. Detail lines (step `ci`) carry names and counts and no value; stdout and stderr carry none. `log.add_secrets` runs with every value before `run_pipeline`. Each step sees only its own names. A pipeline with no secrets makes exactly one `bws` call.
- Constraints: no `set_var`; the server URL comes through the `env` closure. No `derive(Debug)` on a struct that holds the closure, token or values. `make lint` (clippy `-D warnings`) stays clean without a blanket `allow`.

### T014 Split the test group (devops S3)
- Goal: the new tests cannot exhaust the 512-process limit of the step container.
- Files: `rust-ci-runner/.factory-ci.toml`.
- Acceptance: the `unit` step runs `cargo test --locked --lib ci_tests:: -- --skip ci_tests::step_secrets::` and a separate `cargo test --locked --lib ci_tests::step_secrets::`; the first command (`--skip ci_tests::`) is unchanged; the header comment says why.
- Constraints: Evidence for the split: each test builds a `tiny_http` server with an accept thread (`ci_tests.rs` `hub`); `Hub::drop` calls `server.unblock()`, so the accept thread ends, but about 46 existing and about 30 new tests share one process and each also spawns `bws`/`docker` readers. The split is cheap, so it is done anyway. If the pipeline still hits the limit, add `-- --test-threads=4`.

### T015 Version (FR-014)
- Goal: the crate says `0.3.0`.
- Files: `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`.
- Acceptance: both say `0.3.0` for package `rust-ci-runner`; two changed lines.
- Constraints: by hand; touch no other lock entry (`--locked`).

### T016 Release gate and tag (FR-014)
- Goal: a published 0.3.0 image from a verified commit.
- Files: none.
- Acceptance: code from T008 to T015 is on `main`; `make lint` (clippy `-D warnings`) and the selftest passed on it; the `rust-ci-runner` pipeline was dispatched on `main` and passed; the task records that run's id and its full SHA; the SHA equals `git rev-parse main` at tag time (re-run if `main` moved); tag `rust-ci-runner-v0.3.0` is on that commit; `rust-ci-runner-release.yml` finished, smoke step included; the digest is recorded and checked with `docker pull`.
- Constraints: the owner or lead pushes the tag. Never move a tag; a fix ships as 0.3.1.

### T017 Pin (FR-015)
- Goal: the workflow runs 0.3.0.
- Files: `.github/workflows/factory-monorepo-ci.yml`.
- Acceptance: the pin commit goes to a branch first, so `factory-ci-selftest.yml` (which ignores pushes to `main`) runs `test-factory-monorepo-ci.sh` in its `workflow` job and passes; then fast-forward `main`. The `RUNNER_IMAGE` line reads `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest from T016>` and nothing else changes. `RUNNER_IMAGE_DIGEST` (tick variable) is not bumped.
- Constraints: separate commit after the digest exists. No placeholder digest.

### T018 No-secret canary (devops S5)
- Goal: prove 0.3.0 works before any secret exists.
- Files: none.
- Acceptance: `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner` on the new pin ends in `[runner] pipeline passed`.
- Constraints: on failure revert the pin (rollback list) before going on.

### T019 Preconditions re-check (FR-019, spec R1)
- Goal: the access facts still hold before the first secret run.
- Files: none (record the result in the task).
- Acceptance: with `gh api` (read-only): Environment `factory-ci` has custom-branch policy with exactly the policy `main` and no required reviewer (record that, as of 2026-11-28 it was so); the only collaborator of `sudtanj/infrastructure` and `sudtanj/factory-monorepo` is the owner; `factory-monorepo` has no `.github/dependabot.yml`, no Renovate config and no auto-merge on `main`; the tag ruleset for `rust-ci-runner-v*` exists; the GHCR package is readable by the workflow; the machine account's read grant covers only the one project.
- Constraints: an agent runs only these `gh api` reads. No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or any Environment secret, or creates or reads a Bitwarden key.

### T020 Watch entry (FR-016)
- Goal: the header comment is true; names only where needed.
- Files: `.github/factory-ci-repos.toml`.
- Acceptance: the comment says `ci` reads `secrets` as the per-repo allowlist and applies the ref gate of FR-010. The `sudtanj/factory-monorepo` entry gains only non-vault names the browser project needs: today none, or `BROWSER_TEST_URL` if the owner treats the URL as secret. No name is added if none is needed. `sudtanj/infrastructure` never gets `secrets`. `tests/watch_file.rs` and `plan --check` pass.
- Constraints: names only, no value; secret names are public in this file, so no claim of name secrecy. Login names come from feature 011.

### T021 Canary values run (FR-019, security S8)
- Goal: see what Loki shows for a value before a real value exists.
- Files: `.github/factory-ci-repos.toml` (a temporary canary name, removed in this task).
- Acceptance: the owner creates a Bitwarden key with a made-up value, lists the name in a step of the `factory-monorepo` pipeline on `main` (that repo's change) and in the watch entry; the step prints the value plain, as `echo "$V" | base64` and percent-encoded; the run ends in `pipeline passed` with only fixed lines in the Actions log; Loki (query in `quickstart.md`) shows `***` for all three; the name is then removed from the entry and the key deleted.
- Constraints: owner only for every Bitwarden action; an agent reads the Actions log and Loki only.

### T022 First real run (SC-001)
- Goal: a real step secret works end to end.
- Files: none.
- Acceptance: only after T019 to T021 and only when a real step secret exists (feature 011 delivers the login values); `factory-monorepo-ci` on `main` ends in `[runner] pipeline passed`, Actions log fixed lines only, Loki masked. If there is no real secret yet, this task waits for 011 and 010 is complete without it.
- Constraints: the same rules about Bitwarden as T021.

### T023 Docs (FR-017), last
- Goal: both READMEs and the GitHub description match.
- Files: `README.md` (line 55, the "Steps get no pipeline secrets here" sentence), `rust-ci-runner/README.md` (the `ci` steps 2 and 5, the minimal-output paragraph and fault list, the Secrets section, risks), repository description with `gh repo edit` (needs admin).
- Acceptance: states the allowlist (per repo, not per project), the lookup rule, the new lines, the always-refused names (four settings, `BW_`, `BITWARDEN`), no docker steps in a pipeline with secrets, the ref gate and that `branches = ["*"]` or `secret_branches = ["*"]` set on purpose opens it, masking forms and limits, shared names across repos, `sudtanj/infrastructure` never gets `secrets`, secret names are public, steps share `/workspace` (pin images by digest, `npm ci --ignore-scripts`), the tick also reads the names, the runbook line for `step secret unavailable` (check the key exists once with that exact name in the project; a failed call retries once with no backoff, so start the run again), and the rollback list. No value appears.
- Constraints: stop-slop applies.

## Test plan (qa writes it first)

Unit tests sit in the modules named. `ci::run` integration tests sit in `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs` (`use super::*;`, so the private `W` harness is reused). The `WATCH` constant has `branches = ["*"]` for `o/r`, which FR-010 refuses for secrets: tests overwrite `w.watch` (`repos.toml`) with an entry that has `branches = ["main"]` and `secrets = [...]`, and add store rows by pushing `(key, value)` into `w.secrets`; `W::bootstrap()` feeds those values into the leak searches. Use a unique canary per value.

| ID | File | Test | FR / story |
|---|---|---|---|
| T001 | `store_tests.rs` | `named_lookup_returns_values_by_exact_key`; `..._missing_key_is_count_0`; `..._duplicate_key_is_count_2`; `..._key_differing_in_case_does_not_match` | FR-001, FR-003, FR-005, FR-006 |
| | | `named_lookup_refuses_empty_blank_and_each_control_character` (NUL, tab, lone `\r`, `\x0b`, `\x0c`, U+0085, U+2028, U+2029) | FR-006 |
| | | `named_lookup_line_boundary` (3 fails, 4 passes, blank inner line passes, `\r\n` passes) | FR-006, FR-007 |
| | | `named_lookup_passes_unicode_and_equals_untouched`; `..._does_not_need_the_four_runner_keys`; `..._refuses_a_runner_key_name`; `..._looks_up_a_repeated_name_once` | edge cases, FR-004 |
| | | `fetch_named_over_16_mib_is_unreachable` | FR-006 |
| | | `fetch_named_retries_once_and_a_rejected_token_is_final`; `..._refuses_a_foreign_server_url`; `..._child_env_is_cleared` | FR-005 |
| | | `debug_and_display_of_results_and_errors_hold_no_value` | FR-018 |
| T002 | `pipeline.rs` tests | `unlisted_*` (all allowed, each miss, empty allowlist, case difference, one allowed one listed); `secret_names_returns_each_name_once_sorted` | FR-001 to FR-003, FR-007 |
| | | `docker_secret_problems_flags_secrets_with_any_docker_step` (one step; two separate steps; neither; flags irrelevant) | FR-009 |
| | | `each_runner_setting_is_refused_in_a_step` | FR-004 |
| T003 | `watch_tests.rs` | `each_runner_setting_in_secrets_refuses_the_file` | FR-004 |
| | | `secrets_allowed_on_*`: `branches` omitted, `branches = ["*"]`, `["main"]`, `["release/*"]`, `secret_branches` set and matching, set and not matching, set to `["*"]` | FR-010 |
| | `secret_names_tests.rs` | rows for `BW_X`, `BITWARDEN`, `bitwarden_x` refused in a watch entry and a pipeline | FR-004 |
| T004 | `redact.rs` tests | `add_masks_value_and_base64_forms` (`echo "$V" \| base64` with trailing newline, `printf 'x%s' "$V" \| base64`, wrapped at 76, URL-safe, unpadded) | FR-011, FR-013 |
| | | `add_masks_percent_json_and_hex_forms`; `add_keeps_earlier_values`; `add_ignores_empty_short_lines_and_short_forms` | FR-011 |
| | `log.rs` tests | `add_secrets_masks_later_lines_only`; `add_secrets_keeps_size_limit_state`; `a_secret_named_pass_cannot_mangle_a_public_line`; update `fault_accepts_only_the_fixed_list` (16) | FR-011, FR-012 |
| T005 | `ci_tests.rs` `step_secrets::` | `step_secrets_reach_only_the_step_that_lists_them` (`docker.calls` has `-e NAME` and no value; `docker.envs` of that run has it, the other run not; `bws_calls() == 2`) | US1, FR-001, FR-008 |
| | | `pipeline_without_secrets_makes_one_bws_call` | I8 |
| | | `second_bws_call_uses_the_access_token_not_the_clone_token` (`bws.envs`: both blocks `BWS_ACCESS_TOKEN` equal; the clone token absent) | FR-005, security S3 |
| | | `second_bws_call_carries_the_server_url_and_a_cleared_env` | FR-005 |
| | | `unlisted_name_fails_before_any_container` (stderr exactly `[runner] step secret not allowed\n`, `started_containers() == 0`, `bws_calls() == 1`, name in a Loki body only) | FR-002, FR-012, US2 |
| | | `entry_without_secrets_key_refuses_a_requested_name`; `name_differing_in_case_is_not_allowed`; `single_allowed_name_runs` | FR-002, FR-003 |
| | | `runner_setting_in_the_pipeline_is_refused` (four rows) and `runner_setting_in_the_entry_refuses_the_watch_file` (four rows, `bws_calls() == 0`) | FR-004 |
| | | `unavailable_value_ends_the_run_before_any_container` (missing, duplicate, empty, blank, NUL, 3-char line; `step secret unavailable`; zero containers; name and count in Loki) | FR-006, US3 |
| | | `four_char_value_runs`; `blank_inner_line_value_runs_and_masks_each_line` | FR-007, FR-011 |
| | | `two_steps_listing_one_name_look_it_up_once`; `second_bws_failure_is_unavailable` | FR-005, FR-007 |
| | | `docker_step_in_a_pipeline_with_secrets_is_refused` (two separate steps; `--allow-docker` and `allow_docker = true`; `docker steps are refused`; zero containers) | FR-009 |
| | | `secrets_with_branches_omitted_or_star_are_refused_before_bws` (two rows; line `repository or ref not in the watch list`; `bws_calls() == 1`, the first call only) | FR-010 |
| | | `secret_branches_must_match_the_ref`; `explicit_branches_main_gets_secrets`; `a_pipeline_without_secrets_runs_on_branches_star` | FR-010 |
| | | `hostile_step_cannot_print_a_secret_anywhere` (plain, `echo "$V" \| base64`, `printf 'x%s'` base64, percent-encoded, inside a longer line; search stdout, stderr, `docker.calls`, `bws.calls`, `git.calls`, `hub.bodies()`; Loki holds `***`) | FR-011, FR-013, US4, SC-002 |
| | | `a_secret_named_pass_keeps_pipeline_passed_on_stdout` | FR-012 |
| | | `no_public_line_holds_a_secret_name_or_count` | FR-012 |
| | `ci_tests.rs` (existing group) | change `declared_secret_fails_before_any_container` to expect `step secret not allowed`; add two rows to `every_stderr_line_is_in_the_fixed_list` | FR-012 |
| T006 | `engine.rs` tests | `pump_masks_a_value_across_the_64k_cut`; `pump_keeps_order_and_flushes_the_tail`; `pump_never_cuts_a_utf8_char` | FR-013 |
| T007 | `tests/watch_file.rs` | `no_secret_name_in_two_committed_entries`; `infrastructure_entry_has_no_secrets` | FR-016 |

SC-003 is the `started_containers()` assert in the refusal tests. SC-001 and SC-004 are manual. No existing file under `rust-ci-runner/tests/` changes. `tests/no_vendor.rs` must pass: `bws` and `bitwarden` stay in `store.rs`, `ci.rs`, `pipeline.rs`, `log.rs`, `engine.rs`, `main.rs`, not in `redact.rs` or `watch.rs`.

Script check: `.github/scripts/test-factory-monorepo-ci.sh` runs in the selftest `workflow` job for the pin commit pushed to a branch (T017).

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| docker rules | any docker step in a pipeline with secrets | `docker_secret_problems` | `docker steps are refused` | `docker_step_in_a_pipeline_with_secrets_is_refused` |
| allowlist | name not in entry, no `secrets` key, case difference | `unlisted` | `step secret not allowed`, no second call | `unlisted_name_fails_before_any_container` |
| ref gate | `branches` omitted or `*`, ref not in `secret_branches` | `secrets_allowed_on` | `repository or ref not in the watch list`, no second call | `secrets_with_branches_omitted_or_star_are_refused_before_bws` |
| pipeline load | runner setting, `BW_`, `BITWARDEN` name | `valid_secret_name` | `pipeline file refused` | `runner_setting_in_the_pipeline_is_refused` |
| watch read | runner setting in the entry | `watch::parse` | `repository or ref not in the watch list` | `runner_setting_in_the_entry_refuses_the_watch_file` |
| lookup | `bws` down, timeout, rejected token on call 2 (retry once, no backoff) | `Store(..)` | `step secret unavailable` | `second_bws_failure_is_unavailable` |
| lookup | project list over 16 MiB | cut output fails to parse | `step secret unavailable` | `fetch_named_over_16_mib_is_unreachable` |
| lookup | wrong token used for call 2 | token equality assert | would be `Rejected`; caught in test | `second_bws_call_uses_the_access_token_not_the_clone_token` |
| lookup | key missing, duplicate, unusable value | `Count`, `Unusable` | `step secret unavailable`, name and count to Loki | `unavailable_value_ends_the_run_before_any_container` |
| redaction | value in a listed form reaches a line | canary search | `***` in Loki | `hostile_step_cannot_print_a_secret_anywhere` |
| redaction | value across the 64 KB cut | pump test | masked | `pump_masks_a_value_across_the_64k_cut` |
| public line | value such as `pass` mangles a fixed line | test | line intact | `a_secret_named_pass_keeps_pipeline_passed_on_stdout` |
| run | huge value refused by the OS | spawn fails | existing `runner fault` | none (accepted) |
| run | step transforms a value beyond FR-011 | not detected | unmasked in Loki (R2) | none; README |
| test run | 512-process limit hit by the test group | pipeline step fails | split group (T014) | the pipeline itself |
| release | tag SHA differs from the tested SHA, lint fails | T016 checks | no tag until re-run | T016 |
| rollout | pin before digest exists | pull fails | 0.2.0 pin works until then | T017 order |
| rollout | pin bad | T018 canary fails | revert the pin commit | T018 |
| rollout | first lookup fails for a missing key | `step secret unavailable` | owner creates the key with that exact name and starts again | quickstart |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. A value reaches Loki or stdout through a form the redactor missed or a boundary. Prevented: the FR-011 forms, the pump tail, `add_secrets` before `run_pipeline`, the canary run T021 before any real value, the hostile tests.
2. Secrets reach a branch nobody meant (default `branches = ["*"]`, `secret_branches` ignored). Prevented: FR-010 gate with its tests; the explicit `["*"]` case needs a deliberate owner edit and the README says so.
3. The wrong token is used for the lookup (`sec.token` is the clone token) and every secret run fails with `secret store rejected`-class faults. Prevented: the closure is built in `job()` from `token`; the equality test; the canary run T018 and T021.
4. The release or rollout breaks: lock and manifest disagree under `--locked`; the pin is bad; the tested SHA is not the tagged SHA. Prevented: T015 touches two lines; T016 gate records and compares SHAs and runs lint; T017 pushes to a branch for the shell tests; T018 canary; rollback list.
5. A public line leaks a name or count, or a secret like `pass` mangles the grammar. Prevented: `Log::fault` drops text outside `FAULTS`; details use `log.out`; public lines come from fixed grammar; tests for both.

## Invariants (qa turns these into property tests)

- I1: no container starts unless every requested name is in `entry.secrets`, has exactly one valid value, and the ref gate passed.
- I2: for any set of values and any listed form, no byte of stdout, stderr, any child argv or any Loki body holds the raw value or a form from FR-011, once the lookup finished.
- I3: a step's container receives exactly `step.secrets` as `-e NAME` and no other store key.
- I4: `unlisted(p, allowed)` is empty if and only if `secret_names(p)` is a subset of `allowed`; growing `allowed` never adds a miss.
- I5: whatever the allowlist, no name in `RUNNER_KEYS` reaches a step environment.
- I6: stdout lines belong to the existing grammar and stderr lines to `FAULTS` (16 entries), whatever the secret values are; no line holds a secret name.
- I7: `Redactor::add(a)` then `add(b)` masks the same text as `new(a + b)`.
- I8: a pipeline with no step secrets makes exactly one `bws` call and behaves as before.
- I9: the output of `pump` equals its input with masked forms replaced, whatever the chunk cuts.
- I10: the second `bws` call carries the same access token as the first.

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: empty allowlist, duplicate names, case differences, empty, blank, control-character, short-line, blank-inner-line, multi-line, Unicode and `=` values, one name in two steps, `branches` omitted: each has a test row.
- State and order: no state kept; lookup finishes before the first container; a crash leaves only the checkout the existing cleanup removes.
- Timing: second call has the same 60 s limit and one retry, no backoff; the 30-minute run deadline starts later in `run_pipeline`.
- Concurrency: concurrent runs read the store independently; the workflow's queue group is unchanged.
- Resources: values live as `String` for the run; a huge value fails the spawn into `runner fault`; a project list over 16 MiB ends in `step secret unavailable`; the test group is split for the process limit.
- Permissions and trust: the watch file on `main` decides names and refs; the pipeline cannot widen it; runner settings never pass; names are public.
- Failure and rollback: every failure stops before a container; rollback list below.
- Output and UI: no UI; two new closed lines; public lines from fixed grammar.
- Compatibility: pipelines with no secrets behave as before; a secret-listing pipeline now ends in `step secret not allowed` instead of `pipeline file refused` (R6).

## Rollout and rollback

Rollout:

1. Land T001 to T015 on `main`; `make lint` and the selftest pass; the `rust-ci-runner` pipeline passes on `main` (T016 records its run id and full SHA).
2. Tag `rust-ci-runner-v0.3.0` on that SHA (T016). Read the digest.
3. Pin commit to a branch, selftest green, fast-forward `main` (T017).
4. No-secret canary run on `sudtanj/infrastructure` (T018).
5. Re-check the preconditions (T019).
6. Comment and conditional names (T020).
7. Canary values run (T021), then the first real run when 011 provides a value (T022).
8. Docs last (T023).

Rollback, in this order:

1. Revert the pin commit (`factory-monorepo-ci.yml` returns to 0.2.0). 0.2.0 refuses step secrets.
2. `RUNNER_IMAGE_DIGEST` (the tick's variable) is not bumped for 0.3.0, so nothing to revert there.
3. The release workflow moved `:latest` to 0.3.0 (`rust-ci-runner-release.yml`, tag latest step). Moving it back needs owner approval: `docker buildx imagetools create --tag ghcr.io/sudtanj/rust-ci-runner:latest ghcr.io/sudtanj/rust-ci-runner@<0.2.0 digest>`. Nothing in this repo pulls `latest`.
4. Fixes ship as 0.3.1. Never move or delete the 0.3.0 tag.
5. Remove the names from the watch entry.
6. Rotate the values if a leak is suspected (the owner, in Bitwarden), and delete the canary key.

## Devops assessment

Findings from the lead's devops review and how each is handled.

| Finding | Handling | Where |
|---|---|---|
| S1 `bws secret list` has no filter; whole project is fetched | Reworded: second call fetches the whole project, filters in `parse_named`, drops the list before return; 16 MiB cap gives `Unreachable` | Architecture decision, research D1, T009, tests |
| S2 wrong token in the T009 brief (`sec.token` is the clone token) | Confirmed in `ci.rs` (`steps(&a, want_id, &sec.token, ..)`; `job()` holds `token`). Closure built in `job()`; equality test | Lookup closure, T013, T005 |
| S3 `ci_tests` group may exhaust `--pids-limit 512` | Partly wrong: `Hub::drop` calls `server.unblock()`, so the accept thread ends. Split anyway: `mod step_secrets` in its own run with a matching `--skip` | T014, T005 |
| S4 rollback: `:latest` moved | Rollback list with `imagetools` step, owner approval, 0.3.1, no tag moves, digest variable untouched, rotation | Rollback |
| S5 no-secret canary before names | T018 | Rollout |
| S6 selftest ignores pushes to `main` | Pin commit goes to a branch first, then fast-forward | T017 |
| S7 tag gate | T016 records run id and full SHA, equals tagged SHA, re-run if `main` moved, clippy `-D warnings` through `make lint` | T016 |
| Notes: no retry backoff | Runbook line in README | T023 |
| Notes: Loki query | In quickstart | quickstart |
| Notes: GitHub preconditions | Environment, tag ruleset, GHCR readable, `gh repo edit` needs admin, machine account read grant | T019, T023 |
| Notes: Chromium `/dev/shm` | Belongs to `factory-monorepo` 002; out of scope | spec Non-goals |
| N6 one `RUNNER_KEYS` list; drop list buffer; `RwLock` for 011 | Done | Single list, T009, T008 |
| B1 Environment and collaborator facts | Recorded: on 2026-11-28 (`gh api`) Environment `factory-ci` has a custom-branches deployment policy with exactly one policy, `main`, no required reviewer; the only collaborator of `sudtanj/infrastructure` and `sudtanj/factory-monorepo` is the owner. R1 rewritten to "owner-only write access, Environment limited to `main`". A task re-checks it, and checks no Dependabot or Renovate auto-merge on `factory-monorepo` `main` | spec R1, T019 |

## Security assessment

| Finding | Handling | Where |
|---|---|---|
| S1 default `branches = "*"`, `secret_branches` ignored | FR-010 gate before any lookup; line `repository or ref not in the watch list` | Order step 5, T012, T013, tests |
| S2 dind is privileged | Any docker step in a pipeline with secrets is refused | FR-009, T010 |
| S3 token reaches `run_checkout`; `Debug` leaks | Closure in `job()`; no `Debug` on holders; `StepSecrets` Debug names only; interface v1 | Lookup closure, T009, T013, contracts |
| S4 redactor misses `echo "$V" \| base64` and other forms | Forms added; hostile tests use those commands | Redactor forms, T008, T004 |
| S5 pump chunk boundary | Carry the tail | T011, T006 |
| S6 value rule and public-line mangling | Control characters refused, blank inner lines allowed, public line from fixed grammar | T009, T008 |
| S7 reserve `BW_`, `BITWARDEN` | `BWS_` was already reserved (`pipeline.rs` prefixes); `BW_` and `BITWARDEN` added | T010, T003 |
| S8 canary run, rotation, agent rules | T021, rollback step 6, tasks.md rules; quickstart section 4 and T020 to T022 made consistent | tasks.md, quickstart |
| S9 no name in two entries; infrastructure never gets secrets | `tests/watch_file.rs`, README line | T007, T023 |
| N1 parse `bws` output only in `store.rs`, drop rows | Done | T009 |
| N2 shared `/workspace` | In R1 and README | spec R1, T023 |
| N3 `NamedError` has no value in `Display` or `Debug` | Test | T001 |
| N4 names are public | Stated; no claim of name secrecy | spec, T020 |
| N6 allowlist is per repo | In the README | T023 |

No finding was rejected. S3 of the devops list is accepted in effect and corrected in fact (see above).

Infra notes: no new workflow, input, Environment secret or variable. The runner container holds the docker socket and the one access token on an ephemeral GitHub-hosted VM.

## Open risks

- Tabs inside a value are refused (control character), as `store.rs` `usable` does today. A value with a tab needs the rule relaxed.
- `RwLock` and the public-line change touch `Log`, which `run`, `job` and `ci` share; `job_tests.rs` and `log.rs` tests must stay green.
- The redactor skips forms under 8 characters, so a short secret transformed by a step can leak (R2).
- `--lib` tests run only on `main` in the runner pipeline (`branches = ["main"]`); the selftest only compiles them. A broken lib test shows after landing; T016 runs the pipeline before the tag.
- `tests/watch_file.rs` runs in the selftest only, not in the runner pipeline (no `.github` in its mount).
- The temporary canary name in T021 touches the `factory-monorepo` pipeline file, a change in that repo.
- Feature 011 may change names and the source of login values; T020 and T022 are conditional for that reason.

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
rust-ci-runner/tests/watch_file.rs
rust-ci-runner/.factory-ci.toml  Cargo.toml  Cargo.lock
.github/factory-ci-repos.toml  .github/workflows/factory-monorepo-ci.yml
README.md  rust-ci-runner/README.md
```

**Structure Decision**: one existing crate; no new module under `src/`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| `Log` redactor behind an `RwLock` | Secret names are known only after the log exists; values must join without a second log; feature 011 adds values later | A second `Log` resets the 10 MB limit and cut state |
| `StepSecrets` newtype with names-only `Debug` | Security S3: a stray `{:?}` of the map must not print values | A bare `BTreeMap` prints its values |
| Extra redactor forms and the pump tail | Security S4, S5: a step can print `echo "$V" \| base64`; a long line can straddle the read cut | Plain value and plain base64 miss both |
| Public line from fixed grammar | Security S6: a value such as `pass` breaks `pipeline passed` | Printing the redacted text does that |
