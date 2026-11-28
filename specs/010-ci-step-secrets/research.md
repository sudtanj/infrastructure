# Research: Step secrets for the ci command

No `NEEDS CLARIFICATION` remained after the spec. These are the technical decisions the plan rests on, each checked against the code.

## D1. When and how to read the values

- Decision: one extra `bws secret list -o json` call after the fetch, only when a step lists a name and every name passed the allowlist.
- Rationale: the pipeline file is known only after the checkout. The first call happens before it. A filtered second call keeps other repos' values out of the process, satisfies FR-002 ("read no value") and costs nothing for pipelines without secrets. `store.rs` `attempt` already sets a cleared child environment, the token and the server URL, and `fetch` already retries once and treats a rejected token as final.
- Alternatives: keep the first list in memory (values held before they are asked for); `bws secret get` per key (needs ids); the shell lookup of the tick (cannot run inside the container, constitution Principle II).

## D2. Matching rule

- Decision: a key matches only when the whole key equals the name, exact case. Zero or two or more matches fail. This is the rule `parse_list` applies to the four runner keys.
- Rationale: the spec's shared-key rule; one lookup rule; the allowlist is the repo boundary.
- Alternatives: repo-own keys `owner/name<SEP>NAME` (the separator lives in a shell variable of the tick script; Non-goal).

## D3. Value validity

- Decision: a value is usable when it is not empty, holds no NUL, and every line (split by `str::lines`, trimmed) has 4 or more characters. It is passed unchanged.
- Rationale: the redactor drops lines under 4 characters, so a shorter line would leak unmasked, which the tick also refuses. `store.rs` `usable` is stricter (no newline at all) because the four runner values are single-line; step secrets may be multi-line (keys, certificates).
- Open point: a blank line inside a value fails under this rule. See plan, Open risks.
- Alternatives: reuse `usable` (refuses multi-line, contradicts the spec's multi-line edge case); skip blank lines (more permissive than the spec text).

## D4. Adding values to the redactor after the log exists

- Decision: `Log.red` becomes `RwLock<Redactor>`; `Redactor::add` extends it; `Log::add_secrets(&self, values)` is called once, before `run_pipeline`.
- Rationale: `Log` is shared with the shipper sink and carries the 10 MB size state. A rebuilt log would reset that state. A lock read per line costs nothing against the work `write` already does (replace per secret, char scan).
- Alternatives: second `Log` as `job()` does for the first stage (resets the limit, needs streams and shipper in `run_checkout`); interior `Mutex<Redactor>` (same, but readers block each other).

## D5. Where the four runner settings are refused

- Decision: no new code. `pipeline.rs` `valid_secret_name` refuses `FACTORY_CI_TOKEN` (exact) and any name that starts with `FACTORY_CI` (prefix), case-insensitively. `pipeline::parse` applies it to steps, so a pipeline listing one ends in `pipeline file refused`. `watch::parse` applies it to `secrets`, so the whole file is refused and `ci` prints `repository or ref not in the watch list` (`watch::read(..).ok()` is `None`). Tests table-drive all four names in both places (FR-004).
- Rationale: YAGNI; the checks exist and are tested for other names in `secret_names_tests.rs` and `pipeline.rs`.

## D6. Order of refusals

- Decision: parse (`pipeline file refused`), docker rules (`docker steps are refused`), allowlist (`step secret not allowed`), lookup (`step secret unavailable`).
- Rationale: cheapest and most structural first; the lookup is the only network call and goes last. The spec orders no pair, so the docker rule goes first because `docker_problems` already ran there.

## D7. Where the tests go

- Decision: integration tests append to `rust-ci-runner/src/ci_tests.rs`; unit tests join `store_tests.rs`, `watch_tests.rs` and the test modules in `pipeline.rs`, `redact.rs`, `log.rs`.
- Rationale: `W` and the stub scripts are private to `ci_tests.rs`. `rust-ci-runner/.factory-ci.toml` runs `ci_tests::` in its own process (mock server threads, `--pids-limit 512`) and skips it in the first group; a new test module would land in the first group.
- Alternatives: a new `ci_secrets_tests.rs` (needs `W` made `pub(crate)` and an edit to `.factory-ci.toml` skip and run lists).

## D8. Passing allowlist and lookup to `run_checkout`

- Decision: `job()` keeps the matching `WatchEntry` instead of only its id, and hands `entry.secrets` down. Bundle `allowed`, `token` and the lookup call in one small private struct.
- Rationale: `steps` already takes 7 arguments and `run_checkout` 7; clippy's limit is 7. The codebase uses `#[allow(clippy::too_many_arguments)]` once, in `job.rs`; a struct avoids adding a second.

## D9. Version and rollout

- Decision: `0.3.0` (new behaviour, no breaking CLI change, one fault line moves). Tag `rust-ci-runner-v0.3.0` on `main` after the `rust-ci-runner` pipeline passes. The release workflow checks the tag against `Cargo.toml` and that the commit is on `main`. The pin commit comes after the digest. `.github/scripts/test-factory-monorepo-ci.sh` already accepts any `X.Y.Z@sha256:<64 hex>`.
