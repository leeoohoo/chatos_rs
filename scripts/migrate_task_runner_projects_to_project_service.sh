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

TASK_RUNNER_INTERNAL_BASE_URL="${TASK_RUNNER_INTERNAL_BASE_URL:-}"
CHATOS_TASK_RUNNER_INTERNAL_API_SECRET="${CHATOS_TASK_RUNNER_INTERNAL_API_SECRET:-}"
TASK_RUNNER_MTLS_CA_CERT_PATH="${TASK_RUNNER_MTLS_CA_CERT_PATH:-}"
TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH="${TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH:-}"
PROJECT_SERVICE_BASE_URL="${PROJECT_SERVICE_BASE_URL:-}"
CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET="${CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET:-}"
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

require_value() {
  local name="$1"
  local value="$2"
  if [[ -z "${value}" ]]; then
    echo "${name} is required" >&2
    exit 1
  fi
}

require_file() {
  local name="$1"
  local value="$2"
  require_value "${name}" "${value}"
  if [[ ! -f "${value}" ]]; then
    echo "${name} does not exist: ${value}" >&2
    exit 1
  fi
}

require_value "TASK_RUNNER_INTERNAL_BASE_URL" "${TASK_RUNNER_INTERNAL_BASE_URL}"
require_value "CHATOS_TASK_RUNNER_INTERNAL_API_SECRET" "${CHATOS_TASK_RUNNER_INTERNAL_API_SECRET}"
require_file "TASK_RUNNER_MTLS_CA_CERT_PATH" "${TASK_RUNNER_MTLS_CA_CERT_PATH}"
require_file "TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH" "${TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH}"
require_value "PROJECT_SERVICE_BASE_URL" "${PROJECT_SERVICE_BASE_URL}"
require_value "CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET" "${CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET}"

if [[ "${TASK_RUNNER_INTERNAL_BASE_URL}" != https://* ]]; then
  echo "TASK_RUNNER_INTERNAL_BASE_URL must use https" >&2
  exit 1
fi

issue_internal_token() {
  INTERNAL_TOKEN_SECRET="$1" \
    INTERNAL_TOKEN_CALLER="$2" \
    INTERNAL_TOKEN_AUDIENCE="$3" \
    INTERNAL_TOKEN_SCOPE="$4" \
    python3 - <<'PY'
import base64
import hashlib
import hmac
import json
import os
import time
import uuid

def encode(value):
    raw = json.dumps(value, separators=(",", ":")).encode("utf-8")
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")

now = int(time.time())
caller = os.environ["INTERNAL_TOKEN_CALLER"]
secret = os.environ["INTERNAL_TOKEN_SECRET"].encode("utf-8")
header = encode({"alg": "HS256", "typ": "JWT"})
payload = encode({
    "iss": caller,
    "sub": caller,
    "caller": caller,
    "aud": os.environ["INTERNAL_TOKEN_AUDIENCE"],
    "scope": os.environ["INTERNAL_TOKEN_SCOPE"],
    "trace_id": str(uuid.uuid4()),
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

task_runner_url="${TASK_RUNNER_INTERNAL_BASE_URL%/}/api/chatos-sync/projects"
project_service_url="${PROJECT_SERVICE_BASE_URL%/}/api/chatos-sync/projects"
if [[ -n "${PROJECT_STATUS}" ]]; then
  task_runner_url="${task_runner_url}?status=${PROJECT_STATUS}"
fi

tmp_file="$(mktemp)"
trap 'rm -f "${tmp_file}"' EXIT

echo "Fetching projects from ${task_runner_url}"
task_runner_token="$(issue_internal_token \
  "${CHATOS_TASK_RUNNER_INTERNAL_API_SECRET}" \
  "chatos-backend" \
  "task-runner" \
  "projects.sync")"
curl -fsS \
  --cacert "${TASK_RUNNER_MTLS_CA_CERT_PATH}" \
  --cert "${TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH}" \
  --key "${TASK_RUNNER_MTLS_CLIENT_IDENTITY_PATH}" \
  -H "X-Task-Runner-Caller: chatos-backend" \
  -H "X-Task-Runner-Internal-Token: ${task_runner_token}" \
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
  project_service_token="$(issue_internal_token \
    "${CHATOS_PROJECT_SERVICE_INTERNAL_API_SECRET}" \
    "chatos-backend" \
    "project-service" \
    "project.sync")"
  echo "Importing ${project_id} ${project_name}"
  curl -fsS \
    -X POST \
    -H "Content-Type: application/json" \
    -H "X-Project-Service-Caller: chatos-backend" \
    -H "X-Project-Service-Internal-Token: ${project_service_token}" \
    -d "${project}" \
    "${project_service_url}" >/dev/null
done

echo "Project migration complete"
