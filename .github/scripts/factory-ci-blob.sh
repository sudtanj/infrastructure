#!/usr/bin/env bash
# Matrix row blob (contracts/tick-workflow.md, FR-059). Sourced by the plan, job and status scripts.
# The key is FACTORY_CI_BLOB_KEY, read by openssl from the environment, never from argv. Both
# functions refuse a key that is not 64 lower-case hex characters (FR-036), so the job and status
# scripts inherit the check. `plan` sets the variable per row: the docker key for
# `allow_docker` rows, so a `run` job never holds the key of a docker row (R5).
#   fci_blob_encrypt   row JSON on stdin, one line of ciphertext on stdout
#   fci_blob_decrypt   ciphertext on stdin, row JSON on stdout; prints nothing and fails on a wrong key

# The message names the variable, never the value.
fci_blob_key_ok() {
  [[ ${FACTORY_CI_BLOB_KEY:-} =~ ^[0-9a-f]{64}$ ]] && return 0
  echo "FACTORY_CI_BLOB_KEY is not 64 lower-case hex characters" >&2
  return 1
}

fci_blob_encrypt() {
  fci_blob_key_ok || return 1
  openssl enc -aes-256-cbc -pbkdf2 -iter 600000 -salt -a -A -pass env:FACTORY_CI_BLOB_KEY
}

# CBC has no check: a wrong key can still decrypt to garbage, so only valid JSON is returned.
fci_blob_decrypt() {
  fci_blob_key_ok || return 1
  local plain
  plain=$(openssl enc -d -aes-256-cbc -pbkdf2 -iter 600000 -a -A -pass env:FACTORY_CI_BLOB_KEY 2> /dev/null) || return 1
  printf '%s' "$plain" | jq -e . > /dev/null 2>&1 || return 1
  printf '%s' "$plain"
}
