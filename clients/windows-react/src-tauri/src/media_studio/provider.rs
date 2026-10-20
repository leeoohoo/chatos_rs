use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::authentication::GatewayModelConfig;

const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_PROVIDER_RESPONSE_BYTES: usize = 48 * 1_024 * 1_024;
const MAXIMUM_GENERATED_IMAGES: usize = 10;
const MAXIMUM_REFERENCE_IMAGES: usize = 8;
const MAXIMUM_ERROR_DETAIL_CHARACTERS: usize = 2_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageGenerationInput {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageGenerationRequest {
    pub model_config_id: String,
    pub prompt: String,
    pub size: Option<String>,
    pub count: u8,
    pub reference_images: Vec<ImageGenerationInput>,
    pub client_request_id: String,
}

pub(crate) struct ResolvedImageModel {
    pub model_config_id: String,
    pub model_name: String,
    pub base_url: Url,
    pub api_key: String,
}

pub(crate) struct ProviderImageRequest {
    pub url: Url,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratedImageAsset {
    pub id: String,
    pub mime_type: String,
    pub base64_data: Option<String>,
    pub url: Option<String>,
    pub revised_prompt: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ImageGenerationResult {
    pub id: String,
    pub model_config_id: String,
    pub model_name: String,
    pub created_at_unix_ms: u64,
    pub images: Vec<GeneratedImageAsset>,
    pub client_request_id: String,
}

#[derive(Deserialize)]
struct ProviderImageResponse {
    id: Option<String>,
    model: Option<String>,
    data: Vec<ProviderImageItem>,
}

#[derive(Deserialize)]
struct ProviderImageItem {
    id: Option<String>,
    b64_json: Option<String>,
    base64: Option<String>,
    url: Option<String>,
    mime_type: Option<String>,
    revised_prompt: Option<String>,
}

pub(crate) fn resolve_image_model(
    owner_user_id: &str,
    requested_model_config_id: &str,
    model: GatewayModelConfig,
) -> Result<ResolvedImageModel, String> {
    if model.owner_user_id != owner_user_id
        || model.id != requested_model_config_id
        || !model.enabled
        || !model.has_api_key
    {
        return Err("模型配置与当前账号不匹配。".into());
    }
    let model_config_id = required("model_config_id", model.id)?;
    let model_name = required("model", model.model)?;
    let api_key = model
        .api_key
        .and_then(non_empty)
        .ok_or_else(|| "模型配置没有可用凭据。".to_owned())?;
    let base_url = model
        .base_url
        .and_then(non_empty)
        .ok_or_else(|| "模型配置没有 API 地址。".to_owned())?;
    let base_url = parse_provider_base_url(&base_url)?;
    Ok(ResolvedImageModel {
        model_config_id,
        model_name,
        base_url,
        api_key,
    })
}

pub(crate) fn make_provider_request(
    runtime: &ResolvedImageModel,
    request: &ImageGenerationRequest,
) -> Result<ProviderImageRequest, String> {
    validate_generation_request(request)?;
    if request.model_config_id != runtime.model_config_id {
        return Err("所选模型已变化，请重试。".into());
    }
    let operation = if request.reference_images.is_empty() {
        "generations"
    } else {
        "edits"
    };
    let url = provider_endpoint(&runtime.base_url, operation)?;
    if request.reference_images.is_empty() {
        let mut payload = json!({
            "model": runtime.model_name,
            "prompt": request.prompt.trim(),
            "n": request.count,
        });
        if let Some(size) = request.size.as_deref().and_then(non_empty_ref) {
            payload["size"] = json!(size);
        }
        let body = serde_json::to_vec(&payload).map_err(|_| "无法编码图片生成请求。")?;
        return Ok(ProviderImageRequest {
            url,
            content_type: "application/json".into(),
            body,
        });
    }

    let boundary = format!("ChatOSMediaBoundary{}", Uuid::new_v4().simple());
    let mut body = Vec::new();
    append_field(&mut body, &boundary, "model", &runtime.model_name);
    append_field(&mut body, &boundary, "prompt", request.prompt.trim());
    append_field(&mut body, &boundary, "n", &request.count.to_string());
    if let Some(size) = request.size.as_deref().and_then(non_empty_ref) {
        append_field(&mut body, &boundary, "size", size);
    }
    for image in &request.reference_images {
        let bytes = validated_reference_image(image)?;
        let field_name = if request.reference_images.len() > 1 {
            "image[]"
        } else {
            "image"
        };
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!(
                "Content-Disposition: form-data; name=\"{field_name}\"; filename=\"{}\"\r\n",
                sanitized_file_name(&image.name, &image.mime_type)
            )
            .as_bytes(),
        );
        body.extend_from_slice(
            format!(
                "Content-Type: {}\r\n\r\n",
                image.mime_type.to_ascii_lowercase()
            )
            .as_bytes(),
        );
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    Ok(ProviderImageRequest {
        url,
        content_type: format!("multipart/form-data; boundary={boundary}"),
        body,
    })
}

pub(crate) fn parse_provider_response(
    request: &ImageGenerationRequest,
    runtime: &ResolvedImageModel,
    body: &[u8],
) -> Result<ImageGenerationResult, String> {
    if body.len() > MAXIMUM_PROVIDER_RESPONSE_BYTES {
        return Err("模型返回的数据过大。".into());
    }
    let payload: ProviderImageResponse =
        serde_json::from_slice(body).map_err(|_| "模型返回的数据格式无法识别。")?;
    if payload.data.is_empty() || payload.data.len() > MAXIMUM_GENERATED_IMAGES {
        return Err("模型返回的数据格式无法识别。".into());
    }
    let result_id = payload
        .id
        .and_then(non_empty)
        .unwrap_or_else(|| request.client_request_id.clone());
    let mut images = Vec::with_capacity(payload.data.len());
    for (index, item) in payload.data.into_iter().enumerate() {
        let id = item
            .id
            .and_then(non_empty)
            .unwrap_or_else(|| format!("{result_id}:image:{index}"));
        let mime_type = normalized_generated_mime_type(item.mime_type)?;
        if let Some(base64_data) = item.b64_json.or(item.base64) {
            let decoded = BASE64
                .decode(&base64_data)
                .map_err(|_| "模型返回的数据格式无法识别。")?;
            if decoded.is_empty() || decoded.len() > MAXIMUM_IMAGE_BYTES {
                return Err("模型返回的数据格式无法识别。".into());
            }
            images.push(GeneratedImageAsset {
                id,
                mime_type,
                base64_data: Some(base64_data),
                url: None,
                revised_prompt: item.revised_prompt.and_then(non_empty),
            });
            continue;
        }
        if let Some(url) = item.url.and_then(non_empty) {
            let parsed = Url::parse(&url).map_err(|_| "模型返回的数据格式无法识别。")?;
            if parsed.scheme() != "https"
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
            {
                return Err("模型返回的数据格式无法识别。".into());
            }
            images.push(GeneratedImageAsset {
                id,
                mime_type,
                base64_data: None,
                url: Some(url),
                revised_prompt: item.revised_prompt.and_then(non_empty),
            });
            continue;
        }
        return Err("模型返回的数据格式无法识别。".into());
    }
    Ok(ImageGenerationResult {
        id: result_id,
        model_config_id: request.model_config_id.clone(),
        model_name: payload
            .model
            .and_then(non_empty)
            .unwrap_or_else(|| runtime.model_name.clone()),
        created_at_unix_ms: unix_timestamp_milliseconds(),
        images,
        client_request_id: request.client_request_id.clone(),
    })
}

fn validate_generation_request(request: &ImageGenerationRequest) -> Result<(), String> {
    required_ref("model_config_id", &request.model_config_id)?;
    required_ref("prompt", &request.prompt)?;
    required_ref("client_request_id", &request.client_request_id)?;
    if !(1..=10).contains(&request.count)
        || request.reference_images.len() > MAXIMUM_REFERENCE_IMAGES
    {
        return Err("图片生成参数无效。".into());
    }
    for image in &request.reference_images {
        let _ = validated_reference_image(image)?;
    }
    Ok(())
}

fn normalized_generated_mime_type(value: Option<String>) -> Result<String, String> {
    let value = value
        .and_then(non_empty)
        .unwrap_or_else(|| "image/png".into())
        .to_ascii_lowercase();
    match value.as_str() {
        "image/png" | "image/jpeg" | "image/webp" | "image/gif" => Ok(value),
        "image/jpg" => Ok("image/jpeg".into()),
        _ => Err("模型返回的数据格式无法识别。".into()),
    }
}

fn validated_reference_image(image: &ImageGenerationInput) -> Result<&[u8], String> {
    let mime_type = image.mime_type.trim().to_ascii_lowercase();
    if !matches!(
        mime_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp"
    ) {
        return Err("参考图无效或超过 20 MB。".into());
    }
    if image.bytes.is_empty() || image.bytes.len() > MAXIMUM_IMAGE_BYTES {
        return Err("参考图无效或超过 20 MB。".into());
    }
    Ok(&image.bytes)
}

fn parse_provider_base_url(raw: &str) -> Result<Url, String> {
    let mut value = raw.trim().trim_end_matches('/').to_owned();
    for suffix in [
        "/images/generations",
        "/images/edits",
        "/chat/completions",
        "/responses",
    ] {
        if value.ends_with(suffix) {
            value.truncate(value.len() - suffix.len());
            break;
        }
    }
    let parsed = Url::parse(&value).map_err(|_| "模型 API 地址无效。")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("模型 API 地址无效。".into());
    }
    Ok(parsed)
}

fn provider_endpoint(base_url: &Url, operation: &str) -> Result<Url, String> {
    let mut url = base_url.clone();
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| "模型 API 地址无效。".to_owned())?;
        segments.pop_if_empty();
        segments.push("images");
        segments.push(operation);
    }
    Ok(url)
}

fn append_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

fn sanitized_file_name(name: &str, mime_type: &str) -> String {
    let extension = match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "png",
    };
    let stem = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .chars()
        .filter(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_'))
        .collect::<String>();
    format!(
        "{}.{}",
        if stem.is_empty() { "input" } else { &stem },
        extension
    )
}

fn required(field: &str, value: String) -> Result<String, String> {
    non_empty(value).ok_or_else(|| format!("{field} 不能为空。"))
}

fn required_ref<'a>(field: &str, value: &'a str) -> Result<&'a str, String> {
    non_empty_ref(value).ok_or_else(|| format!("{field} 不能为空。"))
}

fn non_empty(value: String) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn non_empty_ref(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn unix_timestamp_milliseconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub(crate) fn provider_error_detail(body: &[u8], api_key: &str) -> String {
    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) {
        if let Some(message) = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .and_then(non_empty_ref)
        {
            return sanitized_provider_error(message, api_key);
        }
        if let Some(message) = value
            .get("message")
            .and_then(serde_json::Value::as_str)
            .and_then(non_empty_ref)
        {
            return sanitized_provider_error(message, api_key);
        }
    }
    sanitized_provider_error(&String::from_utf8_lossy(body), api_key)
}

fn sanitized_provider_error(raw: &str, api_key: &str) -> String {
    let mut detail = raw.to_owned();
    if !api_key.is_empty() {
        detail = detail.replace(api_key, "[已隐藏]");
    }
    let detail = detail
        .chars()
        .filter(|value| !value.is_control() || matches!(value, '\n' | '\r' | '\t'))
        .take(MAXIMUM_ERROR_DETAIL_CHARACTERS)
        .collect::<String>();
    let detail = detail.trim();
    if detail.is_empty() {
        "响应正文为空，未提供具体错误原因。".into()
    } else {
        detail.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gateway_model() -> GatewayModelConfig {
        GatewayModelConfig {
            id: "image-model".into(),
            owner_user_id: "alice".into(),
            name: "Image".into(),
            provider: "openai".into(),
            model: "gpt-image-1".into(),
            api_key: Some("must-not-cross".into()),
            base_url: Some("https://provider.example/v1/responses".into()),
            task_thinking_level: None,
            temperature: None,
            max_output_tokens: None,
            enabled: true,
            task_enabled: Some(false),
            has_api_key: true,
            supports_images: true,
            supports_reasoning: false,
            supports_responses: true,
        }
    }

    fn request(reference_images: Vec<ImageGenerationInput>) -> ImageGenerationRequest {
        ImageGenerationRequest {
            model_config_id: "image-model".into(),
            prompt: "quiet lake".into(),
            size: Some("1024x1024".into()),
            count: 1,
            reference_images,
            client_request_id: "request-1".into(),
        }
    }

    #[test]
    fn runtime_secret_stays_in_native_model_and_endpoint_is_normalized() {
        let runtime = resolve_image_model("alice", "image-model", gateway_model()).unwrap();
        assert_eq!(runtime.api_key, "must-not-cross");
        let wire = make_provider_request(&runtime, &request(Vec::new())).unwrap();
        assert_eq!(
            wire.url.as_str(),
            "https://provider.example/v1/images/generations"
        );
        assert_eq!(wire.content_type, "application/json");
        assert!(!String::from_utf8(wire.body)
            .unwrap()
            .contains("must-not-cross"));
    }

    #[test]
    fn multipart_edit_preserves_multiple_reference_order_and_sanitizes_names() {
        let runtime = resolve_image_model("alice", "image-model", gateway_model()).unwrap();
        let png = BASE64.encode(b"valid-image");
        let wire = make_provider_request(
            &runtime,
            &request(vec![
                ImageGenerationInput {
                    name: "first evil.png".into(),
                    mime_type: "image/png".into(),
                    bytes: BASE64.decode(&png).unwrap(),
                },
                ImageGenerationInput {
                    name: "second.webp".into(),
                    mime_type: "image/webp".into(),
                    bytes: BASE64.decode(&png).unwrap(),
                },
            ]),
        )
        .unwrap();
        assert_eq!(
            wire.url.as_str(),
            "https://provider.example/v1/images/edits"
        );
        let body = String::from_utf8_lossy(&wire.body);
        assert_eq!(body.matches("name=\"image[]\"").count(), 2);
        assert!(body.contains("filename=\"firstevil.png\""));
        assert!(body.contains("filename=\"second.webp\""));
    }

    #[test]
    fn provider_response_accepts_bounded_base64_and_https_only_urls() {
        let runtime = resolve_image_model("alice", "image-model", gateway_model()).unwrap();
        let request = request(Vec::new());
        let body = serde_json::to_vec(&json!({
            "id": "result-1",
            "model": "gpt-image-1",
            "data": [
                {"id": "inline", "b64_json": BASE64.encode(b"image"), "mime_type": "image/png"},
                {"id": "remote", "url": "https://cdn.example/image.png", "mime_type": "image/png"}
            ]
        }))
        .unwrap();
        let result = parse_provider_response(&request, &runtime, &body).unwrap();
        assert_eq!(result.images.len(), 2);

        let insecure = serde_json::to_vec(&json!({
            "data": [{"url": "http://cdn.example/image.png"}]
        }))
        .unwrap();
        assert!(parse_provider_response(&request, &runtime, &insecure).is_err());
    }

    #[test]
    fn provider_response_and_error_projection_are_bounded_and_secret_free() {
        let runtime = resolve_image_model("alice", "image-model", gateway_model()).unwrap();
        let request = request(Vec::new());
        let too_many = serde_json::to_vec(&json!({
            "data": (0..=MAXIMUM_GENERATED_IMAGES)
                .map(|_| json!({"b64_json": BASE64.encode(b"image")}))
                .collect::<Vec<_>>()
        }))
        .unwrap();
        assert!(parse_provider_response(&request, &runtime, &too_many).is_err());

        let body = serde_json::to_vec(&json!({
            "error": {"message": format!("credential={}\u{0}{}", runtime.api_key, "x".repeat(3_000))}
        }))
        .unwrap();
        let detail = provider_error_detail(&body, &runtime.api_key);
        assert!(!detail.contains(&runtime.api_key));
        assert!(!detail.contains('\0'));
        assert!(detail.chars().count() <= MAXIMUM_ERROR_DETAIL_CHARACTERS);
    }
}
