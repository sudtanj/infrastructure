#!/usr/bin/env bash
# Shell tests for factory-ci-job.sh and factory-ci-plan.sh (T047; FR-039 to FR-046, FR-060;
# AS 12.1 to 12.11). Stub `bws`, stub `rust-ci-runner`, real `jq` and `openssl`, FCI_KEY_SEP
# set explicitly, FACTORY_CI_FETCH_WINDOW=2.
#
# Interfaces assumed (contracts/tick-workflow.md, research.md D12; ambiguities in the QA report):
#   factory-ci-blob.sh      sourced; fci_blob_encrypt (row JSON on stdin, ciphertext on stdout)
#                           and fci_blob_decrypt (ciphertext on stdin, JSON on stdout)
#   factory-ci-job.sh       env ITEM_BLOB FACTORY_CI_BLOB_KEY FACTORY_CI_TOKEN BWS_ACCESS_TOKEN
#                           FACTORY_CI_BWS_PROJECT B_PRIVATE FCI_KEY_SEP RUNNER_TEMP; runs
#                           `rust-ci-runner job ... --names-file F --reason-file R` under `env -i`,
#                           then `exec rust-ci-runner job ...`; failure reasons go to
#                           $RUNNER_TEMP/fci-reason and the exit status is non-zero
#   factory-ci-plan.sh      cwd = checkout of B; env as the plan step; writes $GITHUB_OUTPUT and
#                           $GITHUB_STEP_SUMMARY
#   `bws secret list <project> -o json` prints [{"key","value",...}]; the 401/403 and 429 cases
#   are recognised by the words "401", "403" and "429" or "timed out" on stderr.
set -u
here=$(cd "$(dirname "$0")" && pwd)
total=0
failed=0

fail() { total=$((total + 1)); failed=$((failed + 1)); printf 'FAIL: %s\n' "$1"; }
pass() { total=$((total + 1)); }
eq() { if [ "$2" = "$3" ]; then pass; else fail "$1: want [$2] got [$3]"; fi; }
has() { case $2 in *"$3"*) pass ;; *) fail "$1: [$3] not in [$2]" ;; esac; }
lacks() { case $2 in *"$3"*) fail "$1: [$3] found in [$2]" ;; *) pass ;; esac; }

KEY=test-key-0123456789-abcdefghijklmnop
export FACTORY_CI_BLOB_KEY=$KEY
# shellcheck source=/dev/null
. "$here/factory-ci-blob.sh"

# new_world: a fresh directory with stub bws, stub runner, stub sudo and iptables on PATH.
new_world() {
  W=$(mktemp -d)
  mkdir -p "$W/bin" "$W/state" "$W/rt" "$W/repo/.github"
  : > "$W/state/prepare.names"
  echo '[]' > "$W/state/bws.json"
  cat > "$W/bin/bws" <<EOF
#!/bin/sh
d=$W/state
n=\$(( \$(cat \$d/bws.count 2>/dev/null || echo 0) + 1 )); echo \$n > \$d/bws.count
printf '%s\n' "\$@" > \$d/bws.argv.\$n
echo "\${BWS_ACCESS_TOKEN-UNSET}" > \$d/bws.token.\$n
case "\$(cat \$d/bws.mode 2>/dev/null)" in
  rate) echo "Error: 429 Too Many Requests" >&2; exit 1;;
  down) echo "Error: connection timed out" >&2; exit 1;;
  r401) echo "Error: 401 Unauthorized" >&2; exit 1;;
  r403) echo "Error: 403 Forbidden" >&2; exit 1;;
  junk) echo "not json"; exit 0;;
esac
cat \$d/bws.json
EOF
  cat > "$W/bin/rust-ci-runner" <<EOF
#!/bin/sh
d=$W/state
n=\$(( \$(cat \$d/runner.count 2>/dev/null || echo 0) + 1 )); echo \$n > \$d/runner.count
printf '%s\n' "\$@" > \$d/runner.argv.\$n
/usr/bin/env > \$d/runner.env.\$n
ls -A "\$RUNNER_TEMP" > \$d/runner.rt.\$n 2>/dev/null
for v in \$(cat \$d/dumpnames 2>/dev/null); do eval "printf '%s' \"\\\${\$v-UNSET}\"" > \$d/val.\$v.\$n; done
case "\$1" in
  plan)
    case " \$* " in
      *" --rows-in "*)
        echo C >> \$d/phases
        while [ \$# -gt 0 ]; do case \$1 in --rows-out) ro=\$2; shift;; --summary) sm=\$2; shift;; esac; shift; done
        cp \$d/rows.json "\$ro"; echo "item 0: o/r" > "\$sm"; echo "fail_tick=\$(cat \$d/fail_tick 2>/dev/null || echo 0)"; exit 0;;
      *)
        echo A >> \$d/phases
        [ -e \$d/plan.a.fail ] && { echo "bad list" >&2; exit 1; }
        while [ \$# -gt 0 ]; do case \$1 in --rows-out) ro=\$2; shift;; esac; shift; done
        cp \$d/cand.json "\$ro"; exit 0;;
    esac;;
  job)
    case " \$* " in
      *" --names-file "*)
        echo prepare > \$d/runner.kind.\$n
        while [ \$# -gt 0 ]; do case \$1 in --names-file) nf=\$2; shift;; --reason-file) rf=\$2; shift;; esac; shift; done
        if [ -s \$d/prepare.reason ]; then cp \$d/prepare.reason "\$rf"; exit 1; fi
        cp \$d/prepare.names "\$nf"; exit 0;;
    esac
    echo real > \$d/runner.kind.\$n
    exit \$(cat \$d/real.exit 2>/dev/null || echo 0);;
esac
EOF
  printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> %s/state/sudo.log\nexit 0\n' "$W" > "$W/bin/sudo"
  printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> %s/state/iptables.log\nexit 0\n' "$W" > "$W/bin/iptables"
  chmod +x "$W"/bin/*
}

store_add() { jq --arg k "$1" --arg v "$2" '. + [{id:"i", key:$k, value:$v, projectId:"p"}]' "$W/state/bws.json" > "$W/state/t" && mv "$W/state/t" "$W/state/bws.json"; }
names() { printf '%s\n' "$@" > "$W/state/prepare.names"; }
count() { cat "$W/state/$1.count" 2>/dev/null || echo 0; }
reason() { cat "$W/rt/fci-reason" 2>/dev/null || true; }

# mkrow key=value ...: a row with defaults, encrypted into $BLOB.
mkrow() {
  local row
  row=$(jq -n '{repo:"o/r", repo_id:"123", sha:"0123456789abcdef0123456789abcdef01234567", branch:"main", project:".",
    context:"factory-ci", prev_sha:"", allow_docker:false, fail_closed:false, retry:false, secrets_ok:true,
    secrets:"", run_id:"0123456789abcdef", n:3}')
  local kv
  for kv in "$@"; do
    row=$(jq --arg k "${kv%%=*}" --arg v "${kv#*=}" '.[$k] = (if ($v=="true" or $v=="false") then ($v=="true") else $v end)' <<<"$row")
  done
  BLOB=$(printf '%s' "$row" | fci_blob_encrypt)
}

# run_job [VAR=value | -VAR]...: runs the job wrapper; $OUT, $ERR and $RC hold the result.
run_job() {
  local -a env=(PATH="$W/bin:/usr/bin:/bin" HOME="$W" RUNNER_TEMP="$W/rt" ITEM_BLOB="$BLOB"
    FACTORY_CI_BLOB_KEY="$KEY" FACTORY_CI_TOKEN=gh-token-ZZ9 BWS_ACCESS_TOKEN=bws-token-YY8 HAVE_BWS=true
    B_PRIVATE=false FCI_KEY_SEP="${SEP:-/}" FACTORY_CI_FETCH_WINDOW=2 FACTORY_CI_BWS_PROJECT=proj-default
    FACTORY_CI_CLONE_IMAGE=r/i@sha256:0000000000000000000000000000000000000000000000000000000000000000
    FACTORY_CI_DIND_IMAGE=r/d@sha256:0000000000000000000000000000000000000000000000000000000000000000)
  local a
  for a in "$@"; do
    case $a in
      -*) local n=${a#-}; local -a keep=(); local e
          for e in "${env[@]}"; do [ "${e%%=*}" = "$n" ] || keep+=("$e"); done; env=("${keep[@]}") ;;
      *) env+=("$a") ;;
    esac
  done
  OUT=$(env -i "${env[@]}" bash ${BASHFLAGS:-} "$here/factory-ci-job.sh" 2> "$W/err"); RC=$?
  ERR=$(cat "$W/err")
}

runner_arg() { tr '\n' ' ' < "$W/state/runner.argv.$1"; }
real_n() { local i; for i in 1 2 3 4 5 6; do [ "$(cat "$W/state/runner.kind.$i" 2>/dev/null)" = real ] && { echo "$i"; return; }; done; echo 0; }
bwscalls() { count bws; }

# ---------------------------------------------------------------- job wrapper
# AS 12.1: the row comes only from the blob, never from a plaintext ITEM_* variable.
new_world; mkrow repo=o/r project=.
run_job ITEM_REPO=evil/x ITEM_SHA=ffffffffffffffffffffffffffffffffffffffff ITEM_BRANCH=evil
eq "plaintext ITEM vars ignored: exit" 0 "$RC"
a=$(runner_arg "$(real_n)")
has "real run uses the blob repo" "$a" "--repo o/r "
lacks "real run ignores ITEM_REPO" "$a" "evil"
has "real run gets run id" "$a" "--run-id 0123456789abcdef"
rm -rf "$W"

# AS 12.1: only names from the names file are fetched; nothing else is exported.
new_world; mkrow secrets="NPM_TOKEN OTHER_UNUSED"; names NPM_TOKEN
store_add NPM_TOKEN npm-value-1111; store_add OTHER_UNUSED other-value-2222
echo "NPM_TOKEN OTHER_UNUSED BWS_ACCESS_TOKEN" > "$W/state/dumpnames"
run_job
eq "fetch: exit" 0 "$RC"; n=$(real_n)
eq "fetch: listed name exported" npm-value-1111 "$(cat "$W/state/val.NPM_TOKEN.$n")"
eq "fetch: unlisted name not exported" UNSET "$(cat "$W/state/val.OTHER_UNUSED.$n")"
eq "fetch: store token unset before the real run (I18)" UNSET "$(cat "$W/state/val.BWS_ACCESS_TOKEN.$n")"
eq "fetch: exactly one list call" 1 "$(bwscalls)"
has "fetch: list call names the project" "$(cat "$W/state/bws.argv.1")" "proj-default"
rm -rf "$W"

# FR-040 / AS 12.2, 12.6: key order inside one project.
for SEP in / :; do
  export SEP
  pk="o/r${SEP}NPM_TOKEN"
  # prefixed wins when both exist (repo_id set, name listed)
  new_world; mkrow secrets=NPM_TOKEN repo_id=123; names NPM_TOKEN; store_add NPM_TOKEN shared-val-1; store_add "$pk" prefixed-val-1
  echo NPM_TOKEN > "$W/state/dumpnames"; run_job; eq "sep $SEP: prefixed wins" prefixed-val-1 "$(cat "$W/state/val.NPM_TOKEN.$(real_n)")"; rm -rf "$W"
  # prefixed key needs a recorded repo id: without it the shared key (when listed) is used
  new_world; mkrow secrets=NPM_TOKEN repo_id=; names NPM_TOKEN; store_add NPM_TOKEN shared-val-2; store_add "$pk" prefixed-val-2
  echo NPM_TOKEN > "$W/state/dumpnames"; run_job; eq "sep $SEP: no repo_id uses shared" shared-val-2 "$(cat "$W/state/val.NPM_TOKEN.$(real_n)")"; rm -rf "$W"
  # no repo_id and the shared name not listed: not found
  new_world; mkrow secrets= repo_id=; names NPM_TOKEN; store_add "$pk" prefixed-val-3; store_add NPM_TOKEN shared-val-3
  run_job; eq "sep $SEP: no id, unlisted: exit" 1 "$RC"; has "sep $SEP: reason" "$(reason)" "secret NPM_TOKEN not found"; eq "sep $SEP: real run not started" 0 "$(real_n)"; rm -rf "$W"
  # an unlisted shared key is never used even with a repo id
  new_world; mkrow secrets= repo_id=123; names NPM_TOKEN; store_add NPM_TOKEN shared-val-4
  run_job; has "sep $SEP: unlisted shared key unused" "$(reason)" "secret NPM_TOKEN not found"; rm -rf "$W"
  # a key under another repo's prefix is never used (AS 12.9)
  new_world; mkrow secrets=NPM_TOKEN repo_id=123; names NPM_TOKEN; store_add "x/y${SEP}NPM_TOKEN" foreign-value-5
  echo NPM_TOKEN > "$W/state/dumpnames"; run_job
  eq "sep $SEP: foreign prefix: exit" 1 "$RC"; has "sep $SEP: foreign prefix reason" "$(reason)" "secret NPM_TOKEN not found"
  eq "sep $SEP: foreign prefix: no real run" 0 "$(real_n)"; lacks "sep $SEP: foreign value not printed" "$OUT$ERR" foreign-value-5; rm -rf "$W"
done
unset SEP

# `__` is not a separator (A24).
new_world; mkrow secrets=NPM_TOKEN repo_id=123; names NPM_TOKEN; store_add "o/r__NPM_TOKEN" dunder-value-6
run_job FCI_KEY_SEP=__; eq "double underscore separator refused" 1 "$RC"; eq "no real run" 0 "$(real_n)"; rm -rf "$W"

# AS 12.5: no store token and a pipeline that lists a secret.
new_world; mkrow; names NPM_TOKEN; run_job -BWS_ACCESS_TOKEN HAVE_BWS=false
eq "no store: exit" 1 "$RC"; has "no store: reason" "$(reason)" "no secret store configured"; eq "no store: no bws call" 0 "$(bwscalls)"; eq "no store: no real run" 0 "$(real_n)"; rm -rf "$W"
new_world; mkrow; names; run_job -BWS_ACCESS_TOKEN HAVE_BWS=false
eq "no store, no secrets: runs" 0 "$RC"; eq "no store, no secrets: real run" 1 "$([ "$(real_n)" != 0 ] && echo 1 || echo 0)"; rm -rf "$W"

# FR-060 / AS 12.11: secrets_ok false.
new_world; mkrow secrets_ok=false; names NPM_TOKEN; store_add NPM_TOKEN v-7777
run_job; eq "secrets not ok: exit" 1 "$RC"; has "secrets not ok: reason" "$(reason)" "secrets not allowed on this branch"
eq "secrets not ok: no store call" 0 "$(bwscalls)"; rm -rf "$W"
new_world; mkrow secrets_ok=false; names; run_job; eq "secrets not ok but none listed: runs" 0 "$RC"; rm -rf "$W"

# An empty names file makes no fetch: with no store token there is no store call at all, and
# with one there is at most the single list call that resolves the log credentials.
new_world; mkrow; names; run_job -BWS_ACCESS_TOKEN; eq "empty names, no token: no store call" 0 "$(bwscalls)"; rm -rf "$W"
new_world; mkrow; names; run_job; n=$(bwscalls); eq "empty names: at most one store call" 1 "$([ "$n" -le 1 ] && echo 1 || echo 0)"; eq "empty names: runs" 0 "$RC"; rm -rf "$W"

# AS 12.3: log credentials come from the three project keys.
logrow() { store_add FACTORY_CI_LOG_URL https://loki.example.com; store_add FACTORY_CI_LOG_USER loki-user-1; store_add FACTORY_CI_LOG_TOKEN loki-token-2222; }
new_world; mkrow; names; logrow; echo "FACTORY_CI_LOG_URL FACTORY_CI_LOG_USER FACTORY_CI_LOG_TOKEN" > "$W/state/dumpnames"; run_job
n=$(real_n); eq "log creds: exit" 0 "$RC"; eq "log creds: url" https://loki.example.com "$(cat "$W/state/val.FACTORY_CI_LOG_URL.$n")"
eq "log creds: token" loki-token-2222 "$(cat "$W/state/val.FACTORY_CI_LOG_TOKEN.$n")"; has "log creds on: --quiet-logs" "$(runner_arg "$n")" "--quiet-logs"; rm -rf "$W"
new_world; mkrow; names; run_job; lacks "no log creds: no --quiet-logs" "$(runner_arg "$(real_n)")" "--quiet-logs"; eq "no log creds: exit" 0 "$RC"; rm -rf "$W"
new_world; mkrow; names; store_add FACTORY_CI_LOG_URL https://loki.example.com; run_job
eq "one log key: exit" 1 "$RC"; has "one log key: reason" "$(reason)" "log credentials incomplete"; eq "one log key: no real run" 0 "$(real_n)"; rm -rf "$W"
new_world; mkrow; names; store_add FACTORY_CI_LOG_URL https://loki.example.com; store_add FACTORY_CI_LOG_USER loki-user-1; run_job
eq "two log keys: exit" 1 "$RC"; has "two log keys: reason" "$(reason)" "log credentials incomplete"; rm -rf "$W"

# AS 12.6 variants: duplicate key is ambiguous; an empty value counts as no match.
new_world; mkrow secrets=NPM_TOKEN; names NPM_TOKEN; store_add NPM_TOKEN first-val-8888; store_add NPM_TOKEN second-val-9999
run_job; eq "duplicate: exit" 1 "$RC"; has "duplicate: reason" "$(reason)" "ambiguous"; has "duplicate: names it" "$(reason)" NPM_TOKEN
lacks "duplicate: values not printed" "$OUT$ERR$(reason)" first-val-8888; rm -rf "$W"
new_world; mkrow secrets=NPM_TOKEN; names NPM_TOKEN; store_add NPM_TOKEN ""
run_job; eq "empty value: exit" 1 "$RC"; has "empty value: reason" "$(reason)" "secret NPM_TOKEN not found"; rm -rf "$W"

# AS 12.7: retry on rate limits and outages, no retry on 401 or 403.
for mode in rate down; do
  new_world; mkrow secrets=NPM_TOKEN; names NPM_TOKEN; echo $mode > "$W/state/bws.mode"
  run_job; eq "$mode: exit" 1 "$RC"; has "$mode: reason" "$(reason)" "secret store unreachable"
  n=$(bwscalls); eq "$mode: retried" 1 "$([ "$n" -ge 2 ] && echo 1 || echo 0)"; eq "$mode: no real run" 0 "$(real_n)"; rm -rf "$W"
done
for mode in r401 r403; do
  new_world; mkrow secrets=NPM_TOKEN; names NPM_TOKEN; echo $mode > "$W/state/bws.mode"
  run_job; eq "$mode: exit" 1 "$RC"; has "$mode: reason" "$(reason)" "secret store rejected the token"; eq "$mode: not retried" 1 "$(bwscalls)"; rm -rf "$W"
done

# AS 12.4: multi-line value intact; masks first; no value outside mask lines, stderr or bws argv.
new_world; mkrow secrets=PEM_KEY; names PEM_KEY
store_add PEM_KEY "$(printf 'line-one-AAAA\nline-two-BBBB')"; echo PEM_KEY > "$W/state/dumpnames"; run_job
n=$(real_n); eq "multi-line: exit" 0 "$RC"; eq "multi-line: intact" "$(printf 'line-one-AAAA\nline-two-BBBB')" "$(cat "$W/state/val.PEM_KEY.$n")"
first=$(printf '%s\n' "$OUT" | head -n 2)
eq "masks come first" "$(printf '::add-mask::line-one-AAAA\n::add-mask::line-two-BBBB')" "$first"
rest=$(printf '%s\n' "$OUT" | grep -v '^::add-mask::' || true)
lacks "no value outside mask lines (stdout)" "$rest" line-one-AAAA; lacks "no value (stderr)" "$ERR" line-two-BBBB
lacks "no value in bws argv" "$(cat "$W"/state/bws.argv.*)" line-one-AAAA
lacks "no value in runner argv" "$(cat "$W"/state/runner.argv.*)" line-two-BBBB; rm -rf "$W"

# The prepare run starts under env -i; the real run never holds the store token or an unlisted name (I18).
new_world; mkrow secrets=NPM_TOKEN; names NPM_TOKEN; store_add NPM_TOKEN v-12345; run_job
p=$(cat "$W/state/runner.env.1"); eq "first runner call is prepare" prepare "$(cat "$W/state/runner.kind.1")"
has "prepare keeps the clone token" "$p" "FACTORY_CI_TOKEN=gh-token-ZZ9"
for v in BWS_ACCESS_TOKEN FACTORY_CI_BLOB_KEY ITEM_BLOB HOME=; do lacks "prepare env lacks $v" "$p" "$v"; done
r=$(cat "$W/state/runner.env.$(real_n)"); lacks "real env lacks store token" "$r" "BWS_ACCESS_TOKEN"; lacks "real env lacks blob key" "$r" "FACTORY_CI_BLOB_KEY"
lacks "real env lacks ITEM_BLOB" "$r" "ITEM_BLOB"; rm -rf "$W"

# An invalid pipeline copies the reason and makes no bws call.
new_world; mkrow; echo "invalid .factory-ci.toml: step a depends on unknown step b" > "$W/state/prepare.reason"
run_job; eq "invalid pipeline: exit" 1 "$RC"; has "invalid pipeline: reason copied" "$(reason)" "depends on unknown step"; eq "invalid pipeline: no bws" 0 "$(bwscalls)"
eq "invalid pipeline: no real run" 0 "$(real_n)"; rm -rf "$W"

# The row's flags and the docker variant (reads only the project it was given).
new_world; mkrow allow_docker=true fail_closed=true retry=true prev_sha=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa; names
store_add FACTORY_CI_LOG_URL https://loki.example.com; store_add FACTORY_CI_LOG_USER u-1234; store_add FACTORY_CI_LOG_TOKEN t-12345
run_job FACTORY_CI_BWS_PROJECT=proj-docker; a=$(runner_arg "$(real_n)")
for f in --allow-docker --private --retry --quiet-logs "--prev aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; do has "flag $f" "$a" "$f"; done
has "docker variant reads its project" "$(cat "$W/state/bws.argv.1")" "proj-docker"; lacks "docker variant never reads the default project" "$(cat "$W"/state/bws.argv.*)" "proj-default"; rm -rf "$W"
new_world; mkrow; names; run_job; a=$(runner_arg "$(real_n)")
for f in --allow-docker --private --retry; do lacks "no $f by default" "$a" "$f"; done; rm -rf "$W"

# ---------------------------------------------------------------- plan wrapper (AS 12.10, FR-045)
plan_world() {
  new_world
  mkdir -p "$W/repo/.github"; echo '# list' > "$W/repo/.github/factory-ci-repos.toml"
  jq -n '{rows:[], unchanged:[], invalid:[], problems:[], unchecked:[], checked:1}' > "$W/state/cand.json"
  echo '[]' > "$W/state/rows.json"; : > "$W/gh_out"; : > "$W/gh_summary"
}
cand_row() {
  jq -n '{repo:"o/r", repo_id:"123", sha:"0123456789abcdef0123456789abcdef01234567", branch:"main", project:".", context:"factory-ci",
    prev_sha:"", allow_docker:false, fail_closed:false, retry:false, secrets_ok:true, secrets:"", run_id:"0123456789abcdef", n:0}' \
    | jq '{rows:[.], unchanged:[], invalid:[], problems:[], unchecked:[], checked:1}' > "$W/state/cand.json"
}
run_plan() {
  local -a env=(PATH="$W/bin:/usr/bin:/bin" HOME="$W" RUNNER_TEMP="$W/rt" GITHUB_OUTPUT="$W/gh_out" GITHUB_STEP_SUMMARY="$W/gh_summary"
    FACTORY_CI_BLOB_KEY="$KEY" FACTORY_CI_TOKEN=gh-token-ZZ9 FACTORY_CI_TOKEN_DOCKER=gh-docker-ZZ8 BWS_ACCESS_TOKEN=bws-token-YY8 HAVE_BWS=true
    B_PRIVATE=false FCI_KEY_SEP=/ FACTORY_CI_FETCH_WINDOW=2 FACTORY_CI_BWS_PROJECT=proj-default)
  env+=("$@")
  OUT=$(cd "$W/repo" && env -i "${env[@]}" bash "$here/factory-ci-plan.sh" 2> "$W/err"); RC=$?
  ERR=$(cat "$W/err")
}

# No rows: no store call, phase C still runs.
plan_world; run_plan
eq "idle plan: exit" 0 "$RC"; eq "idle plan: no bws call (I28)" 0 "$(bwscalls)"; eq "idle plan: both phases ran" "A C" "$(tr '\n' ' ' < "$W/state/phases" | sed 's/ $//')"; rm -rf "$W"

# Rows and a healthy store: the log credentials reach phase C.
plan_world; cand_row; store_add FACTORY_CI_LOG_URL https://loki.example.com; store_add FACTORY_CI_LOG_USER u-1234; store_add FACTORY_CI_LOG_TOKEN t-12345
run_plan; eq "healthy plan: exit" 0 "$RC"; eq "healthy plan: phases" "A C" "$(tr '\n' ' ' < "$W/state/phases" | sed 's/ $//')"
cenv=$(cat "$W/state/runner.env.2"); has "phase C gets the log url" "$cenv" "FACTORY_CI_LOG_URL=https://loki.example.com"
has "phase C gets the log token" "$cenv" "FACTORY_CI_LOG_TOKEN=t-12345"; eq "one store call" 1 "$(bwscalls)"
rm -rf "$W"

# Rows and a rejected token or an unreachable store: non-zero, reason in the summary, phase C never starts.
for mode in r401 down; do
  plan_world; cand_row; echo $mode > "$W/state/bws.mode"; run_plan
  eq "$mode plan: non-zero" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"
  eq "$mode plan: phase C never started" A "$(tr '\n' ' ' < "$W/state/phases" | sed 's/ $//')"
  case $mode in
    r401) has "$mode plan: summary reason" "$(cat "$W/gh_summary")" "secret store rejected the token" ;;
    down) has "$mode plan: summary reason" "$(cat "$W/gh_summary")" "secret store unreachable" ;;
  esac
  eq "$mode plan: nothing posted by the wrapper" 0 "$(wc -c < "$W/gh_out" | tr -d ' ')"
  rm -rf "$W"
done

# A malformed list fails before any store call.
plan_world; cand_row; : > "$W/state/plan.a.fail"; run_plan
eq "malformed list: non-zero" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; eq "malformed list: no store call" 0 "$(bwscalls)"; eq "malformed list: phase C not started" A "$(tr '\n' ' ' < "$W/state/phases" | sed 's/ $//')"; rm -rf "$W"

printf '%s checks, %s failed\n' "$total" "$failed"
[ "$failed" -eq 0 ]
