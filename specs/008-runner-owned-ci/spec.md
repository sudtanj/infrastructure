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
- Q: Which architectures does the image have? → A: `linux/amd64` only. The only consumer is the hosted `ubuntu-24.04` runner. An arm64 image would need QEMU in the release build and an aarch64 `bws` asset whose hash and existence are unverified. The arm64 binary stays a release asset. See the owner question in `plan.md`.
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
5. **Given** a repo that is not in `.github/factory-ci-repos.toml`, or a ref that no `branches` pattern of its entry matches, **When** the runner starts, **Then** it exits 2 before it reads Bitwarden.

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
3. **Given** the store answers a non-2xx code or nothing within 15 s, **When** the probe ends, **Then** the runner exits 2 with the fixed hint and starts no step.
4. **Given** the pipeline passed but a push was dropped, **When** the runner finishes, **Then** it exits 3 and prints `log lines were dropped`.

---

### User Story 4 - A hostile branch gets nothing (Priority: P2)

A branch of a watched repo carries a malicious `.factory-ci.toml`. The runner holds the docker socket, so the file must not reach the socket, the secrets or the public log.

**Why this priority**: The socket is root on the VM. The design is only acceptable if untrusted input cannot reach it.

**Independent Test**: Fixtures with `docker = true`, with `secrets = ["X"]`, with an image starting with `-`, with a symlinked `.factory-ci.toml` and with a symlinked project folder.

**Acceptance Scenarios**:

1. **Given** a step has `docker = true`, **When** the runner reads the file, **Then** it exits 2 with `docker steps are refused` before it starts any container. The flag `--allow-docker` lifts this only when the watch entry also sets `allow_docker = true`.
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

- `ref` has a leading `-`, contains `..`, or uses characters outside `[A-Za-z0-9._/-]`: the runner exits 2 before it reads Bitwarden.
- `project` is not `.` or a top-level folder name, or is a symlink or leaves the clone: exit 2.
- `BWS_ACCESS_TOKEN` is unset or empty: exit 2, fixed text, no `bws` call.
- Bitwarden holds zero or two secrets with one of the four keys: exit 2, names the key and the count, never a value. A value that is empty, multi-line, shorter than 4 characters or padded with whitespace: exit 2.
- Bitwarden is down or slow: each call has a time limit, one retry, then exit 2.
- The Loki URL is not https (except loopback), carries a user name, or has a path other than the push path: refused or kept as given, never doubled.
- The docker socket is missing or its group is not added: exit 2 with `cannot reach the container engine`, before the clone.
- The work directory is missing, relative, or contains `:`: exit 2.
- The clone times out or the ref does not exist: exit 2 with `clone failed`; git's own text goes to Loki, redacted.
- Two runs of the same repo, ref and project start together: they queue.
- The step runs longer than the run deadline of 30 minutes: the runner ends it and exits 1. The job limit is longer than the runner deadline.
- Loki fails after the probe passed: the run goes on, the exit code is 3 if it would have been 0.
- A step prints more than 10 MB: the log is cut and Loki gets the cut notice.
- The runner image has no digest or a wrong one: the pull fails, the job fails.

## Requirements *(mandatory)*

### Functional Requirements

**Workflow**

- **FR-001**: `factory-monorepo-ci.yml` MUST start only by `workflow_dispatch` with inputs `repo` (default `sudtanj/factory-monorepo`), `ref` (default `main`) and `project` (default `.`), run only on `main`, in the Environment `factory-ci`, with job permissions `contents: read` and `packages: read` and an empty top-level `permissions`. The job limit is 35 minutes, longer than the runner deadline of 30. Concurrency is per repo, ref and project.
- **FR-002**: The workflow MUST contain no Bitwarden call, no `jq`, no `curl`, no `git clone`, no output filter and no secret other than `secrets.BWS_ACCESS_TOKEN` and `secrets.GITHUB_TOKEN`. Its only steps are the pinned checkout (for the watch list) and one step that logs in to ghcr.io over stdin, creates the work directory and runs one `docker run`.
- **FR-003**: The `docker run` MUST use the image in the env line `RUNNER_IMAGE` (`ghcr.io/sudtanj/rust-ci-runner:<version>@sha256:<digest>`), mount the host socket, mount the work directory `$RUNNER_TEMP/ci` at the identical path, mount the watch list read-only, and run as the runner user with the socket's group added, `--cap-drop ALL`, `--security-opt no-new-privileges`, `--read-only` and a tmpfs for `/tmp`. It passes `BWS_ACCESS_TOKEN`, `GITHUB_RUN_ID` and `FACTORY_CI_BWS_SERVER_URL` by name only (`-e NAME`), never by value in argv.
- **FR-004**: Every input and context value MUST reach the script through `env:`. No `${{ }}` sits inside a `run:` block.
- **FR-005**: The workflow MUST NOT post commit statuses or write a job summary.

**CLI**

- **FR-006**: The runner MUST gain `rust-ci-runner ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]`. It reads the watch list, requires an entry for `--repo` whose `branches` match `--ref`, and exits 2 otherwise, before it reads Bitwarden.
- **FR-007**: The runner MUST validate `--ref` (`[A-Za-z0-9._/-]`, 1 to 200 characters, no leading `-`, no `..`), `--project` (`.` or a top-level folder name that passes `folder_name_ok`) and `--work` (absolute, no `:`, an existing directory) before any other work.
- **FR-008**: Exit codes MUST be: 0 the pipeline passed and every log line was delivered; 1 the pipeline failed; 2 a setup fault or refusal; 3 the pipeline passed and a log line was dropped.

**Secrets**

- **FR-009**: The runner MUST read `BWS_ACCESS_TOKEN` and run `bws secret list -o json` as a child process with a 60 s limit and one retry (none after a rejected token), the token only in the child's environment, never in argv. `FACTORY_CI_BWS_SERVER_URL`, when set and not empty, must start with `https://` and goes to the child as `BWS_SERVER_URL`.
- **FR-010**: The runner MUST require exactly one secret for each of `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, and refuse a value that is empty, has CR, LF or NUL, has leading or trailing whitespace, or is shorter than 4 characters. Errors name keys and counts only.
- **FR-011**: The fetched values MUST stay in memory. They MUST NOT be set in the runner's own environment, passed to step containers, or printed. The redactor MUST cover all four values, the access token, and their base64 forms.
- **FR-012**: Every engine child process MUST be started without `BWS_ACCESS_TOKEN` in its environment.

**Clone**

- **FR-013**: The runner MUST clone `https://github.com/<repo>.git` at `--ref` with depth 1, no tags and no submodules, into a new directory under `--work`, with `git` as a child process. The host is fixed; no environment variable changes it. The clone token travels in git's environment as an `http.https://github.com/.extraHeader` entry, never in argv or `.git/config`. The clone has a 300 s limit and `GIT_ALLOW_PROTOCOL=https`.
- **FR-014**: The project directory MUST be a real directory inside the clone, not a link. `.factory-ci.toml` MUST be a regular file, not a link.

**Loki**

- **FR-015**: `LogConfig` loading MUST strip trailing `/` and then a trailing `/loki/api/v1/push` (and a `/` after it) from the URL, for every command that loads it. Normalising twice gives the same result.
- **FR-016**: Before the first container, the runner MUST post one probe line to the store (15 s limit, https, basic auth, labels `repo`, `project=probe`, `step=probe`, `stream=stdout`), print `[runner] log store answered HTTP <code>` (`000` when nothing answered), and exit 2 on a non-2xx code with the hint: `check FACTORY_CI_LOG_URL (host only, e.g. https://logs-prod-xxx.grafana.net), FACTORY_CI_LOG_USER (numeric user id) and FACTORY_CI_LOG_TOKEN (needs logs:write)`.
- **FR-017**: At the end, the runner MUST wait for the shipper to drain and, if a line was dropped, print `[runner] log lines were dropped` and return exit 3 (or 1 when the pipeline failed). `run --quiet-logs` MUST apply the same rule.

**Minimal output**

- **FR-018**: In `ci` mode stdout MUST carry only these lines: `[<name>] started`, `[<name>] success in <N>s`, `[<name>] failed in <N>s`, `[<name>] skipped`, `[runner] pipeline passed`, `[runner] pipeline failed`, `[runner] log store answered HTTP <code>`, `[runner] log lines were dropped`. `<name>` is a step name (`[A-Za-z0-9_-]`, 1 to 64 characters) or `runner`. The runner MUST drop the reason after `failed in Ns` and after `skipped` from the printed line. Loki gets the full text.
- **FR-019**: In `ci` mode stderr MUST carry only fixed fault lines `[runner] <text>` from a closed list in `plan.md`. No error text from git, bws, docker, TOML parsing or a panic reaches stdout or stderr. The detail goes to Loki once the store is proven, redacted.
- **FR-020**: Step output MUST go to Loki only.

**Containment**

- **FR-021**: The runner MUST refuse a step with `docker = true` before it starts any container, unless `--allow-docker` is given and the watch entry has `allow_docker = true`. With `--allow-docker`, `FACTORY_CI_DIND_IMAGE` must be a digest-pinned reference, as in `job`.
- **FR-022**: The runner MUST look up no step secret. A step that declares `secrets` fails the run before any container.
- **FR-023**: Before the clone, the runner MUST check the engine answers (`docker version`) and exit 2 with `cannot reach the container engine` if not.
- **FR-024**: Step containers MUST NOT get the docker socket, and the bind-mount source for `/workspace` MUST be the same string as the path the runner sees. Workspace paths are never rewritten.

**Image and release**

- **FR-025**: The image MUST be `linux/amd64`, built from a base pinned by digest, hold `/rust-ci-runner`, a docker CLI, `git`, CA certificates and `bws` 2.1.0, and run as a non-root user. The `bws` zip MUST be checked against `f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6` and list exactly `bws`, or the build fails.
- **FR-026**: `rust-ci-runner-release.yml` MUST build and push that image, keep both binary release assets, keep the `RUNNER_IMAGE_DIGEST` summary line, and publish version 0.2.0 (`Cargo.toml` and the tag agree). The lint `factory-ci-lint-workflow.sh` and its self-test rows MUST match the new platform string.
- **FR-027**: Only `rust-ci-runner-release.yml` MUST compile the runner. `rust-ci-runner/.factory-ci.toml` MUST run all `--lib` tests, one extra process for the new modules, through the released image.

**Tests and docs**

- **FR-028**: Each behaviour above MUST have a Rust unit test under `cargo test --lib` that runs in the `rust-ci-runner` pipeline. A shell test of the workflow file MUST run in the `factory-ci-selftest` workflow.
- **FR-029**: `README.md` and `rust-ci-runner/README.md` MUST describe the new CLI, workflow inputs, image contents, bootstrap order and the minimal-output rule, and the GitHub repository description MUST be checked against them. This is the last task.

### Key Entities

- **Watch entry**: `repo`, `branches`, `allow_docker` from `.github/factory-ci-repos.toml`. `id`, `secret_branches` and `secrets` are ignored by `ci`.
- **Run**: one `ci` invocation. Inputs `repo`, `ref`, `project`, `work`. Result is an exit code in 0 to 3.
- **Bootstrap values**: `BWS_ACCESS_TOKEN` (GitHub) and the four Bitwarden keys.
- **Public line**: a stdout or stderr line from the closed grammar of FR-018 and FR-019.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: One manual start runs a pipeline of a watched repo and ends in under 35 minutes with the runner's exit code, and the `run:` block of the workflow has no more than 15 lines.
- **SC-002**: Across stdout and stderr of a run and a fixture with hostile content, zero lines fall outside the closed grammar, and zero of the five bootstrap values appear anywhere.
- **SC-003**: A start from a branch other than `main` runs no job. A repo or ref outside the watch list exits 2 before any Bitwarden call.
- **SC-004**: With a wrong Loki user, the run exits 2 within 20 seconds of the probe starting and starts no step. With a store that fails after the probe, a passing pipeline exits 3.
- **SC-005**: `cargo test --lib` passes in the `rust-ci-runner` pipeline with the new tests, and the word `cargo` is absent from `factory-monorepo-ci.yml`.
- **SC-006**: The release publishes a digest, and the first run with that digest passes both `sudtanj/infrastructure` project `rust-ci-runner` and `sudtanj/factory-monorepo`.

## Assumptions

- Supersedes 005 (FR-001 to FR-012 where they conflict), 006 FR-003 to FR-008 and 007 FR-002 to FR-009 for `factory-monorepo-ci.yml`. The `pipeline` input is replaced by `repo` and `project`. `specs/007-runner-image/` FR-012 (native arm64 build) stays.
- The Environment `factory-ci` exists, allows only `main`, and holds `BWS_ACCESS_TOKEN`. The variable `FACTORY_CI_BWS_SERVER_URL` is optional and should stay unset.
- The machine account is limited to the Bitwarden project `rust-ci-runner`, which holds exactly the four keys.
- `FACTORY_CI_TOKEN` can read `sudtanj/factory-monorepo` and can clone the public `sudtanj/infrastructure`.
- The hosted `ubuntu-24.04` runner has a docker daemon, `/var/run/docker.sock` owned by a group that the workflow adds to the container with `--group-add`, and `stat`.
- `factory-ci-selftest.yml` still compiles the runner on pull requests. The owner decides whether that stays (open question in `plan.md`); this feature leaves it alone.
- Adding `sudtanj/factory-monorepo` and `sudtanj/infrastructure` to the watch list also makes the by-hand tick plan them. The tick is not in use today.
- The first release of the new image is `rust-ci-runner-v0.2.0`. Release `0.1.0` stays in ghcr.io, so a revert of the workflow works.
- Out of scope: pipeline secrets for steps, commit statuses, arm64 image, caching, `--locked` (no `Cargo.lock` on `main`), and any change to the tick workflow.
