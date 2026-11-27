# Contract: tick workflow and plan script rules added in review round 2

Covers FR-034 to FR-037 and, from round 4, FR-051 to FR-069 (FR-038 is withdrawn). The load step and the new job layout are in `bootstrap-secrets.md`. The base contract is `specs/002-ci-runner/contracts/` (tick, plan output); this file lists only what 003 adds or changes. Files: `.github/workflows/factory-ci-tick.yml`, `.github/scripts/factory-ci-plan.sh`, `factory-ci-job.sh`, `factory-ci-status.sh`, `factory-ci-blob.sh`, `factory-ci-lint-workflow.sh`.

## Heartbeat (`verdict` job, step `heartbeat`; FR-034; optional since R2-38)

Inputs: `FACTORY_CI_HEARTBEAT_URL` (a Bitwarden key, loaded by `verdict`, FR-057; leave it unset for manual runs), `PLAN_RESULT` (`needs.plan.result`), `FAIL_TICK` (`needs.plan.outputs.fail_tick`), `RUN_RESULTS` (`needs.run.result`).

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

`FACTORY_CI_BLOB_KEY` (the only blob key since round 4) is valid only when it matches `^[0-9a-f]{64}$`. The same check sits in every place that reads it:

- the guard steps of `plan` and `run` in the tick;
- `factory-ci-plan.sh`;
- `fci_blob_encrypt` and `fci_blob_decrypt` in `factory-ci-blob.sh`, so `factory-ci-job.sh` and `factory-ci-status.sh` refuse a bad key through them.

A refusal message names the variable and says "64 lower-case hex characters", never the value.

## Plan script (FR-037, FR-056)

1. The `run` function strips `BWS_ACCESS_TOKEN` and `FACTORY_CI_BLOB_KEY`. For the phase A call (`plan --rows-out`) it also strips `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` and `FACTORY_CI_LOG_TOKEN`, because `plan::candidates` only reads GitHub and the log line is sent from `plan::finish` (`rust-ci-runner/src/plan.rs`, phase C). The phase C call keeps them.
2. The plan script makes no `bws` call (round 4). The `load secrets` step before it is the store check (FR-056).

## Lint rules (FR-035)

The lint stays line-based. Each rule below has a mutation row in `--self-test` that fails for the named rule.

| Rule | Reads | Fails when |
| --- | --- | --- |
| `tick-on-only-dispatch` | the tick's `on:` block | any key other than `workflow_dispatch:` at 2 spaces of indent under `on:` (quoted or not), or a flow-style `on:` |
| file loop | `*.yml` and `*.yaml` | a `.yaml` workflow is skipped |
| the existing 40-hex `uses:` pin rule and `permissions` present rule (names as in the script) | every workflow file | a `uses:` not pinned to 40 hex, or a file with no `permissions` |
| `tick-environment` | the tick's jobs | a job is not in `factory-ci` (round 4: `plan`, `run` and `verdict` all use it) |
| `main-guard` (existing rule, regex tightened) | the tick job `if:` lines the rule reads, and the mirror job | the line does not match `if: github.ref == 'refs/heads/main'( && .*)?$`, or holds `\|\|` |
| `selftest-shell` (extended) | the selftest | `secrets: inherit` or `secrets[` appears |
| `permissions` (tightened) | every workflow file | a `permissions:` that is not `{}` or a block of `scope: read\|write\|none` lines, at workflow or job level (`write-all`, `read-all`) |
| `uses-pin`, `runs-on-pinned` (tightened) | every workflow file | a `uses:` or `runs-on:` key in flow style (`{uses: x}`) or with quotes is read like a plain one; `runs-on` must be the plain block form `runs-on: ubuntu-24.04` |
| `main-guard` (every release job) | every workflow file with a job in `factory-ci-release` | the job has no end-anchored main `if:`, and the file is not a tag-push workflow whose `verify` step runs `git merge-base --is-ancestor "$GITHUB_SHA" origin/main` |

Known gap, accepted: YAML escape sequences in trigger keys. Only the owner can merge (R2-2), which covers it.

## Lint rules added in round 4 (FR-047, FR-052, FR-054, FR-057)

Each has a mutation row in `--self-test`. Rules that existed only for the docker split are deleted (`bws-scope`, `blob-key-scope`, `have-bws`, `run-docker-name`, the docker rows of `tick-environment`, `main-guard` and `verdict-always`).

| Rule | Fails when |
| --- | --- |
| `github-items` | a workflow reads a `secrets.X` other than `BWS_ACCESS_TOKEN` or `GITHUB_TOKEN`, or a `vars.X` other than `FACTORY_CI_BWS_SERVER_URL` |
| `bws-token-scope` | `secrets.BWS_ACCESS_TOKEN` is mapped in a step that is neither `load secrets` nor the `run` job's `job` step, or the `verdict` job maps it outside `load secrets` |
| `load-order` | a tick job lacks `install bws` then `load secrets` before its repo scripts, or has a `uses:` after `load secrets` |
| `load-list` | a job's `load secrets` arguments differ from the table in `bootstrap-secrets.md` |
| `verdict-soft` | `verdict`'s `install bws` or `load secrets` lacks `continue-on-error: true` or `timeout-minutes: 2`, the load step lacks `FACTORY_CI_FETCH_WINDOW: 20`, the job lacks `timeout-minutes: 10`, or a soft step comes after the verdict step |
| `single-run` | the workflow has a `run-docker` job, `matrix_docker`, or `max-parallel: 4` other than once |
| `verdict-main` | any tick job, `verdict` included, lacks an end-anchored main-only `if:` (`verdict`: `... && always()`) |
| `install-identical` | the three `install bws` `run:` bodies differ byte for byte, or one lacks the `$GITHUB_PATH` append, `--proto '=https' --tlsv1.2` or the `BWS_SHA256` comparison, or downloads a checksum file |
| `image-owner` | the `run` guard step does not refuse a clone or dind reference outside `ghcr.io/$owner/` |
