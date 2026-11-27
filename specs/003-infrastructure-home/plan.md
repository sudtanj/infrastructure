# Implementation Plan: Infrastructure Home

**Branch**: `003-infrastructure-home` | **Date**: 2026-11-27 (review rounds 1 to 4 folded) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/003-infrastructure-home/spec.md`

## Summary

The runner and every workflow now live in `sudtanj/infrastructure` (public). This feature finishes the move and the first run: wording, paths, the watch list and `ubuntu-24.04` pins; the first compile with `Cargo.lock`, `--locked` and `cargo fmt`; the log credentials from Environment secrets instead of Bitwarden; this repo's own pipeline files; a README Setup that lists every secret, variable, ruleset and hand step.

Approach: change scripts and workflows only, plus `#[ignore]` on the engine tests and the compile fixes. The Rust code already reads `FACTORY_CI_LOG_*` from the environment. CI is the only compiler (1 GB VM), so the first work item is a bootstrap workflow that produces the lock file and a format patch as an artifact, then a loop of push, read the public log, fix, with a budget of 5 runs. The selftest also dry-runs the release (both musl targets and an unpushed image build) so a release problem shows before the tag.

Round 4 (one Bitwarden token): GitHub keeps `BWS_ACCESS_TOKEN` and an optional server URL; one Bitwarden project holds everything else. A new load script fetches each job's values with one call, masks them and hands them on through `$GITHUB_ENV`. The docker split, the `factory-ci-plan` and `factory-ci-docker` Environments and the project variables go, and the runner drops its docker token. Constitution 1.2.0. Briefs B1 to B10 are at the end of this file.

## Technical Context

**Language/Version**: Bash (scripts, lint), GitHub Actions YAML, TOML, Rust 1.90.0 (toolchain pinned; `rust-version = "1.90"` declared)
**Primary Dependencies**: none added. Existing: `jq`, `openssl`, `curl`, `bws` 2.1.0 (pinned by SHA-256), `gh` (the engineer's tool for run logs and artifacts)
**Storage**: none
**Testing**: shell tests (`.github/scripts/test-*.sh`), the workflow lint and its self-test, `cargo test` in CI, the selftest workflow
**Target Platform**: GitHub-hosted `ubuntu-24.04`
**Project Type**: CI runner plus workflows plus docs
**Performance Goals**: a tick with no work stays under 2 minutes; it makes two Bitwarden calls (plan, verdict), and a `run` item whose pipeline lists a secret makes one more (round 4)
**Constraints**: no cargo, docker, make or test run on the dev VM; no private value in public output or committed files; every action pinned by SHA
**Scale/Scope**: 2 watched repos; about 20 files change, 4 added (`.factory-ci.toml` x2, one shell test, one temporary workflow)

## Constitution Check

Gate against `.specify/memory/constitution.md` v1.2.0 (round 4 amended it, T063).

| Principle | Result |
| --- | --- |
| I. One home for automation | Pass. FR-017 keeps factory-monorepo workflow-free; the watch list names both repos. |
| II. Secrets source | Pass, v1.2.0 (round 4). One GitHub secret, `BWS_ACCESS_TOKEN`, in `factory-ci` (main only); every other value and every pipeline secret in one Bitwarden project; FR-047, FR-048. The amendment is MINOR (D31): Principle II keeps its purpose, gains two rules, and narrows what GitHub may hold. |
| III. Nothing private in public logs | Pass. Round 4 adds the rule to mask every value fetched from Bitwarden before any other use; FR-050 and FR-059 test it, and the load script masks before it checks or writes. |
| IV. Simplicity | Pass with the Complexity Tracking entries. Round 4 deletes more than it adds: a job, an Environment, five secrets, two variables, the docker routing in the runner, and several lint rules. |
| V. Tested changes | Pass. Every FR maps to a test task (table below). Bootstrap exception stated (FR-020). |
| VI. Spec-driven | Pass. Every change here is in `tasks.md`; the PR links this folder. |
| Pinned actions, minimum permissions | Pass. `actions/upload-artifact` reuses the existing SHA pin (T002); the selftest keeps `contents: read` and no secret. |

Re-check after design: no new violation. The round 4 amendment (T063) is the only governance step; components that must change to comply are listed in D31.

## Architecture decision

Detail in `research.md` D1 to D25.

**Chosen**:
- Scripts read `FACTORY_CI_LOG_*` from the job environment (`fci_log_env`). The tick passes them as Environment secrets to the `plan` step and the two `job` steps only. The plan's Bitwarden token check stays. A job calls Bitwarden only when its pipeline lists a secret. `factory-ci-docker` holds its own Loki token.
- The first compile runs in a separate, temporary, same-repo-only bootstrap workflow that uploads two files; the selftest stays free of artifacts, secrets and Environments.
- Engine tests are `#[ignore]`d; the pipeline runs `cargo test --locked`; the selftest host job adds `--include-ignored`.
- Required checks on `main` are the selftest jobs; the runner's statuses stay informational.
- Round 2: the plan checks the store only when a row can use a secret (D19); the `main` ruleset needs a pull request with 0 approvals and no CODEOWNERS, because an author cannot approve their own pull request and only the owner can merge (D20); the lint gets the FR-035 rules and stays line-based (D21); blob keys are 64 lower-case hex everywhere (D22); phase A runs without the Loki credentials (D23); `capture()` loses its dead parameter (D24); the heartbeat is a requirement (D25).

**Rejected**: Bitwarden as source with an Actions fallback (two sources, partial states, breaks Principle II); dropping the plan's store check (a revoked token would turn one failed tick into many item errors); a bootstrap job inside the selftest (conflicts with the lint rule that bans artifacts there); a local `docker run cargo generate-lockfile` (Docker forbidden on the VM); a Docker-free test allowlist (rots); `workflow_dispatch` on the release workflow (a publish path from a non-tag ref).

**Round 4 (one Bitwarden token; D26 to D31)**. It supersedes the Chosen bullet about Environment-secret log credentials, the plan store check (D19) and the docker Loki token, and reverses the rejection of "dropping the plan's store check" above (the load step is the check, D28):
- Chosen: GitHub holds `BWS_ACCESS_TOKEN` in `factory-ci` and an optional server URL. A new `factory-ci-load.sh` runs one `bws secret list` per job, masks each requested value, checks it and writes it to `$GITHUB_ENV`. The job loads only its own names (plan: digest, blob key, token, log trio, log link; run: those plus the two images; verdict: heartbeat URL). `run` calls `bws` a second time only for a pipeline that lists secrets. One `run` job serves every item. `plan`, `run` and `verdict` share `factory-ci`. The plan has no store check of its own. The runner uses one token and one matrix. The constitution moves to 1.2.0.
- Rejected: a repository-level token (reaches any branch); keeping `factory-ci-plan` (no job needs it); keeping a project variable (the account sees one project); a load step that writes a JSON file (a cleartext copy of every secret); passing values as step outputs (GitHub drops masked outputs); a composite action (needs a `uses:` and a pin for nothing); keeping the `run-docker` job as a copy (identical to `run` once the credentials match); a fixed list of names in the workflow without a script allowlist (a typo would pass silently).
- Order matters: the guard and `fetch runner` need the digest, so `load secrets` runs before them; `install bws` needs only the public server URL, so it runs first.
- Review results (critic and security, then the owner's R2-38): `install bws` also puts `bws` on `$GITHUB_PATH` (B-1); the removed-name grep is anchored (M-1); one migration order without scheduler steps (M-2, R2-38); release and mirror wording (M-3); the reject pattern covers `400` and `invalid_client` (M-4); `verdict` soft steps are timed (M-5); a selftest step proves the hand-off (m-1); whitespace and trailing newlines are refused (m-2); every tick job has the main-only guard (F1); the clone and dind references must start with `ghcr.io/<owner>/` (F3, pending the file alternative D32); derived forms are masked and `$GITHUB_ENV` is written once (F5, F6); one archive over `--proto '=https' --tlsv1.2`, identical copies (F7). The M-6 heartbeat rework is withdrawn because ticks start by hand (R2-38, D33).

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
│   ├── selftest-workflow.md
│   └── tick-workflow.md
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
| `.github/workflows/factory-ci-tick.yml` | `FACTORY_CI_LOG_*` in three steps; fixed message when the image is not pullable; `https://` guard in `install bws` x3; header wording; `/fail` heartbeat; hex blob-key guards in three steps | T014, T018, T036, T040 |
| `.github/scripts/factory-ci-secrets.sh` | `fci_log_env` replaces `fci_log_creds` | T011 |
| `.github/scripts/factory-ci-plan.sh` | all-or-none at start; logs from env; `https://` guard; header wording; round 2: hex blob-key guard, phase A without the log credentials, store check only when a row can use a secret | T012, T018, T040, T041, T042 |
| `.github/scripts/factory-ci-blob.sh` | `fci_blob_encrypt` and `fci_blob_decrypt` require a 64 lower-case hex key | T040 |
| `.github/scripts/factory-ci-job.sh` | logs from env; guard; skip the list call without names | T013 |
| `.github/scripts/factory-ci-lint-workflow.sh` | rules `runs-on-pinned`, `no-cargo`, widened `no-pull-request-target`, selftest rules, each with self-test rows; round 2: tick `on:` only `workflow_dispatch`, `*.yaml` loop, pin and `permissions` on every file, tick `environment:` allowlist, end-anchored main guard, `secrets: inherit` and `secrets[` | T015, T028, T039 |
| `.github/scripts/test-factory-ci-secrets.sh` | cases for `fci_log_env`; drop cases for `fci_log_creds`; blob-key hex rows | T007, T040 |
| `.github/scripts/test-factory-ci-hardening.sh` | plan and job cases; grep counts; guard cases; README path moves to the root README; heartbeat rows; blob-key rows; phase A environment rows; store-check rows | T008, T018, T036, T040, T041, T042 |
| `.github/scripts/test-factory-ci-docs.sh` | new: wording, watch entries, `--locked`, README against workflows both ways, README rows, private-identifier scan; round 2: `/fail` row, drop CODEOWNERS row, 0-approvals and owner-only rows | T009, T037, T038 |
| `.github/factory-ci-repos.toml` | two entries | T017 |
| `.factory-ci.toml` | new, root pipeline | T021 |
| `rust-ci-runner/.factory-ci.toml` | new, runner pipeline; `test` runs `cargo test` in three processes (pids limit) | T020, T044 |
| `rust-ci-runner/Cargo.toml` | `rust-version = "1.90"` | T001 |
| `rust-ci-runner/Cargo.lock` | new, from the artifact | T004 |
| `rust-ci-runner/Makefile` | `--locked` | T004 |
| `rust-ci-runner/src/**`, `rust-ci-runner/tests/**` | compile and format fixes; `#[ignore]` on engine tests; one `LOG_VARS` case if missing; round 2: `engine.rs` `capture()` loses its `container` parameter | T005, T008, T010, T043 |
| `README.md` | Setup rewrite; round 2: heartbeat step 11 text, ruleset without CODEOWNERS and with the owner-only rationale | T024, T037, T038 |
| `rust-ci-runner/README.md` | "Set up B" becomes a pointer; log and secrets sections say Actions secrets; wording | T025 |
| `.specify/memory/constitution.md` | v1.0.2 | T026, T045 |
| `specs/002-ci-runner/{tasks,spec}.md` | pointers to 003 | T027 |
| `.github/workflows/factory-ci-tick.yml` (round 4) | delete `run-docker`; `plan`, `run`, `verdict` in `factory-ci`, each with the main-only `if:`; per job `install bws` (byte-identical, `$GITHUB_PATH`, one archive, `--proto =https`), `load secrets`, guard from the environment (image owner prefix in `run`); `verdict` checkout, `contents: read`, soft timed install and load, job limit 10 min; outputs `matrix`, `count`, `fail_tick`; the heartbeat step unchanged | T058 |
| `.github/scripts/factory-ci-load.sh` | new, the fetch, mask, check and hand-off script | T054 |
| `.github/workflows/factory-ci-selftest.yml` (round 4) | two hand-off steps in the `shell` job with a stub `bws` (FR-065) | T068 |
| `.github/workflows/rust-ci-runner-release.yml`, `factory-ci-mirror-images.yml` (round 4) | summary and header wording: add the Bitwarden secret KEY (FR-064) | T069 |
| `.github/scripts/factory-ci-secrets.sh` (round 4) | `fci_list TOKEN`; header comment | T054 |
| `.github/scripts/factory-ci-job.sh` (round 4) | no project, no https check, no `HAVE_BWS`; list only with names | T055 |
| `.github/scripts/factory-ci-plan.sh` (round 4) | no `bws`, no docker split, one key | T056 |
| `.github/scripts/factory-ci-blob.sh` (round 4) | header comment | T056 |
| `.github/scripts/factory-ci-lint-workflow.sh` (round 4) | delete the docker rules, add six | T059 |
| `.github/scripts/test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh` (round 4) | rows L, J, P, W; delete docker, project and store-check rows | T054 to T058 |
| `.github/scripts/test-factory-ci-docs.sh` (round 4) | two-table parse, check (h), fixture | T060 |
| `rust-ci-runner/src/main.rs`, `plan.rs`, `pipeline.rs` (tests), `plan_tests.rs`, `plan_hardening_tests.rs`, `tests/cli.rs` (round 4) | one token, one matrix, RS1 to RS4 | T057 |
| `README.md`, `rust-ci-runner/README.md` (round 4) | two tables, migration, troubleshooting, rollback | T061, T062 |
| `.specify/memory/constitution.md`, `CLAUDE.md`, `AGENTS.md` (round 4) | v1.2.0 and one sentence | T063, T064 (done) |

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
| FR-022, FR-023 | docs test (d), (e) both ways, including `environment:` names (T009); rows for 0 approvals, the owner-only rationale and no `CODEOWNERS` (T038) |
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
| FR-034 | hardening test: 13 heartbeat rows with a fake `curl` (aa6a307, T036); lint rule `verdict-ping`; docs test row for `/fail` in the README (T037) |
| FR-035 | one mutation row per new lint rule in `--self-test` (T039) |
| FR-036 | shell rows: short, non-hex, upper-case, 63, 65 and valid keys through plan, job, status and the blob functions (T040) |
| FR-037 | hardening row: a fake runner records its environment per call; phase A has none of the three names, phase C has all (T041) |
| FR-038 | hardening rows: no `bws` call for a row without names or id; one call with names; the store message when the token fails; docker rows follow the docker token (T042) |
| FR-041 | docs-test row: the README does not say "32 characters" (T047) |
| FR-042 | mutation rows `write-all` and `read-all` for `permissions` (T048) |
| FR-043 | mutation row that deletes the `_DOCKER` guard (T049) |
| FR-044 | mutation rows for one flow-style and one quoted `uses:` and `runs-on:` (T050) |
| FR-045 | mutation rows: the mirror workflow copied without its guard; the release workflow without its `verify` check (T051) |
| FR-046 | hardening rows: equal keys fail, different pass, docker key unset passes; README sentence (T052) |
| SC-003 to SC-006 | quickstart Part 3 (T032 to T034) |
| FR-047 | W1, W2, W3 (hardening test); lint `github-items` (M1); docs test D1 |
| FR-048 | L4, L5, L14; docs test D3; quickstart Part 3 |
| FR-049 | docs test check (h) D2; the grep of T029 |
| FR-050 | L1 to L17 |
| FR-051 | W4; lint `load-list` (M4) |
| FR-052 | W1; lint `bws-token-scope` (M2), `load-order` (M3) |
| FR-053 | J1, J2, J3; RS1 |
| FR-054 | P1, P2, P6; W7; RS2; lint `single-run` (M7) |
| FR-055 | RS2, RS3, RS4 (CI: `check`, `pipeline`, `own-pipelines`) |
| FR-056 | P3; L10, L11; T066 |
| FR-057 | W8; lint `verdict-soft` (M5); T066 |
| FR-058 | lint `tick-environment` (M6); W1 |
| FR-059 | L2, L12, J5, P4 |
| FR-060 | docs test D1, D3, D5; quickstart Part 2 |
| FR-061 | docs test D6; T063, T064 |
| FR-062 | T071 |
| FR-063 | W9; lint `install-identical` (M9); T068 |
| FR-064 | D7 |
| FR-065 | T068; lint M13 |
| FR-066 | W8, lint `verdict-main` (M10); T070 live |
| FR-067 | H1 |
| FR-068 | W11; lint `image-owner` (M11) |
| FR-069 | lint `install-identical` (M9), `bws-install` |

### Round 4 test rows

Row labels are the IDs the tasks use. The engineer names each row with its ID in the test output label.

**Load script** (`test-factory-ci-secrets.sh`, new section; stub `bws` records argv and serves a JSON array; real `jq`):

- L1 one `bws secret list` call, no project argument, all requested names found; `$GITHUB_ENV` holds exactly the requested names.
- L2 a value with a newline: its mask lines are printed, the check refuses it, and `$GITHUB_ENV` is empty (masks precede checks and writes).
- L3 token unset and token empty: fixed line naming `BWS_ACCESS_TOKEN` and `factory-ci`, zero `bws` calls.
- L4 a required name missing: exit 1, the key named, no value printed, `$GITHUB_ENV` empty.
- L5 an optional name missing: skipped, exit 0.
- L6 two secrets with one key: "ambiguous", `$GITHUB_ENV` empty.
- L7 a line under 4 characters: refused by key.
- L8 whitespace and control values, each refused by key with no value printed: leading space, trailing space, a trailing newline (checked on the JSON value), a lone CR, NUL; and a value that starts with `=` is written intact.
- L9 a name outside the allowlist: refused.
- L10 `bws` fails with each of `400 invalid_client`, a bare 400, 401 and 403 on stderr: no retry (one call each), "secret store rejected the token".
- L11 `bws` fails with a timeout: retried within `FACTORY_CI_FETCH_WINDOW=2`, "secret store unreachable".
- L12 sentinels in every name, one with `%`: stdout and stderr hold only `::add-mask::` lines for them, `%` escaped as `%25`.
- L13 `RUNNER_IMAGE_DIGEST` is written as `FCI_RUNNER_IMAGE_DIGEST`; no other renames.
- L14 an empty value counts as missing (required fails, optional skipped).
- L15 the `bws` argv is `secret list -o json` and nothing else.
- L16 `bws` prints malformed JSON that contains a sentinel: the sentinel appears nowhere in stdout or stderr, and the attempt is retried.
- L17 the derived forms are masked: the URL-encoded value, and the base64 of `LOG_USER:LOG_TOKEN`. Test values have the real shape of each key (64-hex key, `sha256:` digest, `https://` URL, PAT-like token), not placeholders.

**Job script** (`test-factory-ci-secrets.sh`, job section): J1 no names: zero `bws` calls with the token set. J2 names and token: exactly one call, no project argument. J3 names and no token: "no secret store configured", zero calls. J4 the `https://` check is gone: an `http://` `FACTORY_CI_BWS_SERVER_URL` in the environment changes nothing in `job.sh`. J5 sentinels in names and log values: masks only.

**Plan script** (`test-factory-ci-hardening.sh`): P1 `$GITHUB_OUTPUT` keys are exactly `count fail_tick matrix`. P2 every row, `allow_docker` or not, decrypts with the one key. P3 zero `bws` calls with the token set, rows with secrets and an id, and the store set to fail. P4 phase A has none of the three log names, phase C all three (kept from FR-037), plus sentinels masked only. P5 the blob-key shape rows (kept, docker rows gone). P6 no `_docker` output or count.

**Workflow greps** (`test-factory-ci-hardening.sh`): W1 (every grep skips comment lines) `secrets.` appears 4 times and only as `BWS_ACCESS_TOKEN`, once in `plan:load secrets`, once in `run:load secrets`, once in `run:job`, once in `verdict:load secrets`. W2 `secrets.` names other than `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN`: none. W3 `vars.` names: only `FACTORY_CI_BWS_SERVER_URL`, in the three `install bws` steps. W4 each job's `load secrets` arguments equal its row in `contracts/bootstrap-secrets.md`. W5 three `install bws` steps, each with the https guard before any download. W6 three `fetch runner` steps read `FCI_RUNNER_IMAGE_DIGEST` and keep the fixed message; the `run` guard checks the two images. W7 no `run-docker`, `matrix_docker`, `count_docker` in the workflow. W8 `verdict`: `install bws` and `load secrets` with `continue-on-error: true` and `timeout-minutes: 2` before the verdict step, the load step with `FACTORY_CI_FETCH_WINDOW: 20`, the job with `timeout-minutes: 10`, `if: github.ref == 'refs/heads/main' && always()`, `permissions: contents: read`, one `curl`. W9 each of the three `install bws` steps appends `$RUNNER_TEMP/bin` to `$GITHUB_PATH`; a copy without it fails (critic B-1). W11 the `run` guard refuses a clone or dind reference outside `ghcr.io/$owner/` (FR-068). H1 an entry of `.github/factory-ci-repos.toml` with `allow_docker = true` and no `# RISK-ACCEPTED:` comment above its `[[watch]]` line fails (FR-067).

**Lint mutation rows** (`--self-test`): M1 `github-items` (an extra `secrets.X`, an extra `vars.X`). M2 `bws-token-scope` (the token mapped in `guard`, in `verdict` outside `load secrets`). M3 `load-order` (a `uses:` after `load secrets`; `load secrets` moved before `install bws`). M4 `load-list` (a name added to `plan`'s list; one dropped from `run`'s). M5 `verdict-soft` (drop `continue-on-error` from either step). M6 `tick-environment` (a job in `factory-ci-release`). M7 `single-run` (a `run-docker` job added; `max-parallel: 8`). M8 the kept rules still fire after the deletions (`main-guard` on `run`, `runs-on`, `guard-first`). M9 `install-identical` (one copy edited; one without the `$GITHUB_PATH` line; one that fetches a checksum file). M10 `verdict-main` (`verdict` without the main guard). M11 `image-owner` (the guard without the owner prefix). M13 `selftest-shell` (the hand-off steps removed).

**Docs test** (`test-factory-ci-docs.sh`): D2b a line with `factory-ci-plan.sh` does not trip check (h), while a line naming the Environment `factory-ci-plan` does (the pattern is anchored). D7 neither `rust-ci-runner-release.yml` nor `factory-ci-mirror-images.yml` contains "repository variable" (FR-064). D8 keyword rows: `ACTIONS_STEP_DEBUG`, `gh secret list --org`, "Selected branches" with `main`, the migration order (a merge step before a manual tick step, no pause or resume), the "token leaked" runbook, the docker root statement, `invalid_client`. D1 the GitHub table names equal the workflow `secrets.`/`vars.` names both ways and the Environment names match. D2 check (h): a README or script line naming a removed item fails (mutation row). D3 the Bitwarden table names equal the union of the load arguments both ways (mutation rows for one extra and one missing). D4 the fixture and its old mutations updated. D5 keyword rows: the PAT permissions, `Only select repositories`, `90`, the scheduler token with `Actions` and `infrastructure`, `gh secret list --env factory-ci`, the migration note, the blob-key rotation note. D6 the constitution reads `Version**: 1.2.0` and names `BWS_ACCESS_TOKEN`; `CLAUDE.md` and `AGENTS.md` name it.

**Runner** (CI only): RS1 `reserved_secret_names_are_refused_whatever_the_case` lists every bootstrap key. RS2 `plan_hardening_tests` and `plan_tests` build one client and expect all rows in `matrix`. RS3 `tests/cli.rs`: an `allow_docker` entry with only `FACTORY_CI_TOKEN` plans and its row has `allow_docker: true`. RS4 `grep -rn "_DOCKER\|matrix_docker\|docker_api" rust-ci-runner` prints nothing.

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
| T013 job | Partial log trio reaches a job (round 4: the Bitwarden keys are partial) | `fci_log_env` in the job | `refuse`: item `error`, one retry | T008 |
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
| Live: scheduler (only if the owner runs one) | Token expired | dispatch returns 401 | No tick; heartbeat goes quiet; owner alert | quickstart Part 3 |
| Live: non-main dispatch | Environment refuses | job fails | `plan` does not run; `verdict` red; no secret read | quickstart 10 |
| T036 heartbeat | Ping URL unreachable | `curl` exit non-zero | Ignored, job stays as the verdict decides; the check goes late | T036 curl-error row |
| T036 heartbeat | A red tick pings the plain URL | `verdict-ping` lint; fake `curl` rows | Lint and hardening test red | T036 |
| T038 ruleset | Ruleset asks for one approval | the owner cannot merge their own PR | README states 0 approvals; docs test row | T038 |
| T039 lint | A rule matches too much and fails the repo | lint run on the repo in the selftest | Selftest red; adjust the regex | T039 |
| T039 lint | A rule misses a form (YAML escapes) | none by the line-based lint | Accepted; owner-only merge (R-9) | none |
| T040 blob key | An existing key is 32 characters or not hex | the guard step | Tick red with the variable name; owner regenerates with `openssl rand -hex 32` (README) | T040 |
| T041 plan | Phase A needs a log value | `plan --rows-out` errors | Tick red at phase A; remove the `-u` flags | T041 |
| T042 plan | The predicate skips a row that can reach a secret | the job ends `error` for a missing secret | Widen the predicate; the job still checks the store itself | T042 |
| T042 plan | Bitwarden down, a row can use a secret | `fci_list` fails | `fail` with the store message, tick red (unchanged) | T042 |
| T043 engine | A caller still passes the old argument | compile error | CI `check` red; fix the call | selftest |
| T054 load | `BWS_ACCESS_TOKEN` unset or empty (Environment missing, secret not set) | script check | exit 1, "BWS_ACCESS_TOKEN is not set in the factory-ci Environment", no `bws` call; job red | L3 |
| T054 load | Required key missing, misspelled or empty | `fci_key_value` returns 1 | exit 1 naming the key; nothing written to `$GITHUB_ENV` | L4, L14 |
| T054 load | Optional key missing | same | skipped; the job continues | L5 |
| T054 load | Two secrets with one key | `fci_key_value` returns 2 | exit 1 "ambiguous"; nothing written | L6 |
| T054 load | Value has a line under 4 characters or a newline | script check after masking | exit 1 naming the key; masks already printed; nothing written | L7, L8, L2 |
| T054 load | Token revoked or expired (401 or 403) | stderr text | no retry; "secret store rejected the token"; tick red at `plan` | L10 |
| T054 load | Bitwarden down or throttled | `fci_list` window ends | "secret store unreachable"; `plan` red; `run` red without a status; `verdict` stays decided by the plan result | L11, P23 |
| T054 load | The workflow passes a name the allowlist lacks | script check | exit 1 "unknown secret name" | L9 |
| T054 load | `$GITHUB_ENV` refuses a write (reserved name) | first live tick | job red at `load secrets`; rename in the script | T066, R-17 |
| T055 job | Pipeline lists a secret and the token is unset | `[ -n "$BWS_ACCESS_TOKEN" ]` | "no secret store configured" (unchanged) | J3 |
| T055 job | Pipeline lists no secret | names empty | zero `bws` calls | J1 |
| T056 plan | A candidate row exists and Bitwarden is down | none: the plan never calls it | the plan already failed at its load step if Bitwarden was down | P3 |
| T056 plan | The old docker key or token is still set in GitHub | none read | ignored; the README migration note says delete | W1 |
| T057 runner | A caller still uses `matrix_docker` or `docker_api` | compile error | CI `check` red; fix the call | RS4 |
| T057 runner | An `allow_docker` entry now plans with the one token | none | row lands in `matrix`; the job gets `--allow-docker` | RS3 |
| T058 tick | A step that holds `BWS_ACCESS_TOKEN` is added | W1 count and `bws-token-scope` | hardening and lint red | W1, M2 |
| T058 tick | A `uses:` step is added after the load step | `load-order` | lint red | M3 |
| T058 tick | `verdict` load fails | `continue-on-error` | job result follows the plan; no ping | W8, P23 |
| T058 tick | A required image variable in Bitwarden is not pinned | `guard, pinned images` in `run` | job red with the existing fixed message | W6 |
| T059 lint | A rule deleted with the docker split leaves a hole | mutation rows for the new rules | selftest `workflow` job red | M1 to M8 |
| T060 docs | README table drifts from the load lists | check (d) both ways | `shell` job red | D1, D3 |
| T060 docs | A removed name reappears outside `specs/` | check (h) | `shell` job red | D2 |
| T061 README | The migration note is missing | keyword row | `shell` job red | D5 |
| T067 owner | Old Environments or secrets are left behind | `gh secret list`, `gh variable list` | owner deletes; they are read by nothing | quickstart 6 |
| T058 tick | `bws` is not on `PATH` when `load secrets` runs (critic B-1) | W9; the selftest hand-off step (T068); `install bws` appends `$GITHUB_PATH` | job red at `load secrets` with a "bws not found" style error, not silent | W9, T068 |
| T058 tick | The three `install bws` copies drift | lint `install-identical` | selftest `workflow` job red | M9 |
| T058 tick | A clone or dind reference points outside `ghcr.io/<owner>/` | the `run` guard step | job red before any container starts, fixed message | W11, M11 |
| T058 tick | A dispatch from another branch | the main-only `if:` on all three jobs | every job skipped, no Environment entered, no secret read | W8, M10, quickstart 10 |
| T054 load | Value has leading or trailing whitespace, a trailing newline, a lone CR or NUL | checks on the JSON value | exit 1 naming the key; nothing written | L8 |
| T054 load | `bws` prints malformed JSON that contains a value | `jq -e` fails quietly | retried, then "secret store unreachable"; the text is never printed | L16 |
| T054 load | A revoked token answers `400 invalid_client` | reject pattern | no retry, "secret store rejected the token" | L10, T066 |
| T058 verdict | Bitwarden hangs | `timeout-minutes: 2` on the soft steps, fetch window 20 s, job limit 10 min | verdict decided by the plan and run results; no ping | W8 |
| T058 docker | An `allow_docker = true` entry without the risk marker | hardening row | `shell` job red | H1 |
| T069 wording | A workflow summary tells the owner to set a repository variable | docs test D7 | `shell` job red | D7 |
| T070 live | `factory-ci` lacks its branch policy, admins can bypass, or a repo secret exists | `gh api` checks | owner fixes before the first tick | T070 |

## Pre-mortem, rounds 1 to 3

Assume 003 shipped and broke. Five likeliest causes.

1. **The first run never turns green and the PR sits.** The crate never compiled. Prevention: T005 is a gate with a budget of 5 runs and a fast `check` job; fixes go to the source; the PR does not merge before green.
2. **A log credential leaks to a public log.** More jobs now hold them in the environment. Prevention: the grep count (9, three steps), masking before export, the sentinel test (T008), `LOG_VARS` keeps them out of step containers, quickstart check 5 signed out, and the separate docker token limits one escape.
3. **The first tick fails because the image is private, an Environment is empty or a ruleset is missing, or the owner cannot merge.** Prevention: the ruleset needs 0 approvals (an author cannot approve their own PR, D20); README order puts "make the package public" before the variable; the fixed message names the fix; the plan guard names the missing blob key; quickstart Part 3 starts with a manual dispatch.
4. **The first release breaks after the tag.** Prevention: the selftest dry-runs both targets and the image build (T006) and the release workflow installs the toolchain explicitly.
5. **Docs drift from the workflows again.** Prevention: the docs test compares secret, variable and Environment names both ways and scans for private identifiers, so the next workflow change fails the PR.

## Pre-mortem, round 4

Assume round 4 shipped and broke. The five likeliest causes.

1. **The first tick fails at `load secrets` because `bws` is not on `PATH`, or a key is missing, misspelled or has a trailing space.** Prevention: `install bws` writes `$GITHUB_PATH` (W9) and a selftest step proves the hand-off (T068); a missing required key names itself (L4); the README Bitwarden table lists every key with its form; the docs test compares the table with the load lists both ways (D3); T067 has the owner confirm the names.
2. **A secret value leaks into a public log through the `$GITHUB_ENV` hand-off.** Prevention: mask before check and before write (L2); one-line values only (L8); only requested names written (L1); sentinel tests on load, plan and job (L12, J5, P4); no `uses:` after load (lint); signed-out check in quickstart 5.
3. **A tick job reads a value it should not have, or a pipeline reads a bootstrap key.** Prevention: per-job load lists are compared in a test (W4) and a lint rule (`load-list`); the token is mapped only in two step kinds (W1, `bws-token-scope`); runner reserved prefixes with a row per bootstrap name (RS1).
4. **Bitwarden throttles or is down and ticks go red or silent.** Prevention: retry window and a fixed message; `verdict` steps are `continue-on-error`; T066 measures calls; the healthchecks.io late alert covers silence (accepted, R-15).
5. **A half-removed docker path breaks the tick or the runner build.** Prevention: one grep over the repo must print nothing (docs check h, FR-049); the runner change is checked by `check`, `pipeline` and `own-pipelines` in CI (T057); `verdict` needs only `plan` and `run` (W7, `single-run`).

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
- P11. For any `FACTORY_CI_BLOB_KEY` value (one key since round 4), the guards accept exactly the 64-character lower-case hex strings (length 63 or 65, upper case and non-hex characters fail).
- P12 (superseded by P19, round 4). For any candidate rows, the plan calls `bws` zero times when no row has `secrets_ok` and (`secrets` or `repo_id`), whatever the Bitwarden token and server state.
- P13. For any call of the plan binary with `--rows-out`, the three `FACTORY_CI_LOG_*` names are absent from its environment; with `--rows-in` they are present when set.
- P14. For any `verdict` input (plan result, `fail_tick`, run results, ref, URL set or not), the ping is `<url>/fail` when the plan is not `success`, `fail_tick` is `1`, or a run result is `failure` or `cancelled`; the plain URL otherwise; none off `main` or without a URL; a curl error never changes the job result.
- P15. For any workflow key set under the tick `on:` block, only `workflow_dispatch` passes; any other 2-space key or a flow-style `on:` fails the lint.

- P16. For any set of Bitwarden values (including `%`, a 3-character line, a newline, a very long line), `load.sh` prints no value except as `::add-mask::` lines, and `$GITHUB_ENV` holds only the requested names, written only when every check passed.
- P17. For any requested name list, `load.sh` makes at most one `bws secret list` call and none when the token is unset.
- P18. For any name outside the load allowlist, `load.sh` fails and writes nothing.
- P19. For any pipeline, `job.sh` makes zero `bws` calls when it lists no secret and one when it lists any; `plan.sh` makes zero whatever the rows.
- P20. In the tick workflow, the set of `secrets.X` names is `{BWS_ACCESS_TOKEN}`, the set of `vars.X` names is `{FACTORY_CI_BWS_SERVER_URL}`, and every tick job loads exactly its table row.
- P21. For any tick workflow copy that contains `run-docker`, `matrix_docker`, a second Environment or a `uses:` after `load secrets`, the lint fails.
- P22. For any bootstrap key, the runner rejects it as a pipeline secret name.
- P23. For any `verdict` input with `install bws` or `load secrets` failed, the verdict result depends only on the plan result and `fail_tick`, and no ping goes out.
- P24. For any three `install bws` `run:` bodies, they are byte-identical, each appends `$RUNNER_TEMP/bin` to `$GITHUB_PATH`, and none fetches a checksum file.
- P25. For any clone or dind reference, the `run` guard accepts it only when it starts with `ghcr.io/<owner>/` and ends with `@sha256:<64 hex>`.
- P26. For any Bitwarden value with leading or trailing whitespace, a CR, LF or NUL, `load.sh` refuses it and writes nothing; a value that starts with `=` is written intact.
- P27. For any dispatch ref other than `refs/heads/main`, all three tick jobs are skipped.

## Edge cases (feiyi-edge-cases)

- **Inputs**: handled. A blob key of 63, 65, upper-case or non-hex characters is refused (T040). Empty, partial and short-line log secrets (T007, T008); a secret with `%`, newline or CR is masked with the existing escaping (002); a server URL with `http://`, a space or a newline is refused (T008). Two watch entries for one repo: `plan --check` rejects duplicates (002 test). Maximum: the cap of 20 items per tick is unchanged. A repo with no `.factory-ci.toml` is skipped (documented).
- **State**: handled. First run with no data: the first tick plans everything; an unchanged self-watch gets "unchanged since". Migration: Bitwarden still holds the old `FACTORY_CI_LOG_*` keys; they are ignored and deleted by the owner only after checks 1 to 5 and 8 pass and some days of green ticks. Crash mid-tick: unchanged 002 behaviour (stale `pending` after 2 h). Partial failure of this feature: the lock lands with `--locked` in one commit (T004), so no build breaks in between. Retry: all scripts are idempotent; the bootstrap job generates the lock only when none exists.
- **Time and concurrency**: handled. Two ticks at once: the `concurrency` group stays. Two selftest runs on one ref: the older is cancelled. Timeouts: every external call keeps its timeout; the new `docker create` has none and the job's `timeout-minutes` bounds it. Token expiry: README says rotate before day 80 and sets a day-75 reminder. Clock: N/A, no new time logic.
- **Resources**: handled. Memory: nothing runs on the VM; CI jobs have 7 GB. A Rust compile in a container adds time (D8, 30-minute timeout). Artifacts: 3-day retention, a lock file and a patch. Logs: unchanged bounds. Docker Hub rate limit: the mirror reruns.
- **Permissions and trust**: handled. Environment secrets only on `main`; PR workflows and forks get none; the bootstrap workflow reads no secret and does not run for forks; the scheduler token can only start ticks; no `pull_request_target` anywhere. A private repo's name appears in the public watch list (accepted, 002 FR-058). The log credentials are in the environment of a job that clones untrusted code: the runner never passes them to step containers (test), and the docker Environment has its own token.
- **UI**: N/A for screens. The README is the only interface; its states are covered by the order in Setup and quickstart Part 3.

## Edge cases, round 4 (feiyi-edge-cases)

- **Inputs**: handled. Empty: token unset gives one fixed line (L3); an empty Bitwarden value counts as missing (L14). One item: one key looked up. Maximum: the cap of 20 items per tick is unchanged, and the `run` matrix stays at `max-parallel: 4`; a project with thousands of secrets still returns one JSON array (bounded by `timeout 30`). One over the limit: a duplicate key fails as ambiguous (L6). Duplicates and a name asked twice: the second write to `$GITHUB_ENV` repeats the same value, harmless. Unicode, embedded newline or CR, `%`: a newline or CR is refused (L8); `%` is escaped by `fci_mask` (L12); NUL cannot sit in a shell variable and `bws` JSON escapes it, which `jq` turns into a value `bash` drops, so the value fails its short-line or hex guard; a very long line is masked in full. Malformed data: `bws` output that is not a JSON array is a failed attempt and retried (existing, `fci_list`); a missing field `.value` reads as empty.
- **State**: handled. First run: the Bitwarden keys must exist before the first tick; a missing required key names itself (L4). Migration from round 3: the owner creates the keys, then deletes the retired Environments and secrets (T067, README migration note); old and new settings never mix because the workflow no longer reads the old names. Crash mid-operation: `load.sh` writes `$GITHUB_ENV` last, so a refused or killed run leaves it empty; the job fails before any repo script runs. Partial failure: `plan` loads fine but `run` cannot: statuses stay `pending` and go stale (R-15). Retry: `load.sh` is idempotent; two runs write the same file lines, and a rerun of the job gets a fresh file.
- **Time and concurrency**: handled. Two ticks at once: the `concurrency` group stays. Parallel `run` items each make their own list call, so with 4 in parallel the limit risk grows (R-14). Timeouts: `timeout 30` per call, 60 s window, 120 s per download, job timeouts. Token expiry: the machine-account token expires; README sets a reminder and the failure is the fixed "rejected the token" line at `plan`. PAT expiry 90 days, rotated in Bitwarden only. Clock: N/A, no new time logic. Ordering: the blob key can change between `plan` and `run` (R-19).
- **Resources**: handled. Memory: nothing runs on the VM; the load script holds one JSON array in a shell variable (the largest realistic project is well under 1 MB). Disk: `fci_list` writes a small stderr file under `$RUNNER_TEMP` and deletes it. The `GITHUB_ENV` file is small. Slow, down or garbage Bitwarden: retry window, then "secret store unreachable"; the outcomes per job are in the failure table. Growth: no new logs or caches.
- **Permissions and trust**: handled. The token is limited to `factory-ci` on `main`; a branch dispatch is refused by the Environment before any step. A pipeline cannot ask for a bootstrap key (reserved prefixes, RS1). A repo's own steps cannot reach another repo's keys (owner-scoped keys, unchanged). Values from Bitwarden are trusted data written by the owner; the load script still refuses newlines and short lines. A forked pull request gets no secret.
- **UI**: N/A for screens. The README is the interface. Its states: Setup order, the migration note, and the troubleshooting rows for "BWS_ACCESS_TOKEN is not set", "KEY is missing in Bitwarden", "secret store rejected the token" and "log credentials incomplete".

## Rollout

1. Open the PR. The selftest is the first compile (T001 to T006). Fix until green, within 5 runs.
2. Merge when every selftest job is green and the owner has read the README Setup.
3. Owner does Setup (README). Round 4 replaces the Environment and secret steps: one Environment `factory-ci`, one Bitwarden project, one token (see the Round 4 rollout below).
4. Owner pushes tag `rust-ci-runner-v0.1.0` on `main` (matches `Cargo.toml`). The release workflow pushes the image and prints the digest.
5. Owner switches the `rust-ci-runner` package to public, checks it with `gh api`, enters the release digest into Bitwarden as `RUNNER_IMAGE_DIGEST`, runs `factory-ci-mirror-images` from `main`, makes those packages public, and enters the two image references and the optional log link into Bitwarden (round 4; they were repository variables).
6. Owner runs one manual tick. The scheduler and the heartbeat are optional (R2-38).
7. Live checks (T032 to T035). The monorepo checks wait for factory-monorepo PR 2.
8. Round 4: the log keys stay in Bitwarden as the source; after checks 1 to 5 and 8 pass and some days of green ticks the owner confirms the retired Environments and secrets are gone (T067).

**Round 4 rollout** (the implementation lands in the same pull request, before merge; ticks start only by hand, R2-38):
1. Engineer: T054 to T056 in one commit, with T057 (parallel); then T058 and T059 in one push; then T060, T061, T062, T068, T069, with T062 any time. Selftest green, within 4 runs.
2. Owner migration, one order (R2-31, T067). The owner has already created `factory-ci` (main only, admin bypass off) with `BWS_ACCESS_TOKEN`, and deleted `factory-ci-plan`.
   1. Create the Bitwarden project, the machine account, and the keys that already exist (the GitHub token, the blob key, the log trio).
   2. Merge once the selftest is green and the owner has read the README Setup.
   3. Tag the release, take the digest, run the mirror workflow.
   4. Set the remaining keys: `RUNNER_IMAGE_DIGEST`, `FACTORY_CI_CLONE_IMAGE`, `FACTORY_CI_DIND_IMAGE` and the optional ones.
   5. Run the T070 checks, then one manual tick.
   6. Delete the retired items (old repository variables, any leftover Environment secrets and `factory-ci-docker`) after the manual runs stay green, and run the T070 checks again.
3. Live checks T066, with the heartbeat key unset.

## Rollback

- Before merge: close the PR.
- After merge, before Setup: revert the merge commit; nothing runs, because ticks start only by hand.
- After Setup: if a scheduler runs, pause it first; otherwise nothing starts a tick. Then revert. Revert the lock and `--locked` together; one without the other breaks the build. Write down the previous `RUNNER_IMAGE_DIGEST` before changing it, so the variable can go back.
- What `git revert` does not restore: the Environment secrets and the four Environments, the public GHCR package and its tags, the Bitwarden contents (the old log keys stay until deleted, which is why deletion comes last), the scheduler job and its token, the Grafana access policies and tokens, the rulesets and Actions settings.
- Logs: with the log secrets removed from an Environment, shipping stops; a private repo is refused (fail closed). Pipelines without secrets keep running.
- Statuses already posted stay (002 rule).
- **Round 4 rollback**: if a scheduler runs, pause it; then revert the round 4 commits together (workflow, scripts, runner, lint, tests, README; reverting one part breaks the tick). Before reverting, the owner must restore what the old design read from GitHub, because a revert restores code and not settings: the `factory-ci-plan` and `factory-ci-docker` Environments and their secrets, the repository variables, and the second Bitwarden account and project. If the owner has already deleted them, recreate them from the pre-round-4 README at the commit before this round. Without a revert, the new design rolls back one way only: remove `BWS_ACCESS_TOKEN` and every tick stops at `plan` with the fixed line. Bitwarden contents are not touched by either.

## Open risks

- R-1. The compile loop count is unknown. Each push costs one public run. Mitigation: read the whole log per run, fix every error in it, budget of 5.
- R-2. A dependency may need a newer rustc than 1.90 or disagree with the resolver fallback. Mitigation: pin in `Cargo.toml`, rerun.
- R-3. `own-pipelines` inside containers may expose test assumptions (root-owned files, missing tools, `git` ownership). The first run decides; T021 has a fallback.
- R-4. A public repo's workflow log is public: the first compile failures show the code. It is public code. Accepted.
- R-5. (Only if the owner runs a schedule, R2-38.) cron-job.org is a single outside dependency; the healthchecks.io grace and `/fail` ping cover a quiet or red tick.
- R-6. Self-watch posts statuses on every branch of this repo; the tick cap of 20 items can delay the monorepo during a burst. Accepted; revisit if it shows.
- R-7. The lint stays line-based; flow-style `on:` forms and the FR-035 rules have mutation rows, other YAML forms are a known gap.
- R-8. factory-monorepo's root `.factory-ci.toml` is on its PR 2, not on `main`: the monorepo live checks (T032) wait for it.
- R-9. YAML escape sequences in trigger keys (for example a quoted key with an escape) pass the line-based lint. Covered only by the owner-only merge rule (D20): nobody else can merge a workflow change.
- R-10. The `main` ruleset has 0 approvals, so the selftest is the only gate and the owner can merge their own red-free change without a second reader. Accepted for a one-person repo (D20).
- R-11. The plan predicate of D19 (`secrets` or `repo_id`) is derived from the watch entry, not from the pipeline files; if a third route to a secret appears, the plan skips a store check it should run. The job still checks the store itself.
- R-12. Round 3: flow-style and quoted `uses:` and `runs-on:` keys are read by the line regex (FR-044), but a full YAML parse stays deferred; anchors, multi-line flow maps and escape sequences remain a gap, covered by the owner-only merge rule (D20).
- R-13. The plan's store-check predicate (D19) reads only the row's `secrets` and `repo_id`. A pipeline that names owner-scoped secrets on a row with empty `repo_id` skips the early check. The job still checks the store itself and refuses, so the cost is one `error` status instead of an early tick failure. No code change.

- R-14. Round 4, small since R2-38. Ticks start by hand, so Bitwarden sees two calls per tick (plan and verdict) plus one per item whose pipeline lists a secret, a handful a day, and one identity exchange per `bws` run. Bitwarden's limits are not known to me. `fci_list` retries a 429 with jittered backoff inside its 60 s window (20 s in `verdict`); T066 records any retry. If the owner adds a schedule later, this risk grows to about 576 calls a day and needs a new look.
- R-15. Round 4, accepted by the owner and moot when `FACTORY_CI_HEARTBEAT_URL` is unset (the setting for manual runs, R2-38). With a URL set: a Bitwarden outage means `verdict` cannot read it, so no `/fail` ping goes out and only the healthchecks.io late alert fires. Separately, a `run` job that cannot load ends red with no status; the `pending` statuses go stale and the next tick replans them after the stale window.
- R-16. Round 4, accepted by the owner, reworded after security F2. A docker-enabled pipeline step can get root on the runner, so it can read `BWS_ACCESS_TOKEN` itself: every Bitwarden secret and, through `FACTORY_CI_TOKEN`, every watched repo. The owner rotates the machine-account token after any untrusted docker run (README). Neither watch entry sets `allow_docker`, and an entry that does needs a `# RISK-ACCEPTED:` comment (FR-067, H1).
- R-17 (replaced). The `$GITHUB_ENV` hand-off was untestable here. A selftest step now proves it with a stub `bws` and probes a `RUNNER_` name (FR-065, T068); the `FCI_RUNNER_IMAGE_DIGEST` rename stays either way.
- R-18. Round 4. Masking every fetched value also masks the image references and the digest in later logs. Harmless; it can hide a useful string when someone debugs a pull failure. The fixed "not pullable" message does not need them.
- R-19. Round 4. Rotating `FACTORY_CI_BLOB_KEY` while a tick is running makes that tick's `run` jobs fail to decrypt. The README says to rotate between ticks.
- R-20. Round 4. The three `install bws` steps stay inline copies; the lint compares their `run:` bodies byte for byte (`install-identical`, FR-069), so a drift fails the selftest.
- R-21. Round 4. A pipeline-secret key in Bitwarden that equals a shared name in a watch entry is shared across every repo whose entry lists it. Unchanged from 002; with one project the shared names and the bootstrap keys now sit side by side, and only the reserved prefixes keep the bootstrap keys out of reach.
- R-22. Round 4 (critic m-5, security F8). Every step after `load secrets`, and the `actions/checkout` post step, inherits `$GITHUB_ENV`. The checkout is pinned to a commit and uses `persist-credentials: false`. Accepted. No `uses:` step may follow the load step (`load-order`).
- R-23. Round 4 (security F3), pending the owner. The runner digest and the clone and dind references sit in Bitwarden, with no review and no history. The guard requires `ghcr.io/<owner>/` and `@sha256:`; the committed-file alternative is research D32.

**Deferred (no task in 003)**:
- Replace the third-party `alpine/git` clone image with one built in this repo (security M1). Mitigation now: the clone runs with a mirrored image pinned by digest; T031 records how that digest was checked. The token it receives is described under the token split below.
- A container escape reads the runner environment (security M6). Mitigation now: narrow token scopes, `allow_docker` off on both entries, T034 checks the DROP rules cover IPv6.
- Token split (security M3). `FACTORY_CI_TOKEN` has Contents read plus Commit statuses write (and Metadata read) on the watched repos, and it enters the `alpine/git` clone container. The earlier text said Contents read only; that was wrong. Accepted until the clone image and token split work is done: a clone token with Contents read only for the clone container, and a separate statuses token for the runner. That is a runner change and out of scope here.
- `cargo audit` or `cargo deny`: deferred; the lock plus `--locked` closes drift first, and an advisory gate would block the first compile loop.
- Build provenance attestation for the image: optional, deferred.
- A YAML-parsing lint: deferred; the flow-style mutation rows cover the known gap.
- Secret-name charset `^[A-Z][A-Z0-9_]*$` and the reserved shell names (security L3). Only the owner writes the store, so a bad name is the owner's own mistake.
- The release `verify` job depends on the tag ruleset (security L4): without the ruleset anyone with write access could tag. Owner-only write access covers it now.
- The concurrency slot of the tick is shared with a non-main dispatch (security L6): a dispatch from another branch can occupy the slot until its `plan` job fails the Environment rule.
- `stop_and_remove` is not strictly bounded at 30 s (reviewer m3): the `shortcut:` comment in `engine.rs` records the limit.
- YAML escape sequences in triggers: see R-9.
- A test that fetched file content never reaches the summary `problems` text (security L7): add only if cheap during T008, otherwise deferred.

## DevOps assessment

The lead's devops review (round 1) is folded in; its required changes became tasks.

- **Infra**: nothing to provision. GitHub-hosted runners, GHCR, Bitwarden Secrets Manager free plan (2 of 3 projects, 2 of 3 machine accounts), Grafana Cloud free Loki (14-day retention), cron-job.org, healthchecks.io.
- **Required changes folded into steps**: runner label pin and its lint (T015, T016); `--locked` builds (T004); release dry run and toolchain install (T006); selftest triggers, concurrency and hygiene (T001); public GHCR package, its message and visibility check (T014, T019, T024, T031); scheduler headers, body, expected 204, failure notification and day-75 reminder (T024); heartbeat grace and `/fail` ping (T024); `factory-ci-release` Environment and the mirror `main` guard (T019); rollback list and Setup ordering (T024); required checks are the selftest jobs (T030).
- **Cost**: public repo, free minutes. `own-pipelines` and `build-release` add a few minutes per PR; concurrency cancels superseded runs. README says to keep the repo public or lengthen the tick.
- **Observability**: heartbeat ping with `/fail` on a red tick or failed plan (FR-034), one Loki line per tick with work, `verdict` job; Grafana usage alert on ingest (README).
- **Cache**: none added (YAGNI); cargo registry downloads each run.
- **Watch**: `ubuntu-24.04` deprecation notices; Docker Hub rate limits (rerun the mirror).
- **Round 4 (my own assessment; none was handed over this round)**: nothing to provision beyond Round 1: Bitwarden now needs 1 project and 1 machine account (free plan allows 3 and 3). Cost: public repo, free minutes; `verdict` adds a checkout, a `bws` download and a list call (about 30 s), and every manual tick adds two Bitwarden calls. Bitwarden's rate limit for these calls is unknown to me, so T066 records any retry (R-14). Observability: ticks start by hand (R2-38), so no scheduler or late alert is needed; the heartbeat stays optional and unset, and with it set a Bitwarden outage silences the `/fail` ping (R-15). Rollback and rotation are simpler: one token on GitHub, the rest edited in Bitwarden with no workflow change.

## Security assessment

The lead's security review (round 1) is folded in.

- **Moved risk (rounds 2 and 3; reversed by round 4, see the round 4 bullets below)**: log credentials leave Bitwarden for Environment secrets, limited to `main` with admin bypass off. Anyone who can merge to `main` can read them: already true for every bootstrap secret. Mitigation: the `main` ruleset (pull request, selftest as the required checks, no force push, no deletion, empty bypass list), owner-only write access (only the owner can merge, D20; no CODEOWNERS because an author cannot approve their own pull request), and the tag ruleset on `rust-ci-runner-v*`.
- **New exposure (rounds 2 and 3, superseded by round 4)**: the `plan` job and both run jobs hold the credentials in step environments. Mitigations: grep count 9 in three steps, `::add-mask::` before use, `set +x` in scripts, the runner keeps them out of step containers (test), two Loki tokens so the docker Environment can be cut off alone, Grafana policy `logs:write` only and a usage alert.
- **Fork PRs**: the selftest runs with `contents: read`, no secret, no Environment, `persist-credentials: false`, and the lint bans `workflow_run`, `secrets.`, `environment:` and `id-token` in it. `pull_request_target` is banned everywhere, in block, list and map forms. The bootstrap workflow runs for same-repo PRs only.
- **Artifact**: holds `Cargo.lock` and a patch of `.rs` files; two explicit paths, no hidden files, 3-day retention; the engineer downloads from the run of the same PR, applies with `--include`, and reads the diff first.
- **Supply chain**: `--locked` plus a committed lock closes drift; the resolver fallback prefers compatible versions; `actions/upload-artifact` reuses its SHA pin; the `bws` archive hash is checked against `BWS_SHA256` in CI (T022); the Bitwarden server URL must be `https://`.
- **GHCR public**: the image holds only the binary of public code; the first push is private and the owner confirms the switch and that the digest came from `main`; publish and mirror run in `factory-ci-release`.
- **Tokens**: Contents read on the watched repos only; one token for clone and statuses (split deferred); Bitwarden machine accounts read-only on one project each with expiry; blob keys from `openssl rand -hex 32`; everything rotates before day 80.
- **Private repo name**: visible in the public watch list (002 FR-058, accepted). Committed docs never hold a private run URL, a Grafana hostname or a value (docs test (g)).
- **Constitution**: Principles II and III hold. T026 clarifies the wording only.
- **Round 2**: blob keys must be 64 lower-case hex (FR-036); phase A runs without the log credentials (FR-037); the lint bans `secrets: inherit` and `secrets[` in the selftest and pins the tick Environments per job (FR-035); the heartbeat pings `/fail` on a red tick (FR-034).
- **Deferred**: see Open risks (M1, M3, M6, L3, L4, L6, audit, attestation, YAML lint and escapes, L7, m3).
- **Round 4 (my own assessment; the lead handed over no separate devops or security text for this round)**:
  - **New concentration**: one Bitwarden token reads every secret in the project: the GitHub token, the blob key, Loki credentials, pipeline secrets. The owner accepted that a docker-enabled step runs in a job that can read every secret. Neither watch entry sets `allow_docker` today. The token sits only in `factory-ci` (`main`, bypass off) and only in the steps FR-052 names: the `load secrets` steps and the `run` job's `job` step. `job.sh` unsets it before it `exec`s the runner, and the runner reserves `FACTORY_CI*`, `RUNNER_`, `BWS_` and `FCI_` as pipeline secret names, so no pipeline can ask for a bootstrap key.
  - **`$GITHUB_ENV` hand-off**: later steps inherit the loaded values. Mitigation: no `uses:` after the load step (lint `load-order`); one-line values only, so a value cannot add a second variable to the file; only requested names are written; every value is masked before it is checked or written.
  - **Masking breadth**: image references and the digest are masked too, so run logs show `***` where they appear. Accepted: the owner's rule is to mask every fetched value.
  - **Machine account scope**: Can read on one project only, token expiry set. A second project granted by mistake makes duplicate keys fail as ambiguous.
  - **Heartbeat**: the URL is read from Bitwarden only in `verdict`. A ping URL is a secret; the `verdict` job holds no other value.
  - **What got safer**: nine fewer secrets and two fewer Environments to configure, rotate and leak from; a revoked GitHub token or blob key is changed in one place.
  - **Review results**: F1: `factory-ci` is checked live (`custom_branch_policies` true, one policy named `main`, `protected_branches` false, `can_admins_bypass` false), repo and org secret lists are empty, and every tick job, `verdict` included, has a main-only `if:` (FR-066, T070). F2: R-16 now says a docker step can read `BWS_ACCESS_TOKEN` itself; rotate after an untrusted docker run; `allow_docker = true` needs a `# RISK-ACCEPTED:` comment (H1). F3: clone and dind references must start with `ghcr.io/<owner>/` (FR-068); the committed-file alternative is pending the owner (D32, R-23). F4: a "token leaked" runbook and an expiry of 90 days or less go in the README (B8). F5: `set +x`, `jq` and `bws` stderr never printed, derived forms masked (URL-encoded, base64 of `LOG_USER:LOG_TOKEN`, `x-access-token:`), a malformed-JSON sentinel row (L16, L17). F6: one `printf` after every check; rows for a lone CR, a trailing newline, NUL and a leading `=` (L8). F7: one archive, `curl --proto '=https' --tlsv1.2`, identical copies (FR-069). F8: the checkout post step inherits `$GITHUB_ENV` (R-22).

## Engineer briefs (round 4)

Each brief is self-contained. Common constraints C1 to C7 in `tasks.md` apply to all: no cargo, docker, make or build on the VM; read, write, grep, `bash -n`, `git`, `gh` only; no secret value in any file or output (sentinels only); explicit `git add` paths, commit message ends with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`; implement through `/speckit-implement` for the tasks named. Shell tests may run on the VM with a stand-in `openssl` on `PATH` (accepts `-pass env:`, echoes fixed text) for the test run only. Write the test rows first, see them fail, then change the code. Parallel: T054 and T057 and T062 can start together; the rest runs in ID order.

### B1. Load script and store library (T054)

- **Goal**: one script that fetches the named Bitwarden values with one call, masks, checks and hands them to later steps.
- **Files**: `.github/scripts/test-factory-ci-secrets.sh`, `.github/scripts/factory-ci-secrets.sh`, `.github/scripts/factory-ci-load.sh` (new, executable). Review additions: L8 (whitespace, trailing newline, lone CR, NUL refused; a leading `=` kept), L10 (400 invalid_client, 400, 401, 403 each rejected with one call), L16 (malformed JSON with a sentinel prints nothing), L17 (URL-encoded and base64 `LOG_USER:LOG_TOKEN` masked); test values have the real shape of each key.
- **Acceptance**: rows L1 to L15 (plan.md "Round 4 test rows") pass; `fci_list TOKEN` has no project parameter; the script follows `contracts/bootstrap-secrets.md` step by step; the allowlist and required or optional flags equal `data-model.md`; `RUNNER_IMAGE_DIGEST` is written as `FCI_RUNNER_IMAGE_DIGEST` and nothing else is renamed; `bash -n` clean; no `set -x`; stderr of `bws` is never printed. `fci_list` retries with jitter (`sleep $((delay + RANDOM % 3))`, one line) and matches `400|401|403|invalid_client`; `jq` stderr goes to `/dev/null`; every `NAME=value` line is built first and appended with one `printf`; T054 to T056 land in one commit.
- **Constraints**: reuse `fci_list`, `fci_key_value`, `fci_mask`, `fci_short_line`; mask before any check or write; write `$GITHUB_ENV` last and only after every check passed; the stub `bws` must record argv so L1 and L15 can assert it; chmod +x like its siblings (the lint script lost its bit once, commit ad377fa).

### B2. Job script (T055)

- **Goal**: `factory-ci-job.sh` talks to Bitwarden only for pipeline secrets, with the one token and no project.
- **Files**: `.github/scripts/factory-ci-job.sh`, `.github/scripts/test-factory-ci-secrets.sh`, `.github/scripts/test-factory-ci-hardening.sh`.
- **Acceptance**: J1 to J5 pass; `grep -n "BWS_PROJECT\|HAVE_BWS\|BWS_SERVER_URL" .github/scripts/factory-ci-job.sh` prints nothing; every older row that set `FACTORY_CI_BWS_PROJECT` now runs without it and still passes; the final `unset` list loses only the removed names.
- **Constraints**: keep the refuse reasons and masks as they are; do not touch the `prepare` call; do not rename `names`.

### B3. Plan script and blob header (T056)

- **Goal**: `factory-ci-plan.sh` makes no `bws` call and seals every row with the one key.
- **Files**: `.github/scripts/factory-ci-plan.sh`, `.github/scripts/factory-ci-blob.sh`, `.github/scripts/test-factory-ci-hardening.sh`.
- **Acceptance**: P1 to P6 pass; the old docker, project, store-check and README-docker rows are gone (list in T056); outputs are `matrix`, `count`, `fail_tick`; the grep for `bws`, `fci_list`, `_DOCKER`, `can_use` and `docker` over `factory-ci-plan.sh` prints nothing.
- **Constraints**: keep the blob-key shape check, the `fci_log_env` check, the phase A log-name strip and the summary and failure handling.

### B4. Runner: one token, one matrix (T057)

- **Goal**: the runner has no docker token and no docker matrix.
- **Files**: `rust-ci-runner/src/main.rs`, `plan.rs`, `pipeline.rs`, `plan_tests.rs`, `plan_hardening_tests.rs`, `tests/cli.rs`.
- **Acceptance**: RS1 to RS4; `grep -rn "_DOCKER\|matrix_docker\|docker_api" rust-ci-runner/src rust-ci-runner/tests` prints nothing; `allow_docker` stays on the watch entry and the row; the plan output file still lists every row sorted by `n`; the selftest `check`, `pipeline` and `own-pipelines` jobs are green.
- **Constraints**: no cargo here, so read each changed file twice and `grep` every caller of a removed item before deleting it; match `cargo fmt` style by hand; do not loosen an assertion, delete only tests of removed behaviour and say which; if `Apis::pick` and `Status.docker` have no remaining reason, remove them, otherwise leave them and say why.

### B5. Tick workflow (T058)

- **Goal**: the new job layout of `contracts/bootstrap-secrets.md`.
- **Files**: `.github/workflows/factory-ci-tick.yml`, `.github/scripts/test-factory-ci-hardening.sh`. Review additions: W8 (verdict timeouts, fetch window 20, job limit 10, main guard with `&& always()`), W9 (`$GITHUB_PATH` in each `install bws`), W11 (image owner guard), H1 (`allow_docker` risk marker); `install bws` downloads the archive only with `curl --proto '=https' --tlsv1.2`, compares `BWS_SHA256`, and its three `run:` bodies are byte-identical; the `heartbeat` step stays as it is.
- **Acceptance**: W1 to W8 pass; `grep -c "secrets\." ` shows 4 and all `BWS_ACCESS_TOKEN`; `grep -n "run-docker\|_DOCKER\|factory-ci-plan\([^.A-Za-z0-9_-]\|$\)\|factory-ci-docker\|HAVE_BWS\|FACTORY_CI_BWS_PROJECT" .github/workflows/factory-ci-tick.yml` prints nothing; no `${{` inside a `run:` block; every `uses:` stays pinned; `verdict` has `permissions:\n      contents: read` and a checkout with `persist-credentials: false`. W1 and every grep skip comment lines. The `run` guard derives `$owner` from `github.repository_owner`, lower-cased, as `fetch runner` does.
- **Constraints**: step order checkout, `install bws`, `load secrets`, guard, `fetch runner`, wrapper; no `uses:` after `load secrets`; the token only in `load secrets` and the `run` job's `job` step; the https guard stays in each `install bws` step; the header comment names the new layout; lands in one push with B6.

### B6. Lint (T059)

- **Goal**: the lint enforces the new layout and loses the rules of the removed split.
- **Files**: `.github/scripts/factory-ci-lint-workflow.sh`. Review additions: M9 `install-identical`, M10 `verdict-main` (replaces `verdict-always`), M11 `image-owner`, M13 `selftest-shell` for the hand-off steps (T068); `verdict-soft` also checks `timeout-minutes: 2`, the fetch window and the job limit; `bws-install` checks the `BWS_SHA256` comparison and `--proto =https` instead of `sha256sum -c`; new rules skip comment lines.
- **Acceptance**: M1 to M8 each fail for their named rule; every deleted rule has no mutation row left; `bash .github/scripts/factory-ci-lint-workflow.sh --self-test .github/workflows/factory-ci-tick.yml` prints `self-test ok`; the lint passes on the repo, also with `FACTORY_CI_LINT_STRICT=1`; the `have-bws` guard text and the `bws-install` checks still read the install step.
- **Constraints**: stay line-based (the existing `shortcut:` note holds); every mutation must change the file or the self-test reports it; do not weaken `main-guard`, `uses-pin`, `permissions`, `runs-on-pinned`.

### B7. Docs test (T060)

- **Goal**: the docs test checks the two README tables against the workflows and bans the removed names.
- **Files**: `.github/scripts/test-factory-ci-docs.sh`. Review additions: D2b (`factory-ci-plan.sh` does not trip check (h); the Environment name does), D7 (release and mirror workflows say no "repository variable"), D8 (keyword rows listed in the test plan).
- **Acceptance**: D1 to D5 pass on the new README (after B8) and fail on mutated copies; the removed-name patterns are built from parts, so the file never holds the words; the test still passes `bash -n`.
- **Constraints**: write the rows first and watch them fail against the old README; keep checks (a) to (g) working; the GitHub table has the header `Name | Type | Scope | Notes` and the Bitwarden table starts its header with `Bitwarden secret`, which the engineer may rename if the parser stays unambiguous and `plan.md` and the README agree.

### B8. README Setup (T061)

- **Goal**: the owner can set everything up from two tables.
- **Files**: `README.md`. Review additions: the migration note uses the order of R2-31 without scheduler steps (the owner already created `factory-ci` with `BWS_ACCESS_TOKEN` and deleted `factory-ci-plan`); the scheduler and heartbeat steps are labelled optional and the README says to skip them and leave `FACTORY_CI_HEARTBEAT_URL` unset for manual runs; "Deployment branches: Selected branches, `main`"; `gh secret list`, `gh secret list --org <org>`, `gh secret list --env factory-ci`; a warning never to enable `ACTIONS_STEP_DEBUG` or `ACTIONS_RUNNER_DEBUG`; the docker root statement and token rotation after an untrusted docker run; a "token leaked" runbook (revoke the machine-account token, issue a new one with an expiry of 90 days or less, rotate every project value and pipeline secret, review or delete old run logs); a troubleshooting row for a revoked token (`invalid_client` or 400 until T066 records the real text).
- **Acceptance**: docs test 0 failed; the first table lists the `factory-ci` Environment (deployment branches `main`, admin bypass off), `BWS_ACCESS_TOKEN`, the optional `FACTORY_CI_BWS_SERVER_URL` and `factory-ci-release` with no secret; the second lists every Bitwarden key of `data-model.md` with what it is, how to create it and when to set it; `FACTORY_CI_TOKEN` is a fine-grained PAT, "Only select repositories" set to the watched repos, Contents Read-only, Commit statuses Read and write, Metadata Read-only, 90 days; the scheduler token is a fine-grained PAT on `sudtanj/infrastructure` only with Actions Read and write and lives only in cron-job.org; the troubleshooting table gains "BWS_ACCESS_TOKEN is not set", "KEY is missing in Bitwarden", "secret store rejected the token" and drops the rows for removed items; rollback notes match D31 and "Round 4 rollback"; step 13 reads as FR-022 says; a migration note exists; the blob key is rotated between ticks; `grep -n "_DOCKER\|BWS_PROJECT\|factory-ci-plan\([^.A-Za-z0-9_-]\|$\)\|factory-ci-docker" README.md` prints nothing.
- **Constraints**: load `stop-slop` first; no value, run URL or Grafana host; Setup order must work (the digest exists after the release, the image names after the mirror, so the Bitwarden table says "set when"); keep the intro and the fit notice.

### B9. Runner README (T062)

- **Goal**: the runner README matches the runner.
- **Files**: `rust-ci-runner/README.md`.
- **Acceptance**: as T062; docs checks (f) and (g) 0 failed.
- **Constraints**: `stop-slop`; keep it to runner behaviour and point to the root README for setup.

### B10. After the merge (T066, T067, T071)

- **Goal**: live results, owner migration, README and description in sync.
- **Files**: `specs/003-infrastructure-home/research.md` (results only), nothing else unless a check fails.
- **Acceptance**: each check has a recorded pass or fail with a one-line cause and no private identifier; T071 reports which of the two READMEs and the description needed a change, with the reason when none did.
- **Constraints**: start only after the owner confirms the migration; do not set a variable or create a secret yourself; a failed check opens a new task and reruns the Spec Kit steps.
### B11. Selftest hand-off, wording, live checks (T068, T069, T070)

- **Goal**: prove the `$GITHUB_ENV` hand-off before the first tick, remove the stale "repository variable" instructions, and check the Environment live.
- **Files**: T068 `.github/workflows/factory-ci-selftest.yml` and the lint self-test; T069 `.github/workflows/rust-ci-runner-release.yml`, `.github/workflows/factory-ci-mirror-images.yml`, the docs test; T070 `research.md` (results only).
- **Acceptance**: T068: two steps in the `shell` job, the first running `factory-ci-load.sh` against a stub `bws` (realistic fake values, a literal fake token, no `secrets.`) and appending `RUNNER_PROBE=1` to `$GITHUB_ENV`, the second failing unless `FCI_RUNNER_IMAGE_DIGEST` and the other names arrived and printing whether the probe did; M13 removes either step and fails `selftest-shell`; the `selftest-isolated` rule still passes. T069: D7 passes; both workflows now say "add the Bitwarden secret" for the three keys; no step logic changes. T070: each `gh` check of FR-066 has a recorded result with names and booleans only.
- **Constraints**: T068 and T069 are comment and summary text plus one workflow step; load `stop-slop` for the text; T070 runs before the first tick, after the owner's Setup, and again after the retired items are deleted; record no value.


## Complexity Tracking

| Violation | Why Needed | Simpler Alternative Rejected Because |
| --- | --- | --- |
| Selftest keeps cargo, shell and lint jobs while the runner runs the same checks | No runner exists to build the first one (bootstrap exception); the workflow stays as the pre-merge gate and the required check until the runner is proven (OQ-5) | Removing them leaves the PR with no check before the first tick |
| New `test-factory-ci-docs.sh` | The constitution says the README lists every name the workflows read; only a test keeps it true | A review checklist drifts |
| Temporary `factory-ci-bootstrap.yml` | The lock and the format patch cannot be made locally (1 GB VM); a separate file keeps artifacts out of the selftest and is deleted before merge | A bootstrap job in the selftest conflicts with the lint rule banning artifacts there |
| `build-release` job | A release problem should show before the tag | Finding it after the tag costs a tag, a release and a broken image |
| `factory-ci-load.sh`, a new script (round 4) | Three jobs need the same fetch, mask, check and hand-off, and a masked value must be masked before any check or write (FR-050). One script has one test file section and one lint list; inline copies would triple it | Inline shell in three workflow steps: lint cannot read it well, and a fix needs three edits |
| A second `bws secret list` in `run` when the pipeline lists secrets (round 4) | The names are known only after `prepare`, which needs the clone token from the first load | One list in `job.sh` only: the blob key and the image digest are needed before it runs |
