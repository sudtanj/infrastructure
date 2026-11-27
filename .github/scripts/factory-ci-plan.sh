#!/usr/bin/env bash
# The plan step (contracts/tick-workflow.md, plan-output.md; FR-045, FR-059). Run from this repo's checkout.
#   A  rust-ci-runner plan --repos ... --rows-out CAND     GitHub reads only
#   between A and C, only when a row can use a secret: check the store, mask the log credentials from the environment
#   C  rust-ci-runner plan --rows-in CAND ...               refusals, cap, posts
# then each final row is encrypted and only {n, blob}, counts and fail_tick reach $GITHUB_OUTPUT.
# A failure appends its reason to $GITHUB_STEP_SUMMARY, writes no output and exits 1. An idle tick
# makes no store call.
set +x
umask 077
export LC_ALL=C
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=/dev/null
. "$here/factory-ci-blob.sh"
# shellcheck source=/dev/null
. "$here/factory-ci-secrets.sh"

: "${GITHUB_OUTPUT:?}"
tmp=${RUNNER_TEMP:-/tmp}
cand=$tmp/fci-cand.json
rows=$tmp/fci-rows.json
summary=$tmp/fci-summary.md
rm -f "$cand" "$rows" "$summary"

fail() {
  [ -z "${GITHUB_STEP_SUMMARY:-}" ] || printf '%s\n' "$(fci_clean "$1")" >> "$GITHUB_STEP_SUMMARY"
  rm -f "$cand" "$rows" "$summary"
  exit 1
}

# R6, FR-036: a weak key makes the blob guessable offline; the tick guard step checks it too.
[[ $FACTORY_CI_BLOB_KEY =~ ^[0-9a-f]{64}$ ]] || fail "FACTORY_CI_BLOB_KEY is not 64 lower-case hex characters"
case ${FACTORY_CI_BWS_SERVER_URL:-} in ''|https://*) ;; *) fail "FACTORY_CI_BWS_SERVER_URL must start with https://" ;; esac
fci_log_env || fail "$FCI_ERR"
runner=$(command -v rust-ci-runner) || fail "rust-ci-runner not found"
# The runner never needs the store tokens or the blob key.
run() {
  env -u BWS_ACCESS_TOKEN -u BWS_ACCESS_TOKEN_DOCKER -u FACTORY_CI_BLOB_KEY -u FACTORY_CI_BLOB_KEY_DOCKER "$runner" "$@"
}

# Phase A reads GitHub only, so it runs without the log credentials (FR-037); phase C keeps them.
( unset FACTORY_CI_LOG_URL FACTORY_CI_LOG_USER FACTORY_CI_LOG_TOKEN; run plan --repos .github/factory-ci-repos.toml --rows-out "$cand" ) || fail "plan failed before any status was posted"
total=$(jq '.rows | length' "$cand") || fail "plan wrote an unreadable candidate file"

if [ "$total" -gt 0 ]; then
  plain=$(jq '[.rows[] | select(.allow_docker | not)] | length' "$cand")
  docker=$((total - plain))
  # A job reaches a store only through shared names or an owner-scoped key, and only when secrets_ok (FR-038).
  can_use='[.rows[] | select(.secrets_ok == true and ((.secrets // "") != "" or (.repo_id // "") != ""))]'
  plain_use=$(jq "$can_use | map(select(.allow_docker | not)) | length" "$cand")
  docker_use=$(jq "$can_use | map(select(.allow_docker)) | length" "$cand")
  [ "$docker" -eq 0 ] || [[ ${FACTORY_CI_BLOB_KEY_DOCKER:-} =~ ^[0-9a-f]{64}$ ]] ||
    fail "FACTORY_CI_BLOB_KEY_DOCKER is not 64 lower-case hex characters"
  if [ "$plain_use" -gt 0 ] && [ -n "${BWS_ACCESS_TOKEN:-}" ]; then
    fci_list "${FACTORY_CI_BWS_PROJECT:-}" "$BWS_ACCESS_TOKEN" || fail "$FCI_ERR"
  fi
  if [ "$docker_use" -gt 0 ] && [ -n "${BWS_ACCESS_TOKEN_DOCKER:-}" ]; then
    fci_list "${FACTORY_CI_BWS_PROJECT_DOCKER:-}" "$BWS_ACCESS_TOKEN_DOCKER" || fail "$FCI_ERR"
  fi
  if [ -n "$FCI_HAVE_LOGS" ]; then
    for v in "$FCI_LOG_URL" "$FCI_LOG_USER" "$FCI_LOG_TOKEN"; do
      ! fci_short_line "$v" || fail "log credentials have a line under 4 characters"
      fci_mask "$v"
    done
  fi
fi

out=$(run plan --rows-in "$cand" --rows-out "$rows" --summary "$summary") || fail "plan failed while posting statuses"
[ -z "${GITHUB_STEP_SUMMARY:-}" ] || cat "$summary" >> "$GITHUB_STEP_SUMMARY"
case $out in
  fail_tick=0) fail_tick=0 ;;
  fail_tick=1) fail_tick=1 ;;
  *) fail "plan gave no fail_tick" ;;
esac

plain_items=()
docker_items=()
while IFS= read -r row; do
  n=$(printf '%s' "$row" | jq -r '.n')
  is_docker=$(printf '%s' "$row" | jq -r '.allow_docker')
  key=$FACTORY_CI_BLOB_KEY
  [ "$is_docker" != true ] || key=${FACTORY_CI_BLOB_KEY_DOCKER:-}
  blob=$(printf '%s' "$row" | FACTORY_CI_BLOB_KEY=$key fci_blob_encrypt) || fail "could not encrypt a row"
  item=$(jq -cn --argjson n "$n" --arg blob "$blob" '{n: $n, blob: $blob}')
  if [ "$is_docker" = true ]; then
    docker_items+=("$item")
  else
    plain_items+=("$item")
  fi
done < <(jq -c '.[]' "$rows")
rm -f "$cand" "$rows" "$summary"

matrix() { if [ "$#" -eq 0 ]; then echo '{"include":[]}'; else printf '%s\n' "$@" | jq -sc '{include: .}'; fi; }
{
  echo "matrix=$(matrix "${plain_items[@]}")"
  echo "matrix_docker=$(matrix "${docker_items[@]}")"
  echo "count=${#plain_items[@]}"
  echo "count_docker=${#docker_items[@]}"
  echo "fail_tick=$fail_tick"
} >> "$GITHUB_OUTPUT"
