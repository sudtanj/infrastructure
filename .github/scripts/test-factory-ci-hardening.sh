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

KEY=test-key-0123456789-abcdefghijklmnop
KEYD=docker-key-0123456789-abcdefghijklmnop
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
bad=$(printf '%s' "$BLOB" | FACTORY_CI_BLOB_KEY=another-key fci_blob_decrypt 2>/dev/null); rc=$?
eq "wrong key: no plaintext" "" "$bad"
# the job wrapper with the wrong key: reason "job did not finish", nothing runs
run_job FACTORY_CI_BLOB_KEY=another-key
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
K31=kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk; K32=${K31}k
eq "test key lengths" "31 32" "${#K31} ${#K32}"
plan_world; cand_row; run_plan FACTORY_CI_BLOB_KEY=$K31
eq "31-character blob key: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"; eq "31-character blob key: nothing ran" 0 "$(count runner)"
has "31-character blob key: reason in the summary" "$(cat "$W/gh_summary")" "shorter than 32"; eq "31-character blob key: no output" "" "$(cat "$W/gh_out")"
rm -rf "$W"
plan_world; cand_row; echo '[{"repo":"o/r","allow_docker":false,"n":0}]' > "$W/state/rows.json"; run_plan FACTORY_CI_BLOB_KEY=$K32
eq "32-character blob key: accepted" 0 "$RC"
b=$(sed -n 's/^matrix=//p' "$W/gh_out" | jq -r '.include[0].blob')
eq "32-character blob key: round trip" o/r "$(printf '%s' "$b" | FACTORY_CI_BLOB_KEY=$K32 fci_blob_decrypt | jq -r .repo)"
rm -rf "$W"
# a docker row needs a docker key of 32 characters, refused before any status is posted
for dk in "$K31" ""; do
  plan_world; cand_row; jq '.rows[0].allow_docker = true' "$W/state/cand.json" > "$W/t" && mv "$W/t" "$W/state/cand.json"
  run_plan FACTORY_CI_BLOB_KEY_DOCKER="$dk"
  eq "docker row with a docker key of ${#dk} characters: refused" 1 "$([ "$RC" -ne 0 ] && echo 1 || echo 0)"
  eq "docker key of ${#dk} characters: no status posted" A "$(phases)"; rm -rf "$W"
done
plan_world; cand_row; run_plan FACTORY_CI_BLOB_KEY_DOCKER=
eq "no docker row needs no docker key" 0 "$RC"; rm -rf "$W"

# ---------------------------------------------------------------- README (R13)
eq "README states RUNNER_IMAGE_DIGEST without an @" 0 "$(grep 'RUNNER_IMAGE_DIGEST' "$here/../../rust-ci-runner/README.md" | grep -c '@')"
eq "README names the digest as sha256:<64 hex>" 1 "$([ "$(grep -c 'sha256:<64 hex>' "$here/../../rust-ci-runner/README.md")" -ge 1 ] && echo 1 || echo 0)"
eq "README marks FACTORY_CI_BLOB_KEY_DOCKER optional" 1 "$([ "$(grep -m1 'FACTORY_CI_BLOB_KEY_DOCKER' "$here/../../rust-ci-runner/README.md" | grep -c optional)" -ge 1 ] && echo 1 || echo 0)"

# ---------------------------------------------------------------- names (M3)
for hostile in '$(touch PWNED)' 'a;b' 'A B' '1BAD' '`touch PWNED`' 'A-B' '../x'; do
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
new_world_hard; mkrow fail_closed=true retry=true; names; store_add FACTORY_CI_LOG_URL https://loki.example.com; store_add FACTORY_CI_LOG_USER u-1234; store_add FACTORY_CI_LOG_TOKEN t-12345
run_job; sl=$(cat "$W/state/sudo.log" 2>/dev/null || true)
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
  OUT=$(env -i PATH="$W/bin:/usr/bin:/bin" HOME="$W" RUNNER_TEMP="$W/rt" ITEM_BLOB="$BLOB" FACTORY_CI_BLOB_KEY="$KEY" FACTORY_CI_TOKEN=gh-token-ZZ9 \
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

# ---------------------------------------------------------------- placeholder checksum (lint)
tmp=$(mktemp -d); cp "$here"/factory-ci-*.sh "$tmp"/; sed -i 's/^BWS_SHA256=.*/BWS_SHA256=REPLACE_ME/' "$tmp/factory-ci-secrets.sh"
wf="$here/../workflows/factory-ci-tick.yml"
bash "$tmp/factory-ci-lint-workflow.sh" "$wf" > /dev/null 2>&1; eq "placeholder tolerated by the lint" 0 "$?"
FACTORY_CI_LINT_STRICT=1 bash "$tmp/factory-ci-lint-workflow.sh" "$wf" > /dev/null 2>&1; eq "placeholder refused under FACTORY_CI_LINT_STRICT=1" 1 "$([ "$?" -ne 0 ] && echo 1 || echo 0)"
rm -rf "$tmp"

printf '%s checks, %s failed\n' "$total" "$failed"
[ "$failed" -eq 0 ]
