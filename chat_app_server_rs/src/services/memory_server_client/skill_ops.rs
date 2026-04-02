use super::dto::{MemorySkillDto, MemorySkillPluginDto};
use super::http::{
    build_url, client, send_optional_json, send_optional_json_without_service_token,
    timeout_duration,
};
use super::current_access_token;

pub async fn get_memory_skill(skill_id: &str) -> Result<Option<MemorySkillDto>, String> {
    let path = if current_access_token().is_some() {
        build_url(&format!("/skills/{}", urlencoding::encode(skill_id)))
    } else {
        build_url(&format!("/internal/skills/{}", urlencoding::encode(skill_id)))
    };
    let req = client()
        .get(path.as_str())
        .timeout(timeout_duration());
    if current_access_token().is_some() {
        send_optional_json(req).await
    } else {
        send_optional_json_without_service_token(req).await
    }
}

pub async fn get_memory_skill_plugin(source: &str) -> Result<Option<MemorySkillPluginDto>, String> {
    let normalized_source = source.trim();
    if normalized_source.is_empty() {
        return Ok(None);
    }
    let path = if current_access_token().is_some() {
        build_url("/skills/plugins/detail")
    } else {
        build_url("/internal/skills/plugins/detail")
    };
    let req = client()
        .get(path.as_str())
        .query(&[("source", normalized_source)])
        .timeout(timeout_duration());
    if current_access_token().is_some() {
        send_optional_json(req).await
    } else {
        send_optional_json_without_service_token(req).await
    }
}
