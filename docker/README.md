# docker

Each folder here becomes a public multi-arch image `sudtanj/<folder>` on Docker Hub when it lands on `main`.

## Add an image

Create `docker/<name>/` with three files plus the sources the Dockerfile copies:

- `Dockerfile`: every `FROM` pinned by index digest, no `RUN` in an emulated stage, a numeric non-root `USER`, an exec-form `ENTRYPOINT`, a `distroless/static` final base. Copy `hello-static/Dockerfile`.
- `.factory-ci.toml`: copy `hello-static/.factory-ci.toml` unchanged. The runner accepts one `build` step with exactly those commands and that step image.
- `README.md`: what the image is and how to pull it.

`<name>` is 2 to 64 characters of lower-case letters, digits and single hyphens. The runner creates the public Docker Hub repository when it is missing. Nothing in the folder may hold a secret: the check refuses credential file names and token-shaped text in any file. The names are `.env`, every `.env.*` file (including `.env.example` and `.env.sample`), `.npmrc`, `.netrc`, `id_rsa`, `id_dsa`, `id_ed25519`, `id_ecdsa`, and files ending in `.pem`, `.key`, `.p12` or `.pfx`. The names `image-amd64.tar` and `image-arm64.tar` are reserved for the build output and fail the check before the build.

The final stage must start from `scratch` or from `gcr.io/distroless/static`, `gcr.io/distroless/static-debian12` or `gcr.io/distroless/base-nossl` (any tag except one with `debug`). Other distroless repositories fail.

Only public repositories publish. A folder in a private watched repository gets no Docker Hub credentials and publishes nothing.

Open a pull request. The `rust-ci-runner` tests run the static check over every folder here. After the merge, the tick on `main` checks the folder again, builds both platforms, pushes `latest` and `sha-<12 hex>`, and posts the status.

## Refresh the pins

When a base image gets a security fix:

1. Read the new index digest, for example `docker buildx imagetools inspect gcr.io/distroless/static:latest`. It must list `linux/amd64` and `linux/arm64`.
2. Edit the digest in the `FROM` lines and its source-tag comment.
3. If the step image changes, edit `STEP_IMAGE` in `rust-ci-runner/src/image.rs` and every `.factory-ci.toml` under `docker/`, `rust-ci-runner/tests/fixtures/image/pass/.factory-ci.toml`, `rust-ci-runner/tests/job_publish.rs` and `specs/012-docker-images/contracts/image-project.md` in the same pull request, and record the new digest in `specs/012-docker-images/pins.md`.
4. Merge. The commit rebuilds and republishes the image.
