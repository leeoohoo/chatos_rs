#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

HARNESS_BASE_URL="${HARNESS_BASE_URL:-http://localhost:3000}"
HARNESS_ADMIN_LOGIN="${HARNESS_ADMIN_LOGIN:-admin}"
HARNESS_ADMIN_PASSWORD="${HARNESS_ADMIN_PASSWORD:-admin123456}"
HARNESS_CI_SPACE="${HARNESS_CI_SPACE:-chatos-ci}"
HARNESS_CI_REPO="${HARNESS_CI_REPO:-chatos-rs}"
HARNESS_CI_PIPELINE="${HARNESS_CI_PIPELINE:-chatos-rs}"
HARNESS_CI_CONFIG_PATH="${HARNESS_CI_CONFIG_PATH:-.drone.yml}"
HARNESS_CI_RUN="${HARNESS_CI_RUN:-true}"
HARNESS_CI_FORCE_PUSH="${HARNESS_CI_FORCE_PUSH:-true}"
HARNESS_CI_PAT_IDENTIFIER="${HARNESS_CI_PAT_IDENTIFIER:-chatos-ci-$(date +%Y%m%d%H%M%S)}"
HARNESS_CI_BRANCH="${HARNESS_CI_BRANCH:-$(git -C "$ROOT_DIR" branch --show-current 2>/dev/null || true)}"
HARNESS_GIT_BASE_URL="${HARNESS_GIT_BASE_URL:-}"
HARNESS_CI_SNAPSHOT_SCOPE="${HARNESS_CI_SNAPSHOT_SCOPE:-ci-files}"
HARNESS_CI_IMAGE_SERVICES="${HARNESS_CI_IMAGE_SERVICES:-${CHATOS_CI_IMAGE_SERVICES:-}}"
HARNESS_CI_IMAGE_SERVICES_FILE="docker/.harness-ci-image-services"
HARNESS_CI_REGISTER_IMAGE_SERVICE_PIPELINES="${HARNESS_CI_REGISTER_IMAGE_SERVICE_PIPELINES:-false}"
HARNESS_CI_IMAGE_PIPELINE_DIR="${HARNESS_CI_IMAGE_PIPELINE_DIR:-.harness/pipelines/images}"
HARNESS_CI_IMAGE_PIPELINE_PREFIX="${HARNESS_CI_IMAGE_PIPELINE_PREFIX:-image-}"

if [[ -z "${HARNESS_CI_BRANCH// }" ]]; then
  HARNESS_CI_BRANCH="main"
fi

if [[ ! -f "$ROOT_DIR/$HARNESS_CI_CONFIG_PATH" ]]; then
  echo "[ERROR] Missing pipeline config: $HARNESS_CI_CONFIG_PATH" >&2
  exit 1
fi

case "$HARNESS_CI_SNAPSHOT_SCOPE" in
  ci-files|all)
    ;;
  *)
    echo "[ERROR] unsupported HARNESS_CI_SNAPSHOT_SCOPE=$HARNESS_CI_SNAPSHOT_SCOPE" >&2
    echo "        expected: ci-files or all" >&2
    exit 2
    ;;
esac

ci_paths=(
  ".gitignore"
  ".drone.yml"
  ".drone.images.yml"
  ".harness/pipelines"
  "docker/compose.yml"
  "docker/.env.example"
  "docker/deploy-harness-ci.sh"
  "docs/HARNESS_CI.md"
  "scripts/bootstrap_harness_ci.sh"
  "scripts/generate_harness_image_pipelines.sh"
  "scripts/harness_ci_build_images.sh"
  "scripts/local-dev-stack.sh"
  "scripts/check_openapi_method_contract_gate.sh"
)

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT

json_get() {
  local file="$1"
  local path="$2"
  python3 - "$file" "$path" <<'PY'
import json
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    value = json.load(fh)

for part in sys.argv[2].split("."):
    if not part:
        continue
    if isinstance(value, dict):
        value = value.get(part, "")
    else:
        value = ""
        break

if value is None:
    value = ""
print(value)
PY
}

url_encode() {
  python3 - "$1" <<'PY'
import sys
from urllib.parse import quote

print(quote(sys.argv[1], safe=""))
PY
}

request_json() {
  local method="$1"
  local url="$2"
  local token="$3"
  local body_file="$4"
  local out_file="$5"
  local status
  local args=(-sS -o "$out_file" -w "%{http_code}" -X "$method" "$url")

  if [[ -n "$token" ]]; then
    args+=(-H "Authorization: Bearer $token")
  fi
  if [[ -n "$body_file" ]]; then
    args+=(-H "Content-Type: application/json" --data-binary "@$body_file")
  fi

  status="$(curl "${args[@]}")"
  printf '%s' "$status"
}

request_ok() {
  local status="$1"
  [[ "$status" =~ ^2[0-9][0-9]$ ]]
}

body_mentions_exists() {
  local file="$1"
  python3 - "$file" <<'PY'
import sys

text = open(sys.argv[1], "r", encoding="utf-8", errors="replace").read().lower()
needles = ["already exists", "already exist", "duplicate", "conflict", "exists"]
sys.exit(0 if any(item in text for item in needles) else 1)
PY
}

json_body() {
  local out="$1"
  shift
  python3 - "$out" "$@" <<'PY'
import json
import sys

out = sys.argv[1]
pairs = sys.argv[2:]
body = {}
for pair in pairs:
    key, raw = pair.split("=", 1)
    if raw == "true":
        value = True
    elif raw == "false":
        value = False
    else:
        value = raw
    body[key] = value

with open(out, "w", encoding="utf-8") as fh:
    json.dump(body, fh)
PY
}

login_file="$tmp_dir/login.json"
json_body "$tmp_dir/login-body.json" \
  "login_identifier=$HARNESS_ADMIN_LOGIN" \
  "password=$HARNESS_ADMIN_PASSWORD"
login_status="$(request_json POST "$HARNESS_BASE_URL/api/v1/login" "" "$tmp_dir/login-body.json" "$login_file")"
if ! request_ok "$login_status" && [[ "$login_status" == "404" ]]; then
  login_status="$(request_json POST "$HARNESS_BASE_URL/api/v1/user/login" "" "$tmp_dir/login-body.json" "$login_file")"
fi
if ! request_ok "$login_status"; then
  echo "[ERROR] Harness login failed: HTTP $login_status" >&2
  cat "$login_file" >&2 || true
  exit 1
fi

admin_token="$(json_get "$login_file" access_token)"
if [[ -z "$admin_token" ]]; then
  echo "[ERROR] Harness login response did not include access_token" >&2
  exit 1
fi

user_file="$tmp_dir/user.json"
user_status="$(request_json GET "$HARNESS_BASE_URL/api/v1/user" "$admin_token" "" "$user_file")"
if ! request_ok "$user_status"; then
  echo "[ERROR] Failed to fetch Harness user: HTTP $user_status" >&2
  cat "$user_file" >&2 || true
  exit 1
fi
push_username="$(json_get "$user_file" uid)"
if [[ -z "$push_username" ]]; then
  push_username="$HARNESS_ADMIN_LOGIN"
fi

json_body "$tmp_dir/space-body.json" \
  "identifier=$HARNESS_CI_SPACE" \
  "parent_ref=" \
  "description=Chat OS CI workspace" \
  "is_public=false"
space_file="$tmp_dir/space.json"
space_status="$(request_json POST "$HARNESS_BASE_URL/api/v1/spaces" "$admin_token" "$tmp_dir/space-body.json" "$space_file")"
if request_ok "$space_status"; then
  echo "[OK] Created Harness space: $HARNESS_CI_SPACE"
elif body_mentions_exists "$space_file"; then
  echo "[OK] Harness space already exists: $HARNESS_CI_SPACE"
else
  echo "[ERROR] Create Harness space failed: HTTP $space_status" >&2
  cat "$space_file" >&2 || true
  exit 1
fi

json_body "$tmp_dir/repo-body.json" \
  "parent_ref=$HARNESS_CI_SPACE" \
  "identifier=$HARNESS_CI_REPO" \
  "default_branch=$HARNESS_CI_BRANCH" \
  "description=Chat OS CI mirror" \
  "is_public=false" \
  "readme=false"
repo_create_file="$tmp_dir/repo-create.json"
repo_create_status="$(request_json POST "$HARNESS_BASE_URL/api/v1/repos" "$admin_token" "$tmp_dir/repo-body.json" "$repo_create_file")"
if request_ok "$repo_create_status"; then
  echo "[OK] Created Harness repo: $HARNESS_CI_SPACE/$HARNESS_CI_REPO"
elif body_mentions_exists "$repo_create_file"; then
  echo "[OK] Harness repo already exists: $HARNESS_CI_SPACE/$HARNESS_CI_REPO"
else
  echo "[ERROR] Create Harness repo failed: HTTP $repo_create_status" >&2
  cat "$repo_create_file" >&2 || true
  exit 1
fi

repo_ref="$HARNESS_CI_SPACE/$HARNESS_CI_REPO"
repo_api_ref="$repo_ref/+"
repo_file="$tmp_dir/repo.json"
repo_status="$(request_json GET "$HARNESS_BASE_URL/api/v1/repos/$repo_api_ref" "$admin_token" "" "$repo_file")"
if ! request_ok "$repo_status"; then
  echo "[ERROR] Fetch Harness repo failed: HTTP $repo_status" >&2
  cat "$repo_file" >&2 || true
  exit 1
fi

git_url="$(json_get "$repo_file" git_url)"
if [[ -z "$git_url" ]]; then
  echo "[ERROR] Harness repo response did not include git_url" >&2
  cat "$repo_file" >&2 || true
  exit 1
fi
if [[ -n "$HARNESS_GIT_BASE_URL" ]]; then
  git_url="${HARNESS_GIT_BASE_URL%/}/$repo_ref.git"
elif [[ "$git_url" == "${HARNESS_BASE_URL%/}/"* && "$git_url" != "${HARNESS_BASE_URL%/}/git/"* ]]; then
  git_url="${HARNESS_BASE_URL%/}/git/$repo_ref.git"
fi

json_body "$tmp_dir/token-body.json" "identifier=$HARNESS_CI_PAT_IDENTIFIER"
pat_file="$tmp_dir/pat.json"
pat_status="$(request_json POST "$HARNESS_BASE_URL/api/v1/user/tokens" "$admin_token" "$tmp_dir/token-body.json" "$pat_file")"
if ! request_ok "$pat_status"; then
  echo "[ERROR] Create Harness PAT failed: HTTP $pat_status" >&2
  cat "$pat_file" >&2 || true
  exit 1
fi
push_token="$(json_get "$pat_file" access_token)"
if [[ -z "$push_token" ]]; then
  echo "[ERROR] Harness PAT response did not include access_token" >&2
  exit 1
fi

snapshot_needed=false
if [[ -n "${HARNESS_CI_IMAGE_SERVICES// }" ]]; then
  snapshot_needed=true
fi
if [[ "$HARNESS_CI_SNAPSHOT_SCOPE" == "all" ]]; then
  if ! git -C "$ROOT_DIR" diff --quiet ||
    ! git -C "$ROOT_DIR" diff --cached --quiet ||
    [[ -n "$(git -C "$ROOT_DIR" ls-files --others --exclude-standard)" ]]; then
    snapshot_needed=true
  fi
else
  for path in "${ci_paths[@]}"; do
    if [[ -e "$ROOT_DIR/$path" ]] && ! git -C "$ROOT_DIR" ls-files --error-unmatch "$path" >/dev/null 2>&1; then
      snapshot_needed=true
    fi
  done
  if ! git -C "$ROOT_DIR" diff --quiet -- "${ci_paths[@]}" ||
    ! git -C "$ROOT_DIR" diff --cached --quiet -- "${ci_paths[@]}"; then
    snapshot_needed=true
  fi
fi

if [[ "$snapshot_needed" == "true" && "$HARNESS_CI_SNAPSHOT_SCOPE" == "all" ]]; then
  echo "[WARN] Pushing an isolated Harness-only snapshot of the current worktree." >&2
elif [[ "$snapshot_needed" == "true" ]]; then
  snapshot_needed=true
fi

push_source="HEAD"
if [[ "$snapshot_needed" == "true" ]]; then
  snapshot_index="$tmp_dir/snapshot.index"
  GIT_INDEX_FILE="$snapshot_index" git -C "$ROOT_DIR" read-tree HEAD
  if [[ "$HARNESS_CI_SNAPSHOT_SCOPE" == "all" ]]; then
    GIT_INDEX_FILE="$snapshot_index" git -C "$ROOT_DIR" add -A
  else
    for path in "${ci_paths[@]}"; do
      if [[ -e "$ROOT_DIR/$path" ]]; then
        GIT_INDEX_FILE="$snapshot_index" git -C "$ROOT_DIR" add -f "$path"
      fi
    done
  fi
  if [[ -n "${HARNESS_CI_IMAGE_SERVICES// }" ]]; then
    printf '%s\n' "$HARNESS_CI_IMAGE_SERVICES" >"$tmp_dir/harness-ci-image-services"
    services_blob="$(git -C "$ROOT_DIR" hash-object -w "$tmp_dir/harness-ci-image-services")"
    GIT_INDEX_FILE="$snapshot_index" git -C "$ROOT_DIR" update-index \
      --add --cacheinfo "100644,$services_blob,$HARNESS_CI_IMAGE_SERVICES_FILE"
    echo "[INFO] Limiting Harness image build to: $HARNESS_CI_IMAGE_SERVICES"
  fi
  snapshot_tree="$(GIT_INDEX_FILE="$snapshot_index" git -C "$ROOT_DIR" write-tree)"
  push_source="$(
    GIT_AUTHOR_NAME="Chat OS Harness CI" \
    GIT_AUTHOR_EMAIL="harness-ci@chatos.local" \
    GIT_COMMITTER_NAME="Chat OS Harness CI" \
    GIT_COMMITTER_EMAIL="harness-ci@chatos.local" \
      git -C "$ROOT_DIR" commit-tree "$snapshot_tree" -p HEAD -m "chore: harness ci trial snapshot"
  )"
  echo "[WARN] Pushing isolated Harness-only snapshot commit: $push_source" >&2
fi

if [[ "$HARNESS_CI_SNAPSHOT_SCOPE" != "all" ]] && {
  ! git -C "$ROOT_DIR" diff --quiet ||
  ! git -C "$ROOT_DIR" diff --cached --quiet ||
  [[ -n "$(git -C "$ROOT_DIR" ls-files --others --exclude-standard)" ]]
}; then
  echo "[WARN] Other uncommitted files are not included in the Harness CI mirror." >&2
fi

askpass="$tmp_dir/git-askpass.sh"
cat >"$askpass" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  *Username*) printf '%s\n' "$HARNESS_GIT_USERNAME" ;;
  *Password*) printf '%s\n' "$HARNESS_GIT_PASSWORD" ;;
  *) printf '\n' ;;
esac
EOF
chmod 700 "$askpass"

refspec="$push_source:refs/heads/$HARNESS_CI_BRANCH"
if [[ "$HARNESS_CI_FORCE_PUSH" == "true" ]]; then
  refspec="+$refspec"
fi

echo "[INFO] Pushing $push_source to Harness repo branch: $HARNESS_CI_BRANCH"
GIT_ASKPASS="$askpass" \
GIT_TERMINAL_PROMPT=0 \
HARNESS_GIT_USERNAME="$push_username" \
HARNESS_GIT_PASSWORD="$push_token" \
  git -C "$ROOT_DIR" push "$git_url" "$refspec"

pipeline_url="$HARNESS_BASE_URL/api/v1/repos/$repo_api_ref/pipelines"

create_or_update_pipeline() {
  local identifier="$1"
  local config_path="$2"
  local description="$3"
  local body_file="$tmp_dir/pipeline-body-$identifier.json"
  local out_file="$tmp_dir/pipeline-$identifier.json"
  local pipeline_status patch_status pipeline_id_encoded

  json_body "$body_file" \
    "identifier=$identifier" \
    "description=$description" \
    "disabled=false" \
    "default_branch=$HARNESS_CI_BRANCH" \
    "config_path=$config_path"

  pipeline_status="$(request_json POST "$pipeline_url" "$admin_token" "$body_file" "$out_file")"
  if request_ok "$pipeline_status"; then
    echo "[OK] Created Harness pipeline: $identifier"
    return 0
  fi

  if ! body_mentions_exists "$out_file"; then
    echo "[ERROR] Create Harness pipeline failed: HTTP $pipeline_status" >&2
    cat "$out_file" >&2 || true
    exit 1
  fi

  pipeline_id_encoded="$(url_encode "$identifier")"
  patch_status="$(request_json PATCH "$pipeline_url/$pipeline_id_encoded" "$admin_token" "$body_file" "$out_file")"
  if request_ok "$patch_status"; then
    echo "[OK] Updated Harness pipeline: $identifier"
  else
    echo "[ERROR] Update Harness pipeline failed: HTTP $patch_status" >&2
    cat "$out_file" >&2 || true
    exit 1
  fi
}

create_or_update_pipeline "$HARNESS_CI_PIPELINE" "$HARNESS_CI_CONFIG_PATH" "Chat OS CI"

case "$HARNESS_CI_REGISTER_IMAGE_SERVICE_PIPELINES" in
  1|true|TRUE|True|yes|YES|Yes|on|ON|On)
    while IFS= read -r image_service; do
      if [[ -z "$image_service" ]]; then
        continue
      fi
      image_pipeline_identifier="$HARNESS_CI_IMAGE_PIPELINE_PREFIX$image_service"
      image_pipeline_config="$HARNESS_CI_IMAGE_PIPELINE_DIR/$image_pipeline_identifier.yml"
      if [[ ! -f "$ROOT_DIR/$image_pipeline_config" ]]; then
        echo "[WARN] Skipping missing image service pipeline config: $image_pipeline_config" >&2
        continue
      fi
      create_or_update_pipeline \
        "$image_pipeline_identifier" \
        "$image_pipeline_config" \
        "Build and deploy Chat OS image: $image_service"
    done < <(bash "$ROOT_DIR/docker/deploy.sh" build-services)
    ;;
  *)
    ;;
esac

if [[ "$HARNESS_CI_RUN" == "true" ]]; then
  pipeline_id_encoded="$(url_encode "$HARNESS_CI_PIPELINE")"
  branch_encoded="$(url_encode "$HARNESS_CI_BRANCH")"
  execution_file="$tmp_dir/execution.json"
  execution_status="$(request_json POST "$pipeline_url/$pipeline_id_encoded/executions?branch=$branch_encoded" "$admin_token" "" "$execution_file")"
  if request_ok "$execution_status"; then
    execution_number="$(json_get "$execution_file" number)"
    echo "[OK] Triggered Harness execution: ${execution_number:-unknown}"
  else
    echo "[ERROR] Trigger Harness execution failed: HTTP $execution_status" >&2
    cat "$execution_file" >&2 || true
    exit 1
  fi
fi

echo
echo "Harness repo:      $HARNESS_BASE_URL/$repo_ref"
echo "Harness pipeline:  $HARNESS_BASE_URL/$repo_ref/pipelines/$HARNESS_CI_PIPELINE"
if [[ "${execution_number:-}" != "" ]]; then
  echo "Harness execution: $HARNESS_BASE_URL/$repo_ref/pipelines/$HARNESS_CI_PIPELINE/execution/$execution_number"
fi
