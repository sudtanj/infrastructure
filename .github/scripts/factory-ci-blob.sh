#!/usr/bin/env bash
# Matrix row blob (contracts/tick-workflow.md, FR-059). Sourced by the plan, job and status scripts.
# The key is FACTORY_CI_BLOB_KEY, read by openssl from the environment, never from argv.
# Callers refuse a key under 32 characters. `plan` sets the variable per row: the docker key for
# `allow_docker` rows, so a `run` job never holds the key of a docker row (R5).
#   fci_blob_encrypt   row JSON on stdin, one line of ciphertext on stdout
#   fci_blob_decrypt   ciphertext on stdin, row JSON on stdout; prints nothing and fails on a wrong key

fci_blob_encrypt() {
  openssl enc -aes-256-cbc -pbkdf2 -iter 600000 -salt -a -A -pass env:FACTORY_CI_BLOB_KEY
}

# CBC has no check: a wrong key can still decrypt to garbage, so only valid JSON is returned.
fci_blob_decrypt() {
  local plain
  plain=$(openssl enc -d -aes-256-cbc -pbkdf2 -iter 600000 -a -A -pass env:FACTORY_CI_BLOB_KEY 2> /dev/null) || return 1
  printf '%s' "$plain" | jq -e . > /dev/null 2>&1 || return 1
  printf '%s' "$plain"
}
