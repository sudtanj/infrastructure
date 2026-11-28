# Feature Specification: Step secrets for the ci command

**Feature Branch**: `010-ci-step-secrets`

**Created**: 2026-11-28

**Status**: Draft

**Input**: User description: "Let a step of a pipeline run by the manual `ci` command receive Bitwarden secrets. Each watch entry names the secrets its repo may request. The first user is a Playwright browser test project in sudtanj/factory-monorepo that needs a test login and a target URL. Release the runner as 0.3.0 and bump the workflow's `RUNNER_IMAGE` pin."

## Who this is for

The owner, who starts `factory-monorepo-ci` by hand and writes the `.factory-ci.toml` files of the watched repos. Today the first pipeline that needs a credential cannot run through `ci` at all.

## Problem

`ci` refuses every pipeline that lists `secrets`. Evidence:

- `rust-ci-runner/src/ci.rs` (`run_checkout`) loads the pipeline with a lookup that returns nothing (`prepare(&dir, None, |_| None, true)`), so any step with `secrets` ends the run as `pipeline file refused`, and it hands `run_pipeline` an empty map (`BTreeMap::new()`).
- `README.md` and `rust-ci-runner/README.md` say it outright: "Steps get no pipeline secrets here; a step that lists `secrets` fails the run."
- Feature 008 listed step secrets as out of scope (FR-022). The owner has now reversed that.
- The Playwright project in `sudtanj/factory-monorepo` needs a test login and a target URL. Putting them in the committed `env` table is not an option: `env` values become engine arguments, show in `ps` and sit in a public-readable history of a private repo. GitHub secrets are not an option either: the constitution (Principle II) keeps pipeline secrets in Bitwarden and the workflow holds only `BWS_ACCESS_TOKEN`.

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

- Q: Do step secrets belong in `ci`? → A: Yes. Owner decision. First user: the Playwright browser test project in `sudtanj/factory-monorepo` (a test login and a target URL).
- Q: Which watch key is the allowlist? → A: The existing `secrets`. The tick reads it as the shared key names a repo may read. `ci` reads it as the names a pipeline may request. No new key, so the watch file format and `plan --check` do not change.
- Q: Which Bitwarden keys does `ci` read for a step? → A: The shared key whose name equals the secret name, exact case, in the project the access token can read. The tick also reads a repo-own key `owner/name<SEP>NAME`. `ci` does not: the separator (`/`, or `:` if the store refuses `/`) is a shell variable of the tick script, and the Rust runner has no copy of it. One lookup rule is enough for the first user. The allowlist is the repo boundary.
- Q: Does `secret_branches` apply to `ci`? → A: No. `ci` still ignores it. The gate is `branches`, which `ci` already checks before it reads Bitwarden. `branches = ["main"]` is the whole protection for both watched repos today (risk R3).
- Q: Which public fault lines are new? → A: Two, with no secret name in either: `step secret not allowed` and `step secret unavailable`. Names and counts go to Loki as step `ci`. The README says logs show no secret name, and this keeps that true.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A browser test project runs with its login (Priority: P1)

The owner adds `secrets = ["E2E_BASE_URL", "E2E_USER", "E2E_PASSWORD"]` to the Playwright step and to the `sudtanj/factory-monorepo` watch entry, stores three keys in Bitwarden and starts `factory-monorepo-ci` on `main`. The step sees the three variables, the tests pass, the Actions log shows fixed lines.

**Why this priority**: It is the reason for the feature. Without it the Playwright project cannot run through `ci`.

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

- Inputs. A name listed twice in one step or in two steps: one lookup, one value (FR-007). A name that differs only by case from an allowed one: not allowed, because names match exactly (FR-003). A value with newlines: allowed when every trimmed line has 4 characters or more, masked line by line (FR-006). A value with NUL, empty, or a line under 4 characters: `step secret unavailable` (FR-006). A value with Unicode or `=`: passes as it is, since it travels as an environment value and not in argv. An empty allowlist, the default: every step secret is refused, so an entry written before this feature behaves as before except for the fault line (FR-002).
- State. No state is kept between runs. A crash mid-run leaves only the checkout directory, which the existing cleanup removes. Running twice is safe.
- Partial failure. The lookup finishes for all names before the first container, so step two never fails for a missing value after step one ran (FR-005).
- Time and concurrency. Concurrent runs of one repo, ref and project queue (existing concurrency group). Runs of different repos read the store independently. `ci` makes no new external call; the store's existing time limit and one retry apply.
- Resources. A value so large that the OS refuses it as an environment string makes the step fail to start and ends the run in the existing `runner fault`; no cap is added.
- Trust. Who can name a secret: the watch list, a reviewed file. What the pipeline author cannot do: widen the list, read a runner setting, read a name in the store that no entry allows. The pipeline `env` table cannot carry a secret value out of the store, because only `secrets` names are looked up.
- UI. No UI. The only surface is the fixed stdout and stderr lines (FR-010).

## Requirements *(mandatory)*

Each requirement is an acceptance criterion in the form WHEN trigger THE SYSTEM SHALL response. Tags: `[error]` is a failure path, `[boundary]` is an edge of an allowed range. Each maps to at least one unit test under `cargo test --lib` (Principle V).

### Functional Requirements

**Allowlist**

- **FR-001**: WHEN a step lists a secret name and the watch entry of the run's repo lists that name in `secrets` THE SYSTEM SHALL look up that name for the step. A name is allowed only by the entry of the repo being run; no other entry matters.
- **FR-002** `[error]`: WHEN any step of the fetched pipeline lists a name that is not in the entry's `secrets` (including when the entry has no `secrets` key) THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] step secret not allowed`, start no container, and read no value for any name. The offending name goes to Loki as step `ci`, not to stdout or stderr.
- **FR-003** `[boundary]`: WHEN a step lists a name that equals an allowed name except for case THE SYSTEM SHALL treat it as not allowed (FR-002). WHEN a step lists exactly the one allowed name and the allowlist has exactly one entry THE SYSTEM SHALL allow it.
- **FR-004** `[error]`: WHEN a watch entry lists `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER` or `FACTORY_CI_LOG_TOKEN` in `secrets` THE SYSTEM SHALL refuse the whole watch file (`plan --check` reports it, and `ci` ends with `[runner] repository or ref not in the watch list`). WHEN a pipeline lists one of the four THE SYSTEM SHALL end the run with `[runner] pipeline file refused`. Neither case ever puts one of the four into a step's environment, whatever the allowlist holds. A test lists each of the four names in both places.

**Lookup**

- **FR-005**: WHEN every requested name is allowed THE SYSTEM SHALL resolve all values, from the same fixed `bws` binary and the same access token the run already uses and with no second token, before the first container starts, and SHALL start no container if any one fails.
- **FR-006** `[error]`: WHEN an allowed name has zero matching keys in the store, more than one, an empty value, a value with a NUL, or a value with a trimmed line under 4 characters THE SYSTEM SHALL end the run with exit code 1, print exactly `[runner] step secret unavailable`, and start no container. Names and counts go to Loki as step `ci`. A key matches only when its whole key equals the name.
- **FR-007** `[boundary]`: WHEN several steps list the same name THE SYSTEM SHALL look it up once. WHEN a value has a trimmed line of exactly 4 characters THE SYSTEM SHALL accept it, and WHEN a line has 3 THE SYSTEM SHALL refuse it (FR-006).

**Containment**

- **FR-008**: WHEN a step starts THE SYSTEM SHALL give its container only the values of the names that step lists, passed through the environment of the engine child process (`-e NAME`), never in argv, and never to a step that does not list the name.
- **FR-009** `[error]`: WHEN a step lists a secret and has `docker = true` THE SYSTEM SHALL refuse the run with `[runner] docker steps are refused`, even when `--allow-docker` is given and the entry sets `allow_docker`.
- **FR-010**: WHEN the run is on a ref THE SYSTEM SHALL keep the existing gate: only a ref that matches the entry's `branches` runs, and the check stays before any Bitwarden call. Today that is `main`. `secret_branches` stays ignored by `ci`.

**Output**

- **FR-011**: WHEN a secret value is read THE SYSTEM SHALL add it to the redactor of every output path (stdout, stderr, the log shipper and the detail lines of step `ci`) before any line that could contain it is written, so the value never reaches Loki or the public log in plain form. Masking covers the value, its base64 and each line of 4 or more characters of a multi-line value, as the tick does.
- **FR-012**: WHEN the run prints THE SYSTEM SHALL keep stdout to the existing closed grammar and stderr to the closed fault list, extended by exactly two lines, `step secret not allowed` and `step secret unavailable`. No secret name, count or value appears in any public line. The two lines join the closed list in `log.rs` and its test.
- **FR-013** `[error]`: WHEN a step prints a secret value, the value in base64, or the value inside a longer line THE SYSTEM SHALL write `***` in its place in Loki and print nothing of it publicly. (A value that a step transforms in another way, or splits across two lines, is not masked. See R2.)

**Rollout**

- **FR-014**: THE SYSTEM SHALL release the runner as 0.3.0: `Cargo.toml` and `Cargo.lock` say `0.3.0`, and the tag `rust-ci-runner-v0.3.0` goes on a commit on `main`. The release workflow's smoke step still passes (`ci` against a repo outside the watch list prints `repository or ref not in the watch list` and exits 1).
- **FR-015**: WHEN the release has published its digest THE SYSTEM SHALL change `RUNNER_IMAGE` in `.github/workflows/factory-monorepo-ci.yml` to `ghcr.io/sudtanj/rust-ci-runner:0.3.0@sha256:<digest>` in a separate, reviewed commit after the tag, as the README's bootstrap order says. Until then the 0.2.0 pin keeps working and refuses step secrets.
- **FR-016**: THE SYSTEM SHALL add the secret names of the Playwright project to the `sudtanj/factory-monorepo` entry in `.github/factory-ci-repos.toml`, and update the comment block at the top of that file, which says `ci` ignores `secrets`. The `sudtanj/infrastructure` entry gets no `secrets`.
- **FR-017**: THE SYSTEM SHALL update `README.md`, `rust-ci-runner/README.md` and the GitHub repository description to match (the sentence "Steps get no pipeline secrets here", step 5 of the `ci` command list, the Bitwarden project guidance "holds only the four keys", the fault list and the risks). This is the last task in `tasks.md`.

### Key Entities

- **Watch entry**: `.github/factory-ci-repos.toml`, one table per repo. `secrets` is now read by `ci` as the allowlist of step secret names. Reviewed on `main` of B.
- **Step secret**: a name listed in a step's `secrets`, backed by one Bitwarden key of the same name. Never a runner setting.
- **Runner settings**: `FACTORY_CI_TOKEN`, `FACTORY_CI_LOG_URL`, `FACTORY_CI_LOG_USER`, `FACTORY_CI_LOG_TOKEN`. Read by the runner, never handed to a step.
- **Public line**: a stdout or stderr line from the closed grammar, now with two more fault lines.

## Success Criteria *(mandatory)*

### Hypothesis

If `ci` can hand named, allowlisted secrets to a step, the owner can run the Playwright project from one manual start, with no secret in GitHub, in the repo or in the public log. We would be wrong if the owner still needs another way to supply the login (for example per-repo key prefixes, or secrets for docker steps) before the first green browser run.

### Measurable Outcomes

- **SC-001**: One manual start of `factory-monorepo-ci` on `main` runs the Playwright project of `sudtanj/factory-monorepo` on the 0.3.0 pin, with its three secrets, and ends with `[runner] pipeline passed`. Measured by that run's log and exit code.
- **SC-002**: Across a hostile fixture (unlisted name, runner setting, secret printed in plain and base64, secret in argv-like text), zero lines of stdout, stderr, child argv and Loki bodies hold a canary value, and zero public lines hold a secret name. Measured by the unit and fixture tests.
- **SC-003**: A pipeline that names a secret outside the allowlist, or one with no stored value, starts zero containers. Measured by a test that counts engine calls.
- **SC-004**: After the first week of use, no run ended in a `step secret` fault that the owner did not expect. Measured by reading the Actions run list for the two fault lines. If the owner hits `step secret not allowed` for a name they believed was allowed, that is a documentation failure and goes back to the README.
- **SC-005**: `cargo test --lib` passes in the `rust-ci-runner` pipeline with tests for FR-001 to FR-013, and the release publishes a 0.3.0 digest that the workflow pin matches.

## Assumptions

- The Bitwarden project the machine account can read holds the three Playwright keys next to the four runner keys. The account's "Can read" grant is on that one project. Anyone who can push `main` of an allowlisted repo can request any allowlisted name, and the account can read the whole project (existing README caveat).
- `ci` reuses the one `bws secret list` it already runs, or makes one more with the same token. The plan picks; the spec only fixes that no other token, binary or channel is used.
- Secret names for the Playwright project are placeholders in this spec: `E2E_BASE_URL`, `E2E_USER`, `E2E_PASSWORD`. The owner can rename them in the watch entry, the pipeline and Bitwarden. They pass the existing name rules.
- The values for the three keys are created by the owner in Bitwarden. No code or doc holds a value.
- The tick is not in use and is due to be retired. Adding `secrets` to the `sudtanj/factory-monorepo` entry also makes the tick read those shared names for that repo on its `secret_branches`. This is accepted, and the README notes it.
- A target URL is a secret value here, so it is masked in Loki wherever it appears. That is wanted.
- Existing entries and pipelines that list no secret behave exactly as before. A pipeline that listed secrets was refused before and is refused now when its repo has no allowlist; only the fixed line changes (`pipeline file refused` becomes `step secret not allowed`).

## Non-goals

- Docker steps with secrets, and any use of `--allow-docker` (FR-009).
- Schedules, artifacts and caches.
- The tick workflow and its scripts. The watch file is shared, so a `secrets` edit reaches the tick (see Assumptions).
- Repo-own keys (`owner/name<SEP>NAME`) and any key prefix rule.
- Applying `secret_branches`, or looking up the default branch, in `ci`.
- Per-step or per-run allowlists, secret rotation, expiry, or a store other than Bitwarden.
- Masking of transformed or split values (R2), and any change to the Redactor's encodings.
- A cap on value length.
- New workflow inputs, new Environment secrets, or any change to `factory-monorepo-ci.yml` beyond the `RUNNER_IMAGE` pin.

## Risks

- **R1, scope of trust**: any commit on `main` of an allowlisted repo can read its allowlisted values and print them to Loki or send them over the network. The allowlist narrows what a pipeline can ask for. It does not make the pipeline trustworthy. Mitigation: `main` protection on the watched repos, and listing only the names the pipeline needs.
- **R2, masking is exact**: a step that base64-encodes (other than the plain base64 the redactor knows), URL-encodes, splits or transforms a value across lines puts it in Loki unmasked. Existing, documented limit; the README keeps it. Loki access stays restricted.
- **R3, `branches` is the only ref gate**: widening `branches` in a watch entry (for example to `*`) gives every matching branch the secrets, because `ci` ignores `secret_branches`. Mitigation: the README says so next to the allowlist; revisit if a repo needs a wider `branches`.
- **R4, shared key names**: every allowlisted repo that names `E2E_PASSWORD` gets the same value. No per-repo isolation exists in this version. Mitigation: use distinct names per repo.
- **R5, docker inspect**: a value in a step container's environment is visible to anything that can reach the host docker socket. The runner container holds that socket on an ephemeral GitHub-hosted VM, as 008 accepts.
- **R6, fault line change**: scripts that match `pipeline file refused` for a secret-listing pipeline stop matching. None exist in this repo.
- **R7, rollout order**: bumping the pin before the digest exists fails the pull; bumping it before the watch entry and Bitwarden keys exist fails the first run with a fixed line. The README's bootstrap order covers the first. FR-015 and FR-016 cover the second.
