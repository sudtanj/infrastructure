# Contract: `plan` phases and output

`plan` runs in two phases so that a tick with nothing to run never calls the secret store (FR-045).

## Phase A: candidates (reads only)

`rust-ci-runner plan --repos FILE --rows-out CAND`

Reads the watch list and GitHub. Writes no status, makes no store call, prints nothing about branches, SHAs or
projects. `CAND` is a private JSON file (the workflow keeps it in `$RUNNER_TEMP`) with: `rows` (every item that needs
a run, uncapped, with the fields below), `unchanged` (the "unchanged since" posts to make), `invalid` (folder-name
error posts), `problems`, `unchecked`, and the number of repos checked. Exit 1 only when nothing could run: invalid
watch list, partial log config, missing token, unreadable file.

## Phase C: refusals, cap, posts

`rust-ci-runner plan --rows-in CAND --rows-out ROWS --summary FILE [--cap N] [--now EPOCH]`

1. If B is public (`B_PRIVATE=false`) and no log store resolved (the three `FACTORY_CI_LOG_*` values are absent),
   each private-repo row becomes a problem and is dropped (FR-049).
2. Order and cap: within a repo, oldest commit first, then branch, then project; across repos interleaved
   round-robin, starting with the repo whose oldest row is oldest; at most `cap` rows (20). Assign `n` = 0, 1, 2, ...
   across the whole capped list, `run` and `run-docker` rows together.
3. Writes, at most 60 per tick, paced at 750 ms: `pending` for each row first, with no link (description `queued`, or
   `queued (retry)` for a retry), then "unchanged since" posts, then folder-name and closing `error` posts. A row whose
   `pending` post fails is dropped with a problem. What does not fit waits for the next tick.
4. `ROWS` is a private JSON file with the final rows. It is never written to `$GITHUB_OUTPUT` or printed.
5. `--summary` receives Markdown: one line per row, `item N: owner/name` (repo name only, FR-058), then counts
   (checked repos, planned, unchanged, waiting, writes deferred), problems and unchecked repos. Every string passes
   the sanitizer and the redactor. No branch, SHA, project path or secret name.
6. `fail_tick` is written to the stdout of phase C as the single line `fail_tick=1` or `fail_tick=0` (the wrapper
   reads it): `1` iff there are problems or unchecked repos and `--now` falls in the first 10 minutes of a UTC hour
   (A12). The summary lists problems on every tick regardless.
7. Heartbeat line to the log store, if the credentials resolved (FR-056). The ping is sent by the `verdict` job.

Exit status of phase C: 0 when it ran. 1 on a fatal error (nothing posted).

## Row fields

```json
{
  "repo": "sudtanj/invoices",
  "repo_id": "123456789",
  "sha": "<40 hex>",
  "branch": "main",
  "project": "go-invoices",
  "context": "factory-ci/go-invoices",
  "prev_sha": "<40 hex or empty>",
  "allow_docker": false,
  "fail_closed": false,
  "retry": false,
  "secrets_ok": true,
  "secrets": "SHARED_KEY OTHER",
  "run_id": "<16 hex>",
  "n": 3
}
```

- `project` is `.` for the root. `repo_id` is the entry's verified id, or empty (then the prefixed keys are not
  used). `fail_closed` is true for a private repo on a public B: the job runs with `--private` (FR-059).
- `retry` is true when this row spends the retry budget (A6c). The job adds `--retry`, which appends " (retry)" to
  every description it posts.
- `secrets_ok` is true when the row's branch matches the entry's `secret_branches` (default: the repo's default
  branch). When false, a step that lists a secret ends in `error` (FR-060).
- `secrets` holds the entry's shared key names, space-separated, possibly empty.

## What the workflow sees

The plan wrapper turns each row into `{"n": N, "blob": "<base64 openssl ciphertext>"}` and writes
`matrix={"include":[...]}` for rows without `allow_docker`, `matrix_docker={"include":[...]}` for the rest,
`count`, `count_docker` and `fail_tick` to `$GITHUB_OUTPUT`. See `tick-workflow.md`. Nothing else leaves the plan
step.

Review round 1 (research D12): when several branches point at one commit, the row's `branch` is the default branch if it
is among them, else the alphabetically first; `secrets_ok` is true if any of them matches `secret_branches` (R2). An
`allow_docker` entry with no `FACTORY_CI_TOKEN_DOCKER` is a problem and produces no row (R3).
