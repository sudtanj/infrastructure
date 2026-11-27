# Contract: tick workflow and plan script rules added in review round 2

Covers FR-034 to FR-038. The base contract is `specs/002-ci-runner/contracts/` (tick, plan output); this file lists only what 003 adds or changes. Files: `.github/workflows/factory-ci-tick.yml`, `.github/scripts/factory-ci-plan.sh`, `factory-ci-job.sh`, `factory-ci-status.sh`, `factory-ci-blob.sh`, `factory-ci-lint-workflow.sh`.

## Heartbeat (`verdict` job, step `heartbeat`; FR-034)

Inputs: `FACTORY_CI_HEARTBEAT_URL` (secret of `factory-ci-plan`), `PLAN_RESULT` (`needs.plan.result`), `FAIL_TICK` (`needs.plan.outputs.fail_tick`), `RUN_RESULTS` (`needs.run.result` and `needs.run-docker.result`, space separated).

| Condition | Ping |
| --- | --- |
| `GITHUB_REF` is not `refs/heads/main` | none |
| `FACTORY_CI_HEARTBEAT_URL` is empty | none |
| `PLAN_RESULT` is not `success` | `<url>/fail` |
| `FAIL_TICK` is `1` | `<url>/fail` |
| `RUN_RESULTS` holds `failure` or `cancelled` | `<url>/fail` |
| otherwise | `<url>` |

- A trailing `/` on the URL is removed before `/fail` is added.
- The step runs with `if: always()`, `set +x`, `curl -fsS --max-time 10`, output discarded, and a curl error never fails the job.
- The URL is a secret: it never appears in a log, a summary or an output.
- README Setup step 11 says: a red tick or a failed plan pings `/fail` and alerts at once; a late check means ticks stopped.
- Evidence: the engineer's tick change plus the 13 rows QA added to `test-factory-ci-hardening.sh` in commit aa6a307 (fake `curl`: green, fail, idle, off-main, no URL, curl error). The lint rule `verdict-ping` reads the step.

## Blob key guard (FR-036)

`FACTORY_CI_BLOB_KEY` and `FACTORY_CI_BLOB_KEY_DOCKER` are valid only when they match `^[0-9a-f]{64}$`. The same check sits in every place that reads one:

- the three guard steps in the tick (`plan`, `run`, `run-docker`; the docker key may be empty in `plan`);
- `factory-ci-plan.sh` (both keys);
- `fci_blob_encrypt` and `fci_blob_decrypt` in `factory-ci-blob.sh`, so `factory-ci-job.sh` and `factory-ci-status.sh` refuse a bad key through them.

A refusal message names the variable and says "64 lower-case hex characters", never the value.

## Plan script (FR-037, FR-038)

1. The `run` function strips `BWS_ACCESS_TOKEN`, `BWS_ACCESS_TOKEN_DOCKER` and both blob keys. For the phase A call (`plan --rows-out`) it also strips `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, because `plan::candidates` only reads GitHub and the log line is sent from `plan::finish` (`rust-ci-runner/src/plan.rs`, phase C). The phase C call keeps them.
2. `fci_list` runs (plain store) only when `jq` finds at least one row with `secrets_ok == true` and (`secrets != ""` or `repo_id != ""`) among the plain rows, and the Bitwarden token is set. The docker store follows the same rule for docker rows. With no such row the plan makes no Bitwarden call even when a token is set.

## Lint rules (FR-035)

The lint stays line-based. Each rule below has a mutation row in `--self-test` that fails for the named rule.

| Rule | Reads | Fails when |
| --- | --- | --- |
| `tick-on-only-dispatch` | the tick's `on:` block | any key other than `workflow_dispatch:` at 2 spaces of indent under `on:` (quoted or not), or a flow-style `on:` |
| file loop | `*.yml` and `*.yaml` | a `.yaml` workflow is skipped |
| the existing 40-hex `uses:` pin rule and `permissions` present rule (names as in the script) | every workflow file | a `uses:` not pinned to 40 hex, or a file with no `permissions` |
| `tick-environment` | the tick's jobs | `plan` or `verdict` not in `factory-ci-plan`, `run` not in `factory-ci`, `run-docker` not in `factory-ci-docker` |
| `main-guard` (existing rule, regex tightened) | the tick job `if:` lines the rule reads, and the mirror job | the line does not match `if: github.ref == 'refs/heads/main'( && .*)?$`, or holds `\|\|` |
| `selftest-shell` (extended) | the selftest | `secrets: inherit` or `secrets[` appears |

Known gap, accepted: YAML escape sequences in trigger keys. Only the owner can merge (R2-2), which covers it.
