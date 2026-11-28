# Feature Specification: Public Docker images from `docker/`

**Feature Branch**: `012-docker-images`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "Lets also add folder docker on infrastructure repo to host a folder for deploying public docker image compilation to my docker hub, lets create the speckit when compiling docker images its suppose tk use rust ci runner and its suppose tobe distroless and secure by default, no hardcoded secrets allowed, its should automatically trigger rust ci on push, you are strictly forbidden to change the github action yaml since from now on only the toml we will use"

**Supersedes**: the unmerged spec on branch `origin/004-dockerhub-images` (`specs/004-dockerhub-images/spec.md`). This spec replaces it. Its acceptance scenarios were reused where they fit (one folder per image, a failing build publishes nothing, a non-main branch publishes nothing). Branch 004 should be closed unmerged.

## Who this is for

The owner, who wants to publish small public images to Docker Hub under `sudtanj/` by adding a folder to `sudtanj/infrastructure`, and who wants an unsafe image to be impossible to publish by mistake.

## Problem

There is no place in the repo to build and publish an image, and the runner cannot do it today. Evidence:

- `rust-ci-runner/src/job.rs` refuses any tick pipeline that lists a secret (`NO_JOB_SECRETS`), and `rust-ci-runner/src/pipeline.rs` bans secrets and `docker = true` in one pipeline. A pipeline step cannot log in to a registry and push. This was intended: feature 010 keeps the tick from handing any secret to a step.
- Docker steps run against a privileged `docker:27.5.1-dind` sidecar per run (`rust-ci-runner/src/engine.rs`). Anything that holds a registry token inside such a step puts it next to a privileged container.
- Constitution Principle II keeps pipeline secrets in Bitwarden, and `README.md` of the runner says "Pipeline secrets never go into GitHub". A token in a workflow file or a GitHub secret is not an option.
- The owner has ruled that no GitHub Actions YAML changes from now on (decision 2 below). Every earlier route that added a workflow is closed.
- `.github/factory-ci-repos.toml` lists `sudtanj/infrastructure` with `branches = ["main"]` and no `allow_docker`, so today no docker step and no branch other than `main` runs for this repo.

## Announcement

Add a folder `docker/<image>/` with a `Dockerfile`, a `README.md` and a `.factory-ci.toml`, merge it to `main`, and within one tick (about 5 minutes) plus the build time the runner builds the image, checks it, and pushes `sudtanj/<image>:latest` and `sudtanj/<image>:sha-<first 12 hex of the commit>` to Docker Hub, each one a manifest list with a native `linux/amd64` and a native `linux/arm64` image. The check fails any image that is not distroless or `scratch`, runs as root, has a shell, uses an unpinned base or holds a credential, and a failed check publishes nothing. The Docker Hub token never reaches a file in the repo, a step you write, or a branch other than `main`: the existing job script fetches it from Bitwarden for a passing `docker/*` row on `main`, and only the runner and its push container receive it. Branches other than `main` are not built by the tick; they get the static check only, as a Rust test. The first image, `sudtanj/hello-static`, proves the path.

## Clarifications

### Session 2026-11-28 (owner decisions)

- Q: How does the Docker Hub token reach the automatic path without breaking feature 010's rule that the tick hands no secret to a step? → A: A new built-in publish step of `rust-ci-runner`. The runner reads the token from Bitwarden itself and pushes the built image. No step written in `docker/` ever receives the token, so 010 still holds. (Superseded on the route by the dated token-route line below: the job script fetches it, not the runner; the intent holds.)
- Q: What may change in the repo? → A: No file under `.github/workflows/` and no file under `.github/scripts/`. Only Rust code in `rust-ci-runner/`, `.factory-ci.toml` files and other toml files (such as `.github/factory-ci-repos.toml`) change to carry the automation. See assumption A1 for what this means for docs and `docker/` content.
- Q: Which Docker Hub namespace? → A: `sudtanj`. Images are public as `sudtanj/<folder-name>`.
- Q (2026-11-28, owner change): Which platforms? → A: Every image supports both `linux/amd64` and `linux/arm64`, published as one multi-arch manifest list under each tag (`latest` and `sha-<12 hex>`). arm64 is no longer out of scope. The build uses no emulation: the Dockerfile cross-compiles on the build platform.
- Q (2026-11-28, lead decision, token route): How does the token reach the publish step, given that the job script unsets the store token before it starts the runner? → A: The frozen job script fetches it through its names-file mechanism, only for a passing `docker/<image>` row on `main`. The runner lists `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` in prepare mode, the script exports them to the runner, and the runner passes the token on standard input to its push container. Only the runner and the push container receive it; no step written in `docker/` does. This replaces "the runner reads the token from Bitwarden itself" above and keeps its intent (assumption A11).
- Q (2026-11-28, lead decision, branches): Do branches other than `main` build? → A: No. `branches = ["main"]` stays for `sudtanj/infrastructure`, and the plan gives a row `allow_docker` only for `docker/<image>` on `main`, so `rust-ci-runner` and `ci-canary` stay in the plain job. Other branches get the static image check only, as a Rust test the selftest runs (FR-006, FR-023). "Other branches build without pushing" no longer holds.
- Q (2026-11-28, lead decision, hardening): What changes in the check and the publish? → A: An instruction allowlist and stricter parsing (FR-029), an archive config check before the push (FR-030), teardown of the sidecar before any container or request uses the token and the token on standard input (FR-015), tags `sha-<first 12 hex>` (FR-017), reuse of a valid existing `sha-` list with no digest comparison (FR-020, replaced after review), a Go builder from `mirror.gcr.io` (A13), and a public repository the runner creates when it is missing (FR-016, A5; the owner-created rule of this line was replaced by the dated line above).
- Q (2026-11-28, owner change, keys): Which Bitwarden keys does the publish use? → A: The two the owner already stores in the project `FACTORY_CI_BWS_PROJECT_DOCKER` names, as shared names: `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`. The `sudtanj/infrastructure` watch entry lists both in `secrets`, so the frozen job script's shared route fetches them (the repo-own key `sudtanj/infrastructure/DOCKERHUB_TOKEN` is not used). The earlier "one key, username a constant" decision is withdrawn. The username is not confidential: the runner uses it only for the login and to compare it with the namespace, and never registers it with a redactor (FR-031).
- Q (2026-11-28, owner change, repositories): Who creates `sudtanj/<image>` on Docker Hub? → A: The publish step does, as a public repository, through the Docker Hub API with the same credentials, before the first push. It never changes the visibility of an existing repository. The owner no longer pre-creates it (FR-016).
- Q (2026-11-28, critic review): What changes after review? → A: A failed step or a timeout of a `docker/<image>` pipeline ends in `error`, so the single retry covers a rate limit or network error during the build (FR-007); a retry reuses a valid existing `sha-<12>` list and only tags `latest`, with no digest comparison (FR-020); one shared deadline keeps the item under the 45-minute job limit (FR-033); FR-013 and SC-003 speak of credentials exported to the runner, because the job script loads the docker project into a shell variable whenever the store token is set; FR-015's "never to a sidecar" is about what the runner hands over, and a host escape is R12.
- Q: What does "automatically on push" mean? → A: The existing tick, which an outside scheduler dispatches about every 5 minutes, picks up a push to `main` within that interval. Nothing reacts to the push event itself, and the owner accepts that. Spec 011 (CI state queue) is amended separately to keep the tick; this spec relies on it.

### Defaults chosen by the lead

Recorded as requirements or assumptions below: layout `docker/<image>/`; first image a small placeholder from a static binary; `linux/amd64` and `linux/arm64` in one manifest list (owner change, see Clarifications); the safety rules as a check that fails the image; tags `latest` and `sha-<first 12 hex of the commit>`; only `main` publishes and only `main` is built by the tick; a deleted folder publishes nothing and leaves Docker Hub alone; the only Docker Hub API calls are the login and the creation of a missing public repository (FR-016).

No question was left without a safe default, so `/speckit-clarify` raised none. The dated lines above record the later owner and lead decisions.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A new image folder becomes a public image (Priority: P1)

The owner adds `docker/hello-static/` with the three files, opens a pull request, merges to `main`, and waits. The image shows up on Docker Hub, and anyone can pull it without logging in.

**Why this priority**: It is the reason for the feature.

**Independent Test**: Merge the first image. After the next tick and build, `docker pull sudtanj/hello-static:latest` works on a machine with no Docker Hub login, on an amd64 host and on an arm64 host, each getting its native image, and `sha-<first 12 hex>` points to the same manifest list digest as `latest`.

**Acceptance Scenarios**:

1. **Given** a valid `docker/hello-static/` lands on `main`, **When** the next tick runs, **Then** the runner builds it, the check passes, and both tags exist on `sudtanj/hello-static`.
2. **Given** the Docker Hub repository `sudtanj/hello-static` does not exist, **When** the first publish runs, **Then** the runner creates it as a public repository through the Docker Hub API with the stored credentials and then pushes. **Given** the repository exists and is public, **Then** no repository is created or edited. **Given** the repository exists and is private, **Then** the item ends in `error` with a fixed line, nothing is pushed and the visibility is not changed (FR-016).
3. **Given** a later commit changes files only outside `docker/hello-static/`, **When** the next tick runs, **Then** nothing is built or pushed for that image and its tags stay where they were.
4. **Given** the image is published, **When** a host with `linux/arm64` and a host with `linux/amd64` each pull `sudtanj/hello-static:latest`, **Then** each gets the image for its own architecture, and the tag is one manifest list with exactly those two platforms.
5. **Given** the arm64 image or the amd64 image fails to build, **When** the run ends, **Then** nothing is pushed and no tag points at a single-arch image.

---

### User Story 2 - An unsafe image cannot be published (Priority: P1)

The owner, an agent or a tired late-night edit writes a Dockerfile with an unpinned base, a root user, a shell, or a token. The check stops it, and the run says which rule failed.

**Why this priority**: "Secure by default" must hold when nobody is paying attention. A convention does not stop a mistake; a failing check does.

**Independent Test**: One fixture Dockerfile per rule, each failing the check, plus the first image passing it.

**Acceptance Scenarios**:

1. **Given** a `FROM` without a digest, **When** the check runs, **Then** the project ends in `failure` and nothing is pushed.
2. **Given** a `.factory-ci.toml` under `docker/` that omits the check step, **When** the run reaches publish, **Then** the publish step still runs the check and refuses.
3. **Given** a pipeline in `docker/` that lists `secrets`, **When** the check runs, **Then** it fails.

---

### User Story 3 - Only `main` publishes, and no secret leaks (Priority: P1)

The tick does not build a branch other than `main` (the watch entry stays `branches = ["main"]`). A pull request branch or a push to another branch gets the static image check only, as a Rust test the selftest runs, so the owner sees a bad Dockerfile before it merges. Nothing is pushed and the token is never fetched.

**Why this priority**: The repo is public and anyone can read the logs (Constitution Principle III). A token read on a branch can be printed by a changed Dockerfile or pipeline.

**Independent Test**: Run `image::check` over every `docker/` folder in the selftest. Run the job on a non-image project and on a `main` image fixture with a counting environment: the non-image run makes zero token reads and zero pushes. Search every public line, step container environment, image layer and argv for a token canary.

**Acceptance Scenarios**:

1. **Given** a branch other than `main` with a `docker/<image>/` folder, **When** the selftest runs, **Then** the static check runs over that folder and fails the run on any rule, and the tick neither builds it nor reads the token. **Given** a push that changes only `docker/`, **Then** the selftest may not start (known limit, FR-006), and the check on `main` still blocks the publish.
2. **Given** a build that prints environment variables, **When** the run ends on `main`, **Then** no token appears in the public log, the layers or any step environment.

---

### User Story 4 - Tags stay correct and removal is safe (Priority: P2)

Two pushes land a minute apart. A folder is deleted.

**Why this priority**: These are the cases where a public `latest` goes stale or a repo changes unexpectedly. Rare, and costly to find later.

**Independent Test**: A fake registry and two publishes for one image with the older commit finishing last. A fixture where the folder is gone.

**Acceptance Scenarios**:

1. **Given** commits A then B on `main`, **When** the job for A finishes after the job for B, **Then** `latest` points to B's image and A's image holds only `sha-<A>`.
2. **Given** `docker/old-image/` is deleted, **When** the next tick runs, **Then** nothing is pushed, nothing is deleted on Docker Hub, and its tags stay.

### Edge Cases

Answers to the edge-case checklist for this change.

- Inputs. Folder names: lowercase kebab-case, 2 to 64 characters (FR-002); `Hello`, `hello_static`, `-a` and `a` fail. A Dockerfile with several stages, `FROM scratch`, `FROM <earlier stage>`, `FROM --platform=...`, a CRLF file, a `FROM` inside a comment or a heredoc: the check reads instructions, not text (FR-009). A stage named like a registry image (`FROM alpine AS alpine`): the name counts as a stage only after its `AS` line, never before (FR-009).
- State. First run with no Docker Hub repository: the publish creates it public through the Docker Hub API, and a private or unreadable one stops the item with a fixed line (FR-016). A restart mid-publish leaves at most the `sha-` manifest list; the retry reuses a valid existing list and only tags `latest` (FR-020). A run for an old commit on a repo that has moved on (FR-018). A login name that differs from the namespace stops the publish with a fixed line (FR-031).
- Partial failure. `sha-` written and `latest` fails: the status is `error`, and the one retry the runner already gives reuses the existing list and only tags `latest` (FR-020). A failed build, a failed check or a missing token pushes nothing (FR-007, FR-011, FR-014).
- Time and concurrency. Two jobs for one image run at once (FR-018); the tick's concurrency group serialises ticks, so they do not overlap in practice. Docker Hub or Bitwarden slow or down: the item ends in `error`, retried once after 30 minutes by the existing rule. Pull limits on base images apply; a limit hit during the build is an `error`, not a `failure` (FR-007). The whole item (clone, pull, build, publish) stays under the 45-minute job limit through one shared deadline (FR-033).
- Multi-arch. A Dockerfile whose build must run target-architecture binaries (a `RUN` outside a `--platform=$BUILDPLATFORM` stage) needs emulation, which this feature does not provide: the check refuses it (FR-027). A base that is not multi-arch cannot build the other platform: that build fails and nothing is pushed. Pinning a base by its index digest, not a per-platform digest, lets one `FROM` serve both platforms (FR-028).
- Resources. Builds run on GitHub-hosted VMs. An image larger than the runner's disk ends in `error` with a fixed line (FR-021). No logs or cache grow without bound; the runner keeps nothing between runs.
- Permissions and trust. Who can start a publish: anyone who can merge to `main` of this repo, today only the owner. What the author of a Dockerfile cannot do: read the token, publish from a branch, publish an image the check or the archive check refused (FR-013, FR-014, FR-010, FR-030). Residual, stated plainly: whoever controls a `main` build step controls a privileged docker sidecar and can tamper with the VM, so that person is effectively a token holder; `main` is owner-merged, the check is review help and not a barrier, and a rootless BuildKit is the future fix (out of scope). The token is the one Docker Hub secret in the system; its scope is an assumption (A5).
- UI. None. Surfaces are the commit status, fixed public lines and the Docker Hub page.

## Requirements *(mandatory)*

Each requirement is an acceptance criterion in the form WHEN trigger THE SYSTEM SHALL response. `[error]` marks a failure path and `[boundary]` marks an edge of an allowed range. Each maps to at least one automated test in the runner's own pipeline (Principle V).

### Functional Requirements

**Layout**

- **FR-001**: WHEN `docker/<image>/` on `main` holds `Dockerfile`, `README.md` and `.factory-ci.toml` THE SYSTEM SHALL treat the folder as one runner project that publishes one Docker Hub repository, `sudtanj/<image>`.
- **FR-002** `[error]` `[boundary]`: WHEN a folder under `docker/` lacks one of the three files, or its name is not lowercase kebab-case of 2 to 64 characters THE SYSTEM SHALL fail that project with a fixed line naming the rule, and publish nothing. A name of 2 characters passes; 1 and 65 fail.
- **FR-003**: THE SYSTEM SHALL ship one first image, `docker/hello-static/`, built from a small static binary, with a distroless or `scratch` final stage, to prove the path end to end.

**Trigger**

- **FR-004**: WHEN a commit on `main` changes a file under `docker/<image>/` THE SYSTEM SHALL build, check and publish that image on the next tick that sees the commit, with no workflow change and no reaction to the push event itself.
- **FR-005** `[boundary]`: WHEN a commit changes no file under `docker/<image>/` THE SYSTEM SHALL build and publish nothing for that image (the runner's existing "unchanged since" rule) and leave its tags alone.
- **FR-006**: THE SYSTEM SHALL watch only `main` for `sudtanj/infrastructure` (`branches = ["main"]` stays), so no branch other than `main` is planned or built by the tick. WHEN a commit on any branch changes a file under `docker/` THE SYSTEM SHALL have the static image check run over every `docker/<image>/` folder as a Rust test in `rust-ci-runner/tests/repo_files.rs`, which the existing `factory-ci-selftest` workflow runs. Known limit: the selftest `paths` filter does not list `docker/**`, so a push that changes nothing but `docker/` does not start it, and the workflow cannot be edited (FR-022). The check on `main` (FR-008) still blocks a publish.
- **FR-007** `[error]`: WHEN a step of a `docker/<image>` pipeline fails, or the run times out, THE SYSTEM SHALL end the item in `error`, not `failure`, so the runner's single retry applies, because a rate limit or a network error while the sidecar pulls base images cannot be told from a deterministic build error. Deterministic mistakes in the Dockerfile and the pipeline already fail earlier, as `failure`, at the static check (FR-008). In every case nothing is pushed and only fixed lines are public. Build output goes to the log store, as for every step. A step of any other project keeps ending in `failure`.

**Safe by default**

- **FR-008**: THE SYSTEM SHALL run a check on every project under `docker/` before the build, and SHALL run it again inside the publish step, so a pipeline that omits it still cannot publish. A failed check ends the project in `failure`, names the failed rule in a fixed public line, and pushes nothing.
- **FR-009** `[error]`: WHEN a `FROM` in the Dockerfile names an image without `@sha256:` followed by 64 hex digits THE SYSTEM SHALL fail the check. `FROM scratch` and `FROM <name of an earlier stage>` are exempt. WHEN a `FROM` reference contains an `ARG` expansion (`$X` or `${X}`) THE SYSTEM SHALL fail the check, whether or not the `ARG` has a default. The pipeline `image` rule is in FR-029.
- **FR-010** `[error]`: WHEN the final stage of the Dockerfile is not `scratch` and not a distroless base (`gcr.io/distroless/static`, `static-debian12` or `base-nossl`, no `debug` tag) THE SYSTEM SHALL fail the check. WHEN the final stage has a `RUN`, or adds a shell or package manager, THE SYSTEM SHALL fail the check. WHEN the final stage sets no `USER`, or sets a non-numeric user or UID 0 THE SYSTEM SHALL fail the check.
- **FR-011** `[error]`: WHEN a Dockerfile in `docker/` holds an `ARG` or `ENV` whose name contains `TOKEN`, `PASSWORD`, `PASSWD`, `SECRET`, `CREDENTIAL` or `KEY` (any case), or a value that looks like a Docker Hub, GitHub or cloud token, or passes `--secret`, `--build-arg` or `--ssh` THE SYSTEM SHALL fail the check. WHEN the pipeline `env` table of a `docker/` project holds such a name or value THE SYSTEM SHALL fail the check. WHEN a pipeline under `docker/` lists `secrets` on any step THE SYSTEM SHALL fail the check. This is a name and pattern check, not a full secret scanner (see Non-goals).
- **FR-012**: THE SYSTEM SHALL build `linux/amd64` and `linux/arm64`, each natively and without emulation, and no other platform. Owner change 2026-11-28; see FR-026 to FR-028.

**Publish**

- **FR-013** `[error]`: WHEN a project runs for a ref other than `refs/heads/main`, or a project is not `docker/<image>`, THE SYSTEM SHALL export no Docker Hub credential to the runner, open no registry login, make no Docker Hub call and push nothing. The job script exports the credentials only when the runner's prepare mode lists them, and prepare lists them only for a passing `docker/<image>` on `main`. The script still loads the docker Bitwarden project into a shell variable of its own whenever the store token is set (`job.sh:75-76`); that is outside the runner and unchanged.
- **FR-014** `[error]`: WHEN a run on `main` passes the build and the check THE SYSTEM SHALL push the images it just built from a fresh directory that holds only the two platform archives, and no other image, with the publish step built into the runner. WHEN the token is missing, empty or refused by Docker Hub THE SYSTEM SHALL end the item in `error` with a fixed line and push nothing.
- **FR-015**: THE SYSTEM SHALL hand the Docker Hub token only to the runner process (which uses it for two Docker Hub API calls from memory) and to the push container, never to a step written in `docker/`, a docker sidecar or any container it started other than the push containers, an argv, an environment of any container, an image layer, a log line or a status. "Never to a sidecar" is about what the runner hands over (environment, argument, mount, standard input); a build step that escapes the privileged sidecar to the VM is the residual risk R12. The push container receives the token on standard input (`docker run -i`, `crane auth login --password-stdin`). The token is masked in every output path. Before the push the runner SHALL stop and remove the sidecar and every container of the run and remove the run's volumes, and SHALL mount a fresh directory that holds only the two platform archives, read-only.
- **FR-016** `[error]`: WHEN the Docker Hub repository `sudtanj/<image>` does not exist THE SYSTEM SHALL create it as a public repository through the Docker Hub API with the stored credentials, before it pushes, and verify afterwards that it is public. WHEN it exists and is public THE SYSTEM SHALL create and edit nothing. WHEN it exists and is private THE SYSTEM SHALL end the item in `error` with `publish refused: repository is private`, change nothing and push nothing. THE SYSTEM SHALL NOT change the visibility of an existing repository or sync README text. THE SYSTEM SHALL map every API answer to a fixed line by status code only: a rejected login is `registry refused the token`; a refusal (401 or 403) on the repository probe or on the creation is `docker hub api refused repository access` or `docker hub api refused repository creation`, never `registry refused the token`; a missing repository (404 on the probe after a successful login) is the create case, not an error; anything else is `docker hub api error`. No response body is logged or shown.
- **FR-017**: WHEN a publish succeeds THE SYSTEM SHALL tag the image `latest` and `sha-<first 12 hex digits of the commit>`, and no other tag. Each tag is a manifest list (FR-026).
- **FR-018** `[error]` `[boundary]`: WHEN the commit being published is not the tip of `main` at the moment of the push THE SYSTEM SHALL push only `sha-<12 hex>` and leave `latest` where it is. This holds when two jobs for one image run at once and when the older job finishes last. WHEN the tip of `main` cannot be read THE SYSTEM SHALL push nothing and end the item in `error`.
- **FR-019**: WHEN a folder under `docker/` is deleted THE SYSTEM SHALL push nothing for it and leave its Docker Hub repository and tags alone.
- **FR-020**: WHEN a publish runs again for a commit whose `sha-<12>` manifest list already exists (a retry or a repeat) THE SYSTEM SHALL check that list with the same rules as a new one (exactly one `linux/amd64` and one `linux/arm64` entry), check the archives it just built (FR-030), confirm that the commit is still the tip of `main`, and then change nothing about the list: it SHALL NOT push or overwrite anything and SHALL only tag `latest` on the existing list. A rebuild never matches the existing digests (the image config records the build time), so no digest comparison is made; the existing list is what stays published. WHEN the existing list fails the rules THE SYSTEM SHALL end in `error` with `image index incomplete` and change no tag.
- **FR-021** `[error]`: WHEN a push fails for a reason other than the token (network, rate limit, size, disk) THE SYSTEM SHALL end the item in `error` with a fixed line, so the existing single retry applies.

**Multi-arch** (owner change 2026-11-28)

- **FR-026** `[error]`: WHEN a publish runs THE SYSTEM SHALL push the `linux/amd64` image and the `linux/arm64` image by digest only, then create the manifest list in one registry write, and tag the list. No tag SHALL ever point at a single-platform image, and the list SHALL hold exactly one `linux/amd64` and one `linux/arm64` entry. WHEN either platform image is missing, mislabeled or fails to push THE SYSTEM SHALL tag nothing and end the item in `error` (a failed build step is `error` too, FR-007).
- **FR-027** `[error]`: WHEN a Dockerfile stage has a `RUN` and is not declared `FROM --platform=$BUILDPLATFORM` THE SYSTEM SHALL fail the check, because that stage would need emulation. The final stage already has no `RUN` (FR-010).
- **FR-028**: WHEN a `FROM` names a multi-arch base THE SYSTEM SHALL require the pin to be the index digest (`@sha256:` of the manifest list), so one reference resolves natively on each platform. A pin that is a single-platform digest makes the other platform's build fail, and the run publishes nothing (FR-026).
- **FR-029** `[error]`: THE SYSTEM SHALL read a Dockerfile in `docker/` against an instruction allowlist: `FROM`, `ARG`, `ENV`, `WORKDIR`, `COPY`, `RUN`, `USER`, `ENTRYPOINT`, `CMD`, `LABEL`, `EXPOSE`. It SHALL fail the check on any other instruction, `ADD` and `ONBUILD` included. It SHALL skip a heredoc body only for `RUN` and `COPY` and fail the check on `<<` anywhere else. It SHALL accept `COPY --from=` and `RUN --mount=...from=` only with the name or index of an earlier stage. It SHALL match parser directives (`syntax`, `escape`) in any case and with optional spaces, and fail the check on one. WHEN a step of a `docker/` pipeline names an `image` without `@sha256:` followed by 64 hex digits THE SYSTEM SHALL fail the check.
- **FR-030** `[error]`: WHEN publishing THE SYSTEM SHALL read the image config of each platform archive and refuse, before any push, an archive whose user is empty, non-numeric or uid 0, whose entrypoint or command starts with a shell, or whose `os` and `architecture` are not the platform the archive is named for. A refusal ends the item in `error` with a fixed line and pushes nothing.
- **FR-031** `[error]`: THE SYSTEM SHALL log in to Docker Hub with the value of `DOCKERHUB_USERNAME` and SHALL NOT treat that value as a secret: it is never registered with a redactor, never required to be longer than any floor, and used only for the login and for one comparison. WHEN the value is empty THE SYSTEM SHALL end the item in `error` with `publish refused: no login name`. WHEN it differs from the namespace constant `sudtanj` THE SYSTEM SHALL end the item in `error` with `publish refused: login is not the namespace`, print no value, and push and create nothing.
- **FR-033** `[error]`: THE SYSTEM SHALL run clone, base-image pulls, the pipeline and the publish under one shared deadline that leaves five minutes of the 45-minute job limit for the final status and cleanup. Each publish container and Hub call SHALL use the time that is left. WHEN too little is left THE SYSTEM SHALL end the item in `error` with `publish refused: time ran out` before it starts the next call, and push nothing more.

**Constraints**

- **FR-022**: THE SYSTEM SHALL change no file under `.github/workflows/` and no file under `.github/scripts/`. The automation changes only in Rust code under `rust-ci-runner/`, in `.factory-ci.toml` files and in other toml files (`.github/factory-ci-repos.toml`). A test or review step lists the changed paths of the feature and fails on any path under those two folders.
- **FR-023**: THE SYSTEM SHALL keep `sudtanj/infrastructure` at `branches = ["main"]` and add `allow_docker = true` to its watch entry, and the plan SHALL give a row `allow_docker` only when its project is `docker/<image>` and its branch is `main`. Every other project of the repo (`rust-ci-runner`, `ci-canary`) stays in the plain job, and a `docker = true` step in it is still refused.
- **FR-024**: THE SYSTEM SHALL keep feature 010's rule for user steps: the tick hands no secret to a pipeline step, and a pipeline that lists a secret still ends in `error`. The token reaches only the runner and the push container (FR-015); the publish step is not a step with `secrets`.
- **FR-025**: The last task in `tasks.md` SHALL update `README.md` and the GitHub repository description to match this feature (standing owner rule).

### Key Entities

- **Image project**: a folder `docker/<image>/` with a `Dockerfile`, a `README.md` and a `.factory-ci.toml`. One runner project, one Docker Hub repository.
- **Check**: the rules of FR-009 to FR-011, run before the build and inside the publish step.
- **Publish step**: a built-in step of the runner. Reads the token, pushes the two platform images by digest, creates the manifest list, tags it twice, on `main` only.
- **Docker Hub token**: the one credential. Lives in Bitwarden, read only by the publish step.
- **Tags**: `latest` (the tip of `main`) and `sha-<first 12 hex of the commit>`. Each is a manifest list.
- **Manifest list**: one entry for `linux/amd64`, one for `linux/arm64`.

## Success Criteria *(mandatory)*

### Hypothesis

If an image is a folder of three files and the runner does check and push, the owner can publish a public image by merging a pull request, without touching any workflow file and without a token in the repo. We would be wrong if adding the second image needs a change anywhere outside its own `docker/<image>/` folder, or if the owner still publishes by hand from a laptop.

### Measurable Outcomes

- **SC-001**: After the first image merges, `sudtanj/hello-static:latest` pulls without login within 15 minutes of the merge (one tick plus the build), and `sha-<12 hex>` resolves to the same digest. Measured by `docker pull` and `docker buildx imagetools inspect` from a clean machine.
- **SC-007**: After every tick that ran a `docker/` change on `main`, `sha-<first 12 hex of the main tip>` exists on Docker Hub for the changed image, checked with `docker buildx imagetools inspect`. A missing answer counts as unknown, not as ok.
- **SC-002**: A fixture Dockerfile per rule of FR-009 to FR-011 fails the check, and 100% of them leave zero pushes. Measured by the unit tests that count push calls.
- **SC-003**: Across runs of a non-image project and of a `docker/<image>` project that fails the check, zero Docker Hub credentials exported to the runner and zero Docker Hub calls. Across runs on `main`, zero occurrences of a token canary in public lines, argv, container environments, step environments and image layers. Measured by a counting environment and a canary search.
- **SC-004**: The diff of the feature touches zero paths under `.github/workflows/` and `.github/scripts/`. Measured by the path check of FR-022.
- **SC-006**: `docker buildx imagetools inspect sudtanj/hello-static:latest` and `...:sha-<12 hex>` each list exactly `linux/amd64` and `linux/arm64`, and the two commands print the same digest; a pull on an arm64 host and on an amd64 host starts the image. Measured by the same command and a pull on each host. Unit tests cover the manifest list check with fixtures: both platforms, one missing, a duplicate, a single manifest, an extra platform.
- **SC-005**: The second image, added one month after release, needs a change only inside `docker/<image>/`. Measured by that pull request's file list. This is the hypothesis test.

## Assumptions

- **A1**: Decision 2 binds the automation, not content. `Dockerfile`, `README.md` and `specs/` files, and the new `docker/` content, are not workflow YAML or scripts and change as the feature needs. `Cargo.toml` and `Cargo.lock` count as toml. If the owner meant that nothing else may change, the first image cannot exist. This is the reading the lead adopted; it goes in the open questions of the report.
- **A2**: The runner finds image projects by their `.factory-ci.toml` under `docker/<image>/` and treats each as its own project, as it does for other project folders. The plan confirms how the tick lists them.
- **A3**: The tick runs `docker/<image>` rows on `main` on the docker job path, because they build with Docker steps. That path needs `allow_docker = true` on the `sudtanj/infrastructure` entry and uses the docker Bitwarden project and Environment. The plan narrows the entry flag to those rows (FR-023). The two keys the publish uses, `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`, already sit in the project `FACTORY_CI_BWS_PROJECT_DOCKER` names, as shared names, and the entry lists them in `secrets` (see A11). Both pass `valid_secret_name`.
- **A4**: `.github/factory-ci-repos.toml` keeps `branches = ["main"]` for this repo (lead decision 2026-11-28). A wider `branches` would put every branch tip of `rust-ci-runner` and `ci-canary` on the docker job and would run a branch's `docker/` build in the privileged sidecar. The cost is FR-006: branches get the static check only. Widening `branches` later needs a walk rule for branch-built commits that later become the `main` tip (a branch `success` would stop a fast-forwarded `main` from publishing); `research.md` D3 keeps that analysis.
- **A5**: The owner has created the Docker Hub access token (A14). The runner creates each public repository `sudtanj/<image>` itself (FR-016). Not verified: whether a Read & Write token may create a repository through the Docker Hub API. T002 checks it; if a broader scope is needed that is an owner decision, not a workaround. A 404 on the repository probe after a successful login means "missing", and a 401 or 403 on the probe or the creation is a scope problem, not a bad token (FR-016).
- **A6**: A "distroless base" means an image under `gcr.io/distroless/`, such as `static` or `base-nonroot`, pinned by digest. Other minimal bases (for example Chainguard) are not allowed until a later spec adds them.
- **A7**: The runner is released through the existing tag-driven release workflow with no YAML change. The tick reads the runner digest from the repository variable `RUNNER_IMAGE_DIGEST`. The manual `factory-monorepo-ci` pin stays at its current version, which this feature does not need. If the runner image itself must change (its own `Dockerfile`), that is a change outside the allowed set and goes back to the owner.
- **A8**: Spec 011 keeps the tick, so the tick is the trigger. If 011 removes it, this feature has no trigger.
- **A10**: arm64 is built without emulation. The Dockerfile cross-compiles in a `--platform=$BUILDPLATFORM` stage, and the final stage only copies files (no `RUN`). Each platform is a separate `docker build --platform` with its own `docker save` archive. A Dockerfile that needs emulation is out of scope and refused by FR-027.
- **A11**: The credential route on the tick: the job script `.github/scripts/factory-ci-job.sh` unsets the store token before it starts the runner and cannot change under FR-022, so the runner cannot call Bitwarden itself. The script fetches names the runner's prepare mode lists (its names-file mechanism) and exports them to the runner. For a passing `docker/<image>` row on `main` the runner lists `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`. Because both are shared names, the script resolves them only when the row's `secrets` field lists them, and that field is the watch entry's `secrets`: the entry therefore carries `secrets = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]`. The script reads them from the docker Bitwarden project and exports them to the runner. The runner uses the token for two Docker Hub API calls from its own memory and passes it on standard input to its push container. No user-written step receives either value. This keeps the owner's intent (decision 1). Effects of the entry's `secrets` elsewhere: on the manual `ci` path a ref gets step secrets only through `secret_branches` (`watch.rs:39-43`, `ci.rs:224-231`, `ci.rs:385-391`), the entry has none, so nothing is unlocked there; the tick's rows carry the list, but prepare lists a name only for a docker row on `main`, so no other project receives either value. The script masks the username in the Actions log (the word `sudtanj` shows as `***` in later lines of that job); commit statuses are unaffected.
- **A13**: The Go builder in `docker/hello-static` comes from `mirror.gcr.io/library/golang:<supported version>@sha256:<index digest>`, so the sidecar's pull avoids the anonymous Docker Hub limit, with `GOTOOLCHAIN=local` so Go downloads no toolchain. The version is a Go release that still gets security fixes on the day the image is written. Base digests are refreshed by commit when a base gets a security fix (A9); `docker/README.md` names the routine.
- **A14**: Owner setup before the first publish (owner actions; an agent never reads a value): the project `FACTORY_CI_BWS_PROJECT_DOCKER` names holds the two keys `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` under exactly those names, readable by the machine account behind the docker store access token; the Docker Hub token has Read & Write scope (no delete, no admin) and an expiry, and (T002 decides) whatever scope the API needs to create a repository; `FACTORY_CI_TOKEN_DOCKER` covers `sudtanj/infrastructure` with Commit statuses read and write and Contents and Metadata read, and is the same GitHub user as `FACTORY_CI_TOKEN`; the `factory-ci-docker` Environment is limited to `main`; `FACTORY_CI_BLOB_KEY_DOCKER` is set. No agent can check the key names: `bws secret list` returns values, so a names-only read does not exist. The owner runs the one-line check in quickstart 1.
- **A12**: A folder under `docker/` with neither `.factory-ci.toml` nor `Dockerfile` is not a runner project and is ignored: there is no project to post a status on. A folder with either one is a project, and FR-002 fails it by name when the other required files are missing.
- **A9**: Bumping a base image digest is a commit to the folder, so it rebuilds the image. No schedule rebuilds images when a base gets a security fix.

## Non-goals

- Image signing, SBOM and vulnerability-scan gating.
- Registries other than Docker Hub, and private images.
- Platforms other than `linux/amd64` and `linux/arm64` (`arm/v7`, `386`, Windows), and QEMU emulation of any kind.
- Changing the visibility of an existing Docker Hub repository, deleting one, or syncing README text through the Docker Hub API. (Creating a missing repository as public is in scope, FR-016.)
- Reacting to the push event, or any new workflow, script or trigger. The tick is the only trigger.
- Scheduled rebuilds, base image update bots, and deleting tags or repositories on Docker Hub.
- A full secret scanner. The check looks for credential names, flags and common token shapes in `docker/` files only.
- Per-image tags beyond `latest` and `sha-<12 hex>`; semantic version tags.
- Giving any user-written step the Docker Hub token.

## Risks

- **R1, privileged builds**: image builds use `docker = true` steps, which run against a privileged sidecar. `allow_docker` on `sudtanj/infrastructure` lets `docker/<image>` rows on `main` run them; no other project of the repo gets docker. Anyone who can merge to `main` can add a `docker = true` step to a `docker/` project and get root on the runner VM. Today that is the owner alone. Mitigation: the token is never in the sidecar and `main` is protected (see R12).
- **R2, token blast radius**: one Docker Hub token that can write to the whole namespace sits in Bitwarden. A runner bug that prints it exposes every image. Mitigation: masking on every output path, a canary test, and a token scoped as narrowly as Docker Hub allows.
- **R3, trigger delay**: a push reaches Docker Hub up to 5 minutes plus the build time later, and later if the tick is busy. Accepted by the owner.
- **R4, latest race**: the tip check and the push are two steps. The plan must serialise publishes of one image so the older job cannot move `latest` back. FR-018 has a test for the case.
- **R5, check is a heuristic**: the name and pattern rules of FR-011 miss an obfuscated credential. They stop the common mistake only.
- **R6, base image supply**: a digest pin freezes a vulnerable base until someone bumps it, and nothing alerts. Accepted for now (A9).
- **R10, no emulation**: a Dockerfile that must run target-arch code cannot publish until a later spec adds emulation (QEMU binfmt in the privileged sidecar). Cross-compilation covers static binaries in Go, Rust and C.
- **R11, orphan platform manifests**: the two platform images are pushed by digest before the list exists, so a failed run leaves untagged manifests on Docker Hub. They hold no tag and Docker Hub removes them later.
- **R7, Docker Hub limits**: anonymous pulls of base images and pushes are rate limited. A limit hit ends the item in `error`, with one retry.
- **R8, private repository**: a repository that exists and is private would not be pullable. The runner never changes it and ends in `error` (FR-016); the owner decides.
- **R13, repository creation scope**: the Docker Hub API may refuse repository creation to a Read & Write token. T002 checks. If it does, the owner either widens the scope or creates the repository by hand; the runner reports `docker hub api refused repository creation` and pushes nothing.
- **R12, residual token exposure**: the build steps run against a privileged docker sidecar. Whoever controls a `main` build step can tamper with the VM through it before the runner tears it down, and is effectively a token holder. Mitigations: `main` is owner-merged; the token is on standard input of the push container only; the sidecar and its volumes are removed before any container or request uses the token; the push container sees only the two archives, read-only; the archives are checked before the push. The static check is review help, not a barrier. A rootless BuildKit is the future fix and is out of scope.
- **R9, A1 reading**: if the owner reads decision 2 more strictly than A1, scope shrinks to nothing shippable. Named in the report so the owner can correct it.
