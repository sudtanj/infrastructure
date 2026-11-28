# Quickstart: validate step secrets

Nothing here runs on a laptop or agent VM. Tests run in the `rust-ci-runner` pipeline and the selftest; the release runs in `rust-ci-runner-release.yml`. Rules are in [contracts/ci-secrets.md](contracts/ci-secrets.md). No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or Environment secrets, or creates or reads Bitwarden keys; agents debug with the Actions log and Loki only. Every Bitwarden action is the owner's.

Loki query (Grafana Explore, Loki data source) for one run's runner detail:

```text
{service_name="factory-ci", repo="sudtanj/factory-monorepo", step="ci"}
```

Replace `step="ci"` with a step name for that step's output. Narrow the time range to the run.

## 1. Tests (after T018 landed the code on `main`)

Start `factory-monorepo-ci` with `repo = sudtanj/infrastructure`, `ref = main`, `project = rust-ci-runner`. Steps `unit` and `integration` pass, the new tests among them. Expected: `[runner] pipeline passed`. `factory-ci-selftest.yml` is green too (`make lint`, `--test '*'` including `tests/repo_files.rs`).

## 2. Release

Check that the commit you tag is the commit the pipeline run tested (compare full SHAs), then push tag `rust-ci-runner-v0.3.0`. Expected: the release workflow ends green, its smoke step prints `repository or ref not in the watch list` and exits 1, and the summary shows a digest. Read the digest from the release run summary; do not pull the image on the agent VM, the canary in section 4 pulls it.

## 3. Pin

Push two commits to a branch: one adding `.github/workflows/factory-monorepo-ci.yml` to the `paths` of `factory-ci-selftest.yml` (its glob does not match that name), then the pin. The selftest job `shell` runs `.github/scripts/test-factory-monorepo-ci.sh` and passes. Fast-forward `main`.

## 4. No-secret canary

Start `factory-monorepo-ci` with `repo = sudtanj/infrastructure`, `ref = main`, `project = ci-canary` (a one-step pipeline; this run pulls the 0.3.0 image). Expected: `[runner] pipeline passed`. If not, revert the pin commit.

## 5. Preconditions

Run the read-only `gh api` checks of T022 (the token needs admin scope on both repos; the owner runs them) (Environment `factory-ci` limited to `main`, collaborators, no Dependabot or Renovate auto-merge on `factory-monorepo` `main`, tag ruleset). Do not go on if one differs.

## 6. Refusal checks (no container may start)

Before the canary key of section 7 exists, with its name in the entry and in a step, start `factory-monorepo-ci` for `sudtanj/factory-monorepo`. Expected stderr: `[runner] step secret unavailable`, exit 1, no `[<step>] started` line; Loki step `ci` shows the name and `expected 1, found 0`. Runbook: check the key exists once, with exactly that name, in the project the machine account reads, then start the run again (the `bws` call retries once with no backoff). With a name not in the entry: `[runner] step secret not allowed`.

## 7. Canary values (before any real value)

The owner creates a key with a made-up value and lists its name in a step of the `factory-monorepo` pipeline and, temporarily, in the watch entry. The step prints the value plain, as `echo "$V" | base64` and percent-encoded. Expected: `pipeline passed`, only grammar lines in the Actions log, `***` for all three in Loki. Then remove the name from the entry and delete the key.

## 8. The real run (SC-001)

When feature 011 (or a non-login name from T023) gives a real step secret: start `factory-monorepo-ci` on `main`. Expected: only grammar lines in the Actions log, `[runner] pipeline passed`, `***` in Loki. Search the Actions log for the value: no hit.

## 9. After one week (SC-004)

List runs of `factory-monorepo-ci` and count the two new fault lines. An unexpected `step secret not allowed` means the README missed something.
