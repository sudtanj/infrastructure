# Implementation Plan: Bitwarden watch list and a 5-minute schedule for the factory CI tick

**Branch**: `013-bitwarden-watch-list` | **Date**: 2026-11-28 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `specs/013-bitwarden-watch-list/spec.md`

Skill note: `.specify/scripts/bash/setup-plan.sh` ran. `.specify/extensions.yml` does not exist, so no hooks ran. Constitution Principle VI also asks for `/speckit-clarify` before this plan and `/speckit-analyze` after `tasks.md`; the clarifications are recorded in spec.md and `/speckit-analyze` runs on this revision. Nothing was built or run on the planning VM (low RAM): every file claim below comes from reading the code at `origin/main` 8660df5.

## Summary

The watch list moves from the committed file into the Bitwarden key `FACTORY_CI_REPOS`. Two readers get it: a new runner subcommand `watch-list` that a tick step runs before `plan` (it writes the validated list to the stub path on the discarded VM), and the manual `ci` command, which reads it in the `bws` call it already makes. The committed file becomes a comment-only stub. Everything that prints about the list switches to `entry N`. The tick gains a 5-minute cron in the last pull request.

### Evidence that shaped the design (read from the frozen files)

1. `factory-ci-plan.sh` runs every runner call through `run()`, which is `env -u BWS_ACCESS_TOKEN -u BWS_ACCESS_TOKEN_DOCKER -u FACTORY_CI_BLOB_KEY -u FACTORY_CI_BLOB_KEY_DOCKER "$runner"`. The plan runner therefore cannot read Bitwarden. The script is frozen, so the read moves to a separate workflow step. 011's spec (line 49) relies on the same fact and stays true.
2. The same script passes `--repos .github/factory-ci-repos.toml` to phase A, relative to the checkout. A step that writes that path before `plan` feeds phase A with no script edit.
3. `.github/scripts/test-factory-monorepo-ci.sh` pins `factory-monorepo-ci.yml`: exactly one `docker run`, exactly three `-v` mounts, the text `/etc/factory-ci/repos.toml:ro`, no word `bws`. A pre-fetch step there fails the shell tests, so `ci` reads the list in-process. The same pin forbids deleting the file (the mount source would be created as a directory), so the stub stays.
4. `.github/scripts/factory-ci-lint-workflow.sh` rule `no-schedule` (line 92) fails on any `schedule:`, and its self-test mutates a copy to prove it. The selftest YAML lints a copy without the block (see D5).
5. The release workflow's smoke test runs `ci --repos <stub> --repo x/y --ref main` with no token and demands stderr exactly `[runner] repository or ref not in the watch list`, exit 1. The new `ci` keeps that output for an empty token.
6. `rust-ci-runner/tests/no_vendor.rs` allows `bws` and `bitwarden` only in `pipeline.rs`, `store.rs`, `ci.rs`, `engine.rs`, `main.rs`, `log.rs`. `watch.rs` and `plan.rs` must not name them, so the Bitwarden code lives in `store.rs`, `main.rs`, `ci.rs`.
7. `watch::parse` uses the repo name as the label of an entry and quotes globs, secret names and values in messages. `plan.rs` prints `- item N: <repo>`, `<repo>: <why>` and the unchecked repo list into `$GITHUB_STEP_SUMMARY`, which is public. Both leak a confidential list.
8. `verdict`'s heartbeat step runs on `always()`, so a failed plan still pings the dead-man URL. A red tick is the only alarm for a bad list (GitHub emails on a failed run).

## Technical Context

**Language/Version**: Rust 1.90 (`rust-toolchain.toml`), GitHub Actions YAML, bash (frozen)

**Primary Dependencies**: existing only (`serde_json`, `toml`); no new crate. `bws` 2.1.0 pinned by the existing install step.

**Storage**: Bitwarden Secrets Manager key `FACTORY_CI_REPOS`; one 0600 file on the tick VM for the length of one job.

**Testing**: `cargo test` (unit in `src/*_tests.rs`, integration in `tests/`), the `factory-ci-selftest` workflow, shell tests (frozen, must stay green). No test runs on the planning VM.

**Target Platform**: GitHub-hosted `ubuntu-24.04`, runner image `linux/amd64,linux/arm64`.

**Project Type**: CLI plus CI workflows.

**Performance Goals**: one extra `bws secret list` per tick, 60 s per attempt, one retry. 288 ticks a day, so 288 Bitwarden calls a day.

**Constraints**: public repo (nothing about the list in any log); no secret in any file or message; no edit under `.github/scripts/`.

**Scale/Scope**: about 25 entries fit a 3,500-character value (unverified limit).

## Constitution Check

| Principle | Result |
|---|---|
| I One home | Pass. Workflows in `.github/workflows/`, runner in `rust-ci-runner/`. The line "which repositories the runner watches is declared in `.github/factory-ci-repos.toml`" is stale after this feature; T024 amends it. |
| II Secrets source | Pass with an amendment. The list is a pipeline-side value read from Bitwarden at run time with the Bitwarden token, which the plan job already holds. The new step uses `BWS_ACCESS_TOKEN` of `factory-ci-plan`; no new GitHub secret. The README variable table keeps `FACTORY_CI_BWS_PROJECT`, now also read by the new step. |
| III Nothing private in public logs | Pass by design. FR-004 and the invariants below; the summary change closes an existing path that would otherwise leak. |
| IV Simplicity | Pass. One new subcommand, one seam in `ci`, no new crate, no script edit. Complexity row below. |
| V Tested changes | Pass. Every acceptance scenario maps to a test in "Test plan". |
| VI Spec-driven | Pass. Artifacts come from the Spec Kit skills; implementation covers only `tasks.md`. |
| Third-party actions pinned, minimal permissions | Pass. No new action, no new permission. |

Re-check after design: unchanged. The version bump is MINOR (D8).

### Complexity Tracking

| Item | Why needed | Simpler alternative rejected because |
|---|---|---|
| Two readers (`watch-list` and `ci`) | `ci` cannot take a pre-step (shell test pins one `docker run`); `plan` cannot hold the token (frozen script strips it) | One reader needs an edit to a frozen script or the manual workflow |
| `ListSource` seam in `ci::Seams` | keeps 2,979 lines of file-based `ci_tests` valid | converting them all to fake `bws` lists is days of churn and risk for no behaviour gain |
| `number` on `WatchEntry` and `entry` on plan rows | the summary must name an entry by position even after docker entries are skipped | repo names in the summary leak the list |

## Architecture decision and alternatives rejected

**Decision (D1)**: fetch-to-file for the tick, in-process read for `ci`, both through `store.rs` and one private parser.

Tick, plan job, in order: `guard` (existing), `fetch runner`, `install bws`, **`fetch watch list`** (new), `plan` (unchanged), **`remove watch list`** (new, `if: always()`). The new step runs `rust-ci-runner watch-list --bws "$RUNNER_TEMP/bin/bws" --project "$FACTORY_CI_BWS_PROJECT" --out .github/factory-ci-repos.toml` with only `BWS_ACCESS_TOKEN`, `FACTORY_CI_BWS_PROJECT` and `FACTORY_CI_BWS_SERVER_URL` in its env. `plan` phase A reads the file exactly as today.

Rejected:

| Alternative | Why not |
|---|---|
| Edit `factory-ci-plan.sh` so phase A keeps the token | The script is frozen. |
| Runner reads `/proc/$PPID/environ` to recover the stripped token | Defeats an isolation the script states on purpose ("the runner never needs the store tokens"). |
| Phase A reads Bitwarden itself | Has no token (evidence 1). |
| Pre-fetch step in `factory-monorepo-ci.yml` | Fails its frozen shell test (evidence 3). |
| Delete the file | Mount source becomes a directory; the shell test pins the mount (evidence 3). |
| Make the tick step `bws`-only in shell with `jq` | Would hold the list in a shell variable and duplicate the validation; the runner already has a tested parser. |
| Put the list in a GitHub secret or variable | The owner chose Bitwarden; GitHub secrets for pipeline data break Principle II. |
| Upload the list as an artifact for the run jobs | The list must not reach run jobs (FR-001). |

### D2 Parser and numbering

`watch::parse(text)` stays as is for the committed file and tests. A new `watch::parse_private(text)` shares the body and differs only in text: every label is `entry N`, TOML syntax errors give `line N: invalid TOML`, and no message interpolates a value, a key name from the input, a glob, a secret name or a repo. Messages come from a fixed table (contracts/runner-cli.md). `WatchEntry` gains `number: usize` (1-based position in the text). Only `watch.rs` builds `WatchEntry` literals (3 sites, grep-verified), so no test file needs a new field.

### D3 Store additions (`store.rs`)

`attempt` takes `project: Option<&str>` and appends it after `secret list` when `Some` (validated: 1 to 64 characters of `[0-9A-Za-z-]`, no leading `-`). `fetch_with` becomes `fetch_json(...)` plus `parse_list_with`, so `ci` can parse the same JSON twice without a second call. `watch_text(json) -> Result<String, ListError>` finds the one row with key `FACTORY_CI_REPOS` (exact case): 0 rows `Missing`, more than 1 `Many(n)`, value absent, not a string, or blank after trim `Empty`. It does not call `usable()` (a TOML value holds newlines). `ListError` and the key name constant sit in `store.rs`; `Debug` prints nothing of the value; the value type has no `Debug`.

### D4 `ci`

`ci::Seams` gains `list: ListSource` with `Store` (production) and `File` (the existing tests' default through the test helper). With `Store`: empty token prints `repository or ref not in the watch list`; one `fetch_json`; `watch_text` then `parse_private` (any `ListError` or parse error prints the new fault `watch list unusable`); entry lookup as today (not found prints the old line); then `parse_list_with(json, keep)` for the four settings and held secrets. One `bws` call in total. `FAULTS` in `log.rs` gains `watch list unusable`; `BWS_ACCESS_TOKEN is not set` leaves it (no code path prints it any more; the tests that expected it change). No detail goes to Loki for a list fault, because Loki settings come from the same call and are not parsed yet; the owner uses `plan --check --repos -` to find the problem.

### D5 Schedule and lint

The cron block is `schedule:` and `- cron: "*/5 * * * *"` under `on:`. The selftest `workflow` job gets two steps before the lint step: (a) assert `workflow_dispatch:` present, `^  schedule:$` once, `cron:` once, and the cron line equals `    - cron: "*/5 * * * *"`; (b) copy `.github/workflows` to `$RUNNER_TEMP/wf`, delete the two schedule lines from the copy of the tick, run the lint on the copy. The `shell` job's `--self-test` call uses the same copy. The lint script and its `no-schedule` row stay untouched, so the lint still catches a second schedule block or `pull_request_target`. The lint test of `selftest-shell` reads the string `factory-ci-lint-workflow.sh --self-test` in the shell job, so that text stays. GitHub cron facts for the README: best effort (late or skipped runs under load), disabled after 60 days without repo activity, runs on the default branch, `github.ref == 'refs/heads/main'` is true, the group `factory-ci-tick` with `cancel-in-progress: false` serialises ticks and keeps at most one pending.

### D6 Redaction and entry numbers in the plan

`Row` (and the `unchanged`/`invalid` status entries that carry a repo) gain `entry: usize`. `problem()` takes the entry number; `Candidates.unchecked` holds `entry N` strings; `summary()` prints `- item N` only. `main.rs::plan_a` and `plan_c` add every repo name (and its owner) found in the list or in the candidates to `Opts.secrets`, and the token if present, so an API error that quotes one is masked. The docker partition in `plan_a` keeps the original numbers because `number` rides on the entry.

### D7 Zero entries

`plan_a` refuses an empty list (`watch list has no entries`). `--check` does not, so the stub passes the selftest. This closes the case where the fetch step is skipped by a workflow edit and the tick watches nothing in silence.

### D8 Version numbers

Runner `0.5.0` (new subcommand, new fault; 012 ships 0.4.0; if 011 releases 0.4.1 first, 0.5.0 still holds). Constitution: MINOR bump, relative to main at merge time (`1.1.1` after 012 becomes `1.2.0`; if 011's `1.2.0` already landed, `1.3.0`), because Principle I and II change where a standing configuration lives. The Sync Impact note says so.

## Merge order and conflicts

1. **012 merges first** (it is the base; this feature is implemented after it). 013 rebases on main after 012 and its release. 012's follow-up T020 edits `.github/factory-ci-repos.toml` (adds `allow_docker` and two secret names to the `sudtanj/infrastructure` entry). If T020 lands before 013's owner step T001, the owner copies the final text; if after, T020 becomes an edit of the Bitwarden value (owner action) and is dropped from git. Either way the stub never holds an entry.
2. **Expected conflicts with 012**: `rust-ci-runner/tests/repo_files.rs` (012 adds the `entry_problems` tests on the real file; T015 here keeps `entry_problems` and its self-test on fixture text and deletes the real-file test), `.specify/memory/constitution.md` (012 bumps to 1.1.1; T022 bumps from the value on main), `README.md` and `rust-ci-runner/README.md` (012 adds sections; T026 edits other paragraphs), `ci.rs` and `main.rs` (012 touches the `keep` block in `ci.rs::job` and adds `silence_panics()` in `main.rs`; T013 and T011 edit nearby lines, so rebase before starting), `Cargo.toml` and `Cargo.lock` (version line).
3. **011 is docs only** and has no code to merge. Its documents carry statements that this feature falsifies: no cron line exists (spec FR-040, O15, "Out of scope" rows), the tick workflow "stays exactly as it is", T030 edits `.github/factory-ci-repos.toml` (it becomes a Bitwarden edit, owner action), its constitution edit (`1.2.0`, Principle II wording about `cron` lines of `factory-monorepo-ci`) collides with T022 on the Version line, and its README text names the committed list. The lead updates those lines when 011 rebases; this feature edits none of them. Evidence 1 (the plan runner gets no token) stays true, so 011's ledger design is unaffected.

## Project Structure

### Documentation (this feature)

```text
specs/013-bitwarden-watch-list/
├── spec.md  plan.md  tasks.md  research.md  data-model.md  quickstart.md
├── checklists/requirements.md
└── contracts/runner-cli.md  contracts/tick-steps.md
```

### Source code: every file to add or change

| File | Change |
|---|---|
| `rust-ci-runner/src/watch.rs` | `number` field; `parse_private`; fixed message table (T009) |
| `rust-ci-runner/src/store.rs` | `project` arg in `attempt`; `fetch_json`; `watch_text`; `ListError`; key constant (T010) |
| `rust-ci-runner/src/main.rs` | `watch-list` command; `plan --check --repos -`; zero-entry refusal in `plan_a`; token removal; redaction seeds; `silence_panics()` for `plan` and `watch-list`; USAGE (T011) |
| `rust-ci-runner/src/plan.rs` | `entry` on rows and statuses; `problem()` and `summary()` use numbers (T012) |
| `rust-ci-runner/src/ci.rs`, `log.rs` | `ListSource`; store path; fault `watch list unusable`; drop `BWS_ACCESS_TOKEN is not set` (T013) |
| `rust-ci-runner/src/{watch,store,plan,ci}_tests.rs`, `fake.rs` | new tests (T003 to T007); fake `bws` list helper |
| `rust-ci-runner/tests/{cli,watch_list,repo_files,no_vendor}.rs` | new CLI tests, stub test, vendor allowlist unchanged (T005, T008, T015) |
| `rust-ci-runner/Cargo.toml`, `Cargo.lock` | version `0.5.0` (T014) |
| `.github/factory-ci-repos.toml` | comment-only stub (T021) |
| `.github/workflows/factory-ci-tick.yml` | `fetch watch list` and `remove watch list` steps (T019); cron (T024) |
| `.github/workflows/factory-monorepo-ci.yml` | `RUNNER_IMAGE` digest line only (T020) |
| `.github/workflows/factory-ci-selftest.yml` | step check for the new tick steps (T016); cron assertion and lint on the stripped copy (T024) |
| `.github/workflows/rust-ci-runner-release.yml` | none (smoke test stays valid); verify only (T017) |
| `.specify/memory/constitution.md` | Principle I line, Principle II bullet, version (T022) |
| `README.md`, `rust-ci-runner/README.md`, GitHub description | last task (T026) |

No file under `.github/scripts/` changes. `git diff --stat origin/main -- .github/scripts` must be empty before every commit.

## Data and API contracts

Full text in [contracts/runner-cli.md](contracts/runner-cli.md) and [contracts/tick-steps.md](contracts/tick-steps.md); entities in [data-model.md](data-model.md).

- `rust-ci-runner watch-list --out FILE [--bws PATH] [--project ID]`: env `BWS_ACCESS_TOKEN` (required), `FACTORY_CI_BWS_SERVER_URL` (optional). Exit 0 and the file written (mode 0600) only for a valid, non-empty list; otherwise exit 1, one fixed stderr line, no file. `--bws` defaults to `/usr/local/bin/bws`.
- `rust-ci-runner plan --repos - --check`: stdin list, `N entries ok` on stdout or `entry N: ...` lines on stderr.
- `ci`: new fault `watch list unusable`; the unlisted line also covers an empty token.
- Key: `FACTORY_CI_REPOS`, exact case, exactly one per project, TOML text, at most 64 KiB, at least one `[[watch]]`.

## Ordered steps and engineer briefs

Each brief is self-contained; the tasks in tasks.md use the same IDs. Rules for all: no cargo, no tests, no `bws` or `bw`, no secret value anywhere on the planning or agent VM; `git add` explicit paths; never force-push; tests use fakes. Run order: T003 to T008 (tests first, `#[ignore = "test-first"]` where the code is missing; remove the marker with the code), then T009 to T014, then PR 1 gate.

**T001 (owner)**: create the key. Goal: `FACTORY_CI_REPOS` exists in the project named by `FACTORY_CI_BWS_PROJECT`, holding the current two `[[watch]]` tables (and the 012 T020 or 011 T030 changes if those landed). Accept: the owner pastes it into `plan --check --repos -` (after T011) or compares it to the file by eye; exactly one key of that name. Constraint: no agent reads it.

**T002 (owner)**: confirm the size limit of a Bitwarden secret value, that the plan job's token can read the project, and that `FACTORY_CI_BWS_PROJECT` holds that project's id. Accept: recorded in the task line.

**T003 watch tests** (`watch_tests.rs`). Cover `parse_private`: each message in the table of contracts/runner-cli.md; a hostile list whose repo, branch glob, secret name, id string, key name and TOML bytes are marker strings (`SECRETMARK_*`) yields errors with none of them (property loop over every bad-input case); `number` is 1-based and counts invalid entries; `parse` (public) still names repos so the old tests stay. Accept: tests fail before T009, pass after.

**T004 store tests** (`store_tests.rs`). `watch_text`: missing, two rows, empty, blank, null value, non-string value, one row among 30 others, key differs in case (`factory_ci_repos` is Missing), value with newlines and tabs accepted, 64 KiB boundary left to the parser. `attempt` with a project passes it as one argv entry (fake `bws` script records argv) and refuses `--x`, empty, 65 characters, a space. `fetch_json` retries once on a transient failure, never on 401, 403 or 400. Accept: as listed.

**T005 `watch-list` CLI tests** (`tests/watch_list.rs`, fake `bws` shell script in the sandbox, like `tests/ship_job.rs`). Cases: valid list writes the exact text, mode 0600; each failure of FR-003 gives exit 1, the fixed stderr line, no file, and removes a pre-existing file; the hostile list leaves no marker in stdout, stderr or the directory listing; the token is absent from stderr when the fake `bws` prints it; a missing token prints `watch list: BWS_ACCESS_TOKEN is not set`; `--out` in a missing directory prints `watch list: cannot write the output file`. Accept: as listed.

**T006 plan tests** (`plan_tests.rs`, `plan_error_tests.rs`, `plan_cap_tests.rs`). The summary of a tick over entries with marker repos holds `- item N` and no marker; a problem line starts `entry K:`; K survives the docker partition (entry 2 allow_docker skipped, entry 3 problem says `entry 3`); the unchecked list holds `entry N`; an API error text containing the repo name is masked; the refusal line for a private repo with no log store says `entry N`. `plan_a` with an empty list exits 1, writes no candidates file (extend `tests/cli.rs`). Accept: as listed; the existing plan tests that expect repo names in problems change to entry numbers.

**T007 `ci` tests** (`ci_tests.rs`, new `mod list_store`). With `ListSource::Store` and a fake `bws` serving the four settings and the list: listed repo proceeds with exactly one `bws` call; unlisted prints the old line; empty token prints the old line and makes no call; missing, duplicate, empty, malformed, 101-entry list print `watch list unusable`; no marker reaches stdout or stderr. The existing file-based tests keep `ListSource::File` through the test helper. Accept: as listed.

**T008 `plan --check --repos -` CLI tests** (`tests/cli.rs`). Valid stdin gives `N entries ok`; invalid gives `entry N:` lines and exit 1 with no marker; an empty stdin gives `watch list: stdin is empty`; `--repos -` without `--check` is a usage error (phase A never reads stdin). Accept: as listed.

**T009 `watch.rs`**: add `number`, `parse_private` with the fixed table; share one parsing body with a `private: bool` flag rather than a copy. Constraints: `parse` output unchanged byte for byte; no `bws` or `bitwarden` in the file (`no_vendor.rs`). Accept: T003 passes, old `watch_tests` pass.

**T010 `store.rs`**: D3. Constraints: no `Debug` on the list value; errors name keys and counts only; reuse `run_capped`; `fetch_with` behaviour unchanged for its callers. Accept: T004 passes; existing `store_tests` pass.

**T011 `main.rs`**: add `watch-list` and `--repos -` handling (read stdin to 64 KiB + 1 byte), refuse zero entries in `plan_a` only, read-and-remove `BWS_ACCESS_TOKEN` at the start of `watch-list` before any thread (like `run_ci`), `silence_panics()` for `plan` and `watch-list` (012 adds the helper; use it), redaction seeds (D6), USAGE and the environment sentence. Write the file with the existing `write_private` (0600); on any failure `remove_file(out)` ignoring not-found. Constraint: stdout of `watch-list` is empty; stderr lines come only from the fixed table. Accept: T005, T006 (`plan_a` part) and T008 pass.

**T012 `plan.rs`**: D6. Accept: T006 passes; serialized `Candidates` still round-trips; no repo name in `summary()` output for any input (property loop in the test).

**T013 `ci.rs`, `log.rs`**: D4. Production `Seams` uses `Store`; the test constructor keeps `File`. Constraint: one `bws` call; the token is redacted; the list text is dropped after parsing and never logged; `is_fault` accepts the new line. Accept: T007 passes; existing `ci_tests` pass; the release smoke expectation (no token, unlisted) holds, asserted by a test named `smoke_unlisted_without_token`.

**T014 version and usage**: `Cargo.toml` and `Cargo.lock` to `0.5.0`; `repo_files.rs` `cargo_toml_and_lock_versions_match` stays green. Accept: both files agree.

**T015 stub test and 012 test relocation** (`tests/repo_files.rs`, PR 2, test first). Add `committed_watch_file_lists_no_repo` (parse the committed file, zero entries, and no line matches `^\s*repo\s*=`); keep `repeated`, `entry_problems` and their self-tests (they run on fixture text); delete the real-file tests `no_secret_name_in_two_committed_entries` and the infrastructure-entry test (their input is no longer in git). Record in the test file header that the owner runs `plan --check --repos -` on the Bitwarden value instead. Accept: fails with entries in the file, passes with the stub.

**T016 selftest step check** (`factory-ci-selftest.yml`, PR 2, test first, lands with T019): add a step to the `workflow` job that asserts, on `factory-ci-tick.yml`, that `fetch watch list` sits after `install bws` and before `plan` inside the `plan` job, that the `run` and `run-docker` jobs and `factory-monorepo-ci.yml` hold no `watch-list` word, that no `upload-artifact` appears, and that `factory-ci-repos.toml` appears nowhere under `uses:`. Accept: red on a throwaway edit that moves the step (documented reasoning if no run is possible); green on T019.

**T017 PR 1 gate (lead)**: the selftest is green on the branch, `main` merge, tag `rust-ci-runner-v0.5.0`, release workflow smoke passes, record the digest, `gh variable set RUNNER_IMAGE_DIGEST`. Accept: the release workflow's smoke test prints no failure; the digest is recorded.

**T018 (owner)**: after T017, dispatch the tick once. It still reads the committed file (the workflow is old) with the new runner. Accept: one green tick with the same rows as before. Purpose: proves the new runner changed nothing before the workflow moves.

**T019 tick YAML** (`factory-ci-tick.yml`, PR 2). Add the two steps of contracts/tick-steps.md. Constraints: the lint rules (no `${{` inside `run:`, values through `env:`, no `if:` on secrets, steps before the wrapper); the new step's env holds only the three names; the `run` and `run-docker` jobs gain nothing; the cleanup step runs `rm -f .github/factory-ci-repos.toml` under `if: always()`. Accept: lint, shell tests and `workflow` selftest job green on the branch.

**T020 manual workflow pin** (`factory-monorepo-ci.yml`, PR 2): set the `RUNNER_IMAGE` line to the 0.5.0 tag and digest from T017, nothing else. Accept: `test-factory-monorepo-ci.sh` passes (regex `rust-ci-runner:[0-9]+\.[0-9]+\.[0-9]+@sha256:[0-9a-f]{64}`).

**T021 stub** (`.github/factory-ci-repos.toml`, PR 2): comments only. Text: the list lives in Bitwarden key `FACTORY_CI_REPOS`, same `[[watch]]` format, validated by `plan --check --repos -`; the tick overwrites this path on its VM; do not commit a `[[watch]]` table here; list the format keys without any real repo name (the example repo is `owner/name`). Accept: T015 passes; `plan --repos .github/factory-ci-repos.toml --check` prints `0 entries ok`.

**T022 constitution** (PR 2): D8. Use the `speckit-constitution` skill rules: Sync Impact note, version, date `2026-11-28` or the merge date. Accept: no mention of the committed file as the source; Principle II lists the watch list as a Bitwarden value read by the plan job and `ci`.

**T023 (owner)**: dispatch one tick after PR 2 merges. Accept: green tick, same rows as T018 (SC-002); the summary names `item N` only; the `plan` log shows no repo name; kill test: temporarily rename the key in Bitwarden, dispatch, see a red tick and no statuses, rename back.

**T024 cron and selftest** (PR 3): D5. Edit `factory-ci-tick.yml` (add `schedule:`) and `factory-ci-selftest.yml` (cron assertion, stripped-copy lint in both jobs) in one commit. Accept: the selftest `workflow` and `shell` jobs are green with the cron present; deleting the cron line or adding a second makes the new assertion step red (shown by a throwaway branch run or a documented local reasoning if no run is possible).

**T025 (owner)**: watch the schedule for a week (SC-004); note late or skipped runs.

**T026 docs, last**: root `README.md`, `rust-ci-runner/README.md`, GitHub description (`gh repo edit sudtanj/infrastructure --description ...`, lead runs it). Contents: where the list lives and its format; the stub; owner setup (T001); the `watch-list` step and the 0600 file; `ci` reads it in-process; the schedule and GitHub cron limits (best effort, 60-day disable, main only, serial through the concurrency group); an idle tick now makes one Bitwarden call; `plan --check --repos -`; names stay in git history; the manual workflow input `repo` is public; the variable table says `FACTORY_CI_BWS_PROJECT` is also read by the watch-list step; README line 53 ("then list repos in `.github/factory-ci-repos.toml`") and the `ci` paragraph change. Accept: grep of both READMEs finds no instruction to list repos in the file.

## Test plan (what QA runs)

| Level | Where | What |
|---|---|---|
| Unit | `cargo test --lib` | T003, T004, T006, T007 groups; existing suites unchanged except the listed rewrites |
| Integration | `cargo test --test '*'` | T005, T008, `repo_files`, `no_vendor`, `cli` |
| Shell | selftest `shell` job | the frozen shell tests, unchanged; they must stay green with the new tick steps |
| Lint | selftest `workflow` job | lint on the stripped copy, cron assertion, step-order assertion (the two new steps sit between `install bws` and `plan`, and `run`/`run-docker` do not mention `FACTORY_CI_REPOS` or `watch-list`) |
| Smoke | release workflow | existing `ci` smoke (no token, unlisted) |
| Manual | owner | T018, T023, T025 |

Acceptance to test map: US1.1 T005+T011; US1.2 lint step assertion; US1.3 T018/T023; US2.1-4 T005; US2.5 lint (the `plan` step needs the fetch step; a failure stops the job); US2.6 T006 (`plan_a` empty); US3 T006; US4 T007; US5 T024 assertions; US6 T015; US7 T008.

## Failure-mode table

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| watch-list: token | env empty or secret missing (forks, rotated) | token read | exit 1, `watch list: BWS_ACCESS_TOKEN is not set`, no file | T005 |
| watch-list: fetch | store down, timeout, garbage, 401 or 403 | `fetch_json` result | 1 retry, then exit 1 fixed line, no file | T004, T005 |
| watch-list: key | missing, duplicate, blank | `watch_text` | exit 1, fixed line, no file | T004, T005 |
| watch-list: parse | bad TOML, unknown key, 101 entries, over 64 KiB, bad glob, zero entries | `parse_private` | exit 1, `entry N`/`line N` lines only | T003, T005 |
| watch-list: write | disk full, bad path | write error | exit 1, `cannot write the output file`, partial file removed | T005 |
| tick: step skipped | stub (zero entries) reaches phase A | `plan_a` | exit 1 `watch list has no entries`, nothing posted | T006 |
| tick: stale file | an older list on the VM | the VM is new each job; the step removes the file first on failure | n/a beyond the removal | T005 |
| plan: API problem | rate limit, id mismatch | existing | summary shows `entry N` | T006 |
| plan: summary | repo name printed | marker tests | none can print | T006 |
| ci: list | missing, bad, empty | `watch_text`, `parse_private` | `watch list unusable`, exit 1 | T007 |
| ci: no token | smoke test | empty token | old unlisted line, exit 1 | T007 |
| schedule | late or skipped run | Actions history | next tick catches up; idempotent statuses | T025 |
| schedule | 60 days without repo activity | Actions UI notice | owner pushes a commit; README says so | docs |
| concurrency | three triggers overlap | group `factory-ci-tick` | serial; one pending kept | existing lint rule `concurrency-*` |
| verdict | failed plan still pings the heartbeat | existing behaviour | tick is red; the owner sees the email | docs note |

## Pre-mortem: five likeliest causes of a broken ship

1. **The stub lands before the key exists, or with an old digest**: the tick watches nothing or fails. Prevented by the order of FR-015 (T001, T017, T018 before T019 to T021), by the zero-entry refusal (D7), and by the red verdict.
2. **A repo name leaks through a path nobody listed**: summary, a panic message, a status, `Debug` output. Prevented by the marker property tests over every output channel (invariants I1 to I4), `silence_panics()`, no `Debug` on list types, and the SC-001 search after the first ticks.
3. **The frozen shell tests or the lint fail on the new YAML**: red selftest, merge blocked. Prevented by writing the steps to the lint rules now (contracts/tick-steps.md quotes them), by the stripped-copy lint, and by running the selftest on the branch before any merge.
4. **A typo in the Bitwarden value stops all CI**: the owner edits the value in the web UI. Prevented by `plan --check --repos -` (T008), by fail-closed behaviour (no wrong repo is planned), and by the red tick with an `entry N: field` line that says where.
5. **The cron floods or starves**: ticks pile up, or GitHub disables the schedule. Prevented by the existing serial concurrency group with one pending slot, the cron in its own last PR (easy revert), and the README note on the 60-day rule.

## Invariants for QA property tests

- I1: for any list text, no stdout, stderr, summary or file produced by `watch-list`, `plan --check`, `plan_a`, `plan_c` or `ci` on failure contains any substring that is a repo name, glob, secret name or id value from that text. Method: generate lists with unique marker values, run every failure and success path, grep all outputs.
- I2: `watch-list` leaves a file if and only if it exits 0, and then the file equals the Bitwarden value byte for byte and parses to at least one entry.
- I3: `parse_private` and `parse` accept exactly the same texts (same Ok and Err split); only the messages differ.
- I4: `summary()` contains no repo name for any candidates input.
- I5: entry numbers are the 1-based position in the text, whatever skipping or filtering follows.
- I6: `ci` makes at most one `bws` call per run, and none with an empty token.
- I7: no run job, `job`, `status` or phase C invocation reads `FACTORY_CI_REPOS` or the list file (the lint step plus a grep test over the YAML).
- I8: with a valid list, the rows planned equal the rows planned from the same text read as a file by the old code path (differential test).

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
|---|---|
| Inputs: empty, one, max, over | handled: blank, one entry, 100 entries, 101 and 64 KiB + 1 (T003, T004, T005) |
| Duplicates | handled: duplicate key rows (`Many`), duplicate repos (existing parser, now `entry N: duplicate of entry M`) |
| Unicode, NUL, newline | handled: JSON strings carry them; the parser refuses control characters in globs and names; CRLF is valid TOML; markers with Unicode in the hostile test |
| Malformed, wrong type, missing field | handled: non-string value, non-JSON `bws` output, missing key (T004) |
| State: first run, older data | handled: the key must exist before the digest moves (T001 before T017); the old file keeps working until PR 2 |
| Crash mid-operation | handled: write to the final path only after validation; remove on any failure; the job VM is discarded |
| Partial failure | handled: fetch fails, no `plan`, no status posted; cleanup runs with `always()` |
| Retry and idempotence | handled: one retry inside the command, a rerun rewrites the file; a double tick is idempotent through statuses |
| Two callers | handled: the concurrency group; each job has its own VM and file |
| Timeouts | handled: 60 s per attempt, 2 attempts (FR-013); the plan job's 15 minutes bound the rest |
| Clock, DST | N/A: cron is UTC; no time logic added |
| Late or skipped scheduled run | handled: next tick covers the gap; README states it; T025 measures |
| Disk full | handled: write error path (T005) |
| Large input | handled: 64 KiB cap read with a one-byte overshoot; the bws list is read through `run_capped` |
| Slow or garbage dependency | handled: T004, T005 |
| Unbounded growth | N/A: nothing is stored; the file is deleted |
| Memory on the smallest machine | N/A: kilobytes; the planning VM ran nothing |
| Permissions and trust | handled: token only in the one step and the manual job; file 0600; no artifact; run jobs get nothing; lint assertion I7 |
| What the caller must not see | handled: I1; masked API errors; no `Debug` |
| Values trusted by mistake | handled: the project id is validated before it becomes an argument; the list is validated whole; a stale file cannot pass because zero entries is refused |
| UI states | N/A: no UI; the summary has the empty, error and idle forms (tests T006) |

## Rollout and rollback

Order (FR-015): T001 key; PR 1 (runner 0.5.0: T003 to T014); T017 release, digest variable; T018 one tick with the old workflow and new runner; PR 2 (T015, T016, T019 to T022); T023 one by-hand tick after merge; PR 3 (T024); T025; T026 docs and description last. Each PR waits for a green selftest on its branch (the PR check returns 403 here, so the push-to-branch selftest is the gate, per the 008 note).

Rollback, in reverse: PR 3 off (remove the cron; the outside dispatcher still ticks); PR 2 off (revert the tick steps and the pin, restore the file from history; the tick reads the file again, the Bitwarden key stays); the digest variable back to the earlier digest; PR 1 last. Reverting PR 2 republishes the names to the working tree (they are in history already).

## Devops infrastructure assessment

None was handed over with the brief; this is the planning view. Infrastructure touched: one Bitwarden key (owner), no new GitHub secret or variable, no new Environment, no new runner label. `factory-ci-plan` already holds `BWS_ACCESS_TOKEN`, so the new step adds no secret to any Environment. Load: 288 extra `bws` calls a day against Bitwarden's rate limits (not verified; the existing tick makes calls only with work). Actions minutes are free in a public repo; an idle tick takes about 2 minutes; 288 ticks a day is fine. Monitoring: red ticks mail the owner; the heartbeat still pings on a red tick, so it does not catch a bad list. A follow-up could ping only on a green plan; out of scope.

## Security assessment

None was handed over; the planning view. Assets: the list (repo names, ids, which repos may hold secrets), the Bitwarden token. Threats and controls: (1) list leaks through logs, summaries, statuses, panics, `Debug` (I1, I4, `silence_panics`, redaction seeds); (2) the file on the VM reaches an artifact or a run job (no upload action, removal step, 0600, lint assertion I7); (3) the token reaches a step that can print it (only the one `run:` step, which runs fixed commands and prints fixed lines; the runner removes the token from its env and masks it); (4) argument injection through the project id (validated); (5) a malicious list edit by someone with Bitwarden write access (out of the model: they could already read pipeline secrets); (6) fork pull requests (no secrets reach them; `schedule` never runs on forks); (7) the committed stub regrows entries (test T015); (8) past names in git history (accepted, documented). Required changes folded in: T005, T011, T012, T015, T019.

## Open risks

- Bitwarden secret value size limit may be below the list size; checked at T002. A list over the limit needs a second key.
- The `ListSource` seam leaves the `File` branch in production code that only tests use; the alternative is a large test rewrite. Revisit when `ci_tests` is next split.
- `watch-list` and `ci` read the list separately; a list change between them in one tick is possible only across different workflows, not within one.
- GitHub may disable the schedule or skip runs; the outside dispatcher stays as a backup until T025 shows the cron is steady.
- The owner's manual workflow input `repo` shows publicly.
- 011's documents contradict this feature (no cron, no edits to the tick); the lead must reconcile them before 011 implements anything.
- Frozen shell tests cannot be run on the planning VM; the first full evidence is the selftest on the branch.
