use super::core::{bearer_token_from_headers, current_user_from_user_service_token};
use super::*;

pub(super) async fn list_mcp_catalog(State(state): State<AppState>) -> Json<Vec<McpCatalogEntry>> {
    Json(state.mcp_catalog_service.list_catalog())
}

pub(super) async fn get_mcp_server_info(State(state): State<AppState>) -> Json<McpServerInfo> {
    Json(state.task_runner_mcp_service.server_info())
}

pub(super) async fn preview_mcp_prompt(
    State(state): State<AppState>,
    Json(input): Json<McpPromptPreviewRequest>,
) -> Result<Json<McpPromptPreviewResponse>, ApiError> {
    let preview = state
        .mcp_catalog_service
        .preview_prompt(input)
        .map_err(ApiError::bad_request)?;
    Ok(Json(preview))
}

pub(super) async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let agent_access_token = match bearer_token_from_headers(&headers) {
        Ok(token) => token.to_string(),
        Err(err) => {
            let err = ApiError::unauthorized(err);
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(crate::mcp_server::JsonRpcError {
                    code: -32001,
                    message: err.message,
                }),
            });
        }
    };
    let current_user =
        match current_user_from_user_service_token(&state.config, &agent_access_token).await {
            Ok(value) => value,
            Err(err) => {
                return Json(JsonRpcResponse {
                    jsonrpc: "2.0",
                    id,
                    result: None,
                    error: Some(crate::mcp_server::JsonRpcError {
                        code: -32001,
                        message: err.message,
                    }),
                });
            }
        };
    let downstream_access_token = match downstream_access_token_from_headers(
        &state.config,
        &headers,
        &agent_access_token,
        &current_user,
    )
    .await
    {
        Ok(value) => value,
        Err(err) => {
            return Json(JsonRpcResponse {
                jsonrpc: "2.0",
                id,
                result: None,
                error: Some(crate::mcp_server::JsonRpcError {
                    code: -32001,
                    message: err.message,
                }),
            });
        }
    };
    let request_context = mcp_request_context_from_headers(&headers);
    Json(
        crate::auth::with_access_token_scope(Some(downstream_access_token), async move {
            state
                .task_runner_mcp_service
                .handle_jsonrpc(request, current_user, request_context)
                .await
        })
        .await,
    )
}

async fn downstream_access_token_from_headers(
    config: &crate::config::AppConfig,
    headers: &HeaderMap,
    agent_access_token: &str,
    agent_user: &CurrentUser,
) -> Result<String, ApiError> {
    let Some(user_access_token) = user_access_token_from_headers(headers)? else {
        return Ok(agent_access_token.to_string());
    };
    let user = current_user_from_user_service_token(config, user_access_token.as_str()).await?;
    ensure_same_owner_scope(agent_user, &user)?;
    Ok(user_access_token)
}

fn user_access_token_from_headers(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    for key in [
        "x-chatos-user-authorization",
        "x-user-service-authorization",
        "x-chatos-user-token",
    ] {
        let Some(value) = header_text(headers, key) else {
            continue;
        };
        let token = if let Some(token) = value.strip_prefix("Bearer ").map(str::trim) {
            token
        } else if let Some(token) = value.strip_prefix("bearer ").map(str::trim) {
            token
        } else {
            value.as_str()
        };
        if token.is_empty() {
            continue;
        }
        return Ok(Some(token.to_string()));
    }
    Ok(None)
}

fn ensure_same_owner_scope(agent_user: &CurrentUser, user: &CurrentUser) -> Result<(), ApiError> {
    let agent_owner = agent_user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("agent token missing owner scope"))?;
    let user_owner = user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("user token missing owner scope"))?;
    if agent_owner == user_owner {
        Ok(())
    } else {
        Err(ApiError::forbidden(
            "agent token and user token owner scope do not match",
        ))
    }
}

fn mcp_request_context_from_headers(headers: &HeaderMap) -> McpRequestContext {
    McpRequestContext {
        project_id: header_text(headers, "x-chatos-project-id")
            .or_else(|| header_text(headers, "x-task-runner-project-id")),
        source_session_id: header_text(headers, "x-chatos-session-id")
            .or_else(|| header_text(headers, "x-chatos-conversation-id")),
        source_turn_id: header_text(headers, "x-chatos-turn-id"),
        source_user_message_id: header_text(headers, "x-chatos-user-message-id"),
        workspace_dir: header_text(headers, "x-task-runner-workspace-dir")
            .or_else(|| header_text(headers, "x-chatos-workspace-dir"))
            .or_else(|| header_text(headers, "x-chatos-workspace-root")),
        remote_server_config: header_text(headers, "x-task-runner-remote-server-config")
            .or_else(|| header_text(headers, "x-task-runner-remote-server-json")),
        tool_profile: header_text(headers, "x-task-runner-tool-profile"),
        task_profile: header_text(headers, "x-task-runner-task-profile"),
        chatos_plan_mode: header_bool(headers, "x-chatos-plan-mode"),
    }
}

fn header_text(headers: &HeaderMap, key: &'static str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn header_bool(headers: &HeaderMap, key: &'static str) -> bool {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}
