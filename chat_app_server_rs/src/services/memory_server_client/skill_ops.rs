use super::dto::{MemorySkillDto, MemorySkillPluginDto};
use super::http::{client, send_optional_json, try_build_url, try_timeout_duration};

pub async fn get_memory_skill(skill_id: &str) -> Result<Option<MemorySkillDto>, String> {
    let req = client()
        .get(try_build_url(&format!(
            "/skills/{}",
            urlencoding::encode(skill_id)
        ))?)
        .timeout(try_timeout_duration()?);
    send_optional_json(req).await
}

pub async fn get_memory_skill_plugin(source: &str) -> Result<Option<MemorySkillPluginDto>, String> {
    let normalized_source = source.trim();
    if normalized_source.is_empty() {
        return Ok(None);
    }
    let req = client()
        .get(try_build_url("/skills/plugins/detail")?)
        .query(&[("source", normalized_source)])
        .timeout(try_timeout_duration()?);
    send_optional_json(req).await
}
