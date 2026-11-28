# Contract: script, lint and workflow changes

## `factory-ci-secrets.sh`: new helper

`fci_watch_list FILE`: needs `FCI_JSON` from a successful `fci_list`. Calls `fci_key_value FACTORY_CI_REPOS` and maps the code:

| Code | `FCI_ERR` (becomes a public summary line through `fci_clean`) |
|---|---|
| 0 | none; the value is written to FILE with `printf '%s\n'` |
| 1 | `watch list key FACTORY_CI_REPOS not found or empty` |
| 2 | `watch list key FACTORY_CI_REPOS found more than once` |
| value over 65536 bytes | `watch list is larger than 64 KiB` |

It then sets `FCI_JSON=` and `FCI_VALUE=` (every other row is discarded). It never prints the value and never uses `set -x`. FILE is created by the caller.

## `factory-ci-plan.sh`

After the blob-key length check and the runner lookup, before phase A:

1. `[ -n "${BWS_ACCESS_TOKEN:-}" ] || fail "BWS_ACCESS_TOKEN is not set"`
2. `list=$(mktemp "$tmp/fci-list.XXXXXX") || fail "cannot create a temp file"` (mode 0600, exclusive; `umask 077` already set).
3. `fci_list "${FACTORY_CI_BWS_PROJECT:-}" "$BWS_ACCESS_TOKEN" || fail "$FCI_ERR"`
4. `fci_watch_list "$list" || fail "$FCI_ERR"`
5. Phase A: `run plan --repos "$list" --rows-out "$cand" || fail "plan failed before any status was posted"`, then `rm -f "$list"`.

`fail()` also removes `$list`. `run()` is unchanged: the runner still has no store token. The later log-credential `fci_list` call stays. An idle tick makes one `bws` call; a tick with work makes two.

## `factory-ci-lint-workflow.sh`

- Rule `schedule` replaces `no-schedule`: if `^  schedule:` exists, there is exactly one such line and exactly one line matching `^    - cron:`, and it equals `    - cron: "*/5 * * * *"`. `pull_request_target` stays refused; `workflow_dispatch:` stays required.
- Rule `heartbeat-gate`: the heartbeat step in `verdict` has no `if:` line.
- Rule `plan-step`: the `plan` job's step named `plan` has no `continue-on-error` and no `if:`.
- Self-test rows: the old `no-schedule` row becomes `schedule|wf|s/^  workflow_dispatch:/&\n  schedule:\n    - cron: "1 * * * *"/`; from T028 a row that edits the real cron expression; rows for the two new rules.

## `factory-ci-tick.yml`

- T019: the heartbeat step in `verdict` loses `if: always()`. The earlier step of the job exits 1 on a bad plan or `fail_tick=1`, so the default `success()` condition sends the ping only after a good plan.
- T028: `on:` gains `schedule:` with the single cron line.

## `factory-monorepo-ci.yml` (T024), renamed `factory-ci-manual.yml` (T030)

- `RUNNER_IMAGE` pinned to 0.5.0 and its digest.
- The `docker run` loses `-v ".../factory-ci-repos.toml:/etc/factory-ci/repos.toml:ro"` and `--repos /etc/factory-ci/repos.toml`; it gains `-e FACTORY_CI_BWS_PROJECT -e GITHUB_REPOSITORY_OWNER`; the step `env:` maps `FACTORY_CI_BWS_PROJECT: ${{ vars.FACTORY_CI_BWS_PROJECT }}` and `GITHUB_REPOSITORY_OWNER: ${{ github.repository_owner }}`.
- The `repo` input has no default and is required.
- The shell test expects two mounts (socket, work dir), no `--repos`, the two new `-e` names, and the new file name after T030.

## `rust-ci-runner-release.yml` (T024)

The smoke step runs `ci --repo x/y --ref main --work ...` with no `--repos` and no mount and still requires stderr exactly `[runner] repository or ref not in the watch list`, exit 1.

## `factory-ci-selftest.yml`

T020: plan-step and run-job checks (patterns written `[u]pload-artifact`, `[c]ontinue-on-error`). T025: both `paths` filters lose the committed file; the `list` job runs `plan --repos ../rust-ci-runner/tests/fixtures/watch-list.example.toml --check` (path relative to its working directory). T028: cron check.
