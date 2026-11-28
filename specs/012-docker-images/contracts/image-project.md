# Contract: an image folder

```
docker/<image>/
  Dockerfile
  README.md
  .factory-ci.toml
  <sources the Dockerfile copies>
```

## `.factory-ci.toml` (one step, no check step, no secrets)

The runner runs the check itself before and inside publish, so the file only builds and saves, once per platform.

```toml
[steps.build]
image = "docker:27.5.1-cli@sha256:<64 hex>"
docker = true
commands = [
  "docker build --platform linux/amd64 -t image-amd64 .",
  "docker save -o image-amd64.tar image-amd64",
  "docker build --platform linux/arm64 -t image-arm64 .",
  "docker save -o image-arm64.tar image-arm64",
]
timeout = 1800
```

- The step writes `image-amd64.tar` and `image-arm64.tar` in the project directory (`/workspace` inside the step). The runner pushes those two files and no others.
- `docker = true` needs `allow_docker = true` on the watch entry and `FACTORY_CI_DIND_IMAGE`.
- A step image pin is the convention. The check does not enforce it (risk R-A).
- No emulation: BuildKit sets `BUILDPLATFORM` to the VM's `linux/amd64`; the `--platform linux/arm64` build only changes `TARGETARCH` and the platform of the final stage.

## `Dockerfile` for `hello-static` (cross-compiled Go, no emulation)

Shape, not final text. The engineer resolves the digests that are not already in the repo and records the source tag in a comment. Both are index digests (FR-028): `docker buildx imagetools inspect <ref>@<digest>` must list `linux/amd64` and `linux/arm64`.

```dockerfile
FROM --platform=$BUILDPLATFORM golang:1.23@sha256:<index digest> AS build
ARG TARGETARCH
WORKDIR /src
COPY go.mod hello.go ./
RUN CGO_ENABLED=0 GOOS=linux GOARCH=$TARGETARCH go build -trimpath -o /hello .

FROM gcr.io/distroless/static@sha256:<index digest>
COPY --from=build /hello /hello
USER 65532:65532
ENTRYPOINT ["/hello"]
```

It passes every rule of `data-model.md`: every `FROM` pinned, the one `RUN` sits in a `--platform=$BUILDPLATFORM` stage (`emulation`), the final base is distroless `static`, no `RUN` or shell in the final stage, numeric `USER`, exec-form `ENTRYPOINT`, no credential names (`TARGETARCH` is not one). `hello.go` prints one fixed line and `go.mod` names a module with no dependency.

## `README.md`

What the image is, how to pull it (`docker pull sudtanj/<image>:latest`, both architectures), the tags, and the rule that the folder is the only place to change. No secrets.

## Failure examples (fixtures for tests)

One fixture Dockerfile per check id, each the passing file with one change (`emulation`: drop `--platform=$BUILDPLATFORM` from the builder). The fixture set lives in `rust-ci-runner/tests/fixtures/image/` as plain files. Token fixtures use synthetic strings built for the test (`dckr_pat_` plus 24 `A`), never a live credential; the canary string for leak searches is `canary-Zq81-value-key-0123456789`.
