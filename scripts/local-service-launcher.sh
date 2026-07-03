#!/usr/bin/env bash
# Shared helpers for local dev service scripts.

local_service_use_launchd() {
  [[ "${CHATOS_LOCAL_USE_LAUNCHD:-auto}" != "0" ]] &&
    [[ "$(uname -s 2>/dev/null || true)" == "Darwin" ]] &&
    command -v launchctl >/dev/null 2>&1
}

local_service_label_part() {
  printf '%s' "$1" |
    tr '[:upper:]' '[:lower:]' |
    sed -E 's/[^a-z0-9]+/-/g; s/^-+//; s/-+$//'
}

local_service_launchd_label() {
  local prefix="$1"
  local name="$2"
  printf '%s-%s\n' "$prefix" "$(local_service_label_part "$name")"
}

local_service_stop_launchd_job() {
  local label="$1"
  if [[ -z "$label" ]] || ! command -v launchctl >/dev/null 2>&1; then
    return 0
  fi

  local uid target
  uid="$(id -u)"
  target="gui/${uid}/${label}"
  if launchctl print "$target" >/dev/null 2>&1 || launchctl list "$label" >/dev/null 2>&1; then
    echo "[INFO] removing launchd job: $label"
    launchctl bootout "$target" >/dev/null 2>&1 || launchctl remove "$label" >/dev/null 2>&1 || true
  fi
}

local_service_launch_with_launchd() {
  local label="$1"
  local name="$2"
  local log_file="$3"
  local pid_file="$4"
  local command="$5"
  local uid target pid attempt

  uid="$(id -u)"
  target="gui/${uid}/${label}"
  local_service_stop_launchd_job "$label"

  launchctl submit -l "$label" -- /bin/bash -lc "exec >>\"$log_file\" 2>&1; $command"

  for attempt in 1 2 3 4 5 6 7 8 9 10; do
    pid="$(launchctl print "$target" 2>/dev/null | awk '/pid =/ { print $3; exit }' || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" >/dev/null 2>&1; then
      echo "$pid" >"$pid_file"
      return 0
    fi
    sleep 1
  done

  echo "[ERROR] $name launchd job did not expose a running pid: $label"
  launchctl print "$target" 2>/dev/null || true
  return 1
}
