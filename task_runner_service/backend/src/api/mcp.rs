// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::core::{bearer_token_from_headers, current_user_from_user_service_token};
use super::*;
use serde_json::json;

pub(super) async fn list_mcp_catalog(State(state): State<AppState>) -> Json<Vec<McpCatalogEntry>> {
    Json(state.mcp_catalog_service.list_catalog())
}

pub(super) async fn list_task_capability_catalog(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<Value>, ApiError> {
    let owner_user_id = user
        .effective_owner_user_id()
        .ok_or_else(|| ApiError::unauthorized("current user is missing owner scope"))?;
    let policy = state
        .task_service
        .resolve_task_runner_policy(Some(&user), Some(owner_user_id))
        .await
        .map_err(ApiError::bad_gateway)?
        .ok_or_else(|| ApiError::internal("plugin management policy resolver is unavailable"))?;
    let selectable_builtin_kinds = policy
        .selectable_builtin_kind_names()
        .into_iter()
        .collect::<std::collections::HashSet<_>>();
    let selectable_builtin_mcps = state
        .mcp_catalog_service
        .list_catalog()
        .into_iter()
        .filter(|item| selectable_builtin_kinds.contains(item.kind.as_str()))
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "policy_revision": policy.policy_revision(),
        "selectable_builtin_mcps": selectable_builtin_mcps,
        "selectable_external_mcps": policy.selectable_external_mcp_views(),
        "selectable_skills": policy.selectable_skill_views(),
    })))
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
    Ok(Json(redact_workspace_paths(&state, preview)?))
}

pub(super) async fn mcp_entrypoint(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<JsonRpcRequest>,
) -> Json<JsonRpcResponse> {
    let id = request.id.clone().unwrap_or(Value::Null);
    let request_method = request.method.clone();
    let request_tool_name = request
        .params
        .get("name")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp request received"
    );
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
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp agent token extracted"
    );
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
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp agent token verified"
    );
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
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        "task runner mcp downstream token resolved"
    );
    let request_context = mcp_request_context_from_headers(&headers);
    tracing::info!(
        method = %request_method,
        tool_name = request_tool_name.as_deref().unwrap_or(""),
        project_id = request_context.project_id.as_deref().unwrap_or(""),
        task_profile = request_context.task_profile.as_deref().unwrap_or(""),
        tool_profile = request_context.tool_profile.as_deref().unwrap_or(""),
        "task runner mcp dispatching jsonrpc"
    );
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
        builtin_prompt_locale: header_text(headers, "x-task-runner-builtin-prompt-locale")
            .or_else(|| header_text(headers, "x-chatos-internal-context-locale")),
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
