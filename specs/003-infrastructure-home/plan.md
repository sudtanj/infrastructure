# Implementation Plan: Infrastructure Home

**Branch**: `003-infrastructure-home` | **Date**: 2026-11-27 (review round 1 folded) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-infrastructure-home/spec.md`

## Summary

The runner and every workflow now live in `sudtanj/infrastructure` (public). This feature finishes the move and the first run: wording, paths, the watch list and `ubuntu-24.04` pins; the first compile with `Cargo.lock`, `--locked` and `cargo fmt`; the log credentials from Environment secrets instead of Bitwarden; this repo's own pipeline files; a README Setup that lists every secret, variable, ruleset and hand step.

Approach: change scripts and workflows only, plus `#[ignore]` on the engine tests and the compile fixes. The Rust code already reads `FACTORY_CI_LOG_*` from the environment. CI is the only compiler (1 GB VM), so the first work item is a bootstrap workflow that produces the lock file and a format patch as an artifact, then a loop of push, read the public log, fix, with a budget of 5 runs. The selftest also dry-runs the release (both musl targets and an unpushed image build) so a release problem shows before the tag.

## Technical Context

**Language/Version**: Bash (scripts, lint), GitHub Actions YAML, TOML, Rust 1.90.0 (toolchain pinned; `rust-version = "1.90"` declared)
**Primary Dependencies**: none added. Existing: `jq`, `openssl`, `curl`, `bws` 2.1.0 (pinned by SHA-256), `gh` (the engineer's tool for run logs and artifacts)
**Storage**: none
**Testing**: shell tests (`.github/scripts/test-*.sh`), the workflow lint and its self-test, `cargo test` in CI, the selftest workflow
**Target Platform**: GitHub-hosted `ubuntu-24.04`
**Project Type**: CI runner plus workflows plus docs
**Performance Goals**: a tick with no work stays under 2 minutes and makes no Bitwarden call
**Constraints**: no cargo, docker, make or test run on the dev VM; no private value in public output or committed files; every action pinned by SHA
**Scale/Scope**: 2 watched repos; about 20 files change, 4 added (`.factory-ci.toml` x2, one shell test, one temporary workflow)

## Constitution Check

Gate against `.specify/memory/constitution.md` v1.0.0.

| Principle | Result |
| --- | --- |
| I. One home for automation | Pass. FR-017 keeps factory-monorepo workflow-free; the watch list names both repos. |
| II. Secrets source | Pass after T026. Log credentials move to Actions secrets (the constitution already lists them there; the README said otherwise). Pipeline secrets stay in Bitwarden. T026 widens the wording to Environment secrets (PATCH 1.0.1; Principle II does not name Bitwarden as the only bootstrap store, so no MINOR bump). |
| III. Nothing private in public logs | Pass. FR-015, FR-032, tests T008 and T009. The artifact holds `Cargo.lock` and a patch of public `.rs` files only. |
| IV. Simplicity | Pass with two entries in Complexity Tracking. |
| V. Tested changes | Pass. Every FR maps to a test task (table below). Bootstrap exception stated (FR-020). |
| VI. Spec-driven | Pass. Every change here is in `tasks.md`; the PR links this folder. |
| Pinned actions, minimum permissions | Pass. `actions/upload-artifact` reuses the existing SHA pin (T002); the selftest keeps `contents: read` and no secret. |

Re-check after design: no new violation. The constitution amendment (T026) is the only governance step.

## Architecture decision

Detail in `research.md` D1 to D18.

**Chosen**:
- Scripts read `FACTORY_CI_LOG_*` from the job environment (`fci_log_env`). The tick passes them as Environment secrets to the `plan` step and the two `job` steps only. The plan's Bitwarden token check stays. A job calls Bitwarden only when its pipeline lists a secret. `factory-ci-docker` holds its own Loki token.
- The first compile runs in a separate, temporary, same-repo-only bootstrap workflow that uploads two files; the selftest stays free of artifacts, secrets and Environments.
- Engine tests are `#[ignore]`d; the pipeline runs `cargo test --locked`; the selftest host job adds `--include-ignored`.
- Required checks on `main` are the selftest jobs; the runner's statuses stay informational.

**Rejected**: Bitwarden as source with an Actions fallback (two sources, partial states, breaks Principle II); dropping the plan's store check (a revoked token would turn one failed tick into many item errors); a bootstrap job inside the selftest (conflicts with the lint rule that bans artifacts there); a local `docker run cargo generate-lockfile` (Docker forbidden on the VM); a Docker-free test allowlist (rots); `workflow_dispatch` on the release workflow (a publish path from a non-tag ref).

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
| `.github/workflows/factory-ci-selftest.yml` | no `paths:`, `workflow_dispatch`, concurrency, `persist-credentials: false`; `rustup toolchain install`; jobs `check`, `pipeline` (fmt, clippy, test with `--include-ignored`), `build-release`, `own-pipelines`; strict lint of the directory; `bws` hash check; `ubuntu-24.04`; `--locked`; header states the exception | T001, T004, T006, T016, T022 |
| `.github/workflows/factory-ci-bootstrap.yml` | new, temporary: lock and format patch artifact | T002, deleted by T028 |
| `.github/workflows/rust-ci-runner-release.yml` | `rustup toolchain install`; `--locked`; `ubuntu-24.04` x3; `publish` in `factory-ci-release`; summary line; header states the exception | T004, T006, T016, T019, T023 |
| `.github/workflows/factory-ci-mirror-images.yml` | `environment: factory-ci-release`; `if: github.ref == 'refs/heads/main'` | T019 |
| `.github/workflows/factory-ci-tick.yml` | `FACTORY_CI_LOG_*` in three steps; fixed message when the image is not pullable; `https://` guard in `install bws` x3; header wording | T014, T018 |
| `.github/scripts/factory-ci-secrets.sh` | `fci_log_env` replaces `fci_log_creds` | T011 |
| `.github/scripts/factory-ci-plan.sh` | all-or-none at start; logs from env; `https://` guard; header wording | T012, T018 |
| `.github/scripts/factory-ci-job.sh` | logs from env; guard; skip the list call without names | T013 |
| `.github/scripts/factory-ci-lint-workflow.sh` | rules `runs-on-pinned`, `no-cargo`, widened `no-pull-request-target`, selftest rules, each with self-test rows | T015, T028 |
| `.github/scripts/test-factory-ci-secrets.sh` | cases for `fci_log_env`; drop cases for `fci_log_creds` | T007 |
| `.github/scripts/test-factory-ci-hardening.sh` | plan and job cases; grep counts; guard cases; README path moves to the root README | T008, T018 |
| `.github/scripts/test-factory-ci-docs.sh` | new: wording, watch entries, `--locked`, README against workflows both ways, README rows, private-identifier scan | T009 |
| `.github/factory-ci-repos.toml` | two entries | T017 |
| `.factory-ci.toml` | new, root pipeline | T021 |
| `rust-ci-runner/.factory-ci.toml` | new, runner pipeline | T020 |
| `rust-ci-runner/Cargo.toml` | `rust-version = "1.90"` | T001 |
| `rust-ci-runner/Cargo.lock` | new, from the artifact | T004 |
| `rust-ci-runner/Makefile` | `--locked` | T004 |
| `rust-ci-runner/src/**`, `rust-ci-runner/tests/**` | compile and format fixes; `#[ignore]` on engine tests; one `LOG_VARS` case if missing | T005, T008, T010 |
| `README.md` | Setup rewrite | T024 |
| `rust-ci-runner/README.md` | "Set up B" becomes a pointer; log and secrets sections say Actions secrets; wording | T025 |
| `.specify/memory/constitution.md` | v1.0.1 | T026 |
| `specs/002-ci-runner/{tasks,spec}.md` | pointers to 003 | T027 |

### Structure Decision

Single repo layout from the constitution. No new top-level folder.

## Test plan

QA writes tests first (T007 to T010); they fail until the impl tasks land. Each FR maps to at least one test.

| Requirement | Test |
| --- | --- |
| FR-001, FR-005 | docs test (a) with a mutated copy (T009); T029 read-through |
| FR-002 | docs test (b); selftest `list` job runs `plan --check` |
| FR-003 | lint rule `runs-on-pinned` and its self-test row (T015) |
| FR-004 | hardening test: fixed message in all three jobs (T008); docs test (e): README says "public" and the visibility command |
| FR-006, FR-007 | selftest (`--locked` fails on a stale lock; `cargo fmt --check`); docs test (c) |
| FR-008, FR-009 | the selftest run itself (SC-001); T028 greps for the bootstrap workflow |
| FR-010, FR-011, FR-012 | T007 (`fci_log_env` on, off, partial, short line), T008 (plan fails at start, job refuses, grep count 9, `LOG_VARS` case) |
| FR-013 | T008: no `bws` call for a no-secret pipeline; unset token with a same-name GitHub secret ends in `error` |
| FR-014 | existing `pipeline.rs` reserved-name tests stay green (T005 gate) |
| FR-015 | T008 sentinel case |
| FR-016 | docs test (e): README names both repos next to `FACTORY_CI_TOKEN` |
| FR-017 | lint `no-cargo` and the directory lint (T015, T016); README sentence (docs test (e)); quickstart check 12 |
| FR-018, FR-019, FR-021 | `own-pipelines` job (T022) |
| FR-020 | lint rule `no-cargo`; docs test (e) on the README sentence |
| FR-022, FR-023 | docs test (d), (e) both ways, including `environment:` names (T009) |
| FR-024 | docs test (f) |
| FR-025 | T027 grep of 002 `tasks.md` |
| FR-026 | T026: `Version` line reads 1.0.1 |
| FR-027 | lint rule `no-pull-request-target` over every workflow, with block, list and map mutation rows (T015) |
| FR-028 | selftest `build-release` job (T006) |
| FR-029 | lint selftest rules and their mutation rows (T015); strict lint of the directory (T016) |
| FR-030 | `own-pipelines` runs `cargo test --locked` in a container without an engine (T022); the host job runs `--include-ignored` |
| FR-031 | T008 guard cases; quickstart check 11 |
| FR-032 | docs test (g) with a mutated copy |
| FR-033 | docs test (d): `environment:` names; T019 |
| SC-003 to SC-006 | quickstart Part 3 (T032 to T034) |

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
| --- | --- | --- | --- | --- |
| T003 artifact | The bootstrap workflow fails or the artifact is missing | `gh run download` exits non-zero | Engineer fixes the workflow, reruns | none: manual loop |
| T003 artifact | A poisoned patch (touches a workflow) | patch limited to `rust-ci-runner/*.rs`; `git apply --include`; diff read before commit | Reject | review |
| T003 fork | A fork PR reaches the bootstrap workflow | `if:` on the head repo | Workflow does not run | lint reads the file (manual) |
| T004 lock | Lock holds a dependency that needs rustc newer than 1.90 | Compile error in the loop; resolver fallback is set | Pin the dependency in `Cargo.toml`, rerun | selftest |
| T005 loop | More than 5 red runs | counter in the task notes | Stop and report; plan revisited | none |
| T005 loop | A fix loosens a test | Review: diff touches an assertion | Revert, fix the source | review |
| T005 toolchain | `rustup` does not install the pinned toolchain | `cargo` says toolchain missing | `rustup toolchain install` step | selftest |
| T006 release dry run | Linker or package missing for aarch64 | `build-release` red | Fix the matrix like the release workflow | selftest |
| T007 plan | One or two log secrets set | `fci_log_env` returns 1 | `fail` with names, tick red, no status posted | T007, T008 |
| T012 plan | Value has a short line | `fci_short_line` | `fail`, no values printed | T008 |
| T012 plan | `FACTORY_CI_BWS_SERVER_URL` is `http://` | guard | `fail`, no download | T008 |
| T013 job | Partial set in `factory-ci-docker` only | `fci_log_env` in the job | `refuse`: item `error`, one retry | T008 |
| T013 job | Bitwarden down, pipeline lists no secret | no `bws` call | Item runs and ships logs | T008 |
| T013 job | Bitwarden down, pipeline lists a secret | `fci_list` fails | `refuse "secret store unreachable"` (unchanged) | existing test |
| T014 tick | Package is private | `docker create` fails | Fixed one-line message, job red | T008 |
| T014 tick | Log secret added to a fourth step | grep count 9 | hardening test red | T008 |
| T015 lint | Line-based read misses a YAML form | mutation rows for list and map forms; `shortcut` note | Known gap recorded in Open risks | T015 |
| T017 watch list | Bad entry | `plan --check` | Selftest `list` red | selftest |
| T020 pipeline | `rust:1.90` lacks rustfmt or mismatches `rust-toolchain.toml` | `own-pipelines` | Fix commands in the file | T022 |
| T021 pipeline | `git` refuses the mounted checkout | `own-pipelines` | `safe.directory`, or a fixed file list in the docs test | T022 |
| T022 selftest | `own-pipelines` over 30 minutes | job timeout | Raise to 45 or split steps | selftest |
| T024 README | Name added to a workflow without the README | docs test | `shell` job red | T009 |
| T028 | Last fmt patch not applied | `cargo fmt --check` red | Apply the printed diff by hand | selftest |
| T031 | First GHCR push is private | visibility check | Owner switches it to public | quickstart 13 |
| Live: tick | Environment missing or not limited | `plan` guard fails on blob key | Tick red, no repo touched | quickstart Part 3 |
| Live: tick | `FACTORY_CI_TOKEN` lacks one repo | GitHub 404 on that repo | Plan reports it; other repo proceeds | quickstart Part 3 |
| Live: monorepo | no `.factory-ci.toml` on its `main` | plan finds no project | Skipped, no status, no error | quickstart 12 |
| Live: scheduler | Token expired | dispatch returns 401 | No tick; heartbeat goes quiet; owner alert | quickstart Part 3 |
| Live: non-main dispatch | Environment refuses | job fails | `plan` does not run; `verdict` red; no secret read | quickstart 10 |

## Pre-mortem

Assume 003 shipped and broke. Five likeliest causes.

1. **The first run never turns green and the PR sits.** The crate never compiled. Prevention: T005 is a gate with a budget of 5 runs and a fast `check` job; fixes go to the source; the PR does not merge before green.
2. **A log credential leaks to a public log.** More jobs now hold them in the environment. Prevention: the grep count (9, three steps), masking before export, the sentinel test (T008), `LOG_VARS` keeps them out of step containers, quickstart check 5 signed out, and the separate docker token limits one escape.
3. **The first tick fails because the image is private, an Environment is empty or a ruleset is missing.** Prevention: README order puts "make the package public" before the variable; the fixed message names the fix; the plan guard names the missing blob key; quickstart Part 3 starts with a manual dispatch.
4. **The first release breaks after the tag.** Prevention: the selftest dry-runs both targets and the image build (T006) and the release workflow installs the toolchain explicitly.
5. **Docs drift from the workflows again.** Prevention: the docs test compares secret, variable and Environment names both ways and scans for private identifiers, so the next workflow change fails the PR.

## Invariants (QA turns each into a property test)

- P1. For any subset of the three log variables: three set gives `on`, none gives `off`, any other gives an error that names only variable names.
- P2. For any value (including newline, `%`, a 3-character line, a 10 KB line) set as a log or bootstrap secret, the value never appears in the output of `plan.sh` or `job.sh` outside `::add-mask::` lines.
- P3. For any pipeline with an empty secret list and any state of `BWS_ACCESS_TOKEN`, `job.sh` calls `bws` zero times.
- P4. For any workflow file in the directory, every `runs-on` is exactly `ubuntu-24.04`.
- P5. For any workflow other than the selftest, release and (while it exists) bootstrap files, no line starts a `cargo` command.
- P6. The set of `secrets.X`, `vars.X` and `environment:` names in the workflows (minus `GITHUB_TOKEN`) equals the set of names in the README Setup tables.
- P7. A pipeline secret resolves from Bitwarden only: with the Bitwarden token unset and any GitHub secret set under the same name, the item ends in `error`.
- P8. Running `plan` twice on an unchanged repo posts no second status (002 invariant, kept).
- P9. For any workflow file and any of the three `on:` forms (block, list, map), a `pull_request_target` trigger fails the lint.
- P10. For any `FACTORY_CI_BWS_SERVER_URL` that does not start with `https://` (empty aside), the scripts and the install steps refuse before any network call.

## Edge cases (feiyi-edge-cases)

- **Inputs**: handled. Empty, partial and short-line log secrets (T007, T008); a secret with `%`, newline or CR is masked with the existing escaping (002); a server URL with `http://`, a space or a newline is refused (T008). Two watch entries for one repo: `plan --check` rejects duplicates (002 test). Maximum: the cap of 20 items per tick is unchanged. A repo with no `.factory-ci.toml` is skipped (documented).
- **State**: handled. First run with no data: the first tick plans everything; an unchanged self-watch gets "unchanged since". Migration: Bitwarden still holds the old `FACTORY_CI_LOG_*` keys; they are ignored and deleted by the owner only after checks 1 to 5 and 8 pass and some days of green ticks. Crash mid-tick: unchanged 002 behaviour (stale `pending` after 2 h). Partial failure of this feature: the lock lands with `--locked` in one commit (T004), so no build breaks in between. Retry: all scripts are idempotent; the bootstrap job generates the lock only when none exists.
- **Time and concurrency**: handled. Two ticks at once: the `concurrency` group stays. Two selftest runs on one ref: the older is cancelled. Timeouts: every external call keeps its timeout; the new `docker create` has none and the job's `timeout-minutes` bounds it. Token expiry: README says rotate before day 80 and sets a day-75 reminder. Clock: N/A, no new time logic.
- **Resources**: handled. Memory: nothing runs on the VM; CI jobs have 7 GB. A Rust compile in a container adds time (D8, 30-minute timeout). Artifacts: 3-day retention, a lock file and a patch. Logs: unchanged bounds. Docker Hub rate limit: the mirror reruns.
- **Permissions and trust**: handled. Environment secrets only on `main`; PR workflows and forks get none; the bootstrap workflow reads no secret and does not run for forks; the scheduler token can only start ticks; no `pull_request_target` anywhere. A private repo's name appears in the public watch list (accepted, 002 FR-058). The log credentials are in the environment of a job that clones untrusted code: the runner never passes them to step containers (test), and the docker Environment has its own token.
- **UI**: N/A for screens. The README is the only interface; its states are covered by the order in Setup and quickstart Part 3.

## Rollout

1. Open the PR. The selftest is the first compile (T001 to T006). Fix until green, within 5 runs.
2. Merge when every selftest job is green and the owner has read the README Setup.
3. Owner does Setup (README): repository rulesets and Actions settings, the four Environments, Bitwarden machine accounts and projects, the Grafana access policies (two tokens), the fine-grained tokens, Environment secrets.
4. Owner pushes tag `rust-ci-runner-v0.1.0` on `main` (matches `Cargo.toml`). The release workflow pushes the image and prints the digest.
5. Owner switches the `rust-ci-runner` package to public, checks it with `gh api`, sets `RUNNER_IMAGE_DIGEST`, runs `factory-ci-mirror-images` from `main`, makes those packages public, sets the two image variables and the Bitwarden and log link variables.
6. Owner runs one manual tick, then sets up the scheduler and the heartbeat.
7. Live checks (T032 to T035). The monorepo checks wait for factory-monorepo PR 2.
8. After checks 1 to 5 and 8 pass and some days of green ticks, the owner deletes the old Bitwarden `FACTORY_CI_LOG_*` keys.

## Rollback

- Before merge: close the PR.
- After merge, before Setup: revert the merge commit; nothing runs, because there is no scheduler.
- After Setup: pause the scheduler job first (stops ticks), then revert. Revert the lock and `--locked` together; one without the other breaks the build. Write down the previous `RUNNER_IMAGE_DIGEST` before changing it, so the variable can go back.
- What `git revert` does not restore: the Environment secrets and the four Environments, the public GHCR package and its tags, the Bitwarden contents (the old log keys stay until deleted, which is why deletion comes last), the scheduler job and its token, the Grafana access policies and tokens, the rulesets and Actions settings.
- Logs: with the log secrets removed from an Environment, shipping stops; a private repo is refused (fail closed). Pipelines without secrets keep running.
- Statuses already posted stay (002 rule).

## Open risks

- R-1. The compile loop count is unknown. Each push costs one public run. Mitigation: read the whole log per run, fix every error in it, budget of 5.
- R-2. A dependency may need a newer rustc than 1.90 or disagree with the resolver fallback. Mitigation: pin in `Cargo.toml`, rerun.
- R-3. `own-pipelines` inside containers may expose test assumptions (root-owned files, missing tools, `git` ownership). The first run decides; T021 has a fallback.
- R-4. A public repo's workflow log is public: the first compile failures show the code. It is public code. Accepted.
- R-5. cron-job.org is a single outside dependency; the healthchecks.io grace and `/fail` ping cover a quiet or red tick.
- R-6. Self-watch posts statuses on every branch of this repo; the tick cap of 20 items can delay the monorepo during a burst. Accepted; revisit if it shows.
- R-7. The lint stays line-based; flow-style `on:` forms have mutation rows, other YAML forms are a known gap.
- R-8. factory-monorepo's root `.factory-ci.toml` is on its PR 2, not on `main`: the monorepo live checks (T032) wait for it.

**Deferred (no task in 003)**:
- Replace the third-party `alpine/git` clone image with one built in this repo (security M1). Mitigation now: the token has Contents read only on the watched repos; T031 records how the mirrored digest was checked.
- A container escape reads the runner environment (security M6). Mitigation now: narrow token scopes, `allow_docker` off on both entries, T034 checks the DROP rules cover IPv6.
- Split the clone token from the status token. The runner uses one token for both; a split is a runner change and out of scope. Mitigation now: one fine-grained token with only Contents read, Commit statuses read and write, Metadata read.
- `cargo audit` or `cargo deny`: deferred; the lock plus `--locked` closes drift first, and an advisory gate would block the first compile loop.
- Build provenance attestation for the image: optional, deferred.
- A YAML-parsing lint: deferred; the flow-style mutation rows cover the known gap.
- A test that fetched file content never reaches the summary `problems` text (security L1): add only if cheap during T008, otherwise deferred.

## DevOps assessment

The lead's devops review (round 1) is folded in; its required changes became tasks.

- **Infra**: nothing to provision. GitHub-hosted runners, GHCR, Bitwarden Secrets Manager free plan (2 of 3 projects, 2 of 3 machine accounts), Grafana Cloud free Loki (14-day retention), cron-job.org, healthchecks.io.
- **Required changes folded into steps**: runner label pin and its lint (T015, T016); `--locked` builds (T004); release dry run and toolchain install (T006); selftest triggers, concurrency and hygiene (T001); public GHCR package, its message and visibility check (T014, T019, T024, T031); scheduler headers, body, expected 204, failure notification and day-75 reminder (T024); heartbeat grace and `/fail` ping (T024); `factory-ci-release` Environment and the mirror `main` guard (T019); rollback list and Setup ordering (T024); required checks are the selftest jobs (T030).
- **Cost**: public repo, free minutes. `own-pipelines` and `build-release` add a few minutes per PR; concurrency cancels superseded runs. README says to keep the repo public or lengthen the tick.
- **Observability**: unchanged: heartbeat ping, one Loki line per tick with work, `verdict` job; Grafana usage alert on ingest (README).
- **Cache**: none added (YAGNI); cargo registry downloads each run.
- **Watch**: `ubuntu-24.04` deprecation notices; Docker Hub rate limits (rerun the mirror).

## Security assessment

The lead's security review (round 1) is folded in.

- **Moved risk**: log credentials leave Bitwarden for Environment secrets, limited to `main` with admin bypass off. Anyone who can merge to `main` can read them: already true for every bootstrap secret. Mitigation: the `main` ruleset (review, selftest, no bypass), CODEOWNERS on `.github/`, `rust-ci-runner/` and `.factory-ci.toml`, the tag ruleset on `rust-ci-runner-v*`.
- **New exposure**: the `plan` job and both run jobs hold the credentials in step environments. Mitigations: grep count 9 in three steps, `::add-mask::` before use, `set +x` in scripts, the runner keeps them out of step containers (test), two Loki tokens so the docker Environment can be cut off alone, Grafana policy `logs:write` only and a usage alert.
- **Fork PRs**: the selftest runs with `contents: read`, no secret, no Environment, `persist-credentials: false`, and the lint bans `workflow_run`, `secrets.`, `environment:` and `id-token` in it. `pull_request_target` is banned everywhere, in block, list and map forms. The bootstrap workflow runs for same-repo PRs only.
- **Artifact**: holds `Cargo.lock` and a patch of `.rs` files; two explicit paths, no hidden files, 3-day retention; the engineer downloads from the run of the same PR, applies with `--include`, and reads the diff first.
- **Supply chain**: `--locked` plus a committed lock closes drift; the resolver fallback prefers compatible versions; `actions/upload-artifact` reuses its SHA pin; the `bws` archive hash is checked against `BWS_SHA256` in CI (T022); the Bitwarden server URL must be `https://`.
- **GHCR public**: the image holds only the binary of public code; the first push is private and the owner confirms the switch and that the digest came from `main`; publish and mirror run in `factory-ci-release`.
- **Tokens**: Contents read on the watched repos only; one token for clone and statuses (split deferred); Bitwarden machine accounts read-only on one project each with expiry; blob keys from `openssl rand -hex 32`; everything rotates before day 80.
- **Private repo name**: visible in the public watch list (002 FR-058, accepted). Committed docs never hold a private run URL, a Grafana hostname or a value (docs test (g)).
- **Constitution**: Principles II and III hold. T026 clarifies the wording only.
- **Deferred**: see Open risks (M1, M6, token split, audit, attestation, YAML lint, L1).

## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| Selftest keeps cargo, shell and lint jobs while the runner runs the same checks | No runner exists to build the first one (bootstrap exception); the workflow stays as the pre-merge gate and the required check until the runner is proven (OQ-5) | Removing them leaves the PR with no check before the first tick |
| New `test-factory-ci-docs.sh` | The constitution says the README lists every name the workflows read; only a test keeps it true | A review checklist drifts |
| Temporary `factory-ci-bootstrap.yml` | The lock and the format patch cannot be made locally (1 GB VM); a separate file keeps artifacts out of the selftest and is deleted before merge | A bootstrap job in the selftest conflicts with the lint rule banning artifacts there |
| `build-release` job | A release problem should show before the tag | Finding it after the tag costs a tag, a release and a broken image |
