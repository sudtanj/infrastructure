# Research: Step secrets for the ci command

No `NEEDS CLARIFICATION` remained after the spec. These are the technical decisions the plan rests on, each checked against the code. The devops assessment, the security threat model and the critic review changed D1, D3, D5 to D9 and D12.

## D1. How to read the values: one call

- Decision: no second `bws` call. `job()` finds the watch entry before it calls the store, so `store::fetch_with` gets the allowlist and, from the one `bws secret list -o json` output, returns the four runner values and `Held`: the values of every row whose key equals an allowlisted name (at most 20 names). Everything else, and the output buffer, is dropped in `store.rs`.
- Rationale: `bws secret list` has no filter, so any extra call would fetch the whole project again. The first call already carries every value; reusing it removes a 60 s call with its own retry and no backoff, the mix-up between the Bitwarden token (in `job()`) and the GitHub clone token (`sec.token`, passed to `steps`), the per-call stub work in tests, and the closure plumbing. The price: values of allowlisted keys are in memory from the first call on, before the pipeline is read. They are validated, redacted and used only for names a step requests (lazy), so a bad allowlisted key does not break runs that do not request it. `run_capped` caps stdout at 16 MiB; a bigger list is cut and fails as the existing `secret store unreachable`. Retention only: dropped `String`s are not zeroed and no zeroize crate is added.
- Alternatives: a second filtered call (no filter exists; extra call and token risk); `bws secret get` per key (needs ids); the shell lookup of the tick (cannot run inside the container, Principle II).

## D2. Matching rule

- Decision: a key matches only when the whole key equals the name, exact case. Zero or two or more matches fail at `take` time.
- Rationale: the spec's shared-key rule; the allowlist is the repo boundary.
- Alternatives: repo-own keys `owner/name<SEP>NAME` (Non-goal).

## D3. Value validity

- Decision: usable when: at most 32 KiB; not blank; no control character other than `\n` and `\r\n` (lone `\r`, NUL, tab, `\x0b`, `\x0c`, U+0085, U+2028, U+2029 refused); a single-line value (no `\n` once trimmed) has no leading or trailing whitespace, a trailing newline included; a multi-line value has every non-blank trimmed line at 4 or more characters, and blank inner lines and a trailing newline are allowed. Passed unchanged.
- Rationale: the redactor drops lines under 4 characters, so a short non-blank line would leak; blank lines carry nothing and PEM files contain them; other separators and control characters can split a value in a log in ways the redactor cannot follow; a pasted Bitwarden value often ends in `\n` and the login would fail late (US3 exists to prevent that); the 32 KiB cap keeps every redactor form below the 64 KiB pump chunk (D12). `store.rs` `usable` (the four runner values) is stricter still (no newline at all).
- Alternatives: reuse `usable` (refuses multi-line); allow a trailing newline on single-line values (late, confusing failures).

## D4. Adding values to the redactor after the log exists

- Decision: `Log.red` becomes `RwLock<Redactor>`; `Redactor::add(batch)` extends it; `Log::add_secrets(values)` is called once with all requested values, before `run_pipeline`. `Redactor::forms()` and `Log::secret_forms()` expose every form.
- Rationale: `Log` is shared with the shipper sink and carries the 10 MB size state; a rebuilt log would reset it. A batch sorts and dedups once (feature 011 adds many values with many forms each; `apply` is O(forms x text)). 011 needs `forms()` to scan every form (its FR-049).
- Alternatives: a second `Log` as `job()` does for the first stage; a `Mutex<Redactor>`.

## D5. The four runner keys and name rules

- Decision: `REQUIRED_KEYS` (renamed from the private `KEYS` in `store.rs`) is the one list of the four keys the runner must find; `log.rs` `is_fault` uses it. No by-name refusal anywhere: `valid_secret_name` already refuses all four by the `FACTORY_CI` prefix (`FACTORY_CI_TOKEN` is also in its exact list), and every runner-held key is named `FACTORY_CI_*`. The access token `BWS_ACCESS_TOKEN` is covered by the `BWS_` prefix. New prefixes `BW_` and `BITWARDEN` (security S7).
- Rationale: a by-name check would be dead code. 011 has other lists (ten reserved names, four required keys and six optional ones) and must not inherit a four-element constant that does three jobs.

## D6. Order of refusals

- Decision: parse (`pipeline file refused`), docker rules (`docker steps are refused`), allowlist (`step secret not allowed`), no names then skip, ref gate (`repository or ref not in the watch list`), `Held::take` (`step secret unavailable`).
- Rationale: cheapest and most structural first; the ref gate needs the requested set. The spec orders no pair.

## D7. Where the tests go and what runs them

- Decision: `ci::run` tests go in `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs`, in their own process; unit tests join `store_tests.rs`, `watch_tests.rs`, `secret_names_tests.rs` and the test modules in `pipeline.rs`, `redact.rs`, `log.rs`, `engine.rs`. File-level checks (names in two entries, `Cargo.toml` against `Cargo.lock`, README sentence) go in `tests/repo_files.rs`, run by the selftest. The selftest `list` job runs `cargo test --locked --lib` in full (T013) so tests run before code reaches `main`.
- Rationale: `W` and the stubs are private to `ci_tests.rs`. `.factory-ci.toml` runs `ci_tests::` as one process under `--pids-limit 512`. Each test builds a `tiny_http` server (`hub`) whose accept thread ends in `Hub::drop` (`server.unblock()`), so threads do end, but 46 existing plus about 30 new tests and their reader threads share a process; the split into `ci_tests::step_secrets::` is a cheap guard, and the header comment of `.factory-ci.toml` (which says threads never exit) is corrected. The runner pipeline mounts only the project directory, so `tests/repo_files.rs`, which reads `../.github` and `../README.md`, runs only in the selftest (full checkout) and fails under a plain `cargo test` outside a full checkout; it stays out of the explicit integration list.

## D8. Where the new data lives (clippy)

- Decision: into `Args`: `id`, `allowed`, `held`, `ref_may_hold_secrets`. `steps` drops its `want_id` parameter (6 left); `run_checkout` keeps 7. No `#[allow]`.
- Rationale: `make lint` runs `clippy -D warnings`; both functions already sit at the limit of 7.

## D9. Version, landing and rollout

- Decision: `0.3.0`. Code lands by push (PR creation on this repo returns 403): feature branch first, so `factory-ci-selftest.yml` (it ignores pushes to `main`) runs lint, lib and integration tests, then a fast-forward of `main`. Tag after the `rust-ci-runner` pipeline passes on the exact commit (run id and full SHA recorded). The release workflow retags `:latest`. The pin is a separate commit on a branch, preceded by a commit that adds `factory-monorepo-ci.yml` to the selftest `paths` (the glob `factory-ci*.yml` does not match it), so job `shell` runs `test-factory-monorepo-ci.sh`. The digest comes from the release summary; no `docker pull` runs on the agent VM; the canary on `ci-canary` pulls the image.
- Rationale: devops S4 to S7, critic B1, S1, S6.

## D10. Ref gate for secrets

- Decision: when the pipeline requests secrets, `secret_branches` (if set) must match the ref; otherwise no `branches` pattern may be made only of `*` and `?` characters. The default `*`, an explicit `["*"]`, `?*` and `*?` are wild (`glob_match` makes `?*` match any non-empty ref).
- Rationale: `branches` defaults to `*` (`watch.rs`) and `ci` used to ignore `secret_branches` (security S1).

## D11. Docker with secrets

- Decision: refuse when any step lists secrets and any step has `docker = true`.
- Rationale: the docker sidecar is privileged and shares the run network (security S2).

## D12. Masking forms, bounds and the output pump

- Decision: `Redactor::add` always adds the value, its plain base64, the base64 of `x-access-token:<value>` (both as today) and each trimmed line of 4 or more characters; derived forms need 8 or more characters and at most 48 KiB: base64 stable middle at three alignments (standard, URL-safe, unpadded), percent-encoding, JSON-string escape, lowercase hex. `pump` takes the forms and cuts a piece back to before any form occurrence that would straddle the cut (forms are at most 48 KiB, below the 64 KiB chunk), flushes at end of stream and never cuts inside a UTF-8 character. Derived forms also include each 76-column line of the standard base64 of the whole 3-byte groups of a value, when that is longer than 76 characters. The public line is computed from the raw text of `event` calls.
- Rationale: `echo "$V" | base64` adds a newline that changes the tail, and a prefix shifts the alignment; `pump` cuts at 65536 bytes and `Log::write` redacts each chunk alone (security S4, S5, S6). A floor on the value itself would unmask `FACTORY_CI_LOG_USER` (a numeric id of about 6 digits) and the existing `"pw"` and `"tok123"` tests, so the floor applies to derived forms only. The percent form of a 32 KiB value can reach 96 KiB, hence the 48 KiB form cap.
- Residual: a value under 6 bytes has an `echo "$V" | base64` form under 8 characters that is not masked; masking only the stable middle leaves up to about 4 bytes of base64 visible at each end; double encodings and splits across lines (spec R2).
