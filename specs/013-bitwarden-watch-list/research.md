# Research: Bitwarden watch list

Decision, why, alternatives. Evidence is a file at `origin/main` 8660df5 unless stated.

## R1 How the tick gets the list

- Decision: the plan script reads it with `fci_list` and `fci_key_value`, writes a 0600 temp file and passes it as `--repos`. The runner still gets no store token.
- Why: `factory-ci-plan.sh` `run()` strips the token on purpose; the helpers already do the retry, timeout and 401/403 handling and have shell tests; the plan runner parses third-party repo data and should not hold the token.
- Rejected: the runner reads Bitwarden (needs `run()` to leak the token, a `bws` path on the VM and a second key lookup); a `watch-list` subcommand (only needed while scripts were frozen); recovering the token from the parent environment.

## R2 How `ci` gets the list

- Decision: in-process, in the existing `bws` call, scoped by `FACTORY_CI_BWS_PROJECT`; `--repos FILE` stays as a development and test option.
- Why: `ci` runs in one container with the token and no shell. The 2,979 lines of `ci_tests` use files.

## R3 Delete the file

- Decision: delete it (owner). The mount, `--repos` argument and the shell test that pinned them change in the same PR.

## R4 Lint and the cron

- Decision: edit the rule (`factory-ci-lint-workflow.sh:92`) to allow exactly one `*/5` cron line. The hardening test lints the real workflow (about line 303) and then passes.

## R5 What public text names the list today

- `plan.rs` `summary()`, `problem()`, line 231 (id); `main.rs` lines 131 and 180; `watch.rs` labels and quoted values.
- Decision: one parser with number-only messages; `entry N` in the plan; redaction seeds from the candidate rows.

## R6 `B_PRIVATE` on a schedule event

- A `schedule` payload has no `repository`, so `github.event.repository.private` is empty. `main.rs:81` reads empty as private, `plan.rs` computes `fail_closed = repo.private && !b_private`, so the private-repo guard is off. Decision: private only when the value is `true`.

## R7 Heartbeat

- `verdict`'s heartbeat step runs on `always()`. Decision: drop the condition on that step; the earlier step already fails the job on a bad plan.

## R8 The walk after a history rewrite

- `walk.rs` `decide` and `walk_one`: no status on the tip and none on the first-parent chain gives `Run { prev: None }` once per project and tip (runbook, "Effects on the tick").

## R9 Value limit

- The only figure found is 25,000 characters, from the SDK's input validation, read through DeepWiki; the docs give no number. The earlier guess of 3,500 is dropped. T002 asks the owner to confirm.

## R10 `silence_panics`

- 012's tree has the helper and a test that forbids it in the `plan` arm. Decision: the test changes to require it in `plan` (T012).
