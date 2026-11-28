# Contract: runner behaviour

## `watch::parse` (every caller: `plan` phase A, `plan --check`, `ci`)

Same keys and limits as before (`repo`, `id`, `branches`, `secret_branches`, `secrets`, `allow_docker`; 64 KiB; 100 entries; 20 items per list; unknown keys refused). It returns `Err(Vec<String>)` with every problem at once. **Messages never contain list text.** `N` is the 1-based position of the table; `K` a line number.

| Message | When |
|---|---|
| `line K: invalid TOML` | syntax error (K from the parser span; no snippet) |
| `file: larger than 64 KB` | size |
| `file: more than 100 entries` | count |
| `file: no entries` | zero tables, or no `watch` key |
| `file: unknown top-level key` | any key other than `watch` |
| `file: watch must be a list of [[watch]] tables` | wrong type |
| `entry N: must be a table` | an item is not a table |
| `entry N: missing repo` / `repo must be a string` / `repo is not owner/name` | repo problems |
| `entry N: duplicate of entry M` | repo repeated (case-insensitive) |
| `entry N: unknown key` | key outside the six |
| `entry N: id is not a number` | id problem |
| `entry N: id is required with allow_docker or secrets` | new rule |
| `entry N: branches must be a list of strings` / `branches has more than 20 items` | same two forms for `secret_branches` and `secrets` |
| `entry N: malformed glob (length)` / `(unsupported sequence)` / `(unsupported character)` | glob problems, no glob shown |
| `entry N: secret_branches must not be * or **` | new rule |
| `entry N: secret is reserved or malformed` / `secret is listed twice` | secrets |
| `entry N: allow_docker must be true or false` | type |

`watch::check_owner(entries, owner)` returns `entry N: repo owner differs from the repository owner` per mismatch (case-insensitive); an empty `owner` returns `file: repository owner is not known`. It is a pure function of its arguments. It never reads the environment.

Who supplies `owner`: `main.rs` reads `GITHUB_REPOSITORY_OWNER` once and passes it to `plan_a`; `ci` takes it from its `env` closure; `plan --check` takes it only from `--owner NAME`. Tests: unit tests pass the owner; CLI tests set the variable in the child process (to the fixture owner), clear it, and set a different value; no test reads the ambient variable (the selftest and CI always set it to the account name).

## `plan`

- `plan --repos FILE|- --check [--owner NAME]`: validates with `parse`; `FILE` or stdin (at most 64 KiB + 1 byte, `-` only with `--check`). Prints `N entries ok` on stdout; errors on stderr, exit 1. Zero entries is an error. No network. It never reads `GITHUB_REPOSITORY_OWNER`; `check_owner` runs only with `--owner`.
- Phase A (`--repos FILE --rows-out CAND`): `parse`, then `check_owner` with the owner `main.rs` read (unset or empty refuses), then the plan. Summary and problem lines use `entry N` and `item N`. The line for an `allow_docker` entry without a docker token is `entry N: FACTORY_CI_TOKEN_DOCKER is not set`. An id mismatch says `id differs from the list`.
- `B_PRIVATE` is private only when it equals `true`.
- Phase C reads only candidates; its redactor is seeded from the repos in the candidate rows.

## `ci`

- Arguments: `--entry N` (a positive integer from 1 to 100) or `--repo OWNER/NAME`, never both and never neither; `--ref`, `--project`, `--work`, `--allow-docker` as before; `--repos FILE` optional. The manual workflow passes `--entry` only, so no repo name enters the dispatch input, the step `env:`, the step header or the log. `--entry` without the store source (with `--repos FILE`) resolves entry N of the file the same way.
- Without `--repos`: the list comes from the one `bws secret list` call, scoped by `FACTORY_CI_BWS_PROJECT` when set; `watch_text`, `parse`, `check_owner` (owner from the `env` closure). With `--entry N` the repo is entry N's `repo`; an entry past the end or without `id` prints the unlisted line. The branch check (`branches` against `--ref`) and the docker check run as today.
- With `--repos FILE`: the file path as before (development and tests; the production workflow never passes it; the owner rule is skipped).
- Fault lines (`[runner] ` prefix): `repository or ref not in the watch list` (also for an empty token) and `watch list unusable`. `BWS_ACCESS_TOKEN is not set` leaves `FAULTS`.
- One `bws` call per run.

## `store.rs`

- `fetch_json(bws_bin, token, server_url, limit, project) -> Result<String, StoreError>`: retry once on a transient failure, never on 400, 401, 403.
- `watch_text(json) -> Result<String, ListError>`: `Missing`, `Many(n)`, `Empty`.
- `project`: 1 to 64 characters of `[0-9A-Za-z-]`, no leading `-`; otherwise the call is refused (`Unreachable`).
