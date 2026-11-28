# Research: Bitwarden watch list

Each entry: decision, why, alternatives. Evidence is a file and line at `origin/main` 8660df5.

## R1 The plan runner cannot read Bitwarden

- Decision: a separate workflow step runs `watch-list` and writes the file phase A already reads.
- Why: `.github/scripts/factory-ci-plan.sh` `run()` removes the Bitwarden access token from every runner call; the script is frozen; the `--repos .github/factory-ci-repos.toml` argument is relative to the checkout, so a file written there before `plan` is read unchanged.
- Rejected: recovering the token from the parent process environment (breaks a stated isolation); editing the script (frozen).

## R2 The manual workflow cannot get a pre-fetch step

- Decision: `ci` reads the list in the `bws` call it already makes.
- Why: `.github/scripts/test-factory-monorepo-ci.sh` requires one `docker run`, three `-v` mounts and `/etc/factory-ci/repos.toml:ro`, and bans the word `bws` in the workflow.

## R3 Stub or delete

- Decision: stub.
- Why: R2's mount. A missing mount source is created as a directory by Docker; `ci` would then fail to read it and print the unlisted line, which hides the real state. The stub also keeps `plan --check` in the selftest and the frozen `--repos` argument valid.

## R4 The selftest lint rejects a schedule

- Decision: lint a copy without the cron block; assert the block separately.
- Why: `factory-ci-lint-workflow.sh:92` fails on `^  schedule:` (rule `no-schedule`); the script is frozen; the selftest YAML is editable. The unchanged lint still guards every other rule and the `pull_request_target` check.

## R5 What public text names the list today

- `plan.rs` `summary()` prints `- item N: <repo>` and the unchecked repos; `problem()` prefixes `<repo>:`; `watch.rs` labels entries by repo name and quotes globs, secret names and values (`malformed glob {g:?}`, `secret {s:?} is reserved or malformed`, `id {shown} is not a number`).
- Decision: number-only messages (`parse_private`), `entry N` in the plan, redaction seeds from the list.

## R6 Where Bitwarden code may live

- `tests/no_vendor.rs` allows the vendor words only in `pipeline.rs`, `store.rs`, `ci.rs`, `engine.rs`, `main.rs`, `log.rs`. The new code goes to `store.rs`, `main.rs`, `ci.rs`; `watch.rs` and `plan.rs` get none.

## R7 Release smoke test

- `rust-ci-runner-release.yml` runs `ci` with no token on `x/y` and demands the unlisted line. Decision: an empty token keeps that line.

## R8 Heartbeat

- The `verdict` heartbeat step runs on `always()`; a red plan still pings. Decision: leave it; document that only the red run signals a bad list.

## R9 Unverified

- Bitwarden's maximum secret value length (believed 3,500 characters). T002 checks it.
- Bitwarden rate limits for 288 calls a day. Believed fine; T025 watches for failures.
