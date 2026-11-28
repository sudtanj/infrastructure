# Quickstart: validating ci-state-queue

Prerequisites: 010 shipped (0.3.0 pinned) and the owner actions of plan.md done. Nobody prints a value in any step below; checks read status codes, counts and names only. Contracts: [config-files](contracts/config-files.md), [release-store](contracts/release-store.md), [asset-format](contracts/asset-format.md), [loki-lines](contracts/loki-lines.md). Steps marked 0.4.1 wait for the vault release.

## 1. Before the tag (CI proves it)

- `factory-ci-selftest` on the branch is green: `cargo build --locked` (T001 alone on a branch first, to prove the hand-edited lock), `make lint`, `plan --repos ... --check`, the shell checks, and the image build for amd64 and arm64. Record the image size and pull time before and after in the pull request (0.4.0 adds no Node; 0.4.1 does).
- The `rust-ci-runner` pipeline on `main` passes, including the groups `releases_http_tests::` and `ci_state_tests::`.
- After the tag: the release run finished, smoke step included; `docker pull` of the digest works; the sha256 of `rust-ci-runner-x86_64-linux` is written into the agent script.

## 2. Release canary on the real store repo (owner, before any state exists)

Run against the store repo with the real state token, in a scratch release `ci-queue+canary+x+y`. Expected result on each line:

| Check | How | Expect |
|---|---|---|
| Store repo private, immutable releases off, initial README commit, no tag ruleset blocking creation | repo settings and `GET /repos/<store>` | `private: true`, setting off |
| Release create | `gh api -X POST` on `releases` | 201 |
| Upload `t-1-1-0` then `t-2-1-0` | upload to `uploads.github.com` | both 201, the second `id` greater |
| Duplicate name | upload `t-1-1-0` again | a 4xx |
| Label update | `PATCH` the asset label, then `GET` | label changed |
| Server time | `gh api -i` | a `Date:` header |
| List after create | 200 trials: upload, list at once | the new asset is in every list |
| In-flight upload | start a large upload, list during it | the asset shows as not `uploaded` with `created_at` |
| Racing runners | two runners start the same state at once, 20 times | never two holders; the loser waits |
| Rate headers | one call, inspect `x-ratelimit-*` | present |
| Delete | `DELETE` the assets | 204, a second delete 404 |
| Trace scan | capture one real Playwright login-failure trace from the 002 fixture (HTML reporter off), run the scan with the typed password seeded as a known value | refused; with a clean value, passes |
| `bw` (0.4.1) | the dedicated account, collection ids set, `list items` | only allowlisted items appear |
| Environment | the Environment that holds the token has no required reviewer and no branch rule that excludes scheduled runs | confirmed; else see the owner decision in plan.md |

If any line fails, stop. A failing label update, id order, 200-trial run or race is a plan change (open risks O1, O12), not a code fix. Delete the canary release afterwards; the canary records which state values it saw (names and counts only).

## 3. First real run (after the pin, the entry, the settings and the 002 pipeline)

Loki query: `{service_name="factory-ci", repo="sudtanj/factory-monorepo"}`, filter on step `ci`; the lines are those of the loki-lines contract.

1. No-secret canary (T030): `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `project=rust-ci-runner` ends `[runner] pipeline passed`, with no queue or state line anywhere.
2. Run the Playwright project by hand. Expect: `restore none`, a pass, one state asset (`state saved`).
3. Run again. Expect `restore ok` and no login.
4. Start four runs of the project at once. Expect them one after another, `[runner] waiting for the queue` once in each waiting log, no `overlap` line, no `queue wait expired`.
5. Use a non-secret fixture pipeline file on `main` that fails on purpose and holds no real login. Expect an artifact asset, a Loki line with the release and asset names, nothing in the Actions log. Open it with `rust-ci-runner/scripts/open-artifact.sh`.
6. 0.4.1: seed a throwaway value as the vault password of a canary item and fail a run on purpose with a step that echoes the field. Expect `artifact upload refused` or a masked log, and no asset holding the value. Rotate the throwaway afterwards.
7. Cancel a waiting run and a running one from the Actions page. Expect the next run to start within one poll and no ticket left in the store.
8. Cron: add the first cron line and its `case` branch. Expect a `ci 1` job at the cron minute, and a later run on a duplicated tick to queue behind the first.
9. Alerts: lower a rule threshold in Grafana and confirm the missing-`pipeline passed` rule and the `queue wait expired` rule fire.

## 4. Rollback check

1. In one commit (or the watch commit first): remove `state`, `vault_items`, the added `secret_branches` and the cron lines with their `case` branches. Then revert the pin. The tick workflow stays deleted.
2. Expect a manual run to pass on 0.3.0 and the schedule to stop. A 0.3.0 runner refuses unknown watch keys, so doing the pin first would fail every run: this step proves the order.
3. The queue, state and artifact releases stay in the store repo; delete them by hand if wanted.
