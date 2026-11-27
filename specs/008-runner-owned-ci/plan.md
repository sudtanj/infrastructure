# Implementation Plan: Runner-owned CI

**Branch**: `008-runner-owned-ci` | **Date**: 2026-11-27 | **Spec**: [spec.md](spec.md)

**Input**: Feature specification from `/specs/008-runner-owned-ci/spec.md`

## Summary

Add a `ci` subcommand to `rust-ci-runner` that does what the workflow did in shell: validate inputs against the watch list, fetch four values from Bitwarden with a pinned `bws`, probe Loki, check the repo identity, fetch the target ref with `git`, refuse docker steps, run the pipeline and print only a closed set of event lines. Rebuild the runner image on Alpine with a docker CLI, `git`, CA certificates and `bws`, multi-arch, so the workflow can `docker run` it with the host socket and the work directory mounted at the same path. Cut `factory-monorepo-ci.yml` to a checkout and one step with one `docker run`. No research, data-model or quickstart files: the spec and the contracts below fix the design, and the verification steps sit in Rollout.

Rebased on `main` after feature 009: `ship.rs` has `SERVICE_NAME = "factory-ci"` and `Shipper::body` emits five labels (`service_name`, `repo`, `project`, `step`, `stream`). The probe moves into the runner and builds its body through the same function, so it carries `service_name` with no second hand-built JSON.

## Technical Context

**Language/Version**: Rust 1.90 (edition 2021, `rust-toolchain.toml`), GitHub Actions YAML, POSIX sh, a Dockerfile

**Primary Dependencies**: no new crates. Existing: `serde`, `serde_json`, `toml`, `ureq`, `getrandom`; dev: `tiny_http`. New in the image: Alpine `git`, `docker-cli`, `ca-certificates`; `bws` 2.1.0.

**Storage**: files under `--work` (`$RUNNER_TEMP/ci`) on the hosted VM, discarded with it

**Testing**: `cargo test --lib`, run through the `rust-ci-runner` pipeline of the released image. Nothing compiles or runs on the planning or the engineer's machine (1 GB VM, no docker): engineers write code and tests and check with `bash -n`, grep and review. Pull requests return 403, so both PRs land by direct push; `factory-ci-selftest` gains a `push` trigger for non-`main` branches so it runs before the merge (T014).

**Target Platform**: GitHub-hosted `ubuntu-24.04` (amd64) with Docker; image `linux/amd64` and `linux/arm64`

**Project Type**: CLI plus CI workflow plus container image

**Performance Goals**: probe under 15 s per attempt; image pull under 30 s (about 100 MB); Bitwarden call under 2 x 60 s

**Constraints**: public repo and public logs; the runner holds a root-equivalent socket; no secret in argv; one source of truth for each pin

**Scale/Scope**: two new module pairs (`ci`, `store`), edits to `ship`, `log`, `engine`, `job`, `watch`, `pipeline`, `main`; one Dockerfile; three workflow files; two shell scripts; `Cargo.lock`; two READMEs

## Constitution Check

*Gate: pass before Phase 0. Re-checked after design.*

- I One home: workflow in `.github/workflows/`, scripts in `.github/scripts/`, runner in `rust-ci-runner/`, targets in `.github/factory-ci-repos.toml`. Pass. The watch list is reused, not duplicated.
- II Secrets source: amendment 1.1.0 (T001, done) lets the manual workflow hold only `BWS_ACCESS_TOKEN`. No value is committed. Pass.
- III Public logs: closed output grammar behind one choke point (`Log::emit`), fixed fault lines, panic hook, reasons to Loki only, no argv secrets. Pass (I1, I2).
- IV Simplicity: one subcommand, one new store module, no new crate. Complexity Tracking lists the costs.
- V Tested: each requirement maps to a lib test or the workflow shell test. The new `ci` tests first run in the `rust-ci-runner` pipeline after the merge; before it, the selftest type-checks them with `cargo test --lib --no-run`. Stated, not hidden.
- VI Spec Kit: this folder. Pass.
- "Only the release workflow compiles the runner" (owner decision 3): `factory-ci-selftest` keeps its compile jobs as a conscious pre-merge exception (Q2 below): one job runs `cargo test --lib --no-run` so test code type-checks, the other runs `cargo build --release` and feeds a no-push Dockerfile build. Recorded in the workflow header comment (T014) and the README (T022).
- Third-party actions pinned, minimal `permissions`: kept. Pass.

## Architecture decision

The runner owns the whole CI job. The workflow starts one container. Inside it the runner reads Bitwarden, checks the repo identity, fetches, checks Loki, runs and prints a closed grammar.

Alternatives rejected:

1. **Keep shell in the workflow, add a thin runner flag.** The owner: the logic must not live in GitHub Actions.
2. **Run the runner on the VM (binary copied out).** The 007 design. Rejected by the owner (decision 2).
3. **A new TOML naming targets and projects.** The watch list already holds repo, branches and `allow_docker`. A second file would list the repos twice, and a `projects` key would break the tick's parser (unknown keys are errors). The project is an input.
4. **Native Bitwarden client in Rust.** Needs token parsing, key stretching and AES-CBC-HMAC with no upstream tests. `bws` is Bitwarden's own pinned binary.
5. **docker-in-docker for the runner container.** Needs `--privileged`. The socket mount is the owner's choice; containment is in the security assessment.
6. **Clone in a container (the `job` path).** Needs another pinned image and host-path mapping for a container that only runs `git`. The runner image has `git`.
7. **Debian slim base.** About 60 MB larger, no gain: `bws` and the runner are static musl.
8. **amd64-only image.** The owner chose multi-arch (Q1): two `bws` stages, one per architecture, each pinned by SHA-256, plus QEMU in the release build.
9. **A separate `docker pull` step.** Allowed only if the "only a trigger" goal survives; pulling inside the one `docker run -q` step keeps it.
10. **iptables block of the metadata endpoint in the workflow.** Puts logic back in the workflow. The ephemeral hosted VM is the containment; see security item 7.

**Path mapping**: the runner container mounts `$RUNNER_TEMP/ci` at the identical path. `--work` must be canonical (`parse_args` refuses a path that is not equal to its canonical form), and the runner builds every workspace path under it without rewriting. The host daemon resolves `-v <path>:/workspace` on the host, where the same path exists.

**Ownership**: the runner container runs as the VM user (uid 1001), so the checkout belongs to 1001. Step containers run as root (the image default). Docker's default capability set includes `DAC_OVERRIDE`, so root in a step can write the 1001-owned checkout; this is why step containers keep today's three-capability drop (`NET_RAW`, `MKNOD`, `SETFCAP`) and not `--cap-drop ALL`, which would break `tar`, `apt` and `apk` as root and the checkout write. Files a step creates are root-owned; the VM is discarded, and the work directory is not cleaned. Git in a step would refuse the 1001-owned checkout as "dubious ownership", so the run-wide step env sets `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=safe.directory`, `GIT_CONFIG_VALUE_0=/workspace`.

**Where each old shell duty went**

| Old shell step in `factory-monorepo-ci.yml` | New home |
| --- | --- |
| validate ref | `ci::parse_args` (FR-007) |
| install bws, pinned | `rust-ci-runner/Dockerfile` (FR-025) |
| fetch token, jq key lookup, shape check, masking | `store.rs` and the redactor (FR-009 to FR-011) |
| clone with header in env | `ci::fetch_ref` (FR-013) |
| fetch rust-ci-runner from image, `docker cp` | gone; `docker run` of the image |
| refuse docker steps with grep | `Pipeline::docker_problems` called by `ci` (FR-021) |
| Loki URL strip, probe (with `service_name`) | `ship::LogConfig::from_env`, `ship::probe` (FR-015, FR-016) |
| output filter with grep and sed | `Log::emit` public mode (FR-018) |
| step summary | dropped (FR-005) |

## Contracts

### CLI

```text
rust-ci-runner ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]
```

`parse_args` rejects a repeated flag, an unknown flag, a missing value and any value that starts with `--`. `--work` must be absolute, free of `:`, an existing directory and equal to its canonical path.

Environment read: `BWS_ACCESS_TOKEN` (required; `main` reads it once at start and calls `remove_var` before any thread starts, then passes it down as a value), `FACTORY_CI_BWS_SERVER_URL` (optional; host must be `vault.bitwarden.com` or `vault.bitwarden.eu`, https), `GITHUB_RUN_ID` (optional, `valid_name`; else random), `FACTORY_CI_ENGINE` (docker or podman or a path, default docker), `FACTORY_CI_BWS` (path of `bws`, default `bws`), `FACTORY_CI_DIND_IMAGE` (only with `--allow-docker`). No other variable is read. `GITHUB_SERVER_URL` is ignored.

Exit codes: 0 the pipeline passed and every log line was delivered; 1 anything else. No table, no other code.

### Public output (closed grammar)

All output goes through `Log::emit`, the single choke point. A line that does not match is dropped, and every printed line starts with `[`.

stdout: `[<name>] started`, `[<name>] success in <N>s`, `[<name>] failed in <N>s`, `[<name>] skipped`, `[runner] pipeline passed`, `[runner] pipeline failed`, `[runner] log store answered HTTP <code>`, `[runner] log lines were dropped` (printed when the pipeline passed and a line was dropped; the exit code is 1). The name `runner` is reserved: a pipeline step named `runner` is refused.

stderr, `[runner] <text>` with text from this const list only (`Log::fault` accepts a member and drops anything else):

1. `bad arguments`
2. `repository or ref not in the watch list`
3. `BWS_ACCESS_TOKEN is not set`
4. `secret store unreachable`
5. `secret store rejected the access token`
6. `secret <KEY>: expected 1, found <N>` and `secret <KEY>: unusable value` (KEY is one of the four fixed names)
7. `log store check failed: check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)` (probe answered 401 or 403 only)
8. `log store check failed` (any other non-2xx or no answer after the retry)
9. `cannot reach the container engine`
10. `repository identity check failed`
11. `clone failed`
12. `pipeline file refused`
13. `docker steps are refused`
14. `runner fault`
15. `internal error`

### Child processes

Every `Command` in `ci.rs`, `store.rs`, `ship.rs` (none there today) and `engine.rs` sets stdin, stdout and stderr explicitly (null or piped, never inherited) and starts with `env_clear()` plus an allowlist:

| Child | Allowlist (plus the explicit entries named) |
| --- | --- |
| `bws` | `PATH`, `HOME`, `BWS_ACCESS_TOKEN`, `BWS_SERVER_URL` |
| `git` | `PATH`, `HOME=/tmp`, `GIT_CONFIG_GLOBAL=/dev/null`, `GIT_CONFIG_SYSTEM=/dev/null`, `GIT_TERMINAL_PROMPT=0`, `GIT_ALLOW_PROTOCOL=<seam>`, `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0`, `GIT_CONFIG_VALUE_0` |
| engine (`docker`, `podman`) | `PATH`, `HOME`, `DOCKER_HOST`, `DOCKER_CONFIG`, `DOCKER_CERT_PATH`, `DOCKER_TLS_VERIFY`, `XDG_RUNTIME_DIR`, plus the call's explicit `env` argument |

`Engine::command` also calls `env_remove("BWS_ACCESS_TOKEN")`, which acts on the inherited environment only; an explicit entry with that name still passes (test below). `store::fetch` does not use `Engine`: it spawns its own `Command` with the `bws` row above.

Scope of the "no secret in a child" rule: engine children (docker) and step containers never see any bootstrap value. `git` gets the clone header it needs; `bws` gets the access token it needs.

### Bitwarden

Keys, exact match, exactly one each: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. Command: `bws secret list -o json`, no project argument (one project per machine account, 003 D26). Output: a JSON array of objects with `key` and `value`; other keys are ignored and not counted aloud. `fetch(bws_bin, token, server_url, limit: Duration)`: `limit` per attempt (60 s in production, milliseconds in tests). Cited from 003: `research.md` D26, `contracts/bootstrap-secrets.md` (list call, shape rules, no retry on a rejected token), `factory-ci-secrets.sh` (`BWS_SHA256`).

### Repo identity

Before the fetch, `GET <api base>/repos/<repo>` (production base `https://api.github.com`, a seam for tests) with `Authorization: Bearer <FACTORY_CI_TOKEN>`, 15 s, no redirects, in process with `ureq`. The `id` in the JSON must equal the watch entry's numeric `id`. Any mismatch, error or missing `id` gives fault line 10. The response body is never printed. Entries without `id` are refused by `ci` (line 2).

### Loki

Push: `POST <url>/loki/api/v1/push`, JSON, basic auth. Labels (five): `service_name` (`SERVICE_NAME`), `repo`, `project`, `step`, `stream`. The body comes from one function `render(labels, meta, batch)` extracted from `Shipper::body`; the shipper and the probe both call it. Probe: `project=probe`, `step=probe`, `stream=stdout`, one line. `probe(cfg, run_id, limit) -> Result<u16, u16>` retries once after 2 s on `000` (no answer), 429 and 5xx, and returns the final code; the hint (fault line 7) prints only for 401 and 403. Line prefix `run=<id> sha=<short> branch=<ref>` as today. After the fetch, one `ci` line carries the full checked-out SHA.

A single line over the store's per-line limit makes Loki reject its batch; the shipper drops it, the run ends `log lines were dropped`, exit 1. Documented in the README (T022); no cap in code.

### Workflow to runner

```text
export DOCKER_CONFIG=$(mktemp -d); trap 'rm -rf "$DOCKER_CONFIG"' EXIT
printf '%s' "$GH_TOKEN" | docker login ghcr.io -u "$GITHUB_ACTOR" --password-stdin >/dev/null
mkdir -p "$RUNNER_TEMP/ci"
docker run -q --rm --init --name "factory-ci-runner-$GITHUB_RUN_ID"
  --user "$(id -u):$(id -g)" --group-add "$(stat -c %g /var/run/docker.sock)"
  --cap-drop ALL --security-opt no-new-privileges --read-only
  --tmpfs /tmp:rw,size=64m -e HOME=/tmp
  -e BWS_ACCESS_TOKEN -e GITHUB_RUN_ID -e FACTORY_CI_BWS_SERVER_URL
  -v /var/run/docker.sock:/var/run/docker.sock
  -v "$RUNNER_TEMP/ci:$RUNNER_TEMP/ci"
  -v "$GITHUB_WORKSPACE/.github/factory-ci-repos.toml:/etc/factory-ci/repos.toml:ro"
  "$RUNNER_IMAGE" ci --repos /etc/factory-ci/repos.toml --repo "$REPO" --ref "$REF" --project "$PROJECT" --work "$RUNNER_TEMP/ci"
```

Every variable is quoted, every value reaches the script through `env:`, and the workflow never passes `--allow-docker`. `-e NAME` with no value copies it from the step environment, so argv holds none. `FACTORY_CI_BWS_SERVER_URL` comes from `vars.`; empty means unset. The pull happens inside `docker run -q`, so the "only a trigger" shape holds. The temporary `DOCKER_CONFIG` holds the short-lived `GITHUB_TOKEN` for the login and is removed on exit; the step does not `exec`, so the trap fires. Owner option: make the GHCR package public and drop the login step.

Header comment of the workflow (T019): never switch this job to a self-hosted runner; the owner exception; the specs it supersedes.

### Image

```text
ARG TARGETARCH                      # before the first FROM, so FROM can use it
FROM alpine:3.23@sha256:<index digest> AS unzip     # throwaway: busybox unzip lacks -Z, apk adds a full one
RUN apk add --no-cache unzip
FROM unzip AS bws-amd64
ADD --checksum=sha256:<amd64 hash> https://github.com/bitwarden/sdk-sm/releases/download/bws-v2.1.0/bws-x86_64-unknown-linux-musl-2.1.0.zip /bws.zip
RUN unzip -p /bws.zip bws > /bws && chmod +x /bws
FROM unzip AS bws-arm64
ADD --checksum=sha256:eb0f1ae61d1c3b74244d2841233276e05c77e8be4da197ed90fc6248387005e1 https://github.com/bitwarden/sdk-sm/releases/download/bws-v2.1.0/bws-aarch64-unknown-linux-musl-2.1.0.zip /bws.zip
RUN unzip -p /bws.zip bws > /bws && chmod +x /bws
FROM bws-${TARGETARCH} AS bws
FROM alpine:3.23@sha256:<index digest>
ARG TARGETARCH                      # again: a global ARG is not visible inside a stage
LABEL org.opencontainers.image.source=https://github.com/sudtanj/infrastructure
RUN apk add --no-cache git docker-cli ca-certificates
COPY --from=bws /bws /usr/local/bin/bws
COPY ${TARGETARCH}/rust-ci-runner /rust-ci-runner
USER 65532:65532
ENTRYPOINT ["/rust-ci-runner"]
```

`ADD --checksum` fails the build on a mismatch; there is no listing check. The binary stays at `/rust-ci-runner` because the tick's `fetch runner` step runs `docker cp "$id:/rust-ci-runner"`. The amd64 hash and the Alpine digest are copied from a lookup, never typed (B6). Alpine packages float inside a release; the pin that matters is the image digest. Alpine 3.21 reaches end of life on 2026-11-01, so the base is 3.23.

## Files to add or change

| File | Change |
| --- | --- |
| `.specify/memory/constitution.md`, `.specify/feature.json` | 1.1.0, folder pointer (done, T001) |
| `rust-ci-runner/Cargo.toml` | version 0.2.0, in T014 so the lockfile carries it; main has 0.1.1 |
| `rust-ci-runner/Cargo.lock` (new) | committed (T016) |
| `rust-ci-runner/src/ship.rs`, `ship_tests.rs` | URL normalisation; `render` extracted from `Shipper::body`; `probe` (T002, T003) |
| `rust-ci-runner/src/log.rs` | `Log::public_events(bool)`, grammar and fault list in `emit`, `Log::fault` (T004) |
| `rust-ci-runner/src/pipeline.rs` | refuse a step named `runner` (T004) |
| `rust-ci-runner/src/engine.rs` | `command()`: `env_clear` plus allowlist, `env_remove`; `run_step` keeps its three-capability drop, `no-new-privileges` and `--pids-limit 512` and adds `--memory 5g --memory-swap 5g` (T005) |
| `rust-ci-runner/src/store.rs`, `store_tests.rs` (new) | `fetch`, `parse_list`, `StoreError`, `Secrets`; the engine-child tests live here too (T005, T006) |
| `rust-ci-runner/src/watch.rs`, `watch_tests.rs`, `job.rs`, `job_tests.rs`, `main.rs` (`load_list`) | `watch::read`, `pub(crate)` helpers, `project_dir`, `local` fails on dropped lines (T007) |
| `rust-ci-runner/src/ci.rs`, `ci_tests.rs` (new) | the command (T008, T010) |
| `rust-ci-runner/src/main.rs`, `lib.rs` | `ci` dispatch, token read and `remove_var`, panic hook, usage, `mod` lines (T009) |
| `rust-ci-runner/.factory-ci.toml` | `--locked`, `--skip ci_tests::`, fourth command (T011) |
| `rust-ci-runner/Dockerfile` | rewritten (T012) |
| `.github/workflows/rust-ci-runner-release.yml` | QEMU, provenance, smoke, release after push, idempotent, summary, `--locked` (T013, T016) |
| `.github/workflows/factory-ci-selftest.yml` | `push` trigger, `cargo test --lib --no-run`, `cargo clippy --locked -- -D warnings`, multi-arch no-push Dockerfile build with QEMU, lockfile generator, guard grep step, version bump in `Cargo.toml` (T014); `--locked` and generator removal (T016); the new shell test step (T020) |
| `.github/factory-ci-repos.toml` | two entries with `id`, header comment lines (T015) |
| `.github/workflows/factory-monorepo-ci.yml` | cut to a trigger (T019) |
| `.github/scripts/test-factory-monorepo-ci.sh` (new) | plain grep checks (T020) |
| `specs/005-*/spec.md`, `specs/006-*/spec.md`, `specs/007-*/spec.md`, `README.md`, `rust-ci-runner/README.md` | superseded notes, docs (T022) |

The lint script `factory-ci-lint-workflow.sh` and its self-test rows stay unchanged (platforms stay `linux/amd64,linux/arm64`). The engineer still reads it, because it checks the release workflow.

## `ci` flow (ci.rs)

1. Parse and validate flags (FR-007). `main` installed the panic hook and removed `BWS_ACCESS_TOKEN` from the process environment first.
2. `watch::read`, find the entry, require `id`, match `--ref` with `matches_branch`, gate `--allow-docker` on the entry. Line 2 or 1.
3. Empty token means line 3.
4. `store::fetch`. Build `LogConfig` from the fetched map through the `Env` closure `LogConfig::from_env` takes; no `set_var`. Build the `Redactor` with the access token, its base64, the four values, their base64, and the base64 of `x-access-token:<FACTORY_CI_TOKEN>`.
5. `ship::probe`. Print its line. 401 or 403 gives line 7, other non-2xx gives line 8, exit 1.
6. Start the `Shipper` (labels: target repo, project; meta: run id, empty sha, ref). From here fault detail also goes to Loki, redacted.
7. Engine check: `docker version`. Failure means line 9.
8. Identity check (`GET /repos/<repo>`). Failure means line 10.
9. Fetch into `<work>/ci-<runid>-<random>/src`: `git init -q <dir>`, then `git -C <dir> fetch -q --depth 1 --no-tags -- <base>/<repo>.git refs/heads/<ref>` (only a branch can match, never a tag), then `git -C <dir> checkout -q --detach FETCH_HEAD`, then `git rev-parse HEAD` for the Loki line and `FACTORY_CI_SHA`. Header via `GIT_CONFIG_*` env as `http.https://github.com/.extraHeader` = `Authorization: basic <base64("x-access-token:" + token)>`. Limit 300 s for the fetch. The seam carries the base URL and the allowed protocol: production `https://github.com` and `https`, tests a `file://` path and `file`. Failure means line 11.
10. `project_dir`, then `prepare(dir, None, |_| None, allow_docker)`. A docker problem is line 13; any other problem is line 12, detail to Loki.
11. `run_pipeline` with a `Log` in quiet public mode plus the shipper sink. `run_env(..., "manual")`.
12. Print `pipeline passed|failed`, `engine.cleanup_run(id)`. The work directory stays (shortcut: ephemeral hosted VM; add removal before a persistent host runs this).
13. `Shipper::finish(FLUSH_WINDOW)`. A passing run that dropped lines prints `[runner] log lines were dropped`. Exit 0 only for a pass with nothing dropped; everything else exits 1.

## Test plan (names are the contract)

| Requirement | Test (module) |
| --- | --- |
| FR-015 | `url_loses_the_push_path_and_slashes`, `url_normalisation_is_idempotent`, `url_with_another_path_is_kept` (ship_tests) |
| FR-016 | `probe_204_is_ok`, `probe_401_reports_the_code`, `probe_with_no_answer_reports_000_in_time`, `probe_retries_once_on_5xx_and_429`, `probe_posts_one_line_with_the_five_labels_and_basic_auth` (ship_tests; the body holds `service_name`, built by `render`) |
| FR-017 | `run_quiet_logs_fails_when_a_passing_run_dropped_lines` (job_tests), `dropped_lines_on_a_pass_print_the_line_and_exit_1` (ci_tests) |
| FR-018 | `public_mode_prints_only_the_closed_grammar`, `public_mode_drops_the_reason_after_failed_and_skipped`, `public_mode_ships_the_full_text`, `public_mode_refuses_a_bad_name`, `fault_accepts_only_the_fixed_list`, `step_named_runner_is_refused` (log, pipeline) |
| FR-009, FR-010 | `one_secret_per_key_is_returned`, `zero_or_two_matches_name_key_and_count_only`, `value_shapes_are_refused`, `token_goes_in_env_not_argv`, `bws_child_env_is_the_allowlist`, `rejected_token_is_not_retried`, `unreachable_is_retried_once`, `slow_bws_times_out_within_the_limit`, `server_url_must_be_https_on_an_allowed_host`, `garbage_output_is_unreadable_and_not_echoed` (store_tests) |
| FR-012 | `command_removes_the_inherited_token`, `explicit_env_entry_still_passes`, `engine_child_env_is_the_allowlist` (store_tests; the first two read `Command::get_envs`, the third runs a stub engine that dumps its environment and asserts `CARGO_PKG_NAME` is absent, `PATH` and the explicit entry present) |
| FR-006, FR-007 | `ci_requires_all_flags`, `repeated_flag_is_refused`, `value_starting_with_dashes_is_refused`, `ref_table`, `project_table`, `work_table` (includes a non-canonical path), `repo_outside_the_watch_list_exits_1_before_bws`, `ref_outside_branches_exits_1`, `entry_without_id_exits_1` (ci_tests) |
| FR-013, FR-014 | `fetch_runs_init_fetch_checkout_with_the_header_in_env` (a stub `git` that dumps argv and its `GIT_CONFIG_*` env, no clone), `fetch_token_is_not_in_argv`, `fetch_asks_for_refs_heads_only`, `real_fetch_from_a_file_repo_with_cleared_env` (a stub that records argv and `env`, then runs the real `git`, over `file://` with the protocol seam `file`), `identity_mismatch_exits_1`, `symlinked_pipeline_file_is_refused`, `symlinked_project_is_refused` (ci_tests) |
| FR-019 | `toml_error_prints_the_fixed_line_and_ships_the_detail`, `panic_prints_internal_error_only`, `every_stderr_line_is_in_the_fixed_list`, `hostile_children_cannot_reach_stdout_or_stderr` (stubs `bws`, `git`, `docker` write the token, its base64 and `::add-mask::x` to both streams; stdout and stderr hold only grammar or fixed-list lines) (ci_tests) |
| FR-021, FR-022 | `docker_step_is_refused_before_any_container`, `allow_docker_needs_the_entry_and_a_pinned_dind_image`, `declared_secret_fails_before_any_container` (ci_tests) |
| FR-023 | `engine_down_exits_1_before_the_fetch` (ci_tests) |
| FR-024, I10 | `mount_source_equals_the_work_path`, `step_argv_has_no_forbidden_flag` (requires exactly `--security-opt no-new-privileges`, `--pids-limit 512`, `--cap-drop NET_RAW`, `--cap-drop MKNOD`, `--cap-drop SETFCAP`, `--memory 5g`, `--memory-swap 5g`; forbids any `--cap-add`, `--privileged`, `--pid`, `--network host`, other `-v`, `docker.sock`), `no_docker_config_reaches_step_containers` (no `-e DOCKER_CONFIG`, no `DOCKER_CONFIG` mount in any step argv), `step_env_sets_git_safe_directory` (ci_tests) |
| FR-009 | `missing_access_token_exits_1_without_calling_bws` (ci_tests) |
| FR-022, run fault | `engine_fault_during_run_exits_1_with_runner_fault` (ci_tests) |
| FR-008 | `passing_run_exits_0`, `failing_step_exits_1_without_reason` (ci_tests) |
| FR-011, SC-002 | `no_bootstrap_value_in_stdout_stderr_argv_or_engine_child_env` (covers the base64 forms), `hostile_fixture_prints_only_allowed_lines` (ci_tests) |
| FR-007 shared | `read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref` (watch_tests) |
| FR-001 to FR-005 | `.github/scripts/test-factory-monorepo-ci.sh` (plain grep, no self-mutation): exactly one `docker run`; no `bws`, `jq`, `curl`, `git clone`, `grep -E`, `cargo`; `secrets.` names only `BWS_ACCESS_TOKEN` and `GITHUB_TOKEN`; no `${{` in any `run:`; `refs/heads/main` guard; `environment: factory-ci`; every `uses:` pinned; `-e BWS_ACCESS_TOKEN` without `=`; no `--allow-docker`; no `-e DOCKER_CONFIG` and no `DOCKER_CONFIG` mount on the `docker run` lines; no unquoted `$VAR` on the `docker run` lines (every `$` is inside double quotes); the `run:` block at most 25 lines; `timeout-minutes: 45`; the header names `self-hosted`. Runs in the `factory-ci-selftest` job `shell`. |
| FR-025, FR-026 | the Dockerfile build in the selftest (amd64 and arm64 under QEMU, no push), the lint (unchanged), the release run with its smoke step (Rollout step 3) |
| FR-027 | the `rust-ci-runner` pipeline passing on `main` (Rollout step 2) |

Acceptance scenarios to tests: US1.1 and US1.2 `passing_run_exits_0`, `failing_step_exits_1_without_reason`; US1.3 the shell test (main guard); US1.4 the first run T021; US1.5 `repo_outside_the_watch_list_exits_1_before_bws`, `ref_outside_branches_exits_1`; US2.1 to US2.5 `hostile_fixture_prints_only_allowed_lines`, `hostile_children_cannot_reach_stdout_or_stderr`, `public_mode_drops_the_reason_after_failed_and_skipped`, `toml_error_prints_the_fixed_line_and_ships_the_detail`, `panic_prints_internal_error_only`, `no_bootstrap_value_in_stdout_stderr_argv_or_engine_child_env`; US3.1 to US3.4 the `url_*`, `probe_*` and `dropped_lines_on_a_pass_print_the_line_and_exit_1` tests; US4.1 to US4.3 `docker_step_is_refused_before_any_container`, `declared_secret_fails_before_any_container`, `step_argv_has_no_forbidden_flag`; US5.1 the release run and its smoke; US5.2 the Rollout gate.

Stubs: `bws`, `docker` and `git` are `sh` scripts written into a temp dir (the pattern `job_tests.rs` uses), selected through `FACTORY_CI_BWS`, `FACTORY_CI_ENGINE` and a `git` path seam on `ci::run`. Seams on `ci::run`: the clone base URL, the allowed protocol, the API base URL and a `Limits` struct (bws, probe, fetch, identity durations; production values in `ci()`). Loki and the API are `tiny_http` as in `ship_tests.rs`. The test environment is passed as a closure; no test calls `set_var`.

Test process split (the step container allows 512 processes and mock-server threads never exit): `ci_tests::` has its own `cargo test` command, like `ship_tests::` and `job_tests::`. `store_tests` and `watch_tests` run in the main process because they use only stub scripts.

## Failure modes

| Step | What can fail | Detection | Behaviour | Test |
| --- | --- | --- | --- | --- |
| Args | missing, repeated or `--`-prefixed value, bad ref, project or work | `parse_args` | exit 1, line 1, nothing else done | `ci_requires_all_flags`, `repeated_flag_is_refused`, `ref_table` |
| Watch gate | repo or ref not listed, no `id`; file unreadable or invalid | `watch::read`, `matches_branch` | exit 1, line 2 | `repo_outside_the_watch_list_exits_1_before_bws`, `entry_without_id_exits_1` |
| Token | `BWS_ACCESS_TOKEN` unset | value from `main` | exit 1, line 3, no `bws` call | `missing_access_token_exits_1_without_calling_bws` |
| Bitwarden | down, slow, rejects token, garbage JSON, host not allowed | status, limit, parse | retry once unless rejected; exit 1, lines 4 to 6 | `store_tests` |
| Bitwarden | 0 or 2 matches, bad value shape | `parse_list` | exit 1, line 6, key and count | `store_tests` |
| Probe | URL wrong, user wrong, scope missing, timeout, 5xx | HTTP code after one retry | exit 1, line 7 (401, 403) or 8, no step runs | `probe_*` |
| Engine | socket missing, group missing, daemon down | `docker version` | exit 1, line 9, before the fetch | `engine_down_exits_1_before_the_fetch` |
| Identity | repo renamed or moved, token cannot read it | API id compare | exit 1, line 10 | `identity_mismatch_exits_1` |
| Fetch | ref missing or a tag, no access, network, timeout | git status | exit 1, line 11, git text to Loki redacted | `fetch_*` |
| Pipeline file | TOML error, bad field, link, too large, step `runner` | `read_file`, `parse` | exit 1, line 12, detail to Loki | `toml_error_prints_the_fixed_line_and_ships_the_detail` |
| Docker refusal | `docker = true` | `docker_problems` | exit 1, line 13, no container | `docker_step_is_refused_before_any_container` |
| Secrets | step lists `secrets` | `resolve_secrets` | exit 1, line 12 | `declared_secret_fails_before_any_container` |
| Run | step fails or times out | `Outcome` | exit 1, `failed in Ns`, no reason | `failing_step_exits_1_without_reason` |
| Run | step cannot write the checkout (ownership) | step fails fast; detail in Loki | exit 1 | first run T021, pre-mortem 2 |
| Run | network create fails, sidecar fault | `execute` Err | exit 1, line 14 | `engine_fault_during_run_exits_1_with_runner_fault` |
| Shipping | Loki dies after the probe, 401 mid-run, oversize line | `Shipper::finish` | pass becomes exit 1 with `log lines were dropped` | `dropped_lines_on_a_pass_print_the_line_and_exit_1` |
| Panic | any bug | panic hook, `catch_unwind` | exit 1, line 15 | `panic_prints_internal_error_only` |
| Hostile child | a stub or tool writes secrets or `::add-mask::` to its streams | `Log::emit` allowlist | line dropped | `hostile_children_cannot_reach_stdout_or_stderr` |
| Mount | path mismatch between runner and host | first step sees an empty `/workspace` | step fails fast; detail in Loki | `mount_source_equals_the_work_path`, T021 |
| Pull | digest not published, login fails | `docker run` pull error | job fails before the runner | Rollout gate |
| Image build | `bws` hash mismatch, apk or base unavailable, wrong stage | `ADD --checksum`, Docker build | no image published, no GitHub release | selftest build (amd64), release run |
| Release | smoke fails, rerun of a tag | smoke step; `gh release view` | no release created; a rerun skips an existing release | release run |
| Release | the image's `ci` command is broken or the flags are wrong | smoke step runs `ci --repos /etc/factory-ci/repos.toml --repo x/y --ref main --work <dir>` with the workflow's `docker run` flags; expects stderr `[runner] repository or ref not in the watch list` and exit 1 (the gate runs before Bitwarden, so no token is needed) | no release created | release run |
| Lockfile | generator step output missing | artifact absent | engineer reports; no `--locked` until committed | T016 |

## Pre-mortem

Assume it shipped and broke. The five likeliest causes:

1. **The step containers see an empty or read-only workspace, or git refuses it.** The runner container and the host disagree on a path (symlinked `RUNNER_TEMP`, a typo in `-v`), or git in a step reports "dubious ownership" of the uid-1001 checkout. Prevention: canonical `--work`, identical-path mount, `mount_source_equals_the_work_path`, Docker's default `DAC_OVERRIDE` kept by the three-capability drop, `safe.directory` in the run-wide step env, and T021 reads and writes a file of the clone and runs `git status` inside a step.
2. **`permission denied` on the docker socket, or `bws` cannot write.** The container user lacks the socket group, or `--read-only` blocks `bws`. Prevention: `--group-add $(stat -c %g ...)`, `HOME=/tmp` on a 64 MB tmpfs, the engine check (line 9), the release smoke step that runs `docker version` and `bws --version` with the same flags, and T021. A `bws` path issue is fixed with an env var in the `docker run`.
3. **The logs still do not reach Grafana.** Token scope, user id, region, or a missing label. Prevention: URL normalisation, the probe through the shared `render` (five labels), its retry, the fixed hint for 401 and 403, exit 1 for later drops.
4. **A private detail leaks into the public log.** A reason, a TOML error, a git message, a panic, a hostile child writing to an inherited stream. Prevention: `Log::emit` as the single choke point, explicit stdio on every child, the fixed fault list, the panic hook, the hostile-children and hostile-fixture tests.
5. **The release or the bootstrap leaves no usable image.** Wrong digest, release created before a failed push, an EOL base, a hash typed wrong. Prevention: `gh release create` runs after build, push and smoke and is idempotent; `ADD --checksum` per architecture; Alpine 3.23 by index digest; digests copied from a lookup; the selftest builds the Dockerfile before the merge; bootstrap in two landings with a verified digest between them; the 0.1.0 image stays.

## Invariants (qa turns each into a property test)

- I1. For any step names, step output, failure reasons, pipeline text and child output, every stdout line of `ci` matches the closed grammar of FR-018, every stderr line is in the fixed list, and every printed line starts with `[`.
- I2. None of the five bootstrap values, nor their base64 forms (including base64 of `x-access-token:<token>`), appears in stdout, stderr, the argv of any child, or the environment of any engine child or step container.
- I3. `normalise(normalise(u)) == normalise(u)`; the result never ends with `/` or `/loki/api/v1/push`.
- I4. The exit code is 0 only when the pipeline passed and no log line was dropped; every other end exits 1.
- I5. No container starts before the engine check, and no step container starts when a step has `docker = true` without both opt-ins, or declares a secret.
- I6. The `-v` source of every container the runner starts is a path under `--work`, byte for byte (the steps' only mount).
- I7. An accepted `ref` matches `[A-Za-z0-9._/-]{1,200}`, does not start with `-`, does not contain `..`; no flag appears twice; no flag value starts with `--`.
- I8. For any secret list, a key with a match count other than 1 is an error and no value is returned.
- I9. `BWS_ACCESS_TOKEN` is absent from the inherited environment of every `Engine` child, for any input; an explicit entry still passes.
- I10. No step container argv contains `--privileged`, `--pid`, `--network host`, any `--cap-add`, a `-v` other than the workspace mount, `docker.sock` or `DOCKER_CONFIG`; it always contains `--security-opt no-new-privileges`, `--pids-limit 512`, `--cap-drop NET_RAW`, `--cap-drop MKNOD`, `--cap-drop SETFCAP`, `--memory 5g` and `--memory-swap 5g`. Neither the runner container (shell test) nor a step container gets `-e DOCKER_CONFIG` or a `DOCKER_CONFIG` mount.
- I11. Every child is spawned with explicit stdin, stdout and stderr and a cleared environment plus the allowlist of its row.

## Edge-case checklist (feiyi-edge-cases)

| Group | Answer |
| --- | --- |
| Inputs: empty, one, max, over | handled: `ref` 1 to 200, project name 1 to 100, key counts 0, 1, 2 (`ref_table`, `project_table`, `zero_or_two_matches_name_key_and_count_only`); pipeline file at the 256 KB cap uses the existing limit; a line over the store limit drops its batch and fails the run (documented) |
| Inputs: duplicates | handled: two secrets with one key refuse the run; repeated flag refused; two runs with one input queue (workflow concurrency) |
| Inputs: unicode, NUL, newline | handled: values with CR, LF, NUL refused; refs outside ASCII refused; control characters in step output already replaced in `Log` |
| Inputs: malformed or truncated | handled: bad JSON from `bws`, bad TOML, bad watch list, bad API JSON each exit 1 with a fixed line |
| State: first run, older data | N/A: no stored state; the work dir is new per run. First release: bootstrap order in Rollout |
| State: crash mid-run | handled: containers carry the run label and `cleanup_run` runs at the end; a killed VM discards the rest |
| State: partial failure | handled: probe passes then Loki dies gives exit 1; fetch fails after the probe leaves only the Loki line; a release rerun skips an existing GitHub release |
| State: retry | handled: a rerun starts a fresh directory and fresh containers; the release is idempotent |
| Time: two callers | handled: workflow concurrency per repo, ref and project; random part in the work dir name |
| Time: timeouts | handled: bws 60 s x2, probe 15 s x2, identity 15 s, fetch 300 s, run 30 min, Loki flush 60 s; job 45 min (sum of budgets about 38) |
| Time: clock, expiry | N/A: the probe timestamp is `now`; no expiry logic |
| Time: ordering | handled: the Shipper sorts a batch by timestamp (existing) |
| Resources: disk full | handled: log write errors are dropped by design, the run result stays; the VM is discarded |
| Resources: large input | handled: log cut at 10 MB, queue cap 32 MB, pids limit 512, step memory 5 GB with no swap |
| Resources: slow or garbage dependency | handled: Bitwarden, Loki, GitHub API, git and docker each have a limit and a fixed failure line |
| Resources: unbounded growth | handled: nothing grows; one run per VM; the runner tmpfs is 64 MB |
| Resources: smallest machine | N/A: a hosted runner; the 1 GB planning VM runs nothing |
| Permissions: who calls | handled: `workflow_dispatch`; the Environment limits which ref gets the secret, not who dispatches; the repo must be in the watch list with a matching id |
| Permissions: what the caller must not see | handled: closed output grammar, values in memory, none in argv or step containers |
| Permissions: client values trusted by mistake | handled: `repo`, `ref`, `project` validated; fetch host fixed; `GITHUB_SERVER_URL` ignored; `.factory-ci.toml` untrusted; repo id compared; only `refs/heads/<ref>` fetched |
| UI: states | N/A: no UI; the Actions log is the interface |

## Rollout and rollback

Both landings go to `main` by direct push (PRs return 403). The selftest runs on the branch push before each landing.

0. **Owner settings (T017)**, GitHub settings, not code, before the first workflow run: see the owner items in the security assessment.
1. **Landing A** (T001 to T016): runner, image, release workflow, selftest, watch list, lockfile. Pre-merge checks on the branch push: `factory-ci-selftest` (lib tests type-check, release build, fixture pipeline, Dockerfile build for both architectures, clippy, watch list check, lint, shell tests). The old `factory-monorepo-ci.yml` and its 0.1.0 pin stay.
2. After landing A, start the old workflow with `pipeline = rust-ci-runner`. The released 0.1.0 (or 0.1.1) binary runs the new unit tests. Red means revert or fix forward; no tag yet.
3. Push the tag `rust-ci-runner-v0.2.0` on a commit on `main`. The release builds both binaries, builds and pushes the image with provenance, runs the smoke step against the digest, then creates the GitHub release (idempotent), and prints both pins: `RUNNER_IMAGE_DIGEST` (tick variable) and `RUNNER_IMAGE` (manual workflow). Before the tag, the owner compares the committed `Cargo.lock` with a local `cargo generate-lockfile` (the owner has cargo; the planning VM does not). Verify with a real `docker pull ghcr.io/sudtanj/rust-ci-runner@sha256:<digest>` (T018). Whether 0.1.1 is released first does not matter; this release is 0.2.0.
4. **Landing B** (T019, T020, T022): trimmed workflow with that digest, shell test, docs. Only after step 3 is verified.
5. First runs (T021): `sudtanj/infrastructure` project `rust-ci-runner`, then `sudtanj/factory-monorepo` project `.`. Loki shows the lines, the Actions log shows only the grammar.

Rollback: before landing B nothing user-facing changed. After it, revert B; the old workflow with the 0.1.0 digest returns. Never delete the 0.1.0 package version. A bad 0.2.0 image is fixed by a 0.2.1 tag and new digest lines.

## Complexity Tracking

| Cost | Why needed | Simpler alternative rejected because |
| --- | --- | --- |
| A second subcommand path next to `run` and `job` | The owner wants one command that owns the job | Extending `run` would mix local-dir semantics with fetch, identity and a closed output mode |
| A new module `store.rs` that calls `bws` | The token must leave the workflow | A native client is more code with more risk (alternative 4) |
| Docker socket mounted in the runner container | Owner decision 2 | Containment, not removal (security assessment) |
| Selftest compiles the runner and builds the image | Catch breakage before the direct push | Waiting for the release run finds it after the tag; recorded as a pre-merge exception to "only release compiles" |
| One extra GitHub API call (identity) | A renamed repo or a stolen name must not receive the clone token | Trusting `owner/name` alone |

## Project Structure

```text
specs/008-runner-owned-ci/{spec.md,plan.md,tasks.md,checklists/requirements.md}
rust-ci-runner/src/{ci.rs,ci_tests.rs,store.rs,store_tests.rs}                  # new
rust-ci-runner/src/{ship,log,engine,job,watch,pipeline,main,lib}.rs             # changed
rust-ci-runner/{Dockerfile,Cargo.toml,Cargo.lock,.factory-ci.toml}              # changed, lock new
.github/workflows/{factory-monorepo-ci,rust-ci-runner-release,factory-ci-selftest}.yml
.github/scripts/test-factory-monorepo-ci.sh                                     # new
.github/factory-ci-repos.toml
```

**Structure Decision**: no new directory. Tests sit beside the code as `*_tests.rs`.

## DevOps assessment

Reviewed by the devops role; the lead handed the findings over. Each finding and the change it caused:

1. Platforms: multi-arch stays. The aarch64 `bws` asset exists (`bws-aarch64-unknown-linux-musl-2.1.0.zip`, SHA-256 `eb0f1ae6...05e1`); two `bws` stages with `ADD --checksum`, `docker/setup-qemu-action` pinned by SHA. The lint and its self-test row stay as they are.
2. busybox `unzip` has no `-Z`. Use `ADD --checksum`, then `apk add unzip` in a throwaway stage and `unzip -p bws.zip bws > bws`; no listing check.
3. `gh release create` ran before the image build, so a failed push left a release behind. It now runs after build, push and smoke, and is idempotent: `gh release view "$TAG" >/dev/null 2>&1 || gh release create ...`.
4. Nothing built the Dockerfile before the release. The selftest builds it for `linux/amd64` with the cargo-built binary staged, no push.
5. `alpine:3.21` reaches end of life on 2026-11-01. Base `alpine:3.23` pinned by the multi-arch index digest, read with `curl` from `Docker-Content-Digest` using an anonymous `auth.docker.io` token (engineers have no docker).
6. Job limit 45 minutes: the budgets sum to about 38.
7. `docker run` flags: `--init`, `--tmpfs /tmp:rw,size=64m`, `mkdir -p "$RUNNER_TEMP/ci"`, `--user $(id -u):$(id -g)`, `--group-add $(stat -c %g /var/run/docker.sock)`, identical-path mount, `HOME=/tmp`, `docker run -q`, quiet `docker login`.
8. The publish job smoke step after the push: `docker run` the digest with the same flags and `--entrypoint sh -c 'git --version && docker version && bws --version'`, then `docker create` plus `docker cp <id>:/rust-ci-runner` for the tick path, then `docker rm`.
9. Image `LABEL org.opencontainers.image.source=https://github.com/sudtanj/infrastructure` links the package to the repo.
10. The release summary names both pins: the tick variable `RUNNER_IMAGE_DIGEST` and `RUNNER_IMAGE` in the manual workflow.
11. The GHCR login can go if the package is public; kept until verified (owner option).
12. Step containers run as root against a uid-1001 checkout; see Ownership under Architecture decision.
13. The Environment `factory-ci` rule (main only) stays; `if: github.ref` guards the job and the Environment guards the secret.
14. Work directory cleanup is skipped on purpose (ephemeral VM); marked a shortcut.

## Security assessment

Reviewed by the security role; the lead handed the findings over. The findings and what the plan does about each:

1. **Hostile child output (blocker B1).** Every `Command` sets stdin, stdout and stderr explicitly (null or piped, never inherited), and every printed line passes `Log::emit`, which prints only grammar or fixed-list lines, each starting with `[`. Test: `hostile_children_cannot_reach_stdout_or_stderr`. Invariants I1, I11.
2. **Who can dispatch (blocker B2).** The Environment limits which ref receives the secret, not who dispatches. The owner sets, as GitHub settings and not code (task T017): the Environment `factory-ci` with a deployment-branch rule for `main` only and the owner as required reviewer; `BWS_ACCESS_TOKEN` stored only in that Environment; a ruleset on `main` (pull request required, code-owner review for `.github/**` and `rust-ci-runner/**`, no force-push, no deletion); a tag ruleset for `rust-ci-runner-v*`. The `main` ruleset goes on after landing B, since both landings are direct pushes.
3. **Floating dependencies (M1).** `Cargo.lock` is not committed. T014 adds a selftest step that runs `cargo generate-lockfile` and uploads `Cargo.lock` as an artifact; T016 has the engineer download it from the branch run and commit it, then adds `--locked` to the release build, the selftest and `rust-ci-runner/.factory-ci.toml`. `cargo deny` and `cargo audit` are a follow-up (they need a new tool in the image or a pinned install), not part of this feature.
4. **Repo identity and refs (M2).** Watch entries carry the numeric `id`; `ci` requires it and compares it with the GitHub API before it sends the clone token to git (fault line 10). Fetch asks for `refs/heads/<ref>` only, so a tag cannot match. Entries: `sudtanj/infrastructure` with `branches = ["main"]`, `sudtanj/factory-monorepo` with `branches = ["main"]` too; the plan does not need more, and the owner adds a pattern by commit when a feature branch must run.
5. **Step container limits (M3).** Security's M3 is met with the existing three-capability drop (`NET_RAW`, `MKNOD`, `SETFCAP`), `--security-opt no-new-privileges`, `--pids-limit 512`, and new `--memory 5g --memory-swap 5g`, not `--cap-drop ALL`: the second critic showed that `ALL` breaks `tar`, `apt` and `apk` as root, and Docker's default set already lets root write the uid-1001 checkout. There is no `--cpus` flag: Docker rejects a value above the host CPU count and private-repo hosted runners have 2 CPUs. I10 requires exactly this set and forbids any `--cap-add`, `--privileged`, `--pid`, `--network host`, other `-v` and `docker.sock`. The metadata endpoint is not blocked: an iptables rule would put logic back in the workflow. The ephemeral GitHub-hosted VM is the containment, and the workflow header says the job must never use a self-hosted runner. Owner items on secret scope: a read-only fine-grained PAT for `FACTORY_CI_TOKEN`; a Loki token with `logs:write` only; a Bitwarden machine account with read access to one project.
6. **Workflow injection (M4).** Every variable in `docker run` is quoted, values reach the script through `env:`, the workflow never passes `--allow-docker`, `parse_args` refuses repeated flags and values that start with `--`, and the shell test fails on any unquoted variable in the `docker run` lines.
7. **Environment and the access token (M5).** Every child starts from `env_clear()` plus an allowlist. `main` reads `BWS_ACCESS_TOKEN` once and removes it from the process environment before any thread starts (so `ci_leaves_the_process_environment_unchanged` is gone). The Redactor holds the access token, its base64, the four values, their base64, and the base64 of `x-access-token:<token>`. Correction: the token is visible to `docker inspect` of the runner container and to anything that can reach the socket (the runner and root on the VM); it is not visible to step containers.
8. **Reserved name (m1).** A step named `runner` is refused so it cannot forge `[runner] pipeline passed`.
9. **Bitwarden host (m2).** `FACTORY_CI_BWS_SERVER_URL` must be https on `vault.bitwarden.com` or `vault.bitwarden.eu`.
10. **Registry login (m3).** The login uses a temporary `DOCKER_CONFIG` removed on exit. Owner option: make the GHCR package public and drop the login.
11. **Supply chain (m4).** The Alpine digest and both `bws` hashes are copied from a lookup (`curl` against the registry and the release checksum file), never typed. The build uses `provenance: mode=max`.
12. **Pinned image.** The workflow references `tag@sha256:digest`; the owner bumps the digest by reviewed commit. Alpine packages float inside a release (accepted).
13. **Clone.** Host fixed to `github.com`, `GIT_ALLOW_PROTOCOL` fixed to `https` in production, no submodules, no tags, `env_clear`, empty global and system config, header scoped by URL so a redirect to another host gets none.
14. **Bitwarden scope.** A second project with the same keys makes the run fail as ambiguous, not pick one.

## Open decisions (made by the planner)

- Step capabilities (lead decision): keep today's three-capability drop, not `--cap-drop ALL` plus `--cap-add DAC_OVERRIDE`. See Ownership and security item 5.
- The selftest gets a `push` trigger for non-`main` branches, because pull requests return 403 and the pre-merge checks would never run.
- `docker login` uses a temporary `DOCKER_CONFIG` removed by an exit trap, not between pull and run, because the pull is inside `docker run -q`.
- Step limits `--memory 5g --memory-swap 5g` fit a 7 GB runner; tune after the first run. No `--cpus`: Docker rejects a value above the host CPU count.
- A line over the store's limit drops its batch (documented, not capped).
- Q2 answered by the owner: selftest keeps its compile jobs, one becomes `cargo test --lib --no-run`; the `list` job runs `cargo test --lib --no-run` then `cargo run -q -- plan --check`, which reuses the dev-profile build.
- Q3 answered by the owner: listing a repo also makes the by-hand tick plan it; the header of `factory-ci-repos.toml` says so.

## Open risks

- The new `ci_tests` first run in the `rust-ci-runner` pipeline after landing A (the manual workflow runs from `main` only). Before it, the selftest only type-checks them. No tag until the pipeline is green on `main`.
- The repo ids of `sudtanj/factory-monorepo` (private) and `sudtanj/infrastructure` come from `gh api repos/<repo> --jq .id`; if the engineer's login cannot read the private repo, the owner supplies the id (blocks T015).
- The engineer cannot run `docker`; lookups use `curl` and `gh` only. A failed lookup stops the task; no value is guessed.
- `bws` state or config path inside a read-only container is untested until the smoke step and the first run.
- `cargo generate-lockfile` resolves the latest compatible versions at that moment; a later bump is a reviewed commit.
- Hosted runner changes (docker group id, socket path) break `--group-add`; the engine check turns that into line 9.
- A real clone test needs `git` in the `rust:1.90` step image (it is there today); if not, T010 reports it.

## Step plan and engineer briefs

Every brief: work only the listed task IDs of `specs/008-runner-owned-ci/tasks.md`; read the named files first; no `cargo`, `docker` or builds on the engineer's machine (1 GB, no docker); check with `bash -n`, grep and reading; commit only named paths with explicit `git add <paths>` on branch `008-runner-owned-ci`; never print a secret value; prose follows `stop-slop`; code follows the surrounding style (`rust-ci-runner/src/job.rs`). Rust edits must pass `cargo fmt` and `cargo clippy -D warnings` by eye: no unused items, no `unwrap` on input. Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

**Parallel groups.** Group 1, in parallel: B1 (T002 then T003), B2 (T004), B3 (T005 then T006), B4 (T007), B6 (T012), B7 (T013, T014), B8 (T015). `lib.rs` gets `mod store_tests;` from T005 and `mod store;` from T006 (same stream); merge by keeping both if two briefs touch it. Then B9 (T016) after T014 has run on a branch push. Group 2, after B1 to B4: B5 (T008 to T011); T011 lands with the lock of T016, never without it. Owner T017 any time before T021; gate T018 after landing A. Group 3, after T018: B10 (T019, T020). Last: B11 (T022). T021 is the owner's.

**B1 (T002, T003): Loki URL and probe.**
- Goal: `LogConfig::from_env` strips `/`, then `/loki/api/v1/push`, then `/`. Extract `render(labels, meta, batch) -> String` from `Shipper::body` (which then calls it). `ship::probe(cfg, run_id, limit: Duration) -> Result<u16, u16>` builds one probe line through `render` (labels `project=probe`, `step=probe`, `stream=stdout`, so `service_name` comes for free), posts it with basic auth and no redirects, retries once after 2 s on 0 (no answer), 429 or 5xx, and returns the 2xx code or the final failing code (0 when no answer). The printed code is zero-padded to 3 digits.
- Files: `rust-ci-runner/src/ship.rs`, `ship_tests.rs`.
- Read: `ship.rs` (`LogConfig::from_env`, `SERVICE_NAME`, `Shipper::body`, `Shipper::send`), `ship_tests.rs` (`loki`, `ok_loki`, `from`, the five-label expectation).
- Accept: the `url_*` and `probe_*` tests of the plan exist and would pass; the probe body test asserts all five labels; existing tests hold; no credentials in an error string; no second JSON literal for a push body.
- Constraints: no new crate; errors name variables, never values; https and no-userinfo checks run on the normalised URL.

**B2 (T004): one choke point in `Log`, reserved step name.**
- Goal: `Log::public_events(bool)`. When on, `Log::emit` is the only path to stdout and stderr: it prints a line only if it matches the stdout grammar of FR-018 (reason cut after `failed in Ns` and `skipped`) or, through `Log::fault(text)`, a member of a `pub const FAULTS` list (the 15 lines of the plan; a text outside the list is dropped). Every printed line starts with `[`. A `who` outside `[A-Za-z0-9_-]{1,64}` prints nothing. The sink gets the full original text. `Pipeline` parse refuses a step named `runner`.
- Files: `rust-ci-runner/src/log.rs` (its `tests` module), `rust-ci-runner/src/pipeline.rs` and its tests.
- Read: `log.rs` whole, `engine.rs` lines 440 to 495 (event texts), `pipeline.rs` near `valid_name`.
- Accept: the `public_mode_*`, `fault_accepts_only_the_fixed_list` and `step_named_runner_is_refused` tests; default `Log` behaviour unchanged; `public_form(who, text) -> Option<String>` is the only place the grammar lives.
- Constraints: size-limit accounting stays on the full text; no regex crate.

**B3 (T005, then T006): engine hardening and `store.rs`.**
- Goal (T005): in `engine.rs`, `Engine::command` calls `env_clear()`, sets the allowlist (`PATH`, `HOME`, `DOCKER_HOST`, `DOCKER_CONFIG`, `DOCKER_CERT_PATH`, `DOCKER_TLS_VERIFY`, `XDG_RUNTIME_DIR`, copied from the process when set), `env_remove("BWS_ACCESS_TOKEN")`, then the explicit `env`; stdin stays null. `run_step` keeps its three `--cap-drop` flags, `no-new-privileges` and `--pids-limit 512` unchanged and adds `--memory 5g --memory-swap 5g` (constants); no `--cap-add`, no `--cpus`. Create `store_tests.rs` with the three engine tests and add `mod store_tests;` to `lib.rs` (T006 adds `mod store;`, so the tree compiles after each task). Adjust existing tests that asserted the old flag list.
- Goal (T006): `store::fetch(bws_bin: &str, token: &str, server_url: Option<&str>, limit: Duration) -> Result<Secrets, StoreError>` with `Secrets { token, log_url, log_user, log_token }`; pure `parse_list(json) -> Result<Secrets, StoreError>`. `fetch` spawns its own `Command` (not `Engine`) with `env_clear`, `PATH`, `HOME`, `BWS_ACCESS_TOKEN`, and `BWS_SERVER_URL` when given; stdin null, stdout and stderr piped; per-attempt `limit`. It may reuse the capped-read pattern of `Engine::capture`; copy it or share a small helper, whichever is the smaller diff. `StoreError` variants map to fault lines 4 to 6 and carry key names and counts only.
- Files: `rust-ci-runner/src/engine.rs`, new `store.rs`, `store_tests.rs`, `lib.rs` (`mod store_tests;` in T005, `mod store;` in T006).
- Read: `engine.rs` (`command`, `capture`, `run_step`), spec FR-009 to FR-012, 003 `contracts/bootstrap-secrets.md` steps 2 and 5.
- Accept: the `store_tests` of the plan (the nine store tests plus the three engine tests); the token goes only in the child's environment; a rejected token (stderr matches 400, 401, 403, `invalid_client`, `unauthorized`, `forbidden`, `invalid access token`) is not retried, any other failure is retried once; `server_url` must be https on `vault.bitwarden.com` or `vault.bitwarden.eu`; errors never contain stub stderr or stdout.
- Constraints: `serde_json` for parsing; `Secrets` has a hand-written `Debug` that prints no value; no value in a panic. Engine tests use `Command::get_envs` and a stub engine, never `set_var`.

**B4 (T007): shared edits in `watch.rs` and `job.rs`.**
- Goal: `watch::read(path)` (cap `MAX_BYTES`, UTF-8, parse) used by `main.rs::load_list`; in `job.rs` make `prepare`, `run_pipeline`, `with_shipper`, `log_config`, `var`, `MAX_LOG_BYTES` `pub(crate)`; extract `project_dir(ws, project) -> Result<PathBuf, String>` from `checkout` (real directory, canonical path inside `ws`); `local` returns `Err("log shipping dropped lines")` when the pipeline passed but `finish` returned false.
- Files: `rust-ci-runner/src/watch.rs`, `watch_tests.rs`, `job.rs`, `job_tests.rs`, `main.rs` (only `load_list`).
- Accept: `read_helper_parses_and_caps_like_the_old_load_list`, `entries_without_branches_match_every_ref`, `run_quiet_logs_fails_when_a_passing_run_dropped_lines` (stub engine, a Loki that answers 500, a short retry window through `Timing`); existing tests pass except where `local` changes documented behaviour.
- Constraints: `job` behaviour otherwise unchanged; `load_list` keeps its `LogConfig::from_env` check.

**B5 (T008 to T011): the `ci` command.**
- Goal: `ci.rs`, `ci_tests.rs`, `main.rs` and `lib.rs` wiring, usage text, panic hook, `.factory-ci.toml`, version 0.2.0.
- Files: new `rust-ci-runner/src/ci.rs`, `ci_tests.rs`; `main.rs`, `lib.rs`, `rust-ci-runner/.factory-ci.toml`.
- Read: this plan (flow, contracts, child table, test plan, invariants), spec FR-006 to FR-024, `job.rs`, `ship.rs`, `store.rs`, `log.rs`, `engine.rs`, `watch.rs`.
- Accept: every `ci_tests` row of the test plan exists; `ci::run(args, env, seams) -> i32` takes the environment as a closure and the `bws` path, engine path, `git` path, clone base URL, allowed protocol, API base URL and `Limits` as parameters; `ci()` in `main.rs` reads `BWS_ACCESS_TOKEN` once, calls `remove_var`, and passes the real seams; `parse_args` rejects repeated flags, unknown flags and values starting with `--`, and refuses a `--work` that differs from its canonical path; every `ci` `Command` has explicit stdio and the allowlist env of the plan's child table; the run-wide step env (`run_env`) adds `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=safe.directory`, `GIT_CONFIG_VALUE_0=/workspace`; no step argv or runner container carries `DOCKER_CONFIG`; the identity check, the `git init`, `fetch refs/heads/<ref>` and `checkout --detach FETCH_HEAD` sequence is as in the flow; stderr lines come only from `Log::fault`; the redactor holds all values and forms of I2; exit code is 0 or 1 only. T011: `.factory-ci.toml` runs `cargo test --locked --lib -- --skip ship_tests:: --skip job_tests:: --skip ci_tests::`, then `ship_tests::`, `job_tests::`, `ci_tests::` as separate commands (all `--locked`), comments updated (drop the Cargo.lock shortcut line). The version bump to 0.2.0 is in T014, not here. T011 lands together with the lock of T016, never without it.
- Constraints: no `set_var`; no `unwrap` on input; never format an error from git, bws, docker, TOML or the API into stdout or stderr; Loki gets the detail through `Log::out` on stream `stderr` for step name `ci`.

**B6 (T012): Dockerfile.**
- Goal: the image of the plan's Image section, with real values.
- Files: `rust-ci-runner/Dockerfile`.
- Steps: read the multi-arch index digest of `alpine:3.23` without docker: `T=$(curl -s "https://auth.docker.io/token?service=registry.docker.io&scope=repository:library/alpine:pull" | sed 's/.*"token":"\([^"]*\)".*/\1/')`, then `curl -sI -H "Authorization: Bearer $T" -H "Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json" https://registry-1.docker.io/v2/library/alpine/manifests/3.23` and read `Docker-Content-Digest`. Download `bws-sha256-checksums-2.1.0.txt` from the `bws-v2.1.0` release of `bitwarden/sdk-sm` and copy the hashes of both musl zips; the arm64 one must equal `eb0f1ae61d1c3b74244d2841233276e05c77e8be4da197ed90fc6248387005e1` and the amd64 one `f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6`. Write values into the file with `sed -i` from shell variables or copy and paste; never retype a hash; after writing, `grep` the file for each value.
- Accept: the stage layout of the plan, `ARG TARGETARCH` before the first `FROM` and again right after the last `FROM` (a global ARG is not visible inside a stage), `ADD --checksum=sha256:...` per architecture, `unzip -p bws.zip bws > bws` in a stage that ran `apk add unzip`, `LABEL org.opencontainers.image.source=https://github.com/sudtanj/infrastructure`, binary at `/rust-ci-runner`, user `65532:65532`, no `latest`, no `curl | sh`, comments saying why each tool is there and that the path stays for the tick.
- Constraints: if a lookup fails (no network, digest header absent), stop and report; do not guess.

**B7 (T013, T014): release workflow and selftest.**
- Goal T013: in the `publish` job, add `docker/setup-qemu-action` pinned by a 40-hex SHA (resolve `v3` with `git ls-remote https://github.com/docker/setup-qemu-action 'refs/tags/v3*'`, use the dereferenced `^{}` commit when listed) before buildx; `provenance: mode=max` on `build-push-action`; after the push a smoke step: `docker run` the digest (`${{ steps.push.outputs.digest }}` through `env:`) with the flags of the workflow's `docker run` and `--entrypoint sh -c 'git --version && docker version && bws --version'`, one arm64 run with `--platform linux/arm64 --entrypoint sh -c 'bws --version'`, then `docker create`, `docker cp "$id:/rust-ci-runner" ./out` and `docker rm`; move `gh release create` after the smoke and make it `gh release view "$TAG" >/dev/null 2>&1 || gh release create ...`; the summary names `RUNNER_IMAGE_DIGEST` (tick) and `RUNNER_IMAGE` (manual workflow) with the digest. A further smoke row runs `ci --repos /etc/factory-ci/repos.toml --repo x/y --ref main --work <dir>` with the workflow's `docker run` flags (the publish job needs a pinned checkout to mount `.github/factory-ci-repos.toml`) and requires stderr `[runner] repository or ref not in the watch list` and exit 1. The `docker login` stays.
- Goal T014: `factory-ci-selftest.yml` gets a `push` trigger (`branches-ignore: [main]`, same `paths`); the `list` job runs `cargo test --lib --no-run` then `cargo run -q -- plan --repos ../.github/factory-ci-repos.toml --check` from `rust-ci-runner` (replacing `cargo build --release` there); the `list` job also runs `cargo clippy --locked -- -D warnings` (after the generator step, so a lock exists for `--locked`); the `pipeline` job, after its release build, stages `image/amd64/rust-ci-runner` and the same binary as `image/arm64/rust-ci-runner` (a stand-in; the build only copies it), copies the Dockerfile, sets up QEMU and buildx (pinned by SHA) and runs `docker buildx build --platform linux/amd64,linux/arm64 image` with no push; `rust-ci-runner/Cargo.toml` version becomes `0.2.0` here, before the lock is generated, so the lock carries the final version; a `workflow` job step greps `factory-ci-selftest.yml` and fails on `environment:`, any `secrets.` other than `GITHUB_TOKEN`, `id-token` or `pull_request_target` (write the patterns as `[e]nvironment:` and so on, or the step matches itself); a temporary step in the `list` job runs `cargo generate-lockfile` and uploads `rust-ci-runner/Cargo.lock` (`actions/upload-artifact`, pinned by SHA); a header comment records the exception to "only the release workflow compiles the runner".
- Files: `.github/workflows/rust-ci-runner-release.yml`, `.github/workflows/factory-ci-selftest.yml`.
- Read: `.github/scripts/factory-ci-lint-workflow.sh` fully; it lints the release workflow.
- Accept: every `uses:` pinned to 40-hex; `permissions` unchanged; the lint rules still hold on the release file by reading them; YAML parses (check with `python3 -c 'import yaml,sys;yaml.safe_load(open(sys.argv[1]))' FILE` if python is present, else by reading).
- Constraints: no new action beyond QEMU and upload-artifact; the build jobs stay as 007 left them except `--locked` in T016.

**B8 (T015): watch list entries.**
- Goal: two `[[watch]]` tables: `sudtanj/infrastructure` and `sudtanj/factory-monorepo`, each with `id`, `branches = ["main"]`, no `allow_docker`. Header gains two comment lines: `ci` reads `repo`, `id` (required), `branches`, `allow_docker` and ignores the rest; listing a repo also makes the by-hand tick plan it.
- Files: `.github/factory-ci-repos.toml`.
- Steps: `gh api repos/sudtanj/infrastructure --jq .id` and the same for `sudtanj/factory-monorepo`. If the second fails, stop and report; do not guess.
- Accept: `plan --repos .github/factory-ci-repos.toml --check` would report `2 entries ok` (the selftest job `list` runs it); existing comments kept.
- Constraints: no `secrets`, no `secret_branches`.

**B9 (T016): `Cargo.lock`.**
- Goal: commit `rust-ci-runner/Cargo.lock` generated by CI, then switch the builds to `--locked`.
- Steps: after T014 is pushed, wait for the selftest run on the branch (`gh run list --branch 008-runner-owned-ci`, `gh run watch`), `gh run download <id> -n <artifact> -D <new empty dir>`, copy the file to `rust-ci-runner/Cargo.lock` (treat the download as untrusted: check it starts with `# This file is automatically @generated by Cargo.` and lists only the crates of `Cargo.toml` and their dependencies), then add `--locked` to every `cargo build` and `cargo test` in the release workflow and the selftest, and remove the generator and upload steps.
- Files: `rust-ci-runner/Cargo.lock`, `.github/workflows/rust-ci-runner-release.yml`, `.github/workflows/factory-ci-selftest.yml`.
- Accept: the lock is committed (not ignored); every `cargo build` and `cargo test` line in those workflows has `--locked`.
- Constraints: if the artifact is missing, stop and report.

**B10 (T019, T020): the trimmed workflow and its shell test.**
- Goal: `factory-monorepo-ci.yml` as FR-001 to FR-005 say, `RUNNER_IMAGE` set to the digest from the release (lead supplies it); the header comment names the owner exception, the specs it supersedes (005, 006, 007) and "never run this job on a self-hosted runner". `.github/scripts/test-factory-monorepo-ci.sh` with the checks in the test plan, a plain grep script, wired into the `shell` job of `factory-ci-selftest.yml`.
- Files: `.github/workflows/factory-monorepo-ci.yml`, `.github/scripts/test-factory-monorepo-ci.sh`, `.github/workflows/factory-ci-selftest.yml`.
- Accept: `bash -n` on the new script and the extracted `run:` block; YAML parses; no `${{` in a `run:`; every variable in `docker run` quoted; the script passes against the file by reading; the script has no `--self-test` mode.
- Constraints: start only after the lead gives the verified digest; no placeholder in the merged file.

**B11 (T022): docs and supersession.**
- Goal: `README.md` section "Run factory-monorepo CI by hand" rewritten (inputs, flow, secrets and variables, owner exception, selftest exception, oversize-line behaviour, owner settings of T017); `rust-ci-runner/README.md` (command `ci`, image contents, release and bootstrap order, minimal-output rule); `**Status**: Draft` of `specs/005-plain-actions-ci/spec.md`, `specs/006-runner-unit-tests/spec.md` and `specs/007-runner-image/spec.md` gets "Superseded in part by 008-runner-owned-ci" for the parts the 008 spec lists; the GitHub repository description checked, new text given for the owner to apply with `gh repo edit` if it changes.
- Files: the two READMEs, three spec files.
- Accept: every claim matches the merged workflow; no em dashes, no filler.
- Constraints: last task; load `stop-slop` first.
