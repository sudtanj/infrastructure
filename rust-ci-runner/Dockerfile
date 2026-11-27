# Distribution image: only the binary. Built by .github/workflows/rust-ci-runner-release.yml
# from binaries staged in amd64/ and arm64/ next to this file.
FROM scratch
ARG TARGETARCH
COPY ${TARGETARCH}/rust-ci-runner /rust-ci-runner
ENTRYPOINT ["/rust-ci-runner"]
