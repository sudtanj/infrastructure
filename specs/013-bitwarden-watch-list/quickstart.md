# Quickstart: prove the feature

No agent runs any of this. Steps marked owner use credentials.

1. Owner (T001): create `FACTORY_CI_REPOS` in the factory-ci project with the two current `[[watch]]` tables. Check it by piping the value into `rust-ci-runner plan --repos - --check`; it prints `2 entries ok`. The value never goes to an agent.
2. Branch selftest (T017): the selftest passes on the PR 1 branch. Expect the `watch_list`, `cli`, plan, `ci_tests::list_store` and `repo_files` suites green.
3. Release (T017): tag `rust-ci-runner-v0.5.0`; the smoke test prints no failure; set `RUNNER_IMAGE_DIGEST`.
4. Old workflow, new runner (T018): dispatch the tick. Expect green and the same rows.
5. PR 2 merged (T023): dispatch the tick. Expect green. In the run, the `fetch watch list` step prints nothing and the summary holds `item N` lines with no repo name. Rename the key in Bitwarden, dispatch again: expect a red tick, `watch list: key FACTORY_CI_REPOS not found` and no new statuses; rename it back.
6. Leak search (SC-001): search the repo and the last runs' logs and summaries for each watched repo name; expect no hit outside git history.
7. PR 3 merged (T025): after an hour Actions shows scheduled runs about every 5 minutes (allow lateness); after a week, check the 24-hour gap rule of SC-004.
8. Manual workflow (T020): run `factory-monorepo-ci` for a listed repo; expect the normal fixed lines. An unlisted repo prints `[runner] repository or ref not in the watch list`.
