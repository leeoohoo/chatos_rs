use super::current_access_token;
use super::dto::MemoryAiModelConfigDto;
use super::http::{
    build_url, client, send_optional_json, send_optional_json_without_service_token,
    timeout_duration,
};

pub async fn get_memory_model_config(
    model_id: &str,
) -> Result<Option<MemoryAiModelConfigDto>, String> {
    let path = if current_access_token().is_some() {
        format!("/configs/models/{}", urlencoding::encode(model_id))
    } else {
        format!("/internal/configs/models/{}", urlencoding::encode(model_id))
    };
    let req = client()
        .get(build_url(path.as_str()).as_str())
        .timeout(timeout_duration());
    if current_access_token().is_some() {
        send_optional_json(req).await
    } else {
        send_optional_json_without_service_token(req).await
    }
}
