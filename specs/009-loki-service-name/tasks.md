# Tasks: Name the runner's logs in Grafana

**Input**: `specs/009-loki-service-name/` (spec.md, plan.md)

- [x] T001 [US1] Add `SERVICE_NAME` to the stream labels in `rust-ci-runner/src/ship.rs` and to the label assertion in `rust-ci-runner/src/ship_tests.rs`; bump `rust-ci-runner/Cargo.toml` to 0.1.1 (FR-001).
- [x] T002 [US1] Add `service_name` to the probe body and the Explore query line to the summary in `.github/workflows/factory-monorepo-ci.yml` (FR-002, FR-003).
- [ ] T003 [US1] Run the `rust-ci-runner` pipeline on `main`, then tag `rust-ci-runner-v0.1.1` and pin the published digest in `RUNNER_IMAGE` (FR-004).
- [ ] T004 [US1] Run both pipelines by hand; the owner checks Grafana (SC-001).
- [x] T005 Update `README.md` and `rust-ci-runner/README.md` with the label and the query; the GitHub description needs no change. Last task.
