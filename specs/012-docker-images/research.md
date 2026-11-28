# Research: Public Docker images from `docker/`

Evidence is `file:line` on branch `012-docker-images` at `d2a4c90`. Each decision answers one open point of the brief.

## D1. How the token reaches the publish step (BLOCKER for the literal reading)

**Finding.** The owner decision says "the runner reads the token from Bitwarden itself". The existing tick cannot let it, and the reason is in a script, not the workflow YAML.

- `.github/workflows/factory-ci-tick.yml:290-299` gives the `job` step of `run-docker` `BWS_ACCESS_TOKEN` (from `BWS_ACCESS_TOKEN_DOCKER`), `FACTORY_CI_BWS_PROJECT` (from `FACTORY_CI_BWS_PROJECT_DOCKER`) and `FACTORY_CI_BWS_SERVER_URL`. The YAML passes what a Bitwarden read needs.
- `.github/scripts/factory-ci-job.sh:115` then runs `unset BWS_ACCESS_TOKEN ... FACTORY_CI_BWS_PROJECT FACTORY_CI_BWS_SERVER_URL` and `exec`s the runner at line 117. The runner process holds neither the store token nor the project id. It cannot read Bitwarden. `.github/scripts/` is frozen (spec FR-022), so that line stays.
- `rust-ci-runner/src/engine.rs:180-182` also strips `BWS_ACCESS_TOKEN` from every engine child.

**What the frozen wrapper does offer.** Lines 46-66 run the runner in prepare mode (`--names-file`), lines 68-101 fetch every name that file lists from the docker Bitwarden project and `export` it into the real run's environment, then `exec`. That is the same mechanism feature 002 built for step secrets. Feature 010 emptied it: `rust-ci-runner/src/job.rs:599-624` always writes an empty names file, `rust-ci-runner/src/plan.rs:310` sets `secrets_ok: false` on every row, and `job.sh:69` refuses a non-empty list when `secrets_ok` is not `true`. All three are Rust or data, so they can change without touching a script or YAML.

**Decision.** Use the names-file route, narrowed to one case:

- Prepare mode writes the single name `DOCKERHUB_TOKEN` only when the project is `docker/<image>`, the branch is `main`, and the image check passes. Every other case writes the empty file it writes today.
- The plan sets `secrets_ok` to true only on rows with that project shape and that branch.
- `job.sh` fetches the key `sudtanj/infrastructure/DOCKERHUB_TOKEN` (lines 77-86, repo-prefixed key, the entry has `id`) from the docker project, masks it (`fci_mask`, line 99) and exports it to the runner (line 100).
- The runner's built-in publish step reads `DOCKERHUB_TOKEN` from its own environment. `Engine::command` (`engine.rs:173-186`) clears the environment of every child, so no step container inherits it.

**Deviation to report.** The wrapper reads Bitwarden, the runner does not. The token is in the runner process environment for the whole run on `main` image projects. User-written steps still get nothing: `NO_JOB_SECRETS` (`job.rs:61`) still refuses any pipeline that lists a secret, and the publish step is not a pipeline step. Feature 010's rule holds for user steps and is relaxed for one built-in path. The owner must accept this reading. If the owner refuses it, this feature has no no-YAML route and stops.

**Rejected.**

- Runner calls `bws` itself: no token in its environment (above).
- New workflow env line or script line: banned.
- Token as a GitHub secret: Constitution II.
- Run the publish through the manual `ci` command: it never builds `docker/` projects and refuses docker steps (`ci.rs:127`, README line 61).

## D2. Build path and push tool

**Build.** A pipeline step with `docker = true` runs `docker build --platform linux/<arch>` and `docker save -o image-<arch>.tar`, once per architecture, inside the per-run privileged sidecar (`engine.rs:430-485`). The step container mounts the project directory at `/workspace` (`engine.rs:388-396`, `run_step`), so `image-amd64.tar` and `image-arm64.tar` land in the checkout on the host. In `attempt`, `run_pipeline` receives the project directory as `workspace` (`job.rs:546`).

**Push tool: a digest-pinned `crane` container started by the runner.** The runner uses `Engine::run_logged` with the token as a name-only `-e` (the pattern of `engine.rs:524-530`). No new crate, about 120 lines of Rust.

| Option | Cost | Verdict |
|---|---|---|
| `crane` image, pinned by digest | One new const, one `docker run` | Chosen. Handles `docker save` tarballs, gzip, blob mounts and retries. |
| Rust registry client on `ureq` | Tar reader, token handshake, blob upload, manifest PUT, SHA-256 or a new crate; ~300 lines and its own tests | Rejected: larger, and a hand-written registry client is a new attack surface. |
| `docker login` and `docker push` on the host engine | Token in `~/.docker/config.json` on the VM, next to the host socket | Rejected: breaks FR-015. |
| Push from inside the sidecar | Token next to a privileged container | Rejected: the reason the spec exists. |

**Secrets-plus-docker ban.** `Pipeline::docker_secret_problems` (`pipeline.rs:289-295`) checks a user pipeline. The publish container is not a pipeline step, has no docker socket, no `--privileged`, and no DOCKER_HOST. The ban stays true and needs no change. FR-015 says "no step container": this plan reads "step" as a user-written pipeline step and calls the crane container a runner-owned container. The report names that reading.

**Crane image.** `gcr.io/go-containerregistry/crane:debug` (needs `sh`), pinned `@sha256:<64 hex>` in a Rust const. The engineer resolves the digest when implementing and records the source tag in a comment. It is pulled from gcr.io, not Docker Hub, so the base-image pull limit does not apply. The mirror workflow cannot grow a new image (YAML ban).

**Container flags.** `--rm --read-only --cap-drop ALL --security-opt no-new-privileges --pids-limit 256 --memory 1g`, the run network `factory-ci-<run>` (created by `execute`, `engine.rs:572`, removed by `cleanup_run`), `-v <project dir>:/in:ro`, `--tmpfs /tmp:rw,size=64m`, `--user <uid>:<gid>` of the project directory. The last flag is needed because `job.sh:7` sets `umask 077`, so the workspace is mode 0700 and a different uid cannot enter it.

## D3. Enabling docker for `sudtanj/infrastructure`

`allow_docker = true` on the entry sets `Item.allow_docker` on every row of the repo (`plan.rs:307`), and `finish` partitions on it (`plan.rs:490`), so `rust-ci-runner` and `ci-canary` move to `run-docker`. A second `[[watch]]` table for the same repo is refused as a duplicate (`watch.rs:179-184`), so a toml-only split is impossible.

**Narrowest change: one line, `allow_docker = true`.** Its risk:

- `rust-ci-runner` and `ci-canary` now run under Environment `factory-ci-docker` with the `_DOCKER` token and blob key. Neither pipeline has a `docker = true` step (`rust-ci-runner/.factory-ci.toml`, `ci-canary/.factory-ci.toml`), so no sidecar starts for them.
- Anyone who can merge to a watched branch of this repo can add a `docker = true` step and get root on the runner VM. Today that is the owner. README line 259-261 already says so.
- The status token for these rows becomes `FACTORY_CI_TOKEN_DOCKER`. It must be able to write commit statuses on this repo. Not verified here (outside the repo).
- If `FACTORY_CI_TOKEN_DOCKER` is unset, `plan_a` drops every `allow_docker` entry (`main.rs:100-120`, FR-047): this repo would stop being planned at all. The owner confirms the secret exists before the release.

**Alternative rejected.** A new per-project key in the watch file would let only `docker/*` take the docker path. It costs a parser change, a plan change and tests, and the spec names `allow_docker` and `branches` (FR-023).

**Branches.** Drop the `branches` key (default `["*"]`, `watch.rs:199-202`). This is what commit `8660df5` did for `factory-monorepo`. Cost: every branch tip of this repo is planned, so `rust-ci-runner` runs its two long steps on every branch push (45-minute job cap, `max-parallel: 4`, 20 rows per tick, `plan.rs:63-69`). Branch runs never see a secret (`secrets_ok` false off `main`, D1). A narrower pattern such as `["main", "feat/*"]` would break the owner's branch names (`012-docker-images`), so it is rejected.

**Silent never-publish hole this change opens.** A branch run posts `success` on a sha. If the owner then fast-forwards `main` to that sha, the tip already has a status for the context and the walk calls it `Done` (`walk.rs:89-107`, `at_tip`). Nothing publishes. Fix in D4.

## D4. `latest` ordering and the not-published marker

**Order.** In `publish`: (1) read the tip of `main` with `Api::branches` (already used by the plan, `github.rs:84`); unreadable means push nothing and `error` (FR-018); (2) container A: push `sha-<short>` unless it already exists (it reads the existing manifest first, so a retry reuses the same digest, FR-020); (3) when the tip read in (1) equals the commit, read the tip again; only when it still equals the commit, container B runs `crane tag <repo>:sha-<short> latest`. The tag step copies the manifest list by digest, so `latest` and `sha-` always match. A failed second read ends in `error` with the `sha-` tag in place, and the retry finishes `latest`.

**Why this satisfies FR-018 and R4.**

- Ticks never overlap: `factory-ci-tick.yml:9-11` sets `concurrency: factory-ci-tick` with `cancel-in-progress: false`. A later tick, and so a later commit, starts after the earlier job ends.
- Within one tick a repo has one row per (branch sha, project) and the sha groups prefer `main` (`plan.rs:244-252`), so one `main` row per image.
- The residual window is the gap between the tip read and `crane tag` (about 2 seconds). A newer commit lands in it only if a second tick starts and finishes inside it, which the concurrency group forbids. If the tip moves in the window, the newer commit's own tick moves `latest` forward 5 minutes later.
- An unreadable tip on the first read pushes nothing and ends in `error` (FR-018 last sentence).

**Marker.** A run on a branch other than `main` posts `success` with the fixed text `built and checked, not published`. `walk::decide` takes a new flag `publishes` (true when the row's branch is `main`). With the flag set, a status of that text counts as not done:

- at the tip (`at_tip`, `walk.rs:89`): `Run`, not `Done`;
- on an ancestor (`walk_one`, `walk.rs:205`): `Run(prev=anc)`, not `Unchanged`. This covers two quick pushes where the tick sees only the second.

Without the flag the marker is `Done` as usual, so a branch tip is not replanned every tick.

## D5. The distroless check

**Where.** New module `rust-ci-runner/src/image.rs`, called by the runner at three points in `job.rs`: prepare mode (decides whether the token name is listed), `attempt` before `prepare()` (fails the project with `failure`), and the publish step (FR-008 second run, on the files as the build left them). Plan time in the tick (`plan.rs`) only sees the tree through the API: `Api` has no file-contents call (`github.rs:79-107`). Adding one costs a trait method, a fake and a read per docker project per tick. The job's pre-container stage runs after the clone and before any container or secret, so it is the earliest point that has the files.

**Rules.** Full table in `data-model.md`. Summary: layout and name; parsed instructions (continuations, comments, CRLF, heredoc bodies skipped, parser directives); every `FROM` pinned by digest or `scratch` or an earlier stage name; no `ARG` in `FROM`; final stage `scratch` or `gcr.io/distroless/*` without `debug`, resolved through stage chains; no `RUN`, no `SHELL`, no shell-form `CMD`/`ENTRYPOINT`, no shell or package manager copied in; numeric non-zero `USER`; credential names and token shapes in `ARG`/`ENV`; `--secret`, `--ssh`, `--build-arg`, `--mount=type=secret|ssh`; pipeline `env`, `secrets` and commands; `# syntax=` directive refused (it pulls an unpinned frontend).

## D6. Bitwarden key names

One key: `DOCKERHUB_TOKEN`. `valid_secret_name` (`pipeline.rs:49-91`) accepts it: letters and `_`, and no reserved prefix matches (`DOCKER_` has an underscore after `DOCKER`; `DOCKERHUB_` does not start with it). The wrapper's own regex is `^[A-Za-z_][A-Za-z0-9_]*$` (`job.sh:63`). The stored key is `sudtanj/infrastructure/DOCKERHUB_TOKEN` in the project `FACTORY_CI_BWS_PROJECT_DOCKER` points to (`FCI_KEY_SEP` default `/`, `factory-ci-secrets.sh:11`).

**No username key.** The spec proposed `DOCKERHUB_USERNAME`. A username in Bitwarden would be masked by the wrapper (`fci_mask`) and so would blank the word `sudtanj` across every public log. The namespace is the constant `sudtanj` and the login name is the same constant. Risk: if the Docker Hub login differs from the namespace, login fails with "registry refused the token". Recorded as owner action A5.

## D7. Runner release and digest

- Bump `rust-ci-runner/Cargo.toml` and the `rust-ci-runner` entry of `Cargo.lock` from `0.3.0` to `0.4.0`. `tests/repo_files.rs:cargo_toml_and_lock_versions_match` checks they agree.
- Before the tag, record the current value of `RUNNER_IMAGE_DIGEST` (`gh variable get RUNNER_IMAGE_DIGEST`). It is the rollback value.
- After the PR merges, tag `rust-ci-runner-v0.4.0` on a commit on `main`. `rust-ci-runner-release.yml` verifies tag equals Cargo.toml and the ancestry, builds, smoke-tests, pushes by digest and prints the digest in the run summary. Record that digest.
- The tick reads the digest from the repository variable `RUNNER_IMAGE_DIGEST` (`factory-ci-tick.yml:37,57,133,151,231,249`). A new release reaches the tick with `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<64 hex>` and no YAML edit. The variable is also the rollback switch: set it back to the recorded 0.3.0 digest.
- Canary: after the variable moves, the first tick must plan and post for the repo's other projects as before (`factory-ci/rust-ci-runner`, `factory-ci/ci-canary`) before any image push is trusted. The canary is the 0.4.0 runner doing the 0.3.0 work; it is the same check feature 010 T021 made for 0.3.0.
- `factory-monorepo-ci.yml` hardcodes `RUNNER_IMAGE` and keeps its pin (A7): publishing goes through the tick, so the manual workflow does not need 0.4.0.
- `rust-ci-runner/Dockerfile` is not under the ban (only `.github/workflows/*` and `.github/scripts/*` are) and does not change: `crane` runs as its own container, and the tick copies only `/rust-ci-runner` out of the image.

**Not a blocker.** The digest is a variable.

## D8. Discovery of `docker/<image>` projects (spec A2, answered)

`projects::find` (`projects.rs:42-63`) lists only top-level folders. `docker/hello-static/.factory-ci.toml` is depth 2 and is never found. Changes:

- `find`: also match `docker/<name>/` entries that hold `.factory-ci.toml` or `Dockerfile`. Path is `docker/<name>`; `valid` is the FR-002 name rule (kebab, 2 to 64).
- `discover`: when the recursive tree is truncated, read one more level under `docker` (`projects.rs:72-85`).
- `target()` (`job.rs:284`) and the status fallback accept `docker/<name>` through a new `project_path_ok`. `ci.rs:127` keeps `folder_name_ok`, so the manual path never builds image projects.
- `context` gives `factory-ci/docker/<name>`. `owns` already works on a path prefix (`projects.rs:90-97`).
- A folder with a `Dockerfile` and no `.factory-ci.toml` is a project too, so the job can fail it with a named rule (FR-002). A folder with neither is not a project and is ignored.

## D9. Concurrency, retry and statuses

- Retry: an `error` older than 30 minutes and not marked `(retry)` runs once more (`walk.rs:104`, `OLD_ERROR` at line 9). Publish errors use `error`, so they get that retry. Check and build failures use `failure` and do not.
- `Done` vs the stale-pending close: unchanged.

## D10. Multi-arch without YAML change (owner change 2026-11-28)

**Requirement.** Each tag is one manifest list holding a native `linux/amd64` and a native `linux/arm64` image (FR-026). The tag never points at a single-platform image.

**How arm64 is built: cross-compile inside the build, no emulation.**

| Option | What it needs | Verdict |
|---|---|---|
| Cross-compile in a `FROM --platform=$BUILDPLATFORM` stage; the final stage only `COPY`s (no `RUN`, already FR-010) | Nothing beyond `docker build --platform linux/arm64`. Every `RUN` executes on the VM's native amd64. The distroless final stage resolves natively by its index digest | Chosen |
| QEMU binfmt registered from the privileged dind sidecar (`tonistiigi/binfmt --install arm64`) | A third-party privileged image to pin; `docker run --privileged` inside dind; build steps 5 to 20 times slower under emulation against the 45-minute job cap; a runner code change to register before the build | Rejected for now |

**Does binfmt from inside dind touch the host kernel?** Yes. `binfmt_misc` registration is a kernel-global table (in kernel 6.6, the VM's kernel, it is not per container; user-namespace mounts arrived in 6.7). The sidecar is already `--privileged` (`engine.rs:458`), so the write succeeds and registers the interpreter in the ephemeral GitHub VM's kernel. It dies with the VM, so nothing outlives the job. GitHub's own `setup-qemu-action` does the same on hosted runners, so the runner does not forbid it. Not verified here. It is a real cost (a pinned third-party image with root, plus slow builds), and the spec's final stage has no `RUN`, so emulation buys little. No emulation in this feature.

**The limit.** A Dockerfile whose build must execute target-architecture code (a `RUN` in a stage that is not `FROM --platform=$BUILDPLATFORM`, or an ecosystem that cannot cross-compile, such as native Python wheels) cannot publish. The check refuses it with id `emulation` (FR-027), so the owner learns at the check, not from an `exec format error` deep in a build log. A later spec can add QEMU registration in the runner (the sidecar is privileged already) and lift the rule.

**Build.** One pipeline step builds each platform separately with the default docker driver:

```
docker build --platform linux/amd64 -t image-amd64 . && docker save -o image-amd64.tar image-amd64
docker build --platform linux/arm64 -t image-arm64 . && docker save -o image-arm64.tar image-arm64
```

BuildKit sets `BUILDPLATFORM` to the VM's `linux/amd64` and `TARGETARCH` to the requested platform. The same Dockerfile serves both. Bases are pinned by their index digest (FR-028): one `FROM` then resolves to the right platform image, and a single-platform digest makes the other build fail, which publishes nothing.

**Publish: platform images by digest, one index write, then tags.** No tag exists until the list does.

1. For each architecture: `crane digest --tarball image-<arch>.tar` gives the manifest digest, `crane push image-<arch>.tar <repo>@<digest>` pushes it by digest (no tag, so FR-017's "no other tag" holds), and `crane config <repo>@<digest>` must report `os` `linux` and the matching `architecture`. A mismatch stops before any tag (exit 44).
2. `crane index append -t <repo>:sha-<short> -m <repo>@<amd64 digest> -m <repo>@<arm64 digest>` builds the list and writes it to the `sha-` tag in one manifest PUT. `crane index append` fills each entry's platform from the image config.
3. The script prints the manifest it just wrote as one line, and the runner checks it with a pure function (`publish::verify_index`): index media type, exactly one `linux/amd64` and one `linux/arm64` entry, each digest `sha256:` plus 64 hex, nothing else. That function is the unit-tested manifest-list check (SC-006).
4. `latest` is `crane tag <repo>:sha-<short> latest`, which copies the list by digest.

**Atomicity.** The `sha-` tag is written once, by the index PUT, and the index names only two verified images. A failed run leaves untagged platform manifests (R11), never a tagged single-arch image. A repeat run finds the `sha-` tag, prints its manifest, and the runner verifies it again (FR-020).

## Spike left to the engineer

Not verifiable on this VM. Run the real publish once with a test repo before trusting the contract.

1. `docker save` output from `docker:27.5.1-cli` is a tarball `crane push` and `crane digest --tarball` accept. If not, switch the build to `docker buildx build --output type=oci,dest=image-<arch>.tar` (a `docker-container` driver, buildkit image pinned) with the same file names.
2. `crane push <tar> <repo>@sha256:<digest>` is accepted by Docker Hub. If it is refused, the fallback is a temporary tag per platform image (`tmp-<run>-<arch>`), which breaks "no other tag" (FR-017) and needs a spec decision: report to the lead, do not improvise.
3. `crane:debug` carries `sh`, and `crane index append` fills the platform from the config.
