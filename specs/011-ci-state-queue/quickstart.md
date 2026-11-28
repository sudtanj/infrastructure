# Quickstart: validating ci-state-queue

Prerequisites: 010 shipped. Nobody prints a value in any step below; checks read status codes, counts and names only. Contracts: [config-files](contracts/config-files.md), [release-store](contracts/release-store.md), [asset-format](contracts/asset-format.md), [loki-lines](contracts/loki-lines.md). Steps marked 0.4.1 wait for the vault release.

The owner rule of 2026-11-28 (corrected the same day) bans any change to a file under `.github/workflows/` or `.github/scripts/`. The manual workflow keeps its image pin, so the queue, state, artifacts and vault read run only through `ci` and stay inert until the owner moves that pin. Sections 1 and 2a are the whole acceptance of 011. Sections 2 and 3 run only after the pin moves; they are kept so that day has a script.

## 1. Before the tag (CI proves it)

- `factory-ci-selftest` on the branch is green without any change to it: `cargo build --locked` (T001 alone on a branch first, to prove the hand-edited lock), `make lint`, `plan --repos ... --check`, the shell checks, and the image build for amd64 and arm64. Record the image size and pull time before and after in the pull request (0.4.0 adds no Node; 0.4.1 does, and its `RUN bw --version` runs in that build).
- The `rust-ci-runner` pipeline on `main` passes, including the groups `releases_http_tests::` and `ci_state_tests::`, the job-limit test, the tick-path tests, the alert-rule test and the agent-script test.
- `git diff --stat main -- .github/workflows .github/scripts` is empty on the branch.
- After each tag: the release run finished, smoke step included; `docker pull` of the digest works; the sha256 of `rust-ci-runner-x86_64-linux` is written into the agent script (0.4.1).

## 2a. Tick variable (T039; owner, or an agent the owner approves)

1. Record the current value: `gh variable get RUNNER_IMAGE_DIGEST`. Put it in the pull request. Tell feature 012's owner.
2. Set the new one: `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<digest>`, the digest checked in section 1.
3. Wait for one tick or dispatch it. Expect `plan` to report `fail_tick=0`, the same commit statuses (`factory-ci`, `factory-ci/<project>`) as the tick before the bump, and no new public line. The tick never reads state, artifacts or the vault (FR-062).
4. On any failed status: `gh variable set RUNNER_IMAGE_DIGEST --body <recorded old digest>`, then report. Repeat the whole section after 0.4.1.

## 2. Release canary on the real store repo (deferred: needs the pin; owner, before any state exists)

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
| Trace after restored state | capture a trace taken after a login with a restored state file | refused unless the 002 scrub removed the cookies; a refusal is by design (the README says logged-in traces may be refused) |
| Own container | `docker inspect` of the runner container under the real workflow | name `factory-ci-runner-<id>-<attempt>`, image with the pin |
| `bw` (0.4.1) | the dedicated account, collection ids set, `list items` | only allowlisted items appear |

If any line fails, stop. A failing label update, id order, 200-trial run or race is a plan change (open risks O1, O12), not a code fix. Delete the canary release afterwards.

## 3. First real run (deferred: after the manual pin, the watch keys, the settings and the 002 pipeline)

Loki query: `{service_name="factory-ci", repo="sudtanj/factory-monorepo"}`, filter on step `ci`; the lines are those of the loki-lines contract. The cron steps of the earlier draft are out of scope.

1. Run the Playwright project by hand. Expect: `restore none`, a pass, one state asset (`state saved`).
2. Run again. Expect `restore ok` and no login.
3. Start two runs of the project at once. GitHub's group keeps one running and one pending. Expect them one after another and no `overlap` line.
4. Use a non-secret fixture pipeline file on `main` that fails on purpose and holds no real login. Expect an artifact asset, a Loki line with the release and asset names, nothing in the Actions log. Open it with `rust-ci-runner/scripts/open-artifact.sh`.
5. 0.4.1: seed a throwaway value as the vault password of a canary item and fail a run on purpose with a step that echoes the field. Expect `artifact upload refused` or a masked log, and no asset holding the value. Rotate the throwaway afterwards.
6. Cancel a running run from the Actions page. Expect no ticket left in the store.
7. Alerts: lower a rule threshold in Grafana and confirm the missing-`pipeline passed` rule and the `queue wait expired` rule fire.

## 4. Rollback check

1. If the tick variable moved, set the recorded old digest back first (`gh variable set RUNNER_IMAGE_DIGEST --body <old>`) and confirm one tick passes.
2. Revert the Rust and `Dockerfile` commits. No workflow, manual pin or watch file changed, so nothing else moves. A manual run passes as before.
3. If the manual pin was moved later: remove any `state`, `vault_items` and `secret_branches` additions from the watch file first (or in the same commit as the pin revert), then revert the pin. A runner without these keys refuses them, so doing the pin first would fail every run.
4. The queue, state and artifact releases stay in the store repo; delete them by hand if wanted.
