use super::is_internal_scope;
use super::dto::MemoryAiModelConfigDto;
use super::http::{build_url, client, send_optional_json, timeout_duration};

pub async fn get_memory_model_config(
    model_id: &str,
) -> Result<Option<MemoryAiModelConfigDto>, String> {
    let path = if is_internal_scope() {
        format!("/internal/configs/models/{}", urlencoding::encode(model_id))
    } else {
        format!("/configs/models/{}", urlencoding::encode(model_id))
    };
    let req = client()
        .get(build_url(path.as_str()).as_str())
        .timeout(timeout_duration());
    send_optional_json(req).await
}
