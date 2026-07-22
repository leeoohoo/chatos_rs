// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

use axum::http::HeaderName;
use axum::response::sse::{Event, KeepAlive, Sse};
use chatos_ai_runtime::{
    build_responses_text_input, run_compatible_prompt_with, select_preferred_response_text,
    AiRequestHandler, ModelRuntimeConfig, SimplePromptOptions, StreamCallbacks,
};
use futures_util::stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::tool_catalog::live_mcp_descriptor;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct AdminAiModelConfig {
    id: String,
    name: String,
    provider: String,
    model: String,
    #[serde(default)]
    model_name: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    has_api_key: bool,
    #[serde(default)]
    base_url: Option<String>,
    enabled: bool,
    #[serde(default)]
    supports_responses: bool,
    #[serde(default)]
    thinking_level: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub(super) struct AdminAiModelSettings {
    #[serde(default)]
    model_request_max_retries: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub(super) struct OptimizeProviderSkillRequest {
    model_config_id: String,
    skill_id: String,
    requirement: String,
}

#[derive(Debug, Serialize)]
pub(super) struct OptimizeProviderSkillResponse {
    mcp_id: String,
    skill_id: String,
    model_config_id: String,
    provider: String,
    model: String,
    optimized_instructions: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OptimizeProviderSkillStreamMessage {
    Started { provider: String, model: String },
    Thinking { delta: String },
    Chunk { delta: String },
    Done { optimized_instructions: String },
    Error { message: String },
}

impl OptimizeProviderSkillStreamMessage {
    fn event_name(&self) -> &'static str {
        match self {
            Self::Started { .. } => "started",
            Self::Thinking { .. } => "thinking",
            Self::Chunk { .. } => "chunk",
            Self::Done { .. } => "done",
            Self::Error { .. } => "error",
        }
    }
}

struct PreparedProviderSkillOptimization {
    mcp_id: String,
    skill_id: String,
    model_config_id: String,
    provider: String,
    model: String,
    runtime: ModelRuntimeConfig,
    system_prompt: String,
    user_prompt: String,
}

pub(super) struct AdminModelRuntime {
    pub(super) model_config_id: String,
    pub(super) provider: String,
    pub(super) model: String,
    pub(super) runtime: ModelRuntimeConfig,
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdateProviderSkillRequest {
    instructions: String,
}

pub(super) async fn list_mcps(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Query(query): Query<ListResourcesQuery>,
) -> Result<Json<ListResponse<McpRecord>>, ApiError> {
    state
        .store
        .list_mcps(&user, &query)
        .await
        .map(Json)
        .map_err(ApiError::internal)
}

pub(super) async fn create_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(payload): Json<McpPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    validate_client_managed_mcp_payload(&payload, &user)?;
    let visibility = normalize_visibility(payload.visibility.as_deref(), &user)?;
    let owner_user_id = requested_owner_user_id(payload.owner_user_id.as_deref(), &user)?;
    let name = required_text(payload.name.as_deref(), "name")?;
    let display_name = payload
        .display_name
        .as_deref()
        .and_then(|value| normalized(Some(value)))
        .unwrap_or_else(|| name.clone());
    let runtime = payload
        .runtime
        .ok_or_else(|| ApiError::bad_request("runtime is required"))?;
    validate_mcp_runtime(&runtime)?;
    validate_mcp_visibility_for_runtime(visibility.as_str(), &runtime)?;
    let now = now_rfc3339();
    let record = McpRecord {
        id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.clone(),
        owner_kind: owner_kind_for(&visibility, &user),
        visibility,
        source_kind: default_source_kind(payload.source_kind, &user),
        name,
        display_name,
        description: payload
            .description
            .and_then(|value| normalized(Some(&value))),
        enabled: payload.enabled.unwrap_or(true),
        runtime,
        security: payload.security.unwrap_or_default(),
        metadata: payload.metadata.unwrap_or_default(),
        created_by: user.user_id.clone(),
        updated_by: user.user_id.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn get_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<McpRecord>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(Json(record))
}

pub(super) async fn get_mcp_descriptor(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<McpDescriptorResponse>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;

    resolve_mcp_descriptor(&state, record).await.map(Json)
}

pub(super) async fn list_admin_ai_models(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<crate::auth::AccessToken>,
) -> Result<Json<Vec<AdminAiModelConfig>>, ApiError> {
    ensure_super_admin(&user)?;
    let path = format!("/api/model-configs?user_id={}", user.user_id.trim());
    let mut models: Vec<AdminAiModelConfig> = request_user_service(
        &state,
        reqwest::Method::GET,
        path.as_str(),
        access_token.0.as_str(),
        Option::<&serde_json::Value>::None,
    )
    .await?;
    models.retain(|item| {
        item.enabled
            && item.has_api_key
            && !item.id.trim().is_empty()
            && !effective_model_name(item).is_empty()
    });
    models.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.model.cmp(&right.model))
    });
    Ok(Json(models))
}

pub(super) async fn optimize_mcp_provider_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<crate::auth::AccessToken>,
    Path(mcp_id): Path<String>,
    Json(input): Json<OptimizeProviderSkillRequest>,
) -> Result<Json<OptimizeProviderSkillResponse>, ApiError> {
    let prepared = prepare_provider_skill_optimization(
        &state,
        &user,
        access_token.0.as_str(),
        mcp_id.as_str(),
        input,
    )
    .await?;
    let optimized_instructions =
        execute_provider_skill_optimization(&prepared, StreamCallbacks::default()).await?;
    Ok(Json(OptimizeProviderSkillResponse {
        mcp_id: prepared.mcp_id,
        skill_id: prepared.skill_id,
        model_config_id: prepared.model_config_id,
        provider: prepared.provider,
        model: prepared.model,
        optimized_instructions,
    }))
}

pub(super) async fn optimize_mcp_provider_skill_stream(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Extension(access_token): Extension<crate::auth::AccessToken>,
    Path(mcp_id): Path<String>,
    Json(input): Json<OptimizeProviderSkillRequest>,
) -> Result<Response, ApiError> {
    let prepared = prepare_provider_skill_optimization(
        &state,
        &user,
        access_token.0.as_str(),
        mcp_id.as_str(),
        input,
    )
    .await?;
    let (sender, receiver) = tokio::sync::mpsc::unbounded_channel();
    let started = OptimizeProviderSkillStreamMessage::Started {
        provider: prepared.provider.clone(),
        model: prepared.model.clone(),
    };
    let _ = sender.send(started);
    let chunk_sender = sender.clone();
    let thinking_sender = sender.clone();
    let streamed_content = Arc::new(Mutex::new(String::new()));
    let streamed_content_for_chunks = streamed_content.clone();
    let callbacks = StreamCallbacks {
        on_chunk: Some(Arc::new(move |delta| {
            if let Ok(mut content) = streamed_content_for_chunks.lock() {
                content.push_str(delta.as_str());
            }
            let _ = chunk_sender.send(OptimizeProviderSkillStreamMessage::Chunk { delta });
        })),
        on_thinking: Some(Arc::new(move |delta| {
            let _ = thinking_sender.send(OptimizeProviderSkillStreamMessage::Thinking { delta });
        })),
    };
    tokio::spawn(async move {
        match execute_provider_skill_optimization(&prepared, callbacks).await {
            Ok(optimized_instructions) => {
                let streamed_instructions = streamed_content
                    .lock()
                    .map(|value| value.trim().to_string())
                    .unwrap_or_default();
                let optimized_instructions =
                    more_complete_stream_text(optimized_instructions, streamed_instructions);
                let _ = sender.send(OptimizeProviderSkillStreamMessage::Done {
                    optimized_instructions,
                });
            }
            Err(err) => {
                let _ = sender.send(OptimizeProviderSkillStreamMessage::Error {
                    message: err.message,
                });
            }
        }
    });
    let event_stream = stream::unfold(receiver, |mut receiver| async move {
        receiver.recv().await.map(|message| {
            let data = serde_json::to_string(&message).unwrap_or_else(|_| {
                r#"{"type":"error","message":"serialize stream event failed"}"#.to_string()
            });
            let event = Event::default().event(message.event_name()).data(data);
            (Ok::<Event, Infallible>(event), receiver)
        })
    });
    let mut response = Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(10))
                .text("keepalive"),
        )
        .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    response.headers_mut().insert(
        HeaderName::from_static("x-accel-buffering"),
        HeaderValue::from_static("no"),
    );
    Ok(response)
}

mod support;

use self::support::{
    effective_model_name, ensure_super_admin, execute_provider_skill_optimization,
    more_complete_stream_text, prepare_provider_skill_optimization, request_user_service,
    resolve_mcp_descriptor,
};
pub(super) use self::support::{load_admin_model_runtime, update_mcp_provider_skill};

pub(super) async fn update_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
    Json(payload): Json<McpPayload>,
) -> Result<Json<McpRecord>, ApiError> {
    let mut record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if record.source_kind == SOURCE_KIND_SYSTEM_SEED {
        validate_system_seed_mcp_update(&payload)?;
        if let Some(enabled) = payload.enabled {
            record.enabled = enabled;
        }
        record.updated_by = user.user_id.clone();
        record.updated_at = now_rfc3339();
        state
            .store
            .replace_mcp(&record)
            .await
            .map_err(ApiError::internal)?;
        return Ok(Json(record));
    }
    validate_client_managed_mcp_payload(&payload, &user)?;
    if let Some(owner_user_id) = payload.owner_user_id.as_deref() {
        record.owner_user_id = requested_owner_user_id(Some(owner_user_id), &user)?;
    }
    if let Some(visibility) = payload.visibility.as_deref() {
        record.visibility = normalize_visibility(Some(visibility), &user)?;
        record.owner_kind = owner_kind_for(record.visibility.as_str(), &user);
    }
    if let Some(source_kind) = payload.source_kind {
        if user.is_super_admin() {
            record.source_kind = source_kind;
        }
    }
    if let Some(name) = payload.name.as_deref() {
        record.name = required_text(Some(name), "name")?;
    }
    if let Some(display_name) = payload.display_name {
        record.display_name =
            normalized(Some(&display_name)).unwrap_or_else(|| record.name.clone());
    }
    if let Some(description) = payload.description {
        record.description = normalized(Some(&description));
    }
    if let Some(enabled) = payload.enabled {
        record.enabled = enabled;
    }
    if let Some(runtime) = payload.runtime {
        validate_mcp_runtime(&runtime)?;
        record.runtime = runtime;
    }
    validate_client_managed_mcp_runtime(&record.runtime, &user)?;
    if let Some(security) = payload.security {
        record.security = security;
    }
    if let Some(metadata) = payload.metadata {
        record.metadata = metadata;
    }
    validate_mcp_visibility_for_runtime(record.visibility.as_str(), &record.runtime)?;
    record.updated_by = user.user_id.clone();
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(record))
}

pub(super) async fn delete_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let mut record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_update_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    if record.source_kind == SOURCE_KIND_SYSTEM_SEED {
        record.enabled = false;
        record.updated_at = now_rfc3339();
        record.updated_by = user.user_id;
        state
            .store
            .replace_mcp(&record)
            .await
            .map_err(ApiError::internal)?;
    } else {
        state
            .store
            .delete_mcp(mcp_id.as_str())
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn check_mcp(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(mcp_id): Path<String>,
) -> Result<Json<ResourceCheckRecord>, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        &user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    let is_local_connector = matches!(
        record.runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    );
    let previous_check = state
        .store
        .get_check(RESOURCE_KIND_MCP, record.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    if is_local_connector {
        if let Some(check) = previous_check {
            return Ok(Json(check));
        }
        return Ok(Json(check_record_for_mcp(
            &record,
            "unknown",
            Some("Local Connector has not reported a tool snapshot yet".to_string()),
            Vec::new(),
        )));
    }
    let check = if !record.enabled {
        check_record_for_mcp(
            &record,
            "unavailable",
            Some("resource is disabled".to_string()),
            previous_check
                .map(|check| check.tool_snapshot)
                .unwrap_or_default(),
        )
    } else {
        match live_mcp_descriptor(&state.config, &record).await {
            Ok(Some(descriptor)) => check_record_for_mcp(
                &record,
                if descriptor.tools.is_empty() {
                    "not_declared"
                } else {
                    "available"
                },
                None,
                descriptor.tools,
            ),
            Ok(None) => check_record_for_mcp(
                &record,
                "unknown",
                Some("runtime does not expose a server-side tool inspector".to_string()),
                previous_check
                    .map(|check| check.tool_snapshot)
                    .unwrap_or_default(),
            ),
            Err(err) => {
                let tools = previous_check
                    .map(|check| check.tool_snapshot)
                    .unwrap_or_default();
                let status = if tools.is_empty() {
                    "unavailable"
                } else {
                    "degraded"
                };
                check_record_for_mcp(&record, status, Some(err), tools)
            }
        }
    };
    // Never replace a real inspection snapshot with an empty placeholder. A
    // failed/unsupported inspection is returned to the caller, while the last
    // non-empty snapshot remains the durable fallback for descriptors.
    if !check.tool_snapshot.is_empty() {
        state
            .store
            .replace_check(&check)
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(Json(check))
}
