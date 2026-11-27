# Contract: selftest, bootstrap and release workflows

Covers FR-003, FR-006 to FR-009, FR-020, FR-021, FR-028, FR-029, FR-033, SC-001. The selftest, the bootstrap workflow and the release workflow run cargo directly. That is the bootstrap exception: no runner exists yet to build the first one. Each file's header comment says so.

## `factory-ci-selftest.yml` (after this feature)

Triggers: `pull_request` (no `paths:`) and `workflow_dispatch`. `permissions: contents: read`. `concurrency: {group: selftest-${{ github.ref }}, cancel-in-progress: true}`. Every checkout has `persist-credentials: false`. The file never contains `secrets.`, `environment:`, `workflow_run` or `id-token`, and no `upload-artifact`. All jobs `ubuntu-24.04`. Every job that runs cargo first runs `rustup toolchain install`.

| Job | Runs | Notes |
| --- | --- | --- |
| `check` | `cargo check --locked --all-targets` | Fast compile errors (D17). |
| `pipeline` | `cargo fmt --check`, `cargo clippy --locked -- -D warnings`, `cargo test --locked -- --include-ignored`, `cargo build --release --locked`, the fixture pipeline, the leftover-container check | Host job: Docker exists, so ignored engine tests run (FR-030). |
| `build-release` | the release matrix (both musl targets, same packages and `CC_*`, `CARGO_TARGET_*_LINKER` environment) with `make build`, stage the binaries, `docker buildx build` with `push: false` | No login, no `packages` permission (D14). |
| `own-pipelines` | the built runner on `rust-ci-runner` and on `.` | Own job; sets `FACTORY_CI_SHA`, `FACTORY_CI_EVENT=push`, `FACTORY_CI_BRANCH=selftest`; separate `CARGO_TARGET_DIR` (D8). |
| `list` | `plan --check` on the watch list | `--locked` build. |
| `workflow` | lint of the whole `.github/workflows/` directory with `FACTORY_CI_LINT_STRICT=1`; download of the `bws` archive and compare with `BWS_SHA256` | The hash check needs network only. |
| `shell` | the shell test scripts including the docs test, lint self-test once | The docs test is new. |

The required checks on `main` are these jobs (D3). The runner's `factory-ci*` statuses are informational.

## `factory-ci-bootstrap.yml` (temporary, deleted by T028)

`pull_request`, one job guarded by `if: github.event.pull_request.head.repo.full_name == github.repository`, `permissions: contents: read`, no Environment, no secret. Steps and artifact rules are in `research.md` D4. It is named in the `no-cargo` allowlist of the lint only while it exists.

## `rust-ci-runner-release.yml`

- Three `runs-on: ubuntu-latest` become `ubuntu-24.04`.
- `rustup toolchain install` runs before `make build`; every cargo call gets `--locked`.
- `publish` gets `environment: factory-ci-release`.
- The step that prints the digest keeps its line and adds one: "Make the GHCR package public once, then set RUNNER_IMAGE_DIGEST".
- No `workflow_dispatch`.
