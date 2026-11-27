# Implementation Plan: Runner-owned CI

**Branch**: `008-runner-owned-ci` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/008-runner-owned-ci/spec.md`

## Summary

Add a `ci` subcommand to `rust-ci-runner` that does what the workflow did in shell: validate inputs against the watch list, fetch four values from Bitwarden with a pinned `bws`, probe Loki, clone the target with `git`, refuse docker steps, run the pipeline and print only a closed set of event lines. Rebuild the runner image on Alpine with a docker CLI, `git`, CA certificates and `bws`, so the workflow can `docker run` it with the host socket and the work directory mounted at the same path. Cut `factory-monorepo-ci.yml` to a checkout and one `docker run`. No research, data-model or quickstart files: the design is fixed by the spec and the contracts below, and the verification steps sit at the end of this file.

## Technical Context

**Language/Version**: Rust 1.90 (edition 2021, `rust-toolchain.toml`), GitHub Actions YAML, POSIX sh, a Dockerfile

**Primary Dependencies**: no new crates. Existing: `serde`, `serde_json`, `toml`, `ureq`, `getrandom`; dev: `tiny_http`. New in the image: Alpine `git`, `docker-cli`, `ca-certificates`; `bws` 2.1.0.

**Storage**: files under `--work` (`$RUNNER_TEMP/ci`) on the hosted VM, discarded with it

**Testing**: `cargo test --lib`, run only through the `rust-ci-runner` pipeline of the released image. Nothing compiles or runs on the planning or the engineer's machine (1 GB VM): engineers write code and tests and check with `bash -n`, grep and review. A shell test of the workflow runs in `factory-ci-selftest`.

**Target Platform**: GitHub-hosted `ubuntu-24.04` (amd64) with Docker; image `linux/amd64`

**Project Type**: CLI plus CI workflow plus container image

**Performance Goals**: probe under 15 s; image pull under 30 s (about 100 MB); Bitwarden call under 2 x 60 s

**Constraints**: public repo and public logs; the runner holds a root-equivalent socket; no secret in argv; one source of truth for each pin

**Scale/Scope**: one new module pair (`ci`, `store`), small edits to `ship`, `log`, `engine`, `job`, `watch`, `main`; one Dockerfile; two workflow files; one lint script; one shell test; two READMEs

## Constitution Check

*Gate: pass before Phase 0. Re-checked after design.*

- I One home: workflow in `.github/workflows/`, scripts in `.github/scripts/`, runner in `rust-ci-runner/`, targets named in `.github/factory-ci-repos.toml`. Pass. The watch list is reused, not duplicated.
- II Secrets source: as written in 1.0.0, Loki credentials come from GitHub, so this feature failed the gate. Amendment 1.1.0 (T001, done in this commit) lets the manual workflow hold only `BWS_ACCESS_TOKEN`. No value is committed. Pass after T001.
- III Public logs: closed output grammar, fixed fault lines, panic hook, reasons to Loki only, no argv secrets. Pass (see invariants I1 and I2).
- IV Simplicity: one subcommand, one new store module, no new crate. Complexity Tracking below lists the three costs.
- V Tested: each requirement maps to a lib test or the workflow shell test (table below). The code lands on `main` before its unit tests can run through the runner (see Rollout). Stated, not hidden.
- VI Spec Kit: this folder. Pass.
- Third-party actions pinned, minimal `permissions`: kept. Pass.

Re-check after design: the `ci` command adds no principle conflict.

## Architecture decision

The runner owns the whole CI job. The workflow starts one container. Inside it the runner reads Bitwarden, clones, checks Loki, runs and prints a closed grammar.

Alternatives rejected:

1. **Keep shell in the workflow, add a thin runner flag.** Rejected by the owner: the logic must not live in GitHub Actions.
2. **Run the runner on the VM (binary copied out), not in a container.** Works without a docker CLI in the image and without path mapping, and was the 007 design. Rejected by the owner (decision 2).
3. **A new TOML (`.github/factory-ci-ci.toml`) naming targets and projects.** The watch list already holds repo, branches and `allow_docker`. A second file would hold the same repos twice. A `projects` key in the watch list would change the parser the tick uses (unknown keys are errors) and clash with the tick's tree walk. Rejected: the project is an input.
4. **Native Bitwarden client in Rust.** Needs access-token parsing, key stretching and AES-CBC-HMAC decryption, with no upstream tests and a high cost of a subtle mistake. `bws` is Bitwarden's own pinned binary. Rejected.
5. **docker-in-docker for the runner container.** Would need `--privileged`. The socket mount is what the owner chose, and its risk is handled in the security assessment. Rejected.
6. **Clone in a container (the `job` path, `FACTORY_CI_CLONE_IMAGE`).** Needs another pinned image and host-path mapping for a container that only runs `git`. The runner image has `git`. Rejected.
7. **Debian slim base.** Larger by about 60 MB, no gain: `bws` is a static musl binary and the runner is static musl. Alpine chosen.
8. **Multi-arch image.** Needs QEMU and an unverified aarch64 `bws` asset. Rejected for now (owner question Q1).

**Path mapping**: the runner container mounts `$RUNNER_TEMP/ci` at the identical path and the runner builds every workspace path under `--work` without canonicalising it to a different string. The host daemon resolves `-v <path>:/workspace` on the host, where the same path exists.

**Where each old shell duty went**

| Old shell step in `factory-monorepo-ci.yml` | New home |
| --- | --- |
| validate ref | `ci::parse_args` (FR-007) |
| install bws, pinned | `rust-ci-runner/Dockerfile` (FR-025) |
| fetch token, jq key lookup, shape check, masking | `store.rs` and the redactor (FR-009 to FR-011) |
| clone with header in env | `ci::clone` (FR-013) |
| fetch rust-ci-runner from image, `docker cp` | gone; `docker run` of the image |
| refuse docker steps with grep | `Pipeline::docker_problems` called by `ci` (FR-021) |
| Loki URL strip, probe | `ship::LogConfig::from_env`, `ship::probe` (FR-015, FR-016) |
| output filter with grep and sed | `Log` public mode (FR-018) |
| step summary | dropped (FR-005) |

## Contracts

### CLI

```text
rust-ci-runner ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]
```

Environment read: `BWS_ACCESS_TOKEN` (required), `FACTORY_CI_BWS_SERVER_URL` (optional, https), `GITHUB_RUN_ID` (optional, `valid_name`; else random), `FACTORY_CI_ENGINE` (docker or podman or a path, default docker), `FACTORY_CI_BWS` (path of `bws`, default `bws`), `FACTORY_CI_DIND_IMAGE` (only with `--allow-docker`). No other variable is read. `GITHUB_SERVER_URL` is ignored: the clone host is fixed.

Exit codes: 0 passed and delivered; 1 pipeline failed; 2 setup fault or refusal; 3 passed, a log line dropped.

### Public output (closed grammar)

stdout: `[<name>] started`, `[<name>] success in <N>s`, `[<name>] failed in <N>s`, `[<name>] skipped`, `[runner] pipeline passed`, `[runner] pipeline failed`, `[runner] log store answered HTTP <code>`, `[runner] log lines were dropped`.

stderr, `[runner] <text>` with text from this list only:

1. `bad arguments`
2. `repository or ref not in the watch list`
3. `BWS_ACCESS_TOKEN is not set`
4. `secret store unreachable`
5. `secret store rejected the access token`
6. `secret <KEY>: expected 1, found <N>` and `secret <KEY>: unusable value` (KEY is one of the four fixed names)
7. `log store check failed: check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)`
8. `cannot reach the container engine`
9. `clone failed`
10. `pipeline file refused`
11. `docker steps are refused`
12. `runner fault`
13. `internal error`

### Bitwarden

Keys, exact match, exactly one each: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. Command: `bws secret list -o json`, no project argument (the machine account sees one project, per 003 D26). Output: a JSON array of objects with `key` and `value`. Any other keys are ignored, and the count of keys seen is not printed. Cited from 003: `research.md` D26 (one token, one project), `contracts/bootstrap-secrets.md` (list call, shape rules, no retry on a rejected token), `factory-ci-secrets.sh` (`BWS_SHA256`).

### Loki

Push: `POST <url>/loki/api/v1/push`, JSON, basic auth. Labels: `repo` (target repo), `project`, `step`, `stream`. Probe labels: `repo`, `project=probe`, `step=probe`, `stream=stdout`. Line prefix `run=<id> sha=<short> branch=<ref>` as today. After the clone, one `ci` line carries the full checked-out SHA.

### Workflow to runner

```text
docker run --rm --name factory-ci-runner-$GITHUB_RUN_ID
  --user $(id -u):$(id -g) --group-add $(stat -c %g /var/run/docker.sock)
  --cap-drop ALL --security-opt no-new-privileges --read-only --tmpfs /tmp -e HOME=/tmp
  -e BWS_ACCESS_TOKEN -e GITHUB_RUN_ID -e FACTORY_CI_BWS_SERVER_URL
  -v /var/run/docker.sock:/var/run/docker.sock
  -v $RUNNER_TEMP/ci:$RUNNER_TEMP/ci
  -v $GITHUB_WORKSPACE/.github/factory-ci-repos.toml:/etc/factory-ci/repos.toml:ro
  $RUNNER_IMAGE ci --repos /etc/factory-ci/repos.toml --repo $REPO --ref $REF --project $PROJECT --work $RUNNER_TEMP/ci
```

`-e NAME` with no value copies the value from the step's environment, so argv never holds it. `FACTORY_CI_BWS_SERVER_URL` comes from `vars.`; when empty the runner treats it as unset.

### Image

Base `alpine:3.21@sha256:<digest>` (digest looked up in T012). Stage 1 downloads the `bws` zip, checks SHA-256 and `unzip -Z1`, extracts `bws`. Final stage: `apk add --no-cache git docker-cli ca-certificates`, `COPY` `bws` to `/usr/local/bin/bws`, `COPY amd64/rust-ci-runner /rust-ci-runner`, `USER 65532:65532`, `ENTRYPOINT ["/rust-ci-runner"]`. The binary stays at `/rust-ci-runner` because the tick's `fetch runner` step runs `docker cp "$id:/rust-ci-runner"`. Alpine packages come from the signed Alpine 3.21 repository at build time; their versions float inside the release, and the pin that matters is the image digest the workflow uses.

## Files to add or change

| File | Change |
| --- | --- |
| `.specify/memory/constitution.md` | 1.1.0 (done, T001) |
| `.specify/feature.json` | points to this folder |
| `rust-ci-runner/Cargo.toml` | version 0.2.0 (T011) |
| `rust-ci-runner/src/ship.rs` | URL normalisation in `LogConfig::from_env`; `pub fn probe(cfg, run_id) -> Result<u16, u16>`; shared `finish` rule (T002, T003) |
| `rust-ci-runner/src/ship_tests.rs` | tests for both |
| `rust-ci-runner/src/log.rs` | `Log::public_events(bool)`; event printing through a grammar function (T004) |
| `rust-ci-runner/src/engine.rs` | `command()` calls `env_remove("BWS_ACCESS_TOKEN")` (T005) |
| `rust-ci-runner/src/engine_tests.rs` (new) | stub-docker test of the child environment (T005) |
| `rust-ci-runner/src/store.rs` (new) | `fetch(bws_bin, token, server_url) -> Result<Secrets, StoreError>`, pure `parse_list(json)` (T006) |
| `rust-ci-runner/src/store_tests.rs` (new) | tests (T006) |
| `rust-ci-runner/src/watch.rs`, `watch_tests.rs` | `pub fn read(path) -> Result<Vec<WatchEntry>, String>` moved out of `main.rs` (T007) |
| `rust-ci-runner/src/job.rs`, `job_tests.rs` | `pub(crate)` on `prepare`, `run_pipeline`, `with_shipper`, `log_config`, `var`, `MAX_LOG_BYTES`; `project_dir(ws, project)` extracted from `checkout`; `local` returns an error when a passing run dropped lines (T007) |
| `rust-ci-runner/src/ci.rs` (new) | the command (T008) |
| `rust-ci-runner/src/ci_tests.rs` (new) | tests (T010) |
| `rust-ci-runner/src/main.rs`, `lib.rs` | `ci` dispatch, usage text, panic hook for `ci`, module lines (T009) |
| `rust-ci-runner/.factory-ci.toml` | add `--skip ci_tests::` and a fourth command (T011) |
| `rust-ci-runner/Dockerfile` | rewritten (T012) |
| `.github/workflows/rust-ci-runner-release.yml` | image step and comments (T013) |
| `.github/scripts/factory-ci-lint-workflow.sh` | `release-platforms` rule and its self-test row (T013) |
| `.github/factory-ci-repos.toml` | two entries (T014) |
| `.github/workflows/factory-monorepo-ci.yml` | cut to a trigger (T016) |
| `.github/scripts/test-factory-monorepo-ci.sh` (new), `.github/workflows/factory-ci-selftest.yml` | workflow shell test and its step (T017) |
| `README.md`, `rust-ci-runner/README.md` | docs (T019) |

## `ci` flow (ci.rs)

1. Parse and validate flags (FR-007). Panic hook installed by `main` first.
2. `watch::read`, find the entry, match `--ref` with `matches_branch`, gate `--allow-docker` on the entry. Fail with line 2 or 1.
3. Read `BWS_ACCESS_TOKEN`; empty means line 3.
4. `store::fetch`. Build `LogConfig` from the fetched map through the `Env` closure that `LogConfig::from_env` already takes: no `set_var`.
5. `ship::probe`. Print its line. Non-2xx means line 7, exit 2.
6. Start the `Shipper` (labels: target repo and project; meta: run id, empty sha, ref). From here on, fault detail also goes to Loki, redacted.
7. Engine check: `docker version`. Failure means line 8.
8. Clone into `<work>/ci-<runid>-<random>/src` with `git` (`env_clear`, then `PATH`, `HOME=/tmp`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, `GIT_TERMINAL_PROMPT=0`, `GIT_ALLOW_PROTOCOL=https`, `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=http.https://github.com/.extraHeader`, `GIT_CONFIG_VALUE_0=Authorization: basic <b64>`). Argv: `clone -q --depth 1 --no-tags --single-branch --branch <ref> -- <url> <dir>`. Then `git rev-parse HEAD` for the Loki line and `FACTORY_CI_SHA`.
9. `project_dir`, then `prepare(dir, None, |_| None, allow_docker)`. A docker problem is line 11; any other problem is line 10, detail to Loki.
10. `run_pipeline` with a `Log` in quiet and public mode plus the shipper sink. `run_env(..., "manual")`.
11. Print `pipeline passed|failed`, `engine.cleanup_run(id)`. The work directory stays (shortcut: ephemeral hosted VM; add removal before a persistent host runs this).
12. `Shipper::finish(FLUSH_WINDOW)`. Compute the exit code (FR-008).

## Test plan (qa runs in the `rust-ci-runner` pipeline; names are the contract)

| Requirement | Test (module) |
| --- | --- |
| FR-015 | `url_loses_the_push_path_and_slashes`, `url_normalisation_is_idempotent`, `url_with_another_path_is_kept` (ship_tests) |
| FR-016 | `probe_204_is_ok`, `probe_401_reports_the_code`, `probe_with_no_answer_reports_000_in_time`, `probe_posts_one_line_with_probe_labels_and_basic_auth` (ship_tests) |
| FR-017 | `run_quiet_logs_fails_when_a_passing_run_dropped_lines` (job_tests), `dropped_lines_exit_3`, `failed_pipeline_with_dropped_lines_exits_1` (ci_tests) |
| FR-018 | `public_mode_prints_only_the_closed_grammar`, `public_mode_drops_the_reason_after_failed_and_skipped`, `public_mode_ships_the_full_text`, `public_mode_refuses_a_bad_name` (log) |
| FR-009, FR-010 | `one_secret_per_key_is_returned`, `zero_or_two_matches_name_key_and_count_only`, `value_shapes_are_refused`, `token_goes_in_env_not_argv`, `rejected_token_is_not_retried`, `unreachable_is_retried_once`, `slow_bws_times_out`, `server_url_must_be_https`, `garbage_output_is_unreadable_and_not_echoed` (store_tests) |
| FR-012 | `bws_token_is_absent_from_engine_children` (engine_tests) |
| FR-006, FR-007 | `ci_requires_all_flags`, `ref_table`, `project_table`, `work_table`, `repo_outside_the_watch_list_exits_2_before_bws`, `ref_outside_branches_exits_2` (ci_tests) |
| FR-013, FR-014 | `clone_uses_the_fixed_host_and_an_env_header`, `clone_token_is_not_in_argv_or_git_config`, `symlinked_pipeline_file_is_refused`, `symlinked_project_is_refused` (ci_tests) |
| FR-019 | `toml_error_prints_the_fixed_line_and_ships_the_detail`, `panic_prints_internal_error_only`, `every_stderr_line_is_in_the_fixed_list` (ci_tests) |
| FR-021, FR-022 | `docker_step_is_refused_before_any_container`, `allow_docker_needs_the_entry_and_a_pinned_dind_image`, `declared_secret_fails_before_any_container` (ci_tests) |
| FR-023 | `engine_down_exits_2_before_the_clone` (ci_tests) |
| FR-024 | `mount_source_equals_the_work_path`, `step_argv_has_no_socket_mount` (ci_tests) |
| FR-009 | `missing_access_token_exits_2_without_calling_bws` (ci_tests) |
| FR-011 | `ci_leaves_the_process_environment_unchanged` (ci_tests) |
| FR-022, run fault | `engine_fault_during_run_exits_2_with_runner_fault` (ci_tests) |
| FR-008 | `exit_code_table`, `passing_run_exits_0`, `failing_step_exits_1_without_reason` (ci_tests) |
| FR-011, SC-002 | `no_bootstrap_value_in_stdout_stderr_argv_or_child_env`, `hostile_fixture_prints_only_allowed_lines` (ci_tests) |
| FR-007 shared | `read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref` (watch_tests) |
| FR-001 to FR-005 | `.github/scripts/test-factory-monorepo-ci.sh`: exactly one `docker run`; no `bws`, `jq`, `curl`, `git clone`, `grep -E`; `secrets.` names only `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN`; no `${{` in any `run:`; `refs/heads/main` guard; `environment: factory-ci`; every `uses:` pinned; `-e BWS_ACCESS_TOKEN` without `=`; no `cargo`; the `run:` block at most 15 lines. Runs in the `factory-ci-selftest` job `shell`. |
| FR-025, FR-026 | the lint `release-platforms` rule and its self-test row; the release run itself (Rollout step 3) |
| FR-027 | the `rust-ci-runner` pipeline passing on `main` (Rollout step 2) |

Acceptance scenarios to tests: US1.1 and US1.2 `passing_run_exits_0`, `failing_step_exits_1_without_reason`; US1.3 the shell test (main guard); US1.4 the first run T018 (a workflow run, not a unit test); US1.5 `repo_outside_the_watch_list_exits_2_before_bws`, `ref_outside_branches_exits_2`; US2.1 to US2.5 `hostile_fixture_prints_only_allowed_lines`, `public_mode_drops_the_reason_after_failed_and_skipped`, `toml_error_prints_the_fixed_line_and_ships_the_detail`, `panic_prints_internal_error_only`, `no_bootstrap_value_in_stdout_stderr_argv_or_child_env`; US3.1 to US3.4 the `url_*`, `probe_*` and `dropped_lines_exit_3` tests; US4.1 to US4.3 `docker_step_is_refused_before_any_container`, `declared_secret_fails_before_any_container`, `step_argv_has_no_socket_mount`; US5.1 the release run and the lint; US5.2 the Rollout gate.

Stubs: `bws`, `docker` and `git` are `sh` scripts written into a temp dir (the pattern `job_tests.rs` uses for the engine), selected through `FACTORY_CI_BWS`, `FACTORY_CI_ENGINE` and a `git` path seam on `ci::run`. The clone seam takes the base URL as a parameter, so tests clone from a local `file://` repo; `ci()` itself passes the constant `https://github.com`. Loki is `tiny_http` as in `ship_tests.rs`.

Test process split (the step container allows 512 processes and mock-server threads never exit): the `ci_tests::` module has its own `cargo test` command, like `ship_tests::` and `job_tests::`. `store_tests`, `engine_tests` and `watch_tests` run in the main process because they use only stub scripts, not servers.

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
| --- | --- | --- | --- | --- |
| Args | missing flag, bad ref, project or work | `parse_args` | exit 2, line 1, nothing else done | `ci_requires_all_flags`, `ref_table` |
| Watch gate | repo or ref not listed; file unreadable or invalid | `watch::read`, `matches_branch` | exit 2, line 2 | `repo_outside_the_watch_list_exits_2_before_bws` |
| Token | `BWS_ACCESS_TOKEN` unset | env | exit 2, line 3, no `bws` call | `missing_access_token_exits_2_without_calling_bws` |
| Bitwarden | down, slow, rejects token, garbage JSON | exit status, timeout, parse | retry once unless rejected; exit 2, lines 4 to 6 | `store_tests` |
| Bitwarden | 0 or 2 matches, bad value shape | `parse_list` | exit 2, line 6, key and count | `store_tests` |
| Probe | URL wrong, user wrong, token lacks scope, timeout | HTTP code | exit 2, line 7, no step runs | `probe_*` |
| Engine | socket missing, group missing, daemon down | `docker version` | exit 2, line 8, before clone | `engine_down_exits_2_before_the_clone` |
| Clone | ref missing, token lacks access, network, timeout | git status | exit 2, line 9, git text to Loki redacted | `clone_*` |
| Pipeline file | TOML error, bad field, link, too large | `read_file`, `parse` | exit 2, line 10, detail to Loki | `toml_error_prints_the_fixed_line_and_ships_the_detail` |
| Docker refusal | `docker = true` | `docker_problems` | exit 2, line 11, no container | `docker_step_is_refused_before_any_container` |
| Secrets | step lists `secrets` | `resolve_secrets` | exit 2, line 10 | `declared_secret_fails_before_any_container` |
| Run | step fails or times out | `Outcome` | exit 1, `failed in Ns`, no reason | `failing_step_exits_1_without_reason` |
| Run | network create fails, sidecar fault | `execute` Err | exit 2, line 12 | `engine_fault_during_run_exits_2_with_runner_fault` |
| Shipping | Loki dies after the probe, 401 mid-run | `Shipper::finish` | exit 3 (or 1), `log lines were dropped` | `dropped_lines_exit_3` |
| Panic | any bug | panic hook, `catch_unwind` | exit 2, line 13 | `panic_prints_internal_error_only` |
| Mount | path mismatch between runner and host | first step sees an empty `/workspace` | step fails fast; detail in Loki | `mount_source_equals_the_work_path`, quickstart check |
| Pull | digest not published | `docker run` pull error | job fails before the runner | Rollout gate |
| Release | `bws` hash mismatch, apk or base unavailable | Docker build | no image published | release run |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. **The step containers see an empty workspace.** The runner container and the host disagree on a path (symlinked `RUNNER_TEMP`, a typo in `-v`). Prevention: identical-path mount, no canonicalisation to a different string, `mount_source_equals_the_work_path`, and the first run in Rollout reads a file from the clone inside a step.
2. **`permission denied` on the docker socket, or `bws` cannot write.** The container user lacks the socket group, or `--read-only` blocks `bws`'s state file. Prevention: `--group-add $(stat -c %g ...)`, `HOME=/tmp` on a tmpfs, the engine check that exits 2 with line 8, and the first run in Rollout. If `bws` needs another path, the fix is an env var in the `docker run`, not a design change.
3. **The logs still do not reach Grafana.** A cause other than the URL (token scope, wrong user id, region). Prevention: URL normalisation, the probe that fails the run before any step, exit 3 for later drops, and the fixed hint that names the three values to check.
4. **A private detail leaks into the public log.** A reason, a TOML error, a git message, a panic. Prevention: stdout allowlist in `Log`, stderr from a closed list, panic hook, the hostile fixture test, and a rule that no `ci` code path prints an error string it did not write.
5. **The trimmed workflow lands before its image exists.** The pull fails and the owner cannot run anything. Prevention: bootstrap order (two pull requests, the workflow one only after the digest is verified by a real pull), and the old workflow plus the 0.1.0 image stay valid until then.

## Invariants (qa turns each into a property test)

- I1. For any step names, step output, failure reasons and pipeline text, every stdout line of `ci` matches the closed grammar of FR-018 and every stderr line is in the fixed list.
- I2. None of the five bootstrap values, nor the base64 of any of them, appears in stdout, stderr, the argv of any child process, or the environment of any `docker` child.
- I3. `normalise(normalise(u)) == normalise(u)`; the result never ends with `/` or `/loki/api/v1/push`.
- I4. The exit code is 0 only when the pipeline passed and no log line was dropped.
- I5. No container starts before the engine check, and no step container starts when a step has `docker = true` without both opt-ins, or declares a secret.
- I6. The `-v` source of every container the runner starts is a path under `--work`, byte for byte.
- I7. An accepted `ref` matches `[A-Za-z0-9._/-]{1,200}`, does not start with `-` and does not contain `..`.
- I8. For any secret list, a key with a match count other than 1 is an error and no value is returned.
- I9. `BWS_ACCESS_TOKEN` is absent from the environment of every `Engine` child, for any input.
- I10. No step container's argv contains `docker.sock`.

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
| --- | --- |
| Inputs: empty, one, max, over | handled: `ref` 1 to 200, project name 1 to 100, key counts 0, 1, 2 (`ref_table`, `project_table`, `zero_or_two_matches_name_key_and_count_only`); pipeline file at the 256 KB cap uses the existing limit |
| Inputs: duplicates | handled: two secrets with one key refuse the run; two runs with one input queue (workflow concurrency) |
| Inputs: unicode, NUL, newline | handled: values with CR, LF, NUL refused (`value_shapes_are_refused`); refs outside the ASCII set refused; step output control characters already replaced in `Log` |
| Inputs: malformed or truncated | handled: bad JSON from `bws`, bad TOML, bad watch list each exit 2 with a fixed line |
| State: first run, older data | N/A: no stored state; the work dir is new per run |
| State: crash mid-run | handled: containers carry the run label and `cleanup_run` runs at the end; a killed VM discards the rest |
| State: partial failure | handled: probe passes then Loki dies gives exit 3; clone fails after the probe leaves only the Loki line |
| State: retry | handled: a rerun starts a fresh directory and fresh containers (random id) |
| Time: two callers | handled: workflow concurrency per repo, ref and project; the work dir name has a random part |
| Time: timeouts | handled: bws 60 s x2, probe 15 s, clone 300 s, run 30 min, job 35 min, Loki flush 60 s |
| Time: clock, expiry | N/A: the probe timestamp is `now`; no expiry logic |
| Time: ordering | handled: the Shipper sorts a batch by timestamp (existing) |
| Resources: disk full | handled: log write errors are dropped by design (existing shortcut), the run result stays; the VM is discarded |
| Resources: large input | handled: log cut at 10 MB, queue cap 32 MB, pids limit 512 (existing) |
| Resources: slow or garbage dependency | handled: Bitwarden, Loki, git and docker each have a limit and a fixed failure line |
| Resources: unbounded growth | handled: nothing grows; one run per VM |
| Resources: smallest machine | N/A: a hosted runner; the 1 GB planning VM runs nothing |
| Permissions: who calls | handled: `workflow_dispatch` on `main`, Environment `factory-ci`; the repo must be in the watch list |
| Permissions: what the caller must not see | handled: closed output grammar, values in memory, none in argv or step containers |
| Permissions: client values trusted by mistake | handled: `repo`, `ref`, `project` validated; clone host fixed; `GITHUB_SERVER_URL` ignored; `.factory-ci.toml` treated as untrusted |
| UI: states | N/A: no UI; the Actions log is the interface and its states are the exit codes |

## Rollout and rollback

Two pull requests, both linking `specs/008-runner-owned-ci/`.

1. **PR-A** (T001 to T014): runner, image, release workflow, lint, watch list, constitution. The old `factory-monorepo-ci.yml` and its 0.1.0 pin stay. Pre-merge checks: `factory-ci-selftest` (compile, fixture pipeline, watch list check, workflow lint). The new unit tests cannot run before the merge: the manual workflow runs from `main` only and tests `main`.
2. After the merge, start the old workflow with `pipeline = rust-ci-runner`. The 0.1.0 binary runs the new unit tests. Red means revert PR-A or fix forward; no tag yet.
3. Push the tag `rust-ci-runner-v0.2.0` on a commit on `main`. The release publishes the image and prints the digest. Verify with a real `docker pull ghcr.io/sudtanj/rust-ci-runner@sha256:<digest>` and `docker run --rm --entrypoint docker <image> --version` (T015).
4. **PR-B** (T016, T017, T019): trimmed workflow with that digest, shell test, README. Merge only after step 3 verified.
5. First runs (T018): `sudtanj/infrastructure` project `rust-ci-runner`, then `sudtanj/factory-monorepo` project `.`. Check Loki shows the lines and the log shows only the grammar.

Rollback: before step 4, nothing user-facing changed. After it, revert PR-B; the old workflow with the 0.1.0 digest returns. Never delete the 0.1.0 package version. A bad 0.2.0 image is fixed by a 0.2.1 tag and a new digest line.

## Complexity Tracking

| Cost | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| A second subcommand path next to `run` and `job` | The owner wants one command that owns the job | Extending `run` would mix local-dir semantics with fetch, clone and a closed output mode |
| A new module `store.rs` that calls `bws` | The token must leave the workflow | A native client is more code with more risk (alternative 4) |
| Docker socket mounted in the runner container | Owner decision 2 | See the security assessment; the only mitigations are containment, not removal |

## Project Structure

```text
specs/008-runner-owned-ci/{spec.md,plan.md,tasks.md,checklists/requirements.md}
rust-ci-runner/src/{ci.rs,ci_tests.rs,store.rs,store_tests.rs,engine_tests.rs}   # new
rust-ci-runner/src/{ship,log,engine,job,watch,main,lib}.rs                       # changed
rust-ci-runner/{Dockerfile,Cargo.toml,.factory-ci.toml}                          # changed
.github/workflows/{factory-monorepo-ci,rust-ci-runner-release}.yml               # changed
.github/scripts/{factory-ci-lint-workflow.sh,test-factory-monorepo-ci.sh}        # changed, new
.github/factory-ci-repos.toml
```

**Structure Decision**: no new directory. Tests sit beside the code as `*_tests.rs`, as the repo does today.

## DevOps assessment

Written by the engineering manager. The `devops` sub-agent was not available in this session (no agent tool), so this is a self-review against the same questions; ask the real role to review before T013 and T016.

Findings and the change each caused:

1. Job limit 30 min equals the runner deadline, so the job would be cancelled before the runner reports. Job limit is 35 min (FR-001).
2. `--read-only` with a tmpfs and `HOME=/tmp` is needed for `bws` and `git`; the first run proves it. Added to the pre-mortem and T018.
3. The image digest bump is the only manual release step; `release` already prints it. Kept as one env line (`RUNNER_IMAGE`) so the shell test can read it.
4. The release lint hard-codes `platforms: linux/amd64,linux/arm64`. Changing the image to amd64 breaks it, so T013 edits the rule and its self-test row in the same change.
5. A build with `RUN apk add` for arm64 would need QEMU. Avoided by building amd64 only.
6. Alpine packages float within the release. Accepted; the workflow pins the resulting image digest. Package versions could be pinned with `apk add git=<ver>` but Alpine removes old versions from the index, which breaks rebuilds.
7. The bootstrap order is two pull requests, because the release job requires the tag on `main` and the workflow needs the digest.
8. No `Cargo.lock` on `main` still means dependencies float at release build time (existing shortcut, unchanged).
9. Adding two entries to the watch list makes the by-hand tick plan them (see risks).
10. The Environment `factory-ci` rule (main only) must stay; `if: github.ref` guards the job but the Environment guards the secret.
11. Work directory cleanup is skipped on purpose (ephemeral VM); the plan marks it a shortcut.

## Security assessment

Written by the engineering manager. The `security` sub-agent was not available in this session, so this is a self-review; ask the real role to review T008, T012 and T016 before merge. Each finding names what the plan does.

1. **Docker socket mount.** Anything that controls the runner container controls the VM. The runner image holds only the runner, `git`, `docker`, `bws`. The pipeline file never reaches the socket: its commands run in step containers that get no socket, no bootstrap value and fixed flags (`--pids-limit 512`, `no-new-privileges`, dropped caps). `docker = true` is refused by default and needs both `--allow-docker` and `allow_docker` in the watch entry; the workflow passes no flag. The runner container itself runs as the runner user with `--cap-drop ALL`, `no-new-privileges` and a read-only root. Residual risk: a bug in the runner's own argument building. `step.image` starting with `-` is already refused, and `ci_tests` adds an argv test that no step argv holds a user-controlled flag.
2. **`BWS_ACCESS_TOKEN` reaching step containers or logs.** The token enters the runner container as an environment variable (`-e NAME`, so not in the `docker run` argv; it is visible to `docker inspect` of that container, which only the runner could run). The runner passes it to the `bws` child only, and `Engine::command` removes it from every docker child, so step containers never get it (I9). It joins the redactor list. Not in argv anywhere. The step's `GITHUB_TOKEN` is a different value with `contents: read` and `packages: read`.
3. **Argv exposure.** Clone token: git environment, scoped to `https://github.com/`, not in argv and not in `.git/config`. Loki credentials: `ureq` in process. `bws` token: child environment. `docker login`: stdin. The `docker run` argv holds repo, ref, project and paths, none secret.
4. **Pinned image.** The workflow references `tag@sha256:digest`. The release builds the base by digest and checks `bws` by SHA-256 (the 003 pin) before use. Alpine packages float inside a release (accepted, finding 6 above). The owner bumps the digest by pull request, so the change is reviewed.
5. **Malicious `.factory-ci.toml` in a branch.** It can run commands in a step container with network access, 30 minutes, 512 processes, and no secrets. It cannot read the bootstrap values, reach the socket, print to the public log (quiet mode, allowlist) or reach Loki's credentials. Controls: `docker` refusal, secrets refusal, no symlinked pipeline file or project folder, 256 KB cap, step names limited to `[A-Za-z0-9_-]` so they cannot carry `::` workflow commands, image names starting with `-` refused. The step network can reach the VM's metadata endpoint, as any job container on a hosted runner can; nothing sensitive is stored there for this workflow. Residual: a malicious branch can spend runner minutes. The Environment limits who can start a run to the owner.
6. **Clone.** Host fixed to `github.com`. `GIT_ALLOW_PROTOCOL=https`, no submodules, no tags, `env_clear`, empty global and system config, header scoped by URL so a redirect to another host does not receive it. The clone token only needs read access.
7. **Public log.** Closed grammar on stdout, fixed list on stderr, panic hook, no foreign error text. The `docker run` step of the workflow can print daemon errors on a failed start; they hold the public image name and paths only.
8. **Input trust.** `repo` must be in the committed watch list, so a user with dispatch rights cannot name an arbitrary repo; `ref` and `project` are validated before any secret is read.
9. **Bitwarden scope.** The machine account reads only project `rust-ci-runner`. A second project with the same keys makes the run fail as ambiguous, not pick one.

## Open questions for the owner

- **Q1 Image architectures.** Default: `linux/amd64` only; the arm64 binary stays a release asset. Alternative: keep multi-arch, which needs QEMU in the release and a verified aarch64 `bws` asset and hash.
- **Q2 `factory-ci-selftest.yml` compiles the runner on pull requests** (`cargo build --release`, two jobs). Decision 3 says only the release workflow compiles it. Default: leave it as the pre-merge compile check and say so; this feature does not touch it. Alternatives: run those jobs through the released image (tests the old binary, not the pull request), or delete them.
- **Q3 Watch list side effect.** The two new entries (`sudtanj/factory-monorepo`, `sudtanj/infrastructure`) also make the by-hand tick plan and post statuses to them. Default: accept, since the tick is not in use. Alternative: a second list file for `ci` only (more files, the owner asked to reuse).

## Open risks

- The new unit tests first run after merge (workflow runs from `main` only). Mitigation: no tag until the `rust-ci-runner` pipeline is green on `main`.
- Pin values (Alpine digest, apk packages' availability) are looked up by the engineer in T012 with network access that the planning machine did not have. Unverified until the release run.
- No `Cargo.lock`: a dependency bump can break the release build. Existing shortcut.
- `bws` state or config path inside a read-only container is untested until the first run.
- The metadata endpoint is reachable from step containers.
- Hosted runner changes (docker group id, socket path) break `--group-add`; the engine check turns that into a clear line 8.

## Step plan and engineer briefs

Every brief: work only the listed task IDs of `specs/008-runner-owned-ci/tasks.md`; read the named files first; no `cargo`, `docker` or builds on the engineer's machine (it has 1 GB); check with `bash -n`, grep and reading; commit only the named paths with `git add <paths>`; commit to the feature branch named by the lead; never print a secret value; prose follows `stop-slop`; code follows the surrounding style (see `rust-ci-runner/src/job.rs`). Rust edits must pass `cargo fmt` and `cargo clippy -D warnings` style by eye: no unused items, no `unwrap` on input.

**Parallel groups.** Group 1, all in parallel: B1 (T002, T003), B2 (T004), B3 (T005), B4 (T006), B5 (T007), B6 (T012), B7 (T013), B8 (T014). Conflicts to expect: `lib.rs` gets one `mod` line from B3 and from B4 (union the lines). Group 2, after B1 to B5: B9 (T008 to T011) alone. Gate G (T015) is the owner's. Group 3, after the gate: B10 (T016, T017). Last: B11 (T019). T018 is the owner's.

**B1 (T002, T003): Loki URL and probe.**
- Goal: `LogConfig::from_env` strips `/` then `/loki/api/v1/push` then `/` from the URL; `ship::probe(cfg, run_id) -> Result<u16, u16>` posts one line and returns the 2xx code or the failing code (0 when no answer within 15 s); the printed code is zero-padded to 3 digits (`000`).
- Files: `rust-ci-runner/src/ship.rs`, `rust-ci-runner/src/ship_tests.rs`.
- Read: `ship.rs` (`LogConfig::from_env`, `Shipper::send`), `ship_tests.rs` (`loki`, `ok_loki`, `from`).
- Accept: the seven `url_*` and `probe_*` tests of the test plan exist and would pass; probe uses `ureq` with a 15 s timeout, `redirects(0)`, basic auth, labels `repo`, `project=probe`, `step=probe`, `stream=stdout`; the existing tests still hold (the trailing-slash test keeps its expectation); no credentials in any error string.
- Constraints: no new crate; `from_env` keeps naming variables, never values, in errors; the https and no-userinfo checks run on the normalised URL.

**B2 (T004): public events in `Log`.**
- Goal: `Log::public_events(bool)`. When on, `event` prints only text that matches the grammar of FR-018 with the reason cut, and prints nothing for any other text or for a `who` outside `[A-Za-z0-9_-]{1,64}`; the sink always gets the full original text.
- Files: `rust-ci-runner/src/log.rs` (its in-file `tests` module).
- Read: `log.rs` whole, `engine.rs` lines 440 to 495 (the event texts).
- Accept: the four `public_mode_*` tests; default behaviour of `Log` unchanged (existing tests pass); a pure function `public_form(who, text) -> Option<String>` is the only place the grammar lives.
- Constraints: the size-limit accounting stays on the full text; no regex crate.

**B3 (T005): no `BWS_ACCESS_TOKEN` in engine children.**
- Goal: `Engine::command` calls `env_remove("BWS_ACCESS_TOKEN")`.
- Files: `rust-ci-runner/src/engine.rs`, new `rust-ci-runner/src/engine_tests.rs`, one line in `lib.rs`.
- Accept: `bws_token_is_absent_from_engine_children`: a stub engine script writes its environment to a file; the test sets the variable for the call and finds it absent; other variables still pass through.
- Constraints: the test must not call `set_var` in a way that races other tests: set it on a `Command` that wraps the call, or pass it through `Engine::capture`'s `env` argument and prove `env_remove` wins only for the inherited value. Pick the approach that does not mutate the process environment.

**B4 (T006): `store.rs`.**
- Goal: `store::fetch(bws_bin: &str, token: &str, server_url: Option<&str>) -> Result<Secrets, StoreError>` with `Secrets { token, log_url, log_user, log_token }`; pure `parse_list(json: &str) -> Result<Secrets, StoreError>`; `StoreError` variants map to the fixed lines 4 to 6 and carry key names and counts only.
- Files: new `rust-ci-runner/src/store.rs`, `store_tests.rs`, `lib.rs` lines.
- Read: `engine.rs` `capture` (reuse `Engine::new(bin).capture(args, env, limit, None)`), spec FR-009 to FR-011, 003 `contracts/bootstrap-secrets.md` steps 2 and 5.
- Accept: the nine `store_tests` above; the token goes only in the child's environment (stub records argv and environment); a rejected token (stderr matches 400, 401, 403, `invalid_client`, `unauthorized`, `forbidden`, `invalid access token`) is not retried; any other failure is retried once; `FACTORY_CI_BWS_SERVER_URL` must start with `https://`; error values never contain the stub's stderr or stdout text.
- Constraints: `serde_json` for parsing; `Secrets` has no `Debug` that prints values (implement `Debug` by hand or omit); keep values out of panics.

**B5 (T007): shared edits in `watch.rs` and `job.rs`.**
- Goal: `watch::read(path)` (file read capped at `MAX_BYTES`, UTF-8, parse) used by `main.rs::load_list`; in `job.rs` make `prepare`, `run_pipeline`, `with_shipper`, `log_config`, `var`, `MAX_LOG_BYTES` `pub(crate)`, extract `project_dir(ws, project) -> Result<PathBuf, String>` from `checkout` (same checks: real directory, canonical path inside `ws`), and make `local` return `Err("log shipping dropped lines")` when the pipeline passed but `finish` returned false (print to stderr only that text).
- Files: `rust-ci-runner/src/watch.rs`, `watch_tests.rs`, `job.rs`, `job_tests.rs`, `main.rs` (only `load_list`).
- Accept: `read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref`, `run_quiet_logs_fails_when_a_passing_run_dropped_lines` (job_tests, with a stub engine and a Loki that answers 500 for ever and a short retry window via `Timing`, or a store that is closed); every existing test in those files passes unchanged except where this task changes the documented behaviour of `local`.
- Constraints: behaviour of `job` unchanged; `load_list` keeps its `LogConfig::from_env` check.

**B6 (T012): Dockerfile.**
- Goal: the image of the plan's Image section.
- Files: `rust-ci-runner/Dockerfile`.
- Steps: look up the digest of `alpine:3.21` for `linux/amd64` from the registry (for example `docker buildx imagetools inspect` on a machine that can, or the registry API with `curl`); write it as `alpine:3.21@sha256:<digest>`. Keep the `bws` URL `https://github.com/bitwarden/sdk-sm/releases/download/bws-v2.1.0/bws-x86_64-unknown-linux-musl-2.1.0.zip` and SHA-256 `f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6`; check `unzip -Z1` lists exactly `bws`.
- Accept: the file builds in the release job (not here); comments say why each tool is there and that the binary path `/rust-ci-runner` stays for the tick; no `latest` tag; no `curl | sh`.
- Constraints: if the machine has no network, stop and report the missing digest; do not guess one.

**B7 (T013): release workflow and lint.**
- Goal: the `publish` job builds `linux/amd64` only from the new Dockerfile, keeps release assets for both architectures and the `RUNNER_IMAGE_DIGEST` summary line; header comment updated.
- Files: `.github/workflows/rust-ci-runner-release.yml`, `.github/scripts/factory-ci-lint-workflow.sh` (rule `release-platforms` and its self-test row near line 72 and 223).
- Read: the lint script fully before editing; it checks the release workflow.
- Accept: `bash -n` on the lint script; the rule now expects the new `platforms:` string and the self-test row mutates that string; every `uses:` stays pinned to a 40-hex commit; `permissions` unchanged.
- Constraints: no new action; the build job stays as 007 left it.

**B8 (T014): watch list entries.**
- Goal: two `[[watch]]` tables: `sudtanj/factory-monorepo` and `sudtanj/infrastructure`, default `branches` (all), no `allow_docker`.
- Files: `.github/factory-ci-repos.toml`.
- Accept: `rust-ci-runner plan --repos .github/factory-ci-repos.toml --check` would report `2 entries ok` (the selftest job `list` runs it); the file header comment says `ci` reads `repo`, `branches`, `allow_docker` and ignores the rest.
- Constraints: no `id`, no `secrets`; keep the existing comments.

**B9 (T008 to T011): the `ci` command.**
- Goal: `ci.rs`, `ci_tests.rs`, `main.rs` and `lib.rs` wiring, the usage text, the panic hook, `.factory-ci.toml`, version 0.2.0.
- Files: new `rust-ci-runner/src/ci.rs`, `ci_tests.rs`; `main.rs`, `lib.rs`, `rust-ci-runner/.factory-ci.toml`, `rust-ci-runner/Cargo.toml`.
- Read: this plan (flow, contracts, test plan, invariants), spec FR-006 to FR-024, `job.rs`, `ship.rs`, `store.rs`, `log.rs`, `engine.rs`.
- Accept: every test in the `ci_tests` rows of the test plan exists; `ci::run(args, env, seams) -> i32` takes the environment as a closure and the `bws` path, engine path, `git` path and clone base URL as parameters, and `ci()` in `main.rs` passes the real values; every stderr line comes from a `const` list; the redactor holds all five bootstrap values; the pipeline file's four commands match the plan; the `.factory-ci.toml` comment explains the fourth process.
- Constraints: no `set_var`; no `unwrap` on input; never format an error from git, bws, docker or TOML into stdout or stderr; Loki gets the detail through `Log::out` on stream `stderr` for step name `ci`.

**B10 (T016, T017): the trimmed workflow and its shell test.**
- Goal: `factory-monorepo-ci.yml` as FR-001 to FR-005 say, with `RUNNER_IMAGE` set to the digest from the release; `.github/scripts/test-factory-monorepo-ci.sh` with the checks named in the test plan, wired into the selftest job `shell`.
- Files: `.github/workflows/factory-monorepo-ci.yml`, `.github/scripts/test-factory-monorepo-ci.sh`, `.github/workflows/factory-ci-selftest.yml`.
- Accept: `bash -n` on the new script and on the extracted `run:` block; the YAML parses; the script fails on a mutated copy for each rule (include a `--self-test` that mutates a temp copy, as the lint does); no `${{` in a `run:`.
- Constraints: start only after the lead gives the verified digest; no placeholder in the merged file; keep the header comment with the owner exception, rewritten.

**B11 (T019): docs.**
- Goal: `README.md` section "Run factory-monorepo CI by hand" rewritten for the new inputs and flow; the secrets and variables list; `rust-ci-runner/README.md` command list (`ci`), the image contents and the release section; the repository description checked.
- Files: `README.md`, `rust-ci-runner/README.md`.
- Accept: every claim matches the merged workflow; states whether the GitHub repository description changes (and the new text if so, for the owner to apply with `gh repo edit`); no em dashes, no filler.
- Constraints: last task; load `stop-slop` first.
