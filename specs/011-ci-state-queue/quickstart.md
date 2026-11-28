# Quickstart: validating ci-state-queue

Prerequisites: 010 and 012 shipped. Nobody prints a value in any step below; checks read status codes, counts and names only. Contracts: [config-files](contracts/config-files.md), [release-store](contracts/release-store.md), [asset-format](contracts/asset-format.md), [loki-lines](contracts/loki-lines.md). Steps marked 0.4.1 wait for the vault release.

The owner rule bans any change to a file under `.github/workflows/` or `.github/scripts/`. The schedule, state, queue, artifacts and vault read run through the tick, which reaches the new binary by `gh variable set RUNNER_IMAGE_DIGEST`. The manual `ci` stays on 0.3.0.

## 1. Before the tag (CI proves it)

- `factory-ci-selftest` on the branch is green without any change to it: `cargo build --locked` (T001 alone on a branch first, to prove the hand-edited lock), `make lint`, `plan --repos ... --check`, the shell checks, and the image build for amd64 and arm64.
- The `rust-ci-runner` pipeline on `main` passes, including `releases_http_tests::` and `tick_state_tests::`, the two-workflow job-limit test, the schedule, ledger and plan tests, the alert-rule test and the agent-script test.
- `git diff --stat origin/main -- .github/workflows .github/scripts` is empty on the branch.
- After each tag: the release run finished; `docker pull` of the digest works; the sha256 of `rust-ci-runner-x86_64-linux` is written into the agent script (0.4.1).

## 2. Tick variable (T039; owner, or an agent the owner approves)

1. Record the current value: `gh variable get RUNNER_IMAGE_DIGEST`. Put it in the pull request. Tell feature 012's owner.
2. Set the new one: `gh variable set RUNNER_IMAGE_DIGEST --body sha256:<digest>`.
3. Wait for two ticks. Expect `fail_tick=0`, the same commit statuses (`factory-ci`, `factory-ci/<project>`) as before, no scheduled row and no new public line.
4. On any failed status: `gh variable set RUNNER_IMAGE_DIGEST --body <recorded old digest>`, then report.

## 3. Watch entry (T030, after section 2)

Open the pull request that adds `secret_branches = ["main"]` to the `sudtanj/factory-monorepo` entry and updates the comment block. The pull request text says the key also lets the frozen manual `ci` give declared step secrets on `main` (010), and enables the tick store features only from 0.4.x. Check: `plan --repos .github/factory-ci-repos.toml --check` passes with the 0.4.x runner and with 0.3.0; the next tick still posts the same statuses.

## 4. Release canary on the real store (T029; owner, before any state exists)

Seed the ledger release `ci-sched+<owner>_<name>` first (owner action 1). Run against `<owner>/factory-ci-state` with the real state token, in scratch releases `ci-queue+canary+x+y` and `ci-sched+canary_x`. Expected result on each line:

| Check | How | Expect |
|---|---|---|
| Store repo private, immutable releases off, initial README commit, no tag ruleset blocking creation | repo settings and `GET /repos/<store>` | `private: true`, setting off |
| `FACTORY_CI_TOKEN` reads the store | `GET` the release with that token | 200 |
| Release create and tag lookup lag | `gh api -X POST` on `releases`, then `GET releases/tags/<tag>` at once, 200 trials | 201; the lookup finds the tag every time (the owner seeds the ledger release, the job re-fetches 3 times) |
| Token without contents scope on the store | `GET releases/tags/<tag>` with a fine-grained token that lacks it | record 403 or 404; the plan reads both as a problem |
| Upload `t-<16 hex>-1-0` then a second | upload to `uploads.github.com` | both 201, the second `id` greater |
| Duplicate name (a claim name too) | upload the same name again | a 4xx |
| Label update | `PATCH` the asset label, then `GET` | label changed |
| Server time | `gh api -i` | a `Date:` header |
| List after create | 200 trials: upload, list at once | the new asset is in every list |
| In-flight upload | start a large upload, list during it | the asset shows as not `uploaded` with `created_at` |
| Racing jobs for one slot | two jobs claim one slot, 20 times | exactly one create wins each time |
| Racing runners for one state | two runners start the same state, 20 times | never two holders |
| Rate headers | one call, inspect `x-ratelimit-*` | present |
| Delete | `DELETE` the assets | 204, a second delete 404 |
| Names route | a fixture row on `main` with `secrets_ok`: the frozen script resolves `<repo><SEP>FACTORY_CI_STATE_TOKEN` | the runner holds it, the Actions log shows `***`, no step sees it |
| Helper image | run `chown` and `chmod` from the clone image | both work |
| Trace scan | one real Playwright login-failure trace from the 002 fixture (HTML reporter off), typed password seeded as a known value | refused; clean value passes |
| Trace after restored state | a trace after a login with restored state | refused unless the 002 scrub removed the cookies (by design) |
| `bw` (0.4.1) | the dedicated account, collection ids set, `list items` | only declared items appear |

If any line fails, stop. A failing label update, id order, 200-trial run or race is a plan change (open risks O1, O12), not a code fix. Delete the canary releases afterwards.

## 5. First real runs (T031; after sections 2 to 4 and the project toml change)

Loki query: `{service_name="factory-ci", repo="sudtanj/factory-monorepo"}`, filter on step `ci`; the lines are those of the loki-lines contract.

1. Add `[schedule]` with `every = "15m"` to a fixture project first. Expect one `claim created` per slot, none twice, `claim passed`, and a status only on the first run.
2. Kill a run job mid-run. Expect `claim stale break` after 3600 s, one retry, and no third run.
   Refuse a scheduled row on purpose (remove the store key for one tick): expect `error` on `factory-ci-schedule[/<project>]`, the push context of that sha unchanged, and no new row for the project until the `error` is older than `min(every, 1 h)`. Cancel a tick mid-run after a step finished: expect the claim set to `f`, not deleted.
3. Run the Playwright project. Expect `restore none`, a pass, `state saved`; the next slot `restore ok` and no login.
4. Fail a fixture on purpose. Expect an artifact asset, a Loki line with release and asset names, nothing in the Actions log. Open it with `rust-ci-runner/scripts/open-artifact.sh`.
5. 0.4.1: seed a throwaway vault password in a canary item, echo it in a failing step. Expect `artifact upload refused` or a masked log and no asset holding it. Rotate the throwaway.
6. Cancel a tick from the Actions page. Expect no ticket and no claim left.
7. Alerts: lower a threshold in Grafana and confirm the missing-`pipeline passed`, `queue wait expired` and ledger rules fire.
8. Confirm the manual `ci` refuses the same project (`pipeline file refused` from 0.3.0) and the README says so.

## 6. Rollback check

1. Remove the project toml keys; confirm a tick passes.
2. Set the recorded old digest back (`gh variable set RUNNER_IMAGE_DIGEST --body <old>`); confirm one tick passes.
3. Revert the Rust and `Dockerfile` commits. A manual run of a plain project passes as before.
4. The ledger, queue and state releases stay in the store repo; delete them by hand if wanted.
