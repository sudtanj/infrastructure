# Research: Central Interval CI Runner

Decisions the spec left open, with the options rejected. The plan cites these by number.

## D1. Where the planning logic lives

**Decision**: a `rust-ci-runner plan --repos FILE` subcommand. It reads the watch list, calls the
GitHub REST API with `ureq` (already a dependency), and writes the matrix JSON and a summary to
files the workflow names.

**Why**: the planning is the hard part of this feature: a tree walk for projects, a status
lookup per context, a first-parent ancestor walk, changed-file ownership, a cap, an order, and
three kinds of API failure. All of it is a pure function over an `Api` trait. Tests feed it an
in-memory fake and cover every acceptance scenario of stories 2 to 6 without a network. The
same crate already has the JSON, TOML and HTTP pieces, so the cost is the logic only.

**Rejected**:

- Shell and `gh api` in the workflow. Pagination, rate-limit classification, the ancestor walk
  and path ownership become bash and `jq` that no test reaches without a live GitHub. The
  failure modes in this feature live in exactly that code.
- A separate small program in another language. A second toolchain and a second release for
  logic that belongs next to `job`.

## D2. Per-repo secret allowlist on GitHub Actions

**Amended by D12 (2026-11-27)**: GitHub secrets are no longer a secret source, there are no per-repo projects, the matrix carries an encrypted blob, and `plan` runs in two phases. Where this section differs, D12 wins.

**Facts checked in the code**:

- `engine::run_step` passes a secret to a container as `-e NAME` only for names the step lists,
  and only if the name is in `RunCtx.secrets`. The runner process environment is not
  forwarded: `docker run` copies a variable into the container only when `-e NAME` names it.
  `argv.rs` already asserts that tokens travel in the engine client's environment, not argv.
- `pipeline::valid_secret_name` reserves `FACTORY_CI*`, `GITHUB_*`, `RUNNER_*` and others, so
  `FACTORY_CI_TOKEN` and the three `FACTORY_CI_LOG_*` names can never be listed by a pipeline.
- A's code runs only inside step containers. No step on the runner host executes A's files:
  the checkout is made by the runner binary in a git container, and the host only reads
  `.factory-ci.toml` after the symlink check.

**Superseded in part by D9**: the names to export now come from the pipeline, and `ITEM_SECRETS` holds only the entry's shared names. The isolation mechanics below still hold.

**Decision**: one shell script, `.github/scripts/factory-ci-job.sh`, run as a single workflow
step. The step sets `ALL_SECRETS: ${{ toJSON(secrets) }}`, `ITEM_SECRETS: ${{ matrix.secrets }}`
(a space-separated list of names) and the other item fields in `env:`. The script:

1. checks every listed name against `^[A-Za-z_][A-Za-z0-9_]*$`;
2. exports the value of each listed name, taken from `ALL_SECRETS` with `jq`, matching the key
   exactly (D10, M3);
3. `unset ALL_SECRETS`, then `exec rust-ci-runner job ...`.

`exec` replaces the process image, so the full JSON is not in the environment of the runner
process, of the `docker` client it starts, or of any later step. No other step in the job sees
the secret values: the script does not write `GITHUB_ENV`. The three `FACTORY_CI_LOG_*` values
and `FACTORY_CI_TOKEN` are mapped by name in the same step's `env:` and are readable only by the
runner binary.

**Residual risk**, stated in the README and the plan:

- `toJSON(secrets)` puts every repository secret in this one step's environment for the
  duration of the script (milliseconds). The script is B's own code at the dispatched ref.
- The GitHub runner agent holds every secret of the job in memory so it can mask them. A step
  with `docker = true` runs against a privileged `docker:dind` container. Code that escapes it
  reaches the runner VM as root and can read the agent's memory: all of B's secrets, including
  `FACTORY_CI_TOKEN`. So `allow_docker = true` means the repo's committers get all of B's
  secrets. The same holds for a kernel escape from a plain step container, with a lower chance.
  Keep B's secret set small and set `allow_docker` only on repos you trust that far (spec A13).
- An environment variable over about 128 KB cannot be passed to `exec`. If `toJSON(secrets)`
  grows past that, the step fails before any container starts and the item ends `error`.
- Trailing newlines of a secret value are dropped by shell command substitution. Inner
  newlines (PEM keys) survive.

**Rejected**:

- `secrets: inherit` into a reusable workflow, or mapping `secrets.NAME` statically per name:
  GitHub offers no dynamic name lookup in YAML, and static mapping puts every name in every job.
- GitHub Environments, one per watched repo: a job with an environment still sees all
  repository-level secrets, so the allowlist would not bound anything unless the owner keeps
  all secrets at environment level. That is a manual rule outside the list file, and it drifts.
- Writing the filtered values to `GITHUB_ENV`: they would reach every later step, including the
  fallback status step, for no gain.
- A `--secrets-file` option on the runner: more Rust to avoid ten lines of shell.

## D3. API budget

Calls are REST. Per branch tip: one tree call (the recursive tree of the commit, to find
`.factory-ci.toml` files) and one combined-status call. Per repo: one branch list.

| Tick | Calls | Count for 10 repos x 5 branches x 3 projects |
|---|---|---|
| Idle | 10 branch lists + 50 trees + 50 statuses | **110**, or **120** with the repo lookup of D10 M4 (SC-005 limit: 150) |
| One new tip in each of 3 repos, 1 of 3 projects changed | 110 + per tip: 1 commit list, 1 ancestor status, 1 compare = 9 reads | 119 reads |
| Writes in that tick | per tip: 1 pending + 2 unchanged = 9, plus 3 final posts from the jobs | 12 |
| First tick for a repo whose 5 tips share history | 10 tree and status calls, 5 commit lists, up to 50 ancestor statuses (cached by sha across tips) | about 70 per repo |

Tips that share a commit are fetched once. Idle ticks write nothing. At a 5-minute interval an
idle day costs 12 x 120 = 1440 requests an hour of the 5000 an hour a fine-grained token gets.
The limit of 150 is met up to 13 repos of 5 tips; the plan says so in the README.

**Rate-limit handling**: every response carries `x-ratelimit-remaining`. `HttpApi` keeps the last
value. A read call made while the value is below `RATE_RESERVE` (60) returns `RateLimited`
without sending, so the reserve is left for writes (pending, unchanged, final posts). A 429, or
a 403 with `x-ratelimit-remaining: 0` or a `retry-after` header or a body naming a secondary
limit, also returns `RateLimited`. Writes ignore the reserve and, on a rate-limited response, wait 30 s once,
then defer the remaining writes to the next tick.

**Unchanged statuses**: written once per (commit, project), at plan time, never per tick. After
the post the context has a status on that commit, so FR-009 counts it as done and the next tick
does not touch it. No GitHub endpoint writes several statuses in one request, so there is
nothing to batch. Writes are paced at 750 ms (D10) to stay under the secondary limit of 80
content-creating requests a minute.

**Per-repo caps**: more than 100 matching branches is a repo error ("narrow `branches`"). A
branch list or tree that GitHub reports as truncated is a repo error. Both stop one repo from
spending the budget of the others.

**Rejected**: a GraphQL query per tip (history plus statuses in one call) would cut the first-tick
cost, but it adds a second client and query format for a cost that is paid once per repo. Revisit
if first ticks hit the limit. Conditional requests with `If-None-Match` would make idle 304s free
but need state between ticks, which the design does not have.

## D4. Status ordering and stale pending

**Ordering inside one tick**: the matrix jobs `need` the plan job, and the plan job posts
`pending` before it writes the matrix output. A job's final status therefore always comes after
its `pending`, by at least the time a runner takes to start. GitHub resolves the latest status
per context by creation order, so the final wins.

**Across ticks**: one workflow-level concurrency group (`factory-ci-tick`, no cancel) means a
tick's plan job starts only after every job of the previous tick ended. Two ticks never post
for the same item at once, so the race the spec worries about cannot occur between ticks. GitHub
keeps one queued run per group and cancels older queued runs. That is the behaviour of story 5
scenario 4. A cancelled queued run shows as cancelled in the Actions list, with no failure notice.

**Late final from a dead job**: a job has `timeout-minutes: 45`, far under the stale limit of 2
hours. A job cannot post a final status after a later tick re-planned its item. If it ever did,
both statuses are real results for the same commit and the later one wins.

**Stale pending**: a combined-status entry in state `pending` whose `updated_at` is more than
7200 s before the plan's clock counts as no status. The item is planned again, a new `pending` is
posted (update 2 of at most 3 per commit and context, spec edge case), and the job posts the
final. A pending whose `updated_at` cannot be parsed counts as stale, so a bad value never blocks
a commit. Clock skew of seconds against a 2-hour limit does not matter.

**Pending is the gate**: an item runs only if its `pending` post succeeded. If the post fails
(no write access, rate limit), the item is dropped from the matrix, the tick records the
problem and fails, and the next tick plans it again. A job never runs without a `pending` on
the commit.

**Long ticks**: 20 items at 4 in parallel is 5 waves of up to 30 minutes plus clone and flush.
An item in the last wave has a `pending` older than 2 hours when its job starts. No other tick
plans meanwhile, so the stale rule never fires on it.

## D5. Watch list file

Location `.github/factory-ci-repos.toml`, next to the workflows and covered by the constitution's
rule that dot-directories hold repo tooling. Format TOML, because `toml` is already a dependency
and the pipeline file uses it. Full rules in `contracts/watch-list.md`. The file is validated in
pull requests by `rust-ci-runner plan --repos FILE --check`, which needs no token and no network.

**Rejected**: a repository variable holding the list (not versioned, no review, no diff);
JSON (no comments).

## D6. How a job gets its checkout and its secrets, and why `job` and not `run`

FR-015 says the matrix runs `rust-ci-runner run`. A job also has to clone with the token, enforce
the repo's allowlist and docker opt-in, ship logs, flush them, and post the final status. Putting
that in `run` would add network, tokens and the GitHub API to the local command, which story 8
says needs none. The plan adds a `job` subcommand that calls the same `run_pipeline` function as
`run`, so graph rules and executor are shared (SC-007) and the local command stays free of B's
concerns. A workflow fallback step posts `error` through a `status` subcommand if `job` did not
finish.

## D7. Log shipping

- **Transport**: `ureq` POST to `<url>/loki/api/v1/push`, JSON body, `Authorization: Basic`.
  `ureq` has no basic-auth helper, so `util.rs` gets a base64 encoder (about 15 lines, tested
  with the RFC 4648 vectors). No new dependency: no `base64` crate, no gzip crate. Uncompressed
  batches of at most 1 MB are within every Loki default.
- **Shipper**: one background thread per job, fed by a byte-bounded queue (32 MiB). A full queue
  drops the line and marks the job "logs not delivered", so steps never block on shipping.
- **Where it hooks in**: `log.rs`. `Log::out(who, stream, text)` is the one place step output
  passes: redact, then split into pieces of at most 16 KB at character boundaries, then the size
  cap, then print (unless quiet) and queue for shipping. Redaction runs on the whole line before
  the split, so a secret cannot straddle a 16 KB split. `Log::event(who, text)` is a fixed
  vocabulary line (`started`, `success in 12s`, `failed in 12s`, `skipped`, `timed out`) that is
  always printed and also shipped.
- **Metadata**: a fixed line prefix, `run=<run_id> sha=<8 chars> branch=<name, 40 chars max> `,
  not structured metadata. Structured metadata needs Loki 3 with schema v13 and
  `allow_structured_metadata`; a store without it rejects every batch with a 400. The prefix
  works on every Loki. It costs about 70 bytes a line. Switch to structured metadata if a store
  that supports it is the target and volume matters (spec A16).
- **Labels**: `repo` (`owner/name`), `project` (`.` for the root), `step`, `stream`. Clone and
  runner lines use `step="runner"`.
- **Timestamps**: runner clock, nanoseconds since the epoch, as a string.
- **NUL bytes** in a line are replaced by U+FFFD before shipping, so one binary line cannot make
  Loki reject its whole batch.
- **Run ID**: 16 hex characters, generated by `plan` per item and passed to `job` (it is assigned after the
  `pending` post, so the `pending` status has no link; only the job's final status carries the log link). Local `run`
  generates its own.
- **Link**: `FACTORY_CI_LOG_LINK` is a repository variable (not a secret: it is public in every
  status). It must contain `{run_id}`. When shipping is off, the link is the Actions run URL.
- **Quiet**: `--quiet-logs` sends `Log.out` to no sink on stdout. Workflow commands cannot be
  injected from step output because every printed line starts with `[`, never `::`.
- **Retry**: backoff 1, 2, 4, 8, 16 s (then 16 s) while the time since the batch's first
  attempt plus the next wait stays under 60 s. 401 and 403 set `stopped` for the job, so later
  lines are dropped at once. Other 4xx drop the batch. Per-request timeout 10 s.
- **Flush**: `Shipper::finish(60 s)` runs before the final status post and returns whether any
  batch or line was dropped.

## D8. Spec points that cannot be built as written

Recorded here and in the plan's risk list; the lead decides.

1. FR-002 and A10 say the caller's token lives only in B. It lives in the scheduler (cron-job.org
   or the Pi), by definition. The buildable rule: in no watched repo and in no B secret.
2. FR-012 and FR-014 conflict on the primary rate limit: posting `pending` is an API call. The
   plan keeps a read reserve and runs an item only if its `pending` post succeeded (D3, D4).
3. FR-010 says first-parent walk. The REST commit list is not first-parent. The plan fetches
   100 commits from the tip and follows `parents[0]` inside that window. A chain that leaves the
   window before 50 steps means "no ancestor found", so the project runs. Safe direction.
4. FR-015 and FR-025 name `run` for the job. The plan uses `job`, which shares the code (D6).
5. FR-030 and story 6 put private repo names, branches, SHAs and project paths in public places
   when B is public: matrix job names, the tick summary, the plan's stderr. The spec requires
   the repo name in the summary. The plan keeps that and says so in the README and the risks.
6. A6c makes `error` final. A transient runner fault (a Docker Hub pull limit, a network drop
   during clone) leaves the commit red until the next commit. The plan follows the spec and lists
   it as a risk. A later fix could treat an `error` older than N minutes like a stale pending.
7. Story 3 scenario 3 gives a folder project only its own directory. A project that needs files
   from a sibling folder or the repo root fails, by design.

## D9. Pipeline-declared secrets, fetched from Bitwarden Secrets Manager (story 12, FR-038 to FR-046, A19 to A24)

**Amended by D12 (2026-11-27)**: GitHub secrets are no longer a secret source, there are no per-repo projects, the matrix carries an encrypted blob, and `plan` runs in two phases. Where this section differs, D12 wins.

This section replaces the earlier D9 and changes D2: the names a job gets come from the project's
pipeline, not from the watch entry. The watch entry's `secrets` list now only grants shared keys and
GitHub secrets (FR-040).

**D10 amends this section** (exact-case lookup, environments, the `run`/`run-docker` split, short-line refusal, base64 masks).

**Decision**: the runner reads environment variables only. A shell library,
`.github/scripts/factory-ci-secrets.sh`, fills them. Two wrappers source it: `factory-ci-job.sh`
(matrix job) and `factory-ci-plan.sh` (plan job). Each ends in `exec` of the runner. No GitHub Action.

**Names come from the pipeline**: the runner prints them, so shell never parses TOML. `job` gets a
prepare mode: `job ... --names-file F --reason-file R`. It clones the commit, runs every check the
real run runs before any container (symlink, size, graph, step count, docker rule, and each step
secret name against `valid_secret_name`, which rejects non-env-var names and reserved names), writes
the union of the steps' `secrets` lists to `F` (sorted, one per line), and exits 0 without starting a
container. On any problem it writes one reason line to `R` and exits 1. An invalid pipeline therefore
ends the item in `error` with its reason before any fetch (US1 scenario 5). The prepare run is quiet:
it holds no log credentials yet, so its output goes nowhere; the real run clones again (the commit is
immutable) and ships the clone log. The cost is a second shallow clone.

**Source order per listed name `NAME` of repo `owner/name` (FR-040)**:

1. key `NAME` in the repo's own project, when the entry has `bitwarden_project`;
2. key `owner/name/NAME` in the default project (`FACTORY_CI_BWS_PROJECT`);
3. key `NAME` in the default project, only if the entry's `secrets` lists `NAME`;
4. GitHub secret `NAME` (from `toJSON(secrets)`), only if the entry's `secrets` lists `NAME`.

The first match wins. A key under another repo's prefix is never selected: the script looks keys up
by exact string, built from the item's own repo. With no `BWS_ACCESS_TOKEN`, only rule 4 applies
(FR-043). A name matching nothing fails the item. Bitwarden wins a clash with GitHub: when rule 1, 2
or 3 matched and the entry also lists `NAME` as a GitHub secret that exists, the name goes into
`FACTORY_CI_NOTICES` (name only). `job` writes those lines to its log through `Log::out`, so with
shipping on they stay out of the Actions log. Two projects of one repo share the prefix (accepted by
the spec). The trust boundary is this script, not Bitwarden: the machine account can read the whole
default project.

**Key separator (A24)**: `/`. A key name is free text in Secrets Manager and the listing returns
`key` verbatim, but I could not confirm in the docs that `/` is accepted: the research machine had
no network client. The separator is one constant, `FCI_KEY_SEP`, in the library; if `/` is refused,
set it to `:` (D11) and nothing else changes. Tests set it explicitly. T054 checks
by creating a key with `/` in a real project.

**Tool**: `bws` CLI from `bitwarden/sdk-sm` release `bws-v2.1.0`, asset
`bws-x86_64-unknown-linux-gnu-2.1.0.zip`, installed by a workflow step only when
`secrets.BWS_ACCESS_TOKEN` is non-empty. The step downloads the asset and
`bws-sha256-checksums-2.1.0.txt` from that release and runs `sha256sum -c`. The library also holds the
expected SHA-256 as a constant `BWS_SHA256`, copied once from the checksums file, and refuses to run if
it is a placeholder. Listing: `bws secret list <PROJECT_ID> -o json`, token in `BWS_ACCESS_TOKEN` for
that one call. A non-default server uses `bws config server-base` from the optional variable
`FACTORY_CI_BWS_SERVER_URL`. `bitwarden/sm-action` v3.0.1 is rejected: it takes secret IDs, not names,
and exports to the job environment.

**Job flow** (`factory-ci-job.sh`):

1. Validate `ITEM_*` fields and the project IDs (UUID).
2. Prepare: `./rust-ci-runner job ... --names-file F --reason-file R`. Failure: copy `R` to
   `$RUNNER_TEMP/fci-reason`, exit 1. No fetch happens.
3. If the token is set: list the item's own project (when set) and the default project, each at most
   once, with retry. Resolve every name by the order above. Exact `key` match; more than one match for
   the same key in one project is "secret NAME is ambiguous"; an empty value counts as no match.
4. Resolve remaining names from `ALL_SECRETS` under rule 4. A name in no source: reason
   "secret NAME not found", exit 1.
5. Resolve the three log values: default project key `FACTORY_CI_LOG_URL`, `_USER`, `_TOKEN` (no
   prefix), else GitHub secrets of those names (the log credentials are not subject to the entry's list).
   One or two of three: "log credentials incomplete".
6. Nothing is exported until every name resolved. Then mask each line of at least 4 characters of each
   value with `::add-mask::` before any other output, `export NAME=value` with shell builtins, `unset
   BWS_ACCESS_TOKEN ALL_SECRETS`, and `exec ./rust-ci-runner job ...` (the real run).

**Retry (FR-044)**: backoff 1, 2, 4, 8, 16 s until 60 s from a call's first attempt
(`FACTORY_CI_FETCH_WINDOW` shortens it in tests). 401 and 403 are not retried. `bws` may not set
distinct exit codes, so the script classifies by stderr text (`401`, `403`, `Unauthorized`,
`Forbidden`, `invalid access token`). That text is an unverified assumption; T054 checks it. A miss costs
60 s before `error`. Stderr is never printed.

**Failure reasons** (one fixed line in `$RUNNER_TEMP/fci-reason`, never a value, token or project ID):
the prepare reason, `secret store unreachable`, `secret store rejected the token`, `secret NAME not
found`, `secret NAME is ambiguous`, `log credentials incomplete`. The fallback step posts `error` with
it, or `job did not finish` when the file is missing. The runner never started.

**Planning check (FR-045)**: unchanged in shape. `factory-ci-plan.sh`: (1) `plan --check`; (2) with the
token set, `plan --list-projects` and one `bws secret list` per distinct entry project and for the
default project, with retry; a 401 or 403 fails with "secret store rejected the token", a project the
token cannot read fails naming the repo and project ID, an outage fails with "secret store
unreachable"; (3) merged log values for `plan`'s all-or-none check (the pending status carries no link); (4) `exec plan`.
Failures append their line to `$GITHUB_STEP_SUMMARY` and exit non-zero before `plan` starts: no status
is posted, the matrix is skipped, `verdict` fails the tick.

**Isolation**: the wrapper step env holds `toJSON(secrets)` and `BWS_ACCESS_TOKEN`, because GitHub is a
fallback source and the store needs its token. Both are unset before the real run. The prepare run
starts with only `FACTORY_CI_TOKEN`, which it needs to clone: the script runs it under `env -i` with
`PATH`, `HOME`, `FACTORY_CI_TOKEN`, `GITHUB_API_URL` and `RUNNER_TEMP`. The real run's environment holds
the item's fetched names, the log credentials and `FACTORY_CI_TOKEN`. A's code runs only in step
containers, which get `-e NAME` for the names their own step lists (FR-018, FR-020). Nothing writes
`GITHUB_ENV`.

**GitHub secret limit**: with names taken from pipelines and kept in Bitwarden, B's GitHub secrets are
`FACTORY_CI_TOKEN`, `BWS_ACCESS_TOKEN` and the optional caller token, far from the limit of 100. The
GitHub path remains for names an entry lists.

**Vendor-free runner (FR-038)**: the binary has no Bitwarden client. It knows the watch-list key
`bitwarden_project` (validated as a UUID, quoted when malformed, passed through as `bws_project`), the
reserved prefix `BWS_`, `plan --list-projects`, and the `job` prepare flags, none of which names a
vendor except the key and the prefix. `tests/no_vendor.rs` fails if any other source file mentions
`bws` or `bitwarden`.

**Residual risks**: an `allow_docker` escape reads `BWS_ACCESS_TOKEN` and `FACTORY_CI_TOKEN` from the
runner agent, then every project the machine account can read; scope it to "Can read" on the default and
per-repo projects only (A21). Anyone who can push a pipeline to a listed repo can read every key under
`owner/name/` and every shared name the entry lists (A24). Free plan: 3 projects, 3 machine accounts. Rate
limits are undocumented: a tick of 20 items makes at most about 45 list calls and relies on the retry. An
outage after planning ends each affected item in `error`, final until a new commit (A6c). Prepare plus
real run clone twice.

**Unverified**: that Secrets Manager accepts `/` in a key; `bws` stderr text for auth failures; the exact
`bws config server-base` form; the checksum value to paste.

## D10. Operations and hardening review (D1 to D12, H1 to H3, M1 to M7, L1 to L5)

**Amended by D12 (2026-11-27)**: GitHub secrets are no longer a secret source, there are no per-repo projects, the matrix carries an encrypted blob, and `plan` runs in two phases. Where this section differs, D12 wins.

This section amends D2, D3, D4 and D9 where they differ. Accepted findings are decisions; rejected or
adjusted ones say why.

### Operations

- **D1 cost**: an idle tick bills the `plan` job (minimum 1 minute, plus runner start) and often a second
  billed minute, so about 2 minutes. At 5 minutes that is 8,640 ticks and about 17,000 billed minutes a
  month against 2,000 to 3,000 free on a private repo. Decision: recommend a public B. For a private B,
  interval 60 to 120 minutes. No code change; plan and quickstart document both. The user decides.
- **D2 write pacing**: 250 ms is 240 writes a minute, three times the secondary limit of 80 content-creating
  requests a minute (500 an hour). Pace writes at 750 ms (`PACE_MS`), cap status writes at 60 per tick
  (`WRITE_CAP`; pending first, then unchanged posts, then folder-name errors), carry the rest: an item or
  unchanged post not written is not done, so the next tick plans it again. A secondary-limit 403 is
  `RateLimited`. The cap also covers the `error` posts of invalid folder names. Matrix jobs post their own
  finals (at most 20 a tick). Sustained hourly use of 500 is possible only on repeated first-time ticks;
  the plan accepts that.
- **D3 invalid `if:`**: `secrets` is not available in a step `if:`. The install step takes
  `env: HAVE_BWS: ${{ secrets.BWS_ACCESS_TOKEN != '' }}` and the script tests `[ "$HAVE_BWS" = true ]` in `run:`.
  The lint rejects `secrets.` inside any `if:`. The same for the docker token.
- **D4 main only** merges with H2.
- **D5 verdict on idle ticks**: `verdict` runs only when something happened:
  `always() && (needs.plan.result != 'success' || needs.plan.outputs.problems != '0' || needs.run.result != 'skipped'
  || needs['run-docker'].result != 'skipped')`. It stays outside the `main` guard and has no environment: a dispatch
  of another ref then ends red rather than skipped. Retention of runs: 7 to 14 days (repository setting).
- **D6 dead-man alert**: after planning, `plan` ships one line to Loki with `repo="factory-ci"`, `project="."`,
  `step="heartbeat"`, `stream="stdout"`: `run=<id> tick items=N problems=M`. Delivery failure is ignored. The
  Grafana alert fires on absence for 20 minutes with no-data set to Alerting. It needs shipping on; with shipping
  off the owner relies on red ticks and the scheduler's own failure email. Scheduler recipes: cron-job.org failure
  notification; Pi cron line `flock -n /var/lock/fci.lock curl -fsS ... || logger -t factory-ci "dispatch failed"`
  with the token in a 0600 header file passed as `-H @file`.
- **D7 tokens**: fine-grained tokens expire and cover one owner; expiry and rotation steps go in the quickstart.
- **D8, L1 images**: `alpine/git` and `docker:27.5.1-dind` are mirrored to GHCR by a manual workflow
  (`docker buildx imagetools create`, which keeps the digest) and pinned in repository variables
  `FACTORY_CI_CLONE_IMAGE` and `FACTORY_CI_DIND_IMAGE`. `job` requires both and refuses a reference without
  `@sha256:` and 64 hex digits. Local `run` keeps the tags. The finding that the clone image is pulled twice per
  item is adjusted: the prepare run and the real run share the VM, so the second use hits the local image cache,
  one pull per job. The digests cannot be known here; the owner pastes them from the mirror workflow's summary,
  the way the runner digest works. Self-test and lint reject an unset or unpinned variable at tick start.
- **D9 idle Bitwarden cost**: the plan check skips entirely when `BWS_ACCESS_TOKEN` is unset or the list names
  no Bitwarden project and no default project is set; otherwise one list call per distinct project. T065 counts
  calls. Install the musl asset (`bws-x86_64-unknown-linux-musl-2.1.0.zip`) when the checksums file lists it,
  else the gnu one; the pasted `BWS_SHA256` constant for the chosen file name is authoritative.
- **D10 quickstart gaps** filled (Bitwarden setup, Grafana access policy `logs:write`, user ID and URL form,
  Actions, GHCR public, dispatch headers `X-GitHub-Api-Version: 2022-11-28` and a `User-Agent`).
- **D11 Grafana free**: 50 GB a month, 14-day retention. Status links to logs die after 14 days (README note).
  A usage alert at 70% goes in the quickstart.
- **D12 rollout** replaced (plan.md).

### Security

- **H1 split jobs**. Plan rows carry `docker`. `plan` writes two matrices and counts. `run` (repos without
  `allow_docker`): environment `factory-ci`, main Bitwarden token, `toJSON(secrets)`, `FACTORY_CI_TOKEN`.
  `run-docker`: environment `factory-ci-docker`, `BWS_ACCESS_TOKEN_DOCKER` mapped to `BWS_ACCESS_TOKEN` inside the
  step only, `FACTORY_CI_TOKEN_DOCKER` mapped to `FACTORY_CI_TOKEN`, project `FACTORY_CI_BWS_PROJECT_DOCKER`, no
  GitHub secret JSON (so FR-040 rule 4 does not apply to docker repos: their names come from Bitwarden only).
  The docker project also holds the three `FACTORY_CI_LOG_*` keys, because that account cannot read the default
  project. `plan` uses `FACTORY_CI_TOKEN_DOCKER` for the API calls on `allow_docker` repos and
  `FACTORY_CI_TOKEN` for the others, so no job's token reaches repos it should not. The plan job holds both
  tokens and both Bitwarden tokens (environment `factory-ci-plan`): it runs no code from a watched repo. Free
  plan fit: 2 machine accounts, 2 projects plus up to 1 per-repo project. The per-tick cap is shared across both
  matrices. Lint: `run-docker` never references `secrets.BWS_ACCESS_TOKEN`, `toJSON(secrets)` or the main token.
  Static mapping instead of `toJSON(secrets)` in `run` is not adopted (see plan, Security).
- **H1 rootless dind**: not adopted. Docker documents `docker:dind-rootless` as needing `--privileged` (or an
  equivalent relaxed profile) too, and Ubuntu 24.04 runners restrict unprivileged user namespaces; the privileged
  container is the escape surface, not the user inside it. Unverified here (no network client); T065 may try it.
- **H2 Environments**: `factory-ci-plan`, `factory-ci`, `factory-ci-docker`, each with deployment branches limited
  to `main`; a ruleset protects `main`. Every job with an environment also has `if: github.ref == 'refs/heads/main'`.
  Lint checks both. Behaviour of environments with `workflow_dispatch` on a non-main ref is unverified (T065).
- **H3 fail closed on a public B**: `B_PRIVATE: ${{ github.event.repository.private }}` goes to `plan` and the
  wrappers. The wrapper passes `--quiet-logs` iff the log credentials resolved. `plan` refuses (a problem line) a
  private repo when B is public and no log store resolved. `job --require-logs` exits before cloning when no log
  config exists; the wrapper sets it for private repos on a public B (row field `fail_closed`).
- **M1**: `GET /user` once per token per plan; `walk` ignores any status whose `creator.login` differs. If `/user`
  fails the repo is a problem (fail closed). Fine-grained tokens calling `GET /user` is unverified (T065).
- **M2**: one `sanitize` function (`[A-Za-z0-9 _.:/-]`, other characters become `_`, length limit, a leading `::`
  is prefixed with `x`) used by reasons, descriptions and summaries. The wrapper writes reasons only to files.
- **M3**: reserved names as in the plan, matched on the upper-cased name; exact-case GitHub lookup; `while IFS= read -r`
  with a regex recheck, no `eval`; `jq --arg k` for keys; watch repo regex without `..`.
- **M4**: entries may carry `id` (number). `plan` calls `GET /repos/{o}/{n}` once per entry (the same call yields
  `private`); a mismatch skips the repo and names both ids. FR-040 rule 2 needs an `id`. Cost: one more call per
  repo per tick, so the idle count of the reference fleet is 120.
- **M5**: `bws` output via stdin or a 0600 file in `$RUNNER_TEMP`, removed before `exec`; `set +x`; stderr not printed.
  "Mask before the first bws call" is rejected as stated: values are unknown before the call. GitHub already masks
  the token because it is a secret, and each value is masked at the moment it is parsed.
- **M6**: a value with a trimmed line under 4 characters is refused (name only). This changes A22. The runner and
  the wrapper mask base64 of each whole value and of `x-access-token:<token>`. Partial or re-aligned encodings are
  not caught; README says so.
- **M7**: wrapper runs `sudo iptables -I DOCKER-USER -d 169.254.169.254 -j DROP` and the same for `168.63.129.16`.
  Step containers add `--cap-drop NET_RAW --cap-drop MKNOD --cap-drop SETFCAP`. `--cap-drop ALL` is not adopted: it
  breaks `apt-get` as root and nothing here can test it. Revisit after a live trial.
- **L2**: the lint refuses a placeholder `BWS_SHA256`, and the install step checks `unzip -Z1` lists exactly `bws`.
- **L3**: scopes go in the quickstart: `FACTORY_CI_TOKEN` fine-grained, selected repos, Contents read, Commit statuses
  write, Metadata read, 90-day expiry; caller token Actions write on B only, not classic; machine accounts read-only;
  Loki token `logs:write` only.
- **L4**: cap order interleaves repos round-robin, oldest first within a repo, starting with the repo whose oldest
  item is oldest. This changes FR-012.
- **L5** (obsolete after D-b, R11): clash notices were to go to the log only when a sink is configured (`Log::out` guarded by `has_sink()`); with
  shipping off they are dropped, never printed to the Actions log.

## D11. Clarify session 2026-11-27 (Q1 to Q5)

This section amends D1, D4, D6, D9 and D10 where they differ; D12 amends it in turn (Q2 is replaced by the retry marker).

- **Q1 public B (A25)**: B is public and the scheduler fires every 5 minutes. A private B is not a supported
  configuration of this version, so the "private B at 60 to 120 minutes" option of D1 is dropped. Because B is
  public, step output of a private watched repo must never reach the Actions log: a private repo needs a log store
  (FR-049). The plan refuses to plan a private repo without one, and `job --require-logs` stops a job that slipped
  through.
- **Q2 one retry of an old `error` (A6c, FR-009)**: the combined-status call returns only the latest status per
  context. When the tip's latest status for a context is `error` and its `updated_at` is older than 1,800 s, `walk`
  makes one extra call, `GET /repos/{r}/commits/{sha}/statuses` (all updates, newest first, paged, only statuses by
  the token's user), and counts the `pending` updates for that context. One `pending` means the item ran once: it is
  retried (`Run`). Two or more mean it was already retried: `Done` (the `error` stays). An `error` younger than
  1,800 s is `Done`. A retry posts a new `pending`, so a commit and context reach at most 4 updates (pending, error,
  pending, final). The call happens only for old `error` statuses, so idle ticks on healthy repos pay nothing. A
  `failure` is never retried. A timestamp that cannot be parsed counts as young (no retry), the safe side.
- **Q3 `item N` names (FR-058)**: the matrix job `name:` is `item ${{ strategy.job-index }}` (zero-based) and the
  matrix row no longer carries `short`. The tick summary lists `item N: owner/name` with the repo name only, never a
  branch or SHA. Counts, problems and unchecked repos follow the same rule. The context (project path) stays out of
  the summary too, because a folder name can name a private product. Status descriptions on the watched repo are
  unaffected.
- **Q4 two heartbeat channels (FR-056)**: after `plan` writes its outputs it reports through each configured channel:
  the Loki line of D6 (shipping on), and a plain `GET` to `FACTORY_CI_HEARTBEAT_URL` (variable set). The URL is a
  capability URL (anyone holding it can ping), so it is an Environment secret of `factory-ci-plan`, must start with
  `https://`, is added to the redactor, and is never printed. One try and one retry after 2 s, 10 s timeout; failure
  is logged as `heartbeat not delivered` (fixed text) and ignored. The name starts with `FACTORY_CI`, so pipelines
  cannot list it. The owner points the URL at any dead-man service (healthchecks.io style) with a grace period of
  about 20 minutes.
- **Q5 separator (A24)**: `FCI_KEY_SEP` is `/` first and `:` if Bitwarden refuses `/`. `__` is dropped everywhere. A
  key is built as `owner` + sep + `name` + sep + `NAME`. Neither `/` nor `:` can occur in a repo name
  (`[A-Za-z0-9._-]`) or a variable name (`[A-Za-z_][A-Za-z0-9_]*`), so two different (repo, name) pairs never give
  the same key. With `__` a repo `a/b__c` with name `D` and a repo `a/b` with name `c__D` would both give
  `a__b__c__D`, which would let one repo read another's secret. T054 tries `/` against a real project.

## D12. Critic review decisions D-a to D-e (2026-11-27)

This section supersedes D2, D9, D10 and D11 wherever they differ. The earlier sections stay as history; each of
them says it is amended. Decisions are the lead's; rejected or adjusted points say why.

### D-a. Nothing private in the public log (BLOCKER)

- **Problem**: the tick workflow's step `env:` blocks printed `ITEM_SHA`, `ITEM_BRANCH`, `ITEM_PROJECT`,
  `ITEM_SECRETS` and `ALL_SECRETS`. GitHub prints each step's evaluated `env:` in the run log, and B is public.
- **Decision**: the matrix carries `{n, blob}` only. `plan` phase C writes the final rows to a private file; the plan
  wrapper encrypts each row with the `openssl` CLI (AES-256-CBC, PBKDF2, salt, single-line base64) using the key
  `FACTORY_CI_BLOB_KEY` from a masked secret, and writes only the ciphertext to `$GITHUB_OUTPUT`. The job wrapper
  decrypts inside the script (`set +x`, nothing echoed). The ciphertext contains neither the key nor any text GitHub
  masks, so outputs are not dropped. CBC gives no authentication, but the blob never leaves one workflow run and a
  forged blob would only fail to decrypt or parse; a MAC is not worth a second tool.
- **Simplest safe form**: the `openssl` CLI is preinstalled on `ubuntu-24.04`. No crypto crate enters the runner, and
  no new action is used. Rejected: encrypting in Rust (a crypto dependency for one use), GitHub artifacts (a second
  transport and still public), and base64 alone (not secret).
- **Private repos**: `job --private` (rows with `fail_closed`) prints nothing to stdout; step names, durations and
  results go only to the log store. `--private` needs a log store and fails closed without one. Public repos keep
  the fixed events (`started`, `success in 12s`) on stdout.
- **Names**: the job is `item ${{ matrix.n }}`, `n` unique across `run` and `run-docker`. The summary and the committed
  watch list still name repos (accepted by clarify answer 3B).
- **Fallback step**: `factory-ci-status.sh` decrypts the blob itself, so the fallback `error` post needs no `ITEM_*` in
  `env:`.
- **Live check** (T065): grep a real public run log for a branch, a SHA, a project path and a secret name.

### D-b. Bitwarden is the only secret store

GitHub secret fallback (FR-040 source 4) is gone. B's GitHub secrets are bootstrap only: `FACTORY_CI_TOKEN`,
`FACTORY_CI_TOKEN_DOCKER`, `BWS_ACCESS_TOKEN`, `BWS_ACCESS_TOKEN_DOCKER`, `FACTORY_CI_BLOB_KEY`,
`FACTORY_CI_HEARTBEAT_URL`. Removed with it: `toJSON(secrets)`, `ALL_SECRETS`, `ITEM_SECRETS` as a GitHub-name list,
case-folding `jq` over the secret JSON, the clash notice (`FACTORY_CI_NOTICES`), the 128 KB limit and the
GitHub-secret residual risk of D2. Without `BWS_ACCESS_TOKEN` a step that lists a secret ends the item in `error`
"no secret store configured" (FR-043). The log credentials also come only from the store, so without it shipping is
off, and a private repo on a public B is refused (FR-049).

### D-c. No per-repo project

`bitwarden_project`, `plan --list-projects` and the row field `bws_project` are removed. Keys live in the default
project: `owner/name<SEP>NAME` (needs the entry's `id`, D-d 9) and shared `NAME` (listed in `secrets`). The docker
account reads the docker project, which holds the docker repos' prefixed and shared keys and a copy of the three log
keys. Count: 2 projects (default, docker) and 2 machine accounts of the free plan's 3 and 3. The store check lists
each project once.

### D-d. Major fixes

2. The lint tolerates the `BWS_SHA256` placeholder; `FACTORY_CI_LINT_STRICT=1` (the release gate, after T054) refuses it.
3. The task cycle T044, T045, T054, T065 is broken: T044 excludes T046, T054 and T065 (tasks.md).
4. Shared-file edit order: T028 and T037, then T049, then T060, then T061. T060 depends on T016. The image-pin check
   moves into S13 with T049 and its test T055. T022 writes only the workflow skeleton and no longer depends on T026.
   The wrapper scripts are written once: T050 (secrets library, job and plan wrappers) and T062 (blob library and
   status script), both before the lint run.
6. `secret_branches` (default: the repo's default branch, from `GET /repos/{r}`, already called once per entry). The
   plan sets `secrets_ok` on each row; the job wrapper refuses a pipeline that lists secrets when it is false.
7. Docker exit code 125 (image cannot be pulled, engine fault) and a failed clone end the item in `error`. `job` maps
   the first failed step's reason `exit code 125` to `error`; exit codes 126 and 127 stay `failure` (the step's own
   command). A name that is not a docker error stays `failure`.
8. The ping comes from `verdict` (`if: always()`, every tick), not from `plan`, so it fires after the tick's last job.
   The grace on the alert must exceed the longest tick; 20 jobs at 4 at a time and 30 minutes each is about 150
   minutes plus clones, so set the grace to 3 hours or accept a late alert on long ticks. `verdict` runs on every tick
   (it is cheap on a public B), so the D5 idle skip is dropped. The Loki heartbeat line stays but only on ticks with
   candidate rows (see D-d 11); the dead-man alert is the ping.
9. Prefixed keys need the entry's `id` (FR-052). The test and the story say so.
10. `fail_tick`: persistent repo errors fail `verdict` only when the tick starts in the first 10 minutes of a UTC hour
    (`--now` makes it testable). The summary lists problems every tick. The finding asked for "first tick of each
    hour"; the tool has no state, so the 10-minute window approximates it (at most 2 ticks at a 5-minute interval).
11. Two-phase `plan`: phase A reads GitHub and writes candidates; the wrapper checks the store and fetches the log
    credentials only if there are candidate rows; phase C refuses, caps and posts. An idle tick makes no store call.

### D-e. Minor fixes

- The combined status call is paged (`page=N` until short or `total_count`).
- A truncated recursive tree falls back to the root tree plus one non-recursive call per top-level folder.
- Every `runs-on` is `ubuntu-24.04` (workflows written here; `ci.yml` is not touched).
- Rollout uses a separate public copy of B, never this monorepo (A1). The quickstart starts entries on the default
  branch (`branches = ["main"]`).
- `--cap-drop NET_RAW,MKNOD,SETFCAP` is part of FR-054. FR-054's real test is the live check in T065 (a step
  container cannot reach `169.254.169.254`), backed by T058 (the wrapper calls `iptables`) and T030 (the argv).
- The A6c retry uses the marker " (retry)" in the status description instead of paging the status list. A retry posts
  `pending` "queued (retry)"; every final of a retried item ends with " (retry)". An old `error` or an old `pending`
  without the marker spends the budget once; with the marker an old `error` is final and an old `pending` is closed
  with an `error`. Updates per commit and context: at most 4. This replaces D11 Q2 and removes `statuses_all`.

### Not decided (note for the user)

MAJOR 5, the early compile check: the crate has never compiled. A first compile needs a scratch public repo or Actions
enabled. No repo is created here. T045 stays blocked until the owner chooses.

### Consequences the lead should know

- The Loki heartbeat line cannot be a dead-man signal, because idle ticks make no store call. The quickstart alerts on
  the ping only.
- A tick that dies between phase C's posts and the matrix output leaves `pending` statuses with no job. They age out
  (7200 s) and use the retry budget; a second loss closes them as `error`.
- The blob key is one more bootstrap secret in three Environments.

### Review round 1, 2026-11-27

Lead decisions on the reviewer, security and QA findings. Finding ids R1 to R16 map to tasks T066 to T081.

- **R1**: the closing `error` the plan posts for an old marked `pending` has the description
  "job did not finish (retry)", so `walk` sees the marker and never replans it. A plan-level test replays the post
  and expects `Done` on the next tick.
- **R2**: commits shared by several branches: the item's branch is the default branch if it is among them, else the
  alphabetically first; `secrets_ok` is true if any of the branches matches `secret_branches` (default: the default
  branch).
- **R3**: an `allow_docker` entry with `FACTORY_CI_TOKEN_DOCKER` unset gets a problem line and no row; the plain token
  is never used for docker rows (FR-047).
- **R4**: a failed final status prints "final status not delivered" and the HTTP code; never the request path.
- **R5**: docker rows use a separate blob key, `FACTORY_CI_BLOB_KEY_DOCKER`, kept only in `factory-ci-plan` and
  `factory-ci-docker`. `factory-ci-plan.sh` picks the key by `allow_docker`. The `run` job never holds the docker key
  and `run-docker` never holds the plain key. The lint checks both.
- **R6**: `factory-ci-blob.sh` uses `-iter 600000`; the tick's guard step refuses a blob key under 32 characters.
- **R7**: `Log` replaces `\r` and other control characters except tab before printing or shipping.
- **R8**: the Loki URL must be `https` ("log URL must be https"), except plain `http` to `127.0.0.1`, `localhost` or `[::1]`,
  which the test fake uses; the code keeps that exception. Security L-b, accepted low risk: the exception ships in the
  production binary, and it can only be reached by an owner who sets the URL secret. The tick workflow drops
  `packages: read` (the GHCR packages are public).
- **R9**: `branches()` is an error when the list is still full after 5 pages (more than 500 branches), shown as a repo
  problem.
- **R10**: failing to spawn the container engine is `error`, not `failure`.
- **R11**: dead code removed: `Log::has_sink`, `Log::redact` (no production caller), `text::value_ok` and its test
  cases. The shell `fci_short_line` is the real short-line rule (FR-053).
- **R12**: `factory-ci-selftest.yml` gets a `shell` job (the two shell test scripts and the lint self-test on
  `ubuntu-24.04`); the "TEMP" artifact step goes.
- **R13**: README states `RUNNER_IMAGE_DIGEST` as `sha256:<64 hex>` (no `@`), as the guard checks.
- **R14**: the three `#[ignore = "test-first"]` markers in `tests/executor.rs` go so CI runs them; `walk.rs` uses
  `is_none_or`; the stale "test-first wave" comment in `lib.rs` goes.
- **R15**: a bad blob key leaves the item `pending`: the job cannot decrypt the row, so the fallback status step cannot
  either, and the 2-hour stale rule closes it (A6b, A6c). The claim "a branch deleted mid-tick costs nothing" is
  removed (the tree-404 test covers a vanished commit).
- **R16**: T045 is extended: at the first compile, generate and commit `rust-ci-runner/Cargo.lock`, switch the
  `Makefile` and `rust-ci-runner-release.yml` builds to `--locked`, and run `cargo fmt` (about 300 lines exceed 100
  columns; rustfmt is not available on the dev VM). Blocked with T045.
- **Accepted residual risks, not fixed**: more reserved names (only store administrators create requestable names);
  repo names in the step summary (FR-058); the privileged dind sidecar (documented opt-in); the Environment branch
  policy (verified live in T065); the upstream `alpine/git:latest` tag (pinned by digest at mirror time); no container
  memory limit; an empty repo-scoped value falling through to the shared key (documented). The `ubuntu-latest` pins in
  `ci.yml`, the release workflow and the self-test wait on the user.

### Review round 2, 2026-11-27

Security, reviewer and QA passed round 2 (QA: READY pending T045 and T081). Cleanup findings, tasks T082 to T086:

- **Https exception**: the Loki URL must be `https` except plain `http` to `127.0.0.1`, `localhost` or `[::1]`, which the test
  fake uses. The code keeps the exception; FR-028, R8 and T073 state it. Accepted low risk L-b: it ships in the production
  binary and only the owner who sets the URL secret can reach it.
- **T082**: `main.rs` must not let the plain token stand in for the Docker client; a Docker row with no Docker client is a
  problem line and no row (FR-047).
- **T083**: use the `SPAWN_FAIL` constant for the "cannot run " text in `engine.rs`.
- **T084**: README marks `FACTORY_CI_BLOB_KEY_DOCKER` optional (needed only when Docker rows exist, at least 32 characters
  when set).
- **T085**: the T068 test must not be vacuous: use a list with only the `allow_docker` entry and assert zero requests to
  that repo, or answer `/user` correctly and assert the other repo is still planned.
- **T086**: remove the duplicate lint self-test line in the self-test workflow.
- **T081 extended**: `rust-version = "1.82"` in `Cargo.toml`. Get `Cargo.lock` without a local build with
  `docker run --rm -v "$PWD/rust-ci-runner":/w -w /w rust:1 cargo generate-lockfile` on any machine with Docker; the
  fallback is a scratch branch with an `upload-artifact` step. Run `cargo fmt` there (the `fake.rs` `branches` line is over
  100 characters).

### Review round 3, 2026-11-27

- **Known limit (spec A27)**: removing `FACTORY_CI_TOKEN_DOCKER` while a Docker repo has a stale `pending` leaves that status
  until a tick runs with the Docker token again, because phase C drops that repo's "unchanged" and closing-error posts and
  FR-047 forbids the plain token for Docker repos. Accepted.
- **T087**: a test that phase C without a Docker token posts nothing for the Docker repo and still posts the plain repo's
  entries, plus a comment at the `invalid.retain` in `main.rs`.
- **T085 note**: its test guards the phase A partition; the `Apis::same` branch cannot be observed in phase A; T082 covers
  phase C; the mutation check is the plain-token fallback.
- **Accepted cosmetic**: the "FACTORY_CI_TOKEN_DOCKER is not set" problem line appears from both phases.
