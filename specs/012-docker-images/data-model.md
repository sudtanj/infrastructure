# Data model: Public Docker images from `docker/`

No database. The entities are files, one constant set and the status texts.

## Image project

A folder `docker/<image>/` in `sudtanj/infrastructure`.

| Field | Rule | Check id |
|---|---|---|
| name | `<image>`: `^[a-z0-9]+(-[a-z0-9]+)*$`, length 2 to 64 | `name` (plan posts `error` "invalid project folder name", no job) |
| required files | `Dockerfile`, `README.md` and `.factory-ci.toml`, each a regular file, not a symlink, in the folder | `layout` |
| project path | `docker/<image>`, status context `factory-ci/docker/<image>` | none |

A folder with `.factory-ci.toml` or `Dockerfile` is a project. A folder with neither is ignored.

## Check rules (`rust-ci-runner/src/image.rs`)

`check(dir) -> Vec<&'static str>` returns the ids that fail, sorted, once each. The public status shows `check failed: ` plus at most three ids joined by `, `. It never shows file content.

The parser works on instructions. It joins `\` continuations (a `# escape=` directive is refused), strips `\r`, skips comment lines and empty lines, and skips heredoc bodies (`<<WORD`, `<<-WORD`, `<<"WORD"`) up to the terminator line. Instruction keywords are case-insensitive.

| Id | Fails when | Edge answers |
|---|---|---|
| `layout` | a required file is missing, a symlink or not a file; the Dockerfile is over 256 KB or not UTF-8 | CRLF or BOM are not a layout problem |
| `directive` | a parser directive `syntax=` or `escape=` is present | the frontend image would be unpinned |
| `from-unpinned` | a `FROM` reference (after `--flag=...` options) is not `scratch`, not a stage name defined by an earlier `AS`, and does not end in `@sha256:` plus 64 hex digits | `FROM alpine AS alpine`: the name counts only after its own line; stage names compare case-insensitively; `FROM --platform=$BUILDPLATFORM x@sha256:..` passes because the flag is not the reference. For a multi-arch base the digest must be the index digest (FR-028); the check cannot tell offline, the build of the other platform fails if it is not |
| `from-arg` | a `FROM` reference contains `$` | with or without an `ARG` default |
| `emulation` | a stage that has a `RUN` has no `--platform=$BUILDPLATFORM` (or `${BUILDPLATFORM}`) on its `FROM` | FR-027. Stage names and the final stage follow the same rule; the final stage fails `final-run` first |
| `final-base` | the final stage, followed back through `FROM <stage>` links, does not end at `scratch` or a reference starting `gcr.io/distroless/` whose name and tag contain no `debug` | chain depth limit 20 |
| `final-run` | any stage on the final chain has `RUN`, or the final chain has `SHELL` | |
| `final-shell` | the final stage has a shell-form `CMD` or `ENTRYPOINT` (not starting with `[`), or a `COPY` or `ADD` whose source or destination basename is `sh bash dash ash busybox apk apt apt-get dpkg yum dnf microdnf zypper pip npm` | name heuristic, R5 |
| `final-user` | the last `USER` of the final chain is absent, not `^[0-9]+(:[0-9]+)?$`, or its uid is 0 | `USER 1000:0` passes (gid 0 is not in the spec); `USER $X` fails as non-numeric |
| `secret-name` | an `ARG` or `ENV` name, any stage, contains (case-insensitive) `TOKEN`, `PASSWORD`, `PASSWD`, `SECRET`, `CREDENTIAL` or `KEY`; or a `.factory-ci.toml` `env` key does | `ENV A=1 B=2` and `ENV KEY value` forms both parsed; `ARG TARGETARCH` passes |
| `secret-value` | the Dockerfile or `.factory-ci.toml` text matches `dckr_pat_` plus 20 or more of `[A-Za-z0-9_-]`, `gh[pousr]_` plus 30 or more alphanumerics, `github_pat_` plus 30 or more of `[A-Za-z0-9_]`, `AKIA` plus 16 of `[0-9A-Z]`, `AIza` plus 30 or more of `[0-9A-Za-z_-]`, or `-----BEGIN ... PRIVATE KEY-----` | scans comments too |
| `secret-flag` | a `RUN` has `--mount=type=secret` or `--mount=type=ssh`; or any `.factory-ci.toml` command contains `--secret`, `--ssh` or `--build-arg` | `--build-arg` has no safe use here |
| `pipeline-secrets` | any step of `.factory-ci.toml` lists `secrets`, or `.factory-ci.toml` does not parse as a pipeline | |

Not enforced (open risk R-A): the pipeline step `image` pin, and that the build step really builds this Dockerfile. A step on `main` can push any archives it writes. `main` merges are the owner's.

## Constants (`rust-ci-runner/src/image.rs`)

| Name | Value |
|---|---|
| `NAMESPACE` | `sudtanj` (also the login name) |
| `PUBLISH_BRANCH` | `main` |
| `TOKEN_NAME` | `DOCKERHUB_TOKEN` |
| `PLATFORMS` | `[("amd64", "image-amd64.tar"), ("arm64", "image-arm64.tar")]`, in the project directory |
| `ARCHIVE_MAX` | 2 GiB per archive |
| `CRANE_IMAGE` | `gcr.io/go-containerregistry/crane:debug@sha256:<64 hex>` resolved at implementation |
| `NOT_PUBLISHED` | `built and checked, not published` |
| `REGISTRY` | `index.docker.io` |

`publishes(project, branch) -> bool`: `project` starts `docker/` and `branch == PUBLISH_BRANCH`. `plan.rs` sets `secrets_ok` from it; `job.rs` decides the names file and the publish step from it. `image_project(project) -> bool` is only the `docker/` prefix test.

## Tags and the manifest list

For commit `<sha>` and image `<name>`: `index.docker.io/sudtanj/<name>:sha-<first 7 hex of sha>` and `...:latest`. No other tag. Both are the same manifest list.

The list (`verify_index` accepts exactly this shape):

| Field | Rule |
|---|---|
| mediaType | `application/vnd.oci.image.index.v1+json` or `application/vnd.docker.distribution.manifest.list.v2+json` |
| manifests | exactly two entries; one with `platform.os = linux`, `platform.architecture = amd64`; one with `linux` and `arm64`; no `variant` constraint, no other platform, no `unknown/unknown` entry |
| digest | each `digest` field: `sha256:` plus 64 lowercase hex |

A single manifest (`mediaType` of an image manifest, or no `manifests`) fails. Platform images sit in the repository by digest only, until the list names them.

## State flow of one `main` run

```
clone -> check ok -> pipeline ok (two builds, two archives) -> publish:
   token present?        -- no --> error "publish refused: no token"
   both archives ok?     -- no --> error "publish refused: archive missing" / "archive too large"
   tip of main readable? -- no --> error "tip of main unreadable" (nothing pushed)
   container A:
      sha tag exists?    -- yes -> reuse its manifest
                         -- no --> for each arch: digest, push by digest, config check
                                    (401 -> error "registry refused the token";
                                     mismatch -> error "image platform mismatch";
                                     other -> error "push failed")
                                    index append -> sha tag (one write)
      verify_index       -- bad -> error "image index incomplete"
   tip == sha (read twice, before A and before B)?
                         -- no  -> success "published sha-<short>, not latest"
                         -- 2nd read fails -> error "tip of main unreadable" (sha tag stays)
                         -- yes -> container B: crane tag latest (fail -> error "push failed")
                                   -> success "published latest and sha-<short>"
```

A branch other than `main`: clone -> check -> pipeline -> success `built and checked, not published`. No token read, no crane container.

## Status texts

All fixed, at most 140 characters, no content from the repo.

| State | Text |
|---|---|
| `failure` | `check failed: <ids>` |
| `failure` | `step <n> failed: <reason>` (existing) |
| `success` | `built and checked, not published` |
| `success` | `published latest and sha-<short>` |
| `success` | `published sha-<short>, not latest` |
| `error` | `publish refused: no token` |
| `error` | `publish refused: archive missing` / `publish refused: archive too large` |
| `error` | `registry refused the token` |
| `error` | `image platform mismatch` |
| `error` | `image index incomplete` |
| `error` | `push failed` |
| `error` | `tip of main unreadable` |

## Walk rule (`walk.rs`)

`decide(..., publishes: bool)`. When `publishes` is true, a status whose description starts with `NOT_PUBLISHED` is treated as not done at the tip (`Run`, `retry` false) and as `Run(prev = that ancestor)` on an ancestor. When false, nothing changes.
