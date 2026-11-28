# Data Model: Step secrets for the ci command

No stored data. These are the in-memory values of one `ci` run.

## Allowlist

- Source: `WatchEntry.secrets` (`rust-ci-runner/src/watch.rs`) of the one entry whose `repo` equals `--repo` and whose `branches` match `--ref`. `job()` reads it before the store call.
- Shape: list of names, at most 20, each passed `valid_secret_name`, no duplicates (all enforced at parse).
- Rule: a requested name is allowed when it equals an entry byte for byte. Empty list (default) allows nothing.
- Carried in `Args.allowed` (`ci.rs`).

## Requested names

- Source: `Step.secrets` of each step of the fetched pipeline. Each name already passed `valid_secret_name` at parse.
- Derived: `Pipeline::secret_names()`, sorted, unique. `Pipeline::unlisted(allowed)` gives `(step, name)` pairs that are not allowed.

## Held rows (`store.rs`)

- `Fetched { secrets: Secrets, held: Held }`, returned by `store::fetch_with(bws, token, server, limit, allowed)` from the one `bws secret list` output.
- `Held`: map from allowlisted name to the list of values of rows whose whole key equals it (length 0 means the key is missing, 2 or more means duplicated). No other row is kept. `Debug` prints nothing of its content. Carried in `Args.held`.
- Lifetime: the run. A value is validated, redacted and used only when a step requests its name.

## Step secret values

- Type: `StepSecrets`, a newtype over `BTreeMap<String, String>` (name to value, one per unique requested name). `Held::take(names)` builds it; `.0` goes to `run_pipeline`, then `RunCtx.secrets`; `engine.rs` reads it per step. Its `Debug` prints names only.
- Validity per value (`Held::take`; rule in plan.md "Value rule"): at most 32 KiB; not blank; no control character other than `\n` and `\r\n`; a single-line value has no leading or trailing whitespace; a multi-line value has every non-blank trimmed line at 4 or more characters, blank inner lines and a trailing newline allowed.
- Lifetime: the run. Not written anywhere. Nothing that holds a value or a token derives `Debug`.

## NamedError (`store.rs`)

Keyed by position in the requested list, so no name, item or field is reachable from it and feature 011 can reuse it.

| Variant | Meaning | `Display` |
|---|---|---|
| `Count { index, found }` | `found` rows have exactly this key; `found` is not 1 (also: the name is not in the allowlist, `found` 0) | `secret #<index>: expected 1, found <found>` |
| `Unusable { index }` | the value breaks the value rule: empty or blank, control character, over 32 KiB, whitespace around a single-line value, or a non-blank line under 4 characters | `secret #<index>: unusable value` |

Store failures (no answer, rejected token, bad JSON, over 16 MiB) are the existing `StoreError` from `fetch_with`, with their existing public lines. `ci.rs` writes the Loki detail for a `NamedError` as `secret <name>: expected 1, found <n>` or `secret <name>: unusable value`, mapping `index` to the requested name (010 may log names, 011 may not). The public line is `step secret unavailable`.

## Redactor

`Redactor` holds the strings to mask. `add(batch)` appends, for each non-empty value: the value, its plain base64, the base64 of `x-access-token:<value>` and each trimmed line of 4 or more characters (all unconditional); and, for forms of 8 to 49152 characters, the stable middle of base64 at three alignments (standard, URL-safe, unpadded), percent-encoding, JSON-string escape, lowercase hex and, when the standard base64 of the whole 3-byte groups is longer than 76 characters, each of its 76-column lines. Then it sorts longest first and removes duplicates, once per batch. `forms()` returns them all. `Log` owns it behind an `RwLock` and exposes `add_secrets`, `secret_forms` and `longest_secret`; the output pump takes `secret_forms` and cuts before any form that would straddle its cut.

## Required runner keys

`REQUIRED_KEYS` (`store.rs`): `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. The four keys `parse_list` must find, and the list `log.rs` `is_fault` checks. The name rule refuses them by the `FACTORY_CI` prefix, not by this list.

## Ref gate

`WatchEntry::secrets_allowed_on(ref)`: `secret_branches` set: the ref matches one of them. Unset: no `branches` pattern is made only of `*` and `?` characters, and the ref matches `branches`. Default `branches` is `["*"]`, so the default is refused. Carried as `Args.ref_may_hold_secrets`.

## Public fault lines

`FAULTS` in `log.rs` gains two entries, `step secret not allowed` and `step secret unavailable`. Tests assert membership, not a count. See [contracts/ci-secrets.md](contracts/ci-secrets.md).

## State of one run

```text
watch entry ok (allowlist known) -> token -> the one bws call (runner values + held rows) -> Loki probe
  -> engine ok -> identity -> fetch -> parse pipeline -> docker rules -> allowlist
  -> [ref gate -> Held::take -> add_secrets] -> steps
every arrow can end the run with one fixed line; only the last one starts a container
```
