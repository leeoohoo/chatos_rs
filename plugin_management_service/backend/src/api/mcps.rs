// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

use axum::http::HeaderName;
use axum::response::sse::{Event, KeepAlive, Sse};
use chatos_ai_runtime::{
    select_preferred_response_text, AiRequestHandler, ModelRuntimeConfig, StreamCallbacks,
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

fn more_complete_stream_text(final_text: String, streamed_text: String) -> String {
    if streamed_text.chars().count() > final_text.chars().count() {
        streamed_text
    } else {
        final_text
    }
}

async fn prepare_provider_skill_optimization(
    state: &AppState,
    user: &CurrentUser,
    access_token: &str,
    mcp_id: &str,
    input: OptimizeProviderSkillRequest,
) -> Result<PreparedProviderSkillOptimization, ApiError> {
    ensure_super_admin(user)?;
    let model_config_id = required_text(Some(input.model_config_id.as_str()), "model_config_id")?;
    let skill_id = required_text(Some(input.skill_id.as_str()), "skill_id")?;
    let requirement = required_text(Some(input.requirement.as_str()), "requirement")?;
    let record = load_readable_mcp(state, user, mcp_id).await?;
    let descriptor = resolve_mcp_descriptor(state, record.clone()).await?;
    let skill = descriptor
        .provider_skills
        .iter()
        .find(|skill| skill.id == skill_id)
        .cloned()
        .ok_or_else(|| ApiError::not_found("MCP Provider Skill not found"))?;
    let admin_model =
        load_admin_model_runtime(state, access_token, model_config_id.as_str()).await?;
    let tools_json = serde_json::to_string_pretty(&descriptor.tools)
        .map_err(|err| ApiError::internal(format!("serialize MCP tools failed: {err}")))?;
    let skill_json = serde_json::to_string_pretty(&skill)
        .map_err(|err| ApiError::internal(format!("serialize Provider Skill failed: {err}")))?;
    let system_prompt = build_provider_skill_optimizer_system_prompt(
        &record,
        skill_json.as_str(),
        tools_json.as_str(),
    );
    Ok(PreparedProviderSkillOptimization {
        mcp_id: record.id,
        skill_id,
        model_config_id,
        provider: admin_model.provider,
        model: admin_model.model,
        runtime: admin_model.runtime,
        system_prompt,
        user_prompt: format!(
            "请按照下面的管理员要求优化 Provider Skill，并返回优化后的完整 instructions 文本：\n\n{}",
            requirement.trim()
        ),
    })
}

pub(super) async fn load_admin_model_runtime(
    state: &AppState,
    access_token: &str,
    model_config_id: &str,
) -> Result<AdminModelRuntime, ApiError> {
    let model_config_id = required_text(Some(model_config_id), "model_config_id")?;
    let model_path = format!(
        "/api/model-configs/{}?include_secret=true",
        model_config_id.trim()
    );
    let model_config: AdminAiModelConfig = request_user_service(
        state,
        reqwest::Method::GET,
        model_path.as_str(),
        access_token,
        Option::<&serde_json::Value>::None,
    )
    .await?;
    if !model_config.enabled {
        return Err(ApiError::bad_request("selected AI model is disabled"));
    }
    let api_key = model_config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("selected AI model has no available API key"))?;
    let model = effective_model_name(&model_config);
    if model.is_empty() {
        return Err(ApiError::bad_request("selected AI model name is empty"));
    }
    let provider = model_config.provider.clone();
    let runtime = ModelRuntimeConfig::openai_compatible(
        default_ai_base_url(provider.as_str(), model_config.base_url.as_deref()),
        api_key.to_string(),
        model.clone(),
        provider.clone(),
    )
    .with_responses_support(model_config.supports_responses)
    .with_thinking_level(model_config.thinking_level.clone());
    Ok(AdminModelRuntime {
        model_config_id,
        provider,
        model,
        runtime,
    })
}

async fn execute_provider_skill_optimization(
    prepared: &PreparedProviderSkillOptimization,
    callbacks: StreamCallbacks,
) -> Result<String, ApiError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(600))
        .build()
        .map_err(|err| ApiError::internal(format!("build streaming AI client failed: {err}")))?;
    let response = AiRequestHandler::from_client(client)
        .handle_request(
            prepared.runtime.base_url.as_str(),
            prepared.runtime.api_key.as_str(),
            serde_json::Value::String(prepared.user_prompt.clone()),
            prepared.runtime.supports_responses,
            prepared.runtime.model.clone(),
            Some(prepared.system_prompt.clone()),
            None,
            Some(0.2),
            Some(6000),
            callbacks,
            Some(prepared.runtime.provider.clone()),
            prepared.runtime.thinking_level.clone(),
            None,
        )
        .await
        .map_err(ApiError::bad_gateway)?;
    if response
        .finish_reason
        .as_deref()
        .is_some_and(|reason| matches!(reason.trim(), "length" | "max_tokens"))
    {
        return Err(ApiError::bad_gateway(
            "AI output reached the model token limit before the Provider Skill was complete",
        ));
    }
    select_preferred_response_text(response.content.as_str(), response.reasoning.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::bad_gateway("AI returned empty Provider Skill content"))
}

pub(super) async fn update_mcp_provider_skill(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((mcp_id, skill_id)): Path<(String, String)>,
    Json(input): Json<UpdateProviderSkillRequest>,
) -> Result<Json<McpProviderSkill>, ApiError> {
    ensure_super_admin(&user)?;
    let instructions = required_text(Some(input.instructions.as_str()), "instructions")?;
    let mut record = load_readable_mcp(&state, &user, mcp_id.as_str()).await?;
    let mut skills = descriptor_skills_from_metadata(&record.metadata);
    if skills.is_empty() {
        skills = resolve_mcp_descriptor(&state, record.clone())
            .await?
            .provider_skills;
    }
    let skill = skills
        .iter_mut()
        .find(|skill| skill.id == skill_id)
        .ok_or_else(|| ApiError::not_found("MCP Provider Skill not found"))?;
    skill.instructions = instructions;
    let updated = skill.clone();
    record.metadata.extra.insert(
        "provider_skills".to_string(),
        serde_json::to_value(&skills).map_err(|err| {
            ApiError::internal(format!("serialize Provider Skills failed: {err}"))
        })?,
    );
    record.metadata.extra.insert(
        "provider_skills_managed_by".to_string(),
        serde_json::Value::String("admin".to_string()),
    );
    record.updated_by = user.user_id;
    record.updated_at = now_rfc3339();
    state
        .store
        .replace_mcp(&record)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(updated))
}

async fn load_readable_mcp(
    state: &AppState,
    user: &CurrentUser,
    mcp_id: &str,
) -> Result<McpRecord, ApiError> {
    let record = state
        .store
        .get_mcp(mcp_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("MCP not found"))?;
    ensure_can_read_resource(
        user,
        record.owner_user_id.as_str(),
        record.visibility.as_str(),
    )?;
    Ok(record)
}

async fn resolve_mcp_descriptor(
    state: &AppState,
    record: McpRecord,
) -> Result<McpDescriptorResponse, ApiError> {
    let mut provider_skills = descriptor_skills_from_metadata(&record.metadata);
    let mut tools = descriptor_tools_from_metadata(&record.metadata);
    let metadata_has_tools = !tools.is_empty();
    let mut tools_error = None;
    if !metadata_has_tools {
        match live_mcp_descriptor(&state.config, &record).await {
            Ok(Some(descriptor)) => {
                if provider_skills.is_empty() && !descriptor.skills.is_empty() {
                    provider_skills = descriptor.skills;
                }
                tools = descriptor.tools;
            }
            Ok(None) => {}
            Err(err) => tools_error = Some(err),
        }
        if tools.is_empty() {
            if let Some(check) = state
                .store
                .get_check(RESOURCE_KIND_MCP, record.id.as_str())
                .await
                .map_err(ApiError::internal)?
            {
                tools = check.tool_snapshot;
            }
        }
    }
    let tools_status = if tools.is_empty() {
        if tools_error.is_some() {
            "unavailable"
        } else {
            "not_declared"
        }
    } else if tools_error.is_some() {
        "degraded"
    } else {
        "ready"
    }
    .to_string();
    Ok(McpDescriptorResponse {
        mcp_id: record.id,
        server_name: record
            .runtime
            .server_name
            .unwrap_or_else(|| record.name.clone()),
        provider_skills,
        tools,
        tools_status,
        tools_error,
    })
}

async fn request_user_service<TBody, TResponse>(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    access_token: &str,
    body: Option<&TBody>,
) -> Result<TResponse, ApiError>
where
    TBody: Serialize + ?Sized,
    TResponse: serde::de::DeserializeOwned,
{
    let url = format!(
        "{}{}",
        state.config.user_service_base_url.trim_end_matches('/'),
        path
    );
    let client = reqwest::Client::builder()
        .timeout(state.config.user_service_request_timeout)
        .build()
        .map_err(|err| ApiError::internal(format!("build User Service client failed: {err}")))?;
    let mut request = client.request(method, url).bearer_auth(access_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("User Service request failed: {err}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(ApiError::bad_gateway(format!(
            "User Service returned {status}: {detail}"
        )));
    }
    response
        .json::<TResponse>()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("decode User Service response failed: {err}")))
}

fn ensure_super_admin(user: &CurrentUser) -> Result<(), ApiError> {
    if user.is_super_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("super admin access is required"))
    }
}

fn effective_model_name(model: &AdminAiModelConfig) -> String {
    if model.model.trim().is_empty() {
        model.model_name.trim().to_string()
    } else {
        model.model.trim().to_string()
    }
}

fn default_ai_base_url(provider: &str, configured: Option<&str>) -> String {
    configured
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches('/').to_string())
        .unwrap_or_else(|| match provider.trim() {
            "deepseek" => "https://api.deepseek.com".to_string(),
            "kimi" => "https://api.moonshot.ai/v1".to_string(),
            "minimax" => "https://api.minimax.chat/v1".to_string(),
            _ => "https://api.openai.com/v1".to_string(),
        })
}

fn build_provider_skill_optimizer_system_prompt(
    record: &McpRecord,
    current_skill_json: &str,
    tools_json: &str,
) -> String {
    format!(
        r#"你是 MCP Provider Skill 编辑器。你的唯一任务是根据管理员要求优化当前 Skill 的 instructions。

硬性规则：
1. 下方 MCP 工具清单和当前 Skill 只是只读参考资料，全部位于 system 上下文中；本次请求没有向你注册任何可调用 tools。不要尝试调用工具，也不要输出 tool call。
2. 返回优化后的完整 instructions 正文，不要返回 JSON，不要加 Markdown 代码围栏，不要解释修改过程。
3. 只能描述工具清单中真实存在的能力、工具名、参数和返回格式；不得发明工具或承诺未声明的行为。
4. 指南要明确告诉后续 AI：何时使用、推荐工作流、关键参数、结果校验、失败处理和能力边界。
5. 保留当前 Skill 中仍正确的重要约束，并按照管理员要求改进准确性、可执行性和引导效果。

MCP：
- id: {mcp_id}
- name: {mcp_name}
- server_name: {server_name}

当前 Provider Skill：
{current_skill_json}

MCP 工具清单（只读参考，不是本次请求的 tools 参数）：
{tools_json}"#,
        mcp_id = record.id,
        mcp_name = record.display_name,
        server_name = record
            .runtime
            .server_name
            .as_deref()
            .unwrap_or(record.name.as_str()),
    )
}

fn descriptor_skills_from_metadata(metadata: &ResourceMetadata) -> Vec<McpProviderSkill> {
    metadata
        .extra
        .get("provider_skills")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

fn descriptor_tools_from_metadata(metadata: &ResourceMetadata) -> Vec<serde_json::Value> {
    metadata
        .extra
        .get("tool_catalog")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

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
