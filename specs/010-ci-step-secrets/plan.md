# Implementation Plan: Step secrets for the ci command

**Branch**: `010-ci-step-secrets` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/010-ci-step-secrets/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. The constitution (Principle VI) also asks for `/speckit-clarify` before this plan and `/speckit-analyze` after `tasks.md`. The spec carries a clarification session; `/speckit-analyze` is still to run before `/speckit-implement`.

## Summary

`ci` gives a step the Bitwarden secrets it lists, when the run's watch entry allows the name. After the fetch, the runner checks every step's names against `WatchEntry.secrets`, then makes one more `bws secret list` call with the same token and keeps only the keys named. It validates every value, adds all values to the log's redactor, and hands the map to `run_pipeline`. Any failure ends the run before the first container with one of two new fixed public lines. The engine already passes values as `-e NAME` with the value in the child environment, and the name rules already refuse the four runner settings, so the change is small: one lookup function, three pipeline helpers, one redactor extension and the `ci.rs` wiring.

## Technical Context

**Language/Version**: Rust 2021, toolchain from `rust-ci-runner/rust-toolchain.toml` (pipeline image `rust:1.90`)

**Primary Dependencies**: no new crate. `serde_json`, `toml`, `ureq`, std only.

**Storage**: none. No state between runs.

**Testing**: `cargo test --locked --lib` (unit, and `ci_tests.rs` driving `ci::run` with stub `bws`, `docker`, `git` and a `tiny_http` Loki), `cargo test --locked --test ...` for the integration binaries. Both run in the `rust-ci-runner` pipeline (`rust-ci-runner/.factory-ci.toml`); `factory-ci-selftest.yml` compiles them and runs the integration binaries.

**Target Platform**: the `ghcr.io/sudtanj/rust-ci-runner` image, linux/amd64 and arm64, started by `factory-monorepo-ci.yml`.

**Project Type**: CLI (one binary, `ci` subcommand).

**Performance Goals**: one extra `bws` call (60 s limit per attempt, one retry) only when a step lists a secret. A pipeline with no secrets makes the same single call as today.

**Constraints**: values only in the environment of the engine child, never argv; public stdout and stderr stay a closed grammar; names and counts to Loki only; this VM has 1 GB, so nobody builds locally.

**Scale/Scope**: at most 20 allowlisted names per entry (`watch.rs` `MAX_LIST`), at most 100 steps.

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Runner and watch file stay here. |
| II Secrets source | Pass. Pipeline secrets come from Bitwarden through the existing token. The workflow gains no secret and no input. No value is committed; docs name keys only. |
| III Public logs | Pass by design: two fixed lines, no name or count, redactor filled before any step (FR-011, FR-012). The hostile test in T005 searches every output for canaries. |
| IV Simplicity | Pass. No new crate, no new watch key, no new module. One justified addition in Complexity Tracking. |
| V Tested | Pass. Every FR maps to a test in the Test Plan; tests are written first (T001 to T005). |
| VI Spec-driven | Pass for plan and tasks; `/speckit-analyze` runs next. |

Re-check after design: unchanged.

## Architecture decision

Chosen: a second, filtered `bws secret list` call inside `ci.rs`, after the fetch, only when a step lists a name.

Why a second call. The pipeline file arrives with the checkout, after the first call already ran. Keeping the first call's full list in memory would hold every other repo's values in the process for the whole run and would break FR-002 ("read no value for any name"). A second call after the allowlist check reads nothing when the check fails, costs nothing for pipelines without secrets, and reuses `attempt`, the retry rule, the server-URL check and the cleared child environment of `store.rs`.

Alternatives rejected:
- Reuse the first call and keep the allowlisted rows. It holds values before the pipeline asked for them. Rejected for the FR-002 reading above.
- Build a second `Log` with a bigger Redactor, as `job()` does for the first one. It resets the 10 MB size limit and the cut flag, so a run could print twice the limit. Rejected: `Log` holds its redactor in an `RwLock` and gets `add_secrets`.
- Per-key `bws secret get`. It needs secret ids, which only the list gives. Rejected.
- Resolve in the workflow with `factory-ci-secrets.sh`. The constitution keeps the workflow to the one access token, and the script cannot run inside the container. Rejected.
- Repo-own keys `owner/name<SEP>NAME`. Out of scope (spec Clarifications).

Order inside `run_checkout` (one place, so the order is testable):

1. Load the pipeline from the checkout. Any parse fault is `pipeline file refused`; this covers the four runner settings and every other reserved name.
2. `docker_problems` plus the new `docker_secret_problems`: `docker steps are refused`. FR-009 holds even with `--allow-docker`.
3. `unlisted(allowed)` not empty: details to Loki, fault `step secret not allowed`, no `bws` call.
4. No names requested: skip the lookup (today's path).
5. `store::fetch_named`; on any error: details (names, counts) to Loki, fault `step secret unavailable`.
6. `log.add_secrets(values)`.
7. `run_pipeline(.., secrets, ..)`.

Docker is checked before the allowlist. The spec does not order the two refusals; a step that breaks both gets `docker steps are refused`.

## Files

### Changed

| File | Change |
|---|---|
| `rust-ci-runner/src/store.rs` | Extract `list` from `fetch` (server check, two attempts, returns the JSON); `fetch` calls it and parses as before. Add `pub fn fetch_named(bws_bin, token, server_url, limit, names: &[String]) -> Result<BTreeMap<String, String>, NamedError>`, `pub fn parse_named(json, names)`, `pub enum NamedError { Store(StoreError), Count(String, usize), Unusable(String) }` with `Display` (names and counts only), and `fn usable_secret(v: &str) -> bool`. `parse_list`, `StoreError` and the four-key rules do not change. |
| `rust-ci-runner/src/pipeline.rs` | Add `Pipeline::secret_names(&self) -> Vec<String>` (sorted, unique), `Pipeline::unlisted(&self, allowed: &[String]) -> Vec<(String, String)>` (step, name; exact match) and `Pipeline::docker_secret_problems(&self) -> Vec<String>`. `resolve_secrets`, `docker_problems` and `valid_secret_name` do not change (the tick uses them). |
| `rust-ci-runner/src/redact.rs` | Add `pub fn add<I: IntoIterator<Item = String>>(&mut self, values: I)`; `new` calls it. Same encodings, same 4-character line rule, re-sort and dedup. No mention of `bws` or `bitwarden` (`tests/no_vendor.rs`). |
| `rust-ci-runner/src/log.rs` | `red: Redactor` becomes `RwLock<Redactor>`; `write` takes the read lock; add `pub fn add_secrets(&self, values: Vec<String>)`. Add `step secret not allowed` and `step secret unavailable` to `FAULTS` (14 to 16). Update the doc comment over `FAULTS`. |
| `rust-ci-runner/src/job.rs` | `load_pipeline` becomes `pub(crate)`. Nothing else. `prepare` stays for `run` and the tick path. |
| `rust-ci-runner/src/ci.rs` | `job()` keeps the matching `WatchEntry` and passes its `secrets` as `allowed` down to `run_checkout`, which gets the order above, calls `store::fetch_named` with `s.bws`, `token`, `env("FACTORY_CI_BWS_SERVER_URL")` and `s.limits.bws`, and passes the map to `run_pipeline` in place of `BTreeMap::new()`. Clippy allows 7 arguments; fold `allowed`, `token` and the lookup into one small private struct instead of adding parameters. Update the module doc and the "refuses docker steps" wording. |
| `rust-ci-runner/src/ci_tests.rs` | Add the integration tests of T005. Change `declared_secret_fails_before_any_container` and the case table of `every_stderr_line_is_in_the_fixed_list`. Append inside this file, not a new module: `rust-ci-runner/.factory-ci.toml` skips and then runs `ci_tests::` as its own process group, because the mock servers never free their threads and a step container has `--pids-limit 512`. |
| `rust-ci-runner/src/store_tests.rs`, `watch_tests.rs`; test modules in `pipeline.rs`, `redact.rs`, `log.rs` | New unit tests, T001 to T004. |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | `0.2.0` to `0.3.0` (package entry `rust-ci-runner` only). |
| `.github/workflows/factory-monorepo-ci.yml` | The `RUNNER_IMAGE` line only, in its own later commit. |
| `.github/factory-ci-repos.toml` | Rewrite the header comment (`ci` now reads `secrets`); add `secrets` to the `sudtanj/factory-monorepo` entry. |
| `README.md`, `rust-ci-runner/README.md` | Last task, T015. |

### Added

Spec artifacts only: `research.md`, `data-model.md`, `contracts/ci-secrets.md`, `quickstart.md`, `tasks.md`. No new source file.

### Not changed, and why

- `rust-ci-runner/src/engine.rs`: `run_logged` already passes `-e NAME` and the value through the child environment, only for names in `step.secrets` (loop near line 471). A missing map entry is skipped there; `ci` never reaches that state because the lookup finishes first.
- `rust-ci-runner/src/watch.rs`: `valid_secret_name` already refuses reserved and malformed names, `FACTORY_CI*` included, so the four settings are already refused in the entry (FR-004). Tests only.
- `.github/scripts/test-factory-monorepo-ci.sh`: line 89 checks `RUNNER_IMAGE` against `ghcr.io/sudtanj/rust-ci-runner:[0-9]+\.[0-9]+\.[0-9]+@sha256:[0-9a-f]{64}`, so `0.3.0@sha256:...` passes with no edit. Lines 56 to 58 assert only `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN` as workflow secrets; this feature adds none. Nothing there changes.
- `.github/scripts/factory-ci-secrets.sh` and the tick: untouched (Non-goals).
- `rust-ci-runner/.factory-ci.toml`: untouched; new tests live in groups it already runs.

## Data and API contracts

See [contracts/ci-secrets.md](contracts/ci-secrets.md) (watch entry, pipeline step, lookup rule, public lines) and [data-model.md](data-model.md). No new CLI flag, environment variable, workflow input or file format.

## Engineer briefs

Order and parallelism. T001 to T005 (tests first) run in parallel, each in its own file except that T005 is one file. T006, T007 and T008 run in parallel once their tests exist. T009 needs T006 to T008. T010 can run any time. T011 to T014 are sequential and happen after the code is on `main`. T015 is last. Each brief is also a line in `tasks.md`; the IDs match.

### T006 Redactor and log (FR-011, FR-012)
- Goal: a log can learn secret values after it was built, and prints two new fault lines.
- Files: `rust-ci-runner/src/redact.rs`, `rust-ci-runner/src/log.rs`.
- Acceptance: `Redactor::add` after `new` masks the value, its base64, the base64 of `x-access-token:<value>` and each trimmed line of 4 or more characters, longest first. `Log::add_secrets` takes `&self`; text written before the call is unchanged, text after it is masked; the size limit and cut flag keep their state. `FAULTS.len() == 16` and holds both new strings; `Log::fault` prints them on the fault stream and still drops any other text.
- Constraints: no `bws` or `bitwarden` in `redact.rs`. A poisoned lock recovers with `into_inner`, as `Shared` does in `ci.rs`. No new crate.

### T007 Named lookup (FR-005, FR-006, FR-007)
- Goal: read the values of named keys, all or an error.
- Files: `rust-ci-runner/src/store.rs`.
- Acceptance: `fetch_named` runs `bws secret list -o json` once per attempt with the same cleared child environment, token, optional server URL, server check and retry as `fetch`, then returns a map holding each requested name. A name fails when its exact key matches 0 or 2 or more rows (`Count(name, n)`), or its value is empty, holds NUL, or has any line whose trim is under 4 characters (`Unusable(name)`). Exactly 4 passes, 3 fails. A multi-line value passes when every line passes, so a blank line inside a value fails. A row without a string `key`, or output that is not JSON, is `Store(Unreachable)`; a rejected token is `Store(Rejected)`. Other keys, the four runner keys included, need not exist and are never returned. A name repeated in `names` is looked up once. No `Display` output holds a value or `bws` output. `fetch` behaves as before.
- Constraints: share the retry and server check with `fetch` by extraction, not copy. Values with `=` or Unicode pass untouched. Do not trim the returned value.

### T008 Pipeline helpers (FR-001 to FR-003, FR-007, FR-009)
- Goal: ask a parsed pipeline what it requests and what breaks the rules.
- Files: `rust-ci-runner/src/pipeline.rs`.
- Acceptance: `secret_names` returns each requested name once, sorted. `unlisted(allowed)` returns every `(step, name)` whose name is not byte-equal to an entry of `allowed`; an empty `allowed` returns all. `docker_secret_problems` returns one text per step with `docker = true` and a non-empty `secrets`, whatever `--allow-docker` says.
- Constraints: leave `resolve_secrets` and `docker_problems` alone; the tick calls them.

### T009 Wire `ci` (FR-001 to FR-013)
- Goal: `ci` runs a pipeline with its allowlisted secrets.
- Files: `rust-ci-runner/src/ci.rs`, `rust-ci-runner/src/job.rs` (`load_pipeline` visibility).
- Acceptance: the order in "Architecture decision" holds. The two new faults print exactly `[runner] step secret not allowed` and `[runner] step secret unavailable`, once, exit code 1, and no `docker run`, `network create` or `volume create` call happens. Not allowed: no second `bws` call. Detail lines (step `ci`) carry names and counts and no value; stdout and stderr carry no name or count. `log.add_secrets` runs with every value before `run_pipeline`. Each step sees only its own names. The branch gate, the watch check and the `--allow-docker` rule stay before the first `bws` call. A pipeline with no secrets makes exactly one `bws` call.
- Constraints: no `set_var`; the server URL comes through the `env` closure. `ci.rs` may name `bws` (`tests/no_vendor.rs`). Keep `cargo clippy` clean without a blanket `allow`.

### T010 Version (FR-014)
- Goal: the crate says `0.3.0`.
- Files: `rust-ci-runner/Cargo.toml`, `rust-ci-runner/Cargo.lock`.
- Acceptance: both say `0.3.0` for package `rust-ci-runner`; `git diff` shows two changed lines. The release workflow compares the tag to `Cargo.toml`.
- Constraints: edit by hand (no cargo here); touch no other lock entry. `--locked` fails when lock and manifest disagree.

### T011 Release (FR-014)
- Goal: a published 0.3.0 image and digest.
- Files: none (tag only).
- Acceptance: code from T006 to T010 is on `main`; the `rust-ci-runner` pipeline (project `rust-ci-runner`) passed on that commit; tag `rust-ci-runner-v0.3.0` is pushed on it; `rust-ci-runner-release.yml` finished, smoke step included; the run summary shows the digest.
- Constraints: the owner or lead pushes the tag. Verify the digest with `docker pull` as `rust-ci-runner/README.md` says. Never move a tag.

### T012 Pin (FR-015)
- Goal: the workflow runs 0.3.0.
- Files: `.github/workflows/factory-monorepo-ci.yml`.
- Acceptance: the `RUNNER_IMAGE` line reads `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<64 hex digest from T011>` and nothing else in the file changes; `.github/scripts/test-factory-monorepo-ci.sh` still passes.
- Constraints: separate, reviewed commit after the digest exists. No placeholder digest, ever.

### T013 Watch entry (FR-016)
- Goal: the Playwright project may ask for its three names.
- Files: `.github/factory-ci-repos.toml`.
- Acceptance: the `sudtanj/factory-monorepo` entry gains `secrets = ["E2E_BASE_URL", "E2E_USER", "E2E_PASSWORD"]` (final names from the owner); the `sudtanj/infrastructure` entry gains none; the header comment says `ci` reads `secrets` as the allowlist of step secret names and still ignores `secret_branches`; `plan --check` in `factory-ci-selftest.yml` passes.
- Constraints: names only, no value. Names pass `valid_secret_name`. The owner creates the Bitwarden keys first.

### T014 First real run (SC-001)
- Goal: the Playwright run with its login passes on 0.3.0.
- Files: none.
- Acceptance: `factory-monorepo-ci` on `main` for `sudtanj/factory-monorepo` ends in `[runner] pipeline passed`; the log has only grammar lines; Loki shows `***` for the three values. See [quickstart.md](quickstart.md).
- Constraints: only after T012, T013 and the three Bitwarden keys. The `.factory-ci.toml` of `factory-monorepo` that lists the names is that repo's change, not part of this feature.

### T015 Docs (FR-017), last
- Goal: both READMEs and the GitHub description match.
- Files: `README.md` (line 55: replace "Steps get no pipeline secrets here..." and say Bitwarden holds the four runner keys plus the allowlisted step keys); `rust-ci-runner/README.md` ("The `ci` command" steps 2 and 5, the minimal-output paragraph and fault list, the Secrets section, the risks list, the tick note); the repository description through `gh repo edit`.
- Acceptance: the docs state the allowlist, the shared-key lookup, the two new lines, the four always-refused names, no docker with secrets, masking limits (R2), `branches` as the only ref gate (R3), shared names across repos (R4), and that adding `secrets` to an entry also reaches the tick. No secret value appears.
- Constraints: stop-slop applies; the README bootstrap order stays true.

## Test plan (qa writes it first)

Unit tests sit in the `#[cfg(test)]` modules named below. Integration of `ci::run` sits in `rust-ci-runner/src/ci_tests.rs` with the existing `W` harness. Add the allowlist by overwriting `w.watch` (`repos.toml`) with an `[[watch]]` entry for `o/r` that has `secrets = [...]`; add store rows by pushing `(key, value)` into `w.secrets`. `W::bootstrap()` feeds every `w.secrets` value into the leak searches, so a pushed canary is searched for free. Use a unique canary per value.

| ID | File | Test | FR / story |
|---|---|---|---|
| T001 | `store_tests.rs` | `named_lookup_returns_values_by_exact_key` | FR-001, FR-005 |
| | | `named_lookup_missing_key_is_count_0` | FR-006 |
| | | `named_lookup_duplicate_key_is_count_2` | FR-006 |
| | | `named_lookup_key_differing_in_case_does_not_match` | FR-003, FR-006 |
| | | `named_lookup_refuses_empty_nul_and_3_char_line` | FR-006 |
| | | `named_lookup_accepts_a_4_char_line_and_a_multi_line_value` (3 fails, 4 passes, blank inner line fails) | FR-007 |
| | | `named_lookup_passes_unicode_and_equals_untouched` | edge cases |
| | | `named_lookup_does_not_need_the_four_runner_keys` | FR-005 |
| | | `named_lookup_looks_up_a_repeated_name_once` | FR-007 |
| | | `named_lookup_errors_show_names_and_counts_never_values` | FR-006, FR-012 |
| | | `fetch_named_retries_once_and_a_rejected_token_is_final`, `fetch_named_refuses_a_foreign_server_url`, `fetch_named_child_env_is_cleared` (stub `Sandbox`) | FR-005 |
| T002 | `pipeline.rs` tests | `unlisted_is_empty_when_every_name_is_allowed`, `unlisted_lists_step_and_name_for_each_miss`, `unlisted_with_an_empty_allowlist_lists_all`, `unlisted_treats_a_case_difference_as_a_miss`, `one_allowed_name_and_one_listed_name_passes` | FR-001 to FR-003 |
| | | `secret_names_returns_each_name_once_sorted` | FR-007 |
| | | `docker_secret_problems_flags_docker_steps_with_secrets_only` | FR-009 |
| | | `each_runner_setting_is_refused_in_a_step` (all four, parse error) | FR-004 |
| T003 | `watch_tests.rs` | `each_runner_setting_in_secrets_refuses_the_file` (all four) | FR-004 |
| T004 | `redact.rs` tests | `add_masks_value_base64_and_long_lines`, `add_keeps_earlier_values`, `add_ignores_empty_and_short_lines` | FR-011, FR-013 |
| | `log.rs` tests | `add_secrets_masks_later_lines_only`, `add_secrets_keeps_the_size_limit_state`; update `fault_accepts_only_the_fixed_list` (16, both strings) | FR-011, FR-012 |
| T005 | `ci_tests.rs` | `step_secrets_reach_only_the_step_that_lists_them` (two steps, one lists; `docker.calls` has `-e NAME` and no value; `docker.envs` of that run has the value, the other run has none; ends `pipeline passed`; `bws_calls() == 2`) | US1, FR-001, FR-008 |
| | | `pipeline_without_secrets_makes_one_bws_call` | I8 |
| | | `unlisted_name_fails_before_any_container` (stderr exactly `[runner] step secret not allowed\n`; `started_containers() == 0`; `bws_calls() == 1`; name in a Loki body, not in stdout or stderr) | FR-002, FR-012, US2 |
| | | `entry_without_secrets_key_refuses_a_requested_name` | FR-002 |
| | | `name_differing_in_case_is_not_allowed`, `single_allowed_name_runs` | FR-003 |
| | | `runner_setting_in_the_pipeline_is_refused` (four rows, `pipeline file refused`) | FR-004 |
| | | `runner_setting_in_the_entry_refuses_the_watch_file` (four rows, `repository or ref not in the watch list`, `bws_calls() == 0`) | FR-004 |
| | | `unavailable_value_ends_the_run_before_any_container` (table: missing, duplicate, empty, NUL, 3-char line; stderr `[runner] step secret unavailable\n`; `started_containers() == 0`; Loki holds name and count) | FR-005, FR-006, US3 |
| | | `four_char_value_runs`, `multi_line_value_runs_and_masks_each_line` | FR-007, FR-011 |
| | | `two_steps_listing_one_name_look_it_up_once` | FR-007 |
| | | `second_bws_failure_is_unavailable` (stub fails on call 2) | FR-005, FR-006 |
| | | `second_bws_call_carries_the_server_url_and_a_cleared_env` (last block of `bws.envs`) | FR-005 |
| | | `docker_step_with_secrets_is_refused_even_with_allow_docker` (entry `allow_docker = true`, flag set; `docker steps are refused`; zero containers) | FR-009 |
| | | `ref_outside_branches_never_reaches_bws_with_secrets` (`bws_calls() == 0`) | FR-010 |
| | | `secret_branches_is_ignored_by_ci` (entry sets `secret_branches = ["other"]`; a run on `main` still gets values) | FR-010 |
| | | `hostile_step_cannot_print_a_secret_anywhere` (step prints value, its base64 and the value inside a longer line; search stdout, stderr, `docker.calls`, `bws.calls`, `git.calls`, `hub.bodies()` for the raw canary; Loki holds `***`) | FR-011, FR-013, US4, SC-002 |
| | | `no_public_line_holds_a_secret_name_or_count` | FR-012 |
| | | change `declared_secret_fails_before_any_container` to expect `step secret not allowed`; add two rows to the table in `every_stderr_line_is_in_the_fixed_list` | FR-012 |

SC-003 (zero containers on refusal) is the `started_containers()` assert in the refusal tests. SC-001 and SC-004 are manual (T014, quickstart). No file under `rust-ci-runner/tests/` needs a change: none asserts a fault line, the version or `FAULTS`. `tests/no_vendor.rs` must still pass: new code that names `bws` stays in `store.rs`, `ci.rs`, `pipeline.rs`, `log.rs`.

Script check: `.github/scripts/test-factory-monorepo-ci.sh` accepts the 0.3.0 pin as is. Run it after T012.

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| allowlist check | name not in entry, entry has no `secrets` | `Pipeline::unlisted` not empty | `step secret not allowed`, exit 1, no second `bws` call, no container | `unlisted_name_fails_before_any_container` |
| allowlist check | case-different name | exact compare | same | `name_differing_in_case_is_not_allowed` |
| pipeline load | step lists a runner setting | `valid_secret_name` in parse | `pipeline file refused` | `runner_setting_in_the_pipeline_is_refused` |
| watch read | entry lists a runner setting | `watch::parse` error | `repository or ref not in the watch list` | `runner_setting_in_the_entry_refuses_the_watch_file` |
| docker check | docker step with secrets | `docker_secret_problems` | `docker steps are refused` | `docker_step_with_secrets_is_refused_even_with_allow_docker` |
| lookup | `bws` down, timeout, rejected token on call 2 | `fetch_named` returns `Store(..)` | `step secret unavailable`, store text to Loki | `second_bws_failure_is_unavailable` |
| lookup | key missing or duplicated | `Count(name, n)` | `step secret unavailable`, name and count to Loki | `unavailable_value_ends_the_run_before_any_container` |
| lookup | empty, NUL, short-line value | `Unusable(name)` | same | same table |
| lookup | row without `key` | `Store(Unreachable)` | same | store unit test |
| redaction | value reaches a log line | canary search | none expected; `***` in Loki | `hostile_step_cannot_print_a_secret_anywhere` |
| run | OS refuses a huge value | engine spawn fails | existing `runner fault` | none new (spec: accepted) |
| run | step prints a transformed value | not detected | unmasked in Loki (R2, documented) | none; README |
| rollout | pin bumped before the digest exists | `docker pull` fails | run fails at pull; 0.2.0 pin works until then | none; order of T011, T012 |
| rollout | pin bumped before watch entry or keys exist | `step secret not allowed` or `unavailable` | fixed line, no container | quickstart |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. A secret leaks into Loki or stdout because a value joined the redactor after a line that held it. Prevented: `add_secrets` runs in `run_checkout` before `run_pipeline`, the only caller that starts steps; no log call holding a value sits between the lookup and `add_secrets`; the canary test searches every sink.
2. The allowlist is bypassed and a branch reads another repo's key. Prevented: `unlisted` runs before any value is read and compares bytes exactly; `fetch_named` returns only the requested names; tests cover the empty entry, a case difference and an unlisted store key.
3. The release breaks: `Cargo.lock` and `Cargo.toml` disagree under `--locked`, or the pin points at a missing digest. Prevented: T010 touches exactly two lines and `factory-ci-selftest.yml` builds `--locked` on the branch; T012 happens only after T011 shows a digest, per the README bootstrap order.
4. A public line carries a name or count and breaks Principle III. Prevented: `Log::fault` drops text outside `FAULTS`; details go through `log.out`, which does not print in public mode; `no_public_line_holds_a_secret_name_or_count` and the existing `every_stderr_line_is_in_the_fixed_list`.
5. The `store.rs` refactor changes the bootstrap read and every run fails. Prevented: `fetch` and `parse_list` keep their tests (`store_tests.rs` existing cases stay unchanged) and the `list` extraction is a move, not a rewrite.

## Invariants (qa turns these into property tests)

- I1: for any pipeline and entry, no container starts unless every requested name is in `entry.secrets` and has exactly one valid value.
- I2: for any set of values, no byte of stdout, stderr, any child argv or any Loki body holds a raw value once the lookup finished.
- I3: for any run, a step's container receives exactly `step.secrets` as `-e NAME` and no other store key.
- I4: `unlisted(p, allowed)` is empty if and only if `secret_names(p)` is a subset of `allowed`, and growing `allowed` never adds a miss.
- I5: whatever the allowlist, no name of the four runner settings reaches a step environment.
- I6: stdout lines belong to the existing grammar and stderr lines to `FAULTS` (16 entries); no line holds a secret name.
- I7: `Redactor::add(a)` then `add(b)` masks the same text as `new(a + b)`.
- I8: a pipeline with no step secrets makes exactly one `bws` call and behaves as before.

## Edge-case checklist (feiyi-edge-cases) answered for this plan

- Inputs: empty allowlist, duplicate names, case differences, empty, NUL, short-line, multi-line, Unicode and `=` values, one name in two steps: each has a test row. A blank inner line in a multi-line value is refused (Open risks).
- State and order: no state kept; the lookup finishes before the first container; a crash leaves only the checkout the existing cleanup removes.
- Timing: the second call has the same 60 s limit and one retry; the 30-minute run deadline starts later, in `run_pipeline`.
- Concurrency: concurrent runs read the store independently; the workflow's queue group is unchanged.
- Resources: values live as `String` for the run; a huge value fails the spawn into `runner fault`; no cap (spec).
- Permissions and trust: the watch file on `main` decides names; the pipeline file cannot widen it; the four runner settings never pass; `secret_branches` stays ignored, so `branches` is the gate (R3).
- Failure and rollback: every failure stops before a container; rollback is the old pin.
- Output and UI: no UI; two new closed lines.
- Compatibility: entries and pipelines without secrets behave as before; a secret-listing pipeline now ends in `step secret not allowed` instead of `pipeline file refused` (R6).

## Rollout and rollback

1. Land T001 to T010 on `main` (as the README and `factory-ci-selftest.yml` describe; the selftest compiles the code and runs the integration binaries on the branch).
2. Start the `rust-ci-runner` pipeline on `main`; it must pass.
3. Tag `rust-ci-runner-v0.3.0` (T011). The release workflow builds, smoke-tests and publishes; read the digest.
4. Separate reviewed commit: the `RUNNER_IMAGE` line (T012).
5. The owner creates the three Bitwarden keys; commit the `secrets` names in the watch entry (T013).
6. The owner starts `factory-monorepo-ci` (T014).
7. Docs (T015), last.

Rollback: revert the T012 commit; the 0.2.0 pin works as before and refuses step secrets. Remove the names from the watch entry if needed. The image and tag stay (tags never move). No data to undo. Rotate any value a run may have leaked.

## Infrastructure and security assessments

The lead handed over no devops or security assessment for this feature. Plan-level notes in their place:

- Infra: no new workflow, input, Environment secret or variable. `factory-monorepo-ci.yml` changes one line. The runner container still holds the docker socket and the one access token, on an ephemeral GitHub-hosted VM.
- Security: the trust boundary is the watch file on `main`. Anyone who can push `main` of an allowlisted repo can read its allowlisted values (R1). Values travel only through the environment of the engine child and then `-e NAME`, never argv, and the redactor holds them before any step starts. The four runner settings are refused by the name rule in both files. A step that lists secrets cannot use docker (the sidecar and socket would expose them). Residual: R2 to R5 from the spec.

## Open risks

- FR-004 describes a refusal that `valid_secret_name` already gives (the `FACTORY_CI` prefix); only tests are new. Nothing is missing if the owner expected code.
- "A trimmed line under 4 characters" is read to include a blank line inside a multi-line value, so a PEM with an empty line in the middle is refused. A trailing newline is fine (`str::lines`). To reverse it, skip empty lines in `usable_secret`; the redactor already skips short lines.
- Docker-with-secrets is refused before the allowlist check, so a step that breaks both prints `docker steps are refused`.
- `Log` serves `run`, `job` and `ci`. The `RwLock` change must keep `job_tests.rs` and the `log.rs` tests green.
- The `--lib` tests run only on `main` in the runner pipeline (`branches = ["main"]`); the selftest compiles them (`--no-run`) and runs the integration binaries. A broken lib test shows only after landing. Mitigation: step 2 of the rollout runs before the tag.
- The `factory-monorepo` pipeline must list the same names in its own `.factory-ci.toml`; that file is outside this repo.

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

rust-ci-runner/src/   store.rs  pipeline.rs  redact.rs  log.rs  job.rs  ci.rs  (+ the tests named above)
rust-ci-runner/Cargo.toml  Cargo.lock
.github/factory-ci-repos.toml  .github/workflows/factory-monorepo-ci.yml
README.md  rust-ci-runner/README.md
```

**Structure Decision**: one existing crate; no new module or file under `src/`.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|---|---|---|
| `Log` redactor behind an `RwLock` | The pipeline, and so the secret names, is known only after the log exists; values must join the redactor without a second log | A second `Log` resets the 10 MB limit and cut state; rebuilding the log in `run_checkout` threads the shipper and streams through more code |
