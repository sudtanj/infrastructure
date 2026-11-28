# Quickstart: prove the feature

No agent runs any of this. Steps marked owner use credentials.

1. Owner (T001 to T003): create `FACTORY_CI_REPOS`; confirm the limits and the machine account; set the Environment rules and the ruleset.
2. PR-1 selftest (T015): green on the branch; tag `rust-ci-runner-v0.5.0`; record the previous digest; set the new one.
3. Owner (T016): dispatch the tick (old script, committed file). Expect green and the same rows. Pipe the Bitwarden value into `rust-ci-runner plan --repos - --check`; expect the entry count. Repeat after every edit of the key.
4. PR-2 merged (T021): dispatch the tick. Expect green, rows equal to step 3, `item N` lines only in the summary. Rename the key, dispatch: expect a red tick, `watch list key FACTORY_CI_REPOS not found or empty`, no new statuses and no heartbeat. Rename back.
5. Before PR-3 (T022): compare the Bitwarden value with the committed file.
6. PR-3 merged (T027): dispatch a tick; run the manual workflow with `entry` set to a listed number and to one past the end (the latter prints `[runner] repository or ref not in the watch list`); search the new logs for the scrub strings.
7. PR-4 merged by the owner's account (T029): an hour later Actions shows scheduled runs about every 5 minutes; after a week count Canceled and Failed apart. Stop command: `gh workflow disable factory-ci-tick`.
8. Scrub and docs (T030 to T032): grep the tree for the scrub strings; delete old runs.
9. Rewrite (T033): only on the owner's fresh go; follow contracts/history-rewrite.md; verify on a fresh mirror clone; after the first tick expect one run per project at each watched tip.
