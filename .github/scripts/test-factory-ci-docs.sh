#!/usr/bin/env bash
# Docs and consistency tests (003 T009; FR-001, FR-002, FR-006, FR-007, FR-016, FR-022 to FR-024,
# FR-032; P6). Same `eq` style as test-factory-ci-hardening.sh. Plain grep, sed and awk; no
# network. Each check is a function `ck_X ROOT` that prints one violation per line and nothing
# when the tree is clean, so the mutation self-check can run it on a fixture tree.
#
# The patterns are assembled from parts so this file never matches its own scan (a).
#
# What the README must look like for (d) and (e), so the writer of T024 can follow it:
#   - a secrets or variables table has a first header cell starting with Name, Secret or Variable,
#     one row per name with the name in backticks in the first cell, and a column named
#     Environment or Scope, or one column per Environment (a header cell with `factory-ci`),
#     where the row has a value that is not empty, "no" or "none" in at least one of them;
#   - an Environments table has a first header cell starting with Environment and one row per
#     Environment, the backticked name in the first cell.
set -u
here=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$here/../.." && pwd)
total=0
failed=0

fail() { total=$((total + 1)); failed=$((failed + 1)); printf 'FAIL: %s\n' "$1"; }
pass() { total=$((total + 1)); }
eq() { if [ "$2" = "$3" ]; then pass; else fail "$1: want [$2] got [$3]"; fi; }
has() { case $2 in *"$3"*) pass ;; *) fail "$1: [$3] not in [$2]" ;; esac; }
lacks() { case $2 in *"$3"*) fail "$1: [$3] found in [$2]" ;; *) pass ;; esac; }

# Built from parts: a bare word, the central repo's old letter, and the old advice.
mono=mono; mono=${mono}repo
bee=$(printf '\102')
copy="separate"; copy="$copy public"; copy="$copy copy"
setup="Set up"; setup="$setup $bee"

tracked() {
  if [ -e "$1/.git" ] && git -C "$1" ls-files 2> /dev/null | grep -q .; then git -C "$1" ls-files
  else (cd "$1" && find . -type f | sed 's|^\./||'); fi
}
outside_spec() { grep -v -e '^specs/' -e '^\.specify/' -e '^\.claude/'; }

# ---------------------------------------------------------------- (a) FR-001, FR-005 wording
# `factory-` followed by the bare word is the full repo name and is allowed: strip it, then match.
ck_a() {
  local r=$1 f
  while IFS= read -r f; do
    [ -f "$r/$f" ] || continue
    sed "s/factory-$mono//g" "$r/$f" | grep -q "$mono" && echo "a: $f: bare $mono"
    grep -qF "$copy" "$r/$f" && echo "a: $f: $copy"
  done < <(tracked "$r" | outside_spec)
  while IFS= read -r f; do
    [ -f "$r/$f" ] || continue
    case $f in README.md | rust-ci-runner/README.md | .github/*) ;; *) continue ;; esac
    grep -qw "$bee" "$r/$f" && echo "a: $f: standalone $bee names the central repo"
  done < <(tracked "$r" | outside_spec)
  return 0
}

# ---------------------------------------------------------------- (b) FR-002 watch list
ck_b() {
  local f=$1/.github/factory-ci-repos.toml r n
  [ -f "$f" ] || { echo "b: watch list missing"; return 0; }
  for r in sudtanj/factory-monorepo sudtanj/infrastructure; do
    n=$(grep -c "^repo = \"$r\"" "$f")
    [ "$n" = 1 ] || echo "b: $r listed $n times, want 1"
  done
  n=$(grep -c '^\[\[watch\]\]' "$f")
  [ "$n" = 2 ] || echo "b: $n watch entries, want 2"
  return 0
}

# ---------------------------------------------------------------- (c) FR-006, FR-007 --locked
ck_c() {
  local r=$1 f l
  for f in rust-ci-runner/Makefile .github/workflows/factory-ci-selftest.yml .github/workflows/rust-ci-runner-release.yml \
    rust-ci-runner/.factory-ci.toml .factory-ci.toml; do
    [ -f "$r/$f" ] || { echo "c: $f missing"; continue; }
    while IFS= read -r l; do
      case $l in *--locked*) ;; *) echo "c: $f: no --locked in [$l]" ;; esac
    done < <(grep -oE 'cargo (build|test|clippy|check|fetch)[^"|&;]*' "$r/$f")
  done
  grep -q '^rust-version = "1.90"$' "$r/rust-ci-runner/Cargo.toml" 2> /dev/null || echo 'c: Cargo.toml lacks rust-version = "1.90"'
  return 0
}

# ---------------------------------------------------------------- (d) FR-022, FR-023 names both ways
# readme_rows FILE: S|NAME|has_scope|scope_columns for secret and variable tables, E|NAME for Environments.
readme_rows() {
  awk -F'|' '
    function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
    /^[ \t]*\|/ {
      if (!inT) {
        inT = 1; kind = ""
        for (i = 2; i < NF; i++) { h[i] = trim($i); sc[i] = (i > 2 && h[i] ~ /[Ee]nvironment|[Ss]cope|factory-ci/) }
        if (h[2] ~ /^(Name|Secret|Variable)/) kind = "S"; else if (h[2] ~ /^Environment/) kind = "E"
        next
      }
      if ($0 !~ /[A-Za-z0-9]/) next
      if (kind == "") next
      name = trim($2); gsub(/`/, "", name)
      if (kind == "E") { print "E|" name; next }
      ok = 0; cols = 0
      for (i = 3; i < NF; i++) if (sc[i]) { cols++; v = trim($i); gsub(/`/, "", v); if (v != "" && tolower(v) !~ /^(no|none|-|n\/a)$/) ok = 1 }
      print "S|" name "|" ok "|" cols
      next
    }
    { inT = 0 }' "$1"
}
ck_d() {
  local r=$1 w=$1/.github/workflows rd=$1/README.md wf_names wf_envs rd_names rd_envs x
  [ -f "$rd" ] || { echo "d: README.md missing"; return 0; }
  wf_names=$(cat "$w"/*.yml 2> /dev/null | grep -oE '(secrets|vars)\.[A-Z][A-Z0-9_]*' | sed -E 's/^(secrets|vars)\.//' | grep -v '^GITHUB_TOKEN$' | LC_ALL=C sort -u)
  wf_envs=$(cat "$w"/*.yml 2> /dev/null | grep -E '^ *environment: *[A-Za-z0-9_-]+' | sed -E 's/^ *environment: *([A-Za-z0-9_-]+).*/\1/' | LC_ALL=C sort -u)
  rows=$(readme_rows "$rd")
  rd_names=$(printf '%s\n' "$rows" | awk -F'|' '$1 == "S" && $2 ~ /^[A-Z][A-Z0-9_]*$/ { print $2 }' | LC_ALL=C sort -u)
  rd_envs=$(printf '%s\n' "$rows" | awk -F'|' '$1 == "E" && $2 ~ /^factory-ci[a-z-]*$/ { print $2 }' | LC_ALL=C sort -u)
  for x in $(comm -23 <(printf '%s\n' "$wf_names") <(printf '%s\n' "$rd_names")); do echo "d: workflow-only secret or variable: $x"; done
  for x in $(comm -13 <(printf '%s\n' "$wf_names") <(printf '%s\n' "$rd_names")); do echo "d: readme-only secret or variable: $x"; done
  for x in $(comm -23 <(printf '%s\n' "$wf_envs") <(printf '%s\n' "$rd_envs")); do echo "d: workflow-only environment: $x"; done
  for x in $(comm -13 <(printf '%s\n' "$wf_envs") <(printf '%s\n' "$rd_envs")); do echo "d: readme-only environment: $x"; done
  for x in $(printf '%s\n' "$rows" | awk -F'|' '$1 == "S" && $2 ~ /^[A-Z][A-Z0-9_]*$/ && $3 == 0 { print $2 }'); do
    echo "d: $x has no Environment or scope"
  done
  return 0
}

# ---------------------------------------------------------------- (e) FR-016, FR-022 README keyword rows
# line_with FILE STRING...: some line holds every STRING (fixed, any case).
line_with() {
  local f=$1 s out; shift
  [ -f "$f" ] || return 1
  out=$(cat "$f")
  for s in "$@"; do out=$(printf '%s\n' "$out" | grep -iF -e "$s" || true); done
  [ -n "$out" ]
}
first_line() { grep -inF -m1 -e "$2" "$1" 2> /dev/null | cut -d: -f1; }
# need ROOT LABEL STRING...: one violation when no README line holds all the strings.
need() {
  local r=$1 label=$2; shift 2
  line_with "$r/README.md" "$@" || echo "e: README.md: no line for $label ($*)"
}
ck_e() {
  local r=$1 a b
  need "$r" "bootstrap exception (FR-020)" cargo selftest release
  need "$r" "GHCR package is public (FR-004)" GHCR public
  need "$r" "visibility command (FR-004)" "gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility"
  need "$r" "both repos next to the token (FR-016)" FACTORY_CI_TOKEN factory-monorepo infrastructure
  need "$r" "token permissions (FR-016)" Contents "Commit statuses" Metadata
  need "$r" "release Environment (FR-033)" factory-ci-release "rust-ci-runner-v*"
  need "$r" "main ruleset (FR-022)" ruleset main
  need "$r" "CODEOWNERS (FR-022)" CODEOWNERS .github/
  need "$r" "tag ruleset (FR-022)" ruleset "rust-ci-runner-v*"
  need "$r" "blob keys (FR-022)" "openssl rand -hex 32"
  need "$r" "scheduler (FR-022)" cron-job.org POST
  need "$r" "scheduler header Accept (FR-022)" "Accept: application/vnd.github+json"
  need "$r" "scheduler header Authorization (FR-022)" "Authorization: Bearer"
  need "$r" "scheduler header version (FR-022)" "X-GitHub-Api-Version: 2022-11-28"
  need "$r" "scheduler body (FR-022)" '{"ref":"main"}'
  need "$r" "heartbeat grace (FR-022)" healthchecks.io grace 60
  need "$r" "no .factory-ci.toml sentence (FR-017)" ".factory-ci.toml" nothing
  need "$r" "gh secret list check (FR-022)" "gh secret list"
  need "$r" "rollback: Environment secrets (FR-022)" ollback "Environment secrets"
  need "$r" "rollback: public package (FR-022)" ollback package
  need "$r" "rollback: Bitwarden contents (FR-022)" ollback "Bitwarden contents"
  need "$r" "rollback: scheduler token (FR-022)" ollback "scheduler token"
  need "$r" "rollback: Grafana policy (FR-022)" ollback "Grafana policy"
  # the order that works: rules, then the package check, then the scheduler, then the heartbeat
  a=$(first_line "$r/README.md" "ruleset"); b=$(first_line "$r/README.md" "cron-job.org")
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] || echo "e: README.md: rulesets must come before the scheduler"
  a=$(first_line "$r/README.md" "gh api /users"); b=$(first_line "$r/README.md" "cron-job.org")
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] || echo "e: README.md: the package visibility check must come before the scheduler"
  a=$(first_line "$r/README.md" "cron-job.org"); b=$(first_line "$r/README.md" "healthchecks.io")
  [ -n "$a" ] && [ -n "$b" ] && [ "$a" -lt "$b" ] || echo "e: README.md: the scheduler must come before the heartbeat"
  # the old sentence must be gone
  ! grep -qiF "are not read from GitHub" "$r/README.md" 2> /dev/null || echo "e: README.md: still says the log credentials are not read from GitHub"
  return 0
}

# ---------------------------------------------------------------- (f) FR-024 runner README
ck_f() {
  local f=$1/rust-ci-runner/README.md
  [ -f "$f" ] || { echo "f: rust-ci-runner/README.md missing"; return 0; }
  ! grep -qF "$setup" "$f" || echo "f: rust-ci-runner/README.md still says [$setup]"
  line_with "$f" README Setup || echo "f: rust-ci-runner/README.md does not point to the root README Setup"
  return 0
}

# ---------------------------------------------------------------- (g) FR-032 private identifiers
ck_g() {
  local r=$1 f p
  local pats=("github.com/sudtanj/factory-$mono/actions" "grafana.net" "ghp_" "github_pat_" "glc_")
  for f in README.md rust-ci-runner/README.md specs/003-infrastructure-home/research.md specs/003-infrastructure-home/quickstart.md; do
    [ -f "$r/$f" ] || { echo "g: $f missing"; continue; }
    for p in "${pats[@]}"; do grep -qF -e "$p" "$r/$f" && echo "g: $f: private identifier [$p]"; done
    grep -qE "BEGIN .*PRIVATE KEY" "$r/$f" && echo "g: $f: private key header"
  done
  return 0
}

# ---------------------------------------------------------------- the real tree
run() { eq "$1" "" "$("$2" "$ROOT")"; }
run "(a) no bare $mono, no [$copy], no standalone $bee (FR-001)" ck_a
run "(b) the watch list names both repos (FR-002)" ck_b
run "(c) every cargo call is --locked; rust-version 1.90 (FR-006, FR-007)" ck_c
run "(d) workflow names and README Setup tables agree both ways (FR-022, FR-023)" ck_d
run "(e) README keyword rows (FR-016, FR-020, FR-022)" ck_e
run "(f) runner README (FR-024)" ck_f
run "(g) no private identifier in the docs (FR-032)" ck_g

# ---------------------------------------------------------------- mutation self-check (P6)
# A clean fixture passes every check; one broken rule gives a named failure. Proves each check can fail.
mk_fixture() {
  F=$(mktemp -d)
  mkdir -p "$F/.github/workflows" "$F/.github/scripts" "$F/rust-ci-runner/src" "$F/specs/003-infrastructure-home" "$F/specs/002-ci-runner"
  cat > "$F/README.md" <<'EOF'
# fixture

Docs for factory-monorepo and this repo.

| Name | Environment |
| --- | --- |
| `FACTORY_CI_TOKEN` | `factory-ci` |

| Name | factory-ci-plan | factory-ci |
| --- | --- | --- |
| `BWS_ACCESS_TOKEN` | yes | no |

| Name | Scope |
| --- | --- |
| `RUNNER_IMAGE_DIGEST` | repository |

| Environment | Branches |
| --- | --- |
| `factory-ci` | main |
| `factory-ci-plan` | main |
EOF
  printf '# runner\n\nSetup lives in the root README.md Setup section.\n' > "$F/rust-ci-runner/README.md"
  printf 'name: x\njobs:\n  a:\n    environment: factory-ci\n    steps:\n      - run: echo ${{ secrets.FACTORY_CI_TOKEN }} ${{ vars.RUNNER_IMAGE_DIGEST }} ${{ secrets.GITHUB_TOKEN }}\n  b:\n    environment: factory-ci-plan\n    steps:\n      - run: echo ${{ secrets.BWS_ACCESS_TOKEN }}\n' > "$F/.github/workflows/a.yml"
  printf '#!/bin/sh\necho ok\n' > "$F/.github/scripts/s.sh"
  printf 'Plain notes about the runner.\n' > "$F/specs/003-infrastructure-home/research.md"
  printf 'Plain steps.\n' > "$F/specs/003-infrastructure-home/quickstart.md"
  # outside the scan: specs may name the old words
  printf 'the %s, %s, %s ghp_x\n' "$mono" "$bee" "$copy" > "$F/specs/002-ci-runner/old.md"
}
# mutate NAME CHECK WANT-SUBSTRING FILE SED-SCRIPT: apply on a fresh fixture, expect the failure.
mutate() {
  local name=$1 check=$2 want=$3 file=$4 script=$5 out
  mk_fixture
  eq "fixture is clean for $check before [$name]" "" "$("$check" "$F")"
  sed -i "$script" "$F/$file"
  out=$("$check" "$F")
  has "mutation [$name] fails $check" "$out" "$want"
  rm -rf "$F"
}
append() {  # append NAME CHECK WANT FILE LINE
  mk_fixture; eq "fixture is clean for $2 before [$1]" "" "$("$2" "$F")"
  printf '%s\n' "$5" >> "$F/$4"; has "mutation [$1] fails $2" "$("$2" "$F")" "$3"; rm -rf "$F"
}
# (a)
append "bare word in a script" ck_a "a: .github/scripts/s.sh: bare $mono" .github/scripts/s.sh "# the $mono"
append "bare word in the README" ck_a "a: README.md: bare $mono" README.md "see the $mono"
append "old advice" ck_a "a: rust-ci-runner/README.md: $copy" rust-ci-runner/README.md "1. Make a $copy of this repo."
append "central repo named $bee in a workflow" ck_a "a: .github/workflows/a.yml: standalone $bee" .github/workflows/a.yml "# $bee is public"
append "central repo named $bee in the runner README" ck_a "a: rust-ci-runner/README.md: standalone $bee" rust-ci-runner/README.md "$setup"
mk_fixture; printf 'let b = "%s";\n' "$bee" > "$F/rust-ci-runner/src/data.rs"; eq "(a) $bee in Rust test data is allowed" "" "$(ck_a "$F")"; rm -rf "$F"
mk_fixture; printf 'B_PRIVATE and factory-%s\n' "$mono" >> "$F/.github/scripts/s.sh"; eq "(a) B_PRIVATE and the full repo name are allowed" "" "$(ck_a "$F")"; rm -rf "$F"
# (b)
mk_fixture; mkdir -p "$F/.github"; printf '[[watch]]\nrepo = "sudtanj/factory-%s"\n\n[[watch]]\nrepo = "sudtanj/infrastructure"\n' "$mono" > "$F/.github/factory-ci-repos.toml"
eq "(b) both entries pass" "" "$(ck_b "$F")"
sed -i '/sudtanj\/infrastructure/d' "$F/.github/factory-ci-repos.toml"; has "(b) a missing entry fails" "$(ck_b "$F")" "sudtanj/infrastructure listed 0 times"; rm -rf "$F"
# (c)
mk_fixture; mkdir -p "$F/rust-ci-runner"; printf 'test:\n\tcargo test --locked\n' > "$F/rust-ci-runner/Makefile"
printf 'x: cargo build --locked && cargo fmt --check\n' > "$F/.github/workflows/factory-ci-selftest.yml"; : > "$F/.github/workflows/rust-ci-runner-release.yml"
printf 'commands = ["cargo clippy --locked -- -D warnings", "cargo test --locked"]\n' > "$F/rust-ci-runner/.factory-ci.toml"; printf 'x\n' > "$F/.factory-ci.toml"
printf '[package]\nrust-version = "1.90"\n' > "$F/rust-ci-runner/Cargo.toml"
eq "(c) a clean tree passes" "" "$(ck_c "$F")"
sed -i 's/cargo test --locked/cargo test/' "$F/rust-ci-runner/Makefile"; has "(c) a Makefile call without --locked fails" "$(ck_c "$F")" "c: rust-ci-runner/Makefile: no --locked"
sed -i 's/cargo test --locked/cargo test/' "$F/rust-ci-runner/.factory-ci.toml"; has "(c) a pipeline call without --locked fails" "$(ck_c "$F")" "c: rust-ci-runner/.factory-ci.toml: no --locked"
sed -i 's/1.90/1.89/' "$F/rust-ci-runner/Cargo.toml"; has "(c) another rust-version fails" "$(ck_c "$F")" "rust-version"; rm -rf "$F"
# (d)
mutate "workflow reads a name the README lacks" ck_d "d: workflow-only secret or variable: EXTRA_Z" .github/workflows/a.yml 's/secrets.BWS_ACCESS_TOKEN/secrets.EXTRA_Z/'
mutate "workflow reads a variable the README lacks" ck_d "d: workflow-only secret or variable: EXTRA_V" .github/workflows/a.yml 's/vars.RUNNER_IMAGE_DIGEST/vars.EXTRA_V/'
mutate "README row nobody reads" ck_d "d: readme-only secret or variable: GHOST_NAME" README.md 's/^| `RUNNER_IMAGE_DIGEST` | repository |$/&\n| `GHOST_NAME` | repository |/'
mutate "README row without a scope" ck_d "d: FACTORY_CI_TOKEN has no Environment or scope" README.md 's/^| `FACTORY_CI_TOKEN` | `factory-ci` |$/| `FACTORY_CI_TOKEN` |  |/'
mutate "matrix row with no yes" ck_d "d: BWS_ACCESS_TOKEN has no Environment or scope" README.md 's/^| `BWS_ACCESS_TOKEN` | yes | no |$/| `BWS_ACCESS_TOKEN` | no | no |/'
mutate "workflow Environment the README lacks" ck_d "d: workflow-only environment: factory-ci-release" .github/workflows/a.yml 's/environment: factory-ci$/environment: factory-ci-release/'
mutate "README Environment no workflow uses" ck_d "d: readme-only environment: factory-ci-docker" README.md 's/^| `factory-ci-plan` | main |$/&\n| `factory-ci-docker` | main |/'
mk_fixture; eq "(d) GITHUB_TOKEN needs no README row" "" "$(ck_d "$F")"; rm -rf "$F"
# (e) a README with every row passes, and dropping one fails with its name
mk_readme_e() {
  cat > "$F/README.md" <<'EOF'
## Setup
Rulesets: a `main` ruleset, CODEOWNERS on `.github/`; a tag ruleset for `rust-ci-runner-v*`.
Only the selftest and release workflows run cargo (bootstrap exception).
`FACTORY_CI_TOKEN` covers factory-monorepo and infrastructure: Contents read, Commit statuses read and write, Metadata read.
Blob keys: `openssl rand -hex 32`. Environment `factory-ci-release` takes `rust-ci-runner-v*`.
The GHCR package is public: `gh api /users/sudtanj/packages/container/rust-ci-runner --jq .visibility`.
Scheduler at cron-job.org: POST with Accept: application/vnd.github+json, Authorization: Bearer TOKEN, X-GitHub-Api-Version: 2022-11-28, body {"ref":"main"}.
Heartbeat at healthchecks.io, grace 60 minutes.
A repo without .factory-ci.toml gets nothing.
Check with gh secret list.
Rollback: a revert does not restore Environment secrets, the public package, Bitwarden contents, the scheduler token or the Grafana policy.
EOF
}
mk_fixture; mk_readme_e; eq "(e) a complete README passes" "" "$(ck_e "$F")"
sed -i '/healthchecks.io/d' "$F/README.md"; has "(e) a missing heartbeat row fails" "$(ck_e "$F")" "heartbeat grace"
mk_readme_e; sed -i 's/, X-GitHub-Api-Version: 2022-11-28//' "$F/README.md"; has "(e) a missing scheduler header fails" "$(ck_e "$F")" "scheduler header version"
mk_readme_e; echo "The log credentials are not read from GitHub." >> "$F/README.md"; has "(e) the old sentence fails" "$(ck_e "$F")" "not read from GitHub"
rm -rf "$F"
# (f)
mk_fixture; eq "(f) a clean runner README passes" "" "$(ck_f "$F")"; printf '## %s\n' "$setup" >> "$F/rust-ci-runner/README.md"; has "(f) the old heading fails" "$(ck_f "$F")" "still says"; rm -rf "$F"
# (g)
append "run URL of the private repo" ck_g "g: README.md: private identifier [github.com/sudtanj/factory-$mono/actions]" README.md "see github.com/sudtanj/factory-$mono/actions/runs/1"
append "Grafana host" ck_g "g: rust-ci-runner/README.md: private identifier [grafana.net]" rust-ci-runner/README.md "https://logs-prod-1.grafana.net/loki"
append "GitHub token" ck_g "private identifier [ghp_]" specs/003-infrastructure-home/research.md "token ghp_abc123"
append "fine-grained token" ck_g "private identifier [github_pat_]" specs/003-infrastructure-home/quickstart.md "github_pat_abc"
append "Grafana token" ck_g "private identifier [glc_]" README.md "glc_abc"
append "private key" ck_g "private key header" README.md "-----BEGIN RSA PRIVATE KEY-----"

printf '%s checks, %s failed\n' "$total" "$failed"
[ "$failed" -eq 0 ]
