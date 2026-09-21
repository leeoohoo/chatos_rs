#!/usr/bin/env bash
# Production mTLS validation helpers sourced by docker/deploy.sh.

ensure_config_center_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value CONFIG_CENTER_MTLS_DIR ./secrets/config-center-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    local-connector-service.identity.pem \
    mcp-management-service.identity.pem \
    memory-engine.identity.pem \
    official-website.identity.pem \
    plugin-management-service.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-config-center-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Configuration Center mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -CAfile "$resolved_dir/ca.crt" "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Configuration Center server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    local-connector-service.identity.pem \
    mcp-management-service.identity.pem \
    memory-engine.identity.pem \
    official-website.identity.pem \
    plugin-management-service.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Configuration Center client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Configuration Center client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_mcp_management_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value MCP_MANAGEMENT_MTLS_DIR ./secrets/mcp-management-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos.identity.pem \
    task-runner.identity.pem \
    configuration-center.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-mcp-management-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] MCP Management mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] MCP Management server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos.identity.pem \
    task-runner.identity.pem \
    configuration-center.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] MCP Management client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] MCP Management client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_task_runner_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value TASK_RUNNER_MTLS_DIR ./secrets/task-runner-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos.identity.pem \
    mcp-management-service.identity.pem \
    user-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-task-runner-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Task Runner mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Task Runner server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos.identity.pem \
    mcp-management-service.identity.pem \
    user-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Task Runner client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Task Runner client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_chatos_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value CHATOS_MTLS_DIR ./secrets/chatos-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    task-runner.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-chatos-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] ChatOS mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] ChatOS server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] ChatOS server key is unreadable" >&2
    return 1
  fi
  for required_file in task-runner.identity.pem mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] ChatOS client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] ChatOS client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_local_connector_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value LOCAL_CONNECTOR_MTLS_DIR ./secrets/local-connector-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-local-connector-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Local Connector mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Local Connector server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] Local Connector server key is unreadable" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Local Connector client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Local Connector client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_user_service_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value USER_SERVICE_MTLS_DIR ./secrets/user-service-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    memory-engine.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-user-service-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] User Service mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] User Service server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] User Service server key is unreadable" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    memory-engine.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] User Service client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] User Service client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

validate_runtime_material() {
  validate_production_secrets
  ensure_config_center_mtls_material
  ensure_mcp_management_mtls_material
  ensure_task_runner_mtls_material
  ensure_chatos_mtls_material
  ensure_local_connector_mtls_material
  ensure_user_service_mtls_material
  ensure_plugin_management_mtls_material
  ensure_memory_engine_mtls_material
}

ensure_plugin_management_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value PLUGIN_MANAGEMENT_MTLS_DIR ./secrets/plugin-management-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    local-connector-service.identity.pem \
    memory-engine.identity.pem \
    mcp-management-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-plugin-management-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Plugin Management mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Plugin Management server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  if ! openssl pkey -in "$resolved_dir/server.key" -noout >/dev/null 2>&1; then
    echo "[ERROR] Plugin Management server key is unreadable" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    task-runner.identity.pem \
    local-connector-service.identity.pem \
    memory-engine.identity.pem \
    mcp-management-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Plugin Management client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Plugin Management client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}

ensure_memory_engine_mtls_material() {
  need_cmd openssl
  local configured_dir resolved_dir
  local required_file failures=0
  configured_dir="$(env_value MEMORY_ENGINE_MTLS_DIR ./secrets/memory-engine-mtls)"
  if [[ "$configured_dir" = /* ]]; then
    resolved_dir="$configured_dir"
  else
    resolved_dir="$SCRIPT_DIR/$configured_dir"
  fi

  for required_file in \
    ca.crt server.crt server.key \
    chatos-backend.identity.pem \
    configuration-center.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if [[ ! -s "$resolved_dir/$required_file" ]]; then
      failures=1
      break
    fi
  done

  if (( failures > 0 )) && ! is_production_environment; then
    "$ROOT_DIR/scripts/generate-memory-engine-mtls.sh" "$resolved_dir"
    failures=0
  fi
  if (( failures > 0 )); then
    echo "[ERROR] Memory Engine mTLS material is incomplete: $resolved_dir" >&2
    echo "        Generate or provision it before deployment; production never creates certificates automatically." >&2
    return 1
  fi
  if ! openssl verify -purpose sslserver -CAfile "$resolved_dir/ca.crt" \
    "$resolved_dir/server.crt" >/dev/null; then
    echo "[ERROR] Memory Engine server certificate is not trusted by the configured CA" >&2
    return 1
  fi
  for required_file in \
    chatos-backend.identity.pem \
    configuration-center.identity.pem \
    task-runner.identity.pem \
    user-service.identity.pem
  do
    if ! openssl verify -purpose sslclient -CAfile "$resolved_dir/ca.crt" \
      "$resolved_dir/$required_file" >/dev/null; then
      echo "[ERROR] Memory Engine client certificate is invalid: $required_file" >&2
      return 1
    fi
    if ! openssl pkey -in "$resolved_dir/$required_file" -noout >/dev/null 2>&1; then
      echo "[ERROR] Memory Engine client identity has no readable private key: $required_file" >&2
      return 1
    fi
  done
}
