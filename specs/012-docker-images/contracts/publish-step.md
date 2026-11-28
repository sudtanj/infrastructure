# Contract: the built-in publish step and the names-file hand-off

## 1. Names file (prepare mode, `rust-ci-runner job --names-file F --reason-file R`)

The frozen script `.github/scripts/factory-ci-job.sh` reads `F` (lines 60-66). Each line must match `^[A-Za-z_][A-Za-z0-9_]*$`.

| Condition | Content of `F` |
|---|---|
| project is `docker/<image>`, branch is `main`, `image::check` returns no id | `DOCKERHUB_USERNAME\nDOCKERHUB_TOKEN\n` |
| anything else, including a failing check | empty |

The prepare run never fails on a check; the real run reports it as `failure`. A pipeline that lists any secret still ends prepare with `NO_JOB_SECRETS` (`job.rs:615-617`) before this table applies.

The script then needs, and the plan row provides:

- `secrets_ok == true` (`job.sh:69`). `plan.rs` sets it from `image::publishes(project, branch)` and `false` otherwise.
- the shared names in the row: the row field `secrets` is the entry's `secrets` joined by spaces (`plan.rs:311`), and `fci_lookup` resolves a bare key `NAME` only when the field lists it (`factory-ci-secrets.sh:72-75`, `job.sh:77`). The entry therefore carries `secrets = ["DOCKERHUB_USERNAME", "DOCKERHUB_TOKEN"]`. The repo-own key form `sudtanj/infrastructure/NAME` is tried first and is absent, so the shared key is used. Each key must exist once in the project; a line under 4 characters is refused by the script (`job.sh:81`), the username has 7.
- the docker store access token in the step environment (`factory-ci-tick.yml:292`) and the project in `FACTORY_CI_BWS_PROJECT` (`factory-ci-tick.yml:296`).

Failures the script reports itself, as fixed commit-status lines: `secret <NAME> not found`, `secret <NAME> is ambiguous`, `secret <NAME> has a line under 4 characters`, `secret store rejected the token`, `secret store unreachable`.

The real run's environment then holds `DOCKERHUB_USERNAME`, `DOCKERHUB_TOKEN` and nothing of the store token (`job.sh:115`). `Engine::command` clears the environment of every child (`engine.rs:173-186`), so no container the runner starts inherits it.

## 2. Publish function

`publish::publish(engine, env, api, target, dir, log) -> Done`

Steps, in this order, each failure a fixed line (data-model.md "Status texts"):

1. `publishes(project, branch)`; else return before any env read, engine call or API call.
2. `image::check` again on the files now on disk; a failing id gives `failure` `check failed: ...`.
3. `var(env, "DOCKERHUB_TOKEN")` present and non-empty, and `var(env, "DOCKERHUB_USERNAME")` present (`publish refused: no login name`) and equal to `NAMESPACE` (`publish refused: login is not the namespace`; no value printed). Both are kept in memory; the username is used only for this comparison and the login and is never registered with a redactor.
4. Both archives of `PLATFORMS` exist in `dir`, each a regular file (`symlink_metadata`), size 1 byte to `ARCHIVE_MAX`.
5. `archive::read_config` of each archive; user, entrypoint and platform rules of data-model.md "Archive config".
6. Tip read 1: `api.branches(repo)`, the entry named `main`. Error or no entry gives `tip of main unreadable`, nothing started.
7. Teardown: `engine.cleanup_run(run_id)` stops and removes the sidecar, every container of the run, the run's networks and its volumes (`engine.rs:240-266`). Then `engine.run_left(run_id)` must be false, else `publish refused: sidecar still running`.
8. `hub::ensure_public` (contracts/hub-api.md): login, probe, create a missing repository as public, verify. Runs in the runner process; its errors end the item with its fixed lines before any publish container starts.
8b. Fresh directory under `RUNNER_TEMP`, mode 0700, new name. Move both archives into it (`rename`; a copy when `rename` fails). It holds nothing else.
9. Container A. Then, only when tip 1 equals the commit, tip read 2; error gives `tip of main unreadable` with the `sha-` tag in place; equal runs container B. Otherwise `published sha-<12>, not latest`.
10. Remove the fresh directory. The caller's final `cleanup_run` removes the publish containers.

### Containers A and B: the token is on standard input

Both are started with `Engine::capture_in(args, env, input, limit, container)` where `input` is the token bytes (no newline), `env` is empty, limit 10 minutes.

```
docker run -i --rm --name factory-ci-<run>-publish --label factory-ci.run=<run>
  --read-only --tmpfs /tmp:rw,size=64m --cap-drop ALL
  --security-opt no-new-privileges --pids-limit 256 --memory 1g
  --user <uid>:<gid> -v <fresh dir>:/in:ro
  -e HOME=/tmp -e DOCKER_CONFIG=/tmp/.docker
  -e REPO=index.docker.io/sudtanj/<name> -e TAG=sha-<12 hex>
  <CRANE_IMAGE> sh -c '<script A>'
```

The container uses the default bridge network (the run network is gone after teardown). `--user` is the uid and gid of the fresh directory. No `-e DOCKERHUB_TOKEN`, no value anywhere in argv or any environment. Container B is the same with `--name factory-ci-<run>-tag`, no `-v`, `<script B>`.

Script A, in order:

1. `crane auth login index.docker.io -u sudtanj --password-stdin` (reads the token from standard input); failure exits 41.
2. `d_amd64=$(crane digest --tarball /in/image-amd64.tar)` and the same for arm64; failure exits 43.
3. `m=$(crane manifest "$REPO:$TAG" 2>&1)`. Success: take the child digests from `m`, compare as a set with the two digests of step 2; unequal exits 45; equal skips to step 6. Output with `MANIFEST_UNKNOWN` or `NAME_UNKNOWN`: continue. `UNAUTHORIZED` or `DENIED`: exit 41. Anything else: exit 42.
4. For each architecture: `crane push /in/image-<arch>.tar "$REPO@$d_<arch>"` (by digest, no tag). `UNAUTHORIZED` or `DENIED` exits 41; any other failure exits 42.
5. `crane index append -t "$REPO:$TAG" -m "$REPO@$d_amd64" -m "$REPO@$d_arm64"`; 41 or 42 as above. No other tag is written in steps 4 and 5.
6. `m=$(crane manifest "$REPO:$TAG")`, then print one line, `INDEX ` followed by `m` with newlines removed. Nothing else is printed; crane's own output goes to `/dev/null`.

Script B: `crane auth login ... --password-stdin` (41 on failure), `crane tag "$REPO:$TAG" latest` (41 or 42). It prints nothing.

The runner takes the last stdout line of A that starts with `INDEX `, runs `publish::verify_index` on the JSON, and ends `image index incomplete` when it fails or the line is absent.

### Exit mapping

| Exit | Status |
|---|---|
| 0 | continue |
| 41 | `registry refused the token` |
| 45 | `existing tag differs from this build` |
| 42, 43, any other code, timeout, spawn failure | `push failed` |

`capture_in` reports a non-zero exit as `exit status: <n>` in its error; the mapping matches that text and a test pins it.

## 3. Redaction

Every `Redactor` that can see the run includes `DOCKERHUB_TOKEN`, the base64 of `sudtanj:<token>` and the Hub login access token. It does not include `DOCKERHUB_USERNAME`: a username is not confidential, and registering a 7-character value would only blank `sudtanj` from statuses. The script does mask it in the Actions log (the form a docker config holds). That covers the `creds` passed to `post` (`job.rs:727`) and the log built in `attempt` (`job.rs:520-530`).

## 4. What a user-written step sees

Unchanged. `run_step` (`engine.rs:487-553`) passes only run-wide env, the step's own `env` and its listed `secrets`. `DOCKERHUB_TOKEN` is in none of them. A pipeline that lists it as a secret is refused by `NO_JOB_SECRETS` and by `image::check` (`pipeline-secrets`).

## 5. `verify_index` (pure)

`verify_index(json: &str) -> Result<(), &'static str>`. Accepts only the shape in `data-model.md` "Tags and the manifest list", with either media type. Rejects: not JSON; no `manifests`; a single image manifest; one entry; three entries; two `amd64`; a missing `arm64`; an `unknown/unknown` attestation entry; a malformed digest; a missing `platform`.
