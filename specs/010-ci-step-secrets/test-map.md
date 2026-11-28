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
| FR-013 | `pump_masks_a_value_across_the_64k_cut`, `pump_masks_a_32_kib_value_across_the_cut`, `pump_does_not_split_a_value_at_its_own_emit_boundary`, `pump_never_cuts_a_utf8_char`, new `pump_masks_every_form_around_the_cut`, `pump_masks_two_adjacent_values_around_the_cut`, `a_value_across_the_64k_read_boundary_is_masked_in_loki` | PASS. D1 fixed in T017a, both former D1 tests run and pass in run 46118115166. |
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
| `pump_masks_a_value_after_a_run_of_a_forms_first_character` | `src/engine.rs` | D1, two inputs (a run of `a`, a text that repeats the value's own prefix). `#[ignore]` until the engineer fixes `cut_point`, then remove the marker. |
| `a_value_across_the_64k_read_boundary_after_a_run_of_one_character_is_masked_in_loki` | `src/ci_tests.rs` | D1 through the real engine, shipper and mock Loki. `#[ignore]` with the same instruction. |
| `parallel_steps_get_only_their_own_secrets_and_every_output_is_masked` | `src/ci_tests.rs` (`step_secrets`) | Four steps started together (K1, K2, both, none), three rounds: argv `-e` names, env block, no foreign value, no value in Loki, stdout or argv, one `bws` call. |
| `a_value_across_the_64k_read_boundary_is_masked_in_loki` | `src/ci_tests.rs` | Real engine pump, shipper and mock Loki: 65530 spaces then the value, 65520 spaces then its base64; checks that no whole value, whole base64 or 6-character leftover reaches Loki or a public line, and that two lines hold `***`. |
| `three_names_in_one_step_all_reach_it` | `src/ci_tests.rs` | US1 scenario 1 with exactly three names, one `bws` call, no value in argv. |
| `wrapped_base64_of_a_long_value_is_masked_line_by_line` | `src/ci_tests.rs` | `echo "$V" \| base64` (76 columns) and `base64 -w 0` of a 101-byte value. |

No property-test crate is a dependency (`proptest` absent) and the brief did not allow adding one, so the pump and redactor invariants use enumerated offsets and the existing fixed-LCG loop (`add_then_add_masks_what_new_of_both_masks_for_generated_values`).

## Defects and risks

- D1 (FIXED in T017a, history: FR-013 FAIL found in run 46115243670). `cut_point` in `engine.rs` repeats "move the cut back to the start of a form prefix" until no prefix is found. Any derived form (base64 middle at a shifted alignment, hex, URL-safe) starts with some character `c`. When the 64 KB chunk before the value is a run of `c`, every byte is a one-character prefix of a form, so the walk goes back one byte at a time to the start of the chunk. `cut == 0` then returns the plain character boundary, which is in the middle of the value. Both halves reach Loki unmasked. Observed: the value `fake-site-value-Q7x1` has a base64 form starting with `a`; 65530 `a` then the value shipped `fake-s` in one Loki line and `ite-value-Q7x1` in the next; the same for 16 of the 28 characters of its base64 line. The text that repeats the value's own prefix does the same. Reach: the step must print about 64 KB of one character with no newline, then the secret, so the secret lands on the 64 KB cut. It leaks to Loki only (private to the owner); stdout and stderr stay on the fixed grammar. A pipeline author can already print the value in any form, so no new capability, but SC-002 and FR-013 do not hold for this input. Fix for the engineer: the repeat exists only so the new cut does not split a complete occurrence that ends between the new cut and the old one. Search for those complete occurrences (`buf[..cut]` ends of a form that start before the new cut) instead of repeating the prefix test, so the walk is bounded by one pass, and never fall back to `whole` while a prefix match exists. Then remove the two `#[ignore]` markers.
- R1 (low). The same walk costs about `widest * forms` byte tests per byte of the run, so a 64 KB run of one character is slow (the lib run went from 19.9 s to 21.1 s with the two tests that hit it). It also emits tiny pieces, each a Loki line, if the fallback is replaced by a short cut. Bounded by the 10 MiB log limit.
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

## Re-verification after T017a (head 7ec047f, QA tests on top)

Evidence: selftest run 46117218924 (head 7ec047f) and run 46118115166 (head 86d7689, QA tests added), both success; the second log was read: 399 lib tests passed, 0 ignored; integration binaries ok; `repo_files` 5 passed, 1 ignored (`readme_has_no_stale_secrets_sentence`, T027). `shell`, `workflow`, `pipeline` jobs green. Runs 46117669571 and 46117839981 were QA's own red runs (a wrong expectation in a new test, then rustfmt), fixed.

| Fix | Unit | Integration / simulation (`ci_tests.rs`, `ship_job.rs`) | Result |
|---|---|---|---|
| D1 | `engine::tests::pump_masks_a_value_after_a_run_of_a_forms_first_character`, `pump_masks_every_form_around_the_cut`, `pump_masks_two_adjacent_values_around_the_cut` | `a_value_across_the_64k_read_boundary_after_a_run_of_one_character_is_masked_in_loki` (ran, ok in 46118115166) | PASS |
| H1 | `log::tests::out_local_prints_and_does_not_ship` | `output_of_a_step_with_secrets_is_not_shipped` (ok and failing step, plain step still ships), `no_step_output_shipped` in the hostile, boundary, wrapped and parallel tests, `ship_job::with_shipping_on_stdout_holds_only_events_and_the_markers_reach_loki_redacted` (real binary: secret step ships state only, plain step ships redacted output) | PASS |
| M3 | `watch_tests::secrets_allowed_on_refuses_every_ref_without_secret_branches` | `explicit_branches_without_secret_branches_get_no_secrets` (main, release/*, and `secret_branches` picks release only) | PASS |
| L5 | `held_take_line_boundary`, `held_take_refuses_surrounding_whitespace_on_a_single_line` (7 refused, 8 passes) | `eight_char_value_runs_and_a_seven_char_value_is_unavailable` (no container for 7) | PASS |
| L6 | `log::tests::a_step_named_like_a_secret_prints_without_its_name` (also a partial-name control) | new `a_step_named_like_a_secret_value_prints_as_step` | PASS |
| S1, S2/M2, M4/S3 | `redact::tests::add_masks_the_review_forms` (echo-newline base64, 64 and 76 columns, lowercase and `+` percent, `encodeURIComponent`, uppercase hex, JSON `\/` and `\uXXXX`) | new `a_plain_step_printing_the_review_forms_ships_only_masked_lines` (value, prefixed value, `echo | base64`, uppercase hex through a shipping step) | PASS |
| N1 | stub loop bounded at 2000 tries | n/a | PASS (read; no test kills a stub in the lock) |

### Tests changed to the new behaviour: do they still assert something real?

H1 makes every ci-level Loki masking assertion on a secret-holding step trivially true: such a step ships nothing, so "no value in Loki" and "no `***` line" cannot fail from broken masking. Verdict per test:

- The ci-level tests that dropped their `***` count (`hostile_step_cannot_print_a_secret_anywhere`, `assert_boundary_masked`, `wrapped_base64_of_a_long_value_is_masked_line_by_line`, `blank_inner_line_value_runs_and_masks_each_line`) now prove only: nothing shipped, and nothing on the public lines or in argv. Still real for the second part, no longer a proof of masking in Loki. QA did not edit them.
- Masking across the 64 KB cut stays proven by the engine unit tests, which run `pump` into a redactor and fail on a cut through a value (`shown(..) == want`), unchanged except for the D1 marker and comment.
- Gap found and closed: no ci-level test had a shipping step print a secret. New tests use step `b` with no secrets next to step `a` with one; `b` prints the value across the 64 KB cut (fill of spaces and of `a`, the D1 shape) and the review forms, and its Loki lines must hold `***` and none of the value, its base64 or 6-character pieces. These fail if the shared redactor or the cut break (they would have caught D1 through the real engine, shipper and mock Loki).
- Other changed tests (floor 4 to 8, `secrets_allowed_on_*`, pass-named secret values) still assert their behaviour: both sides of the 7/8 boundary, a default refused plus an explicit `secret_branches` allowed, and the public grammar with real phrases from the public lines.
- Property tests: still no `proptest` dependency, brief did not allow one. Fixed enumeration and the LCG loop stand in.

## Still unverified

- Release smoke and the tag `rust-ci-runner-v0.3.0` (FR-014, T018 onward), the pin gate (FR-015), README (FR-017, ignored until T027), canary (FR-019), SC-001, SC-004.
- Memory on the 1 GB runner: forms are bounded (48 KiB each, 20 names) but no measurement was taken.
- Concurrent runs of one repo: the existing concurrency group; no test here.
- R2 (wrap widths other than 64 and 76, wrap after a prefix) stays a documented limit.
- `cut_point` has no property test; its fallback (a chain of whole forms reaching the chunk start, such as one repeated character that is the whole value) is not covered by a named test. Low risk, local to Loki-bound text of a plain step.
- rustfmt and clippy cannot run on this machine; CI ran them (green in 46118115166).

## Verdict

READY for T008 to T017a. D1 is fixed with evidence (both former D1 tests ran and passed in 46117218924 and 46118115166); H1, M3, L5, L6 and the new forms have unit and integration coverage, and QA closed the one proof gap H1 opened (masking through a shipping step). Landing and docs tasks (T018 onward) and the items under "Still unverified" are outside this verdict and stay NOT TESTED.
