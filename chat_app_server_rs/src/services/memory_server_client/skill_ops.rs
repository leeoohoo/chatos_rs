use super::dto::MemorySkillDto;
use super::http::{build_url, client, send_optional_json, timeout_duration};

pub async fn get_memory_skill(skill_id: &str) -> Result<Option<MemorySkillDto>, String> {
    let req = client()
        .get(build_url(&format!("/skills/{}", urlencoding::encode(skill_id))).as_str())
        .timeout(timeout_duration());
    send_optional_json(req).await
}
