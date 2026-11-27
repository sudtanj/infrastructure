# Implementation Plan: Name the runner's logs in Grafana

**Spec**: `specs/009-loki-service-name/spec.md`

## Changes

- `rust-ci-runner/src/ship.rs`: a `SERVICE_NAME` constant, added to the stream labels in `Shipper::body` (FR-001).
- `rust-ci-runner/src/ship_tests.rs`: the exact-label assertion includes `service_name` with value `factory-ci`.
- `rust-ci-runner/Cargo.toml`: version 0.1.1, which the release workflow checks against the tag.
- `.github/workflows/factory-monorepo-ci.yml`: the probe body gets the label (FR-002), the summary prints the query (FR-003), and `RUNNER_IMAGE` moves to the 0.1.1 digest after the release (FR-004).
- `README.md`, `rust-ci-runner/README.md`: the query and the label.

## Tests

The updated ship test runs in the `rust-ci-runner` pipeline on `main`. Two manual runs after the release prove SC-001 together with the owner's look in Grafana.

## Constitution

The change adds one constant label. No secret moves, the public log gains one fixed line with the public repo name, and the build stays in the release workflow.
