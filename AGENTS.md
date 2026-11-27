# infrastructure

Rules for every coding agent (Codex, Cursor, Gemini and others). Claude Code reads `CLAUDE.md`, which holds the same rule. Follow `.specify/memory/constitution.md`. The Spec Kit commands are in `.claude/skills/speckit-*/SKILL.md`; run them by following those files when your tool has no Spec Kit integration.

## Spec Kit is mandatory

Every change in this repo goes through Spec Kit, for every agent and sub-agent in any tool, and for people. See Principle VI of the constitution.

- Before any code change, the work must be in a feature folder under `specs/` with `spec.md`, `plan.md` and `tasks.md`.
- Run every Spec Kit step, in order, none skipped: `/speckit-specify`, `/speckit-clarify`, `/speckit-plan`, `/speckit-tasks`, `/speckit-analyze`, `/speckit-implement`. Do not write spec, plan or tasks freehand.
- When the spec, plan or tasks change, rerun the steps from the changed one onward, `/speckit-analyze` included, before writing more code.
- Implement with `/speckit-implement`, and only tasks listed in `tasks.md`. Anything not listed goes back through spec, plan and tasks first.
- A sub-agent brief must name the feature folder and the task IDs it covers. An agent that gets a brief without them stops and asks for them.
- Every pull request links its feature folder. Reviewers reject one without it.
- Every change that alters what the repo holds, how it is set up or used, or its rules updates the root `README.md` (and `rust-ci-runner/README.md` when the runner changes) in the same pull request and the GitHub repository description when it merges. The last task in `tasks.md` does this or says why neither needs a change.
- Only the constitution, `CLAUDE.md` and `AGENTS.md` are exempt; they change through the constitution's Governance procedure.


## Where things live

This repo is public and holds the factory CI runner (`rust-ci-runner/`) and every GitHub Actions workflow for sudtanj's repos. Bootstrap secrets come from this repo's Actions secrets and variables; pipeline secrets come from Bitwarden. Never commit a secret and never print one in a log (Principles II and III).
