# Quickstart: validating ci-state-queue

Prerequisites: 010 shipped (0.3.0 pinned). Nobody prints a value in any step below; checks read status codes, counts and names only. Contracts: [config-files](contracts/config-files.md), [release-store](contracts/release-store.md), [asset-format](contracts/asset-format.md).

## 1. Before the tag (CI proves it)

- `factory-ci-selftest` on the branch is green: `cargo build --locked`, `make lint`, `plan --repos ... --schedule ... --check`, the shell checks, and the image build for amd64 and arm64 with `RUN bw --version`. Record the image size before and after in the pull request.
- The `rust-ci-runner` pipeline on `main` passes (it runs the new process groups `releases_http_tests::` and `ci_state_tests::`).
- After the tag: the release run finished, smoke step included; `docker pull` of the digest works.

## 2. API and vault canary (owner, once, before the first real run)

Use a throwaway private repo `sudtanj/ci-canary` (delete it afterwards) and a fine-grained token limited to it with contents write. Expected result on each line:

| Check | How | Expect |
|---|---|---|
| Immutable releases off | repo settings, Releases | off |
| Release create | `gh api -X POST repos/sudtanj/ci-canary/releases -f tag_name=ci-queue-canary -F prerelease=true -f make_latest=false` | 201 |
| Upload `t-1` then `t-2` | `gh api` upload to `uploads.github.com` with `name=t-N` | both 201, `id` of `t-2` greater than `t-1` |
| Duplicate name | upload `t-1` again | 422 |
| Label update | `PATCH .../releases/assets/{id}` with `{"label":"x.1"}` then `GET` | 200, label `x.1` |
| Server time | `gh api -i repos/sudtanj/ci-canary` | a `Date:` header |
| List after create | create, then list at once, ten times | the new asset is in every list |
| Delete | `DELETE` the assets | 204, a second delete 404 |
| `bw` | with the dedicated vault account in a one-collection organization, run the image's `bw` with the three vault variables set from the owner's shell, `list items` | the allowlisted item appears and nothing else |

If any line fails, stop. A failing label update or id order is a plan change (open risk O1), not a code fix.

## 3. First real run (after the pin, the entry, the settings and the 002 pipeline)

Loki query: `{service_name="factory-ci", repo="sudtanj/factory-monorepo"}`, filter on step `ci`.

1. No-secret canary (T030): `factory-monorepo-ci` with `repo=sudtanj/infrastructure`, `project=rust-ci-runner` ends `[runner] pipeline passed`, with no queue or state line anywhere.
2. Run the Playwright project by hand. Expect: the first run restores nothing, passes, uploads one state asset. Loki shows `restore: none` and `saved`.
3. Run again. Expect the second run to restore and skip the login. Loki shows `restore: ok`.
4. Start three runs of the project at once, then a fourth. Expect the first three to run one after another, each other public log with one `[runner] waiting for the queue`, and the fourth to end `[runner] queue is full`.
5. Break the test on a branch the entry allows, run it. Expect an artifact asset, a Loki line with the release and asset names, and nothing about them in the Actions log. Decrypt it with the command in asset-format.md and open the trace. Expect `FACTORY_CI_STATE_KEY` and the state file nowhere in it.
6. Seed a throwaway value as the vault password of the canary item, fail a run on purpose with a step that echoes the field. Expect `artifact upload refused` or a masked log, and no asset holding the value.
7. Cancel a waiting run and a running one from the Actions page. Expect the next run to start within one poll, with no stale-ticket wait.
8. Schedule: add the entry with `interval_minutes = 60`. Expect a `ci <index>` job in the first quarter hour of the hour and no repo name in its title or log.

## 4. Rollback check

Revert the pin and the workflow as the plan says. Expect a manual run to pass on 0.3.0 and the schedule to stop. The queue, state and artifact releases stay in the private repo; delete them by hand if wanted.
