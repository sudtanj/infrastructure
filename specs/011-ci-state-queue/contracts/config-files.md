# Contract: files and command lines

## `.github/factory-ci-schedule.toml`

```toml
# Reviewed on main. Unknown keys are errors. At most 3 entries.
# [[schedule]]
# repo = "sudtanj/factory-monorepo"   # required, must be in factory-ci-repos.toml with a numeric id
# branch = "main"                     # required, must match that entry's branches
# project = "."                       # optional, default "."
# interval_minutes = 60               # required, multiple of 15, 15 to 10080
```

`rust-ci-runner plan --repos FILE --schedule FILE --check` prints `N entries ok` or every problem and exits 1. It reads no token and calls no network. It does not cross-check the watch list; `ci` does that at run time.

## `rust-ci-runner due`

```
rust-ci-runner due --schedule FILE [--now-minute N]
```

stdout: one JSON array of entry indexes, for example `[0,2]` or `[]`, then a newline. Exit 0. A bad file: stderr `schedule file refused`, no stdout, exit 1. `--now-minute` is for tests; the default is the UTC epoch minute.

## `rust-ci-runner ci`

Either form, never both:

```
ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]
ci --repos FILE --schedule FILE --entry N --work DIR
```

`--entry N` is the zero-based index in the schedule file. A bad file or index is `[runner] bad arguments`. The entry's repo, branch and project then go through the same watch-list check as a manual run. `--allow-docker` is not allowed with `--schedule`.

## `rust-ci-runner decrypt-artifact`

stdin: an `FCI1` artifact file. stdout: the tar. The key comes from `FACTORY_CI_ARTIFACT_KEY` in the process environment only. Any failure prints one generic line on stderr and exits non-zero; bytes already written are unverified until the exit code is 0.

## Watch entry

`state = true|false` (default false, needs `id`), `vault_items = ["exact item name", ...]`. See data-model.md.

## Pipeline file

`[state]`, `[artifacts]`, step key `state = true`, step key `vault = [{ item, field, env }]`. See data-model.md. A vault entry has exactly the three string keys. `field` is `username`, `password`, `uri` or the exact name of a custom field.

## Workflow shape (`factory-monorepo-ci.yml`)

- Triggers: `workflow_dispatch` (inputs unchanged), `schedule` with `7,22,37,52 * * * *`; `due` counts from now minus 7 minutes.
- Job `due`: main only, `contents: read`, timeout 5, no Environment, no image pull. It downloads `rust-ci-runner-x86_64-linux` from the release, checks its pinned sha256, runs `due`, and writes `entries` only if the output matches `^\[[0-9,]*\]$`.
- Job `ci`: `needs: due`, runs when `entries != '[]'`, matrix `entry` from the output (`-1` is the manual run), `fail-fast: false`, name `ci <entry>`, timeout 210, Environment `factory-ci`, one `docker run`, which also passes `-e GITHUB_RUN_ATTEMPT -e FACTORY_CI_BUDGET_END -e FACTORY_CI_RUNNER_IMAGE`.
- No `concurrency` block. No new secret, variable or input.
- Mounts: socket, work dir, watch list read-only, schedule file read-only.

## Public lines (closed lists)

stdout adds `[runner] waiting for the queue`. stderr adds nine lines: `queue is full`, `queue wait timed out`, `state not allowed`, `state store not configured`, `state restore failed`, `state save failed`, `vault item not allowed`, `vault unavailable`, `artifact upload refused`. No repo, branch, project, state, release, asset, item, field or secret name, and no count, position or age, appears in any public line.
