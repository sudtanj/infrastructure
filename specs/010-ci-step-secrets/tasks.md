---

description: "Task list for step secrets in the ci command"
---

# Tasks: Step secrets for the ci command

**Input**: `specs/010-ci-step-secrets/` (spec.md, plan.md, research.md, data-model.md, contracts/ci-secrets.md, quickstart.md)

**Prerequisites**: plan.md, spec.md. The full brief of each task is in plan.md under "Engineer briefs" (T008 to T027) and "Test plan" (T001 to T007); IDs match.

**Tests**: required (constitution Principle V, with the justified exception recorded in plan.md). They are written first; the branch selftest in T018 is where "green only with the code" is checked.

**Rules for everyone**:
- No cargo, docker, builds or tests on the agent VM (1 GB); tests run in the selftest and the `rust-ci-runner` pipeline. Git and file reads only. Do not run `.github/scripts/test-factory-monorepo-ci.sh` by hand; the selftest runs it.
- Never write a secret value anywhere, in files, logs, commits or messages.
- No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or any Environment secret, or creates or reads Bitwarden keys. Debugging uses the Actions log and Loki only. Every Bitwarden action is the owner's.
- `redact.rs` and `watch.rs` must not mention `bws` or `bitwarden` (`rust-ci-runner/tests/no_vendor.rs`).
- Nothing that holds a token or a secret value derives `Debug`. No `#[allow(clippy::...)]`.
- `git add` with explicit paths. Never force-push. Code lands only through T018.
- 010 merges before 011 (shared files: `Cargo.toml`, `log.rs` `FAULTS`, `ci.rs`, READMEs).

## Format: `[ID] [P?] [Story] Description`

- **[P]**: can run in parallel (different files, no unfinished dependency)
- Stories: US1 run with a secret, US2 allowlist and ref gate, US3 missing or bad value stops early, US4 nothing leaks

## Phase 1: Tests first (shared by all stories, no story label)

- [X] T001 [P] Write the keyed-lookup unit tests of plan.md "Test plan" T001 (FR-001, FR-003, FR-005 to FR-007, FR-018: `Held::take` exact key, found 0 and 2, control characters, line boundary, blank inner line, trailing newline, surrounding whitespace, 32 KiB cap, lazy validation, `fetch_with`, `Debug` and `Display` without value or name) in `rust-ci-runner/src/store_tests.rs`
- [X] T002 [P] Write the pipeline helper tests of T002 (FR-001 to FR-004, FR-007, FR-009: `unlisted`, `secret_names`, any-docker rule with two separate steps, four runner settings) in the `tests` module of `rust-ci-runner/src/pipeline.rs`
- [X] T003 [P] Write the watch tests of T003 (FR-004 four names; FR-010 `secrets_allowed_on` with `branches` omitted, `["*"]`, `["?*"]`, `["*?"]`, `["main"]`, `["release/*"]`, `secret_branches` matching, not matching, `["*"]`) in `rust-ci-runner/src/watch_tests.rs`, and the `BW_`, `BITWARDEN` rows in `rust-ci-runner/src/secret_names_tests.rs`
- [X] T004 [P] Write the redactor and log tests of T004 (FR-011 to FR-013: `new(["pw"])` still masks, `echo "$V" | base64`, `printf 'x%s'` base64, wrapped, URL-safe, unpadded, percent, JSON, hex, floors and caps, batch equals `new`, `forms()`, secret `pass` keeps the public line, `add_secrets` ordering and size state, `FAULTS` membership) in the `tests` modules of `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`
- [X] T005 Write the `ci::run` integration tests of T005 in a new `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs` (FR-001 to FR-013, US1 to US4, SC-002, SC-003; one `bws` call per run; entries with `branches = ["main"]`); change `declared_secret_fails_before_any_container` and add two rows to `every_stderr_line_is_in_the_fixed_list` in the same file
- [X] T006 [P] Write the pump tests of T006 (FR-013: value across the 64 KB cut, a 32 KiB value across it, one byte over the cap, order and tail flush, no UTF-8 cut) in the `tests` module of `rust-ci-runner/src/engine.rs`
- [X] T007 [P] Write `rust-ci-runner/tests/repo_files.rs` (FR-014, FR-016: no secret name in two committed entries; `sudtanj/infrastructure` has no `secrets`; `Cargo.toml` and `Cargo.lock` versions are equal); it reads `../.github` and needs a full checkout, so do not add it to the integration command of `rust-ci-runner/.factory-ci.toml`

### Tests written first: names, markers, stubs (T001 to T007 done)

Nothing was compiled or run (1 GB VM). Each test below that needs code carries `#[ignore = "test-first"]`; the engineer removes the marker when the test passes. The unit and integration tests do not compile until the stubs below exist, so add those first (T008, T009, T010, T011, T012, T015) and let CI be the first compile. The tests marked "runs now" need no new code.

Planned signatures the tests call (the `pump` row changed in implementation):

| Item | Signature (file) |
|---|---|
| `REQUIRED_KEYS` | `pub const REQUIRED_KEYS: [&str; 4]` (`store.rs`) |
| `parse_list_with` | `fn(&str, &[String]) -> Result<Fetched, StoreError>` (`store.rs`) |
| `fetch_with` | `fn(&str, &str, Option<&str>, Duration, &[String]) -> Result<Fetched, StoreError>` (`store.rs`) |
| `Fetched` | `{ pub secrets: Secrets, pub held: Held }` |
| `Held::take` | `fn(&self, &[String]) -> Result<StepSecrets, NamedError>`; `Held: Debug` (manual) |
| `StepSecrets` | tuple struct, field `.0: BTreeMap<String, String>` readable from the crate; `Debug` |
| `NamedError` | `Count { index: usize, found: usize }`, `Unusable { index: usize }`; `Debug`, `Display` (`secret #<i>: expected 1, found <n>`, `secret #<i>: unusable value`) |
| `Pipeline::secret_names` | `fn(&self) -> Vec<String>` (sorted, once each) |
| `Pipeline::unlisted` | `fn(&self, &[String]) -> Vec<(String, String)>` (`(step, name)`) |
| `Pipeline::docker_secret_problems` | `fn(&self) -> Vec<String>` (empty when fine) |
| `WatchEntry::secrets_allowed_on` | `fn(&self, &str) -> bool` |
| `Redactor::add` | `fn(&mut self, I) where I: IntoIterator<Item = String>` |
| `Redactor::forms` | `fn(&self) -> Vec<String>` |
| `Log::add_secrets` | `fn(&self, Vec<String>)` |
| `Log::secret_forms` | `fn(&self) -> Vec<String>` |
| `Log::longest_secret` | `fn(&self) -> usize` (0 when none) |
| `pump` | `fn(r: impl Read, forms: &[String], f: impl FnMut(&str))` (`engine.rs`; was `carry: usize` in the first test draft, changed by the engineer, see open point 2) |
| `FAULTS` | gains `step secret not allowed`, `step secret unavailable` |

Test names per task (file in brackets):

- T001 [`store_tests.rs`]: `held_take_returns_values_by_exact_key`, `held_take_missing_key_is_found_0`, `held_take_duplicate_key_is_found_2`, `held_take_key_differing_in_case_does_not_match`, `held_take_refuses_empty_blank_and_each_control_character`, `held_take_line_boundary`, `held_take_refuses_surrounding_whitespace_on_a_single_line`, `held_take_size_cap`, `held_take_passes_unicode_and_equals_untouched`, `held_take_validates_only_the_names_taken`, `held_take_looks_up_a_repeated_name_once`, `fetch_with_keeps_only_allowlisted_rows_and_still_needs_the_four_runner_keys`, `fetch_with_empty_allowlist_equals_fetch`, `fetch_with_retries_once_and_a_rejected_token_is_final`, `fetch_with_refuses_a_foreign_server_url`, `fetch_with_child_env_is_cleared`, `debug_and_display_of_held_results_and_errors_hold_no_value_and_no_name`.
- T002 [`pipeline.rs` tests]: `secret_names_returns_each_name_once_sorted`, `unlisted_is_empty_when_every_name_is_allowed`, `unlisted_reports_each_miss_with_its_step`, `unlisted_compares_bytes_so_a_case_difference_is_a_miss`, `unlisted_is_empty_iff_names_are_a_subset_and_growing_never_adds` (I4), `docker_secret_problems_flags_secrets_with_any_docker_step`, `each_runner_setting_is_refused_in_a_step`.
- T003 [`watch_tests.rs`]: `each_runner_setting_in_secrets_refuses_the_file`, `secrets_allowed_on_refuses_branches_omitted`, `secrets_allowed_on_refuses_every_pattern_made_only_of_star_and_question_mark`, `secrets_allowed_on_explicit_branches_main`, `secrets_allowed_on_explicit_release_glob`, `secrets_allowed_on_follows_secret_branches_when_set`, `secrets_allowed_on_secret_branches_star_opens_it_on_purpose`. [`secret_names_tests.rs`]: `bw_and_bitwarden_names_are_refused_in_an_entry_and_a_pipeline`.
- T004 [`redact.rs` tests]: `new_still_masks_a_two_character_value`, `add_masks_value_and_base64_forms`, `add_masks_wrapped_base64_line_by_line` (decided, see open point 1), `add_masks_percent_json_and_hex_forms`, `add_skips_derived_forms_under_8_chars_and_over_48_kib`, `forms_lists_every_form`, `add_batch_equals_new`, `add_then_add_masks_what_new_of_both_masks_for_generated_values` (I7, fixed LCG, no proptest), `add_keeps_longest_first_across_batches`. [`log.rs` tests]: `add_secrets_masks_later_lines_only`, `add_secrets_keeps_size_limit_state`, `a_secret_named_pass_cannot_mangle_a_public_line`, `a_secret_named_pass_is_still_redacted_in_a_non_public_log`, `faults_hold_the_two_step_secret_lines`, `secret_forms_and_longest_secret_follow_add_secrets`, `add_secrets_while_other_threads_write_loses_no_line_and_masks_the_end`. `fault_accepts_only_the_fixed_list` lost its `FAULTS.len() == 14` assert and stays green (runs now).
- T005 [`ci_tests.rs`, `mod step_secrets`]: `step_secrets_reach_only_the_step_that_lists_them`, `two_steps_listing_one_name_get_one_value`, `single_allowed_name_runs`, `four_char_value_runs`, `blank_inner_line_value_runs_and_masks_each_line`, `an_allowlisted_key_with_a_bad_value_does_not_fail_a_run_that_does_not_request_it`, `unlisted_name_fails_before_any_container`, `entry_without_secrets_key_refuses_a_requested_name`, `name_differing_in_case_is_not_allowed`, `runner_setting_in_the_pipeline_is_refused`, `runner_setting_in_the_entry_refuses_the_watch_file`, `docker_step_in_a_pipeline_with_secrets_is_refused`, `secrets_with_branches_omitted_or_star_are_refused`, `secret_branches_must_match_the_ref`, `explicit_branches_main_gets_secrets`, `a_pipeline_without_secrets_runs_on_branches_star`, `rules_are_checked_in_the_documented_order`, `unavailable_value_ends_the_run_before_any_container`, `every_run_makes_one_bws_call`, `hostile_step_cannot_print_a_secret_anywhere`, `a_secret_named_pass_keeps_pipeline_passed_on_stdout`, `no_public_line_holds_a_secret_name_or_count`. Changed, now `#[ignore]`: `declared_secret_fails_before_any_container` (expects `step secret not allowed`), `every_stderr_line_is_in_the_fixed_list` (two rows added).
- T006 [`engine.rs` tests]: `pump_masks_a_value_across_the_64k_cut`, `pump_masks_a_32_kib_value_across_the_cut`, `pump_with_a_value_one_byte_over_the_cap_is_never_given_to_it`, `pump_keeps_order_and_flushes_the_tail`, `pump_does_not_hold_back_a_complete_line`, `pump_never_cuts_a_utf8_char`, `pump_does_not_split_a_value_at_its_own_emit_boundary` (decided, see open point 2).
- T007 [`tests/repo_files.rs`], run now: `no_secret_name_in_two_committed_entries`, `the_repeat_check_finds_a_name_in_two_entries`, `infrastructure_entry_has_no_secrets`, `cargo_toml_and_lock_versions_match`, `the_version_check_reads_the_right_entry`. Ignored until T027: `readme_has_no_stale_secrets_sentence` (written early; T027 only removes the marker).

Requirement to test map:

| Requirement | Tests |
|---|---|
| FR-001 | `step_secrets_reach_only_the_step_that_lists_them`, `single_allowed_name_runs`, `unlisted_*` (T002) |
| FR-002 | `unlisted_name_fails_before_any_container`, `entry_without_secrets_key_refuses_a_requested_name`, `declared_secret_fails_before_any_container`, `unlisted_reports_each_miss_with_its_step` |
| FR-003 | `name_differing_in_case_is_not_allowed`, `unlisted_compares_bytes_so_a_case_difference_is_a_miss`, `held_take_key_differing_in_case_does_not_match`, `single_allowed_name_runs` |
| FR-004 | `each_runner_setting_is_refused_in_a_step`, `each_runner_setting_in_secrets_refuses_the_file`, `bw_and_bitwarden_names_are_refused_in_an_entry_and_a_pipeline`, `runner_setting_in_the_pipeline_is_refused`, `runner_setting_in_the_entry_refuses_the_watch_file` |
| FR-005 | `fetch_with_*` (five tests), `held_take_validates_only_the_names_taken`, `an_allowlisted_key_with_a_bad_value_does_not_fail_a_run_that_does_not_request_it`, `every_run_makes_one_bws_call` |
| FR-006 | `held_take_missing_key_is_found_0`, `held_take_duplicate_key_is_found_2`, `held_take_refuses_empty_blank_and_each_control_character`, `held_take_refuses_surrounding_whitespace_on_a_single_line`, `held_take_size_cap`, `unavailable_value_ends_the_run_before_any_container` |
| FR-007 | `held_take_line_boundary`, `held_take_passes_unicode_and_equals_untouched`, `held_take_looks_up_a_repeated_name_once`, `two_steps_listing_one_name_get_one_value`, `four_char_value_runs`, `blank_inner_line_value_runs_and_masks_each_line`, `secret_names_returns_each_name_once_sorted` |
| FR-008 | `step_secrets_reach_only_the_step_that_lists_them` (`-e NAME`, value in the engine child env, not argv, not in the other step) |
| FR-009 | `docker_secret_problems_flags_secrets_with_any_docker_step`, `docker_step_in_a_pipeline_with_secrets_is_refused` |
| FR-010 | `secrets_allowed_on_*` (six tests), `secrets_with_branches_omitted_or_star_are_refused`, `secret_branches_must_match_the_ref`, `explicit_branches_main_gets_secrets`, `a_pipeline_without_secrets_runs_on_branches_star` |
| FR-011 | T004 redactor tests, `add_secrets_masks_later_lines_only`, `hostile_step_cannot_print_a_secret_anywhere`, `blank_inner_line_value_runs_and_masks_each_line` |
| FR-012 | `faults_hold_the_two_step_secret_lines`, `a_secret_named_pass_*` (log and `ci`), `no_public_line_holds_a_secret_name_or_count`, `every_stderr_line_is_in_the_fixed_list` |
| FR-013 | T006 pump tests, `hostile_step_cannot_print_a_secret_anywhere` |
| FR-014 | `cargo_toml_and_lock_versions_match` (the tag and release workflow: Principle V exception) |
| FR-015, FR-019, SC-001, SC-004 | none (Principle V exception: pin shape in `test-factory-monorepo-ci.sh`, canary and week check by hand) |
| FR-016 | `no_secret_name_in_two_committed_entries`, `infrastructure_entry_has_no_secrets` (+ `plan --check`) |
| FR-017 | `readme_has_no_stale_secrets_sentence` |
| FR-018 | `debug_and_display_of_held_results_and_errors_hold_no_value_and_no_name`, `held_take_*` |
| SC-002 | `hostile_step_cannot_print_a_secret_anywhere` |
| SC-003 | `started_containers() == 0` in every refusal test (`refused` helper) |
| SC-005 | the whole list above in `--lib` |
| I1 | `unavailable_*`, `unlisted_*`, `secrets_with_branches_*` (no container) |
| I2 | `hostile_step_cannot_print_a_secret_anywhere` |
| I3, I5 | `step_secrets_reach_only_the_step_that_lists_them`, `held_take_missing_key_is_found_0` (runner key not reachable) |
| I4 | `unlisted_is_empty_iff_names_are_a_subset_and_growing_never_adds` |
| I6 | `no_public_line_holds_a_secret_name_or_count`, `assert_public` in every `ci` test |
| I7 | `add_batch_equals_new`, `add_then_add_masks_what_new_of_both_masks_for_generated_values`, `new_still_masks_a_two_character_value` |
| I8 | `every_run_makes_one_bws_call` |
| I9 | `pump_masks_*`, `pump_keeps_order_and_flushes_the_tail`, `pump_never_cuts_a_utf8_char` (not "whatever the cuts": open point 2) |
| I10 | `held_take_*` |

Open points the tests surfaced, decided by the lead on 2026-11-28:

1. Wrapped base64 (FR-011, spec R2). Decided: `Redactor::add` also masks each 76-column line of the standard base64 form of a value when that form is longer than 76 characters; pieces under 8 characters are not added and the pieces count against the 48 KiB derived-form cap. `add_masks_wrapped_base64_line_by_line` runs.
2. Pump emit boundary (I9). Decided: `pump` takes the forms (`Log::secret_forms()`) instead of a carry length and moves a cut that has no newline back to before the start of any form occurrence the cut would split, repeated until none is left; the carry logic is gone. If that leaves an empty piece it keeps the character boundary alone (the text after the cut is not known yet, so "cut after the occurrence" is not possible). Two assertions of the qa tests that a piece never holds a value whole were removed: a piece may hold it whole because `Log::write` masks each piece. `pump_does_not_split_a_value_at_its_own_emit_boundary` runs.

Not covered: the `.github/scripts/test-factory-monorepo-ci.sh` pin shape (selftest `shell` job runs it after T020), `plan --check`, the release workflow, FR-019 canary values. No property-based crate exists (`proptest` is not a dependency and the brief does not allow adding one): I7 and I4 use exhaustive or fixed-LCG loops instead.

## Phase 2: Foundation (T008 first; T009, T010, T013 to T017 in parallel with it; T011 after T008; T012 after T011)

- [x] T008 [P] [US3] Add `REQUIRED_KEYS` (rename), `fetch_with`, `parse_list_with`, `Fetched`, `Held`, `StepSecrets`, `NamedError` (position-keyed) and the value rule in `rust-ci-runner/src/store.rs`, keeping `fetch` and `parse_list` as they behave today; rule per data-model.md "Step secret values" and "NamedError" (FR-005 to FR-007, FR-018)
- [x] T009 [P] [US2] Add `Pipeline::secret_names`, `unlisted`, `docker_secret_problems`; reserve `BW_` and `BITWARDEN` in `rust-ci-runner/src/pipeline.rs` (FR-001 to FR-004, FR-007, FR-009)
- [x] T010 [P] [US2] Add `WatchEntry::secrets_allowed_on` in `rust-ci-runner/src/watch.rs` per data-model.md "Ref gate", treating patterns made only of `*` and `?` as wild (FR-010)
- [x] T011 [US4] Add `Redactor::add` (batch, forms, floors and caps) and `forms`, and `Log::add_secrets`, `secret_forms`, `longest_secret`, the `RwLock`, public lines from raw event text and the two new `FAULTS` lines, in `rust-ci-runner/src/redact.rs` and `rust-ci-runner/src/log.rs`; uses `REQUIRED_KEYS` from T008 (FR-011 to FR-013)
- [x] T012 [US4] Cut `pump` back before a form that straddles its 64 KB cut (`forms` = `Log::secret_forms()`) in `pump` in `rust-ci-runner/src/engine.rs`; needs T011 (FR-013)
- [x] T013 [P] Make the `list` job of `.github/workflows/factory-ci-selftest.yml` run `cargo test --locked --lib` without `--no-run` and rename the step; raise `timeout-minutes` only if needed
- [x] T014 [P] Split the `ci_tests::` group in `rust-ci-runner/.factory-ci.toml` (`ci_tests:: -- --skip ci_tests::step_secrets::` and a separate `ci_tests::step_secrets::` command) and correct its header comment about mock-server threads
- [x] T015 [US1] Wire `rust-ci-runner/src/ci.rs` per plan.md "Order inside `run_checkout`": put `id`, `allowed`, `held`, `ref_may_hold_secrets` into `Args`, call `store::fetch_with` in `job()` with the Bitwarden `token`, no new parameter beyond 7, no `#[allow]`; make `load_pipeline` `pub(crate)` in `rust-ci-runner/src/job.rs`; needs T008 to T012 (FR-001 to FR-013, FR-018)
- [x] T016 [P] Change the `rust-ci-runner` package version to `0.3.0` in `rust-ci-runner/Cargo.toml` and `rust-ci-runner/Cargo.lock`, by hand, two lines (FR-014)
- [x] T017 [P] Add `ci-canary/.factory-ci.toml`: one step, image pinned by digest, `commands = ["echo ok"]`, no secrets (canary pipeline)

## Phase 3: Landing (P1 stories US1 to US4 are all carried by T015)

**Goal**: `ci` runs a fixture pipeline with its allowlisted secrets; every refusal stops before any container; nothing leaks.

**Independent test**: the tests of T005 pass in the selftest on the branch.

- [ ] T018 [US1] Land the code: commit T001 to T017 together with messages that link `specs/010-ci-step-secrets/` (no PR is possible, creation returns 403); push the branch; the selftest (lint, full lib tests, integration tests, `plan --check`) must be green; then fast-forward `main` with the same commits in one push

**Checkpoint**: the code is on `main`.

## Phase 4: Release and first use (sequential)

- [ ] T019 [US1] Dispatch the `rust-ci-runner` pipeline on `main`; record its run id and full SHA; confirm the SHA equals `git rev-parse main` (re-run if `main` moved); tag `rust-ci-runner-v0.3.0` on that SHA; wait for `.github/workflows/rust-ci-runner-release.yml`; record the digest from its run summary, no `docker pull` on the agent VM (FR-014)
- [ ] T020 [US1] On a branch push two commits together: add `.github/workflows/factory-monorepo-ci.yml` to both `paths` lists in `.github/workflows/factory-ci-selftest.yml`, then set `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest>`; the selftest job `shell` runs `test-factory-monorepo-ci.sh` and is green; fast-forward `main`; do not bump `RUNNER_IMAGE_DIGEST` (FR-015)
- [ ] T021 [US1] Run `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `ref=main`, `project=ci-canary`; it pulls the image and must end in `[runner] pipeline passed`; otherwise revert the pin (plan.md "Rollback")
- [ ] T022 [US2] Re-check with read-only `gh api`, token with admin scope, run by the owner, before the first secret run (FR-019): Environment `factory-ci` has one custom-branch policy, `main`, and no required reviewer; the only collaborator of both repos is the owner; no Dependabot, Renovate or auto-merge on `factory-monorepo` `main`; the `rust-ci-runner-v*` tag ruleset exists; the GHCR package is readable; the machine account reads one project
- [ ] T023 [US2] Rewrite the header comment of `.github/factory-ci-repos.toml` (`ci` reads `secrets` per repo and applies the FR-010 gate); add to the `sudtanj/factory-monorepo` entry only the non-login names a step needs (none today; the owner names them); never add `secrets` to `sudtanj/infrastructure`; `tests/repo_files.rs` and `plan --check` pass (FR-016)
- [ ] T024 [US4] Canary values run per quickstart.md section 7 (owner creates and deletes the Bitwarden key; a temporary name in the entry and in a `factory-monorepo` step; plain, `echo "$V" | base64` and percent-encoded; check Loki; remove the name) (FR-019)
- [ ] T025 [US1] First real run per quickstart.md section 8, once a real non-login step secret exists; until then 010 is complete without it; logins come from feature 011 (SC-001)
- [ ] T026 [US1] One week after T025 list the runs of `factory-monorepo-ci` with `gh run list` (Actions read is enough; T022 is the task that needs admin scope) and count `step secret not allowed` and `step secret unavailable`; an unexpected one is a README fix (SC-004)

## Final Phase: Docs (last task)

- [ ] T027 Update `README.md` (line 55), `rust-ci-runner/README.md` (the `ci` list, the fault list, Secrets, risks), the GitHub description (`gh repo edit`, needs admin) and add `readme_has_no_stale_secrets_sentence` to `rust-ci-runner/tests/repo_files.rs`, as listed under T027 in plan.md, including the rollback list, the runbook line and the note that `repo_files.rs` needs `../` (FR-017)

## Dependencies

- T001 to T007 first, in parallel; T005 is one file.
- T008, T009, T010, T013 to T017 start together; T011 after T008; T012 after T011; T015 after T008 to T012; T018 after T001 to T017.
- T019 after T018; T020 after T019; T021 after T020; T022 after T021; T023 after T022; T024 after T023 and the owner's canary key; T025 after T024 and a real secret; T026 one week after T025; T027 last.

## Implementation strategy

MVP is T001 to T021: the code, the release and a proof that 0.3.0 runs. Run `/speckit-analyze` before `/speckit-implement`. 27 tasks.
