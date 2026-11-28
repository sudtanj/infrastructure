# Contract: files and command lines

## Workflow cron lines (`.github/workflows/factory-monorepo-ci.yml`)

```yaml
on:
  workflow_dispatch: { ... inputs unchanged ... }
  schedule:
    - cron: "7 * * * *"      # entry 1
    # up to 3 lines in all
```

The run step maps the schedule string with a short `case` (the string arrives through `env: SCHEDULE: ${{ github.event.schedule }}`, never inside a `run:` block):

```sh
case "$SCHEDULE" in
  "7 * * * *") REPO=owner/name; REF=main; PROJECT=.; ENTRY=1 ;;
  "") REPO=$IN_REPO; REF=$IN_REF; PROJECT=$IN_PROJECT; ENTRY=0 ;;
  *) echo "unknown schedule" >&2; exit 1 ;;
esac
```

Rules the shell test enforces: at most 3 `cron` lines; each has one `case` branch and each branch one `cron` line; entry numbers are 1 to 3 and unique; values are plain words; the default branch fails. The mapped repo, ref and project are in the public workflow file, like the watch list. The job name is `ci <entry>`.

## `rust-ci-runner ci`

```
ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] [--entry N] --work DIR [--allow-docker]
```

`--entry N` is a plain digit string 0 to 3 (default 0); it only numbers asset names. A bad value is `[runner] bad arguments`. The mapped target goes through the same watch-list check as a manual run.

## `rust-ci-runner decrypt-artifact`

stdin: an `FCI1` artifact file. stdout: the tar. The key comes from `FACTORY_CI_ARTIFACT_KEY` in the process environment only. Any failure prints one generic line on stderr and exits non-zero; bytes already written are unverified until the exit code is 0.

## Watch entry

`state = true|false` (default false, needs `id`), `vault_items = ["opaque item name", ...]` (parsed and validated from 0.4.0, read from 0.4.1). An entry with either key needs an explicit `secret_branches = ["main"]`. See data-model.md.

## Pipeline file

`[state]`, `[artifacts]`, step key `state = true`, step key `vault = [{ item, field, env }]`. See data-model.md. A vault entry has exactly the three string keys. `field` is `username`, `password`, `uri` or the exact name of a custom field. In 0.4.0 a pipeline whose vault entries pass validation ends `[runner] vault unavailable`.

## Workflow shape

- Triggers: `workflow_dispatch` (inputs unchanged) and up to 3 `schedule` cron lines.
- One job `ci`: main only, `needs` nothing, timeout 210, Environment `factory-ci` (or `factory-ci-schedule` for scheduled starts, see the plan's owner decisions), name `ci <entry>`, one `docker run` that also passes `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_RUNNER_IMAGE`, where the second takes its value from `RUNNER_IMAGE`.
- No `concurrency` block, no `due` job, no schedule file, no new secret, variable or input.
- Mounts: socket, work dir, watch list read-only.

## Public lines (closed lists)

stdout adds `[runner] waiting for the queue`. stderr adds nine lines: `queue wait expired`, `queue renewal failed`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`. No repo, branch, project, state, release, asset, item, field or secret name, and no count, position or age, appears in any public line.

## Landing order of the workflow

T023 lands the workflow with zero `cron` lines and no `schedule:` key, so merging it starts nothing. T030 adds the first `cron` line, its `case` branch and the `schedule:` key, and the header of `.github/factory-ci-repos.toml`. The header comment of the workflow explains why the concurrency group is gone, and the shell test checks it.
