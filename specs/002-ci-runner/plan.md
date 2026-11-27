# Implementation Plan: Central Interval CI Runner

**Branch**: `002-ci-runner` | **Date**: 2026-11-26 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/002-ci-runner/spec.md` (rewritten for the central
interval design, with story 11 and FR-028 to FR-037 for log shipping).

This plan replaces the earlier plan for the polling daemon. Supporting documents:
[research.md](research.md) (decisions and rejected options), [data-model.md](data-model.md),
[contracts/](contracts/), [quickstart.md](quickstart.md), [tasks.md](tasks.md) (tasks and one
engineer brief per step).

## Summary

An external caller starts B's `workflow_dispatch` workflow. Four jobs follow. `plan` runs `rust-ci-runner plan` in two
phases: it reads the watch list and the GitHub REST API to find what each listed repo needs, then (only if there is
work) the wrapper checks the secret store, and the second phase posts `pending` and "unchanged since" statuses and
writes at most 20 rows, which the wrapper encrypts into an opaque matrix. `run` and `run-docker` are that matrix, 4 at a
time: `rust-ci-runner job` clones the commit in a container, runs the `.factory-ci.toml` graph with the existing
engine, ships step logs to a Loki-compatible store, and posts the final status. `verdict` fails the tick when `plan` met
a problem early in the hour, and pings the dead-man URL on every tick. Commit statuses on the watched repos are the
only state; nothing is kept between ticks.

The pipeline format, the graph checks, the container engine with its docker sidecar, redaction
and the log type stay. The daemon, its stores and the GitLab and Gitea code go.

## Technical Context

**Language/Version**: Rust 1.90.0, edition 2021 (`rust-toolchain.toml`, unchanged)

**Primary Dependencies**: `serde`, `serde_json`, `toml`, `ureq` (rustls), `getrandom`: all
already present. `tiny_http` moves from `[dependencies]` to `[dev-dependencies]` (fake HTTP
servers in tests). No new crate.

**Storage**: none. Commit statuses on the watched repos.

**Testing**: `cargo test`. Unit tests per module. `plan` logic is tested against an in-memory
`FakeApi` (`src/fake.rs`, test-only). `github.rs` and `ship.rs` are tested against a local
`tiny_http` server. `tests/argv.rs` (stub engine), `tests/cli.rs` (the built binary),
`tests/executor.rs` (real Docker, CI only). Workflow rules: shell lint script run by the
self-test workflow.

**Target Platform**: GitHub-hosted `ubuntu-24.04` (x86_64) for ticks. The static binary also
runs on aarch64 for local use.

**Project Type**: CLI (single crate: library plus binary)

**Performance Goals**: an idle tick of 10 repos x 5 branches x 3 projects makes 120 API calls
(SC-005 limit 150), see `research.md` D3.

**Constraints**: 1 GB RAM on the dev machine, so no local cargo, docker or builds. The code has
never been compiled (Actions are disabled), so the first CI run is a gate (task T045).

**Scale/Scope**: up to 100 listed repos in the file, but a tick budget of about 13 repos at 5
branches for the 150-call target; 20 items per tick.

## Constitution Check

Gate before design, rechecked after.

- I. Project Isolation: the code stays in `rust-ci-runner/`, which builds and tests from its own
  directory. Tests do not read files outside it; the workflow lint runs in `.github/scripts/` and
  is called from the self-test workflow. The watch list lives in `.github/`, which the
  constitution lists as repo tooling. Pass.
- II. Uniform Contract: `Makefile` targets, `README.md` and `rust-toolchain.toml` remain. Pass.
- III. Fast Bootstrap: template and project exist already. No new project. Pass.
- IV. Simplicity: no new dependency, one binary, no state files. Complexity is listed below. Pass.
- V. Tested Changes: tasks.md maps every FR and acceptance scenario to a test task. Recorded gaps:
  GHCR publishing, the live `workflow_dispatch` path, the real Loki store and real GitHub API
  behaviour cannot be exercised before Actions are enabled; `quickstart.md` lists the manual
  checks. Pass with those gaps.
- Development Workflow (Spec Kit): `/speckit-specify`, `/speckit-clarify`, `/speckit-plan`, `/speckit-tasks`,
  `/speckit-analyze` ran in order on this folder; `/speckit-implement` runs per brief. Principle VI of PR 4, once
  merged, requires the same. Pass.
- Repo Structure: "Repo-wide tooling (CI workflows, root `Makefile`) only orchestrates projects; it MUST NOT contain
  project logic." **Justified exception, recorded here per the constitution's Governance.** The tick workflow and
  `.github/scripts/factory-ci-{secrets,job,plan}.sh` do hold logic (secret lookup in Bitwarden or GitHub, masking, the
  retry window). FR-038 forbids that logic inside the runner binary, because the binary must stay free of any secret
  vendor, so it can live only in the workflow layer. The scripts do nothing else: they fetch values, mask them and
  `exec` the project binary, which holds all planning, pipeline and shipping logic. They have shell tests
  (`test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh`) and a lint, and are listed in Complexity Tracking.

Layout note: the constitution says projects live at the top level as `<lang>-<name>`; this one is
`rust-ci-runner/`. `.scripts/repo.sh` treats every top-level directory except `specs` as a
project, so `.github/` is not one and `rust-ci-runner/` is.

## Decisions

Full text and rejected options are in `research.md`.

1. **Planning is a Rust subcommand** (`plan`), a pure function over an `Api` trait, tested with a
   fake. Shell would leave the ancestor walk and the status rules untested.
2. **Secret isolation**: no GitHub secret is a pipeline secret (D12 D-b). Each job's wrapper holds the bootstrap
   tokens and nothing else of B's. It fetches only the names the pipeline lists (decision 8), unsets the store
   token and `exec`s the runner. The runner passes only `-e NAME` for names a step lists, so the process
   environment never reaches A's containers. Residual risk: a docker-opt-in repo can escape to the VM and read
   that job's tokens and the docker project; the plain job cannot reach either.
3. **API budget**: 120 calls idle (110 plus one repo lookup per entry) for the reference fleet. Unchanged statuses are written once per
   commit, never per tick. Writes are paced at 750 ms and capped at 60 a tick. A read reserve of 60 requests is kept for writes. An item runs only if
   its `pending` post succeeded.
4. **Ordering**: pending is posted before the matrix exists, jobs run after it, a workflow-level
   concurrency group serialises ticks. Stale `pending` is older than 7200 s by `updated_at`.
5. **Watch list**: `.github/factory-ci-repos.toml`, `[[watch]]` tables, validated whole before any
   work, checkable in pull requests with `plan --check`.
6. **`job` beside `run`**: both call `run_pipeline`. `run` stays free of tokens and network.
7. **Log shipping** in `log.rs` plus a new `ship.rs`: a line prefix, not structured metadata,
   four labels, a base64 encoder instead of a crate, drop on failure, flush at most 60 s.
8. **Pipeline-declared secrets from Bitwarden Secrets Manager** (story 12, `research.md` D9 as amended by D12): the runner stays vendor-free. `job` has a quiet prepare mode that validates the pipeline and writes the names its steps list to a file. A sourced shell library, run by a plan wrapper and a job wrapper, fetches only those names with the pinned `bws` v2.1.0 CLI (checksum-verified), from one project (default, or docker for `allow_docker` repos), in the FR-040 order: key `owner/name<SEP>NAME` (needs the entry's `id`), then shared key `NAME` if the entry lists it. There is no GitHub secret source and no per-repo project. It retries 60 s, masks, unsets its token and `exec`s the real run. `plan` runs in two phases, so a tick with nothing to run makes no store call; the store check and the log credentials happen between the phases, before any status is posted. `bitwarden/sm-action` is rejected (ID-based, exports to the job environment).
9. **Hardening and operations** (`research.md` D10, amended by D11 and D12): every bootstrap secret in a main-only GitHub Environment and every job guarded to `main`; the matrix split into `run` and `run-docker`, the latter with its own Bitwarden account, project and token; fail-closed shipping on a public B; only statuses created by the token's user count; repo `id` check before planning; sanitized reason text; metadata endpoints blocked; images pinned by digest and mirrored to GHCR; status writes paced at 750 ms and capped at 60 per tick; B is public with a 5-minute tick (A25), because an idle tick bills about 2 minutes and a private B would bill about 17,000 minutes a month.
10. **Clarify session 2026-11-27** (`research.md` D11): B is public (A25); an `error` older than 30 minutes is retried once (A6c, FR-009), now by a " (retry)" marker in the description (D12); job names are `item N` (FR-058); the Bitwarden key separator is `/`, with `:` as the fallback and never `__` (A24); the heartbeat has two channels, each only when configured (FR-056).
11. **Critic review** (`research.md` D12): nothing private in B's public log: the matrix carries `{n, blob}` with the row encrypted by `openssl` under a masked key, private repos log only to Loki (`job --private`), the wrapper decrypts inside the script (FR-059); `secret_branches` limits secrets to the default branch (FR-060); image-pull and clone failures are `error`; the ping comes from `verdict` on every tick; persistent repo errors fail a tick only in the first 10 minutes of a UTC hour.

## Project Structure

### Documentation (this feature)

```text
specs/002-ci-runner/
├── spec.md  plan.md  research.md  data-model.md  quickstart.md  tasks.md
├── checklists/requirements.md
└── contracts/  watch-list.md  plan-output.md  github-api.md  loki-push.md  cli.md  tick-workflow.md
```

### Source Code

**Deleted** (the long-running mode and the other git hosts):

| File | What goes | Why |
|---|---|---|
| `src/poll.rs` | all: `ls-remote` parser, ref diff, fork rule, poll state | polling daemon |
| `src/store.rs` | all: `RunRecord`, recovery, retention | local state |
| `src/server.rs` | all: `/health`, `/logs` | HTTP server |
| `src/sched.rs` | all: queue, run limit, supersede, dedupe | run queue; the step DAG scheduler is in `engine::execute` and `pipeline.rs`, which stay |
| `src/config.rs` | all: `Config`, `Repo`, host and token fields, env refs, `norm_url`. `DIND_IMAGE` moves to `engine.rs` | daemon config; replaced by `watch.rs` |
| `src/status.rs` | all: GitLab and Gitea mapping, the per-commit post queue | replaced by `github.rs` (GitHub only, synchronous) |
| `src/app.rs`, `src/app_tests.rs` | `App`, `poll_loop`, `start_workers`, `run_job`, `enqueue`, `apply_refs`, `health`, `report`. `run_env` and `local` move to `job.rs` | daemon wiring |
| `.github/workflows/factory-ci.yml` | all | reusable workflow, cut by the spec |
| `.github/workflows/factory-ci-dispatch.yml` | all | dispatch from A, cut |
| `.github/scripts/factory-ci-validate.sh` | all | payload check for the dispatch workflow |
| `engine.rs`: `ls_remote`, `cleanup_polls`, `POLL_LABEL` | those items | polling only |
| `util.rs`: `is_run_id`, `write_atomic` | those items and their tests | used by the store only |
| `tests/executor.rs`: `polling_a_real_repository_...` | that test and the `poll` import | polling |
| `tests/argv.rs`: the `ls_remote` half of the first test | that half | polling |
| `tiny_http` as a runtime dependency | moved to dev-dependencies | server removed |
| `factory-ci-selftest.yml`: the `payload` job | that job | its script is deleted |

**Kept unchanged**: `pipeline.rs` parsing, `validate`, `only`, `needs_docker`, `read_file`,
`MAX_BYTES`, `MAX_STEPS`; `engine.rs` `execute`, `run_step`, `Sidecar`, `clone_repo`, `capture`,
`run_logged`, `cleanup`, `cleanup_run`, `remove_dir`; `redact.rs` core; `rust-ci-runner-release.yml`;
`ci.yml`; `Dockerfile`; `Makefile`; `rust-toolchain.toml`; `tests/fixtures/selftest/`.

**Kept with small edits** (each is a task):

- `pipeline.rs`: drop the `fork` parameter of `resolve_secrets` and `docker_problems` and its
  tests; reserve more prefixes (`LD_`, `GIT_`, `SSL_`, `BASH_ENV`, `ENV`, `*_PROXY`) because
  secrets now reach a host process; add `ITEM_`, `ALL_SECRETS`, `BASH_`, `NODE_OPTIONS`; drop the allowlist message (the runner reports "not defined");
  reject every step secret name that is not a valid variable name or is reserved, at parse time.
- `engine.rs`: step and clone output goes through `Log::out` with its stream; step results go
  through `Log::event` with durations; the unused polling items go.
- `redact.rs`: a multi-line value also redacts each of its lines of 4 characters or more (A22).
- `log.rs`: see below.
- `lib.rs`, `main.rs`: new module list and subcommands.
- `Cargo.toml`: `tiny_http` to dev-dependencies.
- `factory-ci-selftest.yml`: drop `payload`, add `list` (runs `plan --check` on the real list)
  and `workflow` (runs the lint script).
- `README.md`: rewritten for the tick, the list file, secrets, log shipping, the residual risks.

**New**:

```text
rust-ci-runner/src/
├── text.rs       sanitizer for reasons, descriptions and summary lines (FR-051)
├── watch.rs      watch list parse, validate, glob match, id and secret_branches
├── github.rs     Api trait, HttpApi (ureq), error classes, rate reserve, paging, post_status, description
├── projects.rs   tree -> projects (with the truncated-tree fallback), file ownership, folder-name rule
├── walk.rs       status lookup, first-parent window, changed files, per-project decision, retry marker
├── plan.rs       phase A (candidates) and phase C (refusals, cap, posts), rows and summary
├── ship.rs       LogConfig, Shipper thread, Loki batches, retry, flush, link template, heartbeat line
├── job.rs        run_pipeline (shared with run), job flow, prepare mode, final status; run_env; local run
├── fake.rs       test-only in-memory Api
rust-ci-runner/tests/cli.rs            built-binary tests (no Docker)
rust-ci-runner/tests/no_vendor.rs      no secret-vendor code outside pipeline.rs (FR-038)
rust-ci-runner/tests/job_prepare.rs    job prepare mode, image pin, exit-125 mapping
rust-ci-runner/src/{bws,plan_hardening,text}_tests.rs   entry id and secret_branches, creator filter, caps, fairness, sanitizer, masking
.github/workflows/factory-ci-tick.yml           the tick (plan, run, run-docker, verdict)
.github/workflows/factory-ci-mirror-images.yml  manual: mirror alpine/git and docker:dind to GHCR, print digests
.github/scripts/factory-ci-secrets.sh           secret fetch from Bitwarden, mask, exec (sourced library)
.github/scripts/factory-ci-blob.sh              encrypt and decrypt the matrix blob (sourced library)
.github/scripts/factory-ci-plan.sh              plan wrapper: phase A, store check, phase C, blob output
.github/scripts/factory-ci-job.sh               job wrapper: decrypt, prepare, fetch, run
.github/scripts/factory-ci-status.sh            fallback status step: decrypt, post error
.github/scripts/test-factory-ci-secrets.sh      shell tests with a stub bws
.github/scripts/test-factory-ci-hardening.sh    wrapper hardening and blob tests (stub bws, sudo, iptables)
.github/scripts/factory-ci-lint-workflow.sh     workflow rules check
.github/factory-ci-repos.toml                   the watch list (initially no entries)
specs/002-ci-runner/{research,data-model,quickstart}.md, contracts/
```

`log.rs` after the change: `Log::new(out, red, limit)` keeps its signature (tests use it).
`Log::with_shipper(Shipper, Labels, Prefix)` and `Log::quiet(bool)` configure it. `Log::line`
stays as `out(who, "stdout", text)`. New: `Log::out(who, stream, text)` and `Log::event(who, text)`.

## Security assessment

Review findings H1 to H3, M1 to M7, L1 to L5 and the critic's findings are folded in; decisions and rejected
options are in `research.md` D10 and D12.

- **Trust boundary**: everything in a watched repo is untrusted: code, `.factory-ci.toml`, branch names, folder
  names, file names, status texts, and the statuses themselves. B's workflow, scripts and binary are trusted only at
  `main`. B is public: everything B prints, outputs or commits is public.
- **Nothing private in the public log (D-a, FR-059)**: the matrix carries `{n, blob}`. The row (repo, sha, branch,
  project, secret names, run id, flags) is encrypted by the plan wrapper with `openssl` (600,000 iterations) and a key of at least 32 characters from a masked
  secret (a separate key for docker rows, R5), and decrypted inside the job's script. No step `env:` shows an `ITEM_*` value, no job name shows more than
  `item N`, and the lint forbids it. For a private repo, step names and events go only to the log store; a job with
  no store fails closed. The committed watch list and the summary name repos (accepted, clarify 3B).
- **Which ref runs (H2)**: every bootstrap secret lives in a GitHub Environment whose deployment branch policy is
  `main`, every job that touches one declares it and has `if: github.ref == 'refs/heads/main'`, and a ruleset
  protects `main`. A dispatch with another ref gets no secrets. Environments with `workflow_dispatch` are unverified
  here (T065).
- **Blast radius of `allow_docker` (H1)**: the privileged `docker:dind` sidecar can reach the VM, and the VM holds
  what the job holds. `run` handles repos without `allow_docker`; `run-docker` handles the rest with its own
  Bitwarden machine account (`BWS_ACCESS_TOKEN_DOCKER`), its own project (docker repos' prefixed and shared keys, plus
  a copy of the log keys), and its own fine-grained token limited to those repos. A docker escape reaches the docker
  project and the docker repos only. Rootless dind is not adopted (D10). Project count: 2 of the free plan's 3.
- **No GitHub secret is a pipeline secret (D-b)**: `toJSON(secrets)` appears nowhere, so no job holds a set of B's
  secrets. The wrapper of a job holds the bootstrap tokens it needs and nothing else. This removes the fallback
  source, the clash log and the 128 KB limit.
- **Shipping off on a public B (H3)**: B's visibility comes from `github.event.repository.private`. When B is public
  and no log store resolved, phase C drops private-repo rows (a problem line, tick red under A12's hourly rule), and
  `job --private` fails closed as a second gate. `--quiet-logs` is decided from the resolved values after the fetch,
  not from the environment at wrapper start.
- **Secrets only on the default branch (D-d 6, FR-060)**: a row carries `secrets_ok`; a pipeline that lists secrets
  on another branch ends in `error`, so a feature-branch push cannot read the repo's keys.
- **Status spoofing (M1)**: only statuses whose `creator.login` equals the token's user (`GET /user` once per token
  per plan) count.
- **Attacker-influenced text (M2)**: reasons, descriptions and summary lines pass one sanitizer
  (`[A-Za-z0-9 _.:/-]`, truncated, never starting with `::`). The wrapper never echoes a reason to stdout.
- **Names (M3)**: reserved names match case-insensitively (`BWS_*`, `FACTORY_CI*`, `GITHUB_*`, `RUNNER_*`, `ACTIONS_*`,
  `LD_*`, `BASH_*`, `GIT_*`, `ENV`, `NODE_OPTIONS`, `PATH`, `HOME`, `ITEM_*`, `ALL_SECRETS`, plus the earlier list).
  The shell reads the names file with `while IFS= read -r`, rechecks each against `^[A-Za-z_][A-Za-z0-9_]*$` and never
  uses `eval`. Keys are matched with `jq --arg k` exactly. Watch repo names match
  `^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$` without `..`.
- **Repo-jacking (M4)**: an entry may record the numeric repo `id`; phase A checks `GET /repos/{o}/{n}` against it,
  and the prefixed key rule needs an `id`.
- **Fetched JSON (M5)**: `bws` output goes through a pipe or a 0600 file in `$RUNNER_TEMP`, deleted before `exec`;
  `set +x` first; no stderr printed; values masked as soon as parsed. The same `set +x` and no-echo rule covers the
  blob decryption.
- **Masking (M6)**: a value with a trimmed line under 4 characters is refused (name only). The runner and GitHub also
  mask the base64 of each value and of the clone's Basic header. Other encodings are not caught; the log store holds
  unmasked encoded output, and the README says so.
- **Cloud metadata (M7, FR-054)**: the wrapper drops traffic to `169.254.169.254` and `168.63.129.16` in
  `DOCKER-USER`. Step containers drop `NET_RAW`, `MKNOD` and `SETFCAP`. `--cap-drop ALL` is not used: it breaks root
  package installs and cannot be tested here. The real test of the block is the live check in T065.
- **Injection**: item fields reach the shell only through the decrypted blob inside scripts, never through `env:` or
  `run:` expansion; the lint rejects `${{` in `run:` blocks; engine calls use argv.
- **Residual risks**: a kernel or runtime escape from a plain container reaches the `run` job's tokens and the
  default project; a docker-step escape reaches the docker project and tokens; `FACTORY_CI_TOKEN` reads and writes
  statuses on the non-docker listed repos; the Actions UI exposes repo names (job names are `item N`, the summary
  and the watch list name repos); Loki holds unmasked encoded output; a compromise of `main` defeats everything, so
  protect `main`. The blob is CBC without a MAC; it never leaves one workflow run.

## Infrastructure assessment

- **Cost (D1, A25)**: B is public with a 5-minute tick (clarified). An idle tick bills about 2 minutes, which a
  public repo does not pay for. A private B (17,000 billed minutes a month) is not supported.
- **Compute**: GitHub-hosted `ubuntu-24.04`, x86_64, pinned (not `ubuntu-latest`) in every workflow written here.
- **Settings the owner makes**: enable Actions on a separate public copy of B, never this monorepo (A1); three
  Environments (`factory-ci-plan`, `factory-ci`, `factory-ci-docker`) with deployment branch policy `main`; ruleset on
  `main`; run retention 7 to 14 days; repository variables `RUNNER_IMAGE_DIGEST`, `FACTORY_CI_CLONE_IMAGE`,
  `FACTORY_CI_DIND_IMAGE`, `FACTORY_CI_BWS_PROJECT`, `FACTORY_CI_BWS_PROJECT_DOCKER`, optional `FACTORY_CI_LOG_LINK` and
  `FACTORY_CI_BWS_SERVER_URL`; bootstrap secrets only: both repo tokens, both Bitwarden tokens, `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER`,
  optional `FACTORY_CI_HEARTBEAT_URL`; the GHCR packages public.
- **Images (D8, L1)**: `alpine/git` and `docker:27.5.1-dind` are mirrored to GHCR by a manual workflow and pinned by
  digest in repository variables; `job` refuses an unpinned image. The second clone in a job finds the image already on
  the VM, so there is one pull per job.
- **External**: a scheduler calling the dispatch endpoint (recipes in `quickstart.md`), a Loki-compatible store,
  Bitwarden Secrets Manager (free plan: cloud only, 3 projects, 3 machine accounts; this design uses 2 and 2).
- **Rate limits (D2)**: status writes are paced at 750 ms (GitHub's secondary limit is 80 content-creating requests a
  minute, 500 an hour), capped at 60 per tick, the rest carried to the next tick. A secondary-limit 403 is
  `RateLimited`. A tick with nothing to run makes no Bitwarden call; a job makes one list call.
- **Limits**: at most 20 rows per tick across both matrices; job output well under 1 MB (an `{n, blob}` row is a few
  hundred bytes); runner disk about 14 GB.
- **Monitoring (D6, D12)**: a red tick is the alert for failures that run, throttled to the first 10 minutes of a UTC
  hour (A12). Silence needs the ping: the `verdict` job pings `FACTORY_CI_HEARTBEAT_URL` on every tick; the owner sets
  the dead-man grace above the longest tick (about 3 hours for 20 slow items). The Loki heartbeat line is a "tick with
  work" marker only, because idle ticks make no store call. cron-job.org gets its failure email; the Pi uses `flock`
  and `logger`. Usage alert at 70% of the free 50 GB (D11).
- **Retention (D11)**: Grafana free keeps logs 14 days, so status links to logs die after 14 days. README note.
- **Token lifetime (D7)**: fine-grained tokens expire (90 days recommended) and cover one owner. Rotation steps are in
  `quickstart.md`. A 401 at phase A fails the tick with a clear reason.
- **Early compile check (not decided)**: the crate has never compiled. A first compile needs a scratch public repo or
  Actions enabled; no repo is created here, and T045 stays blocked until the owner chooses.

## Failure modes

Test names are indicative module paths; the files that hold them are named in `tasks.md`.

| Step | What can fail | Detection | Behaviour | Test |
|---|---|---|---|---|
| plan: list | unknown key, bad name or id, duplicate, reserved secret, bad glob, file missing or over 64 KB | `watch::parse` | all problems listed, exit 1, nothing else runs | `watch_tests`, `cli.rs` |
| plan: log config | one or two of three set; link without `{run_id}` | `ship::LogConfig::from_env` | exit 1 before any API call | `ship_tests`, `cli.rs` |
| plan: digest or image unset | variable empty, malformed or unpinned | guard step (first) | tick fails before any repo is checked | workflow lint |
| plan A: repo unreachable | 404, 403, network, 5xx after retries | `ApiError::Http` or `Net` | repo skipped, problem recorded, others go on; the tick fails only in the first 10 minutes of a UTC hour | `plan_error_tests` |
| plan A: rate limit | 429, secondary 403, remaining below reserve | `ApiError::RateLimited` | reads stop, planned work is posted and run, unchecked repos listed | `plan_error_tests` |
| plan A: walk fails | commit list, ancestor status or compare error (not rate limit) | `walk::decide` | project runs | `walk_tests` |
| plan A: tree truncated | `truncated` true | `projects` | fall back to root tree plus one call per top-level folder | `projects_tests`, `github_tests` |
| plan A: 100+ branches | count | `plan` | repo error, no partial plan for it | `plan_error_tests` |
| plan A: foreign status | creator is not the token's user | `walk` creator filter | treated as no status | `walk::tests::foreign_creator_ignored` |
| plan A: repo renamed or transferred | repo id differs from the entry's `id` | repo lookup | repo skipped, problem names both ids | `plan_hardening_tests` |
| plan A: old `error` or old `pending` | `error` over 30 minutes, or `pending` over 2 hours | `walk` | one retry marked " (retry)"; a marked old `error` stays, a marked old `pending` is closed with `error` | `walk_tests`, `plan_race_tests` |
| store check: store down or token rejected | `bws` fails in the plan wrapper, candidates exist | `factory-ci-plan.sh` | no status posted, tick red with "secret store unreachable" or "secret store rejected the token" | `test-factory-ci-secrets.sh` |
| store check: idle tick | no candidate rows | `factory-ci-plan.sh` | no store call at all | `test-factory-ci-secrets.sh` |
| plan C: public B without a log store | private repo, B public, no log credentials | phase C | row dropped, problem recorded | `plan_hardening_tests` |
| plan C: pending post fails | no write access, rate limit | `post_status` error | row dropped from the matrix, problem recorded | `plan_tests` |
| plan C: write cap or secondary limit | more than 60 writes needed, or a secondary-limit 403 | write counter, `RateLimited` | pending first, unchanged next, rest wait for the next tick | `plan_hardening_tests` |
| plan C: crash after posts | process killed before the output | none (no state) | pending ages out at 7200 s; the retry budget applies | `plan_race_tests` |
| blob: key missing or ciphertext bad | decrypt fails | job wrapper | the wrapper cannot read the row, so the fallback status step cannot either: the item stays `pending` until the 2 h stale rule closes it (R15) | `test-factory-ci-hardening.sh` |
| job: prepare fails | invalid TOML, cycle, symlink, over 100 steps, docker or name not allowed | `job --names-file` | item `error` with the reason, no fetch | `job_prepare` |
| job: secrets on a non-secret branch | `secrets_ok` false and the pipeline lists secrets | wrapper | item `error` "secrets not allowed on this branch" | `test-factory-ci-secrets.sh` |
| job: no store configured | `BWS_ACCESS_TOKEN` unset and the pipeline lists secrets | wrapper | item `error` "no secret store configured" | `test-factory-ci-secrets.sh` |
| fetch: store down, token rejected | `bws` fails | `factory-ci-secrets.sh` | runner never starts, item `error` with the fixed reason | `test-factory-ci-secrets.sh` |
| fetch: name not found, ambiguous, short line | no match, two matches, line under 4 characters | script | item `error` naming the secret, nothing exported | `test-factory-ci-secrets.sh`, `test-factory-ci-hardening.sh` |
| fetch: another repo's key | key under a foreign prefix | script (exact keys only) | never selected | `test-factory-ci-secrets.sh` |
| job: clone | token wrong, sha missing, transient failure | `clone_repo` State | `error` "clone failed" (retryable once) | `job_tests` |
| job: image not pinned | `FACTORY_CI_CLONE_IMAGE` or `_DIND_IMAGE` lacks `@sha256:` | `job` | exits before cloning, item `error` | `job_prepare` |
| job: log store required but absent | `--private`, no config | `job` | exits before cloning, item `error` "log store required" | `cli_logs` |
| job: step fails or times out | exit non-zero, timeout | `execute` Outcome | `failure` "step X failed: ..." | existing executor tests, `job_tests` |
| job: step image cannot be pulled | docker exit 125 | `job` | item `error`, not `failure` | `job_tests` |
| job: runner panic | bug | `catch_unwind` in `job` | `error` "internal error", status still posted | `job_tests` |
| job: killed or cancelled | VM lost, cancel, timeout | workflow fallback step | `error` "job did not finish"; if even that fails, the stale rule | lint script, quickstart |
| job: status post fails | network, 5xx, 403 | `post_status` retry (3 tries) | exit 1, fallback step tries again, else the stale rule | `job_tests` |
| ship: store down or slow | 429, 5xx, timeout | `Shipper` | retry 60 s, drop, " (logs not delivered)", result unchanged | `ship_tests`, `ship_job` |
| ship: bad credential | 401, 403 | `Shipper` | stop at once, no retry | `ship_tests` |
| ship: bad batch | 400 | `Shipper` | drop, no retry | `ship_tests` |
| ship: queue full | 32 MiB queued | `Shipper::push` | drop line, flag set, steps never block | `ship_tests` |
| scheduler dead or workflow disabled | no dispatch, no plan | ping absence alert | email from the ping service after its grace | quickstart check 11 |
| verdict | plan failed, or `fail_tick` is 1 | `needs.plan` | tick red; always runs and pings | lint script |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. **The code does not compile or has logic slips**, because it was never built. Prevention: tasks
   S1 to S9 leave `cargo` to CI; T045 enables Actions on a scratch basis (or a larger machine) and
   runs `make lint test` before anything else is trusted; engineers write small functions with
   tests beside them.
2. **GitHub API behaves differently than the contract**: the combined status skips a context,
   `trees/{commit sha}` is refused, compare truncates. Prevention: `contracts/github-api.md`
   states each assumption; `HttpApi` tests pin the requests; quickstart step 4 runs one real tick
   against a scratch repo before the list gets real repos; every unexpected shape is a repo error
   or "run", never "unchanged".
3. **A secret or private detail leaks** into the public Actions log, a status or the summary. Prevention: the matrix
   carries only an encrypted blob and `item N`; the lint forbids `ITEM_*` in any `env:`; private repos log only to
   Loki; one redactor on every output path (I7); `valid_secret_name` reservations; the wrapper's `exec` and `unset`; a
   live check (T065) greps a real public run log for a branch, SHA, project path and secret name.
4. **A tick storm**: rate limit, secondary limit, or hundreds of jobs. Prevention: 20-item cap,
   100-branch cap per repo, read reserve, paced writes, `max-parallel: 4`, concurrency group with
   one queued run.
5. **Work silently never runs**: a stuck `pending`, an `error` that stays final, a list typo half applied, a dead
   scheduler. Prevention: the 7200 s stale rule and the one-time retry (A6c) with its marker, `verdict` fails on plan
   problems in the first 10 minutes of each UTC hour, whole-list validation before any call, `plan --check` on pull
   requests, the ping alert. A repeated `error` stays red until a new commit; the README says so.

## Invariants for property tests

Hand-rolled generators with a fixed seed, no new crate. QA turns each into a test.

- I1. `plan` makes no API call for a repo not in the list (the fake records calls).
- I2. A matrix row exists if and only if its `pending` post returned success.
- I3. `count <= cap`; within a repo rows are ordered by (date, branch, project); across repos they are interleaved round-robin so no repo takes the whole cap (L4); status writes per tick never exceed 60 (D2).
- I4. For any history with no status on any commit, every project of every tip is `Run`.
- I5. A project is `Unchanged` only if the nearest ancestor with a status is `success` and no
  changed path (name or previous name) is owned by it.
- I6. For any path list and project set, each path is owned by exactly one project, or by none
  when the root is not a project; `foo/` never owns `foobar/x`.
- I7. For random secret values (including multi-line) and random output, no value or line of 4+
  characters appears in the log sink, a shipped batch, a status description or the summary.
- I8. Splitting a line: the pieces concatenate to the input, each is at most 16 KB, each starts
  and ends on a character boundary.
- I9. `base64` matches the RFC 4648 vectors and the system `base64` on random bytes (CI only).
- I10. Any list the generator builds from valid parts validates; adding an unknown key to any
  table makes it fail.
- I11. A status description is at most 140 characters, ends with the suffix exactly when logs
  were not delivered, and keeps the suffix when the reason is long.
- I12. The state `job` posts last is one of `success`, `failure`, `error`, never `pending`.
- I13. After the first `RateLimited` answer, `plan` makes no further read call.
- I14. Every field of a matrix row passes its validator (sha, names, folder rule, run id).
- I15. A Loki payload has exactly the four labels plus `service_name` (009), values sorted by timestamp, and no line over 16 KB.
- I16. Any shipper failure mode leaves the `JobResult.state` equal to the state with shipping off.
- I17. No printed line in quiet mode contains step output; no printed line starts with `::`; when B is public and no log store resolved, no step output of a private repo is printed or run (H3).
- I20. Only statuses created by the token's user count: for any status set with foreign creators mixed in, the decision equals the decision with the foreign ones removed (M1).
- I21. For any reason, name or TOML error text, the sanitized text matches `[A-Za-z0-9 _.:/-]*`, is at most the limit, and does not start with `::` (M2).
- I22. For any value whose trimmed lines are all 4 characters or more, the value, its base64 and each line are absent from every output path; a value with a shorter line is refused before any container (M6).
- I23. The `run-docker` wrapper path never reads the non-docker `BWS_ACCESS_TOKEN` or token, and its fetch reads only the docker project (H1).
- I24. A project at one commit is retried at most once: for any status history, the retry fires only when the latest `error` is over 1,800 s old or the latest `pending` over 7,200 s old and the description does not end with " (retry)"; a marked old `error` is `Done` and a marked old `pending` is closed with one `error` (A6c).
- I25. For any two distinct (repo, NAME) pairs the Bitwarden keys built with `/` or with `:` differ; no key built for one repo equals a key of another (A24).
- I26. No matrix row name, summary line, job output or plan stderr line contains a branch, a SHA, a project path or a secret name; the only workflow-visible row fields are `n` and the ciphertext (FR-058, FR-059).
- I27. For any row, decrypting its blob with the key gives the row back and decrypting with another key fails; the ciphertext contains neither the key nor any row field in plain text.
- I28. A tick whose candidate file has no rows makes zero secret-store calls (FR-045).
- I29. Items on a branch outside `secret_branches` never have a secret fetched, whatever the pipeline lists (FR-060).

## Edge cases

Answered per `feiyi-edge-cases`. "Handled" names a test in `tasks.md`.

- **Inputs**. Empty list file: handled (valid, no-op; `watch::tests::empty_list`). One repo, the
  cap, one over the cap: handled (`plan::tests::cap_*`). 100+ branches: handled (repo error).
  Duplicate repos or branch tips at one sha: handled (`watch::tests::duplicate`,
  `plan::tests::shared_sha_planned_once`). Unicode, long and NUL in names, files, folders and
  lines: handled (folder rule in `projects::tests`, NUL and 16 KB split in `log::tests`, lossy
  decode in `engine` pump). Malformed API answers (bad sha, missing fields): handled
  (`github::tests::rejects_bad_sha`, serde errors become `Http`/repo error). Truncated TOML:
  handled by existing `pipeline::tests`.
- **State**. First run with no data: handled (statuses only; `walk::tests::no_ancestor_runs`).
  Old data: N/A, no stored state. Crash mid-operation: handled (stale rule, fallback status step).
  Partial failure: pending posted but matrix lost: handled (stale rule, one retry, then closed as `error`). Retry twice: handled
  (idempotent: a status on the tip means done; `plan::tests::second_tick_plans_nothing`).
- **Time and concurrency**. Two ticks at once: handled by the concurrency group, plus the pending
  check (`plan::tests::fresh_pending_not_planned`). Timeouts on every external call: handled (20 s
  API, 10 s Loki, 60 s engine quick calls, step and job deadlines). Clock and expiry at the
  boundary: handled (`walk::tests::stale_boundary` at 7199, 7200, 7201 s; unparsable time is
  stale). Out-of-order events: handled (latest status per context; pending posted before the
  matrix exists).
- **Resources**. Disk full on the runner: the clone fails, `error` posted (`job::tests::clone_failure_is_error`).
  Huge inputs: handled (file, step, branch and log caps; a truncated tree falls back to per-folder calls). Dependency slow or down: handled
  (GitHub: repo error or rate-limit path; Loki: retry then drop). Unbounded growth: log 10 MB per
  run, queue 32 MiB, no state files. Memory: 1 GB dev machine is why nothing builds locally;
  runtime memory is small (one thread per job for shipping, queue capped).
- **Permissions and trust**. Caller can start B's workflow only. A's code sees only the checkout
  of its project directory, the job environment variables and the secrets its step lists. Values
  trusted by mistake: matrix fields (decrypted from our own blob, then validated), branch names (sanitised in the prefix, passed in
  argv), `GITHUB_API_URL` (set by GitHub; not user input). Handled and tested as in Security.
- **UI**. N/A: no screen. The status line and the tick summary are the interface; descriptions are
  one line, at most 140 characters, with the suffix kept.

## Rollout and rollback

Rollout, in order:

1. Create a separate public copy of B (never this monorepo, A1) and enable Actions there. A scratch public repo
   or Actions on the copy is also what the first compile (T045) needs; no repo is created by this plan.
2. Merge the code into the copy. Run T045: `make lint test` through `ci.yml`, then the self-test. Fix compile and
   test failures first.
3. Tag `rust-ci-runner-v0.2.0`. Make the GHCR package public. Run the mirror workflow for `alpine/git` and
   `docker:dind`. Set `RUNNER_IMAGE_DIGEST`, `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE` from the printed
   digests. Record the previous runner digest in the release summary.
4. Create the three Environments, the ruleset on `main`, the tokens, the Bitwarden accounts and projects, and the
   secrets and variables of `quickstart.md`, including `FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER`.
5. Add one scratch repo (entries start on the default branch only). Dispatch one tick by hand. Walk
   `quickstart.md` checks 1 to 14.
6. Set up the ping service (grace above the longest tick) and the usage alert. Confirm each fires (stop dispatching).
7. Add real repos one at a time, a tick between each.
8. Create the scheduler job at 5 minutes.

Rollback: pause the scheduler. Empty the watch list (or remove one entry to stop touching one repo). Set
`RUNNER_IMAGE_DIGEST` back to the previous digest from the release summary. Delete `BWS_ACCESS_TOKEN` (and the
docker one): jobs whose pipelines list secrets then end in `error`, and pipelines without secrets keep running.
Statuses already posted stay on the commits. No data migration exists in either direction. Reverting the merge
removes the feature; the daemon is not coming back through this path.

## Complexity Tracking

| Item | Why needed | Simpler option rejected because |
|---|---|---|
| `plan` in Rust with an `Api` trait and a fake | the ancestor walk, ownership, caps and three API failure kinds need tests | bash and `gh` are untestable without a live API |
| `job` beside `run` | clone, secret checks, shipping and the final status do not belong in the local command | adding them to `run` breaks story 8 (no token, no network) |
| `ship.rs` with a thread and a byte-bounded queue | shipping must run beside steps and never block them | inline HTTP in the log call would stall a step on a slow store |
| base64 encoder in `util.rs` | basic auth for Loki | a `base64` crate for 15 lines of code and 4 test vectors |
| line prefix in place of structured metadata | works on every Loki | metadata needs Loki 3 with a specific schema; a store without it rejects every batch |
| `tiny_http` as dev-dependency | fake GitHub and Loki servers in tests | writing a socket server by hand costs more than the crate |
| Encrypted matrix blob (`openssl` in the plan and job wrappers) | B is public, and a step `env:` or an output would print a branch, SHA or project path (FR-059) | plain matrix fields leak; encrypting in Rust adds a crypto crate for one use |
| Secret-fetch logic in workflow scripts | FR-038 keeps vendor code out of the binary, so the fetch must sit in the workflow layer | putting it in the binary breaks FR-038; leaving it out leaves no secret source (constitution Repo Structure exception) |
| Two matrix jobs and three Environments | an `allow_docker` escape must not reach the main secrets (H1, H2) | one job with a runtime switch leaves every secret in reach of the privileged sidecar |
| Ping from `verdict` on every tick | silence needs its own signal (D6); the Loki line cannot be one because idle ticks make no store call | a red tick cannot report that no tick ran |
| Two-phase `plan` | an idle tick must make no store call, yet a store failure must stop posting (FR-045) | one phase would either call the store every tick or post before the check |
| `fake.rs` in-memory Api | invariants I1 to I6, I13 run in milliseconds without a network | HTTP fakes for every planner test are slow and brittle |


## Story 12: pipeline-declared secrets from Bitwarden Secrets Manager

Design in `research.md` D9 as amended by D12. Contracts in `contracts/tick-workflow.md`, `cli.md`, `watch-list.md`,
`plan-output.md`. Tasks T047 to T055 and the blob and status tasks in `tasks.md`. Spec: story 12, FR-038 to FR-046,
FR-052, FR-059, FR-060, A19 to A24.

- **Names**: the union of the steps' `secrets` lists, written by `job --names-file` after full pipeline validation
  (an invalid pipeline, a non-variable name or a reserved name ends the item in `error` before any fetch). The
  watch entry's `secrets` only grants shared keys in the default project.
- **Source order** (FR-040), in one project (default, or docker for `allow_docker` repos): key `owner/name<SEP>NAME`
  (needs the entry's `id`), then key `NAME` if the entry lists it. Exact-key lookups built from the item's own repo,
  so another repo's prefix is never read. There is no GitHub secret source and no per-repo project.
- **Branches**: a row on a branch outside `secret_branches` has `secrets_ok` false; a pipeline that lists secrets
  then ends in `error` before any fetch (FR-060).
- **Security**: nothing that runs A's code sees more than its step lists. The prepare run starts under `env -i` with the
  clone token only. The wrapper holds the store token for seconds and unsets it before the real run. The real run
  holds the item's fetched names, the log credentials and the clone token. Containers get `-e NAME` for names their
  step lists. `BWS_` is reserved. Values travel through shell builtins and pipes, never argv, and are masked line by
  line (4 or more characters) before anything else.
- **GitHub's 100-secret limit**: B holds six bootstrap secrets at most (two repo tokens, two store tokens, the blob
  key, the ping URL).
- **Idle ticks**: no store call (two-phase `plan`). A job makes one list call.
- **Residual**: an `allow_docker` escape reads the docker store token from the runner agent, then the docker project.
  Anyone who can push a pipeline to a listed repo reads every key under that repo's prefix and the shared names its
  entry lists, on the secret branches (A24).
- **Invariant I18**: for any stub-store answer (ok, missing, ambiguous, empty, error, timeout, 401, a key under
  another repo's prefix), the environment of the real run holds exactly the names the pipeline lists that resolved,
  plus the log credentials when all three resolve, and never the store token, an unlisted name or a value from another
  repo's prefix. A failure never starts the real run. No value appears outside `::add-mask::` lines.
- **Invariant I19**: no source file other than `pipeline.rs` (the reserved `BWS_` prefix) and tests mentions `bws` or
  `bitwarden` (FR-038).
- **Edge cases**: name not a variable name (invalid pipeline); duplicate keys; empty value (no match); multi-line
  value (masked per line of 4 or more characters); an entry without `id` (prefixed keys not read); two projects of one
  repo (share the prefix); a pipeline with no secrets (no store call beyond the log credentials); no secret store
  (`error` only for pipelines that list secrets); a non-secret branch (`error` only for pipelines that list
  secrets); store down (retry 60 s, `error`); 401 or 403 (no retry); none or partial log credentials (off or error);
  outage at the pre-post check (tick red, no status).
- **Rollout**: see the Rollout section. Create two machine accounts (default project; docker project), store their
  tokens, set `FACTORY_CI_BWS_PROJECT` and `FACTORY_CI_BWS_PROJECT_DOCKER`, create keys named `owner/name<SEP>NAME`
  and the three `FACTORY_CI_LOG_*` keys in both projects. Rollback: delete `BWS_ACCESS_TOKEN`.
- **Spec points**:
  (a) FR-038 forbids vendor code in the binary. With per-repo projects gone, the binary knows nothing of Bitwarden
  except the reserved prefix `BWS_`.
  (b) The `bws` exit code may not separate 401 or 403 from network errors. The plan classifies by stderr text,
  unverified until T054.
  (c) The pre-post check cannot cover an outage that starts after phase C. That item ends `error`; the retry budget
  gives it one more run (A6c).
  (d) Names must be known before the fetch, but the pipeline is in the checkout. The runner therefore clones twice: a
  quiet prepare run, then the real run (the second clone uses the cached git image).
  (e) A24: that `/` is accepted in a key is unverified; the separator is one constant and `:` is the fallback (never
  `__`, which can collide).
