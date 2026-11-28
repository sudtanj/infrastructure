# Feature Specification: Step secrets for the ci command

**Feature Branch**: `010-ci-step-secrets`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "Let a step of a pipeline run by the manual `ci` command receive Bitwarden secrets. Each watch entry names the secrets its repo may request. The first consumer is any non-login secret a step needs, such as an API key for a site the owner controls; the Playwright login comes from feature 011 vault items. Release the runner as 0.3.0 and bump the workflow's `RUNNER_IMAGE` pin."

## Who this is for

The owner, who starts `factory-monorepo-ci` by hand and writes the `.factory-ci.toml` files of the watched repos. Today the first pipeline that needs a credential cannot run through `ci` at all.

## Problem

`ci` refuses every pipeline that lists `secrets`. Evidence:

- `rust-ci-runner/src/ci.rs` (`run_checkout`) loads the pipeline with a lookup that returns nothing (`prepare(&dir, None, |_| None, true)`), so any step with `secrets` ends the run as `pipeline file refused`, and it hands `run_pipeline` an empty map (`BTreeMap::new()`).
- `README.md` (line 55) says it outright: "Steps get no pipeline secrets here; a step that lists `secrets` fails the run." `rust-ci-runner/README.md` (step 5 of the `ci` list) says the run "refuses a docker step or a declared secret".
- Feature 008 listed step secrets as out of scope (FR-022). The owner has now reversed that.
- A step in a watched repo can need a credential that is not a vault login, such as an API key for a site the owner controls. The Playwright test login belongs to feature 011 (vault items unlocked by the runner). Putting such a value in the committed `env` table is not an option: `env` values become engine arguments, show in `ps` and sit in a public-readable history of a private repo. GitHub secrets are not an option either: the constitution (Principle II) keeps pipeline secrets in Bitwarden and the workflow holds only `BWS_ACCESS_TOKEN`.

The tick path already solves most of this, and this spec reuses it:

- The watch entry key `secrets` already exists. It lists shared Bitwarden key names a repo may read, and `watch.rs` already refuses reserved or malformed names in it (`valid_secret_name`), including every `FACTORY_CI*` name.
- `engine.rs` already passes a step's secrets as `-e NAME` with the value in the engine child's environment, never in argv, and only for names the step lists.
- `redact.rs` already masks a value, its base64 forms and each line of a multi-line value of 4 or more characters.
- The tick refuses a value with a line shorter than 4 characters, because masking so short a line would hide ordinary text.

What the tick does not give `ci`: the lookup. The tick resolves secrets in a shell script (`factory-ci-secrets.sh`) and hands the runner finished environment variables. `ci` runs inside one container with the token in memory, so the runner must do the lookup itself.

## Announcement

Pipelines run by `factory-monorepo-ci` can now use secrets. List the names a step needs in `secrets` in its `.factory-ci.toml`, add the same names to `secrets` in that repo's entry in `.github/factory-ci-repos.toml`, and store the values in the Bitwarden project the runner already reads. The step gets those values as environment variables and nothing else. A pipeline that asks for a name its repo's entry does not allow fails before any container starts, with one fixed line in the public log. The runner's own settings can never be handed to a step. The values are masked in the Loki log, and the public Actions log still shows only fixed lines. Runner 0.3.0 carries the change; the workflow pins its digest.

## Clarifications

### Session 2026-11-28 (owner decision, planning calls)

- Q: Do step secrets belong in `ci`? → A: Yes. Owner decision. First consumer (revised after the critic review): any non-login secret a step needs, such as an API key for a site the owner controls. The Playwright test login comes from feature 011 vault items, not from Secrets Manager keys.
- Q: Which watch key is the allowlist? → A: The existing `secrets`. The tick reads it as the shared key names a repo may read. `ci` reads it as the names a pipeline may request. No new key, so the watch file format and `plan --check` do not change.
- Q: Which Bitwarden keys does `ci` read for a step? → A: The shared key whose name equals the secret name, exact case, in the project the access token can read. The tick also reads a repo-own key `owner/name<SEP>NAME`. `ci` does not: the separator (`/`, or `:` if the store refuses `/`) is a shell variable of the tick script, and the Rust runner has no copy of it. One lookup rule is enough for the first user. The allowlist is the repo boundary.
- Q: Does `secret_branches` apply to `ci`? → A: Changed after the security review (2026-11-28, finding S1). When the pipeline requests secrets and the entry sets `secret_branches`, the ref must match it. When the entry sets none, no ref gets secrets (review fix M3, 2026-11-28: `branches` no longer counts). Either failure prints `repository or ref not in the watch list`, before any value lookup. A pipeline that requests no secret keeps the old gate (`branches`). See FR-010.
- Q: Which public fault lines are new? → A: Two, with no secret name in either: `step secret not allowed` and `step secret unavailable`. Names and counts go to Loki as step `ci`. The README says logs show no secret name, and this keeps that true.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A step runs with a secret it needs (Priority: P1)

The owner adds `secrets = ["SITE_API_KEY"]` (a placeholder name) to a step and to the `sudtanj/factory-monorepo` watch entry, stores the key in Bitwarden and starts `factory-monorepo-ci` on `main`. The step sees the variable, passes, and the Actions log shows fixed lines. Test logins are not this story: they come from feature 011.

**Why this priority**: It is the reason for the feature. Without it no step can use a credential through `ci`.

**Independent Test**: A fixture pipeline with one step that prints each secret and a mocked store holding the three keys. Run `ci`. The step's container received exactly those three variables, Loki got the output with the values masked, and stdout and stderr hold only grammar lines.

**Acceptance Scenarios**:

1. **Given** the entry allows the three names, the store holds each once, and the pipeline lists them in one step, **When** `ci` runs, **Then** the step starts with the three variables set and the run ends in `[runner] pipeline passed`.
2. **Given** two steps and only the second lists a name, **When** the run goes, **Then** the first step's container has no such variable.

---

### User Story 2 - A branch cannot widen its own access (Priority: P1)

The pipeline file comes from the fetched branch, which is untrusted input. Only the watch list, a reviewed file on B's `main`, decides which secrets a repo may request.

**Why this priority**: Without it, anyone who can push `main` of a watched repo could ask for any secret in the project, including other repos' secrets.

**Independent Test**: Fixture pipelines that request a name outside the allowlist, a name in the store that no entry allows, and each of the four runner settings, with those four also placed in the allowlist.

**Acceptance Scenarios**:

1. **Given** a pipeline requests a name that is not in the entry's `secrets`, **When** `ci` reads the pipeline, **Then** the run ends before any container starts with `[runner] step secret not allowed`.
2. **Given** the entry lists one of the four runner settings, **When** `ci` reads the watch list, **Then** the file is refused as a whole.

---

### User Story 3 - A missing or unusable value stops the run early (Priority: P1)

A key was never created, exists twice, is empty, or has a line under 4 characters. The run must say so before it spends container time, and must not hand a half set of variables to a step.

**Why this priority**: A browser test that runs without its login fails late and confusingly.

**Independent Test**: Mock store outputs with a missing key, a duplicate key, an empty value and a value with a 3-character line.

**Acceptance Scenarios**:

1. **Given** an allowed name has no key in the store, **When** the run prepares, **Then** it ends with `[runner] step secret unavailable` and starts no container.

---

### User Story 4 - Nothing leaks (Priority: P1)

The Actions log is public and Loki is the only place the owner reads details.

**Why this priority**: Constitution Principle III. A leak cannot be undone.

**Independent Test**: A step that prints its secret, its base64, and the secret inside a longer line. Seed each value with a unique canary. Search stdout, stderr, the argv of every child process and every Loki body for the canary.

**Acceptance Scenarios**:

1. **Given** a step prints a secret value, **When** the run ends, **Then** the value appears nowhere in stdout, stderr, any child argv, or any Loki body, and Loki holds `***` in its place.

### Edge Cases

Answers to the edge-case checklist, for this change. "Handled" names the requirement that covers it.

- Inputs. A name listed twice in one step or in two steps: one lookup, one value (FR-007). A name that differs only by case from an allowed one: not allowed, because names match exactly (FR-003). A value with newlines: allowed when every non-blank trimmed line has 4 characters or more, masked line by line; blank inner lines are allowed (FR-006). A value that is empty, blank, holds a control character other than `\n` and `\r\n`, or has a non-blank trimmed line under 4 characters: `step secret unavailable` (FR-006). A value with Unicode or `=`: passes as it is, since it travels as an environment value and not in argv. An empty allowlist, the default: every step secret is refused, so an entry written before this feature behaves as before except for the fault line (FR-002).
- State. No state is kept between runs. A crash mid-run leaves only the checkout directory, which the existing cleanup removes. Running twice is safe.
- Partial failure. The lookup finishes for all names before the first container, so step two never fails for a missing value after step one ran (FR-005).
- Time and concurrency. Concurrent runs of one repo, ref and project queue (existing concurrency group). Runs of different repos read the store independently. `ci` makes no new store call: the one call it already makes (existing time limit, one retry without backoff) also returns the rows of the allowlisted keys. The call returns the whole project; a list over 16 MiB ends as the existing `secret store unreachable`.
- Resources. A value so large that the OS refuses it as an environment string makes the step fail to start and ends the run in the existing `runner fault`; no cap is added.
- Trust. Who can name a secret: the watch list, a file on `main` of B that only the owner can write (the owner is the only collaborator, checked 2026-11-28). Secret names are public in that file; nothing here hides a name. What the pipeline author cannot do: widen the list, read a runner setting, read a name in the store that no entry allows. The pipeline `env` table cannot carry a secret value out of the store, because only `secrets` names are looked up.
- UI. No UI. The only surface is the fixed stdout and stderr lines (FR-010).

## Requirements *(mandatory)*

Each requirement is an acceptance criterion in the form WHEN trigger THE SYSTEM SHALL response. Tags: `[error]` is a failure path, `[boundary]` is an edge of an allowed range. Each maps to at least one unit test under `cargo test --lib` (Principle V).

### Functional Requirements

**Allowlist**

- **FR-001**: WHEN a step lists a secret name and the watch entry of the run's repo lists that name in `secrets` THE SYSTEM SHALL look up that name for the step. A name is allowed only by the entry of the repo being run; no other entry matters.
- **FR-002** `[error]`: WHEN any step of the fetched pipeline lists a name that is not in the entry's `secrets` (including when the entry has no `secrets` key) THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] step secret not allowed`, start no container, and validate, redact or hand on no value. The offending name goes to Loki as step `ci`, not to stdout or stderr.
- **FR-003** `[boundary]`: WHEN a step lists a name that equals an allowed name except for case THE SYSTEM SHALL treat it as not allowed (FR-002). WHEN a step lists exactly the one allowed name and the allowlist has exactly one entry THE SYSTEM SHALL allow it.
- **FR-004** `[error]`: WHEN a watch entry lists `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` or `FACTORY_CI_LOG_TOKEN` in `secrets` THE SYSTEM SHALL refuse the whole watch file (`plan --check` reports it, and `ci` ends with `[runner] repository or ref not in the watch list`). WHEN a pipeline lists one of the four THE SYSTEM SHALL end the run with `[runner] pipeline file refused`. Neither case ever puts one of the four into a step's environment, whatever the allowlist holds. A test lists each of the four names in both places. The `FACTORY_CI` prefix of the name rule is what refuses them; one list of the four required runner keys (`REQUIRED_KEYS`) serves the store read and the public fault check. The name rule also reserves the prefixes `BW_` and `BITWARDEN`.

**Lookup**

- **FR-005**: WHEN every requested name is allowed THE SYSTEM SHALL resolve all values from the one `bws secret list` call the run already makes (it keeps the raw rows of the entry's allowlisted keys, at most 20, and validates them only when a step requests them), with the same fixed `bws` binary and access token and no second call, before the first container starts, and SHALL start no container if any one fails.
- **FR-006** `[error]`: WHEN an allowed name has zero matching keys in the store, more than one, an empty or blank value, a value with a control character other than `\n` and `\r\n` (NUL, lone `\r`, `\x0b`, `\x0c`, U+0085, U+2028, U+2029 included), a value over 32 KiB, a single-line value with leading or trailing whitespace (a trailing newline included; a multi-line value may end in a newline), or a value with a non-blank trimmed line under 8 characters THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] step secret unavailable`, and start no container. Names and counts go to Loki as step `ci`. A key matches only when its whole key equals the name.
- **FR-007** `[boundary]`: WHEN several steps list the same name THE SYSTEM SHALL look it up once. WHEN a value has a trimmed line of exactly 8 characters THE SYSTEM SHALL accept it, and WHEN a non-blank line has 7 THE SYSTEM SHALL refuse it (FR-006). A blank inner line is accepted.

**Containment**

- **FR-008**: WHEN a step starts THE SYSTEM SHALL give its container only the values of the names that step lists, passed through the environment of the engine child process (`-e NAME`), never in argv, and never to a step that does not list the name.
- **FR-009** `[error]`: WHEN a pipeline has a step that lists a secret and any step (the same or another) with `docker = true` THE SYSTEM SHALL refuse the run with `[runner] docker steps are refused`, even when `--allow-docker` is given and the entry sets `allow_docker`. (Security S2: the docker sidecar is privileged, so a no-secret docker step could reach the secret step's container.)
- **FR-010** `[error]`: WHEN the fetched pipeline requests any secret THE SYSTEM SHALL check the ref before any value lookup. If the entry sets `secret_branches`, the ref must match one of them. If the entry sets none, no ref gets secrets (fail closed): `branches` plays no part, whatever its patterns. On failure THE SYSTEM SHALL print `[runner] repository or ref not in the watch list`, exit 1 and start no container. The existing `branches` gate for the run itself stays before any Bitwarden call. A pipeline with no secrets is unaffected. The `sudtanj/factory-monorepo` entry needs `secret_branches = ["main"]` before its first secret run (T023).

**Output**

- **FR-011**: WHEN a requested secret value is read THE SYSTEM SHALL add all requested values, in one batch, to the redactor of every output path (stdout, stderr, the log shipper and the detail lines of step `ci`) before any line that could contain it is written, so the value never reaches Loki or the public log in plain form. Masking always covers the value and each trimmed line of it of 4 or more characters. Derived forms, each of 8 or more characters and none over 48 KiB, cover its base64 (with the stable middle at the three byte alignments, so a prefix or a trailing newline does not hide it), each 76-column line of the standard base64 of the value's whole 3-byte groups when that is longer than 76 characters (so base64 line wrapping does not hide it; pieces under 8 characters are not added and every piece counts against the 48 KiB cap), URL-safe and unpadded base64, percent-encoding, JSON-string escaping, lowercase hex, and each line of 4 or more characters of a multi-line value.
- **FR-012**: WHEN the run prints THE SYSTEM SHALL keep stdout to the existing closed grammar and stderr to the closed fault list, extended by exactly two lines, `step secret not allowed` and `step secret unavailable`. No secret name, count or value appears in any public line. The two lines join the closed list in `log.rs` and its test. The public line is built from the fixed grammar, not from redacted text, so a value such as `pass` cannot change `[runner] pipeline passed`; Loki still gets the redacted text.
- **FR-013** `[error]`: WHEN a step prints a secret value, any form listed in FR-011, or the value inside a longer line THE SYSTEM SHALL write `***` in its place in Loki and print nothing of it publicly. A value that straddles the 64 KB read boundary of the step output pump is masked too: the pump cuts before the start of any such value instead of through it. (A value that a step transforms in another way, or splits across two lines, is not masked. See R2.)

**Rollout**

- **FR-014**: THE SYSTEM SHALL release the runner as 0.3.0: `Cargo.toml` and `Cargo.lock` say `0.3.0`, and the tag `rust-ci-runner-v0.3.0` goes on a commit on `main`. The release workflow's smoke step still passes (`ci` against a repo outside the watch list prints `repository or ref not in the watch list` and exits 1).
- **FR-015**: WHEN the release has published its digest THE SYSTEM SHALL change `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest>` in a separate commit after the tag, as the README's bootstrap order says. The pin commit goes to a branch first, together with a commit that adds `.github/workflows/factory-monorepo-ci.yml` to the `paths` of `factory-ci-selftest.yml` (its glob `factory-ci*.yml` does not match that name), so the selftest job `shell` runs `test-factory-monorepo-ci.sh`; then the branch fast-forwards to `main`. Until then the 0.2.0 pin keeps working and refuses step secrets. After the pin and before any name is added, a no-secret canary run (`repo=sudtanj/infrastructure`, `ref=main`, `project=rust-ci-runner`) must end in `pipeline passed`.
- **FR-016**: THE SYSTEM SHALL update the comment block at the top of `.github/factory-ci-repos.toml`, which says `ci` ignores `secrets`. It SHALL add to the `sudtanj/factory-monorepo` entry only the names that are not vault items. The login values come from feature 011 (vault items unlocked by the runner), so the Playwright login names are not added here; the only candidate is `BROWSER_TEST_URL`, and only if the owner treats the URL as secret. If no name is needed, the entry stays as it is. The one exception is the temporary canary name of FR-019, removed after the canary run. The `sudtanj/infrastructure` entry never gets `secrets`. No secret name appears in two entries of the committed file (a test checks it).
- **FR-018**: THE SYSTEM SHALL keep the lookup behind one interface, names in and name-to-value map out (`Held::take` over the rows the first call kept), so another source can replace it. Errors are keyed by position in the requested list and carry no name; `ci.rs` maps a position to its name for the Loki detail. Contract v1 is in `contracts/ci-secrets.md`. Types that hold a token or values SHALL NOT derive `Debug`, and a test checks that `{:?}` of them and of `NamedError` print no value.
- **FR-019**: BEFORE the first run with real values THE SYSTEM SHALL have a run with canary values (printed plain, as `echo "$V" | base64` and percent-encoded) whose Loki output the owner checks; and BEFORE the first secret run the owner re-checks the preconditions in `plan.md` (Environment `factory-ci` limited to `main`, collaborators of both repos, no Dependabot or Renovate auto-merge on `factory-monorepo` `main`).
- **FR-017**: THE SYSTEM SHALL update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description to match (the sentence "Steps get no pipeline secrets here", step 5 of the `ci` command list, the Bitwarden project guidance "holds only the four keys", the fault list and the risks). This is the last task in `tasks.md`.

### Key Entities

- **Watch entry**: `.github/factory-ci-repos.toml`, one table per repo. `secrets` is now read by `ci` as the allowlist of step secret names, per repo and not per Bitwarden project. It sits on `main` of B, where only the owner can write.
- **Step secret**: a name listed in a step's `secrets`, backed by one Bitwarden key of the same name. Never a runner setting.
- **Runner settings**: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. Read by the runner, never handed to a step.
- **Public line**: a stdout or stderr line from the closed grammar, now with two more fault lines.

## Review amendments (2026-11-28, before release 0.3.0)

Behaviour changes decided after the security review and QA, tracked as T017a in tasks.md:

- **H1, no step output in Loki.** WHEN a step receives secrets THE SYSTEM SHALL ship none of its output to Loki or any other sink. Its `started`, `success in`, `failed in` (with the exit code) and `skipped` lines still ship, and its output still prints where the log prints (never in public mode). There is no opt-in key. Steps without secrets ship their output, masked, as before. This replaces the masking claim of FR-013 for steps that hold secrets; the masking stays as a second layer.
- **M3, fail closed on the ref.** FR-010: without `secret_branches` no ref gets secrets. `branches` never opens them.
- **L5, floor of 8.** FR-006 and FR-007: every non-blank trimmed line of a value has at least 8 characters. A shorter value ends the run with `step secret unavailable`.
- **L6, no secret-named step in a public line.** WHEN a step name, after redaction, differs from the name THE SYSTEM SHALL print the public line as `[step] ...` without the name.
- **Forms.** FR-011 also covers `base64(value + "\n")`, 64-column wrapped base64 next to 76, lowercase percent-encoding, the `encodeURIComponent` set, `+` for a space, uppercase hex, and the JSON `\/` and `\uXXXX` escapes. The pump cut (FR-013) looks for a form that starts at the end of the chunk once, within the longest form, and then only for whole forms that cross the cut, so a run of a form's first character does not move it.
- **Tick and job refuse secrets (T017b).** WHEN the by-hand tick plans a repo THE SYSTEM SHALL set `secrets_ok` to false on every row, and WHEN the tick's job reads a pipeline that lists any secret THE SYSTEM SHALL end the item in `error` with the fixed reason `steps with secrets are not run by this job: use the ci command`, naming no secret, before any secret fetch and before any container. Secrets run only through the manual `ci` command; feature 011 retires the tick. The `ci` command keeps no allowlisted store row from its one `bws` call when the ref may not hold secrets.

## Success Criteria *(mandatory)*

### Hypothesis

If `ci` can hand named, allowlisted secrets to a step, the owner can run a pipeline that needs an API key from one manual start, with no secret in GitHub, in the repo or in the public log. We would be wrong if the owner still needs another way to supply it (for example per-repo key prefixes, or secrets for docker steps) before the first green run. Test logins are outside this hypothesis: feature 011 carries them.

### Measurable Outcomes

- **SC-001**: One manual start of `factory-monorepo-ci` on `main` runs a step of `sudtanj/factory-monorepo` on the 0.3.0 pin with a canary secret (then a real one) and ends with `[runner] pipeline passed`, with only fixed lines in the Actions log. Measured by that run's log and exit code. The Playwright login itself arrives with feature 011.
- **SC-002**: Across a hostile fixture (unlisted name, runner setting, secret printed in plain and base64, secret in argv-like text), zero lines of stdout, stderr, child argv and Loki bodies hold a canary value, and zero public lines hold a secret name. Measured by the unit and fixture tests.
- **SC-003**: A pipeline that names a secret outside the allowlist, or one with no stored value, starts zero containers. Measured by a test that counts engine calls.
- **SC-004**: After the first week of use, no run ended in a `step secret` fault that the owner did not expect. Measured by reading the Actions run list for the two fault lines. If the owner hits `step secret not allowed` for a name they believed was allowed, that is a documentation failure and goes back to the README.
- **SC-005**: `cargo test --lib` passes in the `rust-ci-runner` pipeline and in `factory-ci-selftest.yml` with tests for FR-001 to FR-013 and FR-018, and the release publishes a 0.3.0 digest that the workflow pin matches.

## Assumptions

- The Bitwarden project the machine account can read holds the step secret keys next to the four runner keys. The account's "Can read" grant is on that one project. Anyone who can push `main` of an allowlisted repo can request any allowlisted name, and the account can read the whole project (existing README caveat). `bws secret list` has no filter, so every lookup call fetches the whole project and keeps only the named keys.
- `ci` reuses the one `bws secret list` call it already makes: the call happens after the watch check, so the entry's allowlist is known. The spec fixes that no other token, binary, channel or second call is used. Values of allowlisted keys sit in memory from that call on, before any pipeline is read, but are validated, redacted and used only when a step requests them.
- The secret name in this spec (`SITE_API_KEY`) is a placeholder for tests and docs. It is not added to the watch entry by this feature (FR-016). It passes the existing name rules.
- Values are created by the owner in Bitwarden. No agent calls `bws` or `bw`, reads `BWS_ACCESS_TOKEN` or Environment secrets, or creates or reads Bitwarden keys. No code or doc holds a value.
- The tick is not in use and is due to be retired. Adding `secrets` to the `sudtanj/factory-monorepo` entry also makes the tick read those shared names for that repo on its `secret_branches`. This is accepted, and the README notes it.
- A target URL is a secret value here, so it is masked in Loki wherever it appears. That is wanted.
- Existing entries and pipelines that list no secret behave exactly as before. A pipeline that listed secrets was refused before and is refused now when its repo has no allowlist; only the fixed line changes (`pipeline file refused` becomes `step secret not allowed`).

## Non-goals

- Docker steps in a pipeline that lists secrets, and any use of `--allow-docker` with them (FR-009).
- Schedules, artifacts and caches.
- The tick workflow and its scripts. The watch file is shared, so a `secrets` edit reaches the tick (see Assumptions).
- Repo-own keys (`owner/name<SEP>NAME`) and any key prefix rule.
- Looking up the default branch in `ci`. (`secret_branches` is applied only as FR-010 says.)
- Per-step or per-run allowlists, secret rotation tooling, expiry, or a store other than Bitwarden. Rotation itself is a manual rollback step.
- Masking of values a step transforms beyond the forms in FR-011, or splits across two lines (R2).
- The Playwright login values and the vault items that hold them (feature 011). Chromium `/dev/shm` sizing belongs to `factory-monorepo` feature 002.
- A cap on value length.
- New workflow inputs, new Environment secrets, or any change to `factory-monorepo-ci.yml` beyond the `RUNNER_IMAGE` pin.

## Risks

- **R1, scope of trust**: owner-only write access, Environment limited to `main`. Checked with `gh api` on 2026-11-28: Environment `factory-ci` has a deployment branch policy of custom branches with exactly one policy, `main`, and no required-reviewer rule; the only collaborator on `sudtanj/infrastructure` and `sudtanj/factory-monorepo` is the owner. Any commit the owner pushes to `main` of an allowlisted repo can read its allowlisted values and print them to Loki or send them over the network. The allowlist narrows what a pipeline can ask for; it does not make the pipeline trustworthy. Steps share `/workspace`, so a step without secrets can plant code that a step with secrets then runs; pin step images by digest and use `npm ci --ignore-scripts` where possible. Mitigation: list only the names a pipeline needs, and re-check the access facts before the first secret run (T014). No Dependabot or Renovate auto-merge may exist on `factory-monorepo` `main`.
- **R2, masking has limits**: a step that transforms a value in a way FR-011 does not list (for example double encoding, XOR, splitting across lines) puts it in Loki unmasked. Derived forms need 8 characters, so the `echo "$V" | base64` form of a value under 6 bytes is not masked (the plain value and plain base64 still are). Masking the stable middle of a base64 form leaves up to about 4 bytes of the value's base64 at each end visible in Loki. Derived forms over 48 KiB are not added. A value with a prefix or suffix that is wrapped at 76 columns is masked only where one line holds a whole derived form. Loki access stays restricted.
- **R3, ref gate for secrets**: FR-010 closes every ref when `secret_branches` is missing. An owner who sets `secret_branches = ["*"]` gives all branches the values. The README says so.
- **R4, shared key names**: every allowlisted repo that names `SITE_API_KEY` gets the same value. No per-repo isolation exists in this version. Mitigation: use distinct names per repo.
- **R5, docker inspect**: a value in a step container's environment is visible to anything that can reach the host docker socket. The runner container holds that socket on an ephemeral GitHub-hosted VM, as 008 accepts.
- **R6, fault line change**: scripts that match `pipeline file refused` for a secret-listing pipeline stop matching. None exist in this repo.
- **R7, rollout order**: bumping the pin before the digest exists fails the pull; bumping it before the watch entry and Bitwarden keys exist fails the first run with a fixed line. The README's bootstrap order covers the first. FR-015 and FR-016 cover the second.
- **R8, `latest` moves**: the release workflow retags `:latest` to 0.3.0. Rolling the pin back does not move `latest`; see the rollback list in `plan.md`.
