# Contract: pipeline files and the watch list

Covers FR-002, FR-017 to FR-021. Format and keys: `specs/002-ci-runner/contracts/` and `rust-ci-runner/README.md`.

## Layout rule for any watched repo

- A root `.factory-ci.toml` holds repo-wide checks (anything that spans folders). The root project owns every file that sits in no top-level project folder.
- Each top-level folder with its own `.factory-ci.toml` is one project and owns its folder.
- Statuses are `factory-ci` (root) and `factory-ci/<folder>`.
- Steps are build, lint and test. No Actions workflow builds or tests the repo.

factory-monorepo follows this rule. Its files live in that repo and are written under its own spec; the root file is on its PR 2 and not on `main` yet. A watched repo with no `.factory-ci.toml` is skipped by the plan (`projects::discover` returns no project): no status, no job, no error. A step that needs a secret lists the name in `secrets`; the value lives in Bitwarden, and the watch entry needs `id` for owner-scoped keys.

## `.github/factory-ci-repos.toml` (after this feature)

```toml
[[watch]]
repo = "sudtanj/factory-monorepo"

[[watch]]
repo = "sudtanj/infrastructure"
```

Keep the existing header comment. Both entries use defaults: all branches, no shared names, no `allow_docker`. The monorepo is private, so a tick needs log credentials or refuses the repo (002 FR-049).

## This repo's pipeline files

`rust-ci-runner/.factory-ci.toml`:

```toml
[steps.lint]
image = "rust:1.90"
commands = ["rustup component add clippy rustfmt", "cargo fmt --check", "cargo clippy --locked -- -D warnings"]

[steps.test]
image = "rust:1.90"
commands = ["cargo test --locked"]   # tests that need a container engine are #[ignore]d (T010)
```

`.factory-ci.toml` (root):

```toml
[steps.shell]
image = "ubuntu:24.04"
commands = ["apt-get update -qq", "apt-get install -y -qq --no-install-recommends git jq curl unzip openssl ca-certificates", "git config --global --add safe.directory '*'", ".github/scripts/test-factory-ci-secrets.sh", ".github/scripts/test-factory-ci-hardening.sh", ".github/scripts/test-factory-ci-docs.sh"]

[steps.lint-workflows]
image = "ubuntu:24.04"
commands = ["apt-get update -qq", "apt-get install -y -qq --no-install-recommends git jq curl unzip openssl ca-certificates", "git config --global --add safe.directory '*'", ".github/scripts/factory-ci-lint-workflow.sh --self-test .github/workflows/factory-ci-tick.yml .github/workflows/rust-ci-runner-release.yml"]
```

The engineer fixes the package list and a `timeout` after the first `own-pipelines` run. The checkout is mounted with another owner, hence `safe.directory`. If a shell test needs the host (`sudo`, `iptables`, a real Docker), that test stays in the selftest `shell` job only, and the pipeline file says why in a comment.

## Bootstrap exception

Only `factory-ci-selftest.yml`, `rust-ci-runner-release.yml` and, until T028 deletes it, `factory-ci-bootstrap.yml` run `cargo`. Rule `no-cargo` of the lint fails any other workflow that has a `cargo ` command.
