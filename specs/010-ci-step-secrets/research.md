# Research: Step secrets for the ci command

No `NEEDS CLARIFICATION` remained after the spec. These are the technical decisions the plan rests on, each checked against the code. The devops assessment and the security threat model changed D1, D3, D4, D6, D7 and added D10 to D12.

## D1. When and how to read the values

- Decision: one extra `bws secret list -o json` call after the fetch, only when a step lists a name and the docker, allowlist and ref checks passed.
- Rationale: the pipeline file is known only after the checkout. `bws secret list` has no filter, so the call returns the whole project and `parse_named` keeps the named keys. The gain over reusing the first call: no value is looked up or kept on a refusal path (FR-002), no new call for pipelines without secrets, and the list text and parsed rows are dropped inside `fetch_named` before steps start. `run_capped` caps stdout at 16 MiB, so a larger project list fails to parse and ends as `Unreachable` (`step secret unavailable`). `store.rs` `attempt` already gives a cleared child environment, the token and the server URL; `fetch` retries once with no backoff and treats a rejected token as final.
- Alternatives: keep the first list in memory (values held before they are asked for); `bws secret get` per key (needs ids); the shell lookup of the tick (cannot run inside the container, Principle II).

## D2. Matching rule

- Decision: a key matches only when the whole key equals the name, exact case. Zero or two or more matches fail. Names in `RUNNER_KEYS` are never returned.
- Rationale: the spec's shared-key rule; the allowlist is the repo boundary.
- Alternatives: repo-own keys `owner/name<SEP>NAME` (Non-goal).

## D3. Value validity

- Decision: a value is usable when it is not blank, holds no control character other than `\n` and `\r\n` (lone `\r`, NUL, tab, `\x0b`, `\x0c`, U+0085, U+2028, U+2029 refused), and every non-blank trimmed line has 4 or more characters. Blank inner lines pass. The value is passed unchanged.
- Rationale: the redactor drops lines under 4 characters, so a short non-blank line would leak; blank lines carry nothing to leak and PEM files contain them. Other line separators and control characters can split a value in a log in ways the redactor cannot follow (security S6). `store.rs` `usable` is stricter (no newline at all) because the four runner values are single-line.
- Alternatives: reuse `usable` (refuses multi-line); refuse blank inner lines (refuses ordinary PEM and certificate files).

## D4. Adding values to the redactor after the log exists

- Decision: `Log.red` becomes `RwLock<Redactor>`; `Redactor::add` extends it; `Log::add_secrets(&self, values)` is called once, before `run_pipeline`.
- Rationale: `Log` is shared with the shipper sink and carries the 10 MB size state; a rebuilt log would reset it. The lock also lets feature 011 add values later. A read lock per line costs little next to the replace work in `write`.
- Alternatives: a second `Log` as `job()` does for the first stage; a `Mutex<Redactor>`.

## D5. Where the runner keys are refused

- Decision: `RUNNER_KEYS` in `store.rs` is the one list. `valid_secret_name` refuses its names (besides the `FACTORY_CI` prefix), `fetch_named` refuses to return them, `log.rs` uses it. Every key the runner itself holds is named `FACTORY_CI_*`; the access token itself, `BWS_ACCESS_TOKEN`, is covered by the `BWS_` prefix. New prefixes `BW_` and `BITWARDEN` (security S7).
- Rationale: one list cannot drift; the checks exist and only need tests for all four names in both files (FR-004).

## D6. Order of refusals

- Decision: parse (`pipeline file refused`), docker rules (`docker steps are refused`), allowlist (`step secret not allowed`), no names then skip, ref gate (`repository or ref not in the watch list`), lookup (`step secret unavailable`).
- Rationale: cheapest and most structural first; the ref gate needs the requested set; the lookup is the only network call and goes last. The spec orders no pair.

## D7. Where the tests go

- Decision: integration tests go in `mod step_secrets` at the end of `rust-ci-runner/src/ci_tests.rs`, run in their own process; unit tests join `store_tests.rs`, `watch_tests.rs`, `secret_names_tests.rs` and the test modules in `pipeline.rs`, `redact.rs`, `log.rs`, `engine.rs`. The committed-file test (S9) is `tests/watch_file.rs`, run by the selftest only.
- Rationale: `W` and the stub scripts are private to `ci_tests.rs`. `rust-ci-runner/.factory-ci.toml` runs `ci_tests::` as one process under `--pids-limit 512`. Each test starts a `tiny_http` server (`hub`); `Hub::drop` unblocks its accept thread, so threads do end, but with about 46 existing and about 30 new tests plus reader threads the split into `ci_tests::step_secrets::` is a cheap guard. The runner pipeline mounts only the project directory, so a test that reads `../.github` runs only in the selftest, which checks out the whole repo.

## D8. Passing allowlist and lookup to `run_checkout`

- Decision: `job()` keeps the matching `WatchEntry`, builds a closure `names -> store::fetch_named(&s.bws, token, server, limit, names)` and passes only the closure and `allowed` down, in one small private struct without `Debug`.
- Rationale: `steps` takes the GitHub clone token (`sec.token`); the lookup needs the Bitwarden token (`token` in `job()`). A closure keeps both tokens out of `run_checkout`, makes the lookup swappable (feature 011) and avoids a clippy `too_many_arguments` allow.

## D9. Version and rollout

- Decision: `0.3.0`. Tag after the `rust-ci-runner` pipeline and `make lint` pass on the exact commit. The release workflow checks the tag against `Cargo.toml` and that the commit is on `main`, and it retags `:latest`. The pin commit goes to a branch first because `factory-ci-selftest.yml` ignores pushes to `main`, then fast-forwards. A no-secret canary run follows the pin.
- Rationale: devops S4 to S7.

## D10. Ref gate for secrets

- Decision: when the pipeline requests secrets, `secret_branches` (if set) must match the ref; otherwise `branches` must be explicit with no pattern made only of `*`. The default `*` and an explicit `["*"]` are the same value in `WatchEntry`, so both are refused.
- Rationale: `branches` defaults to `*` (`watch.rs`), and `ci` used to ignore `secret_branches`, so an omitted `branches` would give every pushed branch the values (security S1). The factory-monorepo entry already has `branches = ["main"]`.

## D11. Docker with secrets

- Decision: refuse the pipeline when any step lists secrets and any step has `docker = true`.
- Rationale: the docker sidecar is privileged and shares the run network; a no-secret docker step could reach the secret step's container (security S2).

## D12. Masking forms and the output pump

- Decision: `Redactor::add` masks the value, base64 (stable middle at three alignments, URL-safe, unpadded), percent-encoding, JSON-string escape, lowercase hex and long lines; `pump` carries the last `longest - 1` bytes between chunks; the public line is computed from fixed grammar.
- Rationale: `echo "$V" | base64` adds a newline that changes the tail; a prefix shifts the alignment; `pump` cuts at 65536 bytes and `Log::write` redacts each chunk alone (security S4, S5, S6). Residual: forms under 8 characters, double encodings and splits across lines (R2).
