#!/usr/bin/env bash
# Hardening tests for the wrappers and the blob (T058; FR-054, FR-056, FR-059; M3, M5, M6, M7,
# H1, H3, L2; I26, I27). Stub bws, sudo, iptables and rust-ci-runner; real openssl and jq.
# Same interface assumptions as test-factory-ci-secrets.sh, plus:
#   factory-ci-plan.sh      $GITHUB_OUTPUT gets matrix= and matrix_docker= (JSON {"include":[{"n","blob"}]}),
#                           count=, count_docker=, fail_tick=; ROWS written by phase C is a JSON array of rows
#   factory-ci-job.sh       also appends its sanitized reason to $GITHUB_STEP_SUMMARY, never to stdout
#   factory-ci-status.sh    env ITEM_BLOB FACTORY_CI_BLOB_KEY FACTORY_CI_TOKEN RUNNER_TEMP; calls
#                           `rust-ci-runner status --repo --sha --project --run-id --state error
#                           --description TEXT [--retry]`
#   factory-ci-lint-workflow.sh WORKFLOW reads BWS_SHA256 from factory-ci-secrets.sh beside it
set -u
here=$(cd "$(dirname "$0")" && pwd)
total=0
failed=0

fail() { total=$((total + 1)); failed=$((failed + 1)); printf 'FAIL: %s\n' "$1"; }
pass() { total=$((total + 1)); }
eq() { if [ "$2" = "$3" ]; then pass; else fail "$1: want [$2] got [$3]"; fi; }
has() { case $2 in *"$3"*) pass ;; *) fail "$1: [$3] not in [$2]" ;; esac; }
lacks() { case $2 in *"$3"*) fail "$1: [$3] found in [$2]" ;; *) pass ;; esac; }

KEY=7f3a9c1e5b8d20467f3a9c1e5b8d20467f3a9c1e5b8d20467f3a9c1e5b8d2046
KEYD=4e9d0b62a7c15f384e9d0b62a7c15f384e9d0b62a7c15f384e9d0b62a7c15f38
KEYW=91c07e5a3d2b8f6491c07e5a3d2b8f6491c07e5a3d2b8f6491c07e5a3d2b8f64
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


# The block of the metadata addresses records how many runner calls had happened before it.
new_world_hard() {
  new_world
  printf '#!/bin/sh\nprintf "count=%%s %%s\\n" "$(cat %s/state/runner.count 2>/dev/null || echo 0)" "$*" >> %s/state/sudo.log\nexit 0\n' "$W" "$W" > "$W/bin/sudo"
  chmod +x "$W/bin/sudo"
}
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
    FACTORY_CI_BLOB_KEY="$KEY" FACTORY_CI_BLOB_KEY_DOCKER="$KEYD" FACTORY_CI_TOKEN=gh-token-ZZ9 FACTORY_CI_TOKEN_DOCKER=gh-docker-ZZ8 BWS_ACCESS_TOKEN=bws-token-YY8 HAVE_BWS=true
    B_PRIVATE=false FCI_KEY_SEP=/ FACTORY_CI_FETCH_WINDOW=2 FACTORY_CI_BWS_PROJECT=proj-default)
  env+=("$@")
  OUT=$(cd "$W/repo" && env -i "${env[@]}" bash "$here/factory-ci-plan.sh" 2> "$W/err"); RC=$?
  ERR=$(cat "$W/err")
}


b64() { printf '%s' "$1" | base64 | tr -d '\n'; }
phases() { tr '\n' ' ' < "$W/state/phases" | sed 's/ $//'; }

# ---------------------------------------------------------------- blob (FR-059, I27)
new_world_hard; mkrow secrets=NPM_TOKEN branch=feature-xyz project=svc-secret
row=$(printf '%s' "$BLOB" | fci_blob_decrypt)
eq "blob round trip: repo" o/r "$(jq -r .repo <<<"$row")"; eq "blob round trip: branch" feature-xyz "$(jq -r .branch <<<"$row")"
for needle in "$KEY" feature-xyz svc-secret NPM_TOKEN o/r 0123456789abcdef; do lacks "ciphertext hides $needle" "$BLOB" "$needle"; done
eq "ciphertext is a single line" 1 "$(printf '%s\n' "$BLOB" | wc -l | tr -d ' ')"
bad=$(printf '%s' "$BLOB" | FACTORY_CI_BLOB_KEY=$KEYW fci_blob_decrypt 2>/dev/null); rc=$?
eq "wrong key: no plaintext" "" "$bad"
# the job wrapper with the wrong key: reason "job did not finish", nothing runs
run_job FACTORY_CI_BLOB_KEY=$KEYW
eq "wrong key: job exit" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; has "wrong key: reason" "$(reason)" "job did not finish"
eq "wrong key: no runner call" 0 "$(count runner)"; eq "wrong key: no store call" 0 "$(bwscalls)"; lacks "wrong key: nothing printed" "$OUT$ERR" "o/r"
rm -rf "$W"

# ---------------------------------------------------------------- plan wrapper outputs (FR-058, FR-059, I26)
plan_world
jq -n '[{repo:"o/r",repo_id:"1",sha:"0123456789abcdef0123456789abcdef01234567",branch:"feature-xyz",project:"svc-secret",context:"factory-ci/svc-secret",
  prev_sha:"",allow_docker:false,fail_closed:false,retry:false,secrets_ok:true,secrets:"NPM_TOKEN",run_id:"0123456789abcdef",n:0},
 {repo:"o/d",repo_id:"2",sha:"fedcba9876543210fedcba9876543210fedcba98",branch:"dockerbranch",project:".",context:"factory-ci",
  prev_sha:"",allow_docker:true,fail_closed:false,retry:false,secrets_ok:true,secrets:"",run_id:"fedcba9876543210",n:1}]' > "$W/state/rows.json"
cand_row; echo 1 > "$W/state/fail_tick"; run_plan
eq "plan: exit" 0 "$RC"; go=$(cat "$W/gh_out")
keys=$(sed -n 's/^\([a-z_]*\)=.*/\1/p' <<<"$go" | sort | tr '\n' ' ')
eq "output keys" "count count_docker fail_tick matrix matrix_docker " "$keys"
m=$(sed -n 's/^matrix=//p' <<<"$go"); md=$(sed -n 's/^matrix_docker=//p' <<<"$go")
eq "matrix item keys" "blob n" "$(jq -r '.include[0] | keys | join(" ")' <<<"$m")"; eq "docker matrix item keys" "blob n" "$(jq -r '.include[0] | keys | join(" ")' <<<"$md")"
eq "counts" "1 1" "$(sed -n 's/^count=//p;s/^count_docker=//p' <<<"$go" | tr '\n' ' ' | sed 's/ $//')"
eq "fail_tick" 1 "$(sed -n 's/^fail_tick=//p' <<<"$go")"
eq "n unique" "0 1" "$(jq -r '[.include[0].n] | .[]' <<<"$m") $(jq -r '[.include[0].n] | .[]' <<<"$md")"
for needle in o/r o/d feature-xyz dockerbranch svc-secret NPM_TOKEN 0123456789abcdef fedcba98 "$KEY"; do lacks "GITHUB_OUTPUT hides $needle" "$go" "$needle"; done
blob0=$(jq -r '.include[0].blob' <<<"$m"); eq "blob decrypts to the row" feature-xyz "$(printf '%s' "$blob0" | fci_blob_decrypt | jq -r .branch)"
# R5: a docker row is sealed with the docker key and a plain row with the plain key, never the other
eq "plain blob opens with the plain key" feature-xyz "$(printf '%s' "$blob0" | fci_blob_decrypt | jq -r .branch)"
eq "plain blob stays shut with the docker key" "" "$(printf '%s' "$blob0" | FACTORY_CI_BLOB_KEY=$KEYD fci_blob_decrypt)"
blobd=$(jq -r '.include[0].blob' <<<"$md")
eq "docker blob opens with the docker key" dockerbranch "$(printf '%s' "$blobd" | FACTORY_CI_BLOB_KEY=$KEYD fci_blob_decrypt | jq -r .branch)"
eq "docker blob stays shut with the plain key" "" "$(printf '%s' "$blobd" | fci_blob_decrypt)"
for needle in "$KEYD"; do lacks "GITHUB_OUTPUT hides the docker key" "$go" "$needle"; done
eq "plain rows are gone from the runner temp" "" "$(grep -rl 'feature-xyz' "$W/rt" 2>/dev/null || true)"
lacks "summary has no branch or project" "$(cat "$W/gh_summary")" feature-xyz; lacks "stdout has no row field" "$OUT" feature-xyz
rm -rf "$W"

# ---------------------------------------------------------------- blob key length and cost (R6)
eq "the blob uses 600000 iterations to encrypt and decrypt" 2 "$(grep -c -e '-iter 600000' "$here/factory-ci-blob.sh")"
# FR-036: a blob key is 64 lower-case hex characters, for every caller; each bad shape is refused by name, never by value
h16=0123456789abcdef; h64=$h16$h16$h16$h16
BKL=(short nonhex upper 63 65 empty)
BKV=(abc "$(printf 'g%.0s' $(seq 64))" "$(printf 'A%.0s' $(seq 64))" "${h64%?}" "${h64}0" "")
for i in "${!BKL[@]}"; do
  l=${BKL[$i]}; v=${BKV[$i]}
  for var in FACTORY_CI_BLOB_KEY FACTORY_CI_BLOB_KEY_DOCKER; do
    plan_world; cand_row; [ "$var" = FACTORY_CI_BLOB_KEY ] || { jq '.rows[0].allow_docker = true' "$W/state/cand.json" > "$W/t" && mv "$W/t" "$W/state/cand.json"; }
    run_plan "$var=$v"
    eq "plan $var $l: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; has "plan $var $l: names the variable" "$(cat "$W/gh_summary")" "$var is not 64 lower-case hex"
    [ -z "$v" ] || lacks "plan $var $l: value hidden" "$OUT$ERR$(cat "$W/gh_summary")" "$v"
    rm -rf "$W"
  done
  e=$(printf '{}' | FACTORY_CI_BLOB_KEY=$v fci_blob_encrypt 2>&1); rc=$?
  eq "blob encrypt $l: refused" 1 "$([ "$rc" -ne 0 ] && echo 1 || echo 0)"; has "blob encrypt $l: names the variable" "$e" "FACTORY_CI_BLOB_KEY is not 64 lower-case hex"
  [ -z "$v" ] || lacks "blob encrypt $l: value hidden" "$e" "$v"
  e=$(printf '%s' "$BLOB" | FACTORY_CI_BLOB_KEY=$v fci_blob_decrypt 2>&1); rc=$?
  eq "blob decrypt $l: refused" 1 "$([ "$rc" -ne 0 ] && echo 1 || echo 0)"; has "blob decrypt $l: names the variable" "$e" "FACTORY_CI_BLOB_KEY is not 64 lower-case hex"
done
plan_world; cand_row; echo '[{"repo":"o/r","allow_docker":false,"n":0}]' > "$W/state/rows.json"; run_plan FACTORY_CI_BLOB_KEY=$h64
eq "64-hex blob key: accepted" 0 "$RC"
b=$(sed -n 's/^matrix=//p' "$W/gh_out" | jq -r '.include[0].blob')
eq "64-hex blob key: round trip" o/r "$(printf '%s' "$b" | FACTORY_CI_BLOB_KEY=$h64 fci_blob_decrypt | jq -r .repo)"
rm -rf "$W"
# the tick guard steps: plan (both keys), run and run-docker each test the 64-hex shape, none the old length
tick="$here/../workflows/factory-ci-tick.yml"
eq "tick: four guards test 64 lower-case hex" 4 "$(grep -c '=~ \^\[0-9a-f\]{64}\$' "$tick")"
eq "tick: no 32-character guard left" 0 "$(grep -c -e '-ge 32' "$tick")"

# ---------------------------------------------------------------- plan phase A without the log credentials (FR-037; D23)
# Phase A reads GitHub only, so it never holds the three FACTORY_CI_LOG_* names; phase C, which ships the log line, keeps them.
LOGS=(FACTORY_CI_LOG_URL=https://loki.example.com FACTORY_CI_LOG_USER=u-1234 FACTORY_CI_LOG_TOKEN=t-12345)
plan_world; cand_row; run_plan "${LOGS[@]}"
eq "phase A: plan passes" 0 "$RC"; eq "phase A: both phases ran" "A C" "$(phases)"
eq "phase A: sees none of the three log names" 0 "$(grep -c '^FACTORY_CI_LOG_' "$W/state/runner.env.1")"
eq "phase C: sees all three log names" 3 "$(grep -c '^FACTORY_CI_LOG_' "$W/state/runner.env.2")"
rm -rf "$W"
plan_world; cand_row; run_plan
eq "no log credentials: plan passes" 0 "$RC"; eq "no log credentials: both phases ran" "A C" "$(phases)"; eq "no log credentials: phase A sees none" 0 "$(grep -c '^FACTORY_CI_LOG_' "$W/state/runner.env.1")"
rm -rf "$W"

# ---------------------------------------------------------------- the store check only when a row can use a secret (FR-038; D19)
# A job reaches the store only with shared names (`secrets`) or an owner-scoped key (`repo_id`), and only when secrets_ok.
# cand_with FILTER: cand_row, then a jq filter on its one row.
cand_with() { cand_row; jq "$1" "$W/state/cand.json" > "$W/t" && mv "$W/t" "$W/state/cand.json"; }
store_case() {  # store_case LABEL WANT_RC WANT_CALLS JQ_FILTER [VAR=value]...
  local l=$1 rc=$2 calls=$3 filter=$4; shift 4
  plan_world; cand_with "$filter"; echo down > "$W/state/bws.mode"; run_plan "$@"
  eq "$l: exit" "$rc" "$RC"; eq "$l: store calls" "$calls" "$(bwscalls)"; rm -rf "$W"
}
store_ok() {  # same with a working store
  local l=$1 calls=$2 filter=$3; shift 3
  plan_world; cand_with "$filter"; run_plan "$@"
  eq "$l: exit" 0 "$RC"; eq "$l: store calls" "$calls" "$(bwscalls)"; rm -rf "$W"
}
store_case "no names, no id, store down" 0 0 '.rows[0].repo_id = "" | .rows[0].secrets = ""'
store_ok "shared names, store up" 1 '.rows[0].repo_id = "" | .rows[0].secrets = "NPM_TOKEN"'
store_case "repo id, store down (fci_list retries)" 1 3 '.rows[0].repo_id = "123" | .rows[0].secrets = ""'
store_case "secrets_ok false, names and id, store down" 0 0 '.rows[0].secrets_ok = false | .rows[0].secrets = "NPM_TOKEN"'
plan_world; cand_with '.rows[0].repo_id = "123"'; echo down > "$W/state/bws.mode"; run_plan
has "repo id, store down: the store message" "$(cat "$W/gh_summary")" "secret store unreachable"; rm -rf "$W"
D='.rows[0].allow_docker = true'; DT=BWS_ACCESS_TOKEN_DOCKER=bws-docker-XX7; DP=FACTORY_CI_BWS_PROJECT_DOCKER=proj-docker
store_ok "docker row with an id, docker token" 1 "$D | .rows[0].repo_id = \"123\"" "$DT" "$DP"
plan_world; cand_with "$D | .rows[0].repo_id = \"123\""; run_plan "$DT" "$DP"
eq "docker row: the docker token is the one used" bws-docker-XX7 "$(cat "$W/state/bws.token.1")"; rm -rf "$W"
store_case "docker row without names or id, docker token, store down" 0 0 "$D | .rows[0].repo_id = \"\" | .rows[0].secrets = \"\"" "$DT" "$DP"
store_ok "docker row with an id, no docker token" 0 "$D | .rows[0].repo_id = \"123\""
store_ok "docker row only: the plain store is not asked" 0 "$D | .rows[0].repo_id = \"\" | .rows[0].secrets = \"NPM_TOKEN\""
plan_world; cand_row; run_plan FACTORY_CI_BLOB_KEY_DOCKER=
eq "no docker row needs no docker key" 0 "$RC"; rm -rf "$W"

# ---------------------------------------------------------------- README (R13)
eq "README states RUNNER_IMAGE_DIGEST without an @" 0 "$(grep 'RUNNER_IMAGE_DIGEST' "$here/../../rust-ci-runner/README.md" | grep -c '@')"
eq "README names the digest as sha256:<64 hex>" 1 "$([ "$(grep -c 'sha256:<64 hex>' "$here/../../rust-ci-runner/README.md")" -ge 1 ] && echo 1 || echo 0)"
eq "README marks FACTORY_CI_BLOB_KEY_DOCKER optional" 1 "$([ "$(grep -m1 'FACTORY_CI_BLOB_KEY_DOCKER' "$here/../../rust-ci-runner/README.md" | grep -c optional)" -ge 1 ] && echo 1 || echo 0)"

# ---------------------------------------------------------------- names (M3)
for hostile in '$(touch PWNED)' 'a;b' 'A Z' '1BAD' '`touch PWNED`' 'A-Q' '../x'; do
  new_world_hard; mkrow secrets=NPM_TOKEN; printf '%s\n' "$hostile" | sed "s|PWNED|$W/pwned|" > "$W/state/prepare.names"; store_add NPM_TOKEN v-12345
  run_job
  eq "hostile name [$hostile]: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"
  eq "hostile name [$hostile]: not evaluated" no "$([ -e "$W/pwned" ] && echo yes || echo no)"
  eq "hostile name [$hostile]: no real run" 0 "$(real_n)"
  rm -rf "$W"
done

# ---------------------------------------------------------------- fetch hygiene (M5, M6)
VAL=sekret-value-4321
new_world_hard; mkrow secrets=NPM_TOKEN; names NPM_TOKEN; store_add NPM_TOKEN "$VAL"; echo NPM_TOKEN > "$W/state/dumpnames"
BASHFLAGS=-x run_job; unset BASHFLAGS
eq "xtrace run: exit" 0 "$RC"; lacks "bash -x prints no value (stderr)" "$ERR" "$VAL"
lacks "bash -x prints no value outside masks (stdout)" "$(printf '%s\n' "$OUT" | grep -v '^::add-mask::' || true)" "$VAL"
lacks "bws json never in argv" "$(cat "$W"/state/bws.argv.*)" "$VAL"; lacks "bws json never in runner argv" "$(cat "$W"/state/runner.argv.*)" "$VAL"
eq "no file under the runner temp holds a value before exec" "" "$(grep -rl -e "$VAL" "$W/rt" 2>/dev/null || true)"
has "mask line for the value" "$OUT" "::add-mask::$VAL"; has "mask line for the base64 of the value" "$OUT" "::add-mask::$(b64 "$VAL")"
rm -rf "$W"

new_world_hard; mkrow secrets=SHORTY; names SHORTY; store_add SHORTY abc; run_job
eq "short line: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; has "short line: names the secret" "$(reason)" SHORTY
lacks "short line: value absent" "$OUT$ERR$(reason)" "abc "; eq "short line: no real run" 0 "$(real_n)"
eq "short line: nothing exported or masked" "" "$OUT"; rm -rf "$W"
new_world_hard; mkrow secrets=SHORTY; names SHORTY; store_add SHORTY "$(printf 'longenough1\nab')"; run_job
eq "short second line: refused" 0 "$([ "$RC" -eq 0 ] && echo 1 || echo 0)"; lacks "short second line: first line not printed" "$OUT$ERR$(reason)" longenough1; rm -rf "$W"

# ---------------------------------------------------------------- metadata block and flags (M7, FR-054)
new_world_hard; mkrow fail_closed=true retry=true; names
run_job FACTORY_CI_LOG_URL=https://loki.example.com FACTORY_CI_LOG_USER=u-1234 FACTORY_CI_LOG_TOKEN=t-12345; sl=$(cat "$W/state/sudo.log" 2>/dev/null || true)
has "block 169.254.169.254" "$sl" "iptables -I DOCKER-USER -d 169.254.169.254 -j DROP"; has "block 168.63.129.16" "$sl" "iptables -I DOCKER-USER -d 168.63.129.16 -j DROP"
lacks "both blocks come before any runner call" "$sl" "count=1"; lacks "not after the prepare run either" "$sl" "count=2"
a=$(runner_arg "$(real_n)")
has "--quiet-logs when creds resolved" "$a" "--quiet-logs"; has "--private for fail_closed" "$a" "--private"; has "--retry for retry" "$a" "--retry"
rm -rf "$W"
new_world_hard; mkrow; names; run_job; a=$(runner_arg "$(real_n)")
lacks "no --quiet-logs without creds" "$a" "--quiet-logs"; lacks "no --private" "$a" "--private"; lacks "no --retry" "$a" "--retry"; rm -rf "$W"

# The docker variant is handed only its own token and project: the job wrapper knows no other name.
eq "job wrapper never names the docker token" 0 "$(grep -c 'BWS_ACCESS_TOKEN_DOCKER' "$here/factory-ci-job.sh")"
new_world_hard; mkrow allow_docker=true secrets=NPM_TOKEN; names NPM_TOKEN; store_add NPM_TOKEN v-12345
run_job BWS_ACCESS_TOKEN=docker-store-token FACTORY_CI_BWS_PROJECT=proj-docker
eq "docker item uses the token it was given" docker-store-token "$(cat "$W/state/bws.token.1")"; has "docker item reads the docker project" "$(cat "$W/state/bws.argv.1")" proj-docker; rm -rf "$W"

# ---------------------------------------------------------------- reasons (M2)
new_world_hard; mkrow; printf '%s\n' '::error::boom <script>alert(1)</script> $(id) `x`' > "$W/state/prepare.reason"
run_job GITHUB_STEP_SUMMARY="$W/summary"
r=$(reason); eq "reason is one line" 1 "$(printf '%s\n' "$r" | wc -l | tr -d ' ')"
eq "reason uses only the safe alphabet" "" "$(printf '%s' "$r" | tr -d 'A-Za-z0-9 _.:/-')"
case $r in ::*) fail "reason starts with ::" ;; *) pass ;; esac
lacks "reason never reaches stdout" "$OUT" boom; lacks "reason never reaches stderr as a command" "$ERR" "::error::"
lacks "summary has no markup" "$(cat "$W/summary" 2>/dev/null)" "<script>"; lacks "summary has no backtick" "$(cat "$W/summary" 2>/dev/null)" '`'
rm -rf "$W"

# ---------------------------------------------------------------- status script
run_status() {
  OUT=$(env -i PATH="$W/bin:/usr/bin:/bin" HOME="$W" RUNNER_TEMP="$W/rt" ITEM_BLOB="$BLOB" FACTORY_CI_BLOB_KEY="$KEY" FACTORY_CI_TOKEN=gh-token-ZZ9 "$@" \
    bash "$here/factory-ci-status.sh" 2> "$W/err"); RC=$?
  ERR=$(cat "$W/err")
}
new_world_hard; mkrow retry=true project=svc; run_status
eq "status: exit" 0 "$RC"; sa=$(tr '\n' ' ' < "$W/state/runner.argv.1")
has "status: subcommand" "$sa" "status "; has "status: repo" "$sa" "--repo o/r"; has "status: sha" "$sa" "--sha 0123456789abcdef0123456789abcdef01234567"
has "status: project" "$sa" "--project svc"; has "status: run id" "$sa" "--run-id 0123456789abcdef"; has "status: state" "$sa" "--state error"
has "status: default reason" "$sa" "--description job did not finish"; has "status: retry" "$sa" "--retry"; eq "status: silent" "" "$OUT"
echo "clone failed" > "$W/rt/fci-reason"; rm -f "$W/state/runner.count"; run_status
has "status: reason from the file" "$(tr '\n' ' ' < "$W/state/runner.argv.1")" "--description clone failed"; rm -rf "$W"
new_world_hard; mkrow; run_status; lacks "status: no --retry by default" "$(tr '\n' ' ' < "$W/state/runner.argv.1")" "--retry"; rm -rf "$W"
# FR-036: the status and job wrappers refuse a bad blob key through the blob functions; nothing runs, the value stays hidden
for i in "${!BKL[@]}"; do
  l=${BKL[$i]}; v=${BKV[$i]}
  new_world_hard; mkrow; run_status FACTORY_CI_BLOB_KEY="$v"
  eq "status key $l: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; eq "status key $l: no runner call" 0 "$(count runner)"
  [ -z "$v" ] || lacks "status key $l: value hidden" "$OUT$ERR" "$v"
  run_job FACTORY_CI_BLOB_KEY="$v"
  eq "job key $l: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; eq "job key $l: no runner call" 0 "$(count runner)"
  [ -z "$v" ] || lacks "job key $l: value hidden" "$OUT$ERR$(reason)" "$v"
  rm -rf "$W"
done

# ---------------------------------------------------------------- placeholder checksum (lint)
tmp=$(mktemp -d); cp "$here"/factory-ci-*.sh "$tmp"/; sed -i 's/^BWS_SHA256=.*/BWS_SHA256=REPLACE_ME/' "$tmp/factory-ci-secrets.sh"
wf="$here/../workflows/factory-ci-tick.yml"
bash "$tmp/factory-ci-lint-workflow.sh" "$wf" > /dev/null 2>&1; eq "placeholder tolerated by the lint" 0 "$?"
FACTORY_CI_LINT_STRICT=1 bash "$tmp/factory-ci-lint-workflow.sh" "$wf" > /dev/null 2>&1; eq "placeholder refused under FACTORY_CI_LINT_STRICT=1" 1 "$([ "$?" -ne 0 ] && echo 1 || echo 0)"
rm -rf "$tmp"


# ================================================================ 003 bootstrap secrets from Actions (T008)
# Log credentials come from FACTORY_CI_LOG_URL/_USER/_TOKEN in the environment (FR-010 to FR-012),
# never from Bitwarden (FR-013, I4). Each block names its FR and invariant.
LOGV=(FACTORY_CI_LOG_URL FACTORY_CI_LOG_USER FACTORY_CI_LOG_TOKEN)
# Sentinels carry `%`, a newline and nothing shorter than 4 characters per line.
SENT=("https://loki.sentinel-U1.example/p%41th" "user-sentinel-U2%7" "$(printf 'token-sentinel-L3a%%25\ntoken-sentinel-L3b')")
mkrow secrets=NPM_TOKEN; BLOB003=$BLOB   # one encryption, reused by every world below
tick="$here/../workflows/factory-ci-tick.yml"

# logargs MASK MODE: sets LA to VAR=value words; a clear bit gives VAR= (empty) or nothing (unset).
logargs() {
  local i; LA=()
  for i in 0 1 2; do
    if [ $(($1 >> i & 1)) -eq 1 ]; then LA+=("${LOGV[$i]}=${SENT[$i]}")
    elif [ "$2" = empty ]; then LA+=("${LOGV[$i]}="); fi
  done
}
# empties MASK: the sorted names of the variables whose bit is clear.
empties() { local i o=; for i in 0 1 2; do [ $(($1 >> i & 1)) -eq 1 ] || o="$o ${LOGV[$i]}"; done; printf '%s\n' $o | sort | tr '\n' ' ' | sed 's/ $//'; }
named() { printf '%s' "$1" | grep -o 'FACTORY_CI_LOG_[A-Z]*' | sort -u | tr '\n' ' ' | sed 's/ $//'; }
# alltext: everything a run printed or left behind, `::add-mask::` lines removed.
alltext() {
  printf '%s\n' "$OUT" | grep -v '^::add-mask::' || true
  printf '%s\n' "$ERR"
  cat "$W"/rt/* "$W/summary" "$W/gh_out" "$W/gh_summary" 2> /dev/null || true
}
# leaks LABEL VALUE...: no line of any VALUE outside a mask line (P2, I2, FR-015).
leaks() {
  local label=$1 t v l; shift; t=$(alltext)
  for v in "$@"; do
    while IFS= read -r l; do [ "${#l}" -lt 4 ] || lacks "$label: $l" "$t" "$l"; done <<< "$v"
  done
}
# masked LABEL VALUE: every line of 4 or more characters has a mask line, `%` escaped.
masked() {
  local l
  while IFS= read -r l; do [ "${#l}" -lt 4 ] || has "$1: mask for $l" "$OUT" "::add-mask::${l//%/%25}"; done <<< "$2"
}

# ---- P1 / FR-011 / FR-012 / I1: the plan stops before the first runner call on a partial set.
for mode in empty unset; do
  for m in 1 2 3 4 5 6; do
    t="plan partial mask=$m ($mode)"
    plan_world; cand_row; logargs $m $mode; run_plan "${LA[@]}"
    eq "$t: non-zero" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"
    eq "$t: no runner call" 0 "$(count runner)"; eq "$t: no store call" 0 "$(bwscalls)"
    has "$t: reason" "$(cat "$W/gh_summary")" "log credentials incomplete"
    eq "$t: names exactly the empty variables" "$(empties $m)" "$(named "$(cat "$W/gh_summary")")"
    eq "$t: no output" "" "$(cat "$W/gh_out")"; leaks "$t" "${SENT[@]}"
    rm -rf "$W"
  done
done

# ---- FR-010: all three set and no Bitwarden token: phase C gets them, masked, no store call.
plan_world; cand_row; logargs 7 empty; run_plan BWS_ACCESS_TOKEN= HAVE_BWS=false "${LA[@]}"
eq "plan, logs on, no store token: exit" 0 "$RC"; eq "plan, logs on, no store token: phases" "A C" "$(phases)"
cenv=$(cat "$W/state/runner.env.2")
for i in 0 1; do has "plan phase C gets ${LOGV[$i]}" "$cenv" "${LOGV[$i]}=${SENT[$i]}"; done
has "plan phase C gets the token's first line" "$cenv" "FACTORY_CI_LOG_TOKEN=token-sentinel-L3a%25"
eq "plan, no store token: no bws call" 0 "$(bwscalls)"
for i in 0 1 2; do masked "plan logs on, ${LOGV[$i]}" "${SENT[$i]}"; done
leaks "plan logs on" "${SENT[@]}"; rm -rf "$W"
# no log variable at all: off, phase C runs without them
plan_world; cand_row; run_plan BWS_ACCESS_TOKEN= HAVE_BWS=false
eq "plan, logs off: exit" 0 "$RC"; lacks "plan, logs off: phase C has no log url" "$(cat "$W/state/runner.env.2")" "FACTORY_CI_LOG_URL="; rm -rf "$W"

# ---- FR-011: a 3-character line in any log variable stops the plan; nothing printed.
for i in 0 1 2; do
  plan_world; cand_row; logargs 7 empty; LA+=("${LOGV[$i]}=q7Z"); run_plan "${LA[@]}"
  eq "plan short ${LOGV[$i]}: non-zero" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; has "plan short ${LOGV[$i]}: reason" "$(cat "$W/gh_summary")" "under 4 characters"
  eq "plan short ${LOGV[$i]}: phase C never started" A "$(phases)"; leaks "plan short ${LOGV[$i]}" q7Z "${SENT[@]}"; rm -rf "$W"
done

# ---- FR-031: a server URL that is not https:// is refused before the first bws call, by both scripts.
for u in http://bw.example.com ftp://x.example.com bw.example.com "http://" "HTTPS://bw.example.com"; do
  plan_world; cand_row; run_plan FACTORY_CI_BWS_SERVER_URL="$u"
  eq "plan refuses server url [$u]" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; eq "plan [$u]: no bws call" 0 "$(bwscalls)"
  has "plan [$u]: reason names the rule" "$(cat "$W/gh_summary")" "https://"; rm -rf "$W"
  new_world_hard; BLOB=$BLOB003; names NPM_TOKEN; store_add NPM_TOKEN v-12345; run_job FACTORY_CI_BWS_SERVER_URL="$u"
  eq "job refuses server url [$u]" 1 "$RC"; eq "job [$u]: no bws call" 0 "$(bwscalls)"; has "job [$u]: reason names the rule" "$(reason)" "https://"
  eq "job [$u]: no real run" 0 "$(real_n)"; rm -rf "$W"
done
for u in "" https://vault.bitwarden.eu https://bw.example.com:8443/x; do
  plan_world; cand_row; run_plan FACTORY_CI_BWS_SERVER_URL="$u"; eq "plan accepts server url [$u]" 0 "$RC"; rm -rf "$W"
  new_world_hard; BLOB=$BLOB003; names NPM_TOKEN; store_add NPM_TOKEN v-12345; run_job FACTORY_CI_BWS_SERVER_URL="$u"; eq "job accepts server url [$u]" 0 "$RC"; rm -rf "$W"
done

# ---- FR-011 / FR-012 / P1: the job refuses a partial set first: no runner call, no store call.
for mode in empty unset; do
  for m in 1 2 3 4 5 6; do
    t="job partial mask=$m ($mode)"
    new_world_hard; BLOB=$BLOB003; names NPM_TOKEN; store_add NPM_TOKEN v-12345; logargs $m $mode; run_job GITHUB_STEP_SUMMARY="$W/summary" "${LA[@]}"
    eq "$t: exit" 1 "$RC"; has "$t: reason" "$(reason)" "log credentials incomplete"
    eq "$t: names exactly the empty variables" "$(empties $m)" "$(named "$(reason)")"
    eq "$t: no runner call (refused first)" 0 "$(count runner)"; eq "$t: no store call" 0 "$(bwscalls)"; eq "$t: no real run" 0 "$(real_n)"
    leaks "$t" "${SENT[@]}"; rm -rf "$W"
  done
done
# an empty pipeline refuses the same way: logs are not optional once one variable is set
new_world_hard; BLOB=$BLOB003; names; run_job FACTORY_CI_LOG_URL=${SENT[0]}; eq "job partial, no secrets: refused" 1 "$RC"; has "job partial, no secrets: reason" "$(reason)" "log credentials incomplete"; rm -rf "$W"

# ---- I3 / FR-013: a pipeline with no secret never calls bws, token set or not, logs on or off.
for m in 0 7; do
  new_world_hard; BLOB=$BLOB003; names; logargs $m empty; run_job "${LA[@]}"
  eq "no-secret pipeline, store token set, logs mask=$m: exit" 0 "$RC"; eq "no-secret pipeline, logs mask=$m: zero bws calls (I3)" 0 "$(bwscalls)"
  rm -rf "$W"
done
new_world_hard; BLOB=$BLOB003; names; run_job FACTORY_CI_BWS_PROJECT=; eq "no-secret pipeline, no project: runs" 0 "$RC"; eq "no-secret pipeline, no project: zero bws calls" 0 "$(bwscalls)"; rm -rf "$W"
# logs on: --quiet-logs and the three variables reach the real run, masked.
new_world_hard; BLOB=$BLOB003; names; logargs 7 empty; echo "${LOGV[*]}" > "$W/state/dumpnames"; run_job "${LA[@]}"
n=$(real_n); has "job logs on: --quiet-logs" "$(runner_arg "$n")" "--quiet-logs"
for i in 0 1 2; do eq "job logs on: ${LOGV[$i]} reaches the real run" "${SENT[$i]}" "$(cat "$W/state/val.${LOGV[$i]}.$n")"; masked "job logs on" "${SENT[$i]}"; done; rm -rf "$W"

# ---- FR-013 / I4: no fallback. Store token unset, the pipeline's secret name set as a plain variable (a GitHub secret).
new_world_hard; BLOB=$BLOB003; names NPM_TOKEN; echo NPM_TOKEN > "$W/state/dumpnames"
run_job -BWS_ACCESS_TOKEN HAVE_BWS=false NPM_TOKEN=gh-secret-fallback-99 GITHUB_STEP_SUMMARY="$W/summary"
eq "no fallback: item refused" 1 "$RC"; has "no fallback: reason" "$(reason)" "no secret store configured"
eq "no fallback: no real run" 0 "$(real_n)"; eq "no fallback: no bws call" 0 "$(bwscalls)"; leaks "no fallback" gh-secret-fallback-99
# the status fallback turns the reason into an error status
BLOB=$BLOB003; run_status; sa=$(tr '\n' ' ' < "$W/state/runner.argv.$(count runner)")
has "no fallback: status is error" "$sa" "--state error"; has "no fallback: status carries the reason" "$sa" "--description no secret store configured"
rm -rf "$W"

# ---- P2 / I2 / FR-015: sentinels in every bootstrap variable; masks only, escaped for `%` and newline.
CT="clone-tok-%sentinel-T1" BT="bws-tok-%sentinel-B2" PS="$(printf 'pipe-sentinel-P3%%\npipe-sentinel-P4')" DT="docker-tok-%sentinel-D5"
new_world_hard; BLOB=$BLOB003; names NPM_TOKEN; store_add NPM_TOKEN "$PS"; logargs 7 empty
run_job GITHUB_STEP_SUMMARY="$W/summary" FACTORY_CI_TOKEN="$CT" BWS_ACCESS_TOKEN="$BT" "${LA[@]}"
eq "sentinels (job): exit" 0 "$RC"; leaks "sentinels (job)" "$CT" "$BT" "$PS" "$KEY" "${SENT[@]}"
masked "sentinels (job) pipeline secret" "$PS"; for i in 0 1 2; do masked "sentinels (job) ${LOGV[$i]}" "${SENT[$i]}"; done
rm -rf "$W"
plan_world; cand_row; logargs 7 empty
run_plan FACTORY_CI_TOKEN="$CT" BWS_ACCESS_TOKEN="$BT" FACTORY_CI_TOKEN_DOCKER="$DT" "${LA[@]}"
eq "sentinels (plan): exit" 0 "$RC"; leaks "sentinels (plan)" "$CT" "$BT" "$DT" "$KEY" "$KEYD" "${SENT[@]}"
for i in 0 1 2; do masked "sentinels (plan) ${LOGV[$i]}" "${SENT[$i]}"; done; rm -rf "$W"
# refusals print no value either: a 3-character line in the pipeline secret, the other values still valid
new_world_hard; BLOB=$BLOB003; names NPM_TOKEN; store_add NPM_TOKEN q7Z; logargs 7 empty
run_job GITHUB_STEP_SUMMARY="$W/summary" FACTORY_CI_TOKEN="$CT" BWS_ACCESS_TOKEN="$BT" "${LA[@]}"
eq "short pipeline secret: refused" 1 "$RC"; leaks "short pipeline secret" q7Z "$CT" "$BT" "$KEY" "${SENT[@]}"; rm -rf "$W"
for i in 0 1 2; do
  new_world_hard; BLOB=$BLOB003; names; logargs 7 empty; LA+=("${LOGV[$i]}=q7Z"); run_job GITHUB_STEP_SUMMARY="$W/summary" "${LA[@]}"
  eq "job short ${LOGV[$i]}: refused" 1 "$RC"; has "job short ${LOGV[$i]}: reason" "$(reason)" "under 4 characters"; eq "job short ${LOGV[$i]}: no real run" 0 "$(real_n)"
  leaks "job short ${LOGV[$i]}" q7Z "${SENT[@]}"; rm -rf "$W"
done

# ---- FR-004 / FR-010 / FR-031: the tick workflow. Which step holds each log secret, and the guards.
eq "tick: secrets.FACTORY_CI_LOG_ appears 9 times" 9 "$(grep -c 'secrets.FACTORY_CI_LOG_' "$tick")"
where=$(awk '/^  [a-z][a-z-]*:$/ { job=$1 } /^      - name:/ { sub(/^      - name: */, ""); step=$0 } /secrets\.FACTORY_CI_LOG_/ { print job step }' "$tick" | LC_ALL=C sort | uniq -c | sed "s/^ *//")
eq "tick: three variables in exactly the plan step and both job steps" "3 plan:plan
3 run-docker:job
3 run:job" "$where"
for v in "${LOGV[@]}"; do
  eq "tick: $v maps to its own secret 3 times" 3 "$(grep -c "^          $v: \${{ secrets.$v }}$" "$tick")"
done
msg='the runner image is not pullable; make the GHCR package public'
eq "tick: three fetch runner steps" 3 "$(grep -c 'name: fetch runner' "$tick")"
eq "tick: the fixed GHCR message appears in all three" 3 "$(grep -cF "$msg" "$tick")"
eq "tick: every docker create is guarded and silent" 3 "$(grep -c 'id=$(docker create .* 2> /dev/null) \\$' "$tick")"
eq "tick: no bare docker create left" 0 "$(grep 'docker create' "$tick" | grep -vc '2> /dev/null')"
eq "tick: three install bws steps" 3 "$(grep -c 'name: install bws' "$tick")"
eq "tick: the https guard appears in all three" 3 "$(grep -cF 'FACTORY_CI_BWS_SERVER_URL must start with https://' "$tick")"
early=$(awk '/^      - name:/ { instep = ($0 ~ /install bws/); g = 0 } instep && /must start with https:\/\// { g = 1 } instep && /curl|unzip|bws" config/ && !g { bad++ } END { print bad + 0 }' "$tick")
eq "tick: the guard comes before any download or config in each install bws step" 0 "$early"
# the existing container-environment test is the LOG_VARS coverage (FR-031: step containers never get the log variables)
jr="$here/../../rust-ci-runner/tests/job_rules.rs"
eq "LOG_VARS test exists: the_runner_environment_never_becomes_a_container_environment" 1 "$(grep -c 'fn the_runner_environment_never_becomes_a_container_environment' "$jr")"
eq "LOG_VARS test checks FACTORY_CI_LOG in -e names" 1 "$([ "$(grep -c 'starts_with("FACTORY_CI_LOG")' "$jr")" -ge 1 ] && echo 1 || echo 0)"


# Heartbeat step (verdict job): run its real `run:` block with a fake curl and read the ping URL.
hb=$(mktemp -d)
awk '/^      - name: heartbeat$/ { s = 1 } s && /^        run: \|$/ { r = 1; next } r && /^          / { sub(/^          /, ""); print; next } r { exit }' "$tick" > "$hb/step.sh"
eq "heartbeat: the step body was extracted" 1 "$([ -s "$hb/step.sh" ] && echo 1 || echo 0)"
mkdir -p "$hb/bin"
printf '#!/bin/sh\nprintf "%%s\\n" "$*" >> "%s/curl.log"\nexit "${FAKE_CURL_RC:-0}"\n' "$hb" > "$hb/bin/curl"
chmod +x "$hb/bin/curl"
# ping PLAN FAIL_TICK RUN_RESULTS REF URL [CURL_RC]: "URL curl got (empty when curl never ran)|exit status|output"
ping() {
  rm -f "$hb/curl.log"
  local out rc
  out=$(env -i PATH="$hb/bin:/usr/bin:/bin" PLAN_RESULT="$1" FAIL_TICK="$2" RUN_RESULTS="$3" GITHUB_REF="$4" FACTORY_CI_HEARTBEAT_URL="$5" FAKE_CURL_RC="${6:-0}" bash "$hb/step.sh" 2>&1); rc=$?
  printf '%s|%s|%s' "$(awk '{ print $NF }' "$hb/curl.log" 2> /dev/null || true)" "$rc" "$out"
}
U=https://hc.example.test/ping-uuid-0000
eq "heartbeat: green tick pings the plain URL" "$U|0|" "$(ping success 0 'success success' refs/heads/main $U)"
eq "heartbeat: skipped run jobs (idle tick) still ping ok" "$U|0|" "$(ping success 0 'skipped skipped' refs/heads/main $U)"
eq "heartbeat: failed run job pings /fail" "$U/fail|0|" "$(ping success 0 'failure success' refs/heads/main $U)"
eq "heartbeat: failed docker job pings /fail" "$U/fail|0|" "$(ping success 0 'success failure' refs/heads/main $U)"
eq "heartbeat: cancelled job pings /fail" "$U/fail|0|" "$(ping success 0 'success cancelled' refs/heads/main $U)"
eq "heartbeat: plan failure pings /fail" "$U/fail|0|" "$(ping failure '' 'skipped skipped' refs/heads/main $U)"
eq "heartbeat: plan cancelled pings /fail" "$U/fail|0|" "$(ping cancelled '' 'skipped skipped' refs/heads/main $U)"
eq "heartbeat: fail_tick=1 pings /fail" "$U/fail|0|" "$(ping success 1 'success success' refs/heads/main $U)"
eq "heartbeat: a trailing slash in the secret does not double" "$U/fail|0|" "$(ping success 1 'success success' refs/heads/main $U/)"
eq "heartbeat: no URL set, no curl, step passes" "|0|" "$(ping success 0 'success success' refs/heads/main '')"
eq "heartbeat: not on main, no curl, step passes" "|0|" "$(ping success 0 'success success' refs/heads/feature $U)"
eq "heartbeat: a failing curl does not fail the step and prints nothing" "$U|0|" "$(ping success 0 'success success' refs/heads/main $U 22)"
rm -rf "$hb"
printf '%s checks, %s failed\n' "$total" "$failed"
[ "$failed" -eq 0 ]
