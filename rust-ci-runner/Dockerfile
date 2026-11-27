# Image of the runner for the manual CI job (feature 008): the runner, the docker client, git,
# CA certificates and the pinned bws. Built by .github/workflows/rust-ci-runner-release.yml from
# binaries staged in amd64/ and arm64/ next to this file.
# Base: alpine 3.23, pinned by the multi-arch index digest (3.21 is end of life).
ARG TARGETARCH

# Throwaway stage: the apk unzip is the full one (busybox unzip lacks -Z) and never reaches the final image.
FROM alpine:3.23@sha256:469c9ea410b9215eaee7976ba6c0639039b444be8e5d638f7abbada189971eb2 AS unzip
RUN apk add --no-cache unzip

# One stage per architecture; ADD --checksum fails the build when the zip differs from its pin.
FROM unzip AS bws-amd64
ADD --checksum=sha256:f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6 https://github.com/bitwarden/sdk-sm/releases/download/bws-v2.1.0/bws-x86_64-unknown-linux-musl-2.1.0.zip /bws.zip
RUN unzip -p /bws.zip bws > /bws && chmod +x /bws

FROM unzip AS bws-arm64
ADD --checksum=sha256:eb0f1ae61d1c3b74244d2841233276e05c77e8be4da197ed90fc6248387005e1 https://github.com/bitwarden/sdk-sm/releases/download/bws-v2.1.0/bws-aarch64-unknown-linux-musl-2.1.0.zip /bws.zip
RUN unzip -p /bws.zip bws > /bws && chmod +x /bws

FROM bws-${TARGETARCH} AS bws

FROM alpine:3.23@sha256:469c9ea410b9215eaee7976ba6c0639039b444be8e5d638f7abbada189971eb2
# A global ARG is not visible inside a stage.
ARG TARGETARCH
LABEL org.opencontainers.image.source=https://github.com/sudtanj/infrastructure
# git: the ci command fetches the branch. docker-cli: it starts the step containers through the
# host socket. ca-certificates: https to GitHub, Bitwarden and the log store.
RUN apk add --no-cache git docker-cli ca-certificates
COPY --from=bws /bws /usr/local/bin/bws
# The binary stays at /rust-ci-runner: the tick's fetch step runs docker cp "$id:/rust-ci-runner".
COPY ${TARGETARCH}/rust-ci-runner /rust-ci-runner
USER 65532:65532
ENTRYPOINT ["/rust-ci-runner"]
