# Implementation Plan: Infrastructure Home

**Branch**: `003-infrastructure-home` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-infrastructure-home/spec.md`

## Summary

The runner and every workflow now live in `sudtanj/infrastructure` (public). This feature finishes the move and the first run: wording, paths, the watch list and `ubuntu-24.04` pins; the first compile with `Cargo.lock`, `--locked` and `cargo fmt`; the log credentials from Environment secrets instead of Bitwarden; this repo's own pipeline files; a README Setup that lists every secret, variable and hand step.

Approach: change scripts and workflows only. The Rust code already reads `FACTORY_CI_LOG_*` from the environment. CI is the only compiler (1 GB VM), so the first work item is a selftest run that produces the lock file and a format patch as an artifact, then a loop of push, read the public log, fix.

## Technical Context

**Language/Version**: Bash (scripts, lint), GitHub Actions YAML, TOML, Rust 1.90.0 (toolchain pinned; `rust-version = "1.82"` declared)
**Primary Dependencies**: none added. Existing: `jq`, `openssl`, `curl`, `bws` 2.1.0 (pinned by SHA-256), `gh` (engineer's tool for run logs and artifacts)
**Storage**: none
**Testing**: shell tests (`.github/scripts/test-*.sh`), the workflow lint and its self-test, `cargo test` in CI, the selftest workflow
**Target Platform**: GitHub-hosted `ubuntu-24.04`
**Project Type**: CI runner plus workflows plus docs
**Performance Goals**: a tick with no work stays under 2 minutes and makes no Bitwarden call
**Constraints**: no cargo, docker, make or test run on the dev VM; no private value in public output; every action pinned by SHA
**Scale/Scope**: 2 watched repos; about 15 files change, 3 files added (`.factory-ci.toml` x2, one shell test)

## Constitution Check

Gate against `.specify/memory/constitution.md` v1.0.0.

| Principle | Result |
| --- | --- |
| I. One home for automation | Pass. FR-017 keeps factory-monorepo workflow-free; the watch list names both repos. |
| II. Secrets source | Pass after T024. Log credentials move to Actions secrets (the constitution already lists them there; the README said otherwise). Pipeline secrets stay in Bitwarden. T024 widens the wording to Environment secrets (PATCH 1.0.1). |
| III. Nothing private in public logs | Pass. FR-015 and tests T006. The new artifact (`Cargo.lock`, `fmt.patch`) holds public code only. |
| IV. Simplicity | Pass with two entries in Complexity Tracking. |
| V. Tested changes | Pass. Every FR maps to a test task (table below). Bootstrap exception stated (FR-020). |
| VI. Spec-driven | Pass. Every change here is in `tasks.md`; the PR links this folder. |
| Pinned actions, minimum permissions | Pass. `actions/upload-artifact` is pinned by SHA (T001); selftest keeps `contents: read`. |

Re-check after design: no new violation. The constitution amendment (T024) is the only governance step.

## Architecture decision

Two options for the log credentials, and one for the first compile. Detail in `research.md` D1 to D4.

**Chosen**: scripts read `FACTORY_CI_LOG_*` from the job environment (`fci_log_env`); the tick workflow passes them as Environment secrets to the `plan` step and the two `job` steps only; the plan's Bitwarden token check stays; a job calls Bitwarden only when its pipeline lists a secret. The first compile runs in the selftest, which uploads the lock file and a format patch.

**Rejected**: Bitwarden as source with an Actions fallback (two sources, partial states, breaks Principle II); dropping the plan's store check (a revoked token would turn one failed tick into many item errors); a local `docker run cargo generate-lockfile` (Docker forbidden on the VM); a hand-written lock (invalid).

## Project Structure

### Documentation (this feature)

```text
specs/003-infrastructure-home/
├── spec.md
├── plan.md
├── research.md
├── data-model.md
├── quickstart.md
├── contracts/
│   ├── bootstrap-secrets.md
│   ├── pipeline-files.md
│   └── selftest-workflow.md
├── checklists/requirements.md
└── tasks.md
```

### Source changes (repository root)

| Path | Change | Tasks |
| --- | --- | --- |
| `.github/workflows/factory-ci-selftest.yml` | `rustup toolchain install`; fmt, clippy and test steps; `bootstrap` job (temporary); `own-pipelines` job; strict lint and `bws` hash check; `ubuntu-24.04`; `--locked`; trigger paths; header states the bootstrap exception | T001, T003, T014, T020, T021, T026 |
| `.github/workflows/rust-ci-runner-release.yml` | `--locked`; `ubuntu-24.04` x3; summary line about the public package; header states the exception | T003, T014, T017, T021 |
| `.github/workflows/factory-ci-tick.yml` | `FACTORY_CI_LOG_*` in three steps; fixed message when the image is not pullable; header wording | T012, T016 |
| `.github/workflows/factory-ci-mirror-images.yml` | none (already `ubuntu-24.04`) | none |
| `.github/scripts/factory-ci-secrets.sh` | `fci_log_env` replaces `fci_log_creds` | T009 |
| `.github/scripts/factory-ci-plan.sh` | all-or-none at start; logs from env; header wording | T010, T016 |
| `.github/scripts/factory-ci-job.sh` | logs from env; skip the list call without names | T011 |
| `.github/scripts/factory-ci-lint-workflow.sh` | rules `runs-on-pinned`, `no-cargo`, `log-secrets-scope` with self-test rows; header points to the 003 contract too | T013 |
| `.github/scripts/test-factory-ci-secrets.sh` | cases for `fci_log_env`; drop cases for `fci_log_creds` | T005 |
| `.github/scripts/test-factory-ci-hardening.sh` | plan and job cases (partial, no token, no `bws` call, sentinel); README path moves to the root README | T006, T016 |
| `.github/scripts/test-factory-ci-docs.sh` | new: stale wording, watch entries, README against workflows both ways, `ubuntu-latest` | T007 |
| `.github/factory-ci-repos.toml` | two entries | T015 |
| `.factory-ci.toml` | new, root pipeline | T019 |
| `rust-ci-runner/.factory-ci.toml` | new, runner pipeline | T018 |
| `rust-ci-runner/Cargo.toml` | `rust-version = "1.82"` | T003 |
| `rust-ci-runner/Cargo.lock` | new, from the artifact | T003 |
| `rust-ci-runner/Makefile` | `--locked` | T003 |
| `rust-ci-runner/src/**` | compile and format fixes from the loop; no feature change | T004 |
| `README.md` | Setup rewrite | T022 |
| `rust-ci-runner/README.md` | "Set up B" becomes a pointer; log and secrets sections say Actions secrets; wording | T023 |
| `.specify/memory/constitution.md` | v1.0.1 | T024 |
| `specs/002-ci-runner/{tasks,spec,quickstart}.md` | pointers to 003 | T025 |

### Structure Decision

Single repo layout from the constitution. No new top-level folder.

## Test plan

QA writes tests first (T005 to T008); they fail until the impl tasks land. Each FR maps to at least one test.

| Requirement | Test |
| --- | --- |
| FR-001 | `test-factory-ci-docs.sh` stale-wording case (T007), with a mutated copy |
| FR-002 | docs test: both repos listed; selftest `list` job runs `plan --check` |
| FR-003 | lint rule `runs-on-pinned` and its self-test row (T013); docs test grep |
| FR-004 | docs test: README says "public"; hardening test: tick has the fixed message in all three jobs (T006) |
| FR-005 | docs test (wording); T027 read-through |
| FR-006 | selftest `pipeline` (`--locked` fails on a stale lock); docs test greps `--locked` on every cargo line of the Makefile, selftest and release |
| FR-007 | selftest `cargo fmt --check`; docs test checks `rust-version` |
| FR-008, FR-009 | the selftest run itself (SC-001); T026 checks the `bootstrap` job is gone |
| FR-010, FR-011, FR-012 | T005 (`fci_log_env` on, off, partial, short line), T006 (plan fails at start, job refuses) |
| FR-013 | T006: no `bws` call for a no-secret pipeline; unset token still runs |
| FR-014 | existing `pipeline.rs` test on reserved names stays green (T004 gate) |
| FR-015 | T006 sentinel case |
| FR-016 | docs test: README names both repos next to `FACTORY_CI_TOKEN` |
| FR-017 | docs test (i): no workflow names `factory-monorepo`; lint `no-cargo` (T007, T013) |
| FR-018 | `own-pipelines` runs `.factory-ci.toml` files (T020) |
| FR-019, FR-021 | `own-pipelines` job (T020) |
| FR-020 | lint rule `no-cargo`, docs test on the README sentence |
| FR-022, FR-023 | docs test both ways (T007) |
| FR-024 | docs test: `rust-ci-runner/README.md` has no "Set up B" |
| FR-025 | T025 grep of 002 `tasks.md` |
| FR-026 | T024: `Version` line reads 1.0.1 |
| FR-027 | lint rule `no-pull-request-target` over every workflow and its self-test row (T013) |
| SC-003 to SC-006 | quickstart Part 3 (T030 to T032) |

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
| --- | --- | --- | --- | --- |
| T002 artifact | The `bootstrap` job fails or the artifact is missing | `gh run download` exits non-zero | Engineer reads the log, fixes the job, reruns | none: manual loop |
| T002 lock | Lock holds a dependency that needs rustc newer than 1.90 | Compile error in the loop | Pin the dependency version in `Cargo.toml` or the lock, rerun | selftest |
| T004 loop | A fix loosens a test | Review: diff touches an assertion | Revert, fix the source | review |
| T004 toolchain | `rustup` does not install the pinned toolchain | `cargo` says toolchain missing | Add or fix `rustup toolchain install` step | selftest |
| T010 plan | One or two log secrets set | `fci_log_env` returns 1 | `fail` with names, tick red, no status posted | T005, T006 |
| T010 plan | Value has a short line | `fci_short_line` | `fail`, no values printed | T006 |
| T011 job | Partial set in `factory-ci-docker` only | `fci_log_env` in job | `refuse`: item `error`, one retry | T006 |
| T011 job | Bitwarden down, pipeline lists no secret | no `bws` call | Item runs and ships logs | T006 |
| T011 job | Bitwarden down, pipeline lists a secret | `fci_list` fails | `refuse "secret store unreachable"` (unchanged) | existing test |
| T012 tick | Package is private | `docker create` fails | Fixed one-line message, job red | T006 |
| T012 tick | Secret in a step that should not have it | lint `log-secrets-scope` | selftest `workflow` job fails | T013 self-test |
| T013 lint | Line-based read misses a YAML form | self-test rows; `shortcut` note in the script | Rule false negative: reviewer catches | T013 |
| T015 watch list | Bad entry | `plan --check` | Selftest `list` job red | selftest |
| T018 pipeline | `rust:1.90` missing rustfmt or toolchain mismatch with `rust-toolchain.toml` | `own-pipelines` | Fix commands in the file | T020 |
| T019 pipeline | A shell test needs the host (`sudo`, `iptables`) | `own-pipelines` | Test stays in the selftest `shell` job only, with a comment | T020 |
| T020 selftest | `own-pipelines` takes over 30 minutes | job timeout | Raise `timeout-minutes` to 45 or split steps | selftest |
| T022 README | Name added to a workflow without the README | docs test | Selftest `shell` job red | T007 |
| T026 | Last fmt patch not applied | `cargo fmt --check` red | Apply the patch, rerun | selftest |
| Live: tick | Environment missing | `plan` guard fails on blob key | Tick red, no repo touched | quickstart Part 3 |
| Live: tick | `FACTORY_CI_TOKEN` lacks one repo | GitHub 404 on that repo | Plan reports it; other repo proceeds | quickstart Part 3 |
| Live: scheduler | Token expired | dispatch returns 401 | No tick; heartbeat goes quiet; owner alert | quickstart Part 3 |

## Pre-mortem

Assume 003 shipped and broke. Five likeliest causes.

1. **The first run never turns green and the PR sits.** The crate never compiled; errors are many. Prevention: T004 is an explicit loop with a gate, fixes go to the source, and the PR does not merge before green. The artifact job keeps producing a fresh fmt patch.
2. **A log credential leaks to a public log.** More jobs now hold them in the environment. Prevention: `log-secrets-scope` lint, masking before export, sentinel test (T006), quickstart check 5 signed out, and the credentials stay out of step containers (002 FR-029 test stays green).
3. **The first tick fails because the image is private or an Environment is empty.** Prevention: README order puts "make the package public" before the variable; fixed message names the fix; the plan guard names the missing blob key; quickstart Part 3 starts with a manual dispatch.
4. **A pipeline file fails at its first real run.** The tick would be the first run. Prevention: `own-pipelines` runs both files in the selftest before merge (FR-021).
5. **Docs drift from the workflows again.** Prevention: the docs test compares names both ways and checks wording, so the next workflow change fails the PR.

## Invariants (QA turns each into a property test)

- P1. For any subset of the three log variables: three set gives `on`, none gives `off`, any other gives an error that names only variable names.
- P2. For any value (including newline, `%`, a 3-character line, a 10 KB line) set as a log or bootstrap secret, the value never appears in the output of `plan.sh` or `job.sh` outside `::add-mask::` lines.
- P3. For any pipeline with an empty secret list and any state of `BWS_ACCESS_TOKEN`, `job.sh` calls `bws` zero times.
- P4. For any workflow file in the directory, every `runs-on` is `ubuntu-24.04`.
- P5. For any workflow other than the selftest and release files, no line starts a `cargo` command.
- P6. The set of `secrets.X` and `vars.X` names in the workflows (minus `GITHUB_TOKEN`) equals the set of names in the README Setup tables.
- P7. A pipeline secret resolves from Bitwarden only: with the Bitwarden token unset and any GitHub secret set under the same name, the item ends in `error`.
- P8. Running `plan` twice on an unchanged repo posts no second status (002 invariant, kept).

## Edge cases (feiyi-edge-cases)

- **Inputs**: handled. Empty, partial and short-line log secrets (T005, T006); a secret with `%`, newline or CR is masked with the existing escaping (002); unicode in a name is out of reach because names are fixed. Two watch entries for one repo: `plan --check` rejects duplicates (002 test). Maximum: the cap of 20 items per tick is unchanged.
- **State**: handled. First run with no data: no statuses, the first tick plans everything; a self-watch run of an unchanged repo gets "unchanged since". Migration: Bitwarden still holds the old `FACTORY_CI_LOG_*` keys; they are ignored and the README tells the owner to delete them. Crash mid-tick: unchanged 002 behaviour (stale `pending` after 2 h). Partial failure of this feature: the lock lands before `--locked`, so no build breaks in between (T003 order). Retry: all scripts are idempotent.
- **Time and concurrency**: handled. Two ticks at once: the `concurrency` group stays. Timeouts: every external call keeps its timeout; the new `docker create` has none, and the job's `timeout-minutes` bounds it. Token expiry: README says rotate before day 80. Clock: N/A, no new time logic.
- **Resources**: handled. Memory: nothing runs on the VM; CI jobs have 7 GB. A Rust compile in a container adds time (D8, 30 minute timeout). Artifacts: 3-day retention, a few hundred KB. Logs: unchanged bounds.
- **Permissions and trust**: handled. Environment secrets only on `main`; PR workflows and forks get none; the artifact job reads no secret; the scheduler token can only start ticks. A private repo's names appear in the watch list (accepted, 002 FR-058). The log credentials are in the environment of a job that clones untrusted code: the runner never passes them to step containers; the docker job already holds root-on-host trust.
- **UI**: N/A for screens. The README is the only interface; its states are covered by the order in Setup and quickstart Part 3.

## Rollout

1. Open the PR. The selftest is the first compile (T001 to T004). Fix until green.
2. Merge when the selftest is green and the owner has read the README Setup.
3. Owner does Setup (README): Environments, Bitwarden machine accounts and projects, Grafana token, fine-grained tokens, Environment secrets. 
4. Owner pushes tag `rust-ci-runner-v0.1.0` on `main` (matches `Cargo.toml`). The release workflow pushes the image and prints the digest.
5. Owner makes the `rust-ci-runner` package public, sets `RUNNER_IMAGE_DIGEST`, runs `factory-ci-mirror-images`, makes those packages public, sets the two image variables and the Bitwarden and log link variables.
6. Owner sets up the scheduler (OQ-1) after one manual tick is green.
7. Live checks (T030 to T033).

## Rollback

- Before merge: close the PR.
- After merge, before Setup: revert the merge commit; nothing runs, because there is no scheduler.
- After Setup: disable the scheduler job first (stops ticks), then revert. Statuses already posted stay (002 rule).
- Logs: with the log secrets removed from an Environment, shipping stops; a private repo is refused (fail closed). Pipelines without secrets keep running.
- Lock file: reverting `--locked` and the lock together restores the old build; do not revert one alone.

## Open risks

- R-1. The compile loop count is unknown. Each push costs one public run and a wait. Mitigation: read the whole log per run, fix every error in it before the next push.
- R-2. `rust-version = "1.82"` may disagree with a dependency. Low: see D5.
- R-3. `own-pipelines` inside containers may expose test assumptions (root-owned files, missing tools). The first run decides; T019 has a fallback.
- R-4. A public repo's workflow log is public: the first compile failures show the code. It is public code. Accepted.
- R-5. cron-job.org (or the owner's pick) is a single outside dependency; the heartbeat alert (002 FR-056) covers a quiet tick.
- R-6. Self-watch posts statuses on every branch of this repo, including PR branches; the tick cap of 20 items can delay the monorepo during a burst. Accepted; revisit if it shows.

## DevOps assessment

No assessment was handed over; this is the EM's own, for the lead to correct.

- **Infra**: nothing to provision. GitHub-hosted runners, GHCR, Bitwarden Secrets Manager free plan (2 of 3 projects, 2 of 3 machine accounts), Grafana Cloud free Loki (14-day retention), one external scheduler.
- **Required changes folded into steps**: runner label pin (T014, T013), `--locked` builds (T003), public GHCR package and its message (T012, T017, T022), scheduler token and period (T022), 90-day token rotation note (T022).
- **Cost**: public repo, free minutes. A self-compile in `own-pipelines` adds a few minutes per PR.
- **Observability**: unchanged: heartbeat ping, one Loki line per tick with work, `verdict` job.
- **Cache**: none added (YAGNI); cargo registry downloads each run.

## Security assessment

No assessment was handed over; this is the EM's own, for the lead to correct.

- **Moved risk**: log credentials leave Bitwarden for Environment secrets. The Environments are limited to `main`, which limits who can read them to workflows on `main`. Anyone who can merge to `main` can read them: already true for every bootstrap secret.
- **New exposure**: the `plan` job and both run jobs hold the credentials in step environments. Mitigations: `log-secrets-scope` lint, `::add-mask::` before use, `set +x` in scripts, the runner keeps them out of step containers.
- **Fork PRs**: selftest runs with `contents: read` and no secret. `pull_request` (not `_target`) stays; the lint's `no-pull-request-target` rule applies to the tick only, and T013 extends it to every workflow (FR-027).
- **Artifact**: holds `Cargo.lock` and a format patch of public code; retention 3 days; the job reads no secret.
- **Supply chain**: `--locked` plus a committed lock closes drift; `actions/upload-artifact` pinned by SHA; the `bws` archive hash is checked against `BWS_SHA256` in CI (T020).
- **GHCR public**: the image holds only the binary of public code.
- **Private repo name**: visible in the public watch list (002 FR-058, accepted).
- **Scheduler token**: fine-grained, this repo only, Actions write, 90 days; leak means unwanted ticks, not secret access.
- **Constitution**: Principles II and III hold. T024 clarifies the wording only.

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| Selftest keeps cargo, shell and lint jobs while the runner runs the same checks | No runner exists to build the first one (bootstrap exception); the workflow stays as the pre-merge gate until the runner is proven (OQ-5) | Removing them leaves the PR with no check before the first tick |
| New `test-factory-ci-docs.sh` | The constitution says the README lists every name the workflows read; only a test keeps it true | A review checklist drifts |
