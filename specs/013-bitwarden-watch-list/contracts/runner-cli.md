# Contract: runner commands

## `rust-ci-runner watch-list --out FILE [--bws PATH] [--project ID]`

- Env in: the Bitwarden access token variable (required; read once, removed from the process environment before any thread starts, added to the redactor), `FACTORY_CI_BWS_SERVER_URL` (optional, same rule as `ci`), `HOME`, `PATH`.
- Flags: `--out` required. `--bws` defaults to `/usr/local/bin/bws`. `--project` optional; 1 to 64 characters of `[0-9A-Za-z-]`, not starting with `-`; passed as one argument after `secret list`.
- stdout: empty. Exit 0 only when the file was written.
- stderr, one line per problem, all from this set (N and K are numbers):

| Line | When |
|---|---|
| `watch list: BWS_ACCESS_TOKEN is not set` | empty token |
| `watch list: secret store unreachable` | store failure after 2 attempts |
| `watch list: secret store rejected the access token` | 400, 401, 403 |
| `watch list: key FACTORY_CI_REPOS not found` | no row |
| `watch list: key FACTORY_CI_REPOS found N times` | N rows, N at least 2 |
| `watch list: FACTORY_CI_REPOS is empty` | blank or non-string |
| `watch list: FACTORY_CI_REPOS is larger than 64 KiB` | size |
| `watch list: line K: invalid TOML` | syntax error |
| `watch list: entry N: <problem>` | validation, problem from the table below |
| `watch list: FACTORY_CI_REPOS has no entries` | zero tables |
| `watch list: more than 100 entries` | count |
| `watch list: cannot write the output file` | write error |
| usage text | bad arguments |

- File: mode 0600, the value byte for byte. On any failure the path is removed if it exists.

## Problem table of `parse_private` (the only texts it may produce)

`entry N: missing repo`, `repo must be a string`, `repo is not owner/name`, `duplicate of entry M`, `unknown key`, `id is not a number`, `branches must be a list of strings`, `branches has more than 20 items`, the same three forms for `secret_branches` and `secrets`, `malformed glob (length)`, `malformed glob (unsupported sequence)`, `malformed glob (unsupported character)`, `secret is reserved or malformed`, `secret is listed twice`, `allow_docker must be true or false`, `must be a table`. Top level: `line K: invalid TOML`, `unknown top-level key`, `watch must be a list of [[watch]] tables`. Nothing from the input appears in any of them.

## `rust-ci-runner plan --repos FILE --check` and `plan --repos - --check`

- `FILE`: validated with `parse` as today; stdout `N entries ok`, N may be 0.
- `-`: stdin, at most 64 KiB + 1 byte, validated with `parse_private`; stdout `N entries ok`. Zero entries prints `watch list: FACTORY_CI_REPOS has no entries` and exits 1; empty stdin prints `watch list: stdin is empty`. Errors on stderr in the `entry N:` form. No network.
- `--repos -` without `--check` is a usage error.

## `plan` phase A

Reads the file at `--repos`. An empty list: stderr `watch list has no entries`, exit 1, no candidates file. Summary lines name `item N` and `entry N` only.

## `ci`

Fault lines (`[runner] ` prefix): the unlisted line (also for an empty token) and `watch list unusable`. `BWS_ACCESS_TOKEN is not set` leaves `FAULTS`. One `bws` call per run.
