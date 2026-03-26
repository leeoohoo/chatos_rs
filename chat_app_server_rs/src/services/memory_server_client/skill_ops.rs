use super::dto::{MemorySkillDto, MemorySkillPluginDto};
use super::http::{build_url, client, send_optional_json, timeout_duration};

pub async fn get_memory_skill(skill_id: &str) -> Result<Option<MemorySkillDto>, String> {
    let req = client()
        .get(build_url(&format!("/skills/{}", urlencoding::encode(skill_id))).as_str())
        .timeout(timeout_duration());
    send_optional_json(req).await
}

pub async fn get_memory_skill_plugin(source: &str) -> Result<Option<MemorySkillPluginDto>, String> {
    let normalized_source = source.trim();
    if normalized_source.is_empty() {
        return Ok(None);
    }
    let req = client()
        .get(build_url("/skills/plugins/detail").as_str())
        .query(&[("source", normalized_source)])
        .timeout(timeout_duration());
    send_optional_json(req).await
}
