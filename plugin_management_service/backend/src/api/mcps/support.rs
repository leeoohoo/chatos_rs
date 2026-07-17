// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::super::*;
use super::{
    live_mcp_descriptor, select_preferred_response_text, AdminAiModelConfig, AdminModelRuntime,
    AiRequestHandler, ModelRuntimeConfig, OptimizeProviderSkillRequest,
    PreparedProviderSkillOptimization, StreamCallbacks, UpdateProviderSkillRequest,
};
use chatos_service_runtime::http_body::{
    read_response_json_limited, read_response_preview_text_limited_or_message,
    ERROR_BODY_PREVIEW_LIMIT_BYTES, JSON_BODY_LIMIT_BYTES,
};
use chatos_service_runtime::{http_client_builder, HttpClientTimeouts};
use serde::Serialize;
use std::time::Duration;

pub(super) fn more_complete_stream_text(final_text: String, streamed_text: String) -> String {
    if streamed_text.chars().count() > final_text.chars().count() {
        streamed_text
    } else {
        final_text
    }
}

pub(super) async fn prepare_provider_skill_optimization(
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

pub(in crate::api) async fn load_admin_model_runtime(
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

pub(super) async fn execute_provider_skill_optimization(
    prepared: &PreparedProviderSkillOptimization,
    callbacks: StreamCallbacks,
) -> Result<String, ApiError> {
    let client = http_client_builder(
        HttpClientTimeouts::new(Duration::from_secs(600))
            .with_connect_timeout(Duration::from_secs(15))
            .with_read_timeout(Duration::from_secs(600)),
    )
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

pub(in crate::api) async fn update_mcp_provider_skill(
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

pub(super) async fn load_readable_mcp(
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

pub(super) async fn resolve_mcp_descriptor(
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

pub(super) async fn request_user_service<TBody, TResponse>(
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
    let mut request = state
        .user_service_http()
        .request(method, url)
        .bearer_auth(access_token.trim());
    if let Some(body) = body {
        request = request.json(body);
    }
    let response = request
        .send()
        .await
        .map_err(|err| ApiError::bad_gateway(format!("User Service request failed: {err}")))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail =
            read_response_preview_text_limited_or_message(response, ERROR_BODY_PREVIEW_LIMIT_BYTES)
                .await;
        return Err(ApiError::bad_gateway(format!(
            "User Service returned {status}: {detail}"
        )));
    }
    read_response_json_limited::<TResponse>(response, JSON_BODY_LIMIT_BYTES)
        .await
        .map_err(|err| ApiError::bad_gateway(format!("decode User Service response failed: {err}")))
}

pub(super) fn ensure_super_admin(user: &CurrentUser) -> Result<(), ApiError> {
    if user.is_super_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("super admin access is required"))
    }
}

pub(super) fn effective_model_name(model: &AdminAiModelConfig) -> String {
    if model.model.trim().is_empty() {
        model.model_name.trim().to_string()
    } else {
        model.model.trim().to_string()
    }
}

pub(super) fn default_ai_base_url(provider: &str, configured: Option<&str>) -> String {
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

pub(super) fn build_provider_skill_optimizer_system_prompt(
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

pub(super) fn descriptor_skills_from_metadata(
    metadata: &ResourceMetadata,
) -> Vec<McpProviderSkill> {
    metadata
        .extra
        .get("provider_skills")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub(super) fn descriptor_tools_from_metadata(
    metadata: &ResourceMetadata,
) -> Vec<serde_json::Value> {
    metadata
        .extra
        .get("tool_catalog")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}
