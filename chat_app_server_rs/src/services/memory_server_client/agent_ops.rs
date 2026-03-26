use serde_json::Value;

use super::dto::{
    CreateMemoryAgentRequestDto, MemoryAgentDto, MemoryAgentRuntimeContextDto,
    UpdateMemoryAgentRequestDto,
};
use super::http::{
    build_url, client, push_limit_offset_params, send_delete_result, send_json, send_list,
    send_optional_json, timeout_duration,
};

pub async fn list_memory_agents(
    user_id: Option<&str>,
    enabled: Option<bool>,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<MemoryAgentDto>, String> {
    let mut params: Vec<(String, String)> = Vec::new();
    if let Some(value) = user_id {
        params.push(("user_id".to_string(), value.to_string()));
    }
    if let Some(value) = enabled {
        params.push(("enabled".to_string(), value.to_string()));
    }
    push_limit_offset_params(&mut params, limit, offset);

    send_list("/agents", &params).await
}

pub async fn get_memory_agent(agent_id: &str) -> Result<Option<MemoryAgentDto>, String> {
    let req = client()
        .get(build_url(&format!("/agents/{}", urlencoding::encode(agent_id))).as_str())
        .timeout(timeout_duration());
    send_optional_json(req).await
}

pub async fn create_memory_agent(
    payload: &CreateMemoryAgentRequestDto,
) -> Result<MemoryAgentDto, String> {
    let req = client()
        .post(build_url("/agents").as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_json(req).await
}

pub async fn update_memory_agent(
    agent_id: &str,
    payload: &UpdateMemoryAgentRequestDto,
) -> Result<Option<MemoryAgentDto>, String> {
    let req = client()
        .patch(build_url(&format!("/agents/{}", urlencoding::encode(agent_id))).as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_optional_json(req).await
}

pub async fn delete_memory_agent(agent_id: &str) -> Result<bool, String> {
    let req = client()
        .delete(build_url(&format!("/agents/{}", urlencoding::encode(agent_id))).as_str())
        .timeout(timeout_duration());

    send_delete_result(req).await
}

pub async fn get_memory_agent_runtime_context(
    agent_id: &str,
) -> Result<Option<MemoryAgentRuntimeContextDto>, String> {
    let req = client()
        .get(
            build_url(&format!(
                "/agents/{}/runtime-context",
                urlencoding::encode(agent_id)
            ))
            .as_str(),
        )
        .timeout(timeout_duration());
    send_optional_json(req).await
}

pub async fn ai_create_memory_agent(payload: &Value) -> Result<Value, String> {
    let req = client()
        .post(build_url("/agents/ai-create").as_str())
        .timeout(timeout_duration())
        .json(payload);
    send_json(req).await
}
