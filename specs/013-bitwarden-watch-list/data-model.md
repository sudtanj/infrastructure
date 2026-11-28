# Data model

## Key `FACTORY_CI_REPOS` (Bitwarden)

| Property | Rule |
|---|---|
| Name | exactly `FACTORY_CI_REPOS`, exact case, one row per project |
| Value | UTF-8 TOML, at most 64 KiB, at least one `[[watch]]` table, comments allowed |
| Format | the `[[watch]]` format of `watch::parse`: `repo` (required, `owner/name`), `id`, `branches`, `secret_branches`, `secrets`, `allow_docker`; unknown keys refused |
| Limits | 100 entries, 20 items per list, as `watch.rs` |
| Reserved | `FACTORY_CI*` names are refused in `secrets` (`pipeline::valid_secret_name`), so no step can request this key |

## WatchEntry

Existing fields plus `number: usize`, the 1-based position of the table in the text. Set by both parsers.

## ListError (store.rs)

| Variant | Meaning | Fixed line (watch-list) | Fixed line (ci) |
|---|---|---|---|
| `Store(StoreError)` | store down or rejected | `watch list: secret store unreachable` or `watch list: secret store rejected the access token` | the existing store faults |
| `Missing` | no row | `watch list: key FACTORY_CI_REPOS not found` | `watch list unusable` |
| `Many(n)` | n rows | `watch list: key FACTORY_CI_REPOS found n times` | `watch list unusable` |
| `Empty` | blank or non-string value | `watch list: FACTORY_CI_REPOS is empty` | `watch list unusable` |

## Plan rows

`Row` and the status records that carry a repo gain `entry: usize` (the `number` of the entry they came from). `Candidates.unchecked` holds `entry N` strings.

## Files on the tick VM

`.github/factory-ci-repos.toml`: written by `watch-list` (mode 0600) only for a valid list, removed by the cleanup step, never uploaded.

## State transitions

`watch-list`: start, token read and removed, fetch (2 attempts), key found, parse, write, exit 0. Any failure after the start removes the output path and exits 1.
