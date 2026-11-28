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

`watch::check_owner(entries, owner)` returns `entry N: repo owner differs from the repository owner` per mismatch (case-insensitive); an empty `owner` returns `file: repository owner is not known`.

## `plan`

- `plan --repos FILE|- --check`: validates with `parse`; `FILE` or stdin (at most 64 KiB + 1 byte, `-` only with `--check`). Prints `N entries ok` on stdout; errors on stderr, exit 1. Zero entries is an error. No network. `check_owner` runs only when `GITHUB_REPOSITORY_OWNER` is set.
- Phase A (`--repos FILE --rows-out CAND`): `parse`, then `check_owner` (unset owner refuses), then the plan. Summary and problem lines use `entry N` and `item N`. The line for an `allow_docker` entry without a docker token is `entry N: FACTORY_CI_TOKEN_DOCKER is not set`. An id mismatch says `id differs from the list`.
- `B_PRIVATE` is private only when it equals `true`.
- Phase C reads only candidates; its redactor is seeded from the repos in the candidate rows.

## `ci`

- Without `--repos`: the list comes from the one `bws secret list` call, scoped by `FACTORY_CI_BWS_PROJECT` when set; `watch_text`, `parse`, `check_owner` (with `GITHUB_REPOSITORY_OWNER`).
- With `--repos FILE`: the file path as before (development and tests; the production workflow never passes it).
- Fault lines (`[runner] ` prefix): `repository or ref not in the watch list` (also for an empty token) and `watch list unusable`. `BWS_ACCESS_TOKEN is not set` leaves `FAULTS`.
- One `bws` call per run.

## `store.rs`

- `fetch_json(bws_bin, token, server_url, limit, project) -> Result<String, StoreError>`: retry once on a transient failure, never on 400, 401, 403.
- `watch_text(json) -> Result<String, ListError>`: `Missing`, `Many(n)`, `Empty`.
- `project`: 1 to 64 characters of `[0-9A-Za-z-]`, no leading `-`; otherwise the call is refused (`Unreachable`).
