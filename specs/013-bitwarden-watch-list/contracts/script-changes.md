# Contract: script, lint and workflow changes

## `factory-ci-secrets.sh`: new helper

`fci_watch_list FILE`: needs `FCI_JSON` from a successful `fci_list`. The script runs with `LC_ALL=C`, so `${#FCI_VALUE}` counts bytes. Calls `fci_key_value FACTORY_CI_REPOS` and maps the code:

| Code | `FCI_ERR` (becomes a public summary line through `fci_clean`) |
|---|---|
| 0 | none; the value is written to FILE with `printf '%s'` (no trailing newline) |
| 1 | `watch list key FACTORY_CI_REPOS not found or empty` |
| 2 | `watch list key FACTORY_CI_REPOS found more than once` |
| 0 and value over 65536 bytes | `watch list is larger than 64 KiB` (checked before the write) |

**Limit alignment.** `watch.rs` refuses text where `text.len() > MAX_BYTES` (65,536). The file holds exactly the value, with no added newline, so the script's limit and the parser's limit are the same number: 65,536 bytes passes both, 65,537 fails the script first with the fixed line. (`$(...)` already strips trailing newlines from `FCI_VALUE`.)

It then sets `FCI_JSON=` and `FCI_VALUE=` (every other row is discarded). It never prints the value and never uses `set -x`. FILE is created by the caller.

## `factory-ci-plan.sh`

After the blob-key length check and the runner lookup, before phase A:

1. `[ -n "${BWS_ACCESS_TOKEN:-}" ] || fail "BWS_ACCESS_TOKEN is not set"`
2. `list=$(mktemp "$tmp/fci-list.XXXXXX") || fail "cannot create a temp file"` (mode 0600, exclusive; `umask 077` already set).
3. `fci_list "${FACTORY_CI_BWS_PROJECT:-}" "$BWS_ACCESS_TOKEN" || fail "$FCI_ERR"`
4. `fci_watch_list "$list" || fail "$FCI_ERR"`
5. Phase A: `run plan --repos "$list" --rows-out "$cand" || fail "plan failed before any status was posted"`, then `rm -f "$list"`.

`fail()` also removes `$list`. `run()` is unchanged: the runner still has no store token. The later log-credential `fci_list` call stays. An idle tick makes one `bws` call; a tick with work makes two. Phase A needs `GITHUB_REPOSITORY_OWNER`, which GitHub sets for every step; `run()` passes it through.

## Shell test changes (T017)

Both `test-factory-ci-secrets.sh` and `test-factory-ci-hardening.sh` define their own `new_world`, a `plan_world`, `cand_row` and `run_plan`, and a stub `bws` and a stub `rust-ci-runner`. Every change below is made in both files unless a file is named.

| Where | Change |
|---|---|
| `plan_world` (secrets test about line 266, hardening test about line 137) | drop the line that writes `.github/factory-ci-repos.toml`; add `store_add FACTORY_CI_REPOS '<valid list text>'` so every plan test has a list |
| stub `rust-ci-runner`, phase A branch (the `*)` arm after `--rows-in`) | record, while the file still exists, `--repos` value, the file's mode (`stat -c %a`) and content into `$d/list.*`, so tests can assert them after the script removed the file |
| `run_plan` env arrays (both files) | add `GITHUB_REPOSITORY_OWNER=<fixture owner>` |
| secrets test, "idle plan: no bws call (I28)" (about line 284) | expect exactly 1 call; rename to "idle plan: one bws call for the list" |
| secrets test, "healthy plan ... one store call" (about line 294) | expect 2 calls (list, then log credentials) |
| secrets test, loop `for mode in r401 down` (about lines 297 to 306) | the failing fetch is now the list fetch: `phase C never started` expects an empty phase file (phase A never ran), not `A`; the summary text and "nothing posted" stay |
| secrets test, "malformed list: no store call" (about line 310) | expect 1 call (the list fetch happens before phase A); "malformed list: non-zero" and "phase C not started `A`" stay (the stub fails phase A) |
| secrets test, "malformed list: phase C not started" (about line 312) | keep `A` |
| hardening test, plan tests (about lines 176 to 223: output keys, `fail_tick`, blob-key length cases, docker key cases) | no assertion changes beyond the `plan_world` and `run_plan` changes above; re-run mentally: the blob-key length failures happen before the list fetch, so their "no output" assertions stand |
| hardening test line 172, "wrong key: no store call" | unchanged (this is the job wrapper, not the plan script) |
| both files, new assertions | the runner stub received `--repos` pointing at a mode-0600 file holding the exact value; the file is gone after success and after every failure; the runner stub's environment (`runner.env.*`) holds no `BWS_ACCESS_TOKEN`; no list text in `gh_summary`, `gh_out`, stdout or stderr; each of these exits 1 with the fixed text and runs no phase A: no token, store down, 401, key missing, key twice, key empty, value of 65,537 bytes; a value of 65,536 bytes passes; the key `FACTORY_CI_REPOS_X` does not count |

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
- **No repo name in the dispatch.** The `repo` input is replaced by a required `entry` input (type `string`, no default, description `number of the entry in the watch list, from 1`). The step `env:` maps `ENTRY: ${{ inputs.entry }}` in place of `REPO`; the `docker run` passes `--entry "$ENTRY"` in place of `--repo "$REPO"`; the concurrency group is `...-${{ inputs.entry }}-${{ inputs.ref }}-${{ inputs.project }}`. The step name and every comment name no repo. `ref` and `project` stay (accepted leak L5).
- The shell test (`test-factory-monorepo-ci.sh`, renamed `test-factory-ci-manual.sh` in T030) changes in these places: the input loop checks `entry`, `ref`, `project`; the env loop checks `ENTRY`, `REF`, `PROJECT` and still requires the `inputs.` expression form; the group check requires `inputs.entry`, `inputs.ref` and `inputs.project`; the flag list requires `--entry "$ENTRY"` and no longer `--repo "$REPO"` (a new check forbids `--repo`); `ci --repos /etc/factory-ci/repos.toml` becomes `ci --entry`; the mount check is deleted and the mount count becomes 2 (socket, work dir); two new `-e` names are required; a new check forbids the string `factory-ci-repos` anywhere in the workflow; the default path and name change in T030.

## `rust-ci-runner-release.yml` (T024)

The smoke step runs `ci --repo x/y --ref main --work ...` with no `--repos` and no mount and still requires stderr exactly `[runner] repository or ref not in the watch list`, exit 1 (an empty token prints that line before any other check).

## `factory-ci-selftest.yml`

T020: plan-step and run-job checks (patterns written `[u]pload-artifact`, `[c]ontinue-on-error`). T025: both `paths` filters lose the committed file; the `list` job runs `plan --repos ../rust-ci-runner/tests/fixtures/watch-list.example.toml --check --owner sudtanj` (path relative to its working directory; the fixture entries use fake repo names under `sudtanj/`, and the command never reads the ambient `GITHUB_REPOSITORY_OWNER`). T028: cron check.
