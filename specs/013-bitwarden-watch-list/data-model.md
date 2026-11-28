# Data model

## Key `FACTORY_CI_REPOS` (Bitwarden)

| Property | Rule |
|---|---|
| Name | exactly `FACTORY_CI_REPOS`, exact case, one row per project |
| Value | UTF-8 TOML, at most 64 KiB, at least one `[[watch]]` table, comments allowed |
| Format | `repo` (required, `owner/name`), `id`, `branches`, `secret_branches`, `secrets`, `allow_docker`; unknown keys refused |
| Limits | 100 entries, 20 items per list (`watch.rs`); Bitwarden's own value limit is unverified (T002) |
| Rules added here | owner equals the repository owner (`check_owner`, given the owner as an argument; the environment is read once at the edge); `id` required with `allow_docker` or non-empty `secrets`; `*` and `**` refused in `secret_branches`; zero entries refused |
| Reserved | `FACTORY_CI*` names are refused in `secrets` (`pipeline::valid_secret_name`), so no step can request this key |

## WatchEntry

Existing fields plus `number: usize`, the 1-based position of the table in the text, set by `parse`.

## ListError (store.rs, used by `ci`)

| Variant | Meaning | `ci` fault |
|---|---|---|
| `Missing` | no row | `watch list unusable` |
| `Many(n)` | n rows | `watch list unusable` |
| `Empty` | blank or non-string value | `watch list unusable` |

The plan script maps the same three cases to the fixed lines of contracts/script-changes.md.

## Plan rows

`Row` and the status records that carry a repo gain `entry: usize` (the `number` of the entry they came from). `Candidates.unchecked` holds `entry N` strings.

## List file (tick VM)

Created by `mktemp` in `$RUNNER_TEMP` (mode 0600, exclusive), holds the key's value, removed after phase A and on every failure path. Never uploaded.

## Scrub strings

Derived by git commands, not stored: contracts/history-rewrite.md.
