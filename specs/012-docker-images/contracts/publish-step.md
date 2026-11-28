# Contract: the built-in publish step and the names-file hand-off

## 1. Names file (prepare mode, `rust-ci-runner job --names-file F --reason-file R`)

The frozen wrapper `.github/scripts/factory-ci-job.sh` reads `F` (lines 60-66). Each line must match `^[A-Za-z_][A-Za-z0-9_]*$`.

| Condition | Content of `F` |
|---|---|
| project is `docker/<image>`, branch is `main`, `image::check` returns no id | `DOCKERHUB_TOKEN\n` |
| anything else, including a failing check | empty |

The prepare run never fails on a check; the real run reports it as `failure`. A pipeline that lists any secret still ends prepare with `NO_JOB_SECRETS` (`job.rs:615-617`) before this table applies.

The wrapper then needs, and the plan row provides:

- `secrets_ok == true` (`job.sh:69`). `plan.rs` sets it from `image::publishes(project, branch)` and `false` otherwise.
- a non-empty `repo_id` so the key `sudtanj/infrastructure/DOCKERHUB_TOKEN` is looked up (`factory-ci-secrets.sh:71`). The entry in `.github/factory-ci-repos.toml` has `id`.
- `BWS_ACCESS_TOKEN` in the step environment (`factory-ci-tick.yml:292`).

Failures the wrapper reports itself, as fixed commit-status lines: `secret DOCKERHUB_TOKEN not found`, `secret DOCKERHUB_TOKEN is ambiguous`, `secret store rejected the token`, `secret store unreachable`.

The real run's environment then holds `DOCKERHUB_TOKEN` (and nothing of the store token, `job.sh:115`).

## 2. Publish function

`publish::publish(engine, env, api, target, dir, log) -> Done`

Preconditions, in this order, each a fixed line (data-model.md "Status texts"):

1. `publishes(project, branch)`; else return `success` `built and checked, not published` and read no variable, make no engine call, make no API call.
2. `image::check` again on the files now on disk; a failing id gives `failure` `check failed: ...`.
3. `var(env, "DOCKERHUB_TOKEN")` present and non-empty.
4. Both archives of `PLATFORMS` exist in `dir`, each a regular file (`symlink_metadata`), size 1 byte to `ARCHIVE_MAX`.
5. Tip read 1: `api.branches(repo)`, the entry named `main`. Error or no entry gives `error` `tip of main unreadable` and no container starts.

Then container A. Then, only when `tip1 == sha`, tip read 2 (same call); error gives `error` `tip of main unreadable` with the `sha-` tag in place; `tip2 == sha` runs container B. Otherwise `success` `published sha-<short>, not latest`.

### Container A, platform images and the list (`Engine::capture`)

```
docker run --rm --name factory-ci-<run>-publish --label factory-ci.run=<run>
  --network factory-ci-<run> --read-only --tmpfs /tmp:rw,size=64m --cap-drop ALL
  --security-opt no-new-privileges --pids-limit 256 --memory 1g
  --user <uid>:<gid> -v <dir>:/in:ro
  -e HOME=/tmp -e DOCKER_CONFIG=/tmp/.docker
  -e REPO=index.docker.io/sudtanj/<name> -e TAG=sha-<short>
  -e DOCKERHUB_TOKEN
  <CRANE_IMAGE> sh -c '<script A>'
```

`-e DOCKERHUB_TOKEN` carries no value in argv. The value travels in the environment of the engine client (`Engine::command(args, env)`, `engine.rs:173-186`). The run uses `capture` (stdout returned, 10-minute limit, container killed on timeout). Its stdout is parsed and never logged.

Script A, in order:

1. Log in with `--password-stdin` from `$DOCKERHUB_TOKEN`, user `sudtanj`. A failure exits 41.
2. `crane manifest "$REPO:$TAG"`. Success: go to step 6 (reuse). Output containing `MANIFEST_UNKNOWN` or `NAME_UNKNOWN`: continue. `UNAUTHORIZED` or `DENIED`: exit 41. Anything else: exit 42.
3. For `arch` in `amd64`, `arm64`: `d=$(crane digest --tarball /in/image-$arch.tar)` (failure exits 44); `crane push /in/image-$arch.tar "$REPO@$d"` (output with `UNAUTHORIZED` or `DENIED` exits 41, other failure exits 42); `crane config "$REPO@$d"` must show `"os":"linux"` and `"architecture":"$arch"` (else exit 44).
4. `crane index append -t "$REPO:$TAG" -m "$REPO@$d_amd64" -m "$REPO@$d_arm64"`. 401 exits 41, other failure exits 42.
5. (no other tag is written anywhere in steps 3 and 4.)
6. `m=$(crane manifest "$REPO:$TAG")`, then print one line, `INDEX ` followed by `m` with newlines removed. Nothing else is printed. crane's own output goes to `/dev/null`.

The runner takes the last stdout line that starts with `INDEX `, runs `publish::verify_index` on the JSON, and ends `error` `image index incomplete` when it fails or the line is absent.

### Container B, tag latest (`Engine::run_logged`, `ship = false`)

Same flags, `--name factory-ci-<run>-tag`, no `/in` mount, script B: login as in A, `crane tag "$REPO:$TAG" latest`, exit 41 or 42 as in A. It prints nothing.

### Exit mapping

| Exit | Status |
|---|---|
| 0 | continue |
| 41 | `error` `registry refused the token` |
| 44 | `error` `image platform mismatch` |
| 42, any other code, timeout, spawn failure | `error` `push failed` |

`capture` reports a non-zero exit as `exit status: <n>` in its error; the mapping matches that text, and a test pins it.

## 3. Redaction

Every `Redactor` that can see the run includes `DOCKERHUB_TOKEN` and the base64 of `sudtanj:<token>` (the form a docker config holds). That covers the `creds` passed to `post` (`job.rs:727`) and the log built in `attempt` (`job.rs:520-530`).

## 4. What a user-written step sees

Unchanged. `run_step` (`engine.rs:487-553`) passes only run-wide env, the step's own `env` and its listed `secrets`. `DOCKERHUB_TOKEN` is in none of them. A pipeline that lists it as a secret is refused by `NO_JOB_SECRETS` and by `image::check` (`pipeline-secrets`).

## 5. `verify_index` (pure)

`verify_index(json: &str) -> Result<(), &'static str>`. Accepts only the shape in `data-model.md` "Tags and the manifest list". Rejects: not JSON; no `manifests`; a single image manifest; one entry; three entries; two `amd64`; a missing `arm64`; an `unknown/unknown` attestation entry; a malformed digest; a missing `platform`.
