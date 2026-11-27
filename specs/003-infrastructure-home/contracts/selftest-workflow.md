# Contract: selftest and release workflows

Covers FR-003, FR-006 to FR-009, FR-020, FR-021, SC-001. Both workflows run cargo directly. That is the bootstrap exception: no runner exists yet to build the first one. Each file's header comment says so.

## `factory-ci-selftest.yml` jobs (after this feature)

| Job | Runs | Notes |
| --- | --- | --- |
| `pipeline` | `rustup toolchain install`, `cargo fmt --check`, `cargo clippy --locked -- -D warnings`, `cargo test --locked`, `cargo build --release --locked`, the fixture pipeline, the leftover-container check | `ubuntu-24.04`. Gets the fmt, clippy and test steps; the old monorepo `ci.yml` is gone. |
| `own-pipelines` | the built runner on `rust-ci-runner` and on `.` | Reuses the binary built in `pipeline` (same job, extra step) unless the engineer finds a shorter way. |
| `list` | `plan --check` on the watch list | `--locked` build. |
| `workflow` | lint of every workflow in the directory with `FACTORY_CI_LINT_STRICT=1`; download of the `bws` archive and compare with `BWS_SHA256` | The hash check needs network only. |
| `shell` | the three shell test scripts, lint self-test once | The docs test is new. |
| `bootstrap` | `cargo generate-lockfile`, `cargo fmt`, `git diff > fmt.patch`, upload | Temporary: removed before merge (T026). |

Triggers stay `pull_request` on `rust-ci-runner/**`, `.github/workflows/factory-ci*.yml`, `.github/factory-ci-repos.toml` and `.github/scripts/**`. Add `.factory-ci.toml`, `README.md` and the release workflow path so a change to a pipeline file, the README or the release workflow runs the checks. `permissions: contents: read`. No secret is read.

## `rust-ci-runner-release.yml`

- Three `runs-on: ubuntu-latest` become `ubuntu-24.04`.
- Every cargo call gets `--locked`.
- The step that prints the digest keeps its line and adds one: "Make the GHCR package public once, then set RUNNER_IMAGE_DIGEST".
