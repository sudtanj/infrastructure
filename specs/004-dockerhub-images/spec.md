# Feature Specification: Docker Hub Images

**Feature Branch**: `004-dockerhub-images`

**Created**: 2026-11-27

**Status**: Draft

**Input**: User description: "A Docker Hub deployer. A top-level folder `images/` holds one folder per image, `images/<name>/`, with at least a `Dockerfile` and a `README.md`. Each folder is a separate project with its own Docker Hub repository. A push to `main` that changes an image builds and pushes that image for linux/amd64 and linux/arm64, tagged `latest` and `sha-<short commit>`. Pull requests and other branches build without pushing and without any credential. The Docker Hub username and an access token come from Bitwarden Secrets Manager. Every `FROM` pins a digest. The first image is `images/hello/`."

## Clarifications

### Session 2026-11-27

Answers come from the lead's brief (the owner's decisions). Nothing blocked on the owner.

- Q: A `FROM` line cannot carry a digest in two cases: `FROM scratch` and `FROM <earlier stage name>`. Do they pass the digest check? → A: Yes. Both are exempt, because no digest exists for either. Every other `FROM` needs a digest (FR-018).
- Q: What does the check do with `ARG` inside `FROM` (`FROM ${BASE}`)? → A: It fails the image. The check reads the file as text and cannot prove what an argument holds, so the image reference must be a literal. A `--platform=` flag may use a variable; the image reference may not (FR-020).
- Q: What happens when two pushes reach `main` close together? → A: No change is dropped, and `latest` of an image never moves back to an older commit. The plan picks the mechanism; GitHub concurrency groups alone drop the older pending run, so the plan must show how it avoids that (FR-026).
- Q: Does the workflow retry on a Docker Hub rate limit? → A: No. The image fails with a fixed message, the other images continue, and the owner reruns by manual dispatch. A retry loop is scope the owner did not ask for (FR-027).
- Q: Who creates the Docker Hub repository, and does the README sync to its description? → A: The first push creates the repository; the workflow does not create it through an API and does not sync `README.md` to Docker Hub. The owner checks that the account default makes new repositories public (Assumptions, Out of Scope).
- Clarify pass: no question was left without a safe default. Non-blocking items sit under Assumptions.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Merge a change, get a multi-arch image on Docker Hub (Priority: P1)

The owner edits `images/<name>/` and merges to `main`. The workflow builds that image for amd64 and arm64 and pushes it as `<docker hub username>/<name>` with tags `latest` and `sha-<short commit>`. Other images are not touched.

**Why this priority**: This is the feature. Without it the owner builds and pushes by hand from a laptop.

**Independent Test**: Push a change under `images/hello/` to `main` and pull `<username>/hello:latest` and `:sha-<short commit>` on an amd64 and an arm64 host.

**Acceptance Scenarios**:

1. **Given** a push to `main` that changes only `images/a/`, **When** the workflow runs, **Then** it builds and pushes `a` only, with both tags and both platforms in one manifest.
2. **Given** a push to `main` that changes `images/a/` and `images/b/`, **When** the workflow runs, **Then** both images build as separate jobs and each pushes its own tags.
3. **Given** a push to `main` that changes the deployer workflow file, **When** the workflow runs, **Then** every folder under `images/` builds and pushes.
4. **Given** a push to `main` that changes nothing under `images/` and not the workflow file, **When** the workflow runs or its path filter skips it, **Then** nothing is built and nothing is pushed.
5. **Given** a push to `main` that deletes `images/a/` entirely, **When** the workflow runs, **Then** it builds nothing for `a`, reports no error for it, and leaves `<username>/a` on Docker Hub as it is.

---

### User Story 2 - Pull requests prove the build without publishing (Priority: P1)

A pull request or a push to any other branch builds the changed images for both platforms and pushes nothing. The job has no credential and makes no Bitwarden call.

**Why this priority**: A public repo takes pull requests from forks. A secret must never reach them, and a broken image must show before the merge.

**Independent Test**: Open a pull request that changes `images/hello/`. The run is green, Docker Hub shows no new tag, and the run log shows no Bitwarden call.

**Acceptance Scenarios**:

1. **Given** a pull request that changes `images/a/`, **When** the workflow runs, **Then** it builds `a` for amd64 and arm64, pushes nothing and starts no login.
2. **Given** a pull request from a fork, **When** the workflow runs, **Then** no Environment, secret or Bitwarden token is available to any job.
3. **Given** a push to a branch other than `main`, **When** the workflow runs, **Then** it behaves as in scenario 1.
4. **Given** a pull request that changes the deployer workflow file, **When** the workflow runs, **Then** it builds every image without pushing.

---

### User Story 3 - A bad folder or Dockerfile fails before the build (Priority: P1)

A folder with a bad name, no `Dockerfile` or an unpinned `FROM` fails with a message that names the folder and the rule. The owner fixes it in the same pull request.

**Why this priority**: Digest pins are the supply-chain control the owner chose in place of a vulnerability scan. A check that can be skipped is no control.

**Independent Test**: Run the check against fixture folders, one per rule, and read the exit status and message of each.

**Acceptance Scenarios**:

1. **Given** `images/My_App/`, **When** the check runs, **Then** it fails and names the rule "lower-case kebab-case".
2. **Given** `images/a/` with no `Dockerfile`, **When** the check runs, **Then** it fails for `a` and names the missing file.
3. **Given** `FROM alpine:3.20`, **When** the check runs, **Then** it fails and prints the line number.
4. **Given** `FROM alpine:latest`, **When** the check runs, **Then** it fails.
5. **Given** `FROM alpine:3.20@sha256:<64 hex>`, **When** the check runs, **Then** it passes.
6. **Given** a multi-stage file where one of three `FROM` lines has no digest, **When** the check runs, **Then** it fails on that line only.
7. **Given** `FROM ${BASE}` with an `ARG BASE=...@sha256:...` above it, **When** the check runs, **Then** it fails (FR-020).

---

### User Story 4 - Docker Hub credentials stay in Bitwarden and out of the build (Priority: P1)

The push job reads the Docker Hub username and token from Bitwarden with the `BWS_ACCESS_TOKEN` in the `factory-ci` Environment. Only `main` can use that Environment. The build step never holds the Bitwarden token or the Docker Hub token.

**Why this priority**: The repo is public and a Dockerfile is code that anyone can edit in a pull request. The token has Read & Write scope on every image repository.

**Independent Test**: Shell tests with a fake `bws` and sentinel values: no sentinel appears in any output; a pull-request run makes no `bws` call; an outage pushes nothing.

**Acceptance Scenarios**:

1. **Given** a push to `main`, **When** the push job starts, **Then** it fetches `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` from Bitwarden and masks both before any other use.
2. **Given** the build step, **When** it runs, **Then** neither `BWS_ACCESS_TOKEN` nor the Docker Hub token appears in its environment, its arguments or its build arguments.
3. **Given** Bitwarden is unreachable or returns an error, **When** the push job runs, **Then** it fails red with a fixed message and pushes nothing.
4. **Given** the item `DOCKERHUB_TOKEN` is missing or the token is revoked, **When** the login step runs, **Then** the job fails red and pushes nothing.
5. **Given** a run from a branch other than `main`, **When** a job names the `factory-ci` Environment, **Then** GitHub refuses the job and no secret is read.

---

### User Story 5 - Rebuild one image or all of them by hand (Priority: P2)

The owner starts the workflow manually on `main` and names one image or `all`. The named images build and push with the same tags and rules as a push.

**Why this priority**: A rate limit, a Docker Hub outage or a rotated token leaves a failed run. The owner needs a rerun that does not require a fake commit.

**Independent Test**: Dispatch with `image=hello`, then with `image=all`, then with a name that does not exist.

**Acceptance Scenarios**:

1. **Given** a manual run on `main` with `image=hello`, **When** it runs, **Then** it builds and pushes `hello` only.
2. **Given** a manual run on `main` with `image=all`, **When** it runs, **Then** it builds and pushes every folder under `images/`.
3. **Given** a manual run with a name that has no folder, **When** it starts, **Then** it fails before any build and names the missing folder.
4. **Given** a manual run on a branch other than `main`, **When** it runs, **Then** it builds without pushing, as in User Story 2.

---

### User Story 6 - One failing image does not stop the rest (Priority: P2)

Each image builds in its own job. When one fails, the others finish and push. The run is red and its summary lists which images failed and which passed.

**Why this priority**: The folders are independent projects. A bug in one must not hold back a fix in another.

**Independent Test**: Push changes to a good image and to a fixture with a failing `RUN` line in the same commit.

**Acceptance Scenarios**:

1. **Given** `images/good/` and `images/bad/` both changed, **When** `bad` fails to build, **Then** `good` still builds and pushes, the run ends red, and the summary names `bad` as failed and `good` as pushed.
2. **Given** an image whose arm64 build fails and whose amd64 build passes, **When** the job runs, **Then** it pushes nothing for that image and reports the platform that failed.

---

### User Story 7 - A first image proves the pipeline (Priority: P3)

`images/hello/` is a minimal image the owner can delete later. It proves the pipeline from a merge to a pullable tag.

**Why this priority**: The pipeline needs one real target to be tested end to end. It adds no value after that.

**Independent Test**: Merge the feature. `<username>/hello` shows both tags and both platforms. Delete `images/hello/` in a pull request and see a green run with no workflow edit.

**Acceptance Scenarios**:

1. **Given** the merged feature, **When** the first run on `main` finishes, **Then** `<username>/hello:latest` and `:sha-<short commit>` exist for amd64 and arm64.
2. **Given** `images/hello/` deleted in a pull request, **When** the workflow runs, **Then** no workflow file needed an edit and the run is green.

---

### Edge Cases

- **Folder without a `Dockerfile`**: that image fails the check; other images continue. A folder without `README.md` fails the same way.
- **Bad folder name** (`My_App`, `-a`, `a--b`, `a b`, upper case, a name over 64 characters): fails the check before any build. No Docker Hub call is made for it.
- **A file directly under `images/`** (not a folder): ignored, not an image.
- **`FROM` without a digest**: fails with the file, the line number and the rule. Case-insensitive (`from`), line continuations and `FROM --platform=linux/amd64 image@sha256:...` are read correctly.
- **Multi-stage**: every `FROM` is checked. `FROM builder` where `builder` is an earlier `AS builder` stage passes. `FROM scratch` passes.
- **`ARG` inside `FROM`**: fails (FR-020).
- **A digest of the wrong shape** (`@sha256:abc`): fails; the digest must be 64 lower-case hex characters.
- **Two images changed in one push**: two jobs, two pushes, independent results.
- **An image deleted in a push**: nothing to build. A push that only deletes images makes the run green with an empty plan.
- **Folder deleted and another changed in the same push**: the deleted one is skipped, the other builds.
- **Missing or wrong Docker Hub token**: login fails, the job is red, nothing is pushed, the message does not echo the token.
- **Bitwarden outage**: the push job fails after the fetch window, pushes nothing, and says so in fixed text (FR-016).
- **Docker Hub rate limit** (HTTP 429 on a base-image pull or on push): that image fails with a message that names the cause when the cause is known. No automatic retry. The owner reruns with a manual dispatch.
- **arm64 build failure**: the whole image fails; no amd64-only manifest is published (FR-024).
- **Concurrent push to `main`**: see FR-026.
- **A force push or a branch's first push** with no usable "before" commit: the plan compares with the merge base against `main`; if none exists it builds every image without pushing.
- **A pull request edits the workflow file**: the edited file runs, but a pull-request job has no Environment and no secret, so the edit cannot reach a credential (FR-015). Only a push to `main` can.
- **Run on a tag**: the workflow has no tag trigger.
- **`images/` does not exist**: the plan is empty, the run is green and builds nothing.
- **The Docker Hub repository does not exist yet**: the first push creates it (Assumptions).
- **The short commit**: the first 7 characters of the commit SHA that triggered the run. A manual run uses the SHA of the `main` head it runs on.

## Requirements *(mandatory)*

### Functional Requirements

**Layout and naming**

- **FR-001**: Each image MUST live in `images/<name>/` and hold at least `Dockerfile` and `README.md`. The folder is the build context and a separate project with its own Docker Hub repository.
- **FR-002**: `<name>` MUST match `^[a-z0-9]+(-[a-z0-9]+)*$` and be 2 to 64 characters long. Any other folder name MUST fail the check for that folder.
- **FR-003**: The workflow MUST find images by listing the folders under `images/`. It MUST NOT hold a list of image names.

**Publishing**

- **FR-004**: A push job MUST publish `<docker hub username>/<name>` with exactly two tags: `latest` and `sha-<first 7 characters of the commit SHA>`.
- **FR-005**: A push to `main` that changes a file under `images/<name>/` MUST build and push that image and no other.
- **FR-006**: A push to `main` that changes the deployer workflow file MUST build and push every image.
- **FR-007**: A pull request, or a push to any branch other than `main`, MUST build the changed images and MUST NOT push. A change to the deployer workflow file in such a run builds every image.
- **FR-008**: A manual run MUST take one input: an image name or `all`. On `main` it builds and pushes the named images. On another branch it builds without pushing. A name with no folder MUST fail the run before any build.
- **FR-009**: A folder removed in a push MUST NOT be built, MUST NOT cause an error, and MUST NOT cause any call that deletes or edits its Docker Hub repository.

**Platforms**

- **FR-010**: Every build, pushing or not, MUST cover `linux/amd64` and `linux/arm64`, and a push MUST publish them as one multi-platform manifest under each tag.

**Runner**

- **FR-011**: The deployer MUST be a plain GitHub Actions workflow in this repo on `ubuntu-24.04`. It MUST NOT go through the factory CI runner and MUST NOT be added to `.github/factory-ci-repos.toml`. This is an owner-granted exception of the same kind as `rust-ci-runner-release`; this spec and the plan record it.

**Credentials**

- **FR-012**: Only a job that pushes MAY name the Environment `factory-ci` and read `BWS_ACCESS_TOKEN`. That job runs only on `main`.
- **FR-013**: The push job MUST read `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` from the same Bitwarden project the factory CI secrets use, and MUST mask both values before any other use.
- **FR-014**: The build step MUST NOT receive `BWS_ACCESS_TOKEN`, `DOCKERHUB_TOKEN` or `DOCKERHUB_USERNAME` as an environment variable, argument or build argument. Only the login step holds the token. No secret MAY enter an image layer.
- **FR-015**: A job that does not push MUST have no Environment, no secret, no Bitwarden call and no registry login.
- **FR-016**: A Bitwarden failure, a missing item or a rejected login MUST fail the push job, push nothing, and print a fixed message that carries no secret and no Bitwarden response body. The Bitwarden fetch uses the existing bounded window (60 seconds).
- **FR-017**: The token MUST have Read & Write scope on Docker Hub and MUST NOT be the account password. The README says so; the workflow cannot check it.

**Dockerfile check**

- **FR-018**: Every `FROM` line MUST name its image as `image:tag@sha256:<64 lower-case hex>`. A `FROM` with no digest, including `:latest` without a digest and a bare name, MUST fail the check. `FROM scratch` and `FROM <earlier stage name>` are exempt.
- **FR-019**: In a multi-stage file the check MUST read every `FROM` line, accept `AS <name>` and `--platform=...` flags, and report every failing line in one pass.
- **FR-020**: A `FROM` whose image reference contains a variable (`$X`, `${X}`) MUST fail the check, even when an earlier `ARG` holds a digest.
- **FR-021**: The check MUST run for every image in every run (pull request, branch, `main`, manual) before the build starts. A failing image MUST NOT be built, and its message MUST name the folder, the file, the line number and the rule.

**Failure isolation and reporting**

- **FR-022**: Each image MUST build in its own job. A failure in one MUST NOT cancel or skip the others.
- **FR-023**: The run MUST end red when any image fails, and its summary MUST list each image with its result: pushed, built without push, failed with the reason class (check, build, login, push, rate limit), or skipped.
- **FR-024**: When any platform fails, the image MUST publish nothing, not even the other platform.
- **FR-025**: A push job MUST NOT push before every platform of that image has built.
- **FR-026**: When two pushes reach `main` close together, no image change MUST be dropped, and `latest` of an image MUST NOT move to a commit older than the one it already points to.
- **FR-027**: On a Docker Hub rate limit the image MUST fail with a message that names the cause when the response shows it. The workflow MUST NOT retry on its own.

**Hygiene**

- **FR-028**: The workflow MUST set `permissions` to `contents: read` at workflow and job level, and MUST pin every third-party action by commit SHA (constitution, Repository Structure and Constraints).
- **FR-029**: The plan logic (changed images, name check, `FROM` check, summary) MUST live in scripts under `.github/scripts/` with shell tests, so every acceptance scenario here runs without Docker Hub. The selftest MUST run those tests, and the existing workflow lint MUST pass on the new workflow.

**First image and docs**

- **FR-030**: The repo MUST ship `images/hello/` with a `Dockerfile` whose `FROM` carries a digest, a `README.md`, and a command that prints one fixed line. Deleting the folder MUST need no edit to the workflow.
- **FR-031**: The last task in `tasks.md` MUST update the root `README.md` (the `images/` layout, naming rule, tags, the `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN` names in Bitwarden, the `BWS_ACCESS_TOKEN` use by this workflow, the token scope, the public visibility, the removed-folder behaviour, the exception) and the GitHub repository description, per constitution 1.1.0. That task, and every edit to `README.md`, `.github/`, `CLAUDE.md`, `AGENTS.md` and the constitution, waits until feature 003 has merged.
- **FR-032**: After 003 merges, a MINOR amendment to `.specify/memory/constitution.md` MUST add `images/` to the repository layout and record the bare-workflow exception, as its own pull request under the Governance procedure.

### Key Entities

- **Image folder**: `images/<name>/`. Holds `Dockerfile`, `README.md` and the build context. Maps to one Docker Hub repository.
- **Image name**: the folder name. Lower-case kebab-case, 2 to 64 characters.
- **Tag pair**: `latest` and `sha-<7 characters>`. Both are written on every push of an image.
- **Credential pair**: `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`, stored in Bitwarden, held only by the login step.
- **Run plan**: the list of images to build, whether to push, and the reason each is in the list.
- **Run summary**: one line per image with its result.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: After a merge that changes one image, its `latest` and `sha-<short commit>` tags exist on Docker Hub for both platforms within 30 minutes of the merge (hello: within 10 minutes). Measured on the first merge, then on each later one from the run's duration.
- **SC-002**: Per push to `main`, the number of images pushed equals the number of changed image folders, or the full count when the workflow file changed. Checked by the plan tests with 0, 1, 2, all and deleted-only cases.
- **SC-003**: Across all pull-request and non-`main` runs, Bitwarden calls and registry logins equal 0 (shell test with a fake `bws`; the first real pull request confirms it in the log).
- **SC-004**: The Dockerfile check rejects 100% of the fixtures for the rules in FR-018 to FR-020 and accepts 100% of the passing fixtures (at least 12 fixtures).
- **SC-005**: A secret sentinel appears 0 times in the output of every test and in the first real run's public log.
- **SC-006**: With one failing and one good image in the same run, the good image is pushed and the summary names the failing one (test with fixtures; the first real run need not break an image on purpose).
- **SC-007**: Adding a new image takes 1 new folder and 0 workflow edits. Checked by adding a fixture folder to the plan test and by deleting `images/hello/`.
- **Hypothesis and measure**: the owner will add more than `hello`. If after 90 days `images/` holds only `hello`, the owner deletes it and weighs whether the pipeline earns its upkeep. The first-run duration (SC-001) shows whether QEMU arm64 builds are too slow to keep.

## Assumptions

- The owner already has a Docker Hub account and a Bitwarden Secrets Manager project holding `DOCKERHUB_USERNAME` and `DOCKERHUB_TOKEN`; the owner creates them by hand and the README says how. No value is written to the repo.
- The Bitwarden project is the default project the factory CI already reads. The plan names how the workflow learns its id, reusing what the tick does.
- The Docker Hub account creates new repositories as public by default. If not, the owner makes each repository public after the first push; the workflow does not set visibility.
- arm64 builds use emulation on an amd64 runner. They are slow. A native arm64 runner is out of scope.
- A pull request build of a Dockerfile runs untrusted `RUN` commands on a GitHub-hosted runner without any secret. That is the accepted risk of building fork code.
- `hello` is allowed to ship before the constitution amendment (FR-032). The plan records the exception in its Complexity Tracking until then.
- Feature 003 is changing `README.md`, `.github/`, the constitution, `CLAUDE.md` and `AGENTS.md`. This spec adds files under `.github/` only after 003 merges; the plan sequences that.
- The base image of `hello` is a small public image pinned by digest. Choosing it belongs to the plan.

## Out of Scope

- Registries other than Docker Hub.
- Image signing.
- SBOM, provenance attestation and vulnerability scanning.
- Tag schemes beyond `latest` and `sha-<short commit>` (no semver, no branch tags).
- Deleting or editing a Docker Hub repository when its folder is removed. A removed folder stops publishing and nothing else changes.
- Syncing `README.md` to the Docker Hub description.
- Native arm64 runners, caching tuning and build-time limits beyond GitHub's defaults.
- Automatic retries on Docker Hub failures.

## Risks

- **A push to `main` is the only gate for the token**: a merged Dockerfile runs `RUN` commands during the build, and the token sits in the same job's runner. FR-014 keeps it out of the build step's environment, but the runner's credential file exists while the build runs. The owner reviews every Dockerfile before merging, and the token scope is limited to Read & Write on the account's repositories. The plan decides whether login and push can use a separate step order to shrink this window.
- **The pending-run drop**: GitHub keeps one pending run per concurrency group and cancels older pending ones (FR-026).
- **Rate limits**: anonymous base-image pulls on shared runners hit Docker Hub limits. Digest-pinned pulls do not lower the count.
- **Slow arm64 emulation**: a heavy Dockerfile can take longer than the 30-minute target.
- **Public token blast radius**: a leaked token publishes to every repository of the account. The owner rotates it in Bitwarden; nothing in the repo changes.
