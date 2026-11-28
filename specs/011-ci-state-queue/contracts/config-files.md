# Contract: files and command lines

## Workflow shape

Unchanged. Neither `factory-monorepo-ci.yml` nor `factory-ci-tick.yml` nor any script changes. The tick runs `plan` and `job` from `vars.RUNNER_IMAGE_DIGEST`; the manual workflow runs its pinned 0.3.0 image. A Rust test reads both files and compares `timeout-minutes` of the manual `ci` job and of the tick `run` job (45) with `JOB_LIMIT`.

## `rust-ci-runner plan` (changed behaviour, same flags)

Phase A reads, for a private repo with `allow_docker = false` and `secret_branches` exactly `["main"]`, each non-`docker/` project's `.factory-ci.toml` at the tip of `main` (one blob each, 64 KiB limit, at most 40 per tick) and the ledger of the repo (release `ci-sched+<owner>_<name>` in `<owner>/factory-ci-state`, with `FACTORY_CI_TOKEN`, read only). It adds scheduled rows (`prev_sha == sha`, `secrets_ok = true`). Phase C posts no `pending` for them. `secrets_ok` of every row is `store_allowed || image rule` (feature 012). `plan --check` accepts `secret_branches = ["main"]` and still refuses `state` and `vault_items` keys.

## `rust-ci-runner job` (changed behaviour, same flags)

Prepare mode (`--names-file`, `--reason-file`) writes the names of D7 (declared names on `main` only, 012 names, the runner-only names for `main` rows that need the store). The real run takes those variables once and removes them, then follows D7. A scheduled row (`--prev` equal to `--sha`) ends silently with exit 0 when nothing is due or the claim is lost. Exit 0 still means the status was delivered, whatever its state. There is no new flag.

## `rust-ci-runner ci` (manual, frozen pin)

```
ci --repos FILE --repo OWNER/NAME --ref REF [--project PATH] --work DIR [--allow-docker]
```

No `--entry` flag. A pipeline that declares `[state]`, `[artifacts]`, `[schedule]`, `state = true` or a vault list ends `[runner] state not allowed` before any store call or container (0.4.x). The pinned 0.3.0 refuses those keys itself. Otherwise unchanged.

## `rust-ci-runner decrypt-artifact`

stdin: an `FCI1` artifact file. stdout: the tar. The key comes from `FACTORY_CI_ARTIFACT_KEY` in the process environment only. Any failure prints one generic line on stderr and exits non-zero; bytes already written are unverified until the exit code is 0.

## Watch entry

No new key. `secret_branches = ["main"]` (exactly one literal item) on an entry of a private repo grants the tick's store features on `main`. See data-model.md.

## Pipeline file

`[schedule]`, `[state]`, `[artifacts]`, step keys `state = true` and `vault = [{ item, field, env }]`. See data-model.md. A vault entry has exactly the three string keys. `field` is `username`, `password`, `uri` or the exact name of a custom field. In 0.4.0 a pipeline whose vault entries pass validation ends `vault unavailable`.

## Bitwarden keys for the tick

In the plain lane's project (`FACTORY_CI_BWS_PROJECT`), per watched repo: `<repo><SEP>FACTORY_CI_STATE_TOKEN`, `<repo><SEP>FACTORY_CI_STATE_KEY`, `<repo><SEP>FACTORY_CI_ARTIFACT_KEY` and, for 0.4.1, the five vault keys. `<SEP>` is `/` unless the store refused it. The entry needs its numeric `id` (`factory-ci-secrets.sh:71`).

## Files that do not change

Nothing under `.github/workflows/` or `.github/scripts/`. `.github/factory-ci-repos.toml` changes once, after the tick bump (T030).
