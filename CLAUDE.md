# infrastructure

Follow `.specify/memory/constitution.md`.

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

## Always-on skills

These skills ship in `.claude/skills/` and load for anyone who clones the repo.

- Before writing or editing prose a person will read (docs, READMEs, commit and PR text, emails, UI copy), load `stop-slop` and apply it.
- When editing a draft a person wrote, or when asked whether text reads as AI, load `no-ai-slop` instead, so the writer's voice is kept.
- Before designing or building any web page, component or app screen, load `anti-slop-ui` and follow its workflow (one design school, one layer per group, always check `base-ai-slop.md`).

## Session defaults

Both modes below are on at ultra from the first message of every session.

- Ponytail at level `ultra` for all coding work: question the request and push back on any part the need does not justify before building. Its rules are imported below.
- Ultracave for all chat replies. Its rules are imported below; ignore its "invoke only with /ultracave" line, this file turns it on. Commits, PRs, docs and code comments stay in normal prose, as ultracave's own rules require.

@.claude/skills/ponytail/SKILL.md
@.claude/skills/ultracave/SKILL.md
