#!/usr/bin/env bash
# One matrix item (research.md D9, D12; contracts/tick-workflow.md). Decrypts the row, has the runner
# list the pipeline's secret names (prepare, under `env -i`), fetches them from the store, then
# `exec`s the real run. Prints nothing but `::add-mask::` lines. A failure writes one sanitized
# reason line to $RUNNER_TEMP/fci-reason (the status fallback posts it) and exits 1.
set +x
umask 077
export LC_ALL=C
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=/dev/null
. "$here/factory-ci-blob.sh"
# shellcheck source=/dev/null
. "$here/factory-ci-secrets.sh"

tmp=${RUNNER_TEMP:-/tmp}
reason_file=$tmp/fci-reason
names_file=$tmp/fci-names
prep_reason=$tmp/fci-prep-reason
rm -f "$reason_file" "$names_file" "$prep_reason"

# The public summary names the item number only: a reason can hold a step or secret name (FR-059).
refuse() {
  fci_clean "$1" > "$reason_file"
  printf '\n' >> "$reason_file"
  [ -z "${GITHUB_STEP_SUMMARY:-}" ] || printf 'item %s: did not run, see the commit status\n' "${n:-?}" >> "$GITHUB_STEP_SUMMARY"
  rm -f "$names_file" "$prep_reason"
  exit 1
}

fci_log_env || refuse "$FCI_ERR"
case ${FACTORY_CI_BWS_SERVER_URL:-} in ''|https://*) ;; *) refuse "FACTORY_CI_BWS_SERVER_URL must start with https://" ;; esac
runner=$(command -v rust-ci-runner) || refuse "job did not finish"
row=$(printf '%s' "${ITEM_BLOB:-}" | fci_blob_decrypt) || refuse "job did not finish"
field() { printf '%s' "$row" | jq -r --arg k "$1" '.[$k] // "" | tostring'; }
n=$(field n)
repo=$(field repo) sha=$(field sha) branch=$(field branch) project=$(field project)
run_id=$(field run_id) prev_sha=$(field prev_sha) repo_id=$(field repo_id) shared=$(field secrets)
allow_docker=$(field allow_docker) fail_closed=$(field fail_closed) retry=$(field retry) secrets_ok=$(field secrets_ok)
unset row

case $FCI_KEY_SEP in /|:) ;; *) refuse "invalid key separator" ;; esac

# Before any container starts: step containers must not reach the cloud metadata services (FR-054).
for ip in 169.254.169.254 168.63.129.16; do
  sudo iptables -I DOCKER-USER -d "$ip" -j DROP || refuse "network block failed"
done

# Prepare: clone, run every pre-container check, list the secret names. It holds only the clone token.
pre=(env -i "PATH=$PATH" "FACTORY_CI_TOKEN=${FACTORY_CI_TOKEN:-}"
  "FACTORY_CI_CLONE_IMAGE=${FACTORY_CI_CLONE_IMAGE:-}" "FACTORY_CI_DIND_IMAGE=${FACTORY_CI_DIND_IMAGE:-}")
for v in RUNNER_TEMP GITHUB_SERVER_URL GITHUB_API_URL FACTORY_CI_ENGINE; do
  [ -z "${!v:-}" ] || pre+=("$v=${!v}")
done
args=(job --repo "$repo" --sha "$sha" --project "$project" --branch "$branch" --run-id "$run_id")
[ -z "$prev_sha" ] || args+=(--prev "$prev_sha")
[ "$allow_docker" != true ] || args+=(--allow-docker)
if ! "${pre[@]}" "$runner" "${args[@]}" --names-file "$names_file" --reason-file "$prep_reason" > /dev/null 2>&1; then
  IFS= read -r why < "$prep_reason" 2> /dev/null || true
  refuse "${why:-job did not finish}"
fi

names=()
while IFS= read -r line || [ -n "$line" ]; do
  [ -n "$line" ] || continue
  [[ $line =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] || refuse "invalid secret name in the pipeline"
  names+=("$line")
done < "$names_file"
rm -f "$names_file" "$prep_reason"

if [ "${#names[@]}" -gt 0 ]; then
  [ "$secrets_ok" = true ] || refuse "secrets not allowed on this branch"
  [ -n "${BWS_ACCESS_TOKEN:-}" ] || refuse "no secret store configured"
fi

# One list call resolves the names. Nothing is exported before all resolved.
declare -A vals=()
if [ "${#names[@]}" -gt 0 ]; then
  fci_list "${FACTORY_CI_BWS_PROJECT:-}" "$BWS_ACCESS_TOKEN" || refuse "$FCI_ERR"
  FCI_REPO=$repo FCI_REPO_ID=$repo_id FCI_SHARED=$shared
  for name in "${names[@]}"; do
    fci_lookup "$name"
    case $? in
      0) fci_short_line "$FCI_VALUE" && refuse "secret $name has a line under 4 characters"
         vals[$name]=$FCI_VALUE ;;
      2) refuse "secret $name is ambiguous" ;;
      *) refuse "secret $name not found" ;;
    esac
  done
fi
if [ -n "$FCI_HAVE_LOGS" ]; then
  for v in "$FCI_LOG_URL" "$FCI_LOG_USER" "$FCI_LOG_TOKEN"; do
    ! fci_short_line "$v" || refuse "log credentials have a line under 4 characters"
  done
fi
if [ "$fail_closed" = true ] && [ -z "${FCI_HAVE_LOGS:-}" ]; then
  refuse "private repository and no log store"
fi

for name in "${names[@]}"; do
  fci_mask "${vals[$name]}"
  export "$name=${vals[$name]}"
done
if [ -n "${FCI_HAVE_LOGS:-}" ]; then
  fci_mask "$FCI_LOG_URL"
  fci_mask "$FCI_LOG_USER"
  fci_mask "$FCI_LOG_TOKEN"
  export FACTORY_CI_LOG_URL=$FCI_LOG_URL FACTORY_CI_LOG_USER=$FCI_LOG_USER FACTORY_CI_LOG_TOKEN=$FCI_LOG_TOKEN
  args+=(--quiet-logs)
fi
# The clone sends the clone token as a Basic header; mask its base64 form too (FR-053).
fci_mask_line "$(printf 'x-access-token:%s' "${FACTORY_CI_TOKEN:-}" | base64 | tr -d '\n')"
[ "$fail_closed" != true ] || args+=(--private)
[ "$retry" != true ] || args+=(--retry)

# The real run gets the item's names and log credentials, not the store token or the blob key.
unset BWS_ACCESS_TOKEN FACTORY_CI_BLOB_KEY ITEM_BLOB HAVE_BWS FACTORY_CI_BWS_PROJECT FACTORY_CI_BWS_SERVER_URL
unset FCI_JSON FCI_VALUE FCI_LOG_URL FCI_LOG_USER FCI_LOG_TOKEN vals
exec "$runner" "${args[@]}"
