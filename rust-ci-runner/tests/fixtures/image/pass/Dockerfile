# Fixture: the passing shape of contracts/image-project.md. Digests are fake.
FROM --platform=$BUILDPLATFORM mirror.gcr.io/library/golang:1.24@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa AS build
ARG TARGETARCH
ENV GOTOOLCHAIN=local CGO_ENABLED=0 GOOS=linux
WORKDIR /src
COPY go.mod hello.go ./
RUN GOARCH=$TARGETARCH go build -trimpath -o /hello .

FROM gcr.io/distroless/static@sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
COPY --from=build /hello /hello
USER 65532:65532
ENTRYPOINT ["/hello"]
