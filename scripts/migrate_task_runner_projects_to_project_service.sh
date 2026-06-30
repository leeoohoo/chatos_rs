#!/usr/bin/env bash
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
PROJECT_SERVICE_SYNC_SECRET="${PROJECT_SERVICE_SYNC_SECRET:-}"
PROJECT_STATUS="${PROJECT_STATUS:-}"
DRY_RUN="${DRY_RUN:-0}"

if ! command -v jq >/dev/null 2>&1; then
  echo "jq is required" >&2
  exit 1
fi

if [[ -z "${TASK_RUNNER_SYNC_SECRET}" ]]; then
  echo "TASK_RUNNER_SYNC_SECRET or TASK_RUNNER_CHATOS_CALLBACK_SECRET is required" >&2
  exit 1
fi

if [[ -z "${PROJECT_SERVICE_SYNC_SECRET}" ]]; then
  echo "PROJECT_SERVICE_SYNC_SECRET is required" >&2
  exit 1
fi

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
  echo "Importing ${project_id} ${project_name}"
  curl -fsS \
    -X POST \
    -H "Content-Type: application/json" \
    -H "X-Project-Service-Sync-Secret: ${PROJECT_SERVICE_SYNC_SECRET}" \
    -d "${project}" \
    "${project_service_url}" >/dev/null
done

echo "Project migration complete"
