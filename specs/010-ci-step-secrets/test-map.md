# Test map: 010 step secrets for the ci command

QA of branch `010-ci-step-secrets`, code tasks T008 to T017. Evidence is the selftest log of run 46114238710 (head 27f4925): `cargo test --lib` 384 passed, 0 ignored; integration binaries all ok, `repo_files` 5 passed 1 ignored. QA's added tests are in the run named at the end. Test names are the proof of coverage; every name in tasks.md "Tests written first" was checked to exist as a `fn`.

## Verdict

See the last section.

## Changes the engineer made to QA's tests

| Change | Judgment | Why |
|---|---|---|
| Removed "no piece holds the value whole" in `pump_masks_a_value_across_the_64k_cut` and `pump_masks_a_32_kib_value_across_the_cut` | Legitimate | The assertion encoded the first plan (carry bytes, value always split). The decided design cuts before the value, so the next piece starts with the whole value and `Log::write` masks it. A cut through a value still fails `shown(..) == want`, because `shown` redacts each piece alone. QA added `pump_masks_every_form_around_the_cut`, which also checks the pieces are lossless and at most 64 KiB. |
| `lines()` strips `run= sha= branch=` | Legitimate | `ship.rs:198` puts that label on every shipped line, so the first draft could never match. `splitn(4, ' ')` keeps the rest of the text intact. Leak checks use `hub.bodies()`, the raw bodies, so they do not depend on `lines()`. |
| mkdir lock in the docker stub | Legitimate | See the flake section. It fixes a harness race, not a product one. Residual risk: the loop has no bound, so a stub killed inside the lock would hang the next call. The critical section is two appends and no test kills a stub there. |
| `pump(r, forms, f)` instead of `carry: usize` | Legitimate | Decided in tasks.md open point 2. Every pump test moved with it and still asserts the masked text. |
| `#[ignore = "test-first"]` removed from all lib tests | Verified | 0 ignored in run 46114238710. One marker stays in `tests/repo_files.rs` (`readme_has_no_stale_secrets_sentence`, T027). |

## The failed test `a_timeout_kills_the_step_and_leaves_no_container`

The brief's premise does not hold. Run 46113610090 did not run it and it did not fail there.

- Run 46113610090 failed `ci_tests::step_secrets::step_secrets_reach_only_the_step_that_lists_them` at `ci_tests.rs:2208` (`step_env(&w, "one").contains("SITE_API_KEY=...")`). The `unit tests` step failed, so the `integration tests` step, which holds the timeout test, never started.
- `a_timeout_kills_the_step_and_leaves_no_container` lives in `tests/executor.rs:244` and uses the real docker binary, not the stub. It passed in run 46113946132 and run 46114238710 (executor: 11 passed). No run in the log history has it failing.
- The real failure was a harness race. The two steps have no `depends_on`, so they start together. The stub appended to `docker.calls` and `docker.envs` in two separate writes, so the call and env blocks of different steps interleaved, and `env_block(nth)` read another step's env. The lock keeps both appends together. Runs 46113946132 (unit green; red only on rustfmt) and 46114238710 have it and pass.
- Verdict: flaky harness, fixed, not a product bug. If `a_timeout_...` does fail later, treat that as a new report. It needs docker and busybox on the runner and a 3 s timeout, so a slow runner is the likely cause.
- QA's new parallel test repeats the run 3 times per CI run to catch a return of the race.

## Acceptance criteria to tests

"Ran" means present in the 384 of run 46114238710, none ignored. Rows with "new" are QA tests added after it, see the last section.

### User stories

| ID | Criterion | Tests | Result |
|---|---|---|---|
| US1.1 | three names allowed, stored once, step starts with three variables, `pipeline passed` | new `three_names_in_one_step_all_reach_it`, `step_secrets_reach_only_the_step_that_lists_them`, `single_allowed_name_runs`, `two_steps_listing_one_name_get_one_value` | PASS |
| US1.2 | second step only lists the name, first has no variable | `step_secrets_reach_only_the_step_that_lists_them` (argv `-e`, env block, value not in the other step), new `parallel_steps_get_only_their_own_secrets_and_every_output_is_masked` (four steps, started together) | PASS |
| US2.1 | unlisted name: `step secret not allowed`, no container | `unlisted_name_fails_before_any_container`, `entry_without_secrets_key_refuses_a_requested_name`, `name_differing_in_case_is_not_allowed`, `declared_secret_fails_before_any_container`, `rules_are_checked_in_the_documented_order` | PASS |
| US2.2 | entry lists a runner setting: file refused | `runner_setting_in_the_entry_refuses_the_watch_file`, `each_runner_setting_in_secrets_refuses_the_file`, `bw_and_bitwarden_names_are_refused_in_an_entry_and_a_pipeline` | PASS |
| US3.1 | name with no key: `step secret unavailable`, no container | `unavailable_value_ends_the_run_before_any_container` (missing, duplicate, empty, blank, NUL, 3-character line, trailing newline, over 32 KiB), `held_take_missing_key_is_found_0`, `held_take_duplicate_key_is_found_2` | PASS |
| US4.1 | printed value nowhere in stdout, stderr, argv, Loki; `***` in Loki | `hostile_step_cannot_print_a_secret_anywhere` (plain, base64 two ways, percent, hex, inside a longer line, argv of docker, bws, git), `blank_inner_line_value_runs_and_masks_each_line`, `four_char_value_runs`; new `a_value_across_the_64k_read_boundary_is_masked_in_loki`, `wrapped_base64_of_a_long_value_is_masked_line_by_line` | PASS |

### Functional requirements

| ID | Tests | Result |
|---|---|---|
| FR-001 | `step_secrets_reach_only_the_step_that_lists_them`, `single_allowed_name_runs`, `secret_names_returns_each_name_once_sorted`, `unlisted_is_empty_when_every_name_is_allowed` | PASS |
| FR-002 | `unlisted_name_fails_before_any_container` (name in Loki, not in public lines), `entry_without_secrets_key_refuses_a_requested_name`, `unlisted_reports_each_miss_with_its_step` | PASS |
| FR-003 | `name_differing_in_case_is_not_allowed`, `unlisted_compares_bytes_so_a_case_difference_is_a_miss`, `held_take_key_differing_in_case_does_not_match`, `single_allowed_name_runs` | PASS |
| FR-004 | `each_runner_setting_is_refused_in_a_step`, `runner_setting_in_the_pipeline_is_refused`, `each_runner_setting_in_secrets_refuses_the_file`, `runner_setting_in_the_entry_refuses_the_watch_file` (all four names in both places) | PASS |
| FR-005 | `every_run_makes_one_bws_call` (also asserted in four more tests), `held_take_validates_only_the_names_taken`, `an_allowlisted_key_with_a_bad_value_does_not_fail_a_run_that_does_not_request_it`, `fetch_with_*` | PASS |
| FR-006 | `held_take_refuses_empty_blank_and_each_control_character`, `held_take_refuses_surrounding_whitespace_on_a_single_line`, `held_take_size_cap`, `held_take_line_boundary`, `unavailable_value_ends_the_run_before_any_container` | PASS |
| FR-007 | `held_take_looks_up_a_repeated_name_once`, `two_steps_listing_one_name_get_one_value`, `four_char_value_runs`, `blank_inner_line_value_runs_and_masks_each_line`, `held_take_line_boundary` | PASS |
| FR-008 | `step_secrets_reach_only_the_step_that_lists_them` (`-e NAME`, value in the engine child env, not argv), new parallel test | PASS |
| FR-009 | `docker_secret_problems_flags_secrets_with_any_docker_step`, `docker_step_in_a_pipeline_with_secrets_is_refused` (with `--allow-docker` and entry flag too) | PASS |
| FR-010 | `secrets_allowed_on_*` (6 tests), `secrets_with_branches_omitted_or_star_are_refused`, `secret_branches_must_match_the_ref`, `explicit_branches_main_gets_secrets`, `a_pipeline_without_secrets_runs_on_branches_star` | PASS |
| FR-011 | `add_masks_value_and_base64_forms`, `add_masks_wrapped_base64_line_by_line`, `add_masks_percent_json_and_hex_forms`, `add_skips_derived_forms_under_8_chars_and_over_48_kib`, `forms_lists_every_form`, `add_secrets_masks_later_lines_only`, `add_secrets_while_other_threads_write_loses_no_line_and_masks_the_end` | PASS. Documented limits (spec R2, not defects): a base64 line wrapped at a width other than 76, and a wrapped base64 of the value behind a prefix, are not masked. |
| FR-012 | `faults_hold_the_two_step_secret_lines`, `a_secret_named_pass_keeps_pipeline_passed_on_stdout`, `a_secret_named_pass_cannot_mangle_a_public_line`, `no_public_line_holds_a_secret_name_or_count`, `every_stderr_line_is_in_the_fixed_list` | PASS |
| FR-013 | `pump_masks_a_value_across_the_64k_cut`, `pump_masks_a_32_kib_value_across_the_cut`, `pump_does_not_split_a_value_at_its_own_emit_boundary`, `pump_never_cuts_a_utf8_char`, new `pump_masks_every_form_around_the_cut`, `pump_masks_two_adjacent_values_around_the_cut`, `a_value_across_the_64k_read_boundary_is_masked_in_loki` | PASS for ordinary output. FAIL for one contrived input, defect D1 below (test `pump_masks_a_value_that_follows_its_own_prefix_repeated`, ignored). |
| FR-014 | `cargo_toml_and_lock_versions_match`, `the_version_check_reads_the_right_entry` (ran, `repo_files` 5 passed). Tag `rust-ci-runner-v0.3.0` and the release smoke step: not a test. | PASS for the files, NOT TESTED for tag and smoke |
| FR-015 | none, pin gate is the `shell` job after the pin commit | NOT TESTED (landing, T018 onward) |
| FR-016 | `no_secret_name_in_two_committed_entries`, `the_repeat_check_finds_a_name_in_two_entries`, `infrastructure_entry_has_no_secrets` | PASS for the checks; the comment-block edit is not a test |
| FR-017 | `readme_has_no_stale_secrets_sentence` | NOT TESTED (ignored until T027; README not yet changed) |
| FR-018 | `debug_and_display_of_held_results_and_errors_hold_no_value_and_no_name`, `held_take_*` | PASS |
| FR-019 | canary run by the owner | NOT TESTED (owner action) |

### Success criteria

| ID | Evidence | Result |
|---|---|---|
| SC-001 | needs a manual workflow run on the 0.3.0 pin | NOT TESTED |
| SC-002 | `hostile_step_cannot_print_a_secret_anywhere` | PASS |
| SC-003 | `refused` helper asserts `started_containers() == 0` in every refusal test | PASS |
| SC-004 | one week of use | NOT TESTED |
| SC-005 | 384 lib tests green in run 46114238710 | PASS |

## Added by QA (after run 46114238710)

| Test | File | What it pins |
|---|---|---|
| `pump_masks_every_form_around_the_cut` | `src/engine.rs` | Every string `Redactor::forms()` returns (plain, three base64 alignments, URL-safe, 76-column wrapped pieces, percent, hex, clone header), for four batches (ASCII, 100-byte wrapped value, multi-byte, two values), at eight offsets from "starts on the cut" to "ends one past it". Pieces are lossless, at most 64 KiB, masked. |
| `pump_masks_two_adjacent_values_around_the_cut` | `src/engine.rs` | Two copies back to back, the cut inside either. |
| `pump_masks_a_value_that_follows_its_own_prefix_repeated` | `src/engine.rs` | D1. `#[ignore]` until the engineer fixes `cut_point`. |
| `parallel_steps_get_only_their_own_secrets_and_every_output_is_masked` | `src/ci_tests.rs` (`step_secrets`) | Four steps started together (K1, K2, both, none), three rounds: argv `-e` names, env block, no foreign value, no value in Loki, stdout or argv, one `bws` call. |
| `a_value_across_the_64k_read_boundary_is_masked_in_loki` | `src/ci_tests.rs` | Real engine pump, shipper and mock Loki: 65530 filler bytes then the value, 65520 bytes then its base64. |
| `three_names_in_one_step_all_reach_it` | `src/ci_tests.rs` | US1 scenario 1 with exactly three names, one `bws` call, no value in argv. |
| `wrapped_base64_of_a_long_value_is_masked_line_by_line` | `src/ci_tests.rs` | `echo "$V" \| base64` (76 columns) and `base64 -w 0` of a 101-byte value. |

No property-test crate is a dependency (`proptest` absent) and the brief did not allow adding one, so the pump and redactor invariants use enumerated offsets and the existing fixed-LCG loop (`add_then_add_masks_what_new_of_both_masks_for_generated_values`).

## Defects and risks

- D1 (low, FR-013). `cut_point` in `engine.rs` repeats "move the cut back to the start of a form prefix" until no prefix is found. When the text before the cut is periodic in the value's own first characters (for example `abc` repeated for 64 KB before a value that starts `abcabc...`), the walk reaches the start of the chunk and `cut_point` returns the plain character boundary, which can cut through the value. Both halves then reach Loki unmasked. Needs the step to print 64 KB of the secret's own prefix, so only the pipeline author can trigger it, and that author can already transform the value any way they like. Suggested fix for the engineer: when the walk reaches 0, keep the cut of the first iteration (or the last non-zero one) instead of `whole`, then remove the `#[ignore]`.
- R1 (low). In the same case a long run of a form's first character makes the pump emit many tiny pieces (each becomes a Loki line). Self-inflicted by the step, bounded by the 10 MiB log limit.
- R2 (known limit, spec R2). Base64 wrapped at another width (`base64 -w 40`, `fold`) or wrapped after a prefix (`echo "key=$V" | base64`) is not masked. Plain and 76-column forms are.
- R3. The mkdir lock in the stub is unbounded, see the table above.

## Edge-case checklist (feiyi-edge-cases)

- Inputs, empty and limits: handled. Empty allowlist (`entry_without_secrets_key_refuses_a_requested_name`), one name (`single_allowed_name_runs`), 20 and 21 allowlist names (`watch_tests.rs:180`), value at 4 characters and at 3 (`held_take_line_boundary`), 32 KiB and 32 KiB + 1 (`held_take_size_cap`, `pump_with_a_value_one_byte_over_the_cap_is_never_given_to_it`).
- Inputs, duplicates: handled. Same name in two steps (`two_steps_listing_one_name_get_one_value`), two store keys (`held_take_duplicate_key_is_found_2`), a name differing in case (`name_differing_in_case_is_not_allowed`).
- Inputs, Unicode and control: handled. `held_take_passes_unicode_and_equals_untouched`, `held_take_refuses_empty_blank_and_each_control_character` (NUL, lone CR, VT, FF, U+0085, U+2028, U+2029), `pump_never_cuts_a_utf8_char`, multi-byte value in `pump_masks_every_form_around_the_cut`. Very long strings: 64 KB lines in the new pump and `ci` tests.
- Inputs, malformed: existing `store_tests` parse tests, not re-read by QA beyond the names (NOT TESTED for this change).
- State: first run and migration handled (an entry without `secrets` behaves as before, `a_pipeline_without_secrets_runs_on_branches_star`). No state between runs (spec Edge Cases). Crash mid-run: N/A, only the checkout directory, removed by existing cleanup. Partial failure: handled, all lookups before the first container (`unavailable_value_ends_the_run_before_any_container` counts engine calls). Retry: handled, no state, one `bws` call each run (`every_run_makes_one_bws_call`).
- Time and concurrency: parallel steps handled (new parallel test, three rounds); redactor changed while other threads write handled (`add_secrets_while_other_threads_write_loses_no_line_and_masks_the_end`); timeouts on `bws` and retry existing (`unreachable_is_retried_once`, `fetch_with_retries_once_and_a_rejected_token_is_final`); clock and ordering N/A (no time logic added). Concurrent runs of one repo: the existing concurrency group, not testable here (NOT TESTED).
- Resources: disk full N/A (no new writes). Large input: handled (32 KiB values, 64 KB lines, 48 KiB form cap). Dependency down or garbage: existing store tests. Unbounded growth: handled by the 10 MiB log limit and the 20-name allowlist; the tiny-piece effect is R1. Memory on the 1 GB runner: the forms are at most 48 KiB each and the 20-name cap bounds them; not measured (NOT TESTED).
- Permissions and trust: handled. Pipeline cannot widen the allowlist (US2), cannot name a runner setting (FR-004), cannot get secrets on an open ref (FR-010), cannot combine secrets with a docker sidecar (FR-009); values never in argv (FR-008) and never in a public line (FR-012). Values come from the store, names from the watch file on `main`, not from the pipeline. Open: D1 and R2 are masking limits reachable only by the pipeline author.
- UI: N/A, the surface is the fixed stdout and stderr lines (`assert_public` in every `ci` test).

## Verdict

Filled in after the CI run of QA's tests, below.
