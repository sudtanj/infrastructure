# Contract: files and command lines

## Workflow cron lines (out of scope)

No `schedule:` key, no `case`. `.github/workflows/factory-monorepo-ci.yml` does not change (owner rule of 2026-11-28).

## `rust-ci-runner ci`

```
ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]
```

There is no `--entry` flag (`bad arguments`); every ticket and asset name carries the entry `0`. The attempt number and the runner image come from `docker inspect` of the runner's own container (plan D19), with `GITHUB_RUN_ATTEMPT` and `FACTORY_CI_RUNNER_IMAGE` as overrides.

## `rust-ci-runner decrypt-artifact`

stdin: an `FCI1` artifact file. stdout: the tar. The key comes from `FACTORY_CI_ARTIFACT_KEY` in the process environment only. Any failure prints one generic line on stderr and exits non-zero; bytes already written are unverified until the exit code is 0.

## Watch entry

`state = true|false` (default false, needs `id`), `vault_items = ["opaque item name", ...]` (parsed and validated; nothing reads a vault). An entry with either key needs an explicit `secret_branches = ["main"]`. See data-model.md.

## Pipeline file

`[state]`, `[artifacts]`, step key `state = true`, step key `vault = [{ item, field, env }]`. See data-model.md. A vault entry has exactly the three string keys. `field` is `username`, `password`, `uri` or the exact name of a custom field. A pipeline whose vault entries pass validation ends `[runner] vault unavailable`.

## Workflow shape

Unchanged. The workflow keeps `workflow_dispatch` as its only trigger, its 45-minute job, its `concurrency` group, its image pin and its one `docker run` (container name `factory-ci-runner-$GITHUB_RUN_ID-$GITHUB_RUN_ATTEMPT`, socket mount). A Rust test reads the file and compares `timeout-minutes` with `JOB_LIMIT`.

## Public lines (closed lists)

stdout adds `[runner] waiting for the queue`. stderr adds nine lines: `queue wait expired`, `queue renewal failed`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`. No repo, branch, project, state, release, asset, item, field or secret name, and no count, position or age, appears in any public line.

## Files that do not change

Nothing under `.github/` and no `rust-ci-runner/Dockerfile`. The tick path (`job`, `local`) runs a pipeline that declares `[state]` or `[artifacts]` as before 011 (no store call), and refuses a vault declaration with `vault unavailable`.
