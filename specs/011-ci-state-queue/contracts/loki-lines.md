# Contract: Loki lines of step `ci`

Every line is shipped by `log.out("ci", ...)` after redaction. `ci` is a reserved step name, so a pipeline step cannot forge these lines. No line holds a token, key, vault name or value. Each string below has a test in `ci_state_tests.rs`, and the alert rules grep them. `<...>` is a placeholder.

| String | When (0.4.0 unless marked 0.4.1) |
|---|---|
| `queue registered position <n>` | after the ticket is created |
| `queue claimed after <s>s` | the run takes the turn |
| `queue wait expired after <s>s` | the budget ended before a turn |
| `queue renewal failed after <s>s` | renewals failed for 600 s |
| `stale break ticket <name> age <s>s` | a stale ticket is deleted |
| `overlap lower ticket <name> live` | a renewal sees a live ticket with a lower id |
| `renew failed <reason>` | one renewal fails |
| `renewal stop killed <n> containers` | renewals failed for 600 s and the run killed its step containers before deleting the ticket |
| `rate limited pause <s>s` | the store pause starts |
| `restore none` | no state asset exists |
| `restore ok <bytes> bytes` | state restored |
| `restore failed <reason>` | restore error (no value) |
| `state saved <bytes> bytes asset <name>` | state saved |
| `state not saved <reason>` | no capture, empty, oversize, scan hit, fence lost |
| `artifact uploaded release <tag> asset <name>` | bundle uploaded (FR-029) |
| `artifact skipped path <n> missing` | a declared path does not exist |
| `artifact skipped path <n> not regular` | symlink or special file |
| `artifact too large <bytes> bytes` | over 100 MB |
| `artifact refused <n> causes` | scan refusal, a count only |
| `vault read <n> of <m> items` | 0.4.1, positions and counts only |
| `vault unavailable <reason>` | 0.4.0 for a refused declaration, 0.4.1 for a read failure; reason without names |

Public fault lines are separate (FR-037). Alert rules (`rust-ci-runner/alerts/factory-ci.yaml`) match these exact strings and the fault strings of `FAULTS`.
