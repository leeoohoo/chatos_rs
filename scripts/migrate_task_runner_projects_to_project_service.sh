#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -f "${ROOT_DIR}/.env" ]]; then
  set -a
  # shellcheck disable=SC1091
  source "${ROOT_DIR}/.env"
  set +a
fi

TASK_RUNNER_BASE_URL="${TASK_RUNNER_BASE_URL:-${CHATOS_TASK_RUNNER_BASE_URL:-http://127.0.0.1:39090}}"
TASK_RUNNER_SYNC_SECRET="${TASK_RUNNER_SYNC_SECRET:-${TASK_RUNNER_CHATOS_CALLBACK_SECRET:-${CHATOS_TASK_RUNNER_CALLBACK_SECRET:-}}}"
PROJECT_SERVICE_BASE_URL="${PROJECT_SERVICE_BASE_URL:-http://127.0.0.1:39210}"
PROJECT_SERVICE_INTERNAL_API_SECRET="${PROJECT_SERVICE_INTERNAL_API_SECRET:-${CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET:-${PROJECT_SERVICE_SYNC_SECRET:-}}}"
PROJECT_SERVICE_CALLER="${PROJECT_SERVICE_CALLER:-chatos-backend}"
PROJECT_STATUS="${PROJECT_STATUS:-}"
DRY_RUN="${DRY_RUN:-0}"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required" >&2
  exit 1
fi

if [[ -z "${TASK_RUNNER_SYNC_SECRET}" ]]; then
  echo "TASK_RUNNER_SYNC_SECRET or TASK_RUNNER_CHATOS_CALLBACK_SECRET is required" >&2
  exit 1
fi

if [[ -z "${PROJECT_SERVICE_INTERNAL_API_SECRET}" ]]; then
  echo "PROJECT_SERVICE_INTERNAL_API_SECRET, CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET, or PROJECT_SERVICE_SYNC_SECRET is required" >&2
  exit 1
fi

issue_project_service_token() {
  PROJECT_SERVICE_INTERNAL_API_SECRET="${PROJECT_SERVICE_INTERNAL_API_SECRET}" \
    PROJECT_SERVICE_CALLER="${PROJECT_SERVICE_CALLER}" \
    python3 - <<'PY'
import base64
import hashlib
import hmac
import json
import os
import time

def encode(value):
    raw = json.dumps(value, separators=(",", ":")).encode("utf-8")
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")

now = int(time.time())
caller = os.environ["PROJECT_SERVICE_CALLER"]
secret = os.environ["PROJECT_SERVICE_INTERNAL_API_SECRET"].encode("utf-8")
header = encode({"alg": "HS256", "typ": "JWT"})
payload = encode({
    "iss": caller,
    "sub": caller,
    "aud": "project-service",
    "scope": "project.sync",
    "iat": now,
    "exp": now + 60,
})
signing_input = f"{header}.{payload}"
signature = base64.urlsafe_b64encode(
    hmac.new(secret, signing_input.encode("ascii"), hashlib.sha256).digest()
).rstrip(b"=").decode("ascii")
print(f"{signing_input}.{signature}", end="")
PY
}

task_runner_url="${TASK_RUNNER_BASE_URL%/}/api/chatos-sync/projects"
project_service_url="${PROJECT_SERVICE_BASE_URL%/}/api/chatos-sync/projects"
if [[ -n "${PROJECT_STATUS}" ]]; then
  task_runner_url="${task_runner_url}?status=${PROJECT_STATUS}"
fi

tmp_file="$(mktemp)"
trap 'rm -f "${tmp_file}"' EXIT

echo "Fetching projects from ${task_runner_url}"
curl -fsS \
  -H "X-Chatos-Callback-Secret: ${TASK_RUNNER_SYNC_SECRET}" \
  "${task_runner_url}" > "${tmp_file}"

count="$(jq 'length' "${tmp_file}")"
echo "Found ${count} project(s)"

if [[ "${DRY_RUN}" == "1" || "${DRY_RUN}" == "true" ]]; then
  jq -r '.[] | "\(.id)\t\(.name)\t\(.owner_user_id // "-")\t\(.status // "-")"' "${tmp_file}"
  exit 0
fi

jq -c '.[]' "${tmp_file}" | while IFS= read -r project; do
  project_id="$(jq -r '.id' <<<"${project}")"
  project_name="$(jq -r '.name' <<<"${project}")"
  project_service_token="$(issue_project_service_token)"
  echo "Importing ${project_id} ${project_name}"
  curl -fsS \
    -X POST \
    -H "Content-Type: application/json" \
    -H "X-Project-Service-Caller: ${PROJECT_SERVICE_CALLER}" \
    -H "X-Project-Service-Internal-Token: ${project_service_token}" \
    -d "${project}" \
    "${project_service_url}" >/dev/null
done

echo "Project migration complete"
