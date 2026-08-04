# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

# Sourced by smoke-mcp-management-cloud.sh. ROOT_DIR and require_command are
# provided by the caller so discovery stays separate from Runtime MCP checks.

discover_project_by_source() {
  local source_type="$1"
  require_command docker
  local mongodb_container
  mongodb_container="$(
    docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
  )"
  if [[ -z "$mongodb_container" ]]; then
    return 1
  fi
  docker exec -e MCP_SMOKE_PROJECT_SOURCE_TYPE="$source_type" "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const project = db.getSiblingDB("project_management_service").projects.findOne(
        {
          source_type: process.env.MCP_SMOKE_PROJECT_SOURCE_TYPE,
          status: "active",
          owner_user_id: {$type: "string"},
          id: {$type: "string"}
        },
        {_id: 0, id: 1, owner_user_id: 1}
      );
      if (project) {
        print(project.owner_user_id + "|" + project.id);
      }
    '
}

discover_local_project_with_sandbox() {
  require_command docker
  local mongodb_container
  mongodb_container="$(
    docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
  )"
  if [[ -z "$mongodb_container" ]]; then
    return 1
  fi
  docker exec "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const projectDb = db.getSiblingDB("project_management_service");
      const environments = projectDb.project_runtime_environments.find(
        {
          sandbox_enabled: true,
          file_provider: "local_connector",
          sandbox_provider: "local_connector",
          project_id: {$type: "string"}
        },
        {_id: 0, project_id: 1}
      ).sort({updated_at: -1}).toArray();
      for (const environment of environments) {
        const project = projectDb.projects.findOne(
          {
            id: environment.project_id,
            source_type: "local_connector",
            status: "active",
            owner_user_id: {$type: "string"}
          },
          {_id: 0, id: 1, owner_user_id: 1}
        );
        if (project) {
          print(project.owner_user_id + "|" + project.id);
          break;
        }
      }
    '
}

local_project_has_live_sandbox() {
  local owner_user_id="$1"
  local project_id="$2"
  require_command docker
  local mongodb_container
  mongodb_container="$(
    docker compose -f "$ROOT_DIR/docker/compose.yml" ps -q mongodb 2>/dev/null || true
  )"
  if [[ -z "$mongodb_container" ]]; then
    return 1
  fi
  docker exec \
    -e MCP_SMOKE_LOCAL_OWNER_USER_ID="$owner_user_id" \
    -e MCP_SMOKE_LOCAL_PROJECT_ID="$project_id" \
    "$mongodb_container" mongosh \
    -u "${MONGODB_USER:-admin}" \
    -p "${MONGODB_PASSWORD:-admin}" \
    --authenticationDatabase admin \
    --quiet \
    --eval '
      const projectDb = db.getSiblingDB("project_management_service");
      const connectorDb = db.getSiblingDB("local_connector_service");
      const project = projectDb.projects.findOne({
        id: process.env.MCP_SMOKE_LOCAL_PROJECT_ID,
        owner_user_id: process.env.MCP_SMOKE_LOCAL_OWNER_USER_ID,
        source_type: "local_connector",
        status: "active"
      });
      const environment = project && projectDb.project_runtime_environments.findOne({
        project_id: project.id,
        sandbox_enabled: true,
        file_provider: "local_connector",
        sandbox_provider: "local_connector"
      });
      const match = project && /^local:\/\/connector\/([^/]+)\/([^/]+)(?:\/|$)/.exec(
        project.root_path || ""
      );
      if (!project || !environment || !match) {
        quit(1);
      }
      let deviceId;
      let workspaceId;
      try {
        deviceId = decodeURIComponent(match[1]);
        workspaceId = decodeURIComponent(match[2]);
      } catch {
        quit(1);
      }
      const pairing = connectorDb.local_connector_sandbox_pairings.findOne({
        owner_user_id: project.owner_user_id,
        device_id: deviceId,
        workspace_id: workspaceId,
        enabled: true,
        sandbox_readiness: /^ready$/i
      });
      // The active lease collection is the authoritative online signal used by
      // Local Connector Service. `local_connector_sessions` is legacy history
      // and must not be used to decide whether MCP Management can route locally.
      const liveSession = connectorDb.local_connector_active_sessions.find({
        owner_user_id: project.owner_user_id,
        device_id: deviceId,
        status: "connected"
      }).toArray().some((session) => {
        const expiresAt = Date.parse(session.expires_at || "");
        return Number.isFinite(expiresAt) && expiresAt > Date.now();
      });
      if (!pairing || !liveSession) {
        quit(1);
      }
      print("live");
    ' >/dev/null
}
