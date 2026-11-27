# Implementation Plan: Run the runner's unit tests through the runner

**Branch**: `006-runner-unit-tests` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/006-runner-unit-tests/spec.md`

## Summary

Add `rust-ci-runner/.factory-ci.toml` with one step that runs the `--lib` unit tests in a `rust:1.90` container, and give `factory-monorepo-ci.yml` a `pipeline` choice input that points the same build, refusal and run steps at either the factory-monorepo clone or `rust-ci-runner/`. No research, data model, contracts or quickstart: the design is fixed by the spec and fits here.

## Technical Context

**Language/Version**: GitHub Actions YAML, bash, TOML; Rust 1.90 inside the step container

**Primary Dependencies**: none new. The runner, `bws`, `jq` and the pinned checkout from 005.

**Storage**: files in `$RUNNER_TEMP` only, as in 005

**Testing**: static checks: YAML parse, `bash -n` on each changed `run:` block, grep that no `${{` sits in a `run:` block, TOML parse with only `image`, `commands`, `timeout`, no `docker = true`. The owner's first manual run is the end-to-end check; a workflow cannot run itself before merge.

**Target Platform**: GitHub-hosted `ubuntu-24.04`

**Project Type**: CI workflow and pipeline file

**Constraints**: step container has `--pids-limit 512` (`rust-ci-runner/src/engine.rs`, `run` argv); no `docker` binary inside it; 30 minute job timeout

**Scale/Scope**: one new 20-line file, about 25 changed lines in one workflow, one README paragraph

## Constitution Check

- I One home: workflow stays in `.github/workflows/`, the pipeline file in `rust-ci-runner/`. Pass.
- II Secrets: unchanged. No value committed. Pass.
- III Public logs: output filter and Loki routing unchanged; the new summary line holds a fixed pipeline name and `main`. Pass.
- IV Simplicity: one input, one `DIR` variable, no new script. Pass.
- V Tested: this gives the runner's unit tests a pipeline that runs in CI. The workflow change itself has static checks plus the owner's first run, stated not hidden. Pass.
- VI Spec Kit: this folder. Pass.

## Design

Test selection, checked against the source on `main`:

- `ship_tests` and `job_tests` are `#[cfg(test)] mod` declarations in `src/lib.rs`, so the filters `ship_tests::` and `job_tests::` select them under `--lib`.
- No other lib test starts a process: `Command::new` appears only in `src/engine.rs`, which has no test module. `job_tests.rs` writes fake engine scripts using `/bin/sh` and `/usr/bin/env`, both in `rust:1.90`.
- `tests/executor.rs` and the other files in `tests/` call the real `docker` binary or the built CLI; they are out of scope.
- The three commands split the processes: everything except the two mock-server modules, then each of them alone, so one process stays under the 512 pid limit.
- No `--locked`: `Cargo.lock` is not on `main`. The file carries `# shortcut: no Cargo.lock on main, deps float; add --locked when one is committed.`
- The 003 branch has the same idea with `--locked`, clippy and rustfmt. Those are out of scope here.

Workflow changes:

1. Input `pipeline`: `type: choice`, options `factory-monorepo`, `rust-ci-runner`, default `factory-monorepo`. The `ref` description says it is ignored for `rust-ci-runner`, which tests the `main` checkout in the workspace.
2. `validate ref` and `fetch token and clone factory-monorepo` get `if: inputs.pipeline == 'factory-monorepo'`.
3. `refuse docker steps` and the run step get `env: DIR: ${{ inputs.pipeline == 'rust-ci-runner' && format('{0}/rust-ci-runner', github.workspace) || format('{0}/target', runner.temp) }}` and use `$DIR` where `$RUNNER_TEMP/target` stood.
4. The run step gets `PIPELINE` through `env:`. The summary label is `$PIPELINE $REF` for factory-monorepo and `rust-ci-runner main` otherwise. It is renamed `run pipeline`.
5. Concurrency group: `factory-monorepo-ci-${{ inputs.pipeline }}-${{ inputs.ref }}`.
6. The header comment names both pipelines and points to this folder.
7. Everything else stays: `bws` install, Loki keys, masking, filter, main-only guard, Environment.

The `bws` install and Bitwarden reads still run for `rust-ci-runner`: the run step needs the Loki keys.

## Project Structure

```text
rust-ci-runner/.factory-ci.toml             # new
.github/workflows/factory-monorepo-ci.yml   # changed
README.md                                   # changed section
specs/006-runner-unit-tests/                # spec, plan, tasks
```

**Structure Decision**: no new script; the workflow keeps one file, the runner gets its root pipeline file as every repo does.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
|-----------|------------|-------------------------------------|
| Runner built with cargo in a plain workflow (rule: compile, test and lint run only through the CI runner) | Owner exception of 2026-11-27 from 005, kept as is. The unit tests themselves run through the runner, inside its step container. | Only the runner's own build is exempt; nothing is added to the exception. |
