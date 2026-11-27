# Implementation Plan: Runner image in the pipeline workflow

**Branch**: `007-runner-image` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/007-runner-image/spec.md`

## Summary

Replace the compile step of `factory-monorepo-ci.yml` with a step that copies the runner binary out of the released image, point the run step at that binary, and add a Loki probe to the run step. No research, data model, contracts or quickstart: the design is fixed by the spec and fits here.

## Technical Context

**Language/Version**: GitHub Actions YAML, bash

**Primary Dependencies**: none new. `docker`, `curl`, `jq` on the hosted runner; the image from `rust-ci-runner-release.yml`.

**Storage**: files in `$RUNNER_TEMP` only

**Testing**: static checks: YAML parse, `bash -n` on each changed `run:` block, grep that no `${{` sits in a `run:` block, grep that `cargo` is absent and `DIGEST_PLACEHOLDER` appears once. A shell test of the URL normalisation. The owner's first manual run is the end-to-end check; a workflow cannot run itself before merge.

**Target Platform**: GitHub-hosted `ubuntu-24.04`

**Project Type**: CI workflow

**Constraints**: image is `FROM scratch`: no shell, no docker CLI, so extract the binary with `docker create` and `docker cp`; probe has a 15 s limit; credentials not in argv.

**Scale/Scope**: about 40 changed lines in one workflow, plus README text.

## Constitution Check

- I One home: workflow stays in `.github/workflows/`. Pass.
- II Secrets: Loki settings and clone token stay in Bitwarden; no value committed; `GITHUB_TOKEN` is the job's own. Pass.
- III Public logs: the probe prints one fixed line with a status code and one fixed hint; no URL, user or token. Credentials leave argv. Pass.
- IV Simplicity: one step replaced, one block added, no script. Pass.
- V Tested: static checks plus a shell test of the normalisation; end-to-end by the owner's first run, stated not hidden. Pass.
- VI Spec Kit: this folder. Pass.

## Design

1. Top-level `env: RUNNER_IMAGE: ghcr.io/sudtanj/rust-ci-runner:0.1.0@sha256:DIGEST_PLACEHOLDER`, with the comment. Job permissions gain `packages: read`.
2. Step `fetch rust-ci-runner` replaces `build rust-ci-runner`, same position. `env:` carries `RUNNER_IMAGE`, `GITHUB_TOKEN`. Script: `docker login ghcr.io -u "$GITHUB_ACTOR" --password-stdin`, `docker pull`, `cid=$(docker create)`, `docker cp`, `docker rm`, `chmod +x`. `mkdir -p "$RUNNER_TEMP/bin"` first. A `trap` removes the container on failure.
3. Run step: binary path becomes `$RUNNER_TEMP/bin/rust-ci-runner`.
4. Run step, after the key loop and before `unset json val`'s successor line: strip trailing `/`, then `/loki/api/v1/push`, export. Probe with `jq -nc` body, `curl -K -` for credentials, print `log store answered HTTP <code>`, fail on non-2xx. `$(date +%s%N)` gives the nanosecond timestamp Loki wants.
5. Header comment rewritten. `cargo` is not named in the workflow.
6. bws install, clone, filter, guards stay.
7. `rust-ci-runner-release.yml`, job `build`: `runs-on: ${{ matrix.runner }}`; each matrix entry gets `runner` and uses `musl-gcc` with `musl-tools`; a comment says why. `rust-toolchain.toml` already lists both musl targets.

## Project Structure

```text
.github/workflows/factory-monorepo-ci.yml   # changed
.github/workflows/rust-ci-runner-release.yml  # changed: native build runners
README.md                                   # changed section
rust-ci-runner/README.md                    # only if it says how CI builds the runner
specs/007-runner-image/                     # spec, plan, tasks
```

**Structure Decision**: no new script.

## Complexity Tracking

No violations. The build exception of 005 is retired for this workflow; `rust-ci-runner-release.yml` remains the owner exception for building the runner.
