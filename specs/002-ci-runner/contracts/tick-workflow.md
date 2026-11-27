# Contract: `.github/workflows/factory-ci-tick.yml`

Trigger: `workflow_dispatch` only, no inputs, no `schedule`. The caller:

```
curl -fsS -X POST \
  -H "Authorization: Bearer <caller token>" \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  -H "User-Agent: factory-ci-scheduler" \
  https://api.github.com/repos/OWNER/B/actions/workflows/factory-ci-tick.yml/dispatches \
  -d '{"ref":"main"}'
```

The caller token is fine-grained, B only, Actions write, and lives in the scheduler. B is a public repo (A25),
and everything B prints is public (FR-059): no job `env:`, name or output may show a branch, SHA, project path,
secret name or run ID.

```
concurrency: { group: factory-ci-tick, cancel-in-progress: false }
permissions: {}     # per job below
runs-on: ubuntu-24.04 for every job

plan         if: github.ref == 'refs/heads/main'; environment: factory-ci-plan
             15 min; permissions contents: read
  1. checkout B (persist-credentials: false)
  2. guard: RUNNER_IMAGE_DIGEST, FACTORY_CI_CLONE_IMAGE and FACTORY_CI_DIND_IMAGE are set and pinned
     (@sha256 + 64 hex), else fail                                          (FR-026, FR-057; first step)
  3. fetch runner: docker create + docker cp from ghcr.io/<owner lowercased>/rust-ci-runner@<digest>
  4. install bws, only when HAVE_BWS == 'true' (tested in `run:`, never in `if:`)
  5. factory-ci-plan.sh
       a. rust-ci-runner plan --repos .github/factory-ci-repos.toml --rows-out $RUNNER_TEMP/cand.json
          (GitHub reads only; no write, no store call)
       b. if the candidate file holds rows: store check, then the log credentials from the store
       c. rust-ci-runner plan --rows-in $RUNNER_TEMP/cand.json --rows-out $RUNNER_TEMP/rows.json
          --summary $GITHUB_STEP_SUMMARY   (refusals, cap, pending and unchanged posts)
       d. encrypt each row with openssl and the key FACTORY_CI_BLOB_KEY (FACTORY_CI_BLOB_KEY_DOCKER for `allow_docker` rows, R5); write `matrix`, `matrix_docker`,
          `count`, `count_docker`, `fail_tick` to $GITHUB_OUTPUT; delete the plain rows
     env: FACTORY_CI_TOKEN, FACTORY_CI_TOKEN_DOCKER, BWS_ACCESS_TOKEN, BWS_ACCESS_TOKEN_DOCKER,
          FACTORY_CI_BLOB_KEY, FACTORY_CI_BLOB_KEY_DOCKER, B_PRIVATE (github.event.repository.private), HAVE_BWS,
          FACTORY_CI_BWS_PROJECT, FACTORY_CI_BWS_PROJECT_DOCKER,
          FACTORY_CI_BWS_SERVER_URL, FACTORY_CI_CLONE_IMAGE, FACTORY_CI_DIND_IMAGE
  outputs: matrix, matrix_docker, count, count_docker, fail_tick

run          needs plan; if: github.ref == 'refs/heads/main' && needs.plan.outputs.count != '0'
             environment: factory-ci; 45 min; contents: read
  strategy: { fail-fast: false, max-parallel: 4, matrix: ${{ fromJSON(needs.plan.outputs.matrix) }} }
  name: item ${{ matrix.n }}             (FR-058; n is unique across run and run-docker)
  1. checkout B (scripts)
  2. guard + fetch runner (as plan); install bws when HAVE_BWS == 'true'
  3. step id=job: factory-ci-job.sh
     env: ITEM_BLOB (matrix.blob), FACTORY_CI_BLOB_KEY, FACTORY_CI_TOKEN, BWS_ACCESS_TOKEN, HAVE_BWS,
          B_PRIVATE, vars.FACTORY_CI_LOG_LINK, FACTORY_CI_BWS_PROJECT, FACTORY_CI_BWS_SERVER_URL,
          FACTORY_CI_CLONE_IMAGE, FACTORY_CI_DIND_IMAGE
  4. if: always() && steps.job.outcome != 'success': factory-ci-status.sh (decrypts the blob, posts `error`
     with the reason from $RUNNER_TEMP/fci-reason or "job did not finish")
     env: ITEM_BLOB, FACTORY_CI_BLOB_KEY, FACTORY_CI_TOKEN

run-docker   needs plan; if: github.ref == 'refs/heads/main' && needs.plan.outputs.count_docker != '0'
             environment: factory-ci-docker; same steps and limits as run, with these differences:
  matrix: ${{ fromJSON(needs.plan.outputs.matrix_docker) }}
  env maps BWS_ACCESS_TOKEN_DOCKER to BWS_ACCESS_TOKEN, FACTORY_CI_TOKEN_DOCKER to FACTORY_CI_TOKEN,
  FACTORY_CI_BWS_PROJECT_DOCKER to FACTORY_CI_BWS_PROJECT, FACTORY_CI_BLOB_KEY_DOCKER to FACTORY_CI_BLOB_KEY (the `run`
  job never holds the docker key and `run-docker` never holds the plain key, R5); the wrapper adds --allow-docker.

verdict      needs [plan, run, run-docker]; if: always(); environment: factory-ci-plan; 5 min
  1. fail when needs.plan.result != 'success' or needs.plan.outputs.fail_tick == '1'
  2. ping: if github.ref == 'refs/heads/main' and FACTORY_CI_HEARTBEAT_URL is set, one GET (set +x, output
     discarded, failure ignored)                                                (FR-056)
```

`verdict` always runs, so every tick that ends pings, whether it had work or not. Its failure rule is the only
source of a red tick besides a failed `plan` and a failed matrix job. `fail_tick` is `1` when the plan found
problems and the tick started in the first 10 minutes of a UTC hour (A12), so a persistent repo error mails at most
about twice an hour. A dispatch of another ref skips `plan` (`result` is `skipped`), so `verdict` fails. How the
environment's branch policy treats `verdict` on such a ref is unverified (T065).

Blob (FR-059): each row is JSON; the plan wrapper encrypts it with
`openssl enc -aes-256-cbc -pbkdf2 -iter 600000 -salt -a -A -pass env:FACTORY_CI_BLOB_KEY` and the job wrapper decrypts it
with `-d`; the key must be at least 32 characters and the guard step refuses a shorter one (R6). The ciphertext is base64 of random-looking bytes, so it contains neither the key nor any text GitHub
masks, and GitHub does not drop the output. The matrix carries only `{"n": <int>, "blob": "<base64>"}`.

Scripts (`research.md` D9, D12): `factory-ci-plan.sh`, `factory-ci-job.sh` and `factory-ci-status.sh` source the
libraries `factory-ci-secrets.sh` and `factory-ci-blob.sh`. Each starts with `set +x`, never prints a decrypted
field, a reason, a name or a secret to stdout, and writes reasons only to files. The job wrapper blocks
`169.254.169.254` and `168.63.129.16` in `DOCKER-USER` before any container starts. It adds `--quiet-logs` iff the log
credentials resolved, `--private` iff the row is `fail_closed`, `--retry` iff the row is a retry.

`HAVE_BWS` is `${{ secrets.BWS_ACCESS_TOKEN != '' }}` in `env:`; scripts test it in `run:`. No `if:` uses `secrets.`.

Rules checked by `.github/scripts/factory-ci-lint-workflow.sh` in the self-test:

- no `schedule:`; `workflow_dispatch:` present; the concurrency group and `cancel-in-progress: false`;
- every `runs-on` is `ubuntu-24.04`; `max-parallel: 4` on both matrix jobs; no `${{` inside any `run:` block;
- every job except `verdict` has `if:` containing `github.ref == 'refs/heads/main'` and an `environment:`;
- no `if:` contains `secrets.`; no `toJSON(secrets)` anywhere;
- the only `matrix.` references are `matrix.n` (job name) and `matrix.blob` (the step `env:`); no `ITEM_` name other
  than `ITEM_BLOB` appears in any `env:`;
- the guard step comes before the plan step; `verdict` has `if: always()`;
- every `uses:` is pinned to a 40-hex commit;
- `BWS_ACCESS_TOKEN` appears only in `env:` of the plan step and the `run` job; the `run-docker` job mentions only
  `BWS_ACCESS_TOKEN_DOCKER` and `FACTORY_CI_TOKEN_DOCKER` and never the non-docker names;
- no `sm-action`; every download is followed by `sha256sum -c`; the install checks `unzip -Z1` lists exactly `bws`;
- the library's `BWS_SHA256` placeholder is tolerated unless `FACTORY_CI_LINT_STRICT=1` (the release gate sets it
  once T054 has pasted the real value);
- the fallback step reads `fci-reason`; the matrix steps print no `ITEM_`-derived value.

Review round 1 additions (research D12): the lint also checks that `FACTORY_CI_BLOB_KEY_DOCKER` appears only in the
`plan` step and the `run-docker` job, `FACTORY_CI_BLOB_KEY` only in `plan` and `run`, that no job has
`packages: read`, and that the README states `RUNNER_IMAGE_DIGEST` as `sha256:<64 hex>`. The guard step also fails when
a blob key is shorter than 32 characters. The closing `error` the plan posts for an old marked `pending` reads
"job did not finish (retry)".

Review round 2: the Loki URL held in the store must be `https`, except plain `http` to `127.0.0.1`, `localhost` or `[::1]`
(the test fake). The exception ships in the production binary (accepted low risk L-b, `research.md` D12).
