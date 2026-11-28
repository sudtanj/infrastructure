# Research: Public Docker images from `docker/`

Evidence is `file:line` on branch `012-docker-images`. Each decision answers one open point of the brief or one finding of the devops and security reviews. Lead decisions of 2026-11-28 are marked.

## D1. How the token reaches the publish step

**Finding.** The owner decision says "the runner reads the token from Bitwarden itself". The tick cannot let it, and the reason is a script, not the workflow YAML.

- `.github/workflows/factory-ci-tick.yml:290-299` gives the `job` step of `run-docker` the docker store access token (as `BWS_ACCESS_TOKEN`), `FACTORY_CI_BWS_PROJECT` and `FACTORY_CI_BWS_SERVER_URL`. The YAML passes what a Bitwarden read needs.
- `.github/scripts/factory-ci-job.sh:115` then unsets all three, and line 117 `exec`s the runner. The runner holds neither the store token nor the project id. `.github/scripts/` is frozen (spec FR-022).
- `rust-ci-runner/src/engine.rs:180-182` also strips the store token from every engine child.

**What the frozen script offers.** Lines 46-66 run the runner in prepare mode (`--names-file`); lines 68-101 fetch every name that file lists from the docker Bitwarden project and `export` it into the real run's environment. Feature 010 emptied it: `job.rs:599-624` always writes an empty names file, `plan.rs:310` sets `secrets_ok: false` on every row, and `job.sh:69` refuses a non-empty list when `secrets_ok` is not `true`. All three are Rust, so they change without touching a script or YAML.

**Decision (lead, 2026-11-28).** Use the names-file route, narrowed to one case: prepare lists `DOCKERHUB_TOKEN` only for a `docker/<image>` row on `main` whose check passes; the plan sets `secrets_ok` only for those rows; the script fetches `sudtanj/infrastructure/DOCKERHUB_TOKEN` (`factory-ci-secrets.sh:71`, the entry has `id`), masks it (`fci_mask`) and exports it to the runner; the runner passes it on standard input to its push container. `Engine::command` (`engine.rs:173-186`) clears the environment of every child, so no step container or sidecar inherits it. `NO_JOB_SECRETS` (`job.rs:61`) still refuses a pipeline that lists a secret. The spec records this as a dated clarification and assumption A11; it keeps the owner's intent.

**Rejected.** The runner calls `bws` itself (no token in its environment); a new env or script line (banned); a GitHub secret (Constitution II); the manual `ci` command (it refuses docker steps, `ci.rs:127`).

## D2. Build path and push tool

**Build.** A `docker = true` step runs `docker build --platform linux/<arch>` and `docker save -o image-<arch>.tar`, once per architecture, inside the per-run privileged sidecar (`engine.rs:430-485`). The step container mounts the project directory at `/workspace` (`engine.rs:388-396`), so the archives land in the checkout on the host. `attempt` passes the project directory as the workspace (`job.rs:546`).

**Push tool: a digest-pinned `crane` container started by the runner.** No new crate, about 150 lines of Rust.

| Option | Cost | Verdict |
|---|---|---|
| `crane` image, pinned by digest | One const, two `docker run` | Chosen. Handles `docker save` tarballs, gzip, push by digest and `index append`. |
| Rust registry client on `ureq` | Tar reader, token handshake, blob upload, manifest PUT, SHA-256; about 300 lines | Rejected: larger, new attack surface. |
| `docker login` and `docker push` on the host engine | Token in `~/.docker/config.json` on the VM | Rejected: breaks FR-015. |
| Push from inside the sidecar | Token next to a privileged container | Rejected. |

**Secrets-plus-docker ban.** `Pipeline::docker_secret_problems` (`pipeline.rs:289-295`) checks a user pipeline. The push container is not a pipeline step, has no docker socket, no `--privileged`, no `DOCKER_HOST`. The ban stays true. FR-015's "no step container" is read as "no user-written step".

**Token on standard input (lead).** `Engine::command` sets `stdin(Stdio::null())` (`engine.rs:173-186`), so a new `capture_in` feeds bytes to a child. The container runs `docker run -i` and `crane auth login --password-stdin` reads them. The token is in no argv, no environment of the client or the container, no mount.

**Teardown (lead).** `Engine::cleanup_run` (`engine.rs:240-266`) removes the containers (`rm -f -v`), networks and volumes carrying the run label, which includes the dind sidecar and its data volume. The job calls it before the push, then `run_left` must be false. The push containers use the default bridge, because the run network is gone.

**Fresh directory (lead).** The runner moves the two archives into a new 0700 directory under `RUNNER_TEMP` and mounts it `:ro`. `rename` needs write access to the project directory, which the runner owns, and does not need to read root-owned files. `--user` is the directory owner's uid and gid, because `job.sh:7` sets `umask 077`. The pipeline ends with `chmod 0644` so the owner of the file does not matter.

**Archive check (lead).** Before any push the runner reads the config of each archive (`archive.rs`): user numeric and not 0, no shell entrypoint, `os` and `architecture` match the file name. This closes the gap between "the Dockerfile passed the check" and "the file pushed is what that Dockerfile builds", as far as a static read can.

**Crane image.** `gcr.io/go-containerregistry/crane:debug` (needs `sh`), pinned `@sha256:<64 hex>` in a Rust const; the engineer resolves the digest (T001). It is pulled from gcr.io, so no Docker Hub limit applies.

## D3. Enabling docker for `sudtanj/infrastructure` (lead decision 1)

**Finding.** `allow_docker = true` on the entry sets `Item.allow_docker` on every row (`plan.rs:307`), and `finish` partitions on it (`plan.rs:490`), so `rust-ci-runner` and `ci-canary` would move to the privileged docker job. A second `[[watch]]` table for the same repo is refused (`watch.rs:179-184`).

**Decision.** Keep `branches = ["main"]`. Add `allow_docker = true` to the entry, and in `plan.rs` set row `allow_docker` to `entry.allow_docker && image::publishes(path, branch)`. So only `docker/<image>` on `main` goes to `run-docker`; `rust-ci-runner` and `ci-canary` stay in the plain job, and `docker_problems(false)` (`pipeline.rs:263-269`) still refuses a `docker = true` step in them. `Status.docker` for `pending` is already per row (`plan.rs:455`).

**Costs and open points.**

- Reading statuses and posting `unchanged` and `invalid` use the docker token for the whole entry (`plan.rs:278`, `candidates` picks the API per entry), while plain rows post `pending` and the final status with the plain token. `walk::own` accepts only statuses by the user it read (`walk.rs:67`). With two different GitHub users every plain row would replan each tick. The owner confirms one user (T025) and the canary watches two ticks (T026). A fix in Rust (accept both users) is possible and rejected for now as scope.
- If `FACTORY_CI_TOKEN_DOCKER` is unset, `plan_a` drops every `allow_docker` entry (`main.rs:100-120`): the repo stops being planned. T025 confirms the secret.
- Other branches are not planned at all. FR-006 becomes "the static check as a test".

**Branch validation of `docker/*`.** `tests/repo_files.rs` runs `image::check` over every folder under `docker/` and the selftest runs `cargo test --test '*'` (`factory-ci-selftest.yml:77-79`). Its trigger paths are `rust-ci-runner/**`, `.github/workflows/factory-ci*.yml`, `.github/workflows/factory-monorepo-ci.yml`, `.github/factory-ci-repos.toml` and `.github/scripts/**`: `docker/**` is not listed. A push that changes only `docker/` does not start it, and no YAML edit is allowed. Known limit, recorded in the spec (FR-006), plan (K4) and README. The check on `main` still blocks a publish.

**Widening `branches` later.** It needs the marker analysis kept here: a branch run posts `success`; a later fast-forward of `main` to that sha finds a status at the tip and the walk calls it `Done` (`walk.rs:89-107`, `at_tip`), so nothing publishes. A `walk.rs` flag `publishes` would treat a marked status as not done at the tip (`at_tip`) and on an ancestor (`walk_one`, `walk.rs:205`). Not built now.

## D4. `latest` ordering

**Order.** In `publish`: (1) read the tip of `main` with `Api::branches` (`github.rs:84`); unreadable means push nothing and `error` (FR-018); (2) tear down, move archives, run container A; (3) when tip 1 equals the commit, read the tip again; only when it still equals the commit, container B runs `crane tag <repo>:sha-<12> latest`, which copies the list by digest.

**Why this satisfies FR-018 and R4.**

- Ticks never overlap: `factory-ci-tick.yml:9-11` sets `concurrency: factory-ci-tick` with `cancel-in-progress: false`.
- Within one tick a repo has one row per (branch sha, project), and with `branches = ["main"]` one `main` row per image.
- The residual window is the gap between the second tip read and `crane tag` (a container start). A newer commit lands in it only if a second tick starts and finishes inside it, which the concurrency group forbids. If the tip moves, the newer commit's own tick moves `latest` 5 minutes later.
- A failed second read ends in `error` with the `sha-` tag in place.

**Reuse and K3.** When the `sha-<12>` list exists, container A compares its child digests with `crane digest --tarball` of the two archives (lead decision 4). Equal: continue. Unequal: exit 45. A rebuild of the same commit is not byte-identical, so a retry after a written list fails unless the build is reproducible. The next commit publishes under its own tag. Spec FR-020 says so.

## D5. The distroless check

**Where.** New module `rust-ci-runner/src/image.rs`, called at three points in `job.rs`: prepare mode (decides whether the token name is listed), `attempt` before `prepare()` (fails the project with `failure`), and `publish` (FR-008 second run, on the files as the build left them). Plan time in the tick only sees the tree through the API: `Api` has no file-contents call (`github.rs:79-107`).

**Rules (lead decision 3).** Full table in `data-model.md`: instruction allowlist (`FROM ARG ENV WORKDIR COPY RUN USER ENTRYPOINT CMD LABEL EXPOSE`; `ADD`, `ONBUILD` and the rest refused); heredoc bodies skipped only for `RUN` and `COPY`, `<<` refused elsewhere; `COPY --from=` and `RUN --mount=...from=` only an earlier stage name or index; directives matched case-insensitively with optional spaces; every `FROM` pinned or `scratch` or a stage; no `ARG` in `FROM`; `RUN` only in a `--platform=$BUILDPLATFORM` stage; final stage `scratch` or `gcr.io/distroless/*` without `debug`, numeric non-zero `USER`, exec-form `CMD` and `ENTRYPOINT`; credential names and token shapes; secret flags and mounts; pipeline `env`, `secrets`, commands and pinned step `image`. The check is review help, not a barrier (R12).

## D6. Bitwarden key names

One key: `DOCKERHUB_TOKEN`. `valid_secret_name` (`pipeline.rs:49-91`) accepts it: letters and `_`, and no reserved prefix matches (`DOCKER_` has an underscore after `DOCKER`). The script's own regex is `^[A-Za-z_][A-Za-z0-9_]*$` (`job.sh:63`). The stored key is `sudtanj/infrastructure/DOCKERHUB_TOKEN` in the project `FACTORY_CI_BWS_PROJECT_DOCKER` names (`FCI_KEY_SEP` default `/`, `factory-ci-secrets.sh:11`). The project holds only this key and is reached only by the machine account behind the docker store token (lead decision 7).

**No username key.** A username in Bitwarden would be masked by the script (`fci_mask`) and would blank `sudtanj` from every public log. The namespace is the constant `sudtanj` and the login name is the same constant. If the Docker Hub login differs from the namespace, login fails with `registry refused the token`.

## D7. Runner release and digest

- Bump `rust-ci-runner/Cargo.toml` and the `rust-ci-runner` entry of `Cargo.lock` from `0.3.0` to `0.4.0`. `tests/repo_files.rs:cargo_toml_and_lock_versions_match` checks they agree.
- Before the tag, record `gh variable get RUNNER_IMAGE_DIGEST` (rollback value).
- After the PR merges, tag `rust-ci-runner-v0.4.0` on a commit on `main`. `rust-ci-runner-release.yml` verifies tag equals Cargo.toml and the ancestry, builds, smoke-tests, pushes by digest and prints the digest. Record it.
- The tick reads the digest from the repository variable (`factory-ci-tick.yml:37,57,133,151,231,249`). A new release reaches the tick with `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>`; no YAML edit. The variable is also the rollback switch.
- Canary: two ticks after the variable moves, `factory-ci/rust-ci-runner` and `factory-ci/ci-canary` still post, once each.
- `factory-monorepo-ci.yml` hardcodes `RUNNER_IMAGE` and keeps its pin (A7). `rust-ci-runner/Dockerfile` is not under the ban and does not change.

## D8. Discovery of `docker/<image>` projects (spec A2, answered)

`projects::find` (`projects.rs:42-63`) lists only top-level folders. `docker/hello-static/.factory-ci.toml` is depth 2 and is never found. Changes: `find` also matches `docker/<name>/` entries that hold `.factory-ci.toml` or `Dockerfile` (path `docker/<name>`, `valid` is the FR-002 name rule); `discover` reads one more level under `docker` when the tree is truncated (`projects.rs:72-85`); `target()` (`job.rs:284`) and the status fallback accept `docker/<name>` through `project_path_ok`; `ci.rs:127` keeps `folder_name_ok`, so the manual path never builds image projects; `context` gives `factory-ci/docker/<name>` and `owns` already works on a path prefix (`projects.rs:90-97`).

## D9. Retry and statuses

An `error` older than 30 minutes and not marked `(retry)` runs once more (`walk.rs:104`, `OLD_ERROR` at line 9). Publish errors use `error`; check and build failures use `failure` and do not retry.

## D10. Multi-arch without YAML change (owner change 2026-11-28)

**Requirement.** Each tag is one manifest list holding a native `linux/amd64` and a native `linux/arm64` image (FR-026); the tag never points at a single-platform image.

**arm64 is cross-compiled, never emulated.**

| Option | What it needs | Verdict |
|---|---|---|
| Cross-compile in a `FROM --platform=$BUILDPLATFORM` stage; the final stage only `COPY`s | Nothing beyond `docker build --platform linux/arm64`. Every `RUN` runs on the VM's native amd64 | Chosen |
| QEMU binfmt registered from the privileged sidecar | A pinned third-party privileged image; a runner change to register before the build; builds 5 to 20 times slower under the 45-minute cap | Rejected for now |

**Does binfmt from inside dind touch the host kernel?** Yes. `binfmt_misc` registration is a kernel-global table (in kernel 6.6 it is not per container; user-namespace mounts arrived in 6.7). The sidecar is `--privileged` (`engine.rs:458`), so the write would succeed and register the interpreter in the ephemeral VM's kernel, which dies with the VM. GitHub's own `setup-qemu-action` does the same on hosted runners, so the runner does not forbid it. Not verified here. No emulation in this feature.

**The limit.** A Dockerfile whose build must execute target-architecture code (a `RUN` in a stage that is not `FROM --platform=$BUILDPLATFORM`, or an ecosystem that cannot cross-compile) cannot publish: the check refuses it with `emulation` (FR-027).

**Publish.** Platform images by digest, one index write, then tags; no tag exists until the list does.

1. Container A computes `crane digest --tarball` per archive, pushes each image to `<repo>@<digest>` (no tag, so FR-017's "no other tag" holds), then `crane index append -t <repo>:sha-<12> -m ... -m ...` writes the list in one manifest PUT, filling each entry's platform from the image config. The archive config check (D2) already proved os and architecture.
2. The script prints the manifest it wrote as one line. `publish::verify_index` accepts exactly one `linux/amd64` and one `linux/arm64` entry under either the OCI index or the Docker manifest list media type.
3. `latest` is `crane tag <repo>:sha-<12> latest`.

**Atomicity.** The `sha-` tag is written once, by the index PUT, and the index names two images. A failed run leaves untagged platform manifests (R11), never a tagged single-arch image.

**Supply chain (lead decision 5).** The Go builder is `mirror.gcr.io/library/golang:<supported>@sha256:<index digest>` with `GOTOOLCHAIN=local`, so the sidecar's pull avoids the anonymous Docker Hub limit and Go downloads no toolchain. The Dockerfile passes the check (`from-unpinned` accepts the digest). `docker/README.md` names the digest refresh routine.

## Spike left to the engineer (T001)

Not verifiable on this VM. Use a throwaway Docker Hub repository the owner creates.

1. `docker save` output from `docker:27.5.1-cli` is a tarball `crane push` and `crane digest --tarball` accept. If not, switch the build to `docker buildx output type=oci` with the same file names.
2. `crane push <tar> <repo>@sha256:<digest>` is accepted by Docker Hub. If refused, the fallback is a temporary tag per platform image, which breaks FR-017 and needs a spec decision: report to the lead, do not improvise (plan K1).
3. `crane:debug` carries `sh`, and `crane index append` fills the platform from the config.
4. `mirror.gcr.io/library/golang@sha256:<index digest>` pulls by digest from the sidecar.
