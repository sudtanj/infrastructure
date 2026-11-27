# Implementation Plan: Plain Actions CI for factory-monorepo

**Branch**: `005-plain-actions-ci` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/005-plain-actions-ci/spec.md`

## Summary

One workflow file, one job. It validates `ref`, installs a pinned `bws`, reads `FACTORY_CI_TOKEN` from Bitwarden, clones factory-monorepo, builds `rust-ci-runner` with cargo, runs the pipeline and prints only allow-listed result lines. No research, data model, contracts or quickstart: the design is fixed by the spec and fits here.

## Technical Context

**Language/Version**: GitHub Actions YAML and bash; Rust 1.90.0 from `rust-ci-runner/rust-toolchain.toml` (rustup on the hosted runner)

**Primary Dependencies**: `actions/checkout` pinned to `11d5960a326750d5838078e36cf38b85af677262` (v4); `bws` 2.1.0; `jq`, `unzip`, `curl`, Docker (preinstalled on `ubuntu-24.04`)

**Storage**: files in `$RUNNER_TEMP` only (`bin/`, `target/`, `runner.log`); nothing uploaded

**Testing**: no code under test. Checks: YAML parse, `bash -n` on each extracted `run:` block, grep proving no `${{` inside a `run:` block, and the owner's manual run. A workflow cannot run itself in a pull request: it is `workflow_dispatch` on `main` only.

**Target Platform**: GitHub-hosted `ubuntu-24.04`

**Project Type**: CI workflow

**Constraints**: 30 minute timeout; every network call has a timeout; public log carries no secret and no private output

**Scale/Scope**: one file, about 120 lines

## Constitution Check

- I One home: workflow lives in `.github/workflows/`. Pass.
- II Secrets: `BWS_ACCESS_TOKEN` from the Environment `factory-ci`; `FACTORY_CI_TOKEN` from Bitwarden, a pipeline-side secret read at run time. No value committed. Pass.
- III Public logs: values masked (raw and base64); runner output goes to a file; printed lines pass a strict filter (below). Pass.
- IV Simplicity: no cache, no artifacts, no statuses, no helper script. Pass.
- V Tested: see Testing. The workflow cannot run in CI before merge; the check is static plus the owner's first run. Stated, not hidden.
- VI Spec Kit: this folder. Pass.

## Design

Steps, in order:

1. `actions/checkout` of this repo, `persist-credentials: false`.
2. Validate `REF` (from `env:`): `^[A-Za-z0-9._/-]+$`, no leading `-`, no `..`. Fails before any secret is read.
3. Install `bws`: `curl -fsSL --proto '=https' --tlsv1.2 --max-time 120`; SHA-256 of the zip must equal `f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6` (the digest of the zip, per the comment in `.github/scripts/factory-ci-secrets.sh` on `origin/003-infrastructure-home`); `unzip -Z1` must list exactly `bws`; install to `$RUNNER_TEMP/bin/bws`, called by full path. `FACTORY_CI_BWS_SERVER_URL`, if set, must start with `https://`.
4. Fetch and clone in one step, the only step that sees `BWS_ACCESS_TOKEN`: `bws secret list -o json | jq` for key `FACTORY_CI_TOKEN`, exactly one match; check the token shape `^[A-Za-z0-9_.-]{8,}$` (also blocks workflow-command injection); `::add-mask::` for the raw value and its base64 header form before any other use; `git clone --depth 1 --branch "$REF"` with the header passed through `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_0`/`GIT_CONFIG_VALUE_0` in that command's environment only, so it is in neither argv nor `.git/config`. Git's own stderr is discarded; the step prints its own message.
5. Build in `rust-ci-runner/` (so rustup reads its `rust-toolchain.toml`): `cargo build --release`, without `--locked` because no `Cargo.lock` is committed (comment in the workflow says when to add it).
5a. Refuse docker steps (FR-012): fail unless `grep -qE` finds no `docker = true` (quotes and spacing tolerated) in the clone's root `.factory-ci.toml`, the only file the runner reads (`pipeline.rs`, symlink refused). A grep error or missing file also fails the job. Nothing is printed except a fixed message.
6. Run `rust-ci-runner run --dir "$RUNNER_TEMP/target" --quiet-logs` with `FACTORY_CI_SECRETS=` (no step secret is readable) and a throwaway loopback log store (`FACTORY_CI_LOG_URL=http://127.0.0.1:9`, user and token placeholders): `job.rs` `log_config` refuses quiet mode without one, `ship.rs` accepts plain http to 127.0.0.1, shipping fails silently and the flush can add up to 60 s. Stdout and stderr go to `$RUNNER_TEMP/runner.log`. This step has no token in its environment. Filter the file, print, append to `$GITHUB_STEP_SUMMARY`, exit with the runner's code.

Output filter. Read from `log.rs`, `engine.rs` and `job.rs`: every printed runner line is `[<who>] <text>`; with `--quiet-logs`, step and clone output is not printed at all, only `event` lines, whose text is fixed: `started`, `success in Ns`, `failed in Ns: <reason>`, `skipped: <why>`, and `pipeline passed` or `pipeline failed` for `runner`. Step names match `[A-Za-z0-9_-]{1,64}`. Filter:

```text
grep -E '^\[[A-Za-z0-9_-]{1,64}\] (started|success in [0-9]+s|failed in [0-9]+s(: .*)?|skipped(: .*)?|pipeline (passed|failed))$' | sed -E 's/^(\[[^]]+\] (failed in [0-9]+s|skipped)):.*/\1/'
```

The summary line `factory-monorepo <ref>: passed` or `failed` comes from the exit code. If the exit code is non-zero and the filter printed no line, the fixed line `runner failed before running any step (see a run with a log store for details)` is added to the output and the summary; the log is never echoed.

## Project Structure

```text
.github/workflows/factory-monorepo-ci.yml   # new
README.md                                   # new section
specs/005-plain-actions-ci/                 # spec, plan, tasks
```

**Structure Decision**: one workflow file, no script file, so deleting the stopgap later removes one file and one README section.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| `cargo build` of the runner outside the runner (rule: compile, test and lint run only through the CI runner) | Owner decision, 2026-11-27: the runner cannot build itself before it exists, and the image release (feature 003) is unmerged | Waiting for the image blocks the owner. Only the runner's own build is exempt; factory-monorepo steps still run through `rust-ci-runner`. |
