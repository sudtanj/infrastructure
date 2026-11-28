# Contract: an image folder

```
docker/<image>/
  Dockerfile
  README.md
  .factory-ci.toml
  <sources the Dockerfile copies>
```

The runner creates the public Docker Hub repository `sudtanj/<image>` itself when it is missing (spec FR-016); the owner does nothing per image.

## `.factory-ci.toml` (one step, no check step, no secrets)

The runner runs the check itself before and inside publish, so the file only builds and saves, once per platform. The step image is the one pinned `docker:27.5.1-cli` digest of `pins.md` (`step-template`; `step-image` for the pin shape), and `env` is absent.

```toml
[steps.build]
image = "docker:27.5.1-cli@sha256:851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c"
docker = true
commands = [
  "docker build --platform linux/amd64 -t image-amd64 .",
  "docker save -o image-amd64.tar image-amd64",
  "docker build --platform linux/arm64 -t image-arm64 .",
  "docker save -o image-arm64.tar image-arm64",
  "chmod 0644 image-amd64.tar image-arm64.tar",
]
timeout = 1800
```

- The step writes `image-amd64.tar` and `image-arm64.tar` in the project directory (`/workspace` inside the step). The runner moves those two files into a fresh directory and pushes them; no other file.
- `docker = true` needs `allow_docker` on the watch entry and `FACTORY_CI_DIND_IMAGE`; only a `docker/<image>` row on `main` gets it.
- The check refuses any other step, command or order (`step-template`): free shell could build another Dockerfile or `docker save` any image. It also walks the whole folder (`layout`, `secret-file`, `secret-value`), so a credential in a source file, a `.env` or a `*.pem` fails; the two archives at the top are the only files it skips.
- No emulation: BuildKit sets `BUILDPLATFORM` to the VM's `linux/amd64`; the `--platform linux/arm64` build only changes `TARGETARCH` and the platform of the final stage.

## `Dockerfile` for `hello-static` (cross-compiled Go, no emulation)

Shape, not final text. The engineer resolves the digests and records the source tags in `pins.md` and in a comment. Every `FROM` is an index digest (FR-028): `docker buildx imagetools inspect <ref>@<digest>` must list `linux/amd64` and `linux/arm64`. The builder comes from the Google mirror of the Docker Hub official image, by digest, so the sidecar's pull does not meet the anonymous Docker Hub limit (spec A13). The Go version is one that still gets security fixes on the day the file is written; `go.mod` carries the same version.

```dockerfile
FROM --platform=$BUILDPLATFORM mirror.gcr.io/library/golang:<supported>@sha256:<index digest> AS build
ARG TARGETARCH
ENV GOTOOLCHAIN=local CGO_ENABLED=0 GOOS=linux
WORKDIR /src
COPY go.mod hello.go ./
RUN GOARCH=$TARGETARCH go build -trimpath -o /hello .

FROM gcr.io/distroless/static@sha256:<index digest>
COPY --from=build /hello /hello
USER 65532:65532
ENTRYPOINT ["/hello"]
```

It passes every rule of `data-model.md`: only allowlisted instructions, every `FROM` pinned, the one `RUN` sits in a `--platform=$BUILDPLATFORM` stage (`emulation`), `COPY --from=build` names an earlier stage, the final base is distroless `static`, no `RUN` in the final stage, numeric `USER`, exec-form `ENTRYPOINT`, no credential names (`TARGETARCH` is not one). `hello.go` prints one fixed line and `go.mod` names a module with no dependency.

## `README.md`

What the image is, how to pull it (`docker pull sudtanj/<image>:latest`, both architectures), the tags (`latest`, `sha-<12 hex>`), and the rule that the folder is the only place to change. No secrets.

## `docker/README.md`

How to add an image (three files; the runner creates the public repository), and the refresh routine: when a base gets a security fix, the owner resolves the new index digest (`docker buildx imagetools inspect`), edits the `FROM` lines and the step image, and merges; the commit rebuilds and republishes. Pins carry their source tag in a comment.

## Failure examples (fixtures for tests)

One fixture Dockerfile per check id and per bypass of S1, each the passing file with one change. The fixture set lives in `rust-ci-runner/tests/fixtures/image/` as plain files. Token fixtures use synthetic strings built for the test (`dckr_pat_` plus 24 `A`), never a live credential; the canary string for leak searches is `canary-Zq81-value-key-0123456789`.
