# Implementation Plan: Bitwarden watch list, a 5-minute schedule, and a clean public repo

**Branch**: `013-bitwarden-watch-list` | **Date**: 2026-11-28 (revision 2) | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/013-bitwarden-watch-list/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran in revision 1. `.specify/extensions.yml` does not exist, so no hooks ran. Revision 2 ran `/speckit-clarify` (the owner's answers are the new clarification sessions of spec.md), updated spec, plan and tasks, then ran `/speckit-analyze` (result at the end of this file). Nothing was built or run on the planning VM (low RAM): every file claim comes from reading the code at `origin/main` 8660df5 and, for 012, at the head of its worktree. This plan is public: it writes no private repo name, only the old file paths of the manual workflow and its test, which the engineer needs until T030 renames them. `PRIVATE_REPO`, `OLD_MANUAL` and `NEW_MANUAL` are defined in spec.md.

## Summary

The watch list moves from the committed file into the Bitwarden key `FACTORY_CI_REPOS`. The plan script reads it with the shell helpers it already has, writes it to a private temp file and hands that file to the unchanged `--repos` argument of the plan runner, which never sees the token. The manual `ci` command reads it in-process in the `bws` call it already makes. The committed file is deleted in its own pull request. Every public message about the list switches to `entry N`; four safety rules (owner, id, `secret_branches`, `B_PRIVATE`) and a heartbeat gate close holes the cron would open. The cron lands in its own pull request. Then the tree is scrubbed, the manual workflow is renamed, and, only after a fresh owner go, the history is rewritten.

### Evidence that shaped the design (read from the files)

1. `factory-ci-plan.sh` runs every runner call through `run()`: `env -u BWS_ACCESS_TOKEN -u BWS_ACCESS_TOKEN_DOCKER -u FACTORY_CI_BLOB_KEY -u FACTORY_CI_BLOB_KEY_DOCKER "$runner"`. The script's own comment says the runner never needs the store tokens. It already sources `factory-ci-secrets.sh`, whose `fci_list PROJECT TOKEN` runs one `bws secret list` with 30 s per attempt, retries and a 401/403 stop, and whose `fci_key_value KEY` does an exact-case lookup (0 found, 1 none or empty, 2 more than one).
2. The script passes `--repos .github/factory-ci-repos.toml` to phase A. Owner decision: scripts are editable now, so the argument becomes a temp file made by the script.
3. `.github/scripts/test-factory-monorepo-ci.sh` (OLD_MANUAL's test) pins exactly one `docker run`, exactly three `-v` mounts, the mount text for the repos file and no `bws` word in the workflow. Both change with the mount removal.
4. `factory-ci-lint-workflow.sh:92` (`has '^  schedule:' && bad no-schedule ...`) and its self-test row at line 34 fail when `schedule:` exists. `test-factory-ci-hardening.sh` (around line 303) lints the real tick workflow, so it passes once the rule accepts the cron.
5. The release workflow's smoke test runs `ci --repos <mounted file> --repo x/y --ref main` with no token and requires stderr exactly `[runner] repository or ref not in the watch list`. After the change `ci` without `--repos` and without a token prints the same line, and the smoke step drops the mount.
6. `tests/no_vendor.rs` allows the words `bws` and `bitwarden` only in `pipeline.rs`, `store.rs`, `ci.rs`, `engine.rs`, `main.rs`, `log.rs`.
7. Public text names the list today: `plan.rs` `summary()` (`- item N: <repo>`, unchecked repos), `problem()` (`<repo>: <why>`), `plan.rs:231` (`id {id} in the list, but the API says {}`), `main.rs:131` and `main.rs:180` (`<repo>: FACTORY_CI_TOKEN_DOCKER is not set`), and `watch.rs` (labels are repo names; messages quote globs, secret names, ids).
8. `main.rs:81` sets `b_private: var("B_PRIVATE") != "false"`. The tick sets `B_PRIVATE: ${{ github.event.repository.private }}`. A `schedule` event payload has no `repository`, so the value is empty and `b_private` is true. `plan.rs` sets `fail_closed: repo.private && !o.b_private`, so a true `b_private` turns the guard off for private watched repos. The safe direction is `b_private = (value == "true")`: an empty value counts as public and the guard stays on. `factory-ci-job.sh:111` turns `fail_closed` into `--private`, which silences the job's public output; nothing else reads `B_PRIVATE`.
9. `verdict`'s heartbeat step has `if: always()`, so a failed plan still pings.
10. 012's tree (worktree head 1197688) adds `silence_panics()` to `main.rs` and a test `only_the_job_and_status_arms_of_main_silence_the_panic_hook` (`job_tests.rs`) that forbids `silence_panics` in the `run` and `plan` arms. Revision 1 told the engineer to reuse the helper for `plan`; that was wrong. T012 changes that test on purpose (see D9).
11. `walk.rs` `decide()` reads the status of the tip, and only for a project with none walks the first-parent chain (at most 50 ancestors) for a `success`; with none it returns `Run { prev: None }`. A rewritten repo therefore gets one run per project at each watched tip, not one per commit (D11).

## Technical Context

**Language/Version**: Rust 1.90, GitHub Actions YAML, bash

**Primary Dependencies**: existing only; `bws` 2.1.0 pinned by the existing install step; `git filter-repo` for the rewrite (owner's machine, not a build dependency)

**Storage**: Bitwarden Secrets Manager key `FACTORY_CI_REPOS`; one 0600 temp file for the length of phase A

**Testing**: `cargo test` (unit and integration), the shell tests (`test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh`, the manual-workflow test), the lint and its self-test, the `factory-ci-selftest` workflow. Nothing runs on the planning VM.

**Target Platform**: GitHub-hosted `ubuntu-24.04`; runner image `linux/amd64,linux/arm64`

**Project Type**: CLI plus CI workflows and scripts

**Performance Goals**: one extra `bws secret list` per tick (two when the tick has work); 288 ticks a day

**Constraints**: public repo (no list text in any output, no private name in tree or, after the rewrite, in history); no secret in any file or message

**Scale/Scope**: the parser caps (64 KiB, 100 entries) are tighter than the unverified Bitwarden limit (25,000 characters per SDK validation)

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. The line "which repositories the runner watches is declared in `.github/factory-ci-repos.toml`" becomes false; T026 amends it. |
| II Secrets source | Pass with an amendment. The list is a Bitwarden value read at run time with the token the plan job already holds; no new GitHub secret. T003 asserts that tokens live in Environments only. |
| III Nothing private in public logs | Pass by design (FR-004, FR-005, the hostile-list test). The feature also closes existing leaks (evidence 7, 8). |
| IV Simplicity | Pass. No new subcommand, no new crate, no store path in the plan runner. The two readers share nothing but the key name, and each reuses code that exists. |
| V Tested changes | Pass. Each acceptance scenario maps to a test (test plan). |
| VI Spec-driven | Pass. Clarify, plan, tasks and analyze ran; implementation covers only `tasks.md`. |
| Third-party actions pinned, minimal permissions | Pass. No new action or permission. |
| Constitution rule on the README task | Exception recorded: the history rewrite (T033) is the last task because the owner ordered it so, it is irreversible and owner-gated, and it may never run; the README and description update (T032) is the last task before it. |

### Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| Two readers (shell for the tick, Rust for `ci`) | The runner never gets the token on the tick (evidence 1); `ci` runs in one container with the token and cannot call the shell | One reader needs the token inside the plan runner |
| `--repos FILE` kept in `ci` and `plan` | Existing tests, the fixture check and the shell script's temp file use it | Removing it forces a rewrite of 2,979 lines of `ci_tests` |
| `number` on `WatchEntry` and `entry` on plan rows | The summary must name an entry by position even after docker entries are skipped | Repo names in the summary leak the list |

## Architecture decision and alternatives rejected

**Decision (D1), design B**: the plan script fetches, the plan runner parses.

In `factory-ci-plan.sh`, before phase A: check `BWS_ACCESS_TOKEN` is set; `fci_list "$FACTORY_CI_BWS_PROJECT" "$BWS_ACCESS_TOKEN"`; a new helper `fci_watch_list FILE` in `factory-ci-secrets.sh` calls `fci_key_value FACTORY_CI_REPOS`, maps the return code to a fixed `FCI_ERR`, writes the value to FILE (created before by `mktemp`, mode 0600, created exclusively, so no symlink or pre-existing file is followed) and then clears `FCI_JSON` and `FCI_VALUE` (every other row, including the log credentials and secrets, is discarded; the later log-credential step calls `fci_list` again as it does today). Phase A runs `run plan --repos "$list" --rows-out "$cand"` and the script removes the file straight after, and on every `fail` path.

**Why B and not the runner reading Bitwarden (A)**: (1) the token never enters the plan runner, so the rule "the runner never needs the store tokens" stays true and 011's reading of it stays true; the plan runner parses third-party repo data and a process that holds the token should not; (2) no store code path, no `bws` seam and no new fault list in the tick runner; (3) the retry, timeout and 401/403 logic exists and has shell tests; (4) the Rust change for the tick is only the parser's messages. A would need `run()` changed to leak the token to one call, a `bws` binary lookup on the tick VM (the install step puts it in `$RUNNER_TEMP/bin`, while the runner's fixed path is `/usr/local/bin/bws`) and a second copy of the key lookup.

Rejected:

| Alternative | Why not |
|---|---|
| Phase A reads Bitwarden itself (A) | See above. |
| A `watch-list` subcommand writing the file (revision 1) | Needed only while the scripts were frozen; one more command and fault table for the same result. |
| Recover the stripped token from the parent process's environment | Defeats a stated isolation. |
| Keep a comment-only stub | Revision 1 did it only because OLD_MANUAL's test pinned the mount; the owner chose deletion and the test changes. |
| Strip the schedule block from a lint copy | Revision 1 workaround; the lint rule is editable now. |
| Put the list in a GitHub secret or variable | Pipeline data belongs in Bitwarden (Principle II); the owner chose it. |

### D2 Parser

`watch::parse(text)` keeps its signature and its validation. Its messages change for good: every label is `entry N`, a TOML syntax error is `line K: invalid TOML`, and no message interpolates a value, an input key name, a glob, a secret name, an id or a repo. One parser, one message form, so no second path can leak. Messages come from the fixed table in contracts/runner-cli.md. `WatchEntry` gains `number: usize`; only `watch.rs` builds literals (3 sites). New rules in `parse` (FR-006): `id` required when `allow_docker` is true or `secrets` is non-empty; `secret_branches` items `*` and `**` refused; zero entries refused. A separate function `watch::check_owner(&[WatchEntry], owner: &str) -> Vec<String>` returns `entry N: repo owner differs from the repository owner` lines; `plan_a` and `ci` (store source) call it with `GITHUB_REPOSITORY_OWNER` (unset refuses).

Tests of 010 and 012 that expect the old messages or accept `secret_branches = ["*"]`, `allow_docker` without `id` or zero entries change in T004; each change is named in that task.

### D3 Store (`store.rs`)

`attempt` takes `project: Option<&str>` and appends it after `secret list` when `Some` (1 to 64 characters of `[0-9A-Za-z-]`, no leading `-`, else the call is refused as `Unreachable`). `fetch_with` becomes `fetch_json` plus `parse_list_with` so `ci` parses one JSON twice with one call. `watch_text(json) -> Result<String, ListError>`: one row with the exact key `FACTORY_CI_REPOS`; none is `Missing`, more than one is `Many(n)`, a value that is absent, not a string or blank is `Empty`. It does not use `usable()` (a TOML value holds newlines). The list value type has no `Debug`.

### D4 `ci`

`ci` without `--repos`: empty token prints `repository or ref not in the watch list` (the release smoke needs it); one `fetch_json` with `FACTORY_CI_BWS_PROJECT` (env closure); `watch_text` then `parse` and `check_owner` (any failure prints the new fault `watch list unusable`); the entry lookup as today; then `parse_list_with(json, keep)` for the four settings and the held secrets. With `--repos FILE` the old file path runs unchanged (development and tests). `FAULTS` gains `watch list unusable`; `BWS_ACCESS_TOKEN is not set` leaves it (no path prints it; its tests change). No Loki detail for a list fault: the settings come from the same call and are not parsed yet. The owner finds the problem with `plan --repos - --check`.

### D5 Schedule, lint and heartbeat

Tick `on:` gets `schedule:` with `- cron: "*/5 * * * *"` (PR-4). The lint rule becomes `schedule`: when `^  schedule:` exists, exactly one such line and exactly one `^    - cron:` line, equal to the owner's expression; `pull_request_target` stays refused. Mutation rows: a row that inserts a `schedule:` with another cron (works with and without the real block), and from PR-4 a row that changes the real cron. The heartbeat step in `verdict` loses `if: always()` (a step runs on `success()` by default, and the earlier step already exits 1 on a bad plan or `fail_tick=1`); a lint rule requires the step has no `if:` and still has `set +x`, the single quiet `curl --max-time 10`, and the main-only check; one more lint rule checks the `plan` step has no `continue-on-error` and no `if:`. The selftest repeats the plan-step check and a cron check (written with the bracket trick, `[c]ron`, `[u]pload-artifact`, so a new pattern never matches the lint's own greps of that file). GitHub cron facts for the README: best effort, late or skipped runs, disabled after 60 days without repo activity, runs on the default branch, the `main` guard holds, the group keeps ticks serial with one pending slot, a replaced pending run is Canceled. The cron doubles the cadence while the outside dispatcher runs; statuses make a double tick idempotent. The `3-58/5` option (avoid the top of the hour) is recorded and not taken.

### D6 Redaction and entry numbers

`Row` and the status records that carry a repo gain `entry: usize`. `problem()` takes the entry number; `Candidates.unchecked` holds `entry N`; `summary()` prints `- item N`. `plan_a` seeds `Opts.secrets` with every repo name, its owner and the token variables; `plan_c` seeds from every repo in the candidate rows, `unchanged` and `invalid` (M3: phase C never sees the list, only the candidates). A value under 4 characters is never masked by the redactor, so no message quotes list text, masked or not.

### D7 Zero entries, owner, id, `secret_branches`

Parser rules of D2. `plan --check` and phase A call the same function, so they agree (the devops finding).

### D8 `B_PRIVATE`

`plan_opts()`: `b_private: var("B_PRIVATE") == "true"`. Test: empty, `false`, `garbage`, `true`.

### D9 Panics

012's arm test is changed to require `silence_panics()` in `job`, `status` and `plan` and none in `run`, and a new `plan` code path never `unwrap`s list data. The helper lives in 012's `main.rs`; this feature rebases on it.

### D10 Version numbers

Runner `0.5.0` (new fault, new rules; 012 ships 0.4.0). Constitution MINOR bump relative to `main` at merge time: `1.1.1` after 012 becomes `1.2.0`; if 011's `1.2.0` landed first, `1.3.0`. Reason: Principles I and II change where standing configuration lives, and a rule that was forbidden (a schedule) is now permitted.

### D11 History rewrite and the tick

`contracts/history-rewrite.md` holds the runbook. After the rewrite the public repo (itself a watched repo) has new SHAs. `decide()` finds no status on the new tip, so `open` is non-empty and `needs_row` is true; the chain of at most 50 ancestors carries no status for the project's context, so `walk_one` returns `Run { prev: None }`. Result: one run per project at each watched tip, once; the run posts statuses on the new tip and the next tick sees `Done`. Other watched repos are untouched. Two checks before the go: 012's publish step must be idempotent for a re-run on identical content (lead confirms in 012's tests or by reading `publish.rs`), and 011's claims are keyed by slot and not by SHA (lead confirms).

## Merge order and conflicts

1. **012 merges first**; 013 rebases on `main` after 012's release. 012's follow-up T020 edits `.github/factory-ci-repos.toml` (`allow_docker = true` and the two Docker Hub secret names on the infrastructure entry). If it lands before T001, the owner copies the final text into Bitwarden; if after, T020 becomes an edit of the Bitwarden value and is dropped from git; T022 re-checks equality either way.
2. **Expected conflicts with 012**: `tests/repo_files.rs` (012's `entry_problems` tests read the real file; T023 keeps the helper and its self-test on fixture text and removes the real-file test), `job_tests.rs` (D9), `main.rs` and `ci.rs` (012 edits the `keep` block and adds `silence_panics()`), `watch_tests.rs` (012 adds 8 lines), `constitution.md` (1.1.1), both READMEs, `Cargo.toml` and `Cargo.lock`.
3. **011 is reconciled in the same session** on its own branch (docs only). Its statements changed: the cron exists and doubles the cadence (its 15-minute floor stands; extra ticks are idempotent through claims); the list is in Bitwarden, so its `secret_branches = ["main"]` edit of the private entry (T030) becomes a Bitwarden edit by the owner; its new `format!` lines and rows must pass the hostile-list test and carry `entry`; scripts are editable for 013 only, and 011's frozen-file statements are kept as history with a dated amendment; `ci` makes no second `bws` call (it reuses the one call). 011 must also take the new parser messages and the `id`/`secret_branches` rules into its fixtures.

## Project Structure

### Documentation (this feature)

```text
specs/013-bitwarden-watch-list/
├── spec.md  plan.md  tasks.md  research.md  data-model.md  quickstart.md
├── checklists/requirements.md
└── contracts/runner-cli.md  contracts/script-changes.md  contracts/history-rewrite.md
```

### Source code: every file to add or change

| File | Change | Task |
|---|---|---|
| `rust-ci-runner/src/watch.rs` | number-only messages; `number`; rules; `check_owner` | T009 |
| `rust-ci-runner/src/store.rs` | `project` arg; `fetch_json`; `watch_text`; `ListError` | T010 |
| `rust-ci-runner/src/plan.rs` | `entry`; `problem`, `summary`, id line | T011 |
| `rust-ci-runner/src/walk.rs`, `projects.rs` | audit only; fix any `format!` of list data | T011 |
| `rust-ci-runner/src/main.rs` | `plan --repos -`; zero refusal; owner check; `B_PRIVATE`; redaction seeds; line 131 and 180; `plan` panic hook | T012 |
| `rust-ci-runner/src/ci.rs`, `log.rs` | store source; project; `watch list unusable`; drop the unset-token fault | T013 |
| `rust-ci-runner/src/job_tests.rs` | the arm test of D9 | T012 |
| `rust-ci-runner/src/{watch,store,plan,ci}_tests.rs`, `plan_hostile_tests.rs`, `fake.rs`, `tests/{cli,repo_files}.rs`, `tests/fixtures/watch-list.example.toml` | tests and fixture | T004 to T008, T023 |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | `0.5.0` | T014 |
| `.github/scripts/factory-ci-secrets.sh`, `factory-ci-plan.sh` | `fci_watch_list`; temp file; remove `--repos` file path | T018 |
| `.github/scripts/test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh` | plan tests read the list from the fake store; idle tick makes 1 call; 0600 file; failures | T017 |
| `.github/scripts/factory-ci-lint-workflow.sh` | `schedule`, heartbeat and plan-step rules; mutation rows | T019, T028 |
| `.github/scripts/test-factory-monorepo-ci.sh` | mount count 2, no `--repos`, new `-e` names; renamed in T030 | T024, T030 |
| `.github/workflows/factory-ci-tick.yml` | heartbeat gate (T019); cron (T028) | T019, T028 |
| `.github/workflows/factory-ci-selftest.yml` | plan-step check; fixture `--check`; `paths`; cron check | T020, T025, T028 |
| `.github/workflows/factory-monorepo-ci.yml` | pin 0.5.0; no mount, no `--repos`; env; no `repo` default (T024); renamed to `factory-ci-manual.yml` (T030) | T024, T030 |
| `.github/workflows/rust-ci-runner-release.yml` | smoke step without the mount | T024 |
| `.github/factory-ci-repos.toml` | deleted | T025 |
| `.specify/memory/constitution.md` | Principle I and II, version | T026 |
| `README.md`, `rust-ci-runner/README.md`, GitHub description | docs | T032 |
| tree scrub: README, specs 005 to 013, constitution, workflow names | names | T030 |

## Data and API contracts

[contracts/runner-cli.md](contracts/runner-cli.md) (parser messages and rules, `plan --repos - --check`, `ci` faults), [contracts/script-changes.md](contracts/script-changes.md) (helper, plan script, lint rules, heartbeat), [contracts/history-rewrite.md](contracts/history-rewrite.md) (scrub strings, runbook, tick behaviour, support request); entities in [data-model.md](data-model.md).

## Ordered steps and engineer briefs

Rules for all: no cargo, no tests, no `bws` or `bw`, no secret value anywhere on the planning or agent VM; `git add` explicit paths; never force-push (T033 is the owner-gated exception, run by the lead); tests use fakes and `SECRETMARK_*` markers; no real watched repo name in any file. Each brief is self-contained. Tests come first (`#[ignore = "test-first"]` while the code is missing).

**T001 (owner)**: create `FACTORY_CI_REPOS` in the project named by `FACTORY_CI_BWS_PROJECT`, holding the committed file's two `[[watch]]` tables (and 012's T020 additions if they landed). Accept: exactly one row with that key. (The entry-count check belongs to T016, when the checking command exists.)

**T002 (owner)**: confirm the Bitwarden secret value limit (the SDK validation figure is 25,000 characters, unverified against the docs), that the machine account is read-only and scoped to this one project, that `FACTORY_CI_BWS_PROJECT` holds that project's id, and that Bitwarden event logs are on. Accept: each answer recorded on the task line.

**T003 (owner, H1, M4)**: Environments `factory-ci-plan` (also used by `verdict`), `factory-ci`, `factory-ci-docker`: deployment branch rule `main` only; no required reviewer; the Bitwarden tokens, both GitHub tokens and the blob keys exist as Environment secrets only (delete repo-level copies); a branch ruleset protects `.github/**` and `rust-ci-runner/**`; both GitHub tokens are fine-grained and scoped to the intended repos. Accept: recorded, with the check that a branch run of the tick receives no secret.

**T004 watch tests** (`watch_tests.rs`): every message of the table in contracts/runner-cli.md; a marker loop over every bad input (repo, glob, secret name, id string, unknown key, TOML bytes, a value under 4 characters) with no marker in any error; `number` counts from 1 and counts invalid entries; the new rules (id required with `allow_docker` or `secrets`, `*` and `**` refused in `secret_branches`, zero entries refused, owner mismatch through `check_owner` and unset owner refused); rewrite the existing tests that expect `repo: why` messages, that accept `secret_branches = ["*"]` (010's test becomes a refusal test), or that build `allow_docker` entries without `id`. Accept: fails before T009, passes after.

**T005 store tests** (`store_tests.rs`): `watch_text` (missing, two rows, empty, blank, null, non-string, one row among 30, wrong case, newlines and tabs kept); `attempt` passes `--project` as one argv entry and refuses `--x`, empty, 65 characters, a space; `fetch_json` retries once on a transient failure and never on 400, 401 or 403; `fetch_with` unchanged. Accept: as listed.

**T006 `ci` tests** (`ci_tests.rs`, new `mod list_store`): without `--repos` and with a fake `bws` serving the four settings and the list: listed repo proceeds with exactly one call and the project argument; unlisted prints the old line; empty token prints the old line with no call (`smoke_unlisted_without_token`); missing, duplicate, empty, malformed, 101-entry and owner-mismatch lists print `watch list unusable`; no marker reaches stdout or stderr; the existing file-based tests keep `--repos`. Accept: as listed.

**T007 plan tests** (`plan_tests.rs`, `plan_error_tests.rs`, `plan_cap_tests.rs`): `- item N` and `entry K:` forms; K survives the docker partition; the unchecked list holds `entry N`; the id-mismatch line carries no id; the private-repo refusal says `entry N`; an API error quoting a repo name is masked in phase A and phase C (seeded from candidate rows); `B_PRIVATE` empty, `false`, garbage and `true` through `plan_opts()` and `fail_closed`. Update the tests that expect repo names. Accept: as listed.

**T008 hostile-list test, permanent** (`src/plan_hostile_tests.rs`, `tests/cli.rs`): one list of entries whose repos, owners, branches, globs, secret names, ids and an unknown key are unique markers, including an `allow_docker` entry and no docker token; run `parse`, `plan --repos FILE|- --check`, `plan_a`, `plan_c` and `ci` (store and file source) through a fake API and fake `bws`; assert no marker in stdout, stderr, summary, candidates, rows file or status descriptions; also `plan --repos -` cases (valid, invalid, empty, zero entries, `--repos -` without `--check` is a usage error) and `plan_a` with zero entries (exit 1, no candidates file). A header comment names feature 011 as bound by it. Accept: as listed.

**T009 `watch.rs`**: D2. `parse` output for valid input unchanged; messages from the table only; `check_owner`; no vendor word. Cites `data-model.md` `WatchEntry.number`. Accept: T004 passes.

**T010 `store.rs`**: D3. Cites `data-model.md` `ListError`. Accept: T005 passes.

**T011 `plan.rs`** (and audit of `walk.rs`, `projects.rs`): D6; replace the id line (`plan.rs:231`) with `id differs from the list`; grep every `format!` that touches `e.repo`, `e.id`, `e.branches`, `r.repo` in the four files and fix. Accept: T007 and T008 pass; the grep output is pasted in the commit message.

**T012 `main.rs`**: `plan --repos -` (stdin, capped at 64 KiB + 1 byte, `--check` only); zero-entry refusal and `check_owner` in `plan_a`; `B_PRIVATE` fix (D8); redaction seeds (D6); the lines at 131 and 180 use `entry N`; `plan` arm calls `silence_panics()`; update 012's arm test (D9). Accept: T007, T008 and the arm test pass.

**T013 `ci.rs`, `log.rs`**: D4. Accept: T006 passes; existing `ci_tests` pass; the smoke expectation holds.

**T014**: version `0.5.0` in `Cargo.toml` and `Cargo.lock`. Accept: `cargo_toml_and_lock_versions_match` passes.

**T015 (lead)**: green selftest on the branch; merge PR-1; tag `rust-ci-runner-v0.5.0`; smoke test passes; **record the current `RUNNER_IMAGE_DIGEST` value on this task line first**; then set the new digest. Accept: both digests recorded. Rolling back below 0.5.0 makes the public summary print repo names again and drops M1 to M4; do it only together with `gh workflow disable factory-ci-tick`.

**T016 (owner)**: dispatch one tick with the new runner, the old script and the committed file. Accept: green, same rows as before; pipe the Bitwarden value into `rust-ci-runner plan --repos - --check`: it prints the expected entry count (also after every later edit of the key).

**T017 shell tests first** (`test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh`): `plan_world` serves `FACTORY_CI_REPOS` through the fake store and no longer creates a file under `.github`; assert the runner stub receives `--repos` pointing at a mode-0600 file holding the exact value, the file is gone after exit 0 and after every failure, the runner stub's environment lacks the Bitwarden token, an idle tick makes exactly one `bws` call (the old "no store call" test becomes this), each failure (no token, store down, 401, key missing, key twice, key empty, value over 64 KiB) exits 1 with the fixed summary text, no phase A and no list text in the summary or `GITHUB_OUTPUT`. Accept: fails on the old script, passes on T018.

**T018 script** (`factory-ci-secrets.sh`, `factory-ci-plan.sh`): contracts/script-changes.md. Accept: T017 passes; no `set -x`; no list text in any `printf`.

**T019 lint and heartbeat** (`factory-ci-lint-workflow.sh`, `factory-ci-tick.yml`): replace rule `no-schedule` (D5), add the heartbeat and plan-step rules, replace the self-test row; remove `if: always()` from the heartbeat step only. Accept: the lint passes on the tick, `--self-test` passes, `test-factory-ci-hardening.sh` passes.

**T020 selftest YAML** (`factory-ci-selftest.yml`): a `workflow`-job step asserting the plan step has no `continue-on-error` and no `if:`, that `run`, `run-docker` and `verdict` hold no `FACTORY_CI_REPOS` and no `fci_watch_list`, and that the tick has no `[u]pload-artifact`. Accept: green; red on a copy with a violation (documented reasoning if no run is possible).

**T021 (owner)**: after PR-2 merges, dispatch one tick: green, rows equal T016 (SC-002); the summary has `item N` only; temporarily rename the key, dispatch, see a red tick, no new status and no heartbeat, rename back; check that the `plan` log shows no repo name. Accept: recorded.

**T022 (owner)**: right before PR-3 merges, compare the Bitwarden value with `.github/factory-ci-repos.toml` including 012's T020 additions. Accept: equal entries, or the file is updated first.

**T023 tests first** (`tests/repo_files.rs`, fixture): `committed_watch_file_is_absent`, no other committed `.toml` outside `tests/fixtures` holds a `[[watch]]` table; keep `repeated`, `entry_problems` and their self-tests on fixture text; delete the real-file tests; add `rust-ci-runner/tests/fixtures/watch-list.example.toml` with fake `owner/name` entries; the selftest runs `plan --repos <fixture> --check`. Accept: red while the file exists.

**T024 manual cutover** (`factory-monorepo-ci.yml`, `rust-ci-runner-release.yml`, `test-factory-monorepo-ci.sh`): pin `RUNNER_IMAGE` to 0.5.0 and its digest; drop `--repos` and the mount; add `-e FACTORY_CI_BWS_PROJECT -e GITHUB_REPOSITORY_OWNER` and their `env:` lines; remove the default of the `repo` input; the release smoke runs `ci` with no `--repos` and no mount; the shell test expects two mounts, no `--repos`, the new flags. Accept: that test passes; the lint passes.

**T025 delete** (`.github/factory-ci-repos.toml`, `factory-ci-selftest.yml`): delete the file; remove it from both `paths` filters; the `list` job's `--check` step uses the fixture. Accept: T023 passes; the selftest is green.

**T026 constitution**: `speckit-constitution` rules (D10): Principle I names the key, Principle II lists the watch list as a Bitwarden value read by the plan script and `ci`, the cron is permitted, Sync Impact note. Accept: no sentence names the committed file as the source.

**T027 (owner)**: after PR-3: dispatch a tick and run the manual workflow for a listed repo and an unlisted repo; grep the new runs' logs for the scrub strings. Accept: green, fixed lines only, no hit.

**T028 cron** (`factory-ci-tick.yml`, lint script, selftest): add the schedule; add the lint mutation row that changes the real cron; selftest check with `[c]ron`. Accept: lint, self-test, shell tests and selftest green; a second cron line fails.

**T029 (owner)**: the merge of PR-4 is done from the owner's own account (GitHub mails that account on every failed run). Document the stop command `gh workflow disable factory-ci-tick`. Watch a week: count Canceled and Failed separately; one Failed per tick sends one email. Accept: SC-004 recorded.

**T030 tree scrub and rename**: apply the owner's replace file to the tree mechanically, then review the diff by eye; `git mv` the manual workflow to `factory-ci-manual.yml` and its test to `test-factory-ci-manual.sh`; update the `name:`, concurrency group, `paths` filters in the selftest, the release workflow, README, constitution, the test's own default path and every pin; the listed places: README line 51 area, the workflow's `repo` input description, specs 005 to 013. Accept: the grep of contracts/history-rewrite.md finds nothing in the tree except that file's derivation commands; the shell test and the selftest pass.

**T031 (owner)**: delete old tick and manual runs that print repo names: `gh run list` then `gh run delete` per run id, for the tick and for OLD_MANUAL, until the list is empty (command in the runbook). Accept: `gh run list` for both workflows is empty.

**T032 docs**: root `README.md`, `rust-ci-runner/README.md`, GitHub description. Contents: where the list lives and its format; no committed list; `plan --repos - --check`; the tick reads the key with the plain token (the plain Bitwarden token is now required); `ci` reads it in-process; the schedule and its limits; Canceled versus Failed; the failure email per tick; `gh workflow disable factory-ci-tick`; the heartbeat rule; the accepted leaks (counts, item numbers, run timing, the presence of `allow_docker` rows, the manual workflow's public input); the rollback caution (digest below 0.5.0); the variable table notes `FACTORY_CI_BWS_PROJECT` is read by the plan script and `ci`; the manual workflow name. Accept: a grep of both READMEs finds no instruction to list repos in a file and no scrub string.

**T033 (owner-gated, irreversible)**: history rewrite, contracts/history-rewrite.md. The lead runs it only after a fresh, explicit owner go, and only after 012, 011 and 013 have merged and no other branch or open pull request remains.

## Test plan (what QA runs)

| Level | Where | What |
|---|---|---|
| Unit | `cargo test --lib` | T004 to T008 groups; existing suites with the listed rewrites |
| Integration | `cargo test --test '*'` | `cli`, `repo_files`, `no_vendor` |
| Shell | selftest `shell` job | `test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh`, the manual-workflow test |
| Lint | selftest `workflow` job | lint with the new rules and `--self-test`, plan-step and cron checks |
| Smoke | release workflow | `ci` without token and without `--repos` prints the unlisted line |
| Manual | owner | T016, T021, T027, T029 |

Acceptance to test map: US1.1, US1.4, US2.1 to 2.5 T017; US1.2 T020; US1.3 T016, T021; US2.3, 2.6 and US3 T008; US2.7 and US5.4 T019; US4 T006; US5.1 and 5.2 T019 and T028; US5.5 T007; US6 T023; US7 T008; US8.1 T030; US8.2 and 8.3 T033 verification.

## Failure-mode table

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| Script: token | empty | `[ -n ... ]` | exit 1 `BWS_ACCESS_TOKEN is not set`, no phase A | T017 |
| Script: fetch | store down, timeout, 401, 403 | `fci_list` | existing retry or stop, exit 1 fixed text | T017 |
| Script: key | missing, twice, empty, over 64 KiB | `fci_watch_list` | exit 1 fixed text, no file left | T017 |
| Script: temp file | `mktemp` fails | return code | exit 1 `cannot create a temp file` | T017 |
| Runner: parse | bad TOML, unknown key, 101 entries, owner, id, glob rules, zero entries | `parse`, `check_owner` | exit 1, `entry N` lines only | T004, T008 |
| Runner: API problem | rate limit, id mismatch | existing | summary names `entry N` | T007 |
| Runner: no docker token | docker entry | existing | `entry N: FACTORY_CI_TOKEN_DOCKER is not set` | T008 |
| `B_PRIVATE` empty | cron | `plan_opts` | private repos fail closed | T007 |
| `ci`: list | missing, bad, rule-breaking | `watch_text`, `parse` | `watch list unusable` | T006 |
| `ci`: no token | smoke | empty token | old unlisted line | T006 |
| Heartbeat | plan failed | step has no `if: always()` | no ping, red run | T019 |
| Schedule | late or skipped | Actions history | next tick covers; statuses idempotent | T029 |
| Schedule | disabled after 60 days | Actions notice | owner pushes a commit; README | docs |
| Concurrency | three triggers | group | serial; pending replaced shows Canceled | T029 |
| Cutover | file deleted while Bitwarden differs | owner check | T022 blocks the merge | T022 |
| Rollback | digest below 0.5.0 | README, T015 | refused except with the tick disabled | docs |
| Rewrite | wrong strings, missed ref | verification grep on a fresh mirror | backup restores | T033 |

## Pre-mortem: five likeliest causes of a broken ship

1. **The file is deleted before Bitwarden holds the final list**: the tick fails closed or watches the wrong set. Prevented by the split cutover (T022 equality check, own PR) and by zero-entry refusal.
2. **A repo name or id leaks through a path nobody listed**: summary, a problem line, a panic, a status. Prevented by the one-form parser, the grep of every `format!` (T011), the permanent hostile-list test over every phase (T008), redaction seeds, and the post-cutover search (T027).
3. **The cron runs with `B_PRIVATE` empty**: the private-repo guard is off. Prevented by D8 and its test; the owner sees the first scheduled summary.
4. **The frozen-looking shell tests, lint or selftest fail on the new shapes**: red branch, blocked merge. Prevented by writing the tests first (T017), changing the lint rule and its mutation rows together, and running the selftest on each branch before merging.
5. **The history rewrite breaks the repo**: lost SHAs, a flood of re-runs, moved tags rebuilding images. Prevented by the private mirror backup, disabling the tick and the release workflow around the force-push, the D11 walk analysis, the owner's explicit go, and the verification grep.

## Invariants for QA property tests

- I1: for any list text, no output of `parse`, `plan --check`, `plan_a`, `plan_c`, `ci` or the plan script contains a repo, owner, branch, glob, secret name, id or unknown key from that text.
- I2: the plan script leaves no file holding list text on any exit path, and the file's mode is 0600.
- I3: the plan runner's environment never holds a Bitwarden token variable; no run job, `job`, `status` or phase C reads `FACTORY_CI_REPOS`.
- I4: `summary()` contains no repo name for any candidates input.
- I5: entry numbers are the 1-based position in the text, whatever skipping follows.
- I6: `ci` makes at most one `bws` call per run, and none with an empty token.
- I7: `b_private` is true iff `B_PRIVATE == "true"`.
- I8: `parse` accepts a text only if zero-entry, owner, id and `secret_branches` rules hold.
- I9: with a valid list, the rows planned equal the rows planned from the same text through the old file path (differential).
- I10: the heartbeat step runs only after a successful plan and `fail_tick != 1`.

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
|---|---|
| Inputs: empty, one, max, over | handled: blank, one entry, 100, 101, 64 KiB + 1 (T004, T017) |
| Duplicates | handled: two rows with the key (`2` from `fci_key_value`, `Many`), duplicate repos (parser) |
| Unicode, NUL, newline | handled: jq and JSON carry them; the parser refuses control characters in names and globs; CRLF valid TOML; markers with Unicode in T008 |
| Malformed, wrong type, missing field | handled: non-JSON `bws` output, non-string value, missing key |
| State: first run, older data | handled: key before digest (T001, T015); file kept until T025 |
| Crash mid-operation | handled: temp file removed by `fail` and normal exit; VM discarded |
| Partial failure | handled: fetch fails, no phase A, no status |
| Retry and idempotence | handled: rerun refetches; double tick idempotent by statuses |
| Two callers | handled: concurrency group; one pending slot; Canceled counted |
| Timeouts | handled: 30 s per attempt in `fci_list`, `FACTORY_CI_FETCH_WINDOW`; 15 minute job limit |
| Clock, DST | N/A: cron is UTC, no time logic added |
| Late or skipped scheduled run | handled: next tick; README; T029 |
| Disk full | handled: write failure returns non-zero; the script fails |
| Large input | handled: 64 KiB cap; `bws` output read as today |
| Slow or garbage dependency | handled: T017 |
| Unbounded growth | N/A: temp file removed; rewrite is one-off |
| Memory on the smallest machine | N/A: kilobytes |
| Permissions and trust | handled: Environments main-only, no reviewer, read-only machine account, token not in the plan runner (threat model) |
| What the caller must not see | handled: I1, I2, I4, redaction seeds |
| Values trusted by mistake | handled: project id validated; owner, id and `secret_branches` rules; `B_PRIVATE` safe default |
| UI states | N/A: no UI; summary has empty, error and idle forms |

## Rollout and rollback

Order (FR-017): T001 to T003 owner; PR-1 runner (T004 to T014); T015 release, previous digest recorded; T016; PR-2 script, lint, heartbeat, selftest (T017 to T020); T021; T022; PR-3 file deletion and manual cutover (T023 to T026); T027; PR-4 cron (T028); T029; PR-5 tree scrub and rename (T030); T031; T032 docs; T033 rewrite, only on the owner's fresh go. Each PR waits for a green selftest on its branch (pull request checks return 403 here, so the push-to-branch selftest is the gate).

Rollback, in reverse: PR-5 revert (names return to the tree); PR-4 revert (cron off, outside dispatcher continues); PR-3 revert (the file returns, the manual workflow returns to the old pin only together with the file); PR-2 revert (the script reads the file again; the Bitwarden key stays); digest back to the recorded one (refused below 0.5.0 unless the tick is disabled). T033 cannot be rolled back; the private mirror is the only way back.

## Devops infrastructure assessment

Infrastructure touched: one Bitwarden key (owner), no new GitHub secret, variable, Environment or runner label, plus Environment settings (T003). Load: 288 extra `bws` calls a day, 576 on busy ticks. Actions minutes are free in a public repo; an idle tick takes about 2 minutes. Monitoring: a red tick mails the account that owns the schedule; the heartbeat now pings only after a good plan, so its silence also signals a bad list. The cron doubles the cadence while the outside dispatcher runs; a Canceled run is a replaced pending run, not a failure. Rollback of the runner digest is constrained (T015). The value limit is unverified (T002). T017 and T019 also change shell tests and the lint, which run only in the selftest.

## Security assessment and threat model

**Trust boundaries.** The Bitwarden project is the source of truth for the list and for pipeline secrets: write access to it equals full CI compromise, because a list edit can add a repo, a branch or a secret name. The tick VM is ephemeral. The plan runner parses data from watched repos and holds no store token. The public Actions log, summary, statuses and commit history are public.

**The 13 controls.**

1. Environments main-only: the Environments that hold the Bitwarden token, the GitHub tokens and the blob keys accept deployments from `main` only and have no required reviewer (T003); a branch run gets no secret.
2. A read-only machine account scoped to one project (T002).
3. The plan script discards every row except `FACTORY_CI_REPOS` before running the runner (T018).
4. No list text in any output: one-form parser messages, `entry N`, `item N` (T009, T011, T012).
5. The hostile-list test over every phase, permanent, binding 011 (T008).
6. Phase C redactor seeds from the candidate rows; values under 4 characters are never quoted (T011, T012).
7. The `B_PRIVATE` safe default (T012).
8. The owner rule, the `id` rule and the `secret_branches` rule, plus fine-grained tokens (T003, T009).
9. Files that hold list text: exclusive create, 0600, removed on every path; no symlink or pre-existing file is followed (`mktemp`) (T017, T018).
10. No `continue-on-error` on the plan step and no `if:` on it, asserted by the lint and the selftest (T019, T020).
11. The heartbeat gated on plan success (T019).
12. Bitwarden event logs on, plus the owner's entry-count check after every edit of the key (T002, T016).
13. The README lists what stays visible in public: counts, item numbers, run timing, the presence of `allow_docker` rows, the manual workflow's `repo` input (T032).

Other findings: argument injection through the project id is closed by the validator (D3); fork pull requests get no secrets and `schedule` never runs on forks; a malicious list edit needs Bitwarden write access (out of model); the history and old run logs are handled by T030 to T033; a leaked name through a GitHub cached view is for GitHub Support (draft only).

## Open risks

- The Bitwarden value limit is unverified (T002). The caps of the parser are lower than the figure found.
- The plain Bitwarden token is now required by every tick. A setup with only the docker token breaks; the README says so.
- The `--repos FILE` option stays in `ci` and `plan` for tests and the fixture; a workflow that passes it would bypass Bitwarden. The lint for the manual workflow forbids it (T024).
- GitHub may disable the schedule or skip runs; the outside dispatcher stays until T029 shows the cron is steady, and 011's floor tolerates extra ticks.
- The rewrite is irreversible and breaks every open branch, pull request and clone; forks keep old history; GitHub keeps old SHAs reachable until Support purges them.
- 012's publish step must be idempotent for the post-rewrite run (D11).
- `ListSource` is gone, but `ci_tests` still builds on the file option; revisit when it is split.
- The shell tests, the lint and the selftest cannot run on the planning VM; the first full evidence is the selftest on each branch.

## Analyze result (`/speckit-analyze`, read-only)

Run after tasks.md was written. Findings and their fixes are recorded at the end of tasks.md ("Analyze result").
