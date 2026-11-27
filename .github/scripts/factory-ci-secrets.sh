#!/usr/bin/env bash
# Secret fetch library (research.md D9, D12; FR-039 to FR-046, FR-053). Sourced by the plan and job
# scripts. Never `set -x`, never prints a value, a token or bws stderr. Bitwarden Secrets Manager is
# the only store; one project per list call.
#
# SHA-256 of bws-x86_64-unknown-linux-musl-2.1.0.zip, from the bws-v2.1.0 release of bitwarden/sdk-sm.
# The workflow's install step compares the download with it.
BWS_SHA256=f59ee150e42b82128d437087e9bac920053c6bfddcb960d20ce9386e5ac9bba6

# Key separator: `/`, or `:` if the store refuses `/`. Never `__`, it can collide (A24).
FCI_KEY_SEP=${FCI_KEY_SEP:-/}

# Text safe for a commit status and a summary: one line, [A-Za-z0-9 _.:/-], 140 characters, no
# leading `::` (FR-051).
fci_clean() {
  local line s
  IFS= read -r line <<< "$1" || true
  s=$(printf '%s' "$line" | LC_ALL=C tr -c 'A-Za-z0-9 _.:/-' '_' | cut -c1-140)
  case $s in ::*) s=_$s ;; esac
  printf '%s' "$s"
}

# fci_list PROJECT TOKEN: one `bws secret list`, JSON kept in FCI_JSON (a shell variable, never argv).
# 401 or 403 on stderr: no retry. Anything else is retried until FACTORY_CI_FETCH_WINDOW seconds
# (default 60) have passed. On failure FCI_ERR holds the reason.
fci_list() {
  local project=$1 token=$2 errf out delay=1 deadline=$((SECONDS + ${FACTORY_CI_FETCH_WINDOW:-60}))
  FCI_JSON=
  FCI_ERR=
  [ -n "$project" ] || { FCI_ERR="no secret project configured"; return 1; }
  command -v bws > /dev/null 2>&1 || { FCI_ERR="secret store unreachable"; return 1; }
  errf=$(mktemp "${RUNNER_TEMP:-/tmp}/fci-err.XXXXXX") || { FCI_ERR="secret store unreachable"; return 1; }
  while :; do
    if out=$(BWS_ACCESS_TOKEN=$token timeout 30 bws secret list "$project" -o json 2> "$errf") \
      && printf '%s' "$out" | jq -e 'type == "array"' > /dev/null 2>&1; then
      FCI_JSON=$out
      rm -f "$errf"
      return 0
    fi
    if grep -Eiq '(^|[^0-9])40[13]([^0-9]|$)|unauthori[sz]ed|forbidden|invalid access token' "$errf"; then
      FCI_ERR="secret store rejected the token"
      break
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
      FCI_ERR="secret store unreachable"
      break
    fi
    sleep "$delay"
    [ "$delay" -ge 8 ] || delay=$((delay * 2))
  done
  rm -f "$errf"
  return 1
}

# fci_key_value KEY: exact-case lookup in FCI_JSON. 0 found (value in FCI_VALUE), 1 no match or an
# empty value, 2 more than one key.
fci_key_value() {
  local n
  FCI_VALUE=
  n=$(printf '%s' "$FCI_JSON" | jq -r --arg k "$1" '[.[] | select(.key == $k)] | length') || return 1
  [ "$n" -le 1 ] || return 2
  [ "$n" -eq 1 ] || return 1
  FCI_VALUE=$(printf '%s' "$FCI_JSON" | jq -r --arg k "$1" '.[] | select(.key == $k) | .value // ""')
  [ -n "$FCI_VALUE" ]
}

# fci_lookup NAME: sources in the order of FR-040, first match wins. Needs FCI_REPO, FCI_REPO_ID (may
# be empty) and FCI_SHARED (space-separated shared names). Returns as fci_key_value.
fci_lookup() {
  local name=$1 key keys=() shared w rc=1
  [ -z "$FCI_REPO_ID" ] || keys+=("$FCI_REPO$FCI_KEY_SEP$name")
  read -ra shared <<< "$FCI_SHARED"
  for w in "${shared[@]}"; do
    [ "$w" != "$name" ] || keys+=("$name")
  done
  for key in "${keys[@]}"; do
    fci_key_value "$key"
    rc=$?
    [ "$rc" -eq 1 ] || return "$rc"
  done
  return 1
}

# fci_log_creds: the three log keys of the default project, all or none. Sets FCI_LOG_URL,
# FCI_LOG_USER, FCI_LOG_TOKEN and FCI_HAVE_LOGS=1, or leaves FCI_HAVE_LOGS empty. On failure FCI_ERR.
fci_log_creds() {
  local k found=0 vals=()
  FCI_HAVE_LOGS=
  for k in URL USER TOKEN; do
    fci_key_value "FACTORY_CI_LOG_$k"
    case $? in
      0) found=$((found + 1)); vals+=("$FCI_VALUE") ;;
      2) FCI_ERR="secret FACTORY_CI_LOG_$k is ambiguous"; return 1 ;;
      *) vals+=("") ;;
    esac
  done
  [ "$found" -eq 0 ] && return 0
  [ "$found" -eq 3 ] || { FCI_ERR="log credentials incomplete"; return 1; }
  FCI_LOG_URL=${vals[0]} FCI_LOG_USER=${vals[1]} FCI_LOG_TOKEN=${vals[2]} FCI_HAVE_LOGS=1
}

# fci_short_line VALUE: 0 when a trimmed line is under 4 characters (FR-053).
fci_short_line() {
  local l
  while IFS= read -r l; do
    l=${l#"${l%%[![:space:]]*}"}
    l=${l%"${l##*[![:space:]]}"}
    [ "${#l}" -ge 4 ] || return 0
  done <<< "$1"
  return 1
}

# fci_mask VALUE: `::add-mask::` for each line of 4 or more characters, then for the base64 of the
# whole value. `%` and CR are escaped as the workflow command syntax needs.
fci_mask() {
  local l
  while IFS= read -r l; do
    [ "${#l}" -lt 4 ] || fci_mask_line "$l"
  done <<< "$1"
  fci_mask_line "$(printf '%s' "$1" | base64 | tr -d '\n')"
}

fci_mask_line() {
  local l=${1//%/%25}
  printf '::add-mask::%s\n' "${l//$'\r'/%0D}"
}
