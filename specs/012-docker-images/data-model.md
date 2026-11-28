# Data model: Public Docker images from `docker/`

No database. The entities are files, one constant set and the status texts.

## Image project

A folder `docker/<image>/` in `sudtanj/infrastructure`.

| Field | Rule | Check id |
|---|---|---|
| name | `<image>`: `^[a-z0-9]+(-[a-z0-9]+)*$`, length 2 to 64 | `name`: enforced by `image::name_ok` on the folder name, outside `check(dir)` (the plan posts `error` "invalid project folder name", no job; `check` never returns `name`) |
| required files | `Dockerfile`, `README.md` and `.factory-ci.toml`, each a regular file, not a symlink | `layout` |
| project path | `docker/<image>`, status context `factory-ci/docker/<image>` | none |

A folder with `.factory-ci.toml` or `Dockerfile` is a project. A folder with neither is ignored (spec A12).

## Check rules (`rust-ci-runner/src/image.rs`)

`check(dir) -> Vec<&'static str>` returns the ids that fail, sorted, once each. The public status shows `check failed: ` plus at most three ids joined by `, `. It never shows file content.

The parser works on instructions. It joins `\` continuations, strips `\r` and a BOM, skips comment lines and empty lines, and skips a heredoc body (`<<WORD`, `<<-WORD`, `<<"WORD"`) up to the terminator line, only for `RUN` and `COPY`. Instruction keywords are case-insensitive. Stage index counts `FROM` lines from 0.

| Id | Fails when | Edge answers |
|---|---|---|
| `layout` | a required file is missing, a symlink or not a file; the Dockerfile is over 256 KiB (exactly 256 KiB passes) or not UTF-8 | CRLF and BOM are not a layout problem |
| `directive` | any line matches `^\s*#\s*(syntax|escape)\s*=`, case-insensitive, anywhere in the file | `# syntax=x`, `#syntax = x`, `# SYNTAX=x`, `# Escape=\``, `#   syntax=x` all fail |
| `instruction` | an instruction is not in `FROM ARG ENV WORKDIR COPY RUN USER ENTRYPOINT CMD LABEL EXPOSE` | `ADD`, `ONBUILD`, `SHELL`, `HEALTHCHECK`, `VOLUME`, `STOPSIGNAL`, `MAINTAINER` fail |
| `heredoc` | `<<` appears in an instruction other than `RUN` or `COPY` | `ENV A=<<X`, `LABEL`, `ARG`, `FROM` fail; a `FROM` text inside a `RUN` heredoc body is not an instruction |
| `external-source` | `COPY --from=X` or `RUN --mount=...from=X` where X is not the name (case-insensitive) of an earlier stage and not the number of an earlier stage | `--from=alpine`, `--from=docker.io/x`, `--from=5` in stage 2 fail; `--from=build`, `--from=0` pass |
| `from-unpinned` | a `FROM` reference (after `--flag=...` options) is not `scratch`, not a stage name defined by an earlier `AS`, and does not end in `@sha256:` plus 64 hex digits | `FROM alpine AS alpine`: the name counts only after its own line; `FROM --platform=$BUILDPLATFORM x@sha256:..` passes because the flag is not the reference. For a multi-arch base the digest must be the index digest (FR-028); the check cannot tell offline, the other platform's build fails if it is not |
| `from-arg` | a `FROM` reference contains `$` | with or without an `ARG` default |
| `emulation` | a stage that has a `RUN` has no `--platform=$BUILDPLATFORM` (or `${BUILDPLATFORM}`) on its `FROM` | FR-027 |
| `final-base` | the final stage, followed back through `FROM <stage>` links, does not end at `scratch` or a reference starting `gcr.io/distroless/` whose name and tag contain no `debug` | a chain deeper than 20 links fails (the check treats it as not ending at a permitted base); a chain of exactly 20 passes |
| `final-run` | any stage on the final chain has `RUN` | |
| `final-shell` | the final stage has a shell-form `CMD` or `ENTRYPOINT` (not starting with `[`), or a `COPY` whose source or destination basename is `sh bash dash ash busybox apk apt apt-get dpkg yum dnf microdnf zypper pip npm` | name heuristic, R5 |
| `final-user` | the last `USER` of the final chain is absent, not `^[0-9]+(:[0-9]+)?$`, or its uid is 0 | `USER 1000:0` passes; `USER $X` fails as non-numeric |
| `secret-name` | an `ARG` or `ENV` name, any stage, contains (case-insensitive) `TOKEN`, `PASSWORD`, `PASSWD`, `SECRET`, `CREDENTIAL` or `KEY`; or a `.factory-ci.toml` `env` key does | `ENV A=1 B=2` and `ENV KEY value` forms both parsed; `ARG TARGETARCH` passes |
| `secret-value` | the Dockerfile or `.factory-ci.toml` text matches `dckr_pat_` plus 20 or more of `[A-Za-z0-9_-]`, `gh[pousr]_` plus 30 or more alphanumerics, `github_pat_` plus 30 or more of `[A-Za-z0-9_]`, `AKIA` plus 16 of `[0-9A-Z]`, `AIza` plus 30 or more of `[0-9A-Za-z_-]`, or `-----BEGIN ... PRIVATE KEY-----` | scans comments too |
| `secret-flag` | a `RUN` has `--mount=type=secret` or `--mount=type=ssh`; or any `.factory-ci.toml` command contains `--secret`, `--ssh` or `--build-arg` | `--build-arg` has no safe use here |
| `pipeline-secrets` | any step of `.factory-ci.toml` lists `secrets`, or `.factory-ci.toml` does not parse as a pipeline | |
| `step-image` | any step of `.factory-ci.toml` has an `image` that does not end in `@sha256:` plus 64 hex digits | FR-029 |

## Archive config (`rust-ci-runner/src/archive.rs`)

`read_config(path) -> Result<ImageConfig, &'static str>` reads the config member of a `docker save` tar and returns what it found. It judges nothing: the policy errors below are applied by `publish` (step S6), not by `read_config`.

```rust
#[derive(Debug)]
pub struct ImageConfig {
    pub os: String,
    pub architecture: String,
    pub user: String,
    pub entrypoint: Vec<String>,
    pub cmd: Vec<String>,
}
```

`ImageConfig` derives `Debug` because it holds no secret. A missing field gives an empty `String` or an empty `Vec`.

Policy applied by `publish` to each archive:

| Field | Rule |
|---|---|
| `os` | must be `linux` |
| `architecture` | must equal the platform of the archive name (`amd64` for `image-amd64.tar`, `arm64` for `image-arm64.tar`) |
| `user` | numeric `uid` or `uid:gid`, uid not 0; empty or a name fails |
| `entrypoint`, `cmd` | the first element of `entrypoint`, else of `cmd`, has a basename not in `sh bash dash ash busybox zsh`; both empty is allowed |

Errors of `read_config` (fixed strings): `archive unreadable` for a tar that is missing, truncated, garbage, has no `manifest.json`, names a member that is absent, holds more than one image entry, or has a member over the size limit. Errors of the policy (fixed strings, set by `publish`): `image runs as root`, `image has a shell entrypoint`, `image platform mismatch`.

Size limits are inclusive: a member of exactly 1 MiB (1048576 bytes) passes, one byte more is `archive unreadable`. The Dockerfile limit is the same kind of limit: exactly 256 KiB (262144 bytes) passes, one byte more is `layout`.

## Constants (`rust-ci-runner/src/image.rs`)

| Name | Value |
|---|---|
| `NAMESPACE` | `sudtanj` (also the login name) |
| `PUBLISH_BRANCH` | `main` |
| `TOKEN_NAME` | `DOCKERHUB_TOKEN` |
| `USERNAME_NAME` | `DOCKERHUB_USERNAME` (not a secret to the runner: never in a redactor, no length floor) |
| `HUB_BASE` | `https://hub.docker.com`; passed to `hub::ensure_public` as an argument. The test override `FACTORY_CI_HUB_URL` is read only in debug builds (`#[cfg(debug_assertions)]`), so the release binary never reads it; a base other than `https://` or `http://127.0.0.1` is refused |
| `PLATFORMS` | `[("amd64", "image-amd64.tar"), ("arm64", "image-arm64.tar")]`, in the project directory |
| `ARCHIVE_MAX` | 2 GiB per archive |
| `TAG_HEX` | 12 |
| `CRANE_IMAGE` | `gcr.io/go-containerregistry/crane:debug@sha256:<64 hex>` resolved at implementation |
| `CRANE_SH` | the shell path inside `CRANE_IMAGE`, expected `/busybox/sh`; passed as `--entrypoint` (T002 confirms it) |
| `CRANE_BIN` | the crane binary path inside `CRANE_IMAGE`, expected `/ko-app/crane`; the scripts call it by this absolute path (T002 confirms it) |
| `JOB_DEADLINE` | 40 minutes from the start of the job: one deadline for clone, pulls, pipeline and publish; five minutes of the 45-minute job limit (`factory-ci-tick.yml:215`) stay for the final status and cleanup |
| `REGISTRY` | `index.docker.io` |

`publishes(project, branch) -> bool`: `project` starts `docker/` and `branch == PUBLISH_BRANCH`. `plan.rs` sets row `allow_docker` (with the entry flag) and `secrets_ok` from it; `job.rs` decides the names file and the publish step from it. `image_project(project) -> bool` is only the `docker/` prefix test.

## Tags and the manifest list

For commit `<sha>` and image `<name>`: `index.docker.io/sudtanj/<name>:sha-<first 12 hex of sha>` and `...:latest`. No other tag. Both are the same manifest list.

The list (`verify_index` accepts exactly this shape):

| Field | Rule |
|---|---|
| mediaType | `application/vnd.oci.image.index.v1+json` or `application/vnd.docker.distribution.manifest.list.v2+json` |
| manifests | exactly two entries; one with `platform.os = linux`, `platform.architecture = amd64`; one with `linux` and `arm64`; no other platform, no `unknown/unknown` entry |
| digest | each `digest` field: `sha256:` plus 64 lowercase hex |

A single manifest (a `mediaType` of an image manifest, or no `manifests`) fails. Platform images sit in the repository by digest only until the list names them.

## State flow of one `main` run of a `docker/<image>` project

```
clone -> check ok -> pipeline (two builds, two archives; a failed step or a timeout of an image row ends `error`, not `failure`, FR-007) -> publish:
   token present?            -- no --> error "publish refused: no token"
   login name present and == "sudtanj"?  -- empty --> error "publish refused: no login name"; other --> error "publish refused: login is not the namespace"
   both archives ok?         -- no --> error "publish refused: archive missing" / "archive too large" / "archive unreadable"
   archive config ok?        -- no --> error "publish refused: image runs as root" / "image has a shell entrypoint" / "image platform mismatch"
   tip of main readable?     -- no --> error "tip of main unreadable" (nothing pushed)
   teardown (cleanup_run), run_left?  -- left --> error "publish refused: sidecar still running"
   Docker Hub (runner memory, no child): login; probe repository;
      missing -> create public -> probe shows public; public -> nothing; private -> error "publish refused: repository is private"
      (login rejected -> "registry refused the token"; probe or create 401/403 -> "docker hub api refused repository access" / "... creation"; other -> "docker hub api error")
   move archives into a fresh directory (its path must not contain ":", else "publish refused: archive unreadable")
   every step above and below checks the shared JOB_DEADLINE: too little time left -> error "publish refused: time ran out"
   container A (token on stdin):
      sha tag exists?        -- yes -> reuse its list, push nothing, compare no digest
                             -- no  -> push each platform image by digest, index append -> sha tag (one write)
                                       (401 -> error "registry refused the token"; other -> error "push failed")
      verify_index           -- bad -> error "image index incomplete" (an existing list is never overwritten)
   tip == sha (read before A and again before B)?
                             -- no  -> success "published sha-<12>, not latest"
                             -- 2nd read fails -> error "tip of main unreadable" (sha tag stays)
                             -- yes -> container B (token on stdin): crane tag latest (fail -> error "push failed")
                                       -> success "published latest and sha-<12>"
```

Any other project, or any ref but `main`: publish is never called, the names file is empty, no credential is exported to the runner.

## Status texts

All fixed, at most 140 characters, no content from the repo.

| State | Text |
|---|---|
| `failure` | `check failed: <ids>` |
| `failure` | `step <n> failed: <reason>` for a project that is not an image project (existing); for an image row the same fixed line is posted as `error` (FR-007) |
| `success` | `published latest and sha-<12>` |
| `success` | `published sha-<12>, not latest` |
| `error` | `publish refused: no token`, `publish refused: no login name`, `publish refused: login is not the namespace` |
| `error` | `publish refused: repository is private` |
| `error` | `docker hub api refused repository access`, `docker hub api refused repository creation`, `docker hub api error` |
| `error` | `publish refused: archive missing`, `publish refused: archive too large`, `publish refused: archive unreadable` |
| `error` | `publish refused: image runs as root`, `publish refused: image has a shell entrypoint`, `publish refused: image platform mismatch` |
| `error` | `publish refused: sidecar still running`, `publish refused: time ran out` |
| `error` | `registry refused the token` |
| `error` | `image index incomplete` |
| `error` | `push failed` |
| `error` | `tip of main unreadable` |

## Hub repository (`rust-ci-runner/src/hub.rs`)

The repository `sudtanj/<image>` on Docker Hub, as the runner sees it.

| Field | Rule |
|---|---|
| state | `Missing` (404 on the probe after a successful login), `Public` (200, `is_private` false), `Private` (200, `is_private` true) |
| namespace | the constant `NAMESPACE` |
| name | the image folder name, which already passes `image::name_ok` |
| visibility on create | `is_private: false`; verified by a second probe; the runner never changes an existing repository |

Only the `access_token` field of the login answer and the `is_private` field of the probe answer are read; nothing else from a response is kept, logged or shown.
