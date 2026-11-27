# Infrastructure Constitution

This repository is public. It holds the factory CI runner (`rust-ci-runner`) and every GitHub
Actions workflow for sudtanj's repositories. Other repositories keep no workflows of their own;
their checks run through the runner from here.

## Core Principles

### I. One Home for Automation

- Every GitHub Actions workflow for sudtanj's repositories MUST live in this repository, under
  `.github/workflows/`. Scripts that only workflows use live in `.github/scripts/`.
- The factory CI runner lives in `rust-ci-runner/`. It builds, lints and tests other repositories
  as pipeline steps declared in each repository's `.factory-ci.toml`.
- Which repositories the runner watches is declared in `.github/factory-ci-repos.toml`.

Rationale: one place owns all automation, so checks do not depend on whose machine ran them.

### II. Secrets Source

- Bootstrap secrets are the minimum the runner needs before it can read anything else: the
  Bitwarden access token, Grafana and Loki credentials, runner tokens and blob keys. They come
  from GitHub Actions secrets and variables of this repository, and from nowhere else.
- Pipeline secrets, the ones the watched repositories' steps use, come from Bitwarden, read at
  run time with the Bitwarden access token.
- A manually started pipeline workflow (`factory-monorepo-ci`) may hold only the Bitwarden
  access token in GitHub (and the optional Bitwarden server URL). It reads every other value it
  needs, the Grafana and Loki credentials and the clone token included, from Bitwarden, inside
  the runner. Owner decision of 2026-11-27, `specs/008-runner-owned-ci/`.
- A secret MUST NOT be committed in any file, including examples, fixtures and specs. Docs name
  a secret by key, never by value.

Rationale: a public repo can show its whole setup only if every value lives outside it.

### III. Nothing Private in Public Logs

- Workflow logs, job summaries, commit statuses, artifacts and error messages are public.
  They MUST NOT contain a secret value, a private repository's code, file paths or contents, or
  anything derived from them that identifies a secret.
- Workflows MUST mask secret values and MUST NOT print them, with or without `set -x`.
- Private details reach the maintainer only through the Grafana and Loki sink, never through
  public output. Public statuses link to it, and the link requires login.

Rationale: anyone on the internet can read every log of this repository.

### IV. Simplicity (YAGNI)

- Build what the current spec needs. No abstractions, options or config for hypothetical needs.
- Prefer the standard library and the platform's native features over a new dependency.
- Any added complexity MUST be justified in the feature's `plan.md` (Complexity Tracking).

Rationale: every extra layer is paid for by every pipeline that passes through it.

### V. Tested Changes

- Every acceptance criterion in a feature spec MUST map to at least one automated test.
- Every change MUST be covered by tests that run in the runner's own pipeline or this
  repository's workflows. Checks that run a process MUST NOT run only on a contributor's or an
  agent's machine.
- A pull request merges only when its workflow and runner statuses are green.

Rationale: the runner gates other repositories, so an untested change here breaks all of them.

### VI. Spec-Driven Changes (NON-NEGOTIABLE)

- Every change to this repository, of any size, MUST go through Spec Kit. This binds every
  contributor: people and AI agents alike, in any tool (Claude Code and its sub-agents, Codex,
  Cursor or any other).
- A change MUST belong to a feature folder under `specs/` holding `spec.md`, `plan.md` and
  `tasks.md`. Every Spec Kit step MUST run, in this order, none skipped: `/speckit-specify`,
  `/speckit-clarify`, `/speckit-plan`, `/speckit-tasks`, `/speckit-analyze`, then
  `/speckit-implement`. A fix or change to an existing feature runs the same steps on that
  feature's folder before any code changes. When the spec, plan or tasks change later, the
  steps from the changed one onward run again, `/speckit-analyze` included, before more code
  is written.
- Agents MUST produce `spec.md`, `plan.md` and `tasks.md` by invoking the Spec Kit commands
  (or their skills), never by writing them freehand. Manual edits to those files are allowed
  only to apply review findings, and the next Spec Kit command run on the folder MUST accept
  them.
- Implementation MUST cover only tasks listed in `tasks.md`. Work that is not listed goes back
  through spec, plan and tasks first.
- Every pull request MUST link its feature folder. Reviewers MUST reject a pull request
  without one, or one whose changes are not covered by its `tasks.md`.
- The only exception: amendments to this constitution and to the agent rule files that
  enforce it (`CLAUDE.md`, `AGENTS.md`) follow the Governance procedure instead.

Rationale: one spec-first path for every contributor keeps intent, plan and code traceable,
and stops agents from building what nobody specified.

## Repository Structure and Constraints

- Top-level layout: `rust-ci-runner/` (the runner), `.github/` (workflows, scripts and the
  watched-repo list), `specs/` (Spec Kit feature specs), `.specify/` and `.claude/` (Spec Kit
  tooling and agent skills).
- Repository secrets and variables are the only bootstrap channel (Principle II). The README
  lists every one the workflows read, and a workflow change that reads a new one updates that
  list in the same pull request.
- Third-party actions MUST be pinned and workflow `permissions` MUST be the minimum the job needs.

## Development Workflow

- Every change runs every Spec Kit step (Principle VI). There is no exception for small fixes.
- Work happens on a branch and merges to `main` through a pull request.
- A pull request merges only when its statuses are green and it links its feature folder under
  `specs/`.

## Governance

- This constitution overrides other conventions in the repo. Plans MUST pass its
  Constitution Check before implementation starts.
- Amendments are made by pull request that edits this file, states the reason, and lists any
  workflow or runner component that must change to comply.
- Versioning follows semantic versioning: MAJOR for removing or redefining a principle,
  MINOR for adding a principle or section or materially expanding one, PATCH for wording.
- Reviewers check pull requests against these principles; violations need either a fix or a
  justified exception recorded in the feature's `plan.md`.

**Version**: 1.1.0 | **Ratified**: 2026-11-27 | **Last Amended**: 2026-11-27
