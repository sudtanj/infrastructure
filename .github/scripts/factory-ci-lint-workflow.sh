#!/usr/bin/env bash
# Lint for the tick workflow, the release workflow and the rest of the workflow directory (T026, T015;
# FR-001, FR-003, FR-015, FR-026, FR-029; AS 5.1, 5.4, 6.2, 10.1, 10.2). Rules: end of specs/002-ci-runner/contracts/tick-workflow.md.
#   factory-ci-lint-workflow.sh WORKFLOW [RELEASE]     lint; one "FAIL rule: why" line per problem
#   factory-ci-lint-workflow.sh --self-test WORKFLOW [RELEASE]
# RELEASE defaults to rust-ci-runner-release.yml beside WORKFLOW. FACTORY_CI_LINT_STRICT=1 refuses the
# BWS_SHA256 placeholder in factory-ci-secrets.sh (the release gate sets it after T054).
# shortcut: line-based reading of the YAML, not a parser; fits the fixed layout of this file, so
# upgrade to yq if the workflow gains anchors or flow-style maps.
set -u
here=$(cd "$(dirname "$0")" && pwd)

if [ "${1:-}" = --self-test ]; then
  shift
  wf=${1:?workflow path}
  rel=${2:-$(dirname "$wf")/rust-ci-runner-release.yml}
  tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
  fails=0
  bash "$0" "$wf" "$rel" > "$tmp/good.out" 2>&1 || { echo "FAIL self-test: the good copy does not pass"; cat "$tmp/good.out"; fails=1; }
  # rule | target (wf, rel, self, mir, or yaml for a .yaml copy of wf) | sed expression
  while IFS='|' read -r rule target expr; do
    [ -n "$rule" ] || continue
    rm -f "$tmp/extra.yaml"; cp "$wf" "$tmp/wf.yml"; cp "$rel" "$tmp/rel.yml"; cp "$(dirname "$wf")/factory-ci-selftest.yml" "$(dirname "$wf")/factory-ci-mirror-images.yml" "$tmp/" 2> /dev/null
    [ "$target" = yaml ] && cp "$wf" "$tmp/extra.yaml"
    src=$tmp/wf.yml; [ "$target" = rel ] && src=$tmp/rel.yml; [ "$target" = self ] && src=$tmp/factory-ci-selftest.yml; [ "$target" = mir ] && src=$tmp/factory-ci-mirror-images.yml; [ "$target" = yaml ] && src=$tmp/extra.yaml
    sed -E "$expr" "$src" > "$tmp/m"
    if cmp -s "$tmp/m" "$src"; then echo "FAIL self-test: mutation for $rule changed nothing"; fails=1; continue; fi
    mv "$tmp/m" "$src"
    if out=$(bash "$0" "$tmp/wf.yml" "$tmp/rel.yml" 2>&1); then
      echo "FAIL self-test: mutated copy passed for $rule"; fails=1
    else
      case $out in *"FAIL $rule:"*) ;; *) echo "FAIL self-test: $rule not named in: $out"; fails=1 ;; esac
    fi
  done <<'MUTATIONS'
no-schedule|wf|s/^  workflow_dispatch:/&\n  schedule:/
no-pull-request-target|wf|s/^  workflow_dispatch:/&\n  pull_request_target:/
dispatch|wf|s/workflow_dispatch:/workflow_run:/
concurrency-group|wf|s/group: factory-ci-tick/group: other/
concurrency-cancel|wf|s/cancel-in-progress: false/cancel-in-progress: true/
runs-on|wf|s/runs-on: ubuntu-24.04/runs-on: ubuntu-latest/
max-parallel|wf|s/max-parallel: 4/max-parallel: 8/
run-expression|wf|s/run: bash .github\/scripts\/factory-ci-plan.sh/run: echo ${{ github.sha }}/
main-guard|wf|s/if: github.ref == 'refs\/heads\/main' && needs.plan.outputs.count !=/if: needs.plan.outputs.count !=/
environment|wf|/^    environment: factory-ci$/d
if-secrets|wf|s/if: always\(\) && steps.job.outcome/if: secrets.X != '' \&\& steps.job.outcome/
to-json-secrets|wf|s/^( *)ITEM_BLOB: .*/&\n\1X: ${{ toJSON(secrets) }}/
matrix-ref|wf|s/name: item \$\{\{ matrix.n \}\}/name: item ${{ matrix.n }} ${{ matrix.repo }}/
item-name|wf|s/^( *)ITEM_BLOB: /\1ITEM_REPO: /
guard-first|wf|s/name: guard, pinned images/name: checks/
verdict-always|wf|s/^    if: always\(\)$/    if: success()/
uses-pin|wf|s/(actions\/checkout@)[0-9a-f]{40}/\1v4/
bws-scope|wf|s/secrets.BWS_ACCESS_TOKEN_DOCKER/secrets.BWS_ACCESS_TOKEN/
run-docker-name|wf|s/^    name: item \$\{\{ matrix.n \}\}/    name: docker ${{ matrix.n }}/
have-bws|wf|s/HAVE_BWS:/HAVE:/
verdict-ping|wf|s/--max-time 10/--max-time 99/
verdict-ping|wf|s/^( *)set \+x$/\1set -x/
verdict-ping|wf|s/^( *)(curl -fsS --max-time 10 .*)$/&\n\1echo "$FACTORY_CI_HEARTBEAT_URL"/
verdict-ping|wf|s/^( *)(curl -fsS --max-time 10 .*)$/&\n\1curl -fsS --max-time 10 "$FACTORY_CI_HEARTBEAT_URL"/
verdict-ping|wf|s/GITHUB_REF" = refs\/heads\/main/GITHUB_REF" != x/
verdict-ping|wf|s/suffix=\/fail/suffix=/
verdict-fail|wf|s/"\$FAIL_TICK" != 1/true/
verdict-fail|wf|s/"\$PLAN_RESULT" = success/true/
verdict-always|wf|s/needs: \[plan, run, run-docker\]/needs: [plan, run]/
matrix-ref|wf|s/^( *)B_PRIVATE: .*/&\n\1X: ${{ matrix.blob }}/
item-name|wf|s/run: bash .github\/scripts\/factory-ci-status.sh/run: echo "$ITEM_BLOB"/
run-docker-name|wf|s/fromJSON\(needs.plan.outputs.matrix_docker\)/fromJSON(needs.plan.outputs.matrix)/
bws-scope|wf|s/secrets.FACTORY_CI_TOKEN_DOCKER/secrets.FACTORY_CI_TOKEN/
blob-key-length|wf|s/\{64\}\$ \]\]/{8}$ ]]/
no-packages|wf|s/^( *)contents: read/&\n\1packages: read/
blob-key-scope|wf|s/secrets.FACTORY_CI_BLOB_KEY_DOCKER \}\}$/secrets.FACTORY_CI_BLOB_KEY }}/
blob-key-scope|wf|s/^( *)FACTORY_CI_TOKEN: \$\{\{ secrets.FACTORY_CI_TOKEN \}\}$/&\n\1FACTORY_CI_BLOB_KEY_DOCKER: ${{ secrets.FACTORY_CI_BLOB_KEY_DOCKER }}/
release-tag|rel|s/rust-ci-runner-v\*/x-v*/
release-targets|rel|s/aarch64-unknown-linux-musl/x/g
release-platforms|rel|s/linux\/amd64,linux\/arm64/linux\/amd64/
release-digest|rel|s/RUNNER_IMAGE_DIGEST/X/
selftest-shell|self|s/^  shell:/  other:/
selftest-shell|self|s/test-factory-ci-hardening.sh/x.sh/
selftest-shell|self|s/^  list:/      - uses: actions\/upload-artifact@v4\n&/
no-pull-request-target|wf|s/^on:$/on: [pull_request_target]\nxon:/
no-pull-request-target|wf|s/^on:$/on: {pull_request_target: {types: [opened]}}\nxon:/
no-pull-request-target|rel|s/^on:$/on: [pull_request_target]\nxon:/
no-pull-request-target|self|s/^  pull_request:/  pull_request_target:/
runs-on-pinned|rel|s/runs-on: ubuntu-24.04/runs-on: ubuntu-latest/
runs-on-pinned|rel|s/runs-on: ubuntu-24.04/runs-on: ubuntu-24.04.1/
runs-on-pinned|self|s/runs-on: ubuntu-24.04/runs-on: ubuntu-22.04/
no-cargo|wf|s/^( *)chmod \+x .*/&\n\1cargo build/
selftest-isolated|self|s/^  workflow_dispatch:/&\n  workflow_run:/
selftest-isolated|self|s/^( *)group: selftest-.*/&\n\1x: ${{ secrets.X }}/
selftest-isolated|self|s/^  check:/&\n    environment: x/
selftest-isolated|self|s/^  contents: read/&\n  id-token: write/
selftest-paths|self|s/^  pull_request:/&\n    paths: ["x"]/
selftest-checkout|self|s/persist-credentials: false/persist-credentials: true/
selftest-checkout|self|s/^( *)- uses: actions\/checkout@.*/&\n\1- uses: actions\/checkout@0000000000000000000000000000000000000000/
tick-on-only-dispatch|wf|s/^  workflow_dispatch:/&\n  "push":/
tick-on-only-dispatch|wf|s/^  workflow_dispatch:/&\n  push:/
tick-on-only-dispatch|wf|s/^on:$/on: [workflow_dispatch]\nxon:/
runs-on-pinned|yaml|s/runs-on: ubuntu-24.04/runs-on: ubuntu-latest/
uses-pin|rel|s/(actions\/checkout@)[0-9a-f]{40}/\1v4/
permissions|rel|/^permissions:/d
tick-environment|wf|s/^    environment: factory-ci-docker$/    environment: factory-ci/
tick-environment|wf|s/^    environment: factory-ci$/    environment: factory-ci-docker/
tick-environment|wf|/^  verdict:/,$ s/^    environment: factory-ci-plan$/    environment: factory-ci/
main-guard|wf|s/^    if: github.ref == 'refs\/heads\/main'$/& || github.event_name == 'x'/
main-guard|wf|s/(main' && needs.plan.outputs.count !=) '0'/\1 '0' || true/
main-guard|wf|s/^    if: github.ref == 'refs\/heads\/main' && needs.plan.outputs.count_docker != '0'/& || x/
main-guard|mir|s/^    if: github.ref == 'refs\/heads\/main'$/& || x/
selftest-shell|self|s/^  list:/    secrets: inherit\n&/
selftest-shell|self|s/^  list:/    x: ${{ secrets[1] }}\n&/
MUTATIONS
  [ "$fails" -eq 0 ] && echo "self-test ok"
  exit "$fails"
fi

wf=${1:?usage: factory-ci-lint-workflow.sh WORKFLOW [RELEASE]}
rel=${2:-$(dirname "$wf")/rust-ci-runner-release.yml}
[ -r "$wf" ] || { echo "FAIL input: cannot read $wf" >&2; exit 1; }
rc=0
bad() { echo "FAIL $1: $2" >&2; rc=1; }
has() { grep -Eq -- "$1" "$wf"; }
# job NAME: the lines of one job (two-space key under jobs:) up to the next job
job() { awk -v j="$1" '$0 ~ /^  [A-Za-z0-9_-]+:[ ]*$/ { on = ($1 == j ":") } on' "$wf"; }

# triggers
has '^  schedule:' && bad no-schedule "a schedule trigger is present"
has '^  workflow_dispatch:' || bad dispatch "workflow_dispatch is missing"
# the on: block allows workflow_dispatch only; a flow-style on: hides keys from this reader
has '^on:[ ]*$' || bad tick-on-only-dispatch "on: is not a block (a flow-style on: is refused)"
awk '/^on:/ { on = 1; next } /^[^ #]/ { on = 0 } on && /^  [^ #]/' "$wf" | grep -Evq '^  workflow_dispatch:[ ]*$' && bad tick-on-only-dispatch "the on: block has a key other than workflow_dispatch"
has '^  group: factory-ci-tick$' || bad concurrency-group "group: factory-ci-tick is missing"
has '^  cancel-in-progress: false$' || bad concurrency-cancel "cancel-in-progress: false is missing"
has '^permissions: \{\}$' || bad permissions "workflow-level permissions: {} is missing"

# every workflow in the directory (research D9): pinned runner, no pull_request_target, cargo only
# in the bootstrap-exception workflows. Comment lines are not read.
dir=$(dirname "$wf")
code() { grep -Ev '^ *#' "$1"; }
for f in "$dir"/*.yml "$dir"/*.yaml; do
  [ -r "$f" ] || continue
  code "$f" | grep 'runs-on:' | grep -Evq 'runs-on: ubuntu-24\.04$' && bad runs-on-pinned "$f: a runs-on is not exactly ubuntu-24.04"
  code "$f" | grep -Eq '(^|[^A-Za-z_-])pull_request_target([^A-Za-z_-]|$)' && bad no-pull-request-target "$f: pull_request_target is present"
  code "$f" | grep -E '^ *(- )?uses:' | grep -Ev '@[0-9a-f]{40}( |$)' | grep -q . && bad uses-pin "$f: a uses: is not pinned to a 40-hex commit"
  code "$f" | grep -Eq '^permissions:' || bad permissions "$f: workflow-level permissions: is missing"
  case $f in "$dir"/factory-ci-selftest.yml | "$rel") continue ;; esac
  code "$f" | grep -Eq '(^|[^A-Za-z_-])cargo ' && bad no-cargo "$f: a cargo command outside the bootstrap-exception workflows"
done

# runners and matrix limits
[ "$(grep -c 'runs-on:' "$wf")" -ge 4 ] || bad runs-on "fewer than four runs-on lines"
grep 'runs-on:' "$wf" | grep -Evq 'runs-on: ubuntu-24\.04$' && bad runs-on "a runs-on is not ubuntu-24.04"
[ "$(grep -Ec '^      max-parallel: 4$' "$wf")" -eq 2 ] || bad max-parallel "max-parallel: 4 is not on both matrix jobs"

# no ${{ inside a run: block or a one-line run:
lines=$(awk '
  { match($0, /^ */); ind = RLENGTH }
  inb && (ind > bi || $0 ~ /^ *$/) { if (index($0, "${{")) print NR; next }
  { inb = 0 }
  /^ *(- )?run:/ {
    if ($0 ~ /run: *[|>][-+]?$/) { inb = 1; bi = ind + ($0 ~ /^ *- / ? 2 : 0) }
    else if (index($0, "${{")) print NR
  }' "$wf" | tr '\n' ' ')
[ -z "$lines" ] || bad run-expression "\${{ inside run: at line(s) $lines"

# the main-only guard: end-anchored, extra conditions only with &&, never ||
guard_ok() { printf '%s\n' "$1" | grep -Eq "^    if: github\.ref == 'refs/heads/main'( && .*)?\$" && ! printf '%s\n' "$1" | grep -Fq '||'; }
# the Environment each tick job runs in (the allowlist)
want_env() { case $1 in run) echo factory-ci ;; run-docker) echo factory-ci-docker ;; *) echo factory-ci-plan ;; esac; }
# guards: every job but verdict
for j in plan run run-docker; do
  t=$(job "$j")
  [ -n "$t" ] || { bad job "job $j is missing"; continue; }
  guard_ok "$(printf '%s\n' "$t" | grep -E '^    if:' | head -1)" || bad main-guard "job $j: the if: is not end-anchored main-only without ||"
  printf '%s\n' "$t" | grep -Eq '^    environment:' || bad environment "job $j has no environment"
  printf '%s\n' "$t" | grep -Eqx "    environment: $(want_env "$j")" || bad tick-environment "job $j is not in $(want_env "$j")"
  # the guard step comes before the wrapper call
  g=$(printf '%s\n' "$t" | grep -n 'name: guard, pinned images' | head -1 | cut -d: -f1)
  w=$(printf '%s\n' "$t" | grep -nE 'run: bash \.github/scripts/factory-ci-(plan|job)\.sh' | head -1 | cut -d: -f1)
  { [ -n "$g" ] && [ -n "$w" ] && [ "$g" -lt "$w" ]; } || bad guard-first "job $j: the pinned-image guard is missing or comes after the wrapper"
  printf '%s\n' "$t" | grep -Fq '=~ ^[0-9a-f]{64}$ ]]' || bad blob-key-length "job $j: the guard does not require a 64 lower-case hex blob key"
  for v in RUNNER_IMAGE_DIGEST FACTORY_CI_CLONE_IMAGE FACTORY_CI_DIND_IMAGE; do
    printf '%s\n' "$t" | grep -q "$v: \\\${{ vars\\.$v }}" || bad guard-first "job $j: guard does not read $v"
  done
done
v=$(job verdict)
mirror=$dir/factory-ci-mirror-images.yml
[ -r "$mirror" ] && { guard_ok "$(grep -E '^    if:' "$mirror" | head -1)" || bad main-guard "$mirror: the if: is not end-anchored main-only without ||"; }
[ -n "$v" ] || bad job "job verdict is missing"
printf '%s\n' "$v" | grep -Eq '^    if: always\(\)$' || bad verdict-always "verdict has no if: always()"
printf '%s\n' "$v" | grep -Eqx '    environment: factory-ci-plan' || bad tick-environment "job verdict is not in factory-ci-plan"
printf '%s\n' "$v" | grep -Eq '^    needs: \[plan, run, run-docker\]$' || bad verdict-always "verdict does not need plan, run and run-docker"
printf '%s\n' "$v" | grep -q 'curl -fsS --max-time 10 "\$ping" > /dev/null 2>&1 || true' || bad verdict-ping "the heartbeat is not one quiet curl --max-time 10 that ignores failure"
printf '%s\n' "$v" | grep -q 'suffix=/fail' || bad verdict-ping "a red tick does not ping /fail"
printf '%s\n' "$v" | grep -Eq '^ *set \+x$' || bad verdict-ping "the heartbeat step has no set +x"
printf '%s\n' "$v" | grep -Eq 'set -x|xtrace|bash -x' && bad verdict-ping "the verdict job turns tracing on"
printf '%s\n' "$v" | grep -Eq '(echo|printf|cat).*HEARTBEAT_URL' && bad verdict-ping "the verdict job prints the heartbeat URL"
[ "$(printf '%s\n' "$v" | grep -c 'curl ')" -eq 1 ] || bad verdict-ping "the verdict job has not exactly one curl call"
printf '%s\n' "$v" | grep -q 'GITHUB_REF" = refs/heads/main' || bad verdict-ping "the heartbeat is not limited to main"
printf '%s\n' "$v" | grep -q 'PLAN_RESULT" = success' || bad verdict-fail "verdict does not fail on a plan that did not succeed"
printf '%s\n' "$v" | grep -q 'FAIL_TICK" != 1' || bad verdict-fail "verdict does not fail on fail_tick"

# secrets
grep -Ei '^ *(- )?if:' "$wf" | grep -q 'secrets\.' && bad if-secrets "an if: reads secrets."
grep -qi 'toJSON(secrets)' "$wf" && bad to-json-secrets "toJSON(secrets) is used"
has 'HAVE_BWS: \$\{\{ secrets\.BWS_ACCESS_TOKEN(_DOCKER)? != '"''"' \}\}' || bad have-bws "HAVE_BWS is not set from secrets in env:"
grep -q 'sm-action' "$wf" && bad sm-action "sm-action is used"
# a secret name, not a prefix of a longer one
sec() { printf '%s\n' "$1" | grep -Eq "secrets\\.$2([^A-Za-z0-9_]|\$)"; }
for n in BWS_ACCESS_TOKEN FACTORY_CI_TOKEN; do
  sec "$(job run-docker)" "$n" && bad bws-scope "run-docker uses secrets.$n, not the _DOCKER name"
  sec "$(job run)" "${n}_DOCKER" && bad bws-scope "run uses secrets.${n}_DOCKER"
  sec "$v" "$n" && bad bws-scope "verdict uses secrets.$n"
  sec "$v" "${n}_DOCKER" && bad bws-scope "verdict uses secrets.${n}_DOCKER"
done

for n in FACTORY_CI_BLOB_KEY; do
  sec "$(job run-docker)" "$n" && bad blob-key-scope "run-docker uses secrets.$n, not the _DOCKER name"
  sec "$(job run)" "${n}_DOCKER" && bad blob-key-scope "run uses secrets.${n}_DOCKER"
  sec "$v" "$n" && bad blob-key-scope "verdict uses secrets.$n"
  sec "$v" "${n}_DOCKER" && bad blob-key-scope "verdict uses secrets.${n}_DOCKER"
done
sec "$(job run-docker)" FACTORY_CI_BLOB_KEY_DOCKER || bad blob-key-scope "run-docker does not map the docker blob key"
sec "$(job run)" FACTORY_CI_BLOB_KEY || bad blob-key-scope "run does not map the blob key"
has '^ +packages: +read' && bad no-packages "a job has packages: read (the runner image is public)"

# matrix and item values
refs=$(grep -o 'matrix\.[A-Za-z_]*' "$wf" | sort -u | tr '\n' ' ')
case $refs in ''|'matrix.blob matrix.n '|'matrix.blob '|'matrix.n ') ;; *) bad matrix-ref "matrix references other than n and blob: $refs" ;; esac
grep 'matrix\.n' "$wf" | grep -Evq '^    name: item \$\{\{ matrix\.n \}\}$' && bad matrix-ref "matrix.n is used outside the job name"
grep 'matrix\.blob' "$wf" | grep -Evq '^ *ITEM_BLOB: \$\{\{ matrix\.blob \}\}$' && bad matrix-ref "matrix.blob is used outside ITEM_BLOB"
grep -Eo 'ITEM_[A-Z_]*' "$wf" | grep -qv '^ITEM_BLOB$' && bad item-name "an ITEM_ name other than ITEM_BLOB"
grep -E 'ITEM_' "$wf" | grep -Evq '^ *ITEM_BLOB: ' && bad item-name "ITEM_BLOB is used outside an env: entry"
for j in run run-docker; do
  job "$j" | grep -Eq '^    name: item \$\{\{ matrix\.n \}\}$' || bad run-docker-name "job $j is not named item N"
done
job run-docker | grep -q 'matrix_docker' || bad run-docker-name "run-docker does not read matrix_docker"
job run | grep -q 'outputs\.matrix)' || bad run-docker-name "run does not read matrix"
job run | grep -q 'name: status fallback' || bad fallback "run has no status fallback step"


# bws install, once it exists (T051): checksum and archive listing
if has 'unzip|\.zip'; then
  has 'sha256sum -c' || bad bws-install "a download is not followed by sha256sum -c"
  has 'unzip -Z1' || bad bws-install "the install does not check unzip -Z1 lists exactly bws"
fi
# the library's checksum placeholder is tolerated until T054
lib=$here/factory-ci-secrets.sh
if [ -r "$lib" ]; then
  sum=$(sed -n 's/^BWS_SHA256=//p' "$lib" | head -1 | tr -d "\"'")
  if [ "$sum" = REPLACE_ME ]; then
    [ "${FACTORY_CI_LINT_STRICT:-}" != 1 ] || bad bws-sha256 "BWS_SHA256 is still the placeholder"
  else
    printf '%s' "$sum" | grep -Eqx '[0-9a-f]{64}' || bad bws-sha256 "BWS_SHA256 is neither a 64-hex digest nor the placeholder"
  fi
elif [ "${FACTORY_CI_LINT_STRICT:-}" = 1 ]; then
  bad bws-sha256 "factory-ci-secrets.sh is missing"
fi
st=$here/factory-ci-status.sh
if [ -r "$st" ]; then grep -q 'fci-reason' "$st" || bad fallback "factory-ci-status.sh does not read fci-reason"; fi

# self-test workflow (R12): the shell tests run in CI, and no TEMP artifact step is left
self=$(dirname "$wf")/factory-ci-selftest.yml
if [ -r "$self" ]; then
  sj=$(awk '$0 ~ /^  [A-Za-z0-9_-]+:[ ]*$/ { on = ($1 == "shell:") } on' "$self")
  [ -n "$sj" ] || bad selftest-shell "factory-ci-selftest.yml has no shell job"
  printf '%s\n' "$sj" | grep -Eq 'runs-on: ubuntu-24\.04$' || bad selftest-shell "the shell job is not on ubuntu-24.04"
  for s in test-factory-ci-secrets.sh test-factory-ci-hardening.sh 'factory-ci-lint-workflow.sh --self-test'; do
    printf '%s\n' "$sj" | grep -q -- "$s" || bad selftest-shell "the shell job does not run $s"
  done
  grep -q 'upload-artifact' "$self" && bad selftest-shell "factory-ci-selftest.yml still has an upload-artifact step"
  sc=$(code "$self")
  for t in 'workflow_run' 'secrets\.' 'environment:' 'id-token'; do
    printf '%s\n' "$sc" | grep -Eq "(^|[^A-Za-z_-])$t" && bad selftest-isolated "factory-ci-selftest.yml has $t"
  done
  printf '%s\n' "$sc" | grep -Eq '^    paths(-ignore)?:' && bad selftest-paths "factory-ci-selftest.yml filters pull_request by paths"
  printf '%s\n' "$sc" | grep -Eq 'secrets: *inherit|secrets\[' && bad selftest-shell "factory-ci-selftest.yml has secrets: inherit or secrets["
  [ "$(printf '%s\n' "$sc" | grep -c 'uses: actions/checkout@')" -eq "$(printf '%s\n' "$sc" | grep -c 'persist-credentials: false')" ] \
    || bad selftest-checkout "a checkout in factory-ci-selftest.yml lacks persist-credentials: false"
else
  bad selftest-shell "cannot read $self"
fi

# release workflow (FR-015)
if [ -r "$rel" ]; then
  r() { grep -Eq -- "$1" "$rel"; }
  r 'tags: \["rust-ci-runner-v\*"\]' || bad release-tag "the rust-ci-runner-v* tag trigger is gone"
  r 'x86_64-unknown-linux-musl' && r 'aarch64-unknown-linux-musl' || bad release-targets "a build target is gone"
  r 'platforms: linux/amd64,linux/arm64' || bad release-platforms "linux/amd64,linux/arm64 is gone"
  r 'RUNNER_IMAGE_DIGEST' || bad release-digest "the RUNNER_IMAGE_DIGEST summary line is gone"
else
  bad release-tag "cannot read $rel"
fi
exit "$rc"
