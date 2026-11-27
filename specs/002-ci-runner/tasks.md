# Tasks: Central Interval CI Runner

**Input**: [plan.md](plan.md), [spec.md](spec.md), [research.md](research.md), [data-model.md](data-model.md), [contracts/](contracts/), [quickstart.md](quickstart.md)

**Tests**: required by the constitution (Principle V). Test tasks come first in each phase. QA writes them against the contracts and they fail until the engineer's task lands.

**Format**: `- [ ] T### [P?] [US#] description with path`. `[P]` means the task touches different files from every other open task of its group and can run in parallel with them. All paths are under `rust-ci-runner/` unless they start with `.github/` or `specs/`.

**Status (2026-11-27)**: T045 and T081 are done by `specs/003-infrastructure-home/` (T005 and T004). T046, T054 and T065 move to 003 T032 to T034 and stay open until the owner's Setup is done.

## Common constraints (every task and brief)

- C1. No local `cargo`, `docker`, `make` or builds: the machine has 1 GB of RAM. Read, write, `grep` and `git` only. Re-read each file you change. List in your report what you could not run. CI is the first compiler (T045).
- C2. No new crate. `tiny_http` is dev-only after T001. No new file outside the plan's list.
- C3. Follow `feiyi-anti-slop-code`: comments only for a reason the code cannot show, no wrapper or option nobody asked for, match the style of the file you edit (short functions, `Result<_, String>`, no async).
- C4. Never pass a value through a host shell; engine and git calls use separate arguments. Never print or return a token, secret value or log credential: every message goes through the `Redactor`.
- C5. `cargo clippy -- -D warnings` runs in CI: no unused imports, dead private items or needless clones. After deleting code, grep for the names you removed.
- C6. Prose in docs and comments follows `stop-slop`.
- C7. A test task creates its test file and adds one `#[cfg(test)] mod <name>;` line to `src/lib.rs`; when two tasks touch that file, keep every line.

## Phases and order

Phases run in dependency order. All stories here are P1 except US8 and US9 (P2) and US10 (P3), and none delivers value alone, so they ship together. US9 comes early because `plan` takes a `WatchEntry`; US3 and US2 come before US1 because the planner calls `projects.rs` and `walk.rs`.

Test tasks (QA) are independent files and run in parallel as one wave after Phase 1: T056, T057, T058, T047, T048, T052, T055, T003, T004, T005, T010, T012, T013, T015, T017, T018, T023, T025, T026, T027, T029, T030, T031, T033, T034, T035, T038, T039. Engineer order: Phase 1, then Phase 2 (T006, T008 in parallel; T007 after T006; T009 after T007), then T011, T014 and T016 in parallel, then plan, job, main, workflow (Phases 6 to 9 are sequential on `plan.rs`), then Phase 10 and 11 in parallel, then Polish.

---

## Phase 1: Setup

- [X] T001 Prune the daemon (brief S1): delete `src/poll.rs src/store.rs src/server.rs src/sched.rs src/config.rs src/status.rs src/app.rs src/app_tests.rs`; trim `src/lib.rs`; move `DIND_IMAGE` into `src/engine.rs` and `clone_image` (`alpine/git`) into a const there; delete `ls_remote`, `cleanup_polls`, `POLL_LABEL` from `src/engine.rs`; delete `is_run_id` and `write_atomic` and their tests from `src/util.rs`; move `run_env` and `local` into a new `src/job.rs` unchanged apart from imports; remove the `poll` import and the polling test from `tests/executor.rs` and the `ls_remote` half from `tests/argv.rs`; move `tiny_http` to `[dev-dependencies]` in `Cargo.toml`; replace `src/main.rs` with `run` and `help` only. Crate must stay coherent: no reference to a deleted name.
- [X] T002 [P] Delete `.github/workflows/factory-ci.yml`, `.github/workflows/factory-ci-dispatch.yml`, `.github/scripts/factory-ci-validate.sh`; remove the `payload` job from `.github/workflows/factory-ci-selftest.yml` (brief S1).

## Phase 2: Foundational (blocks every story)

- [X] T003 [P] Tests in `src/util.rs` `mod tests`: `base64` against the RFC 4648 vectors ("", "f", "fo", "foo", "foob", "fooba", "foobar") and a user:token string; `parse_rfc3339` for `2026-11-26T00:00:00Z`, a leap day, garbage (None); `short_sha` (8 chars, shorter input).
- [X] T004 [P] Tests `src/github_tests.rs` against a `tiny_http` fake (FR-006, FR-013, FR-014): request line, `Authorization: Bearer`, `Accept`, `User-Agent` on each call of `contracts/github-api.md`; pagination stops at a short page and at 5 pages; 5xx retried twice then `Http`; 404 and 403 become `Http`; 429, `x-ratelimit-remaining: 0` 403 and `retry-after` 403 become `RateLimited`; read refused locally when the last `x-ratelimit-remaining` is below 60 while a write still goes out; a sha that is not 40 lowercase hex is rejected; `truncated` tree is reported; the token is absent from every error string; `post_status` body; `compose_description` (140 chars, reason shortened before the suffix, suffix kept; I11). Also pins paging of the combined status call (`page=N` until short or `total_count`), the truncated-tree fallback (root tree plus one call per top-level folder), `GET /user`, and `GET /repos/{r}` (`id`, `private`, `default_branch`).
- [X] T005 [P] Tests in `src/log.rs` and `src/redact.rs` `mod tests` (FR-024, FR-030, FR-032, FR-036): `out` prefixes, redacts then splits at 16 KB on character boundaries (I8) and ships nothing bigger; quiet mode prints no `out` text but prints `event` lines; `event` text is only the fixed vocabulary; no printed line starts with `::` (I17); NUL becomes U+FFFD on the shipped copy; cut-off marker once and also queued; a recording `Sink` receives redacted text only (I7); `Redactor` also redacts each line of 4 or more characters of a multi-line value, and leaves shorter lines alone (A22).
- [X] T006 Impl `src/util.rs` (brief S2): `base64`, `parse_rfc3339`, `short_sha`; keep `hex`, `random_id`, `now`.
- [X] T007 Impl `src/github.rs` (brief S2) citing `data-model.md Branch.sha`, `Commit.parents`, `StatusEntry.updated_at`, `TreeEntry`, `ChangedFile.previous_filename`: `Api` trait, `HttpApi`, `ApiError`, `RATE_RESERVE`, `compose_description`, `post_status`. Adds `user()`, `repo(r)` (id, private, default_branch), paged `status`, and the truncated-tree fallback in `tree`.
- [X] T008 [P] Impl `src/log.rs`, `src/redact.rs`, `src/engine.rs` hooks (brief S3): `Log::out`, `Log::event`, `Log::quiet`, `Sink` trait and `Log::with_sink`; multi-line redaction; engine pumps call `out` with their stream; step results call `event` with durations; clone and sidecar lines go through `out`.
- [X] T009 [P] Impl `src/fake.rs` (brief S2): test-only in-memory `Api` with scripted answers, call recording, per-call error injection, and a counter of reads and writes. Registered in `src/lib.rs` under `#[cfg(test)]`. The fake also answers `user()` and `repo()` and pages `status`.

## Phase 3: User Story 9 - Adding a repo by editing the list file (P2)

**Goal**: a validated watch list. **Independent test**: `plan --check` on good and bad files.

### Tests for User Story 9

- [X] T010 [P] [US9] Tests `src/watch_tests.rs` (FR-004, FR-005; AS 9.1 to 9.3): defaults (all branches, no secrets, no docker); unknown key at root and in an entry; malformed repo (`a/b/c`, `a`, `../x`, unicode, newline, 40-char owner, trailing `-`); duplicate case-insensitive; malformed globs (empty, `[`, `{`, space, `..`, 101 chars) and glob matching (`*` crosses `/`, `?`); reserved secret names including `FACTORY_CI_LOG_URL`, `_USER`, `_TOKEN`, `GITHUB_X`, `DOCKER_HOST`; duplicate secret; 21 secrets; 101 entries; file over 64 KB; empty file valid; all problems reported at once, one per line, in the form of `contracts/watch-list.md`; generator test I10. Also: `id` must be a number; `secret_branches` follows the glob rules of `branches`; `bitwarden_project` is an unknown key (there are no per-repo projects).

### Implementation for User Story 9

- [X] T011 [US9] Impl `src/watch.rs` (brief S4) citing `data-model.md WatchEntry.repo`, `WatchEntry.branches`, `WatchEntry.secrets`, `WatchEntry.allow_docker`: `parse(text) -> Result<Vec<WatchEntry>, Vec<String>>`, `glob_match`, `WatchEntry::matches_branch`. Also parses `id` and `secret_branches` (data-model.md WatchEntry.id, WatchEntry.secret_branches).
- [X] T012 [P] [US9] Tests `tests/cli.rs` for `plan --repos FILE --check` using `env!("CARGO_BIN_EXE_rust-ci-runner")` (AS 9.3): valid file exits 0; each bad file exits 1 and stderr names every problem; no token needed; no network; partial `FACTORY_CI_LOG_*` environment exits 1 naming the missing name (FR-029).

## Phase 4: User Story 3 - Several projects in one repo

**Goal**: find projects and who owns which file. **Independent test**: tree fixtures in `projects` tests.

### Tests for User Story 3

- [X] T013 [P] [US3] Tests `src/projects_tests.rs` (FR-008; AS 3.1, 3.4): root plus two folders gives contexts `factory-ci`, `factory-ci/<a>`, `factory-ci/<b>`; nested file ignored; symlink-mode toml counts as a project; submodule or file named like a folder ignored; no root toml means no root project; folder names outside the rule (`a b`, `-x`, over 100 chars, `..`) are flagged invalid; dot-folder `.github` is valid; ownership: `foo/` does not own `foobar/x`, root owns everything outside project folders, a removed folder's files fall to root, property I6. Also: when the recursive tree is `truncated`, the fallback (root tree plus one non-recursive call per top-level folder) finds the same projects.

### Implementation for User Story 3

- [X] T014 [US3] Impl `src/projects.rs` (brief S5) citing `data-model.md Project.path`, `Project.context`: `find(tree) -> Vec<Found>`, `owns`, `folder_name_ok`. Also implements the truncated-tree fallback.

## Phase 5: User Story 2 - An unchanged project is skipped with success

**Goal**: per-project decision. **Independent test**: `walk_tests.rs` with `FakeApi`.

### Tests for User Story 2

- [X] T015 [P] [US2] Tests `src/walk_tests.rs` (FR-009, FR-010, FR-011; AS 2.1 to 2.4; spec edge cases force-push, new branch, stale pending): `Done` on any status except pending older than 7200 s; boundary at 7199, 7200, 7201 s; unparsable time is stale; nearest ancestor `success` and untouched gives `Unchanged{since}`; touched file (including `previous_filename`) gives `Run{prev}`; ancestor `failure`, `error`, fresh `pending`, stale `pending` all give `Run`; no ancestor in 50 commits gives `Run`; first-parent chain over a merge uses `parents[0]` only; chain leaving the 100-commit window gives `Run`; 0 changed files marks every project with a usable ancestor unchanged; compare still full after 3 pages gives `Run`; commit list, status or compare error (not rate limit) gives `Run`; `RateLimited` propagates; statuses cached per sha across tips and projects (call counts); I4, I5, I13. Retry budget (A6c, FR-009, no extra API call): an `error` older than 1800 s whose description does not end with " (retry)" gives `Run` with `retry` true; with the marker it gives `Done`; an `error` younger than 1800 s gives `Done`; a `failure` is never retried; a `pending` older than 7200 s without the marker is replanned as a retry, with the marker it is closed by an `error` post; a status by another creator is ignored; an unparsable `updated_at` counts as young; I24.

### Implementation for User Story 2

- [X] T016 [US2] Impl `src/walk.rs` (brief S5) citing `data-model.md Decision per project`, `Commit.parents`, `StatusEntry.updated_at`: `decide(api, repo, tip, projects, now, cache) -> Result<Decisions, ApiError>`. Implements the retry budget of A6c with the marker: an `error` older than 1800 s or a `pending` older than 7200 s whose description does not end with " (retry)" gives `Run` with `retry` true (its `pending` is posted as "queued (retry)"); with the marker an old `error` is `Done` and an old `pending` is closed with one `error` post (data-model.md Decision per project).

## Phase 6: User Story 1 - A tick runs a changed project and reports its status

**Goal**: plan, job, final status. **Independent test**: one repo, one project, a passing and a failing commit.

### Tests for User Story 1

- [X] T017 [P] [US1] Tests `src/plan_tests.rs` (FR-007, FR-008, FR-009, FR-011, FR-012; AS 1.1, 1.4, 3.1, 3.2, 9.1; SC-005): a changed project gets `pending` "queued" and one matrix row with every field valid (I14), `prev_sha` set when an ancestor was found; a tip whose context has a status is not planned (second tick plans nothing); branch globs pick matching branches only; two branches at one sha plan once with the alphabetically first name; three projects give three independent items and one failing item does not affect the others' rows; reference fleet (10 repos, 5 tips, 3 projects, all done) makes exactly 120 reads (110 plus one repo lookup per entry) and 0 writes; I2. Phase A (`--rows-out`) writes no status and makes no store call; rows carry `repo_id`, `secrets_ok`, `retry`; a truncated tree is not an error.
- [X] T018 [P] [US1] Tests `src/job_tests.rs` (FR-016, FR-017, FR-022; AS 1.1 to 1.3, 1.5, 6.3) with a stub engine script and a `tiny_http` status server: all steps pass gives `success` "all steps passed"; step exit code, timeout and missing image give `failure` naming the first failed step; clone failure gives `error` "clone failed: ..." for that item only, and a second item run in the same test is unaffected (AS 6.3); a panic in the run gives `error` "internal error" and the status is still posted; invalid TOML, cycle, symlinked file, over 100 steps give `error` with every problem and the stub engine records no container start; final state is never `pending` (I12); the final post is retried after a 5xx; the run env holds sha, prev sha, branch and `FACTORY_CI_EVENT=push`; the clone call carries the token in the environment only; description never contains a secret value. Also: a docker exit code 125 from a step (image cannot be pulled) gives `error`, not `failure` (FR-017); `--retry` appends " (retry)" to every posted description and the suffix survives a long reason; `--private` prints nothing to stdout and exits 1 before cloning without a log store; a step exiting 126 or 127 stays `failure`.

### Implementation for User Story 1

- [X] T019 [US1] Impl `src/plan.rs` core (brief S6) citing `data-model.md Item.run_id`, `Project.context`: per repo, list branches, filter, group by sha, tree, projects, `walk::decide`, post `pending` and `unchanged`, build rows, write `--output` and `--summary`. Split into phase A (`--repos`, `--rows-out`: candidates, reads only) and phase C (`--rows-in`: refusals, cap, posts, `--summary`, `fail_tick`) per contracts/plan-output.md.
- [X] T020 [US1] Impl `src/job.rs` (brief S7) citing `data-model.md JobResult.state`, `JobResult.reason`: `run_pipeline` (shared with `local`), `job`, clone into a temp workspace, project directory check, final status, panic guard, `status` helper. Also: exit 125 and a failed clone map to `error`; `--retry` and `--private` per contracts/cli.md; prepare mode comes in T049.
- [X] T021 [US1] Impl `src/main.rs` and `src/lib.rs` (brief S7): subcommands `plan`, `job`, `status`, `run`, `help` per `contracts/cli.md`; exit codes; usage text. Flags per contracts/cli.md (two plan phases, `job --private --retry --names-file --reason-file`).
- [X] T022 [US1] Create `.github/workflows/factory-ci-tick.yml` as a skeleton only (brief S9; contracts/tick-workflow.md): jobs `plan`, `run`, `run-docker`, `verdict`, the concurrency group, `ubuntu-24.04`, the `main` guards, the Environments, the digest and image guard step first, and calls to the wrapper scripts by name. It writes no script and does not wait for T026; T051 and T063 complete it.

## Phase 7: User Story 4 - Several repos in one tick

### Tests for User Story 4

- [X] T023 [P] [US4] Tests `src/plan_cap_tests.rs` (FR-006, FR-012, FR-055; AS 4.1 to 4.3, 9.4): three repos with new commits in two give rows for both and the third costs only its idle reads; 25 candidate items give 20 rows, oldest commit first within a repo and repos interleaved round-robin (one repo with 25 items cannot take the whole cap; ties by branch, project), `pending` posted for exactly those 20, `waiting` is 5; `--cap` overrides; an unlisted repo receives no call at all, and a removed entry is never touched (I1, I3). Also: item numbers `n` are unique across the capped list.

### Implementation for User Story 4

- [X] T024 [US4] Impl in `src/plan.rs` (brief S6): cap, interleaved order, `waiting`, post-pending-then-emit rule, write pacing of 750 ms (zero in tests) and the 60-write cap with carry-over (see T056, T060).

## Phase 8: User Story 5 - Overlapping ticks do not double-run

### Tests for User Story 5

- [X] T025 [P] [US5] Tests `src/plan_race_tests.rs` (FR-009; AS 5.2 to 5.4, spec edge case "two ticks racing"): fresh `pending` is not planned; `pending` 7201 s old is planned again with a new `pending` post; a tick that planned 20 and a next tick that recomputes from statuses plans the rest, none twice (simulate by feeding statuses back into the fake); posts happen before the output is written (order check on the fake). Also: after a retry posts `pending` "queued (retry)", a following tick sees a fresh `pending` and does not plan it again; a second `error` marked " (retry)" stays `Done` (I24).

### Implementation for User Story 5

- [X] T026 [P] [US5] Create `.github/scripts/factory-ci-lint-workflow.sh` (brief S9; FR-001, FR-003, FR-015, FR-026; AS 5.1, 5.4, 6.2, 10.1, 10.2): asserts the rules at the end of `contracts/tick-workflow.md`; asserts the release workflow still has the `rust-ci-runner-v*` tag trigger, both targets, `linux/amd64,linux/arm64`, and the `RUNNER_IMAGE_DIGEST` summary line; the Bitwarden rules at the end of `contracts/tick-workflow.md`; exits non-zero with the failed rule named. Self-test cases: a good copy passes and one mutated copy per rule fails (the script takes the workflow path as an argument).

## Phase 9: User Story 6 - An unreachable repo does not block the others

### Tests for User Story 6

- [X] T027 [P] [US6] Tests `src/plan_error_tests.rs` (FR-013, FR-014; AS 6.1, 6.2, 6.4; SC-004): a repo answering 404 or 403 is skipped, named with its HTTP reason in `problems`, every other repo is processed, `problems=1` in the output; network error and 5xx after retries likewise; 101 matching branches is a repo error with no partial rows for that repo (a truncated tree is not an error: see T013); `RateLimited` on the third repo keeps the first two repos' rows, posts their `pending`, lists repo 3 and later in `unchecked`, and makes no further read (I13); a `pending` post that fails drops the row and records a problem (I2); output text and summary contain no token (I7).

### Implementation for User Story 6

- [X] T028 [US6] Impl in `src/plan.rs` (brief S6): error classes per repo, rate-limit stop with `unchecked`, atomic per-repo results, `problems` count, summary sections, exit codes. Also computes `fail_tick`: problems or unchecked repos fail the tick only when it starts in the first 10 minutes of a UTC hour (A12); the summary lists them every tick.

## Phase 10: User Story 7 - Steps run as a dependency graph of containers

Graph rules and executor are kept; tests for AS 7.1 to 7.3 and 7.5 already exist in `pipeline::tests` and `tests/executor.rs`. New work covers the docker opt-in, name validation and secret isolation.

### Tests for User Story 7

- [X] T029 [P] [US7] Tests in `src/pipeline.rs` `mod tests` (FR-018, FR-019; AS 7.7): `resolve_secrets` and `docker_problems` without the `fork` parameter (fork tests removed); reserved names `BWS_ACCESS_TOKEN`, `BWS_X`, `LD_PRELOAD`, `GIT_DIR`, `SSL_CERT_FILE`, `BASH_ENV`, `ENV`, `HTTPS_PROXY`, `FACTORY_CI_LOG_URL` refused; a step secret name that is not a variable name (`1BAD`, `A-B`) or is reserved makes `parse` fail with every problem named (FR-019, story 12); `resolve_secrets` reports "not defined" for a listed name the lookup lacks.
- [X] T030 [P] [US7] Tests `tests/job_rules.rs` and `tests/argv.rs` additions (FR-016, FR-018, FR-020, FR-021, FR-024; AS 3.3, 7.4, 7.6, 7.7, 9.2): with a stub engine and a runner environment holding `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `_USER`, `_TOKEN` and a fetched secret no step lists, the recorded `docker run` arguments contain no `-e` for any of them and no socket mount; a step that lists the secret gets `-e NAME` and the value only in the engine client's environment; `docker = true` without `--allow-docker` gives `error` "step X needs docker but repo does not allow it" before any container; a step secret the environment lacks gives `error` "requests secret NAME, which is not defined" before any container; the pipeline directory mounted is `<workspace>/<project>`, never the repo root for a folder project; `project` of `../x`, absolute or symlinked is refused. Also asserts (FR-054) that every step container's `run` arguments include `--cap-drop NET_RAW`, `--cap-drop MKNOD` and `--cap-drop SETFCAP`.
- [X] T031 [P] [US7] Tests additions in `tests/executor.rs` (real Docker, CI only; AS 7.4, SC-007): a step run while the runner process holds `FACTORY_CI_TOKEN` and the log secrets in its environment prints an `env` with none of them; local `run` and `job` over the same fixture produce the same step order and result.

### Implementation for User Story 7

- [X] T032 [US7] Impl `src/pipeline.rs` and the checks in `src/job.rs` (brief S7): drop `fork`, extend reserved names (including `BWS_`), drop the allowlist message, validate step secret names at parse time; `job` runs docker and secret checks before any container.

## Phase 11: User Story 11 - Step logs go to the user's own log server

### Tests for User Story 11

- [X] T033 [P] [US11] Tests `src/ship_tests.rs` (FR-028, FR-029, FR-033, FR-034, FR-036; AS 11.4, 11.5, 11.7; SC-011) with a `tiny_http` fake Loki and short timings injected: `LogConfig::from_env` all, none, one set, two set (each missing name reported), link without `{run_id}`, URL with userinfo or another scheme; payload has exactly labels `repo`, `project`, `step`, `stream`, prefix `run=<id> sha=<8> branch=<=40> `, nanosecond string timestamps, values sorted, `Authorization: Basic` equal to base64 of `user:token`; batch leaves after 1 s or 1 MB; 429 and 5xx retry with backoff until the window then drop; 401 and 403 stop at once and later lines are dropped without a request; 400 drops without retry; `finish` returns within the flush window against a hanging server; queue over 32 MiB drops lines without blocking `push`; the dropped flag is set after any loss; URL, user and token absent from every error string (I7, I15, I16).
- [X] T034 [P] [US11] Tests `tests/ship_job.rs` (FR-030, FR-031, FR-035; AS 11.1 to 11.3, 11.6; SC-009, SC-011) with a stub engine that prints known markers on stdout and stderr, a fake Loki and a fake status server: with shipping on, the job's stdout holds only `event` lines and the run id, no marker; the fake Loki received the markers, with the secret value replaced; clone and sidecar lines reached Loki; with the template the final status link has the run id filled in; with no template there is no link; with shipping off the link is the Actions run URL built from `GITHUB_*`; a dead Loki gives the same state and a description ending " (logs not delivered)" even when the reason is 200 characters; the job takes at most 60 s longer than with shipping off; a step result is unchanged by any shipper failure (I11, I16, I17). Also: for a `--private` row stdout stays empty and the step names and events reach only the fake Loki (FR-059).
- [X] T035 [P] [US11] Tests `tests/cli_logs.rs` (FR-030, FR-037; AS 11.8, 11.9): `run --quiet-logs` with no log environment exits non-zero before running anything and prints the reason; `job --quiet-logs` likewise; `job --private` with no configuration exits 1 before cloning (FR-049, FR-059); `run` with no configuration prints step output to stdout; with a fake Loki configured it ships and prints, and with `--quiet-logs` ships and does not print. Also: `job --private` with no configuration exits 1 before cloning and prints nothing.

### Implementation for User Story 11

- [X] T036 [US11] Impl `src/ship.rs` (brief S8) citing `data-model.md LogConfig.url`, `LogConfig.user`, `LogConfig.token`, `LogConfig.link`, `LogLine.labels`, `LogLine.text`, `LogLine.ts_ns`: `LogConfig::from_env`, `Shipper` (thread, queue, batch, retry, flush), `impl Sink for Shipper`, `link(template, run_id)`.
- [X] T037 [US11] Wire shipping (brief S8): `src/job.rs` and `src/main.rs` build the `Log` with the `Shipper` when configured, honour `--quiet-logs`, flush before the final post, append " (logs not delivered)" through `compose_description`, choose the link; `plan` validates the log config, and the `pending` status it posts carries no link (only the job's final status does); `run` ships and prints.

## Phase 12: User Story 8 - A developer runs the same pipeline locally (P2)

Implemented by T020 and T037 (`run` shares `run_pipeline`).

### Tests for User Story 8

- [X] T038 [P] [US8] Tests `tests/cli.rs` additions (FR-025; AS 8.1 to 8.3): with a stub `docker` first on `PATH`: a valid pipeline exits 0 only if every step succeeded, a failing step exits non-zero; a step that lists a missing secret exits non-zero before the stub records any call and names the secret; a step name runs only that step and its dependencies; no token and no network access needed.
- [X] T039 [P] [US8] Tests additions in `tests/executor.rs` (real Docker, CI only; AS 8.4): local `run` of a `docker = true` pipeline builds and runs an image without any opt-in and leaves no container, network or volume.

## Phase 13: User Story 10 - Releases are tagged and pulled by digest (P3)

Tests for AS 10.1 and 10.2 are in T026.


### Implementation for User Story 10

- [X] T040 [US10] Document in `README.md` and keep the release workflow summary text (brief S11): which tag builds, where the digest is printed, the `RUNNER_IMAGE_DIGEST` variable, and that a tick fails before any repo when it is unset (FR-026).

## Phase 13b: User Story 12 - Pipeline secrets come from Bitwarden Secrets Manager (P2)

Spec: story 12, FR-038 to FR-046, FR-052, FR-060, A19 to A24. Design: `research.md` D9 as amended by D12. Runs before Phase 14. The names a job fetches come from its validated pipeline, not the watch entry. The runner binary changes only here: `id` and `secret_branches` in the watch entry, the `job` prepare mode (`--names-file`, `--reason-file`), the image-pin check, removal of `--secrets`, and the reserved prefix `BWS_`. The scripts are written once, in T050 and T062.

### Tests for User Story 12

- [X] T047 [P] [US12] Tests `.github/scripts/test-factory-ci-secrets.sh` (shell; stub `bws`, stub runner and stub `plan` first on `PATH`; real `jq`; `FACTORY_CI_FETCH_WINDOW=2`; `FCI_KEY_SEP` set explicitly; blobs made with a test key by `factory-ci-blob.sh`): job wrapper (AS 12.1 to 12.9, 12.11) reads the row only from the blob, never from a plaintext `ITEM_*` variable; only names from the names file are fetched; in one project, key `owner/name<SEP>NAME` is used only when the row's `repo_id` is non-empty, key `NAME` only when the row's `secrets` lists it, the prefixed key wins when both exist, the docker variant reads only the docker project; a key under another repo's prefix and an unlisted shared key are never used and the item fails "secret NAME not found" without starting the real run; `BWS_ACCESS_TOKEN` unset and a pipeline that lists a secret gives "no secret store configured" while a pipeline with no secret runs; `secrets_ok` false and a non-empty names file gives "secrets not allowed on this branch" with no store call; an empty names file makes no store call; log credentials come from the three project keys (none gives off, one or two gives "log credentials incomplete"); a duplicate key gives "ambiguous"; an empty value counts as no match; exactly one list call per job; a 429-style failure is retried then "secret store unreachable"; 401 or 403 text is not retried and gives "secret store rejected the token"; a multi-line value arrives intact; `::add-mask::` lines for each line of 4 or more characters come first and no value appears elsewhere in stdout, stderr or any `bws` argv; the prepare run starts under `env -i`; the real run's environment never holds the store token or an unlisted name (I18); an invalid pipeline copies the reason and makes no `bws` call; key building is checked with `/` and with `:`, and `__` is refused. Plan wrapper (AS 12.10, FR-045): a candidate file with no rows makes no `bws` call and phase C still runs; with rows, a rejected token or an unreachable store exits non-zero with the reason in the step summary file and phase C never starts; a malformed list fails before any store call; a healthy store passes the log credentials into phase C.
- [X] T048 [P] [US12] Tests `src/secret_names_tests.rs` (FR-040, FR-041, FR-052, FR-060; AS 12.2, 12.11): watch entry `id` parses and must be a number; `secret_branches` parses, defaults to the repo's default branch, follows the glob rules of `branches`, and a branch outside it gives `secrets_ok` false; `bitwarden_project` is an unknown key (problem line); `BWS_ACCESS_TOKEN` in `secrets` is a problem; rows carry `repo_id` only when the API id matches and `secrets` as the entry's shared names; phase A writes no status and makes no store call.

### Implementation for User Story 12

- [X] T049 [US12] Impl (brief S13) citing `data-model.md WatchEntry.id`, `WatchEntry.secret_branches`, `Item.secrets_ok`, `Item.repo_id`, `Secret names and sources`: `src/watch.rs` (`id`, `secret_branches`), `src/plan.rs` row fields `secrets_ok` and `repo_id`, `src/job.rs` prepare mode (`--names-file`, `--reason-file`), the image-pin check for `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE` (FR-057, tested by T055), removal of `--secrets`, and the flags in `src/main.rs`. Runs after T028, T032 and T037 and before T060 and T061 (shared files `job.rs`, `plan.rs`, `main.rs`).
- [X] T050 [US12] Impl the scripts once (brief S13; research.md D9 and D12): `.github/scripts/factory-ci-secrets.sh` (library), `factory-ci-job.sh`, `factory-ci-plan.sh`, covering the blob decrypt through `factory-ci-blob.sh` (T062), the names-file read loop with the name regex recheck and no `eval`, exact-key `jq --arg`, `bws` JSON through stdin or a 0600 file deleted before `exec`, `set +x`, base64 masks and the short-line refusal, the `DOCKER-USER` block for `169.254.169.254` and `168.63.129.16`, `--quiet-logs` only after the log credentials resolved, `--private` for `fail_closed`, `--retry`, the docker variant, the two-phase plan flow with no store call on zero rows, the musl asset and the `unzip -Z1` check. Depends on T062, T047 and T058.
- [X] T051 [US12] Update `.github/workflows/factory-ci-tick.yml` (brief S13; after T022, before T063): `bws` install step with the pinned checksum, gated by `env: HAVE_BWS` and a test in `run:` (a step `if:` cannot read `secrets`), plus the extra `env:` of `contracts/tick-workflow.md`.
- [X] T052 [P] [US12] Tests `tests/no_vendor.rs` (FR-038, FR-046; I19): reads `src/*.rs` under `CARGO_MANIFEST_DIR` and fails if a file other than `pipeline.rs` (the reserved `BWS_` prefix) or a `*_tests.rs` file mentions `bws` or `bitwarden` (case-insensitive); and `run` completes with no `BWS_*` or `FACTORY_CI_BWS_*` variable set (stub `docker` on `PATH`).
- [X] T053 [P] [US12] Docs: README section and `quickstart.md` check 8 (brief S13): free-plan limits (2 projects, 2 accounts used), key layout with `<SEP>`, machine account scope, no GitHub secret fallback, rollback.
- [ ] T054 [US12] Moved to 003 T033, blocked on the owner's Setup. Live check (blocked on T045): quickstart check 8 against a real Secrets Manager project: a key with `/` is accepted (else set `FCI_KEY_SEP` to `:`), a revoked token (confirms the 401 or 403 stderr text the script matches), the `--server-url` or `bws config server-base` form, and the call count of an idle tick (zero). Then paste the real `BWS_SHA256` and run the lint with `FACTORY_CI_LINT_STRICT=1`.
- [X] T055 [P] [US12] Tests `tests/job_prepare.rs` (FR-039, FR-041; AS 12.1, US1 AS 1.5) with a stub engine whose `clone` step copies a fixture checkout: names file holds the sorted, deduplicated union of every step's `secrets`; two steps listing the same name give one line; a pipeline with no secrets gives an empty file and exit 0; invalid TOML, cycle, symlinked file, 101 steps, `docker = true` without `--allow-docker`, a name that is not a variable name (`1BAD`, `A-B`) and a reserved name (`BWS_ACCESS_TOKEN`, `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`) each exit 1 with one reason line, no names file, and the stub records no container start; prepare prints nothing to stdout or stderr and ships nothing; `FACTORY_CI_CLONE_IMAGE` unset or without `@sha256:` and 64 hex exits 1 before cloning with "image is not pinned" (FR-057); the clone token travels in the environment only.

## Phase 13c: Hardening from the review (D1 to D12, H1 to H3, M1 to M7, L1 to L5)

Spec: FR-047 to FR-060, A25, A26. Design: `research.md` D10 and D12. Runs before Phase 14. Replace the story label with the spec's own if a clarify pass renumbers.

### Tests for Hardening

- [X] T056 [P] [US6] Tests `src/plan_hardening_tests.rs` (FR-049, FR-050, FR-052, FR-055, FR-058, FR-059; I3, I20, I26, I28): a status by another creator is ignored and a stranger's `success` leaves the project `Run`; `GET /user` is called once per token; an entry `id` that differs from the API id skips the repo and names both ids; no `id` leaves `repo_id` empty; the default branch comes from `GET /repos`; phase A makes no write and no store call; B public, repo private and no log store drops the row in phase C with a problem, while a store plans it with `fail_closed` true; writes are spaced 750 ms (injectable clock) and capped at 60 per tick in the order pending, unchanged, closing errors, with the rest planned again next tick; a secondary-limit 403 is `RateLimited`; repos interleave round-robin under the cap; rows split into `matrix` and `matrix_docker` by `allow_docker`, calls for `allow_docker` repos use the docker token, and `n` is unique across both; every summary line is `item N: owner/name` with no branch, SHA or project path; `fail_tick` is true only when there are problems and `--now` is in minutes 0 to 9 of a UTC hour; the Loki heartbeat line is sent once from phase C when the log credentials are present and a send failure is ignored.
- [X] T057 [P] [US12] Tests `src/text_tests.rs` (FR-051, FR-053; I21, I22, M2, M3, M6): sanitizer keeps only `[A-Za-z0-9 _.:/-]`, truncates, prefixes a leading `::`, handles a reason `::error::x`, unicode and newlines; reserved names matched case-insensitively including `bws_x`, `Github_Token`, `item_repo`, `all_secrets`, `NODE_OPTIONS`, `bash_env`; repo names `../x`, `a/b/c`, `*`, `[a-z]`, `a b` rejected and `a/b` accepted; the redactor covers the base64 of a value and of `x-access-token:<token>`; (the short-line refusal is the shell `fci_short_line`, tested in T058; there is no Rust helper, R11); key collision (A24, I25): for random (repo, NAME) pairs the keys built with `/` and with `:` never coincide, and `__` is not an accepted separator.
- [X] T058 [P] [US12] Tests `.github/scripts/test-factory-ci-hardening.sh` (shell; stubs for `bws`, `sudo`, `iptables`, `openssl` real, `jq` real; FR-054, FR-059, M3, M5, M6, M7, H1, H3, L2): the blob round trip with a test key returns the row, a wrong key fails: the wrapper writes the reason file and exits non-zero, and no status is posted (the item stays `pending` until the 2 h stale rule closes it, R15), and the plan wrapper's `$GITHUB_OUTPUT` holds only `n`, ciphertext, counts and `fail_tick` (no repo, SHA, branch, project path or secret name appears in it); hostile names (`$(id)`, `a;b`, a newline, a leading digit) in the names file are rejected without evaluation; `bws` JSON never appears in argv, leftover files under `$RUNNER_TEMP` are removed before `exec`, and `bash -x` of the wrapper prints no value; a value with a short line fails with the name only; `::add-mask::` lines include the base64 of each value; `sudo iptables -I DOCKER-USER -d 169.254.169.254 -j DROP` and the same for `168.63.129.16` are called before the real run; `--quiet-logs` is passed iff the log credentials resolved, `--private` iff `fail_closed`, `--retry` iff `retry`; the docker variant never reads the non-docker store token; reasons are sanitized, never echoed to stdout, and escaped in the step summary; the status script decrypts and calls the stub runner's `status` with the row's fields; a placeholder `BWS_SHA256` is tolerated except under `FACTORY_CI_LINT_STRICT=1`, and a zip that does not list exactly `bws` is refused.

### Implementation for Hardening

- [X] T059 [US5] Extend `.github/scripts/factory-ci-lint-workflow.sh` (after T026; brief S14) with the rules at the end of `contracts/tick-workflow.md` (ubuntu-24.04 everywhere, only `matrix.n` and `matrix.blob`, no `ITEM_*` other than `ITEM_BLOB` in an `env:`, no `toJSON(secrets)`, the `main` guard and environment on every job but `verdict`, the `verdict` rules, the `run-docker` name rules) and a mutated copy per rule. It also asserts the `verdict` ping step is a single `curl -fsS --max-time 10` call with output discarded and failure ignored, preceded by no `set -x` and followed by no echo of the URL (FR-056).
- [X] T060 [US6] Impl plan side (brief S14) citing `data-model.md WatchEntry.id`, `Item.repo_id`, `Item.fail_closed`, `Item.retry`, `Item.n`, `Limits and text rules added by the review`: `src/github.rs` (`/user`, `/repos/{r}`, per-token choice, secondary-limit 403, 750 ms pacing, paged status, truncated-tree fallback), `src/walk.rs` (creator filter, retry marker), `src/projects.rs` fallback use, `src/plan.rs` (phase A candidates, phase C refusals, interleaved order, 60-write cap with carry-over, two matrices, `n`, `fail_tick`, Loki heartbeat line), `src/watch.rs`. Depends on T016, T028 and T049.
- [X] T061 [US12] Impl runner side (brief S14): new `src/text.rs` sanitizer; `src/pipeline.rs` reserved names (`ITEM_`, `ALL_SECRETS`, `BASH_`, `NODE_OPTIONS`); `src/redact.rs` base64 forms; `src/engine.rs` `--cap-drop NET_RAW --cap-drop MKNOD --cap-drop SETFCAP`; `src/log.rs` (no `has_sink`: removed, R11); `src/ship.rs` heartbeat sender. Runs after T049. It does not touch `job.rs`, `plan.rs` or `main.rs`.
- [X] T062 [US12] Impl blob and status scripts (brief S14): `.github/scripts/factory-ci-blob.sh` (encrypt and decrypt a row with `openssl enc -aes-256-cbc -pbkdf2 -salt -a -A -pass env:FACTORY_CI_BLOB_KEY`, no plaintext on stdout) and `.github/scripts/factory-ci-status.sh` (decrypt, call `rust-ci-runner status`). Runs before T050 and after T058.
- [X] T063 [US5] Complete `.github/workflows/factory-ci-tick.yml` and create `.github/workflows/factory-ci-mirror-images.yml` (brief S14; after T051 and T059): `run` and `run-docker` with `{n, blob}` matrices, `name: item ${{ matrix.n }}`, the `verdict` job (always, ping, `fail_tick`), `HAVE_BWS`, `B_PRIVATE`, the image variables; the mirror workflow copies `alpine/git` and `docker:27.5.1-dind` to GHCR with `docker buildx imagetools create` and prints digests.
- [X] T064 [P] Docs (brief S14): README (B is a separate public copy and ticks every 5 minutes, a private B is unsupported (A25); private repos need log shipping; Grafana 14-day link expiry; Loki holds unmasked encoded output; the ping is the dead-man signal and the Loki line only marks ticks with work; residual risks; Environments; rotation) and a pointer to `quickstart.md`.
- [ ] T065 Moved to 003 T034, blocked on the owner's Setup. Live checks (blocked on T045): quickstart checks 13 and 14 and the unverified items: grep a real signed-out public run log for a branch, a SHA, a project path and a secret name (none may appear); a step container cannot reach `169.254.169.254` (FR-054); Environments with `workflow_dispatch` on a non-main ref and the `verdict` job there; `GET /user` with a fine-grained token; the ping alert; a rootless dind trial and a `--cap-drop ALL` trial (recorded, neither adopted unless they work); record results in `research.md` D12.

## Phase 13d: Review fixes (round 1, 2026-11-27)

Findings R1 to R16 of the reviewer, security and QA round; decisions in `research.md` D12 (Review round 1) and the spec Clarifications. Brief S15 covers every task here. Each task names its file(s), the test that proves it, and the finding id. Edit order for shared files: `plan.rs` T066, T067; `job.rs` T069, T075; `log.rs` T072, T076; `workflow` T071, T073, T070.

- [X] T066 [US5] R1 (blocker): in `src/plan.rs` the closing `error` for an old marked `pending` has the description "job did not finish (retry)", so `walk.rs` sees the marker and never replans it. Proof: the failing assertions QA added in `src/plan_race_tests.rs`, plus a new plan-level test there that replays the Close post and expects `Done` on the next tick.
- [X] T067 [US4] R2: in `src/plan.rs`, when several branches share a SHA the row's branch is the default branch if it is among them, else the alphabetically first, and `secrets_ok` is true if any of those branches matches `secret_branches` (default: the default branch). Proof: new tests in `src/plan_tests.rs` (default branch present, absent, secret branch among several).
- [X] T068 [US6] R3 (FR-047): in `src/main.rs` an entry with `allow_docker` when `FACTORY_CI_TOKEN_DOCKER` is unset gets a problem line for that repo and no row; no fallback to the plain token. Proof: new test in `tests/cli.rs` (stub API or `--check`-style run) asserting the problem line and no row for the docker repo while other repos are planned.
- [X] T069 [US1] R4 (security M1): in `src/job.rs` a failed final status prints "final status not delivered" plus the HTTP code and never the request path. Proof: new test in `src/job_tests.rs` asserting stderr holds no repo name and no SHA after a 500 and a 403 from the fake status server.
- [X] T070 [US12] R5 (security M2): separate blob key for docker rows: `FACTORY_CI_BLOB_KEY_DOCKER` in `factory-ci-docker` and `factory-ci-plan`; `.github/scripts/factory-ci-plan.sh` picks the key by `allow_docker`; `.github/workflows/factory-ci-tick.yml` maps it into `run-docker` only and the plain key into `run` only; lint rule plus a mutated copy in `.github/scripts/factory-ci-lint-workflow.sh`; setup step in `quickstart.md` and README. Proof: new cases in `.github/scripts/test-factory-ci-hardening.sh` (a docker row decrypts only with the docker key, a plain row only with the plain key) and the lint mutation.
- [X] T071 [US12] R6 (security L3): `.github/scripts/factory-ci-blob.sh` uses `-iter 600000`; the tick guard step in `.github/workflows/factory-ci-tick.yml` and the plan wrapper refuse a blob key shorter than 32 characters. Proof: new cases in `.github/scripts/test-factory-ci-hardening.sh` (a 31-character key is refused, a 32-character key round-trips) and a lint rule for the guard.
- [X] T072 [US11] R7 (security L1): in `src/log.rs` `Log::write` replaces `\r` and other control characters except tab before printing or shipping. Proof: new tests in the `log.rs` test module (a line with `\r`, `\x1b`, `\0` and tab).
- [X] T073 [US11] R8 (security L5): `src/ship.rs` refuses a non-`https` Loki URL with "log URL must be https", except plain `http` to `127.0.0.1`, `localhost` or `[::1]` (test fake; accepted low risk L-b); `.github/workflows/factory-ci-tick.yml` drops `packages: read`. Proof: new cases in `src/ship_tests.rs` (`http://example.com` and `ftp://` refused, `https://` and `http://127.0.0.1`, `http://localhost`, `http://[::1]` accepted) and a lint rule that no job has `packages: read`.
- [X] T074 [US6] R9: in `src/github.rs` `branches()` returns an error when the list is still full after 5 pages (more than 500 branches), surfaced as a repo problem. Proof: new test in `src/github_tests.rs` (500 branches pass, 501 fail) and one in `src/plan_error_tests.rs` (the repo is a problem, others continue).
- [X] T075 [US1] R10: a failure to spawn the container engine is `error`, not `failure`, in `src/engine.rs` and `src/job.rs`. Proof: new test in `src/job_tests.rs` with an engine path that does not exist (state `error`).
- [X] T076 R11: remove dead code `Log::has_sink` and `Log::redact` (no production caller) from `src/log.rs`, and `text::value_ok` with its test cases from `src/text.rs` and `src/text_tests.rs`; the shell `fci_short_line` is the real rule. Proof: grep finds no use, `cargo clippy -- -D warnings` stays green, T058 still covers the short-line rule.
- [X] T077 R12: `.github/workflows/factory-ci-selftest.yml` gets a `shell` job on `ubuntu-24.04` running `.github/scripts/test-factory-ci-secrets.sh`, `.github/scripts/test-factory-ci-hardening.sh` and the lint `--self-test`; remove the "TEMP" upload-artifact step. Proof: the `shell` job passes in CI (T045 gate); a lint check that the self-test workflow has the `shell` job.
- [X] T078 R13: `README.md` states `RUNNER_IMAGE_DIGEST` as `sha256:<64 hex>` (no `@`), matching the guard. Proof: a case in `.github/scripts/test-factory-ci-hardening.sh` that fails if the README line for `RUNNER_IMAGE_DIGEST` contains `@`.
- [X] T079 R14: remove the three `#[ignore = "test-first"]` markers in `rust-ci-runner/tests/executor.rs` (T031 twice, T039) so CI runs them; in `src/walk.rs` replace `map_or(true, ..)` with `is_none_or(..)`; drop the stale "test-first wave" comment in `src/lib.rs`. Proof: `cargo test` lists the three tests as run, `cargo clippy -- -D warnings` has no `unnecessary_map_or`.
- [X] T080 R15: docs only: `plan.md` (done in this round) and the T058 expectation say that a bad blob key leaves the item `pending` until the 2 h stale rule closes it; T017's "branch deleted mid-tick costs nothing" claim is removed (covered by the tree-404 test). Proof: grep of `plan.md` and `tasks.md`; no code.
- [X] T081 Done by 003 T004 (`../003-infrastructure-home/tasks.md`): lock committed, `--locked` on every cargo call. R16 (security M3): blocked with T045: at the first compile generate and commit `rust-ci-runner/Cargo.lock`, switch `rust-ci-runner/Makefile` and `.github/workflows/rust-ci-runner-release.yml` to `--locked`, and run `cargo fmt` and commit. Proof: `make lint test` and the release build pass with `--locked`. Extended (round 2): add `rust-version = "1.82"` to `rust-ci-runner/Cargo.toml`. Get `Cargo.lock` without a local build with `docker run --rm -v "$PWD/rust-ci-runner":/w -w /w rust:1 cargo generate-lockfile` on any machine with Docker (the fallback is a scratch branch with an `upload-artifact` step that uploads the file). Run `cargo fmt` there and commit; QA notes the `fake.rs` `branches` line is over 100 characters. Proof: `make lint test` and the release build pass with `--locked`.

## Phase 13e: Review fixes (round 2, 2026-11-27)

Security, reviewer and QA passed round 2; QA's verdict is READY pending T045 and T081. These are the cleanup findings. Brief S16 covers every task here. T073 was amended in the same round: the https rule keeps its loopback exception (FR-028, `research.md` D12, accepted low risk L-b).

- [x] T082 [US6] R2-1 (reviewer): in `src/main.rs` (around lines 111 and 124) `map_or(&api, ..)` lets the plain token stand in for the Docker client. Phase C treats a Docker row with no Docker client as a problem line and emits no row (FR-047). Proof: new test in `tests/cli.rs` (or `src/plan_hardening_tests.rs`) where a docker entry has no docker client: a problem line, no row for that repo, and a plain-repo row is still planned.
- [x] T083 [US1] R2-2 (reviewer): `src/engine.rs` (line 93) repeats the text "cannot run "; use the `SPAWN_FAIL` constant in both places. Proof: grep finds the literal only in the constant's definition; the T075 test in `src/job_tests.rs` still passes.
- [x] T084 [US12] R2-3 (reviewer): `README.md` (lines 42 to 45) shows `FACTORY_CI_BLOB_KEY_DOCKER` as required. Mark it optional: needed only when `allow_docker` rows exist, and at least 32 characters when set. Proof: a case in `.github/scripts/test-factory-ci-hardening.sh` that fails if the README line for `FACTORY_CI_BLOB_KEY_DOCKER` lacks the word "optional".
- [x] T085 [US6] R2-4 (QA): strengthen the T068 test in `tests/cli.rs` (around line 160). Its fake answers `{}` to `/user`, which stops all planning, so the "no row" and "no request to o/d" checks are vacuous. Use a watch list with only the `allow_docker` entry and assert zero requests to that repo, or make `/user` answer correctly and assert the other repo is still planned. Proof: the test fails when the docker fallback is reintroduced (mutation check in the task notes). Round 3 note: this test guards the phase A partition (which entries go to the Docker client); the `Apis::same` branch cannot be observed in phase A, and the T082 test covers phase C. Mutation check: putting the plain-token fallback back in the partition makes the test fail.
- [x] T086 [US5] R2-5 (reviewer): remove the duplicate lint self-test line in `.github/workflows/factory-ci-selftest.yml` (line 75). Proof: the `shell` job (T077) runs the lint self-test exactly once; `bash -n` and a grep for a single occurrence.

## Phase 13f: Review fixes (round 3, 2026-11-27)

S16 passed the reviewer; QA says pending T045 and T081. Minor findings only. Brief S17 covers T087.

- [X] T087 [US6] R3-1 (reviewer, known limit A27): add a test proving that phase C, with no Docker token, drops Docker entries from `unchanged` and `invalid` (nothing is posted for `o/d`) and still posts the plain repo's entries, in `tests/cli.rs` (or `src/plan_hardening_tests.rs`); add a one-line code comment at the `invalid.retain` in `src/main.rs` (around line 145) noting that a stale `pending` on a Docker repo stays until a tick runs with the Docker token (A27). Proof: the new test, with a mutation check (dropping the Docker filter makes it post for `o/d` and fail). Cosmetic, accepted without a task: the "FACTORY_CI_TOKEN_DOCKER is not set" problem line appears from both phases.

## Phase 14: Polish and gates

- [X] T041 [P] Update `.github/workflows/factory-ci-selftest.yml` (brief S11): add the `list` job (build the runner, `plan --repos .github/factory-ci-repos.toml --check`), the `workflow` job (run T026's script on the tick and release workflows), widen `paths` to `.github/factory-ci-repos.toml` and `.github/scripts/**`.
- [X] T042 [P] Create `.github/factory-ci-repos.toml` with the comment block of `contracts/watch-list.md` and no entries (brief S11).
- [X] T043 [P] Rewrite `README.md` (brief S11; T064 adds the review notes): the tick, setup steps of `quickstart.md`, list file, secret sources and the Bitwarden key layout, `job` and `run`, log shipping, link template, residual risks of `research.md` D2 and D8 (public repo names, docker opt-in, `error` is final, Docker Hub limits), the 13-repo budget note, local use.
- [X] T044 Dead-code and lint read-through (brief S12): after every task except T046, T054 and T065 (which need T045): grep every removed name, unused imports, private items without callers, `#[allow]` leftovers; confirm `lib.rs` lists exactly the modules that exist.
- [X] T045 Done by 003 T005 (`../003-infrastructure-home/tasks.md`): the first compile and a green selftest. CI gate (blocked while Actions are disabled): enable Actions on a scratch basis or use a larger machine, run `make lint test`, then the self-test; fix compile and test failures before anything else ships (brief S12). Extended (R16): at the first compile also generate and commit `rust-ci-runner/Cargo.lock`, switch the `Makefile` and `rust-ci-runner-release.yml` builds to `--locked`, and run `cargo fmt` and commit the result (about 300 lines exceed 100 columns; rustfmt is not on the dev VM). Stays blocked with T045.
- [ ] T046 Moved to 003 T032 (`../003-infrastructure-home/tasks.md`), blocked on the owner's Setup. Live checks (blocked on T045): walk `quickstart.md` checks 1 to 14 against a scratch repo; record results in the PR.

---

## Traceability

Every FR and every acceptance scenario has at least one test task. "existing" means a test already in the repo that stays.

| Requirement | Tests |
|---|---|
| FR-001, FR-003 | T026 |
| FR-002 | T026 (permissions, no token in `run:`), T030 (no token reaches a step), T046 check 7 |
| FR-004, FR-005 | T010, T012 |
| FR-006 | T023, T004 |
| FR-007 | T017 |
| FR-008 | T013, T017 |
| FR-009 | T015, T025 (incl. the old-error retry) |
| FR-010, FR-011 | T015, T017 |
| FR-012 | T017, T023, T025 |
| FR-013, FR-014 | T004, T027 |
| FR-015 | T026, T018 |
| FR-016 | T018, T030 |
| FR-017, FR-022 | T018, T055 |
| FR-018 | T029, T030, T047 |
| FR-038 | T052 |
| FR-039 | T047, T055 |
| FR-040 | T047, T048 |
| FR-041 | T029, T030, T047, T048, T055 |
| FR-042 | T005, T047 |
| FR-043, FR-044 | T047 |
| FR-045 | T047 (plan wrapper cases), T026 |
| FR-046 | T038, T052 |
| FR-047 | T056, T058, T059 |
| FR-048 | T059 |
| FR-049 | T056, T058, T035 |
| FR-050 | T056 |
| FR-051 | T057, T058 |
| FR-052 | T056, T048 |
| FR-053 | T057, T058 |
| FR-054 | T030, T058, T065 |
| FR-055 | T023, T056 |
| FR-056 | T056, T059, T065 |
| FR-058 | T056, T059 |
| FR-059 | T034, T056, T058, T059, T065 |
| FR-060 | T047, T048 |
| FR-057 | T055, T058 |
| FR-019 | T029, existing `pipeline::tests`, T018 |
| FR-020, FR-023 | existing `tests/executor.rs`, T031 |
| FR-021 | T030, existing dind test |
| FR-024 | T005, T018, T034 |
| FR-025 | T038 |
| FR-026 | T026 |
| FR-027 | by construction: T001 removes the other hosts; T004 pins GitHub-only calls |
| FR-028, FR-029 | T033, T012 |
| FR-030 | T034, T035 |
| FR-031 | T034, T018 |
| FR-032 | T005, T034 |
| FR-033, FR-034 | T033 |
| FR-035 | T004, T034 |
| FR-036 | T005, T033 |
| FR-037 | T035, T038 |

| Scenario | Tests | Scenario | Tests |
|---|---|---|---|
| US1.1 | T017, T018 | US7.1 to 7.3 | existing `executor.rs`, `pipeline::tests` |
| US1.2, 1.3, 1.5 | T018 | US7.4 | T030, T031 |
| US1.4 | T017, T025 | US7.5 | existing dind test, T030 |
| US2.1 to 2.4 | T015 | US7.6 | T030 |
| US3.1, 3.4 | T013, T017 | US7.7 | T029, T030 |
| US3.2 | T017 | US8.1 to 8.3 | T038 |
| US3.3 | T030 | US8.4 | existing dind test, T039 |
| US4.1 to 4.3 | T023 | US9.1 | T010, T017 |
| US5.1, 5.4 | T026, T025 | US9.2 | T010, T030 |
| US5.2, 5.3 | T025 | US9.3 | T010, T012 |
| US6.1, 6.4 | T027 | US9.4 | T023 |
| US6.2 | T026, T027 | US10.1, 10.2 | T026 |
| US6.3 | T018 | US11.1 to 11.3 | T034, T035 |
| US11.4, 11.5, 11.7 | T033 | US11.6 | T034 |
| US11.8, 11.9 | T035, T038 | | |

Story 12: AS 12.1 T047, T055; 12.2 T047, T048; 12.3 T047; 12.4 T047, T005; 12.5 to 12.8 T047 (12.6 also T048); 12.9 T047; 12.10 T047 (plan wrapper), T048; 12.11 T047, T048. Pipeline rejection of a bad name: T029, T055.

Success criteria: SC-005 T017; SC-006 T005, T034 (I7); SC-007 T031; SC-009 T034; SC-011 T033, T034; SC-001 to SC-004, SC-008, SC-010 are live checks (T046) backed by the tests above.

Review round 1 (finding to task): R1 T066; R2 T067; R3 T068; R4 T069; R5 T070; R6 T071; R7 T072; R8 T073; R9 T074; R10 T075; R11 T076; R12 T077; R13 T078; R14 T079; R15 T080; R16 T081 (blocked with T045). Requirements touched: FR-017 (T075), FR-024 (T069), FR-028 (T073), FR-036 (T072), FR-047 (T068, T070), FR-059 (T070, T071), FR-060 (T067), A6c (T066), A20 (T070, T071).
Review round 2 (finding to task): R2-1 T082; R2-2 T083; R2-3 T084; R2-4 T085; R2-5 T086; the https loopback exception is T073 (amended) and T081 is extended (rust-version, lockfile, cargo fmt). Requirements touched: FR-028 (T073), FR-047 (T082, T085).

Review round 3: R3-1 T087 (FR-047, A27).


## Task count per story

| Group | Tasks | Count |
|---|---|---|
| Setup | T001, T002 | 2 |
| Foundational | T003 to T009 | 7 |
| US9 | T010 to T012 | 3 |
| US3 | T013, T014 | 2 |
| US2 | T015, T016 | 2 |
| US1 | T017 to T022 | 6 |
| US4 | T023, T024 | 2 |
| US5 | T025, T026 | 2 |
| US6 | T027, T028 | 2 |
| US7 | T029 to T032 | 4 |
| US11 | T033 to T037 | 5 |
| US12 | T047 to T055 | 9 |
| US8 | T038, T039 | 2 |
| US10 | T040 | 1 |
| Hardening (review) | T056 to T065 | 10 |
| Review fixes round 1 | T066 to T081 | 16 |
| Review fixes round 2 | T082 to T086 | 5 |
| Review fixes round 3 | T087 | 1 |
| Polish and gates | T041 to T046 | 6 |
| **Total** | | **87** |

## Dependencies & Execution Order

```
T001, T002 -> test wave (QA, parallel)
T006 -> T007 -> T009
T005 -> T008
T007 -> T011, T014, T016 (T016 also needs T014, T009)
plan.rs order: T019 -> T024 -> T028 -> T060 (T019 needs T014, T016, T011, T009; T060 also needs T016, T049)
job.rs order: T020 -> T032 -> T037 -> T049 (T020 needs T008, T019)
main.rs order: T021 -> T037 -> T049 (T021 needs T020)
T022 needs T021 only (workflow skeleton, no script, no T026)
T036 needs T008, T007; T037 needs T036, T020, T021, T019, T028
T040, T041, T042, T043 need T021, T022
T047, T048, T052, T055, T056, T057, T058 need only the contracts
T062 needs T058; T050 needs T062, T047, T058; T051 needs T022, T050; T053 needs T051
T049 needs T011, T019, T020, T021, T028, T032, T037; T061 needs T049 (it touches other files)
T059 needs T026; T063 needs T022, T051, T059, T062; T064 needs T063
T044 needs every task except T046, T054, T065 and T081; T045 needs T044
T046, T054 and T065 need T045
T087 needs T082; T085 -> T082 (test first); T083 needs T075; T084 and T086 need nothing; T081 needs T045
T066 -> T067 (plan.rs); T069 -> T075 (job.rs); T072 -> T076 (log.rs); T071, T073, T070 edit the tick workflow in that order after T063; T070 and T071 need T062; T074 needs T060; T081 needs T045
T066 to T080 need no gate except the ones above; T044 excludes T081 as well
```

## Parallel Example

```text
Wave 1 (QA, after T001 and T002, all different files):
  T003 util tests            T004 github tests         T005 log and redact tests
  T010 watch tests           T012 cli plan --check     T013 projects tests
  T015 walk tests            T017 plan tests           T018 job tests
  T023 cap tests             T025 race tests           T026 lint script
  T027 error tests           T029 pipeline tests       T030 job rules tests
  T031 executor additions    T033 ship tests           T034 ship job tests
  T035 cli logs tests        T038 cli run tests        T039 executor docker test
  T047 secrets script tests  T048 bws tests            T052 no_vendor test
  T055 job prepare tests     T056 plan hardening       T057 text tests
  T058 hardening script tests
Wave 2 (engineer): T006 then T007 then T009, with T008 beside them
Wave 3 (engineer): T011, T014, T016 together; then T019, T024, T028 in order
Wave 4 (engineer): T020, T021, T032, T036, T037 in order; T022 and T049 to T051 after
Wave 5: T060, T061, T062, T063; then Polish
```

## Implementation Strategy

- **MVP first**: Phases 1 and 2, then US9, US3, US2 and US1 with shipping off and no secrets. Stop and check
  with a scratch repo (quickstart checks 1 to 3).
- **Incremental delivery**: add US4 to US7, then US11 (logs), then US12 (Bitwarden), then the hardening phase. Nothing
  replaces the old daemon until every P1 story passes.
- **Gate**: T045 (first CI run) comes before any live check. The crate has never compiled.
- **Per brief**: run `/speckit-implement` for `specs/002-ci-runner/` with only the brief's task IDs.

## Notes

- `[P]` tasks touch different files and no open task of their group.
- Tests are written first and fail until the matching implementation task lands.
- Commit after each task or brief; stop at any checkpoint to validate.
- Avoid vague tasks, shared-file conflicts and cross-story dependencies that break independence, except the ones listed
  under Dependencies & Execution Order.


---

## Engineer briefs

One brief per step. Each is self-contained: read it, the files it names and the contract files it cites, nothing else. Each names the spec folder and its task IDs and is run with `/speckit-implement` for those tasks only. C1 to C6 above apply to every brief: no local cargo, docker or builds.

### S1. Prune the daemon (T001, T002)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T001, T002. Implement with `/speckit-implement`, only these tasks.

**Goal**: remove the polling daemon and every non-GitHub host so later steps start from a small coherent crate.

**Files**: delete `src/{poll,store,server,sched,config,status,app,app_tests}.rs`; edit `src/lib.rs`, `src/main.rs`, `src/engine.rs`, `src/util.rs`, `Cargo.toml`, `tests/executor.rs`, `tests/argv.rs`; create `src/job.rs` (holds `run_env` and `local` moved from `app.rs`); delete the three workflow and script files and the `payload` job of the self-test.

**Do**:

- `engine.rs`: add `pub const DIND_IMAGE: &str = "docker:27.5.1-dind";` and `pub const CLONE_IMAGE: &str = "alpine/git";`. Remove `ls_remote`, `cleanup_polls`, `POLL_LABEL`. `GIT_AUTH` stays (the clone uses it). `Engine::remove_dir` stays.
- `util.rs`: remove `is_run_id`, `write_atomic`, `TMP_COUNTER` and their tests and unused imports.
- `job.rs`: copy `run_env` and `local` from `app.rs` as they are. Replace `DIND_IMAGE` import by `crate::engine::DIND_IMAGE`.
- `main.rs`: keep `run` and `help`, drop `serve` and the `config` and `server` imports; usage text lists `run` only for now (T021 adds the rest).
- `Cargo.toml`: move `tiny_http` under `[dev-dependencies]`.
- `tests/executor.rs`: import `DIND_IMAGE` from `rust_ci_runner::engine`; delete the `poll` import and `polling_a_real_repository_...`; delete helpers only that test used.
- `tests/argv.rs`: keep the clone half of the first test and the others; delete the poll half.
- Self-test workflow: delete the `payload` job only. Leave the `pipeline` job and its TEMP step.

**Acceptance**: `grep -rn "poll\|store::\|sched::\|config::\|status::\|server::\|tiny_http" rust-ci-runner/src rust-ci-runner/tests` finds no use outside `Cargo.toml` dev-dependencies and `src/github_tests.rs` later; every `mod` in `lib.rs` has a file; `pipeline.rs`, `redact.rs`, `log.rs` are untouched.

**Constraints**: do not change behaviour of anything kept. No local cargo.

### S2. Foundation: util, GitHub client, fake (T003 tests, T004 tests, T006, T007, T009)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T003, T004, T006, T007, T009. Implement with `/speckit-implement`, only these tasks.

**Goal**: the small shared pieces every later step calls.

**Files**: `src/util.rs`, `src/github.rs`, `src/fake.rs`, `src/lib.rs` (`pub mod github;`, `#[cfg(test)] mod fake;`, test module lines).

**Contract**: `contracts/github-api.md`, `data-model.md` (Branch, Commit, StatusEntry, TreeEntry, ChangedFile), `research.md` D3 and D4.

**Do**:

- `util.rs`: `pub fn base64(bytes: &[u8]) -> String` (standard alphabet, padding), `pub fn parse_rfc3339(s: &str) -> Option<i64>` accepting `YYYY-MM-DDTHH:MM:SSZ` (days-from-civil, no crate), `pub fn short_sha(s: &str) -> &str` (8 chars).
- `github.rs`: `pub trait Api` with the six operations of the contract, returning `Result<_, ApiError>`; `ApiError { RateLimited(String), Http(u16, String), Net(String) }`; `HttpApi { base, token, remaining: AtomicI64 }` built by `HttpApi::from_env()` (`GITHUB_API_URL`, `FACTORY_CI_TOKEN`); every response updates `remaining`; reads check `remaining < RATE_RESERVE` first; 20 s timeout; retry 5xx and transport twice (1 s, 2 s; injectable for tests); paging per contract; validate each sha with `is_sha`; `post_status(repo, sha, context, state, description, url)` with 3 tries and the same classification; `pub fn compose_description(reason, delivered) -> String` (140 chars, reason shortened with `…` first, suffix `" (logs not delivered)"` kept). The token never enters an error string; build errors from the status code and a fixed phrase, not from `ureq`'s text.
- `fake.rs`: `FakeApi` holding branches, trees, statuses, commits, compare answers and a script of injected errors, plus `calls: Mutex<Vec<String>>` and read and write counters. Implements `Api`. Post calls append to the statuses so a second plan sees them.

**Acceptance**: T003 and T004 pass in CI. `rg "ureq"` outside `github.rs`, `ship.rs`: none. No `unwrap` on network data.

**Constraints**: synchronous; no new crate; keep each function under about 40 lines.

### S3. Log API, redaction, engine hooks (T005 tests, T008)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T005, T008. Implement with `/speckit-implement`, only these tasks.

**Goal**: one place where step output is redacted, split, capped, printed and queued, plus an Actions-safe event line.

**Files**: `src/log.rs`, `src/redact.rs`, `src/engine.rs`.

**Contract**: `research.md` D7, `data-model.md LogLine`, FR-030, FR-032, FR-036.

**Do**:

- `log.rs`: keep `Log::new(out, red, limit)` and `Log::line` (now `out(who, "stdout", text)`). Add `pub trait Sink: Send + Sync { fn push(&self, who: &str, stream: &str, text: &str); }`, `Log::with_sink(Arc<dyn Sink>)`, `Log::quiet(bool)`, `Log::out(who, stream, text)`, `Log::event(who, text)`. `out`: redact the whole line, split at 16 KB on char boundaries, apply the byte cap (the marker `[runner] log cut off at the size limit` is written and queued once), print `[who] text` unless quiet, push to the sink. `event`: print always and push; callers pass only `started`, `success in 12s`, `failed in 12s`, `skipped`, `timed out`. Replace NUL by U+FFFD in the sink copy.
- `redact.rs`: `new` also adds each trimmed line of 4 or more characters of a multi-line value (shorter lines are left alone, A22).
- `engine.rs`: `run_logged` pumps stdout with stream `stdout`, stderr with `stderr`, through `log.out`. In `execute`, record each step's start `Instant` and call `event(name, "started")` at start, `event(name, "success in Ns")` or `event(name, "failed in Ns")` or `event(name, "skipped")` at the end; keep the reason text on a `log.out` line. Timeouts print `event(name, "timed out")`. No other signature change: `RunCtx` fields stay.

**Acceptance**: T005 passes; existing `tests/executor.rs` and `tests/argv.rs` compile unchanged (they build `Log` with `Log::new`).

**Constraints**: a printed line is always `[who] ...` so it never starts with `::`. Do not add a field to `RunCtx`.

### S4. Watch list (T010, T012 tests, T011)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T010, T011, T012. Implement with `/speckit-implement`, only these tasks.

**Goal**: parse and validate the list completely before any work.

**Files**: `src/watch.rs`, `src/lib.rs`.

**Contract**: `contracts/watch-list.md`, `data-model.md WatchEntry`.

**Do**: `#[derive(Deserialize)] #[serde(deny_unknown_fields)]` raw structs; collect every problem in a `Vec<String>`; `pub fn parse(text: &str) -> Result<Vec<WatchEntry>, Vec<String>>`; reuse `pipeline::valid_secret_name`; `glob_match(glob, name)` with `*` and `?` only; `WatchEntry::matches_branch(&self, name)`; size, entry and secret-count limits from the data model. Unknown-key serde errors are turned into one problem line naming the key.

**Acceptance**: T010 passes; messages match `contracts/watch-list.md`.

**Constraints**: no regex crate; the glob matcher is iterative or short recursive with a step limit so `*` patterns cannot blow up.

### S5. Projects and walk (T013, T015 tests, T014, T016)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T013, T014, T015, T016. Implement with `/speckit-implement`, only these tasks.

**Goal**: find the projects at a tip and decide Done, Unchanged or Run for each.

**Files**: `src/projects.rs`, `src/walk.rs`, `src/lib.rs`.

**Contract**: `data-model.md` Project, Decision per project; `contracts/github-api.md`; `research.md` D4; spec FR-008 to FR-011 and edge cases.

**Do**:

- `projects.rs`: from the tree entries of one commit return `Vec<Found { path, valid }>`: root if `.factory-ci.toml` is a blob at the top, folders for `<folder>/.factory-ci.toml` where `<folder>` is a top-level tree. `owns(project_path, set, file)`. `folder_name_ok(name)`.
- `walk.rs`: `decide(api, repo, tip, candidates, now, cache)` returns a map context to `Decision`. Tip status check first (`Done`). For the rest: one commit-window call (`sha=tip`, 100), build the first-parent chain from `parents[0]` inside the window up to 50 steps, then fetch the combined status of ancestors in order, with a per-repo `HashMap<sha, Vec<StatusEntry>>` cache, until every candidate context found a status or the chain ends. For a `success` ancestor, fetch changed files once per ancestor (cache), collect `filename` and `previous_filename`, and mark `Unchanged` when no changed path is owned by the project. A non-rate-limit `ApiError` anywhere in the walk makes the affected projects `Run`; `RateLimited` is returned to the caller. Return the committer date of the tip with the decisions for ordering. Retry budget (A6c, FR-009), no extra call: an `error` older than 1800 s or a `pending` older than 7200 s whose description lacks " (retry)" gives `Run` with `retry` true; with the marker an old `error` is `Done` and an old `pending` is closed with an `error` post. Ignore any status whose `creator_login` differs from the token's user.

**Acceptance**: T013 and T015 pass; call counts match the cache expectations in T015.

**Constraints**: never return `Unchanged` on missing or malformed data; guard every index; no recursion over history.

### S6. Plan (T017, T023, T025, T027 tests, T019, T024, T028)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T017, T019, T023, T024, T025, T027, T028. Implement with `/speckit-implement`, only these tasks.

**Goal**: the `plan` function in two phases and its output.

**Files**: `src/plan.rs`, `src/lib.rs`.

**Contract**: `contracts/plan-output.md`, `data-model.md` Item, PlanResult; `research.md` D3, D4 and D12 (D-d 11); spec FR-006, FR-007, FR-009 to FR-014, FR-045, FR-055, FR-058.

**Do**:

- Phase A: `pub fn candidates(api: &dyn Api, list: &[WatchEntry], opts: &Opts) -> Candidates` (reads only: branches, trees, statuses, walk; no write, no store call). Per repo in list order: `branches` (error if over 100 after filtering), group by sha keeping the alphabetically first branch, `tree`, `projects::find`, `walk::decide`; invalid folder names become closing-error posts; per-repo errors drop the repo's rows; `RateLimited` stops reads and lists unchecked repos.
- Phase C: `pub fn finish(api, cand, opts) -> Final`: drop private-repo rows when B is public and no log store resolved; order rows (within a repo oldest first, repos round-robin), cut at `cap` (20), assign `n`; post `pending` (`queued`, or `queued (retry)`) one by one at 750 ms spacing, then unchanged posts, then closing errors, at most 60 writes; a failed `pending` drops its row; write the private rows file and the summary (`item N: owner/name` only); compute `fail_tick`.
- `run_id`: 16 hex characters from `random_id()[..16]`. Nothing printed to stdout except `fail_tick=0|1` from phase C.

**Acceptance**: T017, T023, T025, T027 pass; the 120-read count of T017 holds; phase A writes nothing.

**Constraints**: no network in `plan.rs` itself (everything through `Api`); no row without a successful `pending`; no branch, SHA or project path in any message. Edits `plan.rs` in the order T019, T024, T028 (then T060).

### S7. Job, CLI and pipeline rules (T018, T029, T030, T031 tests, T020, T021, T032)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T018, T020, T021, T029, T030, T031, T032. Implement with `/speckit-implement`, only these tasks.

**Goal**: the `job` (real and prepare), `status`, `run`, `plan` and `help` commands, and the docker checks.

**Files**: `src/job.rs`, `src/main.rs`, `src/pipeline.rs`, `src/lib.rs`.

**Contract**: `contracts/cli.md`, `data-model.md` JobResult; spec FR-015 to FR-025.

**Do**:

- `pipeline.rs`: drop the `fork` parameter and its tests; reserve `BWS_`, `LD_`, `GIT_`, `SSL_`, `BASH_ENV`, `ENV`, and names ending `_PROXY`; `resolve_secrets` keeps the single "not defined" message (no allowlist message; the fetch scripts decide which names a repo may read).
- `job.rs`: `run_pipeline(dir, step, engine, env, secrets, log, allow_docker) -> Outcome-like result` used by both `local` and `job`. `job(args)`: validate flags, build the redactor from the token, the log credentials and the value of every name the pipeline lists, clone with `Engine::clone_repo` into `$RUNNER_TEMP` or the temp dir, verify the project directory is a real directory inside the workspace, read and parse, docker checks, `execute`, compute `JobResult`, build the description with `github::compose_description`, post with `HttpApi::post_status`, clean up with `cleanup_run`. Wrap the run in `catch_unwind`; a panic posts `error` "internal error". Exit 0 only when the final post succeeded. `status` posts one status with the same link rule.
- `job.rs` also maps a first failed step whose reason is `exit code 125` (image cannot be pulled, engine fault) and a failed clone to `error`, keeps 126 and 127 as `failure`, supports `--retry` (every posted description ends with " (retry)"), and `--private` (stdout stays empty; step names and events go only to the log sink; no log config means exit 1 before cloning).
- `main.rs`: subcommands per `contracts/cli.md`; flags parsed by hand like the existing `run`; every failure prints a redacted one-line message to stderr and exits 1.

**Acceptance**: T018, T029, T030, T031 pass; `job` never starts a container on an invalid pipeline.

**Constraints**: the final status state is never `pending`; secrets stay in the engine client's environment, never argv; `plan`, `job`, `status` take the token only from `FACTORY_CI_TOKEN`.

### S8. Log shipping (T033, T034, T035 tests, T036, T037)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T033, T034, T035, T036, T037. Implement with `/speckit-implement`, only these tasks.

**Goal**: Loki push with retry, flush and the Actions-quiet path.

**Files**: `src/ship.rs`, `src/log.rs` (sink hookup only), `src/job.rs`, `src/main.rs`, `src/plan.rs` (config validation only; the pending status has no link), `src/lib.rs`.

**Contract**: `contracts/loki-push.md`, `data-model.md` LogConfig and LogLine, `research.md` D7; spec FR-028 to FR-037, A15 to A18.

**Do**:

- `ship.rs`: `LogConfig::from_env(get) -> Result<Option<LogConfig>, Vec<String>>`; `Shipper::start(cfg, labels{repo, project}, meta{run_id, sha, branch}) -> Arc<Shipper>` implementing `log::Sink`; one thread, a `Mutex<VecDeque>` with a byte counter capped at 32 MiB, `Condvar` wake-up; batch after 1 s or 1 MB; build the JSON with `serde_json`; `ureq` POST with a 10 s timeout and `Authorization: Basic`; retry 429, 5xx and transport errors with backoff 1, 2, 4, 8, 16 s until 60 s from the batch's first attempt; 401 and 403 set `stopped`; other 4xx drop; a `dropped` `AtomicBool`; `finish(window) -> bool` returning delivered; timings in a `Timing` struct so tests use milliseconds. The line prefix and NUL replacement are applied at ship time.
- `link(template, run_id)` and the Actions URL from `GITHUB_SERVER_URL`, `GITHUB_REPOSITORY`, `GITHUB_RUN_ID`.
- Wire: `job` starts the shipper before the clone, makes the `Log` with `with_sink`, `quiet(--quiet-logs)`, calls `finish(60 s)` before the final post, passes the result to `compose_description`. `--quiet-logs` without a complete config exits 1 before anything runs. `run` ships and prints, or ships only with `--quiet-logs`.

**Acceptance**: T033, T034, T035 pass; no step output in the job's stdout when shipping is on.

**Constraints**: shipping never changes a step or job result and never blocks a step; no print of dropped lines; URL, user and token never in any message.

### S9. Workflow skeleton and lint script (T022, T026)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T022, T026. Implement with `/speckit-implement`, only these tasks.

**Goal**: a workflow skeleton and the lint script that checks the final workflow.

**Files**: `.github/workflows/factory-ci-tick.yml` (skeleton), `.github/scripts/factory-ci-lint-workflow.sh`.

**Contract**: `contracts/tick-workflow.md`.

**Do**: write the lint script (T026), taking the workflow path as an argument, with a mutated-copy self-test per rule. Write the workflow skeleton (T022): jobs `plan`, `run`, `run-docker`, `verdict`; the concurrency group; `runs-on: ubuntu-24.04`; the `main` guards and Environments; the digest and image guard as the first plan step; calls to the wrapper scripts by name. Pin each `uses:` to a full commit SHA, copying the pins in `rust-ci-runner-release.yml` and `ci.yml` (checkout is `11d5960a326750d5838078e36cf38b85af677262`). Write no wrapper script here: T050 and T062 write them once. T051 and T063 complete the workflow.

**Acceptance**: the lint passes on a good copy and fails on each mutated copy; the skeleton has no `${{` inside a `run:` block and no `ITEM_*` in an `env:`.

**Constraints**: C1 to C7. `permissions` per job as in the contract; do not write `GITHUB_ENV`; do not wait for the final workflow (T063).

### S10. Local run parity (T038, T039 tests)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T038, T039. Implement with `/speckit-implement`, only these tasks.

Covered by S7 and S8; QA writes the tests. The engineer fixes any gap they expose in `src/job.rs::local` or `src/main.rs`.

### S11. Docs, self-test, seed (T040, T041, T042, T043)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T040, T041, T042, T043. Implement with `/speckit-implement`, only these tasks.

**Goal**: documentation and CI wiring for the new workflow.

**Files**: `README.md`, `.github/workflows/factory-ci-selftest.yml`, `.github/factory-ci-repos.toml`.

**Do**: follow T040 to T043. The self-test `list` job builds with `cargo build --release` in `rust-ci-runner/` and runs `plan --check`; the `workflow` job runs the lint script with each workflow path. README states plainly: public repo names and SHAs appear in B's Actions UI when B is public; `error` is final until a new commit; `allow_docker` gives a repo's committers B's secrets; the 13-repo note for SC-005; Docker Hub pull limits.

**Acceptance**: every claim in the README matches `plan.md`; no leftover mention of `serve`, polling, GitLab, Gitea, `FACTORY_CI_DISPATCH` or the reusable workflow.

**Constraints**: prose per `stop-slop`; no em dashes.

### S12. Read-through and CI gate (T044, T045, T046)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T044, T045, T046. Implement with `/speckit-implement`, only these tasks.

**Goal**: get the crate to a first green CI run.

**Do**: T044 is a grep pass for dead code and unused imports. T045 and T046 need Actions or a larger machine; the lead decides how. Record every compile or test failure and fix it at its source, not by loosening a test.

**Constraints**: do not weaken `-D warnings`.

### S13. Pipeline-declared secrets and the fetch (T047, T048, T052, T055 tests; T049, T050, T051, T053, T054, T062)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T047, T048, T049, T050, T051, T052, T053, T054, T055, T062. Implement with `/speckit-implement`, only these tasks.

**Goal**: a job gets exactly the secrets its pipeline lists, from Bitwarden Secrets Manager in the FR-040 order (one project, no GitHub fallback), with only plain variables at the runner; the matrix row stays encrypted; the store is checked only when there is work.

**Files**: `.github/scripts/factory-ci-secrets.sh` (library), `factory-ci-blob.sh` (library), `factory-ci-job.sh`, `factory-ci-plan.sh`, `factory-ci-status.sh`, `.github/workflows/factory-ci-tick.yml`, `.github/scripts/test-factory-ci-secrets.sh`, `src/watch.rs`, `src/plan.rs`, `src/main.rs`, `src/job.rs`, `src/secret_names_tests.rs`, `tests/job_prepare.rs`, `tests/no_vendor.rs`, `README.md`, `quickstart.md`.

**Contract**: `research.md` D9 as amended by D12, `contracts/cli.md`, `contracts/tick-workflow.md`, `contracts/plan-output.md`, `contracts/watch-list.md`, `data-model.md` WatchEntry, Item, Secret names and sources.

**Do**:

- Runner side (T049): prepare mode of `job` clones, runs every pre-container check (reuse the real-run path), writes the names file or the reason file, prints nothing, starts no container. Add the image-pin check (FR-057). Remove `--secrets`. Add `id` and `secret_branches` to `watch.rs` and the row fields `secrets_ok` and `repo_id`. Nothing vendor-related beyond the reserved prefix `BWS_`.
- Blob and status scripts (T062): `factory-ci-blob.sh` encrypts and decrypts one row with `openssl enc -aes-256-cbc -pbkdf2 -salt -a -A -pass env:FACTORY_CI_BLOB_KEY` and prints nothing else; `factory-ci-status.sh` decrypts and calls `rust-ci-runner status`.
- Library and wrappers (T050), written once: `fci_list <project>` (retry, classify, cache), `fci_lookup <name>` (the two sources, exact keys, `FCI_KEY_SEP` default `/`, `:` the fallback), `fci_mask`. Call shape: `BWS_ACCESS_TOKEN="$BWS_ACCESS_TOKEN" timeout 30 bws secret list "$project" -o json`; keep the output in a shell variable or a 0600 file, parse with `jq` from a pipe, pass only names with `--arg`, export with `export "$name=$value"`. The job wrapper decrypts the blob first, runs prepare under `env -i`, refuses `secrets_ok` false, fetches only if the names file is non-empty, resolves the log credentials, adds `--quiet-logs`, `--private`, `--retry` and `--allow-docker` as the row says, unsets the store token and `exec`s the real run. The plan wrapper runs phase A, checks the store and fetches the log credentials only when the candidate file has rows, runs phase C, encrypts the final rows and writes the outputs.
- Install step (T051): download `bws-x86_64-unknown-linux-musl-2.1.0.zip` if the release's `bws-sha256-checksums-2.1.0.txt` lists it, else the gnu zip, from the `bws-v2.1.0` release of `bitwarden/sdk-sm`; run `sha256sum -c`; compare with the constant `BWS_SHA256`; check `unzip -Z1` lists exactly `bws`. Fetch the real hash once and paste it; if you cannot, leave `REPLACE_ME` (the lint tolerates it until T054; the library must refuse it at run time, so a tick fails closed). A non-default server: `bws config server-base "$FACTORY_CI_BWS_SERVER_URL"` when set; report anything you could not confirm.
- Do not use `bitwarden/sm-action`.

**Acceptance**: T047, T048, T052, T055 pass; the lint (T026, T059) passes on the edited workflow; no GitHub secret is read; a key under another repo's prefix is never read; nothing is exported until every name resolved; an idle tick makes no store call.

**Constraints**: C1 to C7. Never `set -x`; never write `GITHUB_ENV`; never print a value, a token, a decrypted field or raw `bws` stderr. Edit order for shared files: `job.rs` T020, T032, T037, T049; `plan.rs` T019, T024, T028, T060; `main.rs` T021, T037, T049.

### S14. Hardening (T056 to T065)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T056, T057, T058, T059, T060, T061, T063, T064, T065 (T062 is in S13). Implement with `/speckit-implement`, only these tasks.

**Goal**: apply the review findings: split docker and plain jobs, main-only Environments, nothing private in the public log, fail-closed shipping on a public B, status creator filter, repo id check, sanitized text, metadata block, pinned images, write pacing and cap, the `verdict` ping.

**Files**: `src/github.rs`, `src/walk.rs`, `src/projects.rs`, `src/plan.rs`, `src/watch.rs`, `src/text.rs`, `src/pipeline.rs`, `src/redact.rs`, `src/engine.rs`, `src/log.rs`, `src/ship.rs`, `.github/workflows/factory-ci-tick.yml`, `.github/workflows/factory-ci-mirror-images.yml`, `.github/scripts/factory-ci-lint-workflow.sh`, the test files named in T056 to T058, `README.md`.

**Contract**: `research.md` D10 and D12, `contracts/tick-workflow.md`, `plan-output.md`, `github-api.md`, `cli.md`, `loki-push.md`, `watch-list.md`, `data-model.md`.

**Acceptance**: T056 to T058 pass; job names are `item N` and the summary names repos only (FR-058); the matrix carries only `n` and the ciphertext (FR-059); the lint passes on the final workflow and fails on each mutated copy; no `if:` reads `secrets.`; no `toJSON(secrets)`; `run-docker` never references the non-docker names; the idle call count of the reference fleet is 120; at most 60 status writes a tick.

**Constraints**: C1 to C7. Do not adopt rootless dind or `--cap-drop ALL`; T065 only trials them. `verdict` stays outside the `main` guard. T060 edits `plan.rs` after T028; T061 touches no file that T060 touches.

### S15. Review fixes, round 1 (T066 to T081)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T066, T067, T068, T069, T070, T071, T072, T073, T074, T075, T076, T077, T078, T079, T080, T081. Implement with `/speckit-implement`, only these tasks. T081 stays blocked with T045.

**Goal**: close the findings R1 to R16 of review round 1 with the smallest edits, each proved by the test named in its task.

**Files**: `src/plan.rs`, `src/main.rs`, `src/job.rs`, `src/engine.rs`, `src/log.rs`, `src/ship.rs`, `src/github.rs`, `src/walk.rs`, `src/text.rs`, `src/lib.rs`, the test files named per task, `tests/executor.rs`, `.github/scripts/factory-ci-{plan,blob,lint-workflow}.sh`, `.github/scripts/test-factory-ci-hardening.sh`, `.github/workflows/factory-ci-tick.yml`, `.github/workflows/factory-ci-selftest.yml`, `README.md`, `quickstart.md`.

**Contract**: `research.md` D12 (Review round 1), `contracts/tick-workflow.md`, `plan-output.md`, `github-api.md`, `cli.md`; spec FR-017, FR-024, FR-028, FR-036, FR-047, FR-060, A20.

**Acceptance**: every named test passes; the lint passes on the final workflow and fails on each new mutated copy; no `ignore` markers remain in `tests/executor.rs`; no dead code from R11; clippy clean.

**Constraints**: C1 to C7. Write each test before its fix. Keep `plan.rs` order T066, T067; `job.rs` order T069, T075; `log.rs` order T072, T076. Do not touch `ci.yml` or the `ubuntu-latest` pins (the user decides). Record nothing further: the accepted residual risks are in `research.md` D12.


### S16. Review fixes, round 2 (T082 to T086)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T082, T083, T084, T085, T086. Implement with `/speckit-implement`, only these tasks. (T081 is extended in this round; it stays in S15 and blocked with T045.)

**Goal**: close the round-2 cleanup findings with the smallest edits.

**Files**: `src/main.rs`, `src/engine.rs`, `tests/cli.rs`, `src/plan_hardening_tests.rs`, `README.md`, `.github/scripts/test-factory-ci-hardening.sh`, `.github/workflows/factory-ci-selftest.yml`.

**Contract**: `research.md` D12 (Review round 2), `contracts/plan-output.md`, `contracts/cli.md`; spec FR-047, FR-028.

**Acceptance**: each named test passes; the T068 test (T085) fails when the docker fallback is put back; no duplicate lint self-test line; the `SPAWN_FAIL` literal appears once.

**Constraints**: C1 to C7. Write the test before the fix (T082 after T085). No cargo, docker or make on the dev VM. Do not touch `ci.yml`, `.scripts/` or the root `Makefile`.


### S17. Review fixes, round 3 (T087)

**Spec folder**: `specs/002-ci-runner/`. **Tasks**: T087. Implement with `/speckit-implement`, only this task.

**Goal**: prove the no-Docker-token behaviour of phase C and document its known limit.

**Files**: `tests/cli.rs` or `src/plan_hardening_tests.rs`, `src/main.rs` (one comment line).

**Contract**: spec FR-047 and A27; `research.md` D12 (Review round 3).

**Acceptance**: the new test passes and fails when the Docker filter in phase C is removed; the comment sits at the `invalid.retain` and names A27.

**Constraints**: C1 to C7. No cargo, docker or make on the dev VM. Do not change behaviour: this task adds a test and a comment.
