# Data Model: Step secrets for the ci command

No stored data. These are the in-memory values of one `ci` run.

## Allowlist

- Source: `WatchEntry.secrets` (`rust-ci-runner/src/watch.rs`) of the one entry whose `repo` equals `--repo` and whose `branches` match `--ref`.
- Shape: list of names, at most 20, each passed `valid_secret_name`, no duplicates (all enforced at parse).
- Rule: a requested name is allowed when it equals an entry byte for byte. Empty list (default) allows nothing.

## Requested names

- Source: `Step.secrets` of each step of the fetched pipeline. Each name already passed `valid_secret_name` at parse.
- Derived: `Pipeline::secret_names()`, sorted, unique. `Pipeline::unlisted(allowed)` gives `(step, name)` pairs that are not allowed.

## Step secret values

- Type: `BTreeMap<String, String>`, name to value, one entry per unique requested name. Built by `store::fetch_named`; passed to `run_pipeline`, then to `RunCtx.secrets`; `engine.rs` reads it per step.
- Type: newtype `StepSecrets` over that map; its `Debug` prints names only.
- Validity per value: not blank; no control character other than `\n` and `\r\n`; every non-blank trimmed line 4 or more characters; blank inner lines allowed.
- Lifetime: the run. Not written anywhere. Nothing that holds a value or a token derives `Debug`.

## NamedError (new, `store.rs`)

| Variant | Meaning | Loki detail text (no value) |
|---|---|---|
| `Store(StoreError)` | call failed, output not a list, row without `key`, rejected token, refused server URL | the existing `StoreError` text |
| `Count(name, n)` | `n` rows have exactly this key; `n` is not 1 | `secret <name>: expected 1, found <n>` |
| `Unusable(name)` | value empty, NUL, or has a short line | `secret <name>: unusable value` |

None of these texts is a public line. They go to Loki as step `ci`. The public line for all three is `step secret unavailable`.

## Redactor

`Redactor` holds the list of strings to mask. `add(values)` appends, for each non-empty value: the value, its base64, the base64 of `x-access-token:<value>`, the stable middle of base64 at three alignments (standard, URL-safe, unpadded), percent-encoding, JSON-string escape, lowercase hex and each trimmed line of 4 or more characters; forms under 8 characters are skipped; then it sorts longest first and removes duplicates. `Log` owns it behind an `RwLock` and exposes the longest form for the output pump.

## Runner keys

`RUNNER_KEYS` (`store.rs`): `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. One list for the name rule, the lookup and the fault check.

## Ref gate

`WatchEntry::secrets_allowed_on(ref)`: `secret_branches` set: ref matches one of them. Unset: no `branches` pattern is made only of `*`, and the ref matches `branches`. Default `branches` is `["*"]`, so the default is refused.

## Public fault lines

`FAULTS` in `log.rs` goes from 14 to 16 entries; the two new ones are `step secret not allowed` and `step secret unavailable`. See [contracts/ci-secrets.md](contracts/ci-secrets.md).

## State of one run

```text
watch entry ok -> token -> first bws call -> Loki probe -> engine ok -> identity -> fetch
  -> parse pipeline -> docker rules -> allowlist -> [ref gate -> second bws call -> add_secrets] -> steps
every arrow can end the run with one fixed line; only the last one starts a container
```
