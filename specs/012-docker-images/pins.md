# Pins for feature 012

Read on 2026-11-28 from the registry v2 APIs with plain HTTPS reads (no pull). Each digest is the digest of the image index, the value `Docker-Content-Digest` returned for the tag. Platforms come from the index entries; `unknown/unknown` entries are attestation manifests and are not listed. `docker buildx imagetools inspect` was not run (no Docker on the agent VM); the index JSON carries the same platform list.

| Use | Source tag | Digest | Platforms |
| --- | --- | --- | --- |
| Go builder in `docker/hello-static/Dockerfile` | `mirror.gcr.io/library/golang:1.27.4` | `sha256:5f1b98fa02fe396ebce2a0227a0e08309e11792915b9dd2e1f71053d7e7e6666` | `linux/amd64`, `linux/arm`, `linux/arm64`, `linux/386`, `linux/ppc64le`, `linux/riscv64`, `linux/s390x`, `windows/amd64` |
| Final base in `docker/hello-static/Dockerfile` | `gcr.io/distroless/static:latest` | `sha256:4c2cd1dee442c9a99924a378c7c126fac8fef14771579ef567c3f2bdd7a5122a` | `linux/amd64`, `linux/arm64`, `linux/arm`, `linux/s390x`, `linux/ppc64le`, `linux/riscv64` |
| Step image of every image folder (`image::STEP_IMAGE`) | `docker:27.5.1-cli` | `sha256:851f91d241214e7c6db86513b270d58776379aacc5eb9c4a87e5b47115e3065c` | `linux/amd64`, `linux/arm`, `linux/arm64` |
| Publish containers (`image::CRANE_IMAGE`) | `gcr.io/go-containerregistry/crane:debug` | `sha256:e78770b31258a3846f878036d9c1f63fbe4c871f9f56990bf77fd95c013e3c1b` | `linux/amd64`, `linux/arm64`, `linux/arm`, `linux/s390x`, `linux/ppc64le`, `linux/riscv64` |

Notes:

- Go 1.27.4 is the newest 1.27 release on the mirror; 1.27 and 1.26 are the two supported Go lines on the day of the read. `docker/hello-static/go.mod` carries `go 1.27.4`, and the Dockerfile sets `GOTOOLCHAIN=local`.
- `golang`, `distroless/static` and `docker:27.5.1-cli` each list `linux/amd64` and `linux/arm64` (FR-028).
- The `crane:debug` digest equals the one already in `image.rs` (`CRANE_IMAGE`). It is **unverified**: the existence of `/busybox/sh` and `/ko-app/crane` inside it is not checked here. The owner confirms both paths in T002.
- `crane:debug` is a moving tag. Its digest is fixed here; refresh it with the routine in `docker/README.md` when needed.
