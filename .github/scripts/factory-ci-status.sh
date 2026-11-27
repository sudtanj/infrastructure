#!/usr/bin/env bash
# Status fallback (contracts/tick-workflow.md): when the job step did not succeed, post `error` for
# the item with the reason in $RUNNER_TEMP/fci-reason, or "job did not finish". It decrypts the row
# itself, so no step env: shows an item field. Prints nothing.
set +x
umask 077
export LC_ALL=C
here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=/dev/null
. "$here/factory-ci-blob.sh"
# shellcheck source=/dev/null
. "$here/factory-ci-secrets.sh"

runner=$(command -v rust-ci-runner) || exit 1
row=$(printf '%s' "${ITEM_BLOB:-}" | fci_blob_decrypt) || exit 1
field() { printf '%s' "$row" | jq -r --arg k "$1" '.[$k] // "" | tostring'; }
IFS= read -r why < "${RUNNER_TEMP:-/tmp}/fci-reason" 2> /dev/null || true
args=(status --repo "$(field repo)" --sha "$(field sha)" --project "$(field project)" --run-id "$(field run_id)"
  --state error --description "$(fci_clean "${why:-job did not finish}")")
[ "$(field retry)" != true ] || args+=(--retry)

unset ITEM_BLOB FACTORY_CI_BLOB_KEY row
exec "$runner" "${args[@]}"
