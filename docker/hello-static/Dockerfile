# Pins (specs/012-docker-images/pins.md), read 2026-11-28. Refresh routine: docker/README.md.
# golang:1.27.4 on mirror.gcr.io, index digest; the mirror avoids the anonymous Docker Hub pull limit.
FROM --platform=$BUILDPLATFORM mirror.gcr.io/library/golang:1.27.4@sha256:5f1b98fa02fe396ebce2a0227a0e08309e11792915b9dd2e1f71053d7e7e6666 AS build
ARG TARGETARCH
ENV GOTOOLCHAIN=local CGO_ENABLED=0 GOOS=linux
WORKDIR /src
COPY go.mod hello.go ./
RUN GOARCH=$TARGETARCH go build -trimpath -o /hello .

# gcr.io/distroless/static:latest, index digest
FROM gcr.io/distroless/static@sha256:4c2cd1dee442c9a99924a378c7c126fac8fef14771579ef567c3f2bdd7a5122a
COPY --from=build /hello /hello
USER 65532:65532
ENTRYPOINT ["/hello"]
