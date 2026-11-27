#!/usr/bin/env bash
# Plain grep checks of .github/workflows/factory-monorepo-ci.yml (008 T020; FR-001 to FR-005, FR-024, SC-001, SC-005; I10).
# Test-first: written before the trimmed workflow (T019) exists, so it fails against the old file.
# The checks only read the workflow; nothing is edited and there is no self-test mode.
# Usage: test-factory-monorepo-ci.sh [WORKFLOW]   (default: ../workflows/factory-monorepo-ci.yml beside this script)
set -u
here=$(cd "$(dirname "$0")" && pwd)
wf=${1:-$here/../workflows/factory-monorepo-ci.yml}
total=0
failed=0

fail() { total=$((total + 1)); failed=$((failed + 1)); printf 'FAIL: %s\n' "$1"; }
pass() { total=$((total + 1)); }
eq() { if [ "$2" = "$3" ]; then pass; else fail "$1: want [$2] got [$3]"; fi; }
ok() { if "${@:2}"; then pass; else fail "$1"; fi; }

[ -f "$wf" ] || { echo "no such workflow: $wf" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

# Lines of the file without comment lines.
grep -v '^[[:space:]]*#' "$wf" > "$tmp/code.yml"

# Every `run:` block, one after the other; a line `#--block--` ends each block.
awk '
  function indent(s) { match(s, /^ */); return RLENGTH }
  {
    if (in_run) {
      if ($0 ~ /^[[:space:]]*$/ || indent($0) > run_indent) { print; next }
      print "#--block--"; in_run = 0
    }
    if ($0 ~ /^[[:space:]]*(- )?run:[[:space:]]*[|>][-+]?[[:space:]]*$/) {
      run_indent = indent($0) + ($0 ~ /^[[:space:]]*- run:/ ? 2 : 0); in_run = 1; next
    }
    if ($0 ~ /^[[:space:]]*(- )?run:[[:space:]]*[^|>[:space:]]/) { sub(/^[[:space:]]*(- )?run:[[:space:]]*/, ""); print; print "#--block--" }
  }
  END { if (in_run) print "#--block--" }
' "$tmp/code.yml" > "$tmp/runs.sh"

# Logical lines of the run blocks: a trailing backslash joins the next line.
sed -e ':a' -e '/\\$/{N;s/\\\n[[:space:]]*/ /;ba' -e '}' "$tmp/runs.sh" > "$tmp/logical.sh"
grep 'docker run' "$tmp/logical.sh" > "$tmp/dockerrun.txt"

# FR-001: one trigger step, exactly one `docker run`
eq "exactly one docker run line" 1 "$(grep -c 'docker run' "$tmp/logical.sh")"
eq "docker run is quiet (-q)" 1 "$(grep -c 'docker run -q ' "$tmp/dockerrun.txt")"

# FR-001: none of the old shell logic stays in the workflow
for word in bws jq curl cargo unzip; do
  ok "no $word in the workflow" test "$(grep -cw "$word" "$tmp/code.yml")" = 0
done
for pat in 'git clone' 'grep -E' 'grep -qE' 'sha256sum' 'docker cp' 'docker pull' 'GITHUB_STEP_SUMMARY'; do
  ok "no '$pat' in the workflow" test "$(grep -c -- "$pat" "$tmp/code.yml")" = 0
done

# FR-003: the only secrets are the access token and the job token
eq "secrets used" "BWS_ACCESS_TOKEN
GITHUB_TOKEN" "$(grep -o 'secrets\.[A-Za-z_]*' "$tmp/code.yml" | sed 's/^secrets\.//' | sort -u)"
ok "no id-token or pull_request_target" test "$(grep -cE 'id-token|pull_request_target' "$tmp/code.yml")" = 0

# FR-002, FR-004: no expression inside a run block, values reach the script through env:
ok "no \${{ in a run block" test "$(grep -c '\${{' "$tmp/runs.sh")" = 0
ok "the secret goes through env:" grep -qE '^[[:space:]]+BWS_ACCESS_TOKEN: \$\{\{ secrets\.BWS_ACCESS_TOKEN \}\}' "$tmp/code.yml"
for v in REPO REF PROJECT; do
  ok "$v reaches the script through env:" grep -qE "^[[:space:]]+$v: \\$\\{\\{ (inputs|github\\.event\\.inputs)\\.[a-z]+ \\}\\}" "$tmp/code.yml"
done

# FR-002: dispatch inputs, main guard, Environment, limits
for i in repo ref project; do
  ok "input $i" grep -qE "^      $i:" "$tmp/code.yml"
done
ok "workflow_dispatch trigger" grep -q 'workflow_dispatch:' "$tmp/code.yml"
ok "main guard" grep -qF "github.ref == 'refs/heads/main'" "$tmp/code.yml"
ok "environment: factory-ci" grep -qE '^[[:space:]]+environment: factory-ci[[:space:]]*$' "$tmp/code.yml"
ok "timeout-minutes: 45" grep -qE '^[[:space:]]+timeout-minutes: 45[[:space:]]*$' "$tmp/code.yml"
grp=$(grep 'group:' "$tmp/code.yml")
case $grp in *inputs.repo*inputs.ref*inputs.project*) pass ;; *) fail "concurrency group does not name repo, ref and project" ;; esac
ok "cancel-in-progress is false" grep -qE 'cancel-in-progress: false' "$tmp/code.yml"
ok "top-level permissions are empty" grep -qE '^permissions: \{\}' "$tmp/code.yml"
ok "no write permission" test "$(grep -c ': write' "$tmp/code.yml")" = 0

# every uses: pinned to a 40-hex commit
unpinned=$(grep -E '^[[:space:]]*(- )?uses:' "$tmp/code.yml" | grep -vE '@[0-9a-f]{40}([[:space:]]|$)' || true)
eq "unpinned uses:" "" "$unpinned"
ok "at least one uses: (the checkout)" grep -qE '^[[:space:]]*(- )?uses: actions/checkout@' "$tmp/code.yml"

# the image: pinned by a real digest, no placeholder
img=$(grep -E '^[[:space:]]+RUNNER_IMAGE:' "$tmp/code.yml" | head -n 1)
ok "RUNNER_IMAGE pinned by digest" grep -qE 'RUNNER_IMAGE: ghcr\.io/sudtanj/rust-ci-runner:[0-9]+\.[0-9]+\.[0-9]+@sha256:[0-9a-f]{64}[[:space:]]*$' <<< "$img"
ok "no placeholder digest" test "$(grep -ciE 'TODO|<digest>|0{64}' "$tmp/code.yml")" = 0

# FR-005 and I10: what the docker run line holds
dr=$(cat "$tmp/dockerrun.txt")
case $dr in *'-e BWS_ACCESS_TOKEN '* | *'-e BWS_ACCESS_TOKEN') pass ;; *) fail "-e BWS_ACCESS_TOKEN (no value) missing on the docker run line" ;; esac
case $dr in *'BWS_ACCESS_TOKEN='*) fail "BWS_ACCESS_TOKEN= on the docker run line" ;; *) pass ;; esac
case $dr in *'--allow-docker'*) fail "--allow-docker on the docker run line" ;; *) pass ;; esac
case $dr in *DOCKER_CONFIG*) fail "DOCKER_CONFIG on the docker run line (-e or mount)" ;; *) pass ;; esac
for flag in '--rm' '--init' '--cap-drop ALL' '--security-opt no-new-privileges' '--read-only' '--tmpfs /tmp:rw,size=64m' '-e HOME=/tmp' \
  '-v /var/run/docker.sock:/var/run/docker.sock' 'ci --repos /etc/factory-ci/repos.toml'; do
  case $dr in *"$flag"*) pass ;; *) fail "docker run line lacks: $flag" ;; esac
done
for flag in '--user "$(id -u):$(id -g)"' '--group-add "$(stat -c %g /var/run/docker.sock)"' '--repo "$REPO"' '--ref "$REF"' '--project "$PROJECT"' '--work "$RUNNER_TEMP/ci"' '"$RUNNER_IMAGE"'; do
  case $dr in *"$flag"*) pass ;; *) fail "docker run line lacks: $flag" ;; esac
done
case $dr in *'--privileged'* | *'--network host'* | *'--pid'* | *'--cap-add'*) fail "docker run line has a forbidden flag" ;; *) pass ;; esac
ok "the work dir is mounted at the identical path" grep -qF -- '-v "$RUNNER_TEMP/ci:$RUNNER_TEMP/ci"' "$tmp/dockerrun.txt"
ok "the watch list is mounted read-only" grep -qF '/etc/factory-ci/repos.toml:ro' "$tmp/dockerrun.txt"
eq "mounts on the docker run line (socket, work dir, watch list)" 3 "$(grep -o -- " -v " "$tmp/dockerrun.txt" | wc -l | tr -d " ")"

# FR-005: every `$` on the docker run line sits inside double quotes (single quotes are literal)
unquoted=$(awk '
  { q = 0; s = 0
    for (i = 1; i <= length($0); i++) {
      c = substr($0, i, 1)
      if (c == "\\" && !s) { i++; continue }
      if (c == "\047" && !q) { s = !s; continue }
      if (c == "\"" && !s) { q = !q; continue }
      if (c == "$" && !q && !s) { print substr($0, i, 20); exit }
    } }
' "$tmp/dockerrun.txt")
eq "unquoted \$ on the docker run line" "" "$unquoted"

# the login: stdin, quiet, temporary DOCKER_CONFIG removed by a trap
ok "docker login reads the password on stdin" grep -q 'docker login ghcr.io .*--password-stdin' "$tmp/logical.sh"
ok "login output is silenced" grep -qE 'docker login .*(>/dev/null|> /dev/null)' "$tmp/logical.sh"
ok "temporary DOCKER_CONFIG from mktemp -d" grep -qE 'DOCKER_CONFIG=\$\(mktemp -d\)' "$tmp/runs.sh"
ok "trap removes it" grep -qE "trap 'rm -rf \"\\\$DOCKER_CONFIG\"' EXIT" "$tmp/runs.sh"
ok "no exec before the trap can fire" test "$(grep -cE '^[[:space:]]*exec ' "$tmp/runs.sh")" = 0
ok "mkdir -p of the work dir" grep -qF 'mkdir -p "$RUNNER_TEMP/ci"' "$tmp/runs.sh"

# the run: blocks stay short (at most 25 lines each)
longest=$(awk '/^#--block--$/ { if (n > m) m = n; n = 0; next } { n++ } END { print m + 0 }' "$tmp/runs.sh")
ok "a run: block longer than 25 lines ($longest)" test "$longest" -le 25

# header: the owner exception, the superseded specs, never self-hosted
head=$(grep '^#' "$wf")
for w in self-hosted 005 006 007; do
  case $head in *"$w"*) pass ;; *) fail "header comment does not mention $w" ;; esac
done

if [ "$failed" -ne 0 ]; then
  printf '%s of %s checks failed\n' "$failed" "$total"
  exit 1
fi
printf 'all %s checks passed\n' "$total"
