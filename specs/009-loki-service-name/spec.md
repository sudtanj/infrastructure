# Feature Specification: Name the runner's logs in Grafana

**Feature Branch**: `009-loki-service-name`
**Created**: 2026-11-27
**Status**: Implemented
**Input**: Owner: "Fix those unit test and run, I want to see logs on Grafana."

## User Scenarios & Testing

### User Story 1 - Find a run's logs in Grafana (Priority: P1)

The owner starts a manual run and then looks for its output in Grafana. Today Loki accepts the lines (the probe gets HTTP 204), but they carry no `service_name` label, so Grafana Logs Drilldown files them under `unknown_service` and the owner finds nothing.

**Acceptance Scenarios**:

1. **Given** a manual run has finished, **When** the owner opens Logs Drilldown, **Then** a service named `factory-ci` lists the probe line and every step line of that run.
2. **Given** a manual run has finished, **When** the owner reads the job summary, **Then** it shows the Explore query that finds the run's lines.

## Requirements

- **FR-001**: Every stream the runner pushes to Loki carries the label `service_name="factory-ci"`, next to the existing `repo`, `project`, `step` and `stream` labels.
- **FR-002**: The workflow's probe line carries the same `service_name` label.
- **FR-003**: The job summary prints `Grafana Explore: {service_name="factory-ci", repo="<repo>"}`. The query names no private value.
- **FR-004**: A new runner release (0.1.1) ships the label, and the manual workflow pins it by digest.

## Success Criteria

- **SC-001**: After one manual run of each pipeline, the owner sees that run's lines under service `factory-ci` in Grafana.

## Assumptions

- One fixed value is enough: `repo` and `project` already split the lines by target.
- Lines pushed before this change stay under `unknown_service` until Loki retention removes them.
