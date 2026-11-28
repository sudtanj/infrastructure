# Quickstart: validate step secrets

Nothing here runs on a laptop or agent VM. Tests run in the `rust-ci-runner` pipeline and the release in `rust-ci-runner-release.yml`. See [contracts/ci-secrets.md](contracts/ci-secrets.md) for the rules.

## 1. Tests (after T001 to T010 are on `main`)

Start `factory-monorepo-ci` with `repo = sudtanj/infrastructure`, `ref = main`, `project = rust-ci-runner`. Steps `unit` and `integration` must pass, with the new tests listed in `plan.md` among them. Expected: `[runner] pipeline passed`.

## 2. Release

Push tag `rust-ci-runner-v0.3.0` on that commit. Expected: the release workflow ends green, its smoke step prints `repository or ref not in the watch list` and exits 1, and the summary shows a digest. Check it with `docker pull ghcr.io/sudtanj/rust-ci-runner@<digest>`.

## 3. Pin and watch entry

Commit the `RUNNER_IMAGE` line (`0.3.0@sha256:<digest>`), then the `secrets` names in the `sudtanj/factory-monorepo` entry. Run `.github/scripts/test-factory-monorepo-ci.sh`; it passes.

## 4. Refusal checks (no container may start)

With the three Bitwarden keys absent, start `factory-monorepo-ci` for `sudtanj/factory-monorepo`. Expected stderr: `[runner] step secret unavailable`, exit 1, no `[<step>] started` line. In Grafana, step `ci` shows the name and `expected 1, found 0`.

Remove one name from the entry (or list an extra one in the pipeline). Expected: `[runner] step secret not allowed`.

## 5. The real run (SC-001)

Create `E2E_BASE_URL`, `E2E_USER`, `E2E_PASSWORD` in the Bitwarden project the machine account reads, with the values the Playwright project needs. Start `factory-monorepo-ci` on `main`. Expected: only grammar lines in the Actions log, `[runner] pipeline passed`, and in Loki the three values appear as `***`. Search the Actions log for each value by eye or with the search box: no hit.

## 6. After one week (SC-004)

List runs of `factory-monorepo-ci` and count the two new fault lines. An unexpected `step secret not allowed` means the README missed something: fix the README.
