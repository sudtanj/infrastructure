# Quickstart: validating ci-state-queue

Prerequisites: 010 shipped (0.3.0 pinned) and the owner actions of plan.md done. Nobody prints a value in any step below; checks read status codes, counts and names only. Contracts: [config-files](contracts/config-files.md), [release-store](contracts/release-store.md), [asset-format](contracts/asset-format.md), [loki-lines](contracts/loki-lines.md).

## 1. Before the tag (CI proves it)

- `factory-ci-selftest` on the branch is green: `cargo build --locked` (T001 alone on a branch first, to prove the hand-edited lock), `make lint`, `plan --repos ... --schedule ... --check`, the shell checks, and the image build for amd64 and arm64 with `RUN bw --version` (the arm64 smoke is non-blocking). Record the image size and the pull time before and after in the pull request.
- The `rust-ci-runner` pipeline on `main` passes, including the groups `releases_http_tests::` and `ci_state_tests::`.
- After the tag: the release run finished, smoke step included; `docker pull` of the digest works; the sha256 of `rust-ci-runner-x86_64-linux` is written into the workflow and the agent script.

## 2. Release canary on the real store repo (owner, before any state exists)

Run against the store repo with the real state token, in a scratch release `ci-queue-canary`. Expected result on each line:

| Check | How | Expect |
|---|---|---|
| Store repo private, immutable releases off | repo settings and `GET /repos/<store>` | `private: true`, setting off |
| Release create | `gh api -X POST` on `releases` with the canary tag | 201 |
| Upload `t-1` then `t-2` | upload to `uploads.github.com` | both 201, `id` of `t-2` greater |
| Duplicate name | upload `t-1` again | a 4xx |
| Label update | `PATCH` the asset label, then `GET` | label changed |
| Server time | `gh api -i` | a `Date:` header |
| List after create | 200 trials: upload, list at once | the new asset is in every list |
| In-flight upload | start a large upload, list during it | the asset shows as `starter` with `created_at` |
| Racing runners | two runners start the same state and entry at once, 20 times | never two holders; the loser waits |
| Rate headers | one call, inspect `x-ratelimit-*` | present |
| Delete | `DELETE` the assets | 204, a second delete 404 |
| `bw` | the dedicated account, collection ids set, `list items` | only allowlisted items appear |

If any line fails, stop. A failing label update, id order, 200-trial run or race is a plan change (open risks O1, O12), not a code fix. Delete the canary release afterwards.

## 3. First real run (after the pin, the entry, the settings and the 002 pipeline)

Loki query: `{service_name="factory-ci", repo="sudtanj/factory-monorepo"}`, filter on step `ci`; the lines are those of the loki-lines contract.

1. No-secret canary (T030): `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `project=rust-ci-runner` ends `[runner] pipeline passed`, with no queue or state line anywhere.
2. Run the Playwright project by hand. Expect: the first run restores nothing (`restore none`), passes, uploads one state asset (`state saved`).
3. Run again. Expect `restore ok` and no login.
4. Start three runs of the project at once, then a fourth. Expect three runs one after another, `[runner] waiting for the queue` once in each waiting log, no `overlap` line, and the fourth ending `[runner] queue is full`.
5. Use a non-secret fixture on a branch the entry allows (the entry allows only `main`, so use a fixture pipeline file on `main` that fails on purpose and holds no real login). Expect an artifact asset, a Loki line with the release and asset names, nothing in the Actions log. Open it with `rust-ci-runner/scripts/open-artifact.sh` and read the trace.
6. Seed a throwaway value as the vault password of a canary item, fail a run on purpose with a step that echoes the field. Expect `artifact upload refused` or a masked log, and no asset holding the value. Rotate the throwaway afterwards.
7. Cancel a waiting run and a running one from the Actions page. Expect the next run to start within one poll, with no stale-ticket wait, and no ticket left in the store.
8. Schedule: add the entry with `interval_minutes = 60`. Expect a `ci <index>` job in the first quarter hour of the hour, no repo name in its title or log, and a second tick for the same slot to exit 0 with `slot taken` in Loki.
9. Alerts: stop the schedule for one interval plus 45 minutes in a test copy, or lower the rule threshold in Grafana, and confirm the missing-`pipeline passed` rule fires.

## 4. Rollback check

1. In one commit (or the watch commit first): remove `state`, `vault_items`, the added `secret_branches` and the schedule entries. Then revert `factory-monorepo-ci.yml`, then the pin.
2. Expect a manual run to pass on 0.3.0 and the schedule to stop. A 0.3.0 runner refuses unknown watch keys, so doing the pin first would fail every run: this step proves the order.
3. The queue, state and artifact releases stay in the store repo; delete them by hand if wanted.
