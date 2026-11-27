# Feature Specification: Runner-owned CI

**Feature Branch**: `008-runner-owned-ci`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "Decouple the pipeline from GitHub Actions. `factory-monorepo-ci.yml` becomes only a trigger. The Bitwarden fetch, the clone, the Loki URL fix and check, the docker-step refusal and the minimal-output filter move into rust-ci-runner and its TOML config. The runner runs as a docker image with the host docker socket mounted."

## Clarifications

### Session 2026-11-27 (owner decisions)

- Q: Where does the CI logic live? → A: In the runner and its TOML. Owner: "decoupled from github action, it should be inside the toml not on github action". The owner moved all of these out of `.github/workflows/factory-monorepo-ci.yml` into rust-ci-runner and TOML config: the Bitwarden fetch (BWS), the clone of the target repo, the Grafana Loki URL fix and check, the docker-step refusal, and the minimal-output filter. The workflow becomes only a trigger.
- Q: How does the runner start step containers? → A: The runner runs as the docker image. Owner: `docker run` of `ghcr.io/sudtanj/rust-ci-runner`, with the host docker socket mounted, so the runner starts the step containers itself. The image therefore gains a docker CLI, plus what else it needs (git, CA certificates, and bws or a native Bitwarden client).
- Q: Who builds the runner? → A: Owner: building the runner itself stays the exception. Only `.github/workflows/rust-ci-runner-release.yml` compiles it. Every pipeline, including the runner's own unit tests (`rust-ci-runner/.factory-ci.toml`), runs through the released image.
- Q: How do runs start, and which secrets exist? → A: Owner: runs start by hand only (`workflow_dispatch`), from `main` only, with the Environment `factory-ci` that holds the single secret `BWS_ACCESS_TOKEN`. One Bitwarden machine account, limited to the project `rust-ci-runner`, holds `FACTORY_CI_TOKEN` (clone token for private repos), and `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN` (Grafana Cloud Loki).
- Q: What does the public Actions log show? → A: Owner: minimal output only. The runner's event lines (`[step] started|success in Ns|failed in Ns|skipped`, `[runner] pipeline passed|failed`), without failure reasons. Step output goes only to Loki.

### Session 2026-11-27 (planning calls, no owner answer needed)

- Q: Which TOML names what to run? → A: The existing watch list `.github/factory-ci-repos.toml`, unchanged in format. An entry says which repo may run (`repo`), which refs may run (`branches`, default all) and whether docker steps are allowed (`allow_docker`). It has no project key, because the tick finds projects by walking the tree, so the project is a workflow input (default `.`). A new key would change what the tick plans.
- Q: Does a step get pipeline secrets? → A: No, as in 005. The runner looks up no secret for steps, so a step that declares `secrets` fails the run. Bitwarden is read only for the four bootstrap keys.
- Q: Native Bitwarden client or `bws`? → A: `bws` 2.1.0, pinned by the SHA-256 the 003 branch already uses (`f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6` for the x86_64 musl zip). A native client would have to re-implement token decoding, key derivation and AES-CBC-HMAC decryption of every secret: more security-critical code than the whole `ci` command, with no upstream test suite. The `bws` call is a child process and the token travels in its environment.
- Q: Which architectures does the image have? → A: `linux/amd64` and `linux/arm64`, as in 007. Owner decision (Q1): the image keeps both. `bws` 2.1.0 has an aarch64 musl asset (SHA-256 `eb0f1ae61d1c3b74244d2841233276e05c77e8be4da197ed90fc6248387005e1`), so the Dockerfile has one `bws` stage per architecture, each pinned with `ADD --checksum`, and the release build adds QEMU.
- Q: What was the cause of the missing Grafana logs? → A: The likely cause is a `FACTORY_CI_LOG_URL` that included `/loki/api/v1/push`; the runner appends the path again. The runner now strips the suffix, probes the store before the first step, and exits non-zero when shipping drops lines.
- Q: The constitution says Loki credentials come from GitHub. → A: Principle II was amended to 1.1.0 in this feature (T001): the manual pipeline workflow holds only `BWS_ACCESS_TOKEN` and reads the rest from Bitwarden.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - One trigger, one container (Priority: P1)

The owner opens Actions, starts `factory-monorepo-ci` on `main` with a repo, a ref and a project, and the job does one thing: run the pinned runner image. The runner reads Bitwarden, clones the repo, checks the log store, runs the pipeline and returns an exit code.

**Why this priority**: It is the whole feature. Everything else here exists to make this safe.

**Independent Test**: Start the workflow with repo `sudtanj/factory-monorepo`, ref `main`, project `.`. The `run:` block holds a registry login, one directory creation and one `docker run`. The job ends with the runner's exit code.

**Acceptance Scenarios**:

1. **Given** the workflow runs on `main` with a repo in the watch list, **When** every step passes, **Then** the job succeeds and the log shows one `started` and one `success in Ns` line per step and `[runner] pipeline passed`.
2. **Given** a step fails, **When** the run ends, **Then** the job fails, the log shows `[step] failed in Ns` and `[runner] pipeline failed`, and no reason.
3. **Given** the workflow is started from a branch other than `main`, **When** the job is evaluated, **Then** it does not run.
4. **Given** repo `sudtanj/infrastructure`, ref `main`, project `rust-ci-runner`, **When** the run ends, **Then** the unit tests of the runner ran through the released image.
5. **Given** a repo that is not in `.github/factory-ci-repos.toml`, or a ref that no `branches` pattern of its entry matches, **When** the runner starts, **Then** it exits 1 before it reads Bitwarden.

---

### User Story 2 - Nothing private reaches the public log (Priority: P1)

The repo and its logs are public; the target repos are private. A reader learns step names, pass or fail and durations, and nothing else.

**Why this priority**: Constitution Principle III. A leak cannot be undone.

**Independent Test**: Run the runner against a fixture whose steps print secret look-alikes, forged result lines, workflow commands and a TOML syntax error. Capture stdout and stderr. Every line matches the allowed grammar.

**Acceptance Scenarios**:

1. **Given** a step prints `[x] success in 0s` or `::add-mask::abc`, **When** the run ends, **Then** neither text appears in the runner's stdout or stderr.
2. **Given** a step fails with a long reason, **When** the failure event prints, **Then** it reads `[name] failed in Ns` and the reason reaches Loki only.
3. **Given** the pipeline file has a TOML error or a bad field, **When** the runner refuses it, **Then** the public line is the fixed text `pipeline file refused` and the detail reaches Loki.
4. **Given** the runner panics, **When** the process ends, **Then** stderr shows `internal error` and no panic text.
5. **Given** the five bootstrap values, **When** the run ends, **Then** none of them, nor their base64 forms, appears in stdout, stderr, the argv or the environment of any child process.

---

### User Story 3 - Logs reach Loki or the run fails (Priority: P1)

The owner saw no logs in Grafana and nothing said so. The runner now fixes the URL form, proves the store accepts a line before the first step, and fails when lines are dropped later.

**Why this priority**: With step output kept out of the public log, a run whose logs go nowhere is worse than a failed run.

**Independent Test**: Mock Loki. Run with a URL ending in `/loki/api/v1/push`, with a wrong user, and with a store that dies mid-run.

**Acceptance Scenarios**:

1. **Given** `FACTORY_CI_LOG_URL` ends with `/`, `/loki/api/v1/push` or `/loki/api/v1/push/`, **When** the settings load, **Then** the stored URL has none of those endings.
2. **Given** correct settings, **When** the probe posts one line, **Then** stdout shows `[runner] log store answered HTTP 204` and the first step starts.
3. **Given** the store answers a non-2xx code, or nothing within 15 s, after one retry for no answer, 429 and 5xx, **When** the probe ends, **Then** the runner exits 1 with a fixed line (the hint only for 401 and 403) and starts no step.
4. **Given** the pipeline passed but a push was dropped, **When** the runner finishes, **Then** it prints `[runner] log lines were dropped` and exits 1.

---

### User Story 4 - A hostile branch gets nothing (Priority: P2)

A branch of a watched repo carries a malicious `.factory-ci.toml`. The runner holds the docker socket, so the file must not reach the socket, the secrets or the public log.

**Why this priority**: The socket is root on the VM. The design is only acceptable if untrusted input cannot reach it.

**Independent Test**: Fixtures with `docker = true`, with `secrets = ["X"]`, with an image starting with `-`, with a symlinked `.factory-ci.toml` and with a symlinked project folder.

**Acceptance Scenarios**:

1. **Given** a step has `docker = true`, **When** the runner reads the file, **Then** it exits 1 with `docker steps are refused` before it starts any container. The flag `--allow-docker` lifts this only when the watch entry also sets `allow_docker = true`.
2. **Given** a step declares `secrets`, **When** the runner prepares the run, **Then** the run fails before any container and no value is looked up.
3. **Given** any step container, **When** it starts, **Then** its argv holds no docker socket mount and its environment holds no bootstrap value.

---

### User Story 5 - The image carries everything and ships first (Priority: P2)

The release builds an image with the runner, a docker CLI, git, CA certificates and `bws`, pinned by digest, before the trimmed workflow uses it.

**Why this priority**: Without the image, US1 cannot run. It has to exist before the workflow changes.

**Independent Test**: Push `rust-ci-runner-v0.2.0` on a commit on `main`. The release publishes the image and prints its digest.

**Acceptance Scenarios**:

1. **Given** the tag, **When** the release runs, **Then** the image holds `/rust-ci-runner`, `docker`, `git`, `bws` and CA certificates, and the `bws` archive matched its SHA-256 at build time.
2. **Given** the trimmed workflow names a digest that is not published, **When** the job starts, **Then** it fails at the pull and no pipeline runs.

### Edge Cases

- `ref` has a leading `-`, contains `..`, or uses characters outside `[A-Za-z0-9._/-]`: the runner exits 1 before it reads Bitwarden.
- `project` is not `.` or a top-level folder name, or is a symlink or leaves the clone: exit 1.
- `BWS_ACCESS_TOKEN` is unset or empty: exit 1, fixed text, no `bws` call.
- Bitwarden holds zero or two secrets with one of the four keys: exit 1, names the key and the count, never a value. A value that is empty, multi-line, shorter than 4 characters or padded with whitespace: exit 1.
- Bitwarden is down or slow: each call has a time limit, one retry, then exit 1.
- The Loki URL is not https (except loopback), carries a user name, or has a path other than the push path: refused or kept as given, never doubled.
- The docker socket is missing or its group is not added: exit 1 with `cannot reach the container engine`, before the clone.
- The work directory is missing, relative, or contains `:`: exit 1.
- The clone times out or the ref does not exist: exit 1 with `clone failed`; git's own text goes to Loki, redacted.
- The GitHub API says the repo id differs from the watch entry (renamed, moved, name taken), or answers an error: exit 1 with `repository identity check failed`, before the fetch.
- Two runs of the same repo, ref and project start together: they queue.
- The step runs longer than the run deadline of 30 minutes: the runner ends it and exits 1. The job limit is longer than the runner deadline.
- Loki fails after the probe passed: the run goes on, and a pass ends with `[runner] log lines were dropped` and exit 1.
- A step prints more than 10 MB: the log is cut and Loki gets the cut notice.
- The runner image has no digest or a wrong one: the pull fails, the job fails.

## Requirements *(mandatory)*

### Functional Requirements

**Workflow**

- **FR-001**: `factory-monorepo-ci.yml` MUST start only by `workflow_dispatch` with inputs `repo` (default `sudtanj/factory-monorepo`), `ref` (default `main`) and `project` (default `.`), run only on `main`, in the Environment `factory-ci`, with job permissions `contents: read` and `packages: read` and an empty top-level `permissions`. The job limit is 45 minutes (the budgets sum to about 38), longer than the runner deadline of 30. Concurrency is per repo, ref and project. The header comment says the job must never run on a self-hosted runner.
- **FR-002**: The workflow MUST contain no Bitwarden call, no `jq`, no `curl`, no `git clone`, no output filter and no secret other than `secrets.BWS_ACCESS_TOKEN` and `secrets.GITHUB_TOKEN`. Its only steps are the pinned checkout (for the watch list) and one step that logs in to ghcr.io over stdin with a temporary `DOCKER_CONFIG`, creates the work directory and runs one `docker run`. The `run:` block holds at most 25 lines.
- **FR-003**: The `docker run` MUST be `docker run -q`, use the image in the env line `RUNNER_IMAGE` (`ghcr.io/sudtanj/rust-ci-runner:<version>@sha256:<digest>`), mount the host socket, mount the work directory `$RUNNER_TEMP/ci` at the identical path, mount the watch list read-only, and run with `--init`, as the runner user (`--user $(id -u):$(id -g)`) with the socket group added (`--group-add $(stat -c %g /var/run/docker.sock)`), `--cap-drop ALL`, `--security-opt no-new-privileges`, `--read-only`, `--tmpfs /tmp:rw,size=64m` and `HOME=/tmp`. It passes `BWS_ACCESS_TOKEN`, `GITHUB_RUN_ID` and `FACTORY_CI_BWS_SERVER_URL` by name only (`-e NAME`), never by value in argv. Every variable in the command is quoted and the workflow never passes `--allow-docker`. The image pull happens inside this command. The login uses a temporary `DOCKER_CONFIG` removed on exit and prints nothing.
- **FR-004**: Every input and context value MUST reach the script through `env:`. No `${{ }}` sits inside a `run:` block.
- **FR-005**: The workflow MUST NOT post commit statuses or write a job summary.

**CLI**

- **FR-006**: The runner MUST gain `rust-ci-runner ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]`. It reads the watch list, requires an entry for `--repo` that has a numeric `id` and whose `branches` match `--ref`, and exits 1 otherwise, before it reads Bitwarden.
- **FR-007**: The runner MUST validate `--ref` (`[A-Za-z0-9._/-]`, 1 to 200 characters, no leading `-`, no `..`), `--project` (`.` or a top-level folder name that passes `folder_name_ok`) and `--work` (absolute, no `:`, an existing directory, equal to its canonical path) before any other work, and MUST refuse a repeated flag, an unknown flag, and any flag value that starts with `--`.
- **FR-008**: Exit codes MUST be: 0 the pipeline passed and every log line was delivered; 1 anything else. A pass that dropped log lines prints `[runner] log lines were dropped` and exits 1.

**Secrets**

- **FR-009**: The runner MUST read `BWS_ACCESS_TOKEN` once at start and remove it from its own process environment before any thread starts, then run `bws secret list -o json` as a child process with a per-attempt limit (60 s in production, a parameter) and one retry (none after a rejected token). The child is spawned with a cleared environment holding only `PATH`, `HOME`, `BWS_ACCESS_TOKEN` and `BWS_SERVER_URL`; the token is never in argv. `FACTORY_CI_BWS_SERVER_URL`, when set and not empty, must be https on `vault.bitwarden.com` or `vault.bitwarden.eu` and goes to the child as `BWS_SERVER_URL`.
- **FR-010**: The runner MUST require exactly one secret for each of `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, and refuse a value that is empty, has CR, LF or NUL, has leading or trailing whitespace, or is shorter than 4 characters. Errors name keys and counts only.
- **FR-011**: The fetched values MUST stay in memory. They MUST NOT be set in the runner's own environment, passed to step containers, or printed. The redactor MUST cover all four values, the access token, their base64 forms and the base64 of `x-access-token:<FACTORY_CI_TOKEN>`.
- **FR-012**: Every engine child process MUST start with a cleared environment plus an allowlist (`PATH`, `HOME`, `DOCKER_HOST`, `DOCKER_CONFIG`, `DOCKER_CERT_PATH`, `DOCKER_TLS_VERIFY`, `XDG_RUNTIME_DIR`) and the call's explicit entries, and never inherit `BWS_ACCESS_TOKEN`. No bootstrap value reaches an engine child or a step container; `git` and `bws` children receive only what they need.

**Clone**

- **FR-013**: The runner MUST fetch `https://github.com/<repo>.git` into a new directory under `--work` with `git` as a child process: `git init`, then `git fetch --depth 1 --no-tags -- <url> refs/heads/<ref>` (only a branch can match, never a tag), then `git checkout --detach FETCH_HEAD`; no submodules. The host is fixed; no environment variable changes it. Before the fetch the runner MUST compare the `id` of `GET /repos/<repo>` on the GitHub API with the watch entry's `id`, and refuse on a mismatch or an error. The clone token travels in git's environment as an `http.https://github.com/.extraHeader` entry (`Authorization: basic base64("x-access-token:<token>")`), never in argv or `.git/config`. The fetch has a 300 s limit. The allowed protocol (`GIT_ALLOW_PROTOCOL`) and the base URL are parameters of the code: production `https`, tests `file`.
- **FR-014**: The project directory MUST be a real directory inside the clone, not a link. `.factory-ci.toml` MUST be a regular file, not a link.

**Loki**

- **FR-015**: `LogConfig` loading MUST strip trailing `/` and then a trailing `/loki/api/v1/push` (and a `/` after it) from the URL, for every command that loads it. Normalising twice gives the same result.
- **FR-016**: Before the first container, the runner MUST post one probe line to the store (15 s limit, https, basic auth, retried once on no answer, 429 and 5xx), built by the same code as every other push body so it carries the same five labels (`service_name`, `repo`, `project=probe`, `step=probe`, `stream=stdout`). It prints `[runner] log store answered HTTP <code>` (`000` when nothing answered) and exits 1 on a non-2xx code. The hint `check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)` prints only for 401 and 403.
- **FR-017**: At the end, the runner MUST wait for the shipper to drain and, if a line was dropped on a passing run, print `[runner] log lines were dropped` and exit 1. `run --quiet-logs` MUST apply the same rule.

**Minimal output**

- **FR-018**: In `ci` mode stdout MUST carry only these lines: `[<name>] started`, `[<name>] success in <N>s`, `[<name>] failed in <N>s`, `[<name>] skipped`, `[runner] pipeline passed`, `[runner] pipeline failed`, `[runner] log store answered HTTP <code>`, `[runner] log lines were dropped`. `<name>` is a step name (`[A-Za-z0-9_-]`, 1 to 64 characters, never `runner`, which is reserved and refused as a step name) or `runner`. The runner MUST drop the reason after `failed in Ns` and after `skipped` from the printed line. Loki gets the full text.
- **FR-019**: In `ci` mode stderr MUST carry only fixed fault lines `[runner] <text>` from a closed list in `plan.md`. No error text from git, bws, docker, TOML parsing or a panic reaches stdout or stderr. The detail goes to Loki once the store is proven, redacted.
- **FR-020**: Step output MUST go to Loki only.

**Containment**

- **FR-021**: The runner MUST refuse a step with `docker = true` before it starts any container, unless `--allow-docker` is given and the watch entry has `allow_docker = true`. With `--allow-docker`, `FACTORY_CI_DIND_IMAGE` must be a digest-pinned reference, as in `job`.
- **FR-022**: The runner MUST look up no step secret. A step that declares `secrets` fails the run before any container.
- **FR-023**: Before the clone, the runner MUST check the engine answers (`docker version`) and exit 1 with `cannot reach the container engine` if not.
- **FR-024**: Step containers MUST NOT get the docker socket, and the bind-mount source for `/workspace` MUST be the same string as the path the runner sees. Workspace paths are never rewritten. Step containers MUST run with exactly the existing three-capability drop (`--cap-drop NET_RAW`, `MKNOD`, `SETFCAP`), `--security-opt no-new-privileges`, `--pids-limit 512`, `--memory 5g` and `--memory-swap 5g`, and never with any `--cap-add`, `--privileged`, `--pid`, `--network host`, another `-v`, `DOCKER_CONFIG` or `docker.sock` in argv. The run-wide step environment sets `GIT_CONFIG_COUNT=1`, `GIT_CONFIG_KEY_0=safe.directory`, `GIT_CONFIG_VALUE_0=/workspace`.

**Image and release**

- **FR-025**: The image MUST be `linux/amd64` and `linux/arm64`, built from `alpine:3.23` pinned by the multi-arch index digest, hold `/rust-ci-runner`, a docker CLI, `git`, CA certificates and `bws` 2.1.0, carry `LABEL org.opencontainers.image.source=https://github.com/sudtanj/infrastructure`, and run as a non-root user. Each `bws` zip MUST be fetched with `ADD --checksum=sha256:...` (amd64 `f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6`, arm64 `eb0f1ae61d1c3b74244d2841233276e05c77e8be4da197ed90fc6248387005e1`) and extracted with `unzip -p bws.zip bws > bws` in a throwaway stage; a hash mismatch fails the build. The digest and hashes are copied from a lookup, not typed.
- **FR-026**: `rust-ci-runner-release.yml` MUST build both binaries with `--locked`, build and push the image (QEMU, `provenance: mode=max`), smoke-test the pushed digest with the workflow's `docker run` flags (`git --version`, `docker version`, `bws --version`, plus `docker cp` of `/rust-ci-runner`), and only then create the GitHub release, idempotently (`gh release view "$TAG" >/dev/null 2>&1 || gh release create ...`), keeping both binary assets. Its summary names both digest pins: the tick variable `RUNNER_IMAGE_DIGEST` and `RUNNER_IMAGE` of the manual workflow. It publishes version 0.2.0 (`Cargo.toml` and the tag agree). The lint script and its self-test rows are unchanged because the platforms stay `linux/amd64,linux/arm64`.
- **FR-027**: Only `rust-ci-runner-release.yml` MUST compile the runner for release. As a conscious pre-merge exception, `factory-ci-selftest.yml` keeps its compile jobs: one runs `cargo test --lib --no-run` and `cargo clippy --locked -- -D warnings`, the other builds the release binary and a no-push Dockerfile build for `linux/amd64,linux/arm64` with QEMU. A selftest step fails if the file contains `environment:`, a `secrets.` name other than `GITHUB_TOKEN`, `id-token` or `pull_request_target`. The selftest also runs on a push to any branch except `main`, because pull requests return 403. `rust-ci-runner/Cargo.lock` MUST be committed and every `cargo build` and `cargo test` of the release, the selftest and `rust-ci-runner/.factory-ci.toml` MUST use `--locked`. `rust-ci-runner/.factory-ci.toml` MUST run all `--lib` tests, with one extra process for each of `ship_tests::`, `job_tests::` and `ci_tests::`, through the released image.

**Tests and docs**

- **FR-028**: Each behaviour above MUST have a Rust unit test under `cargo test --lib` that runs in the `rust-ci-runner` pipeline. A shell test of the workflow file MUST run in the `factory-ci-selftest` workflow.
- **FR-029**: `README.md` and `rust-ci-runner/README.md` MUST describe the new CLI, workflow inputs, image contents, bootstrap order and the minimal-output rule, and the GitHub repository description MUST be checked against them. This is the last task.
- **FR-030**: Every child process the runner spawns (`bws`, `git`, the engine) MUST have stdin, stdout and stderr set explicitly (null or piped, never inherited) and a cleared environment plus its allowlist. All printed output MUST pass one function, `Log::emit`, which drops any line outside the closed grammar or the fixed fault list; every printed line starts with `[`.
- **FR-031**: A single log line over the store's per-line limit makes the store reject its batch; the shipper drops it and the run ends with `[runner] log lines were dropped` and exit 1. The README says so; the code adds no cap.

### Key Entities
- **Watch entry**: `repo`, `id` (required by `ci`), `branches`, `allow_docker` from `.github/factory-ci-repos.toml`. `secret_branches` and `secrets` are ignored by `ci`.
- **Run**: one `ci` invocation. Inputs `repo`, `ref`, `project`, `work`. Result is an exit code, 0 or 1.
- **Bootstrap values**: `BWS_ACCESS_TOKEN` (GitHub) and the four Bitwarden keys.
- **Public line**: a stdout or stderr line from the closed grammar of FR-018 and FR-019.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: One manual start runs a pipeline of a watched repo and ends in under 45 minutes with the runner's exit code, and the `run:` block of the workflow has no more than 25 lines.
- **SC-002**: Across stdout and stderr of a run and a fixture with hostile content, zero lines fall outside the closed grammar, and zero of the five bootstrap values appear anywhere.
- **SC-003**: A start from a branch other than `main` runs no job. A repo or ref outside the watch list exits 1 before any Bitwarden call.
- **SC-004**: With a wrong Loki user, the run exits 1 within 20 seconds of the probe starting and starts no step. With a store that fails after the probe, a passing pipeline prints `log lines were dropped` and exits 1.
- **SC-005**: `cargo test --lib` passes in the `rust-ci-runner` pipeline with the new tests, and the word `cargo` is absent from `factory-monorepo-ci.yml`.
- **SC-006**: The release publishes a digest, and the first run with that digest passes both `sudtanj/infrastructure` project `rust-ci-runner` and `sudtanj/factory-monorepo`.

## Assumptions

- Supersedes 005 (FR-001 to FR-012 where they conflict), 006 FR-003 to FR-008 and 007 FR-002 to FR-009 for `factory-monorepo-ci.yml`. The `pipeline` input is replaced by `repo` and `project`. `specs/007-runner-image/` FR-012 (native arm64 build) stays.
- The Environment `factory-ci` exists, allows only `main`, and holds `BWS_ACCESS_TOKEN`. The variable `FACTORY_CI_BWS_SERVER_URL` is optional and should stay unset.
- The machine account is limited to the Bitwarden project `rust-ci-runner`, which holds exactly the four keys.
- `FACTORY_CI_TOKEN` can read `sudtanj/factory-monorepo` and can clone the public `sudtanj/infrastructure`.
- The hosted `ubuntu-24.04` runner has a docker daemon, `/var/run/docker.sock` owned by a group that the workflow adds to the container with `--group-add`, and `stat`.
- `factory-ci-selftest.yml` keeps its compile jobs as a pre-merge exception (owner decision Q2), changes one to `cargo test --lib --no-run`, and gains a `push` trigger for branches other than `main`.
- Adding `sudtanj/factory-monorepo` and `sudtanj/infrastructure` to the watch list also makes the by-hand tick plan them (the header of the watch list says so). Both entries carry their numeric `id` and `branches = ["main"]`. The tick is not in use today.
- The first release of the new image is `rust-ci-runner-v0.2.0`. Release `0.1.0` stays in ghcr.io, so a revert of the workflow works.
- Out of scope: pipeline secrets for steps, commit statuses, caching, `cargo deny` and `cargo audit` (follow-up), blocking the metadata endpoint (the ephemeral hosted VM is the containment; the job must never run on a self-hosted runner), and any change to the tick workflow.
