use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{guess_format, ImageReader};
use reqwest::Url;
use serde_json::{json, Value};
use std::io::Cursor;

use crate::authentication::GatewayModelConfig;

pub(super) const MAXIMUM_VIDEO_BYTES: usize = 512 * 1_024 * 1_024;
pub(super) const MAXIMUM_VIDEO_JOB_RESPONSE_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_IMAGE_BYTES: usize = 20 * 1_024 * 1_024;
const MAXIMUM_AUDIO_BYTES: usize = 20 * 1_024 * 1_024;
const MINI_MAX_RATIOS: [&str; 6] = ["16:9", "9:16", "1:1", "4:3", "3:4", "21:9"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VideoInput {
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VideoGenerationRequest {
    pub model_config_id: String,
    pub prompt: String,
    pub size: String,
    pub seconds: u8,
    pub ratio: String,
    pub input_image: Option<VideoInput>,
    pub reference_audio: Option<VideoInput>,
    pub client_request_id: String,
}

#[derive(Clone)]
pub(super) struct ResolvedVideoModel {
    pub model_config_id: String,
    pub model_name: String,
    pub base_url: Url,
    pub api_key: String,
}

pub(super) struct ProviderVideoRequest {
    pub url: Url,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ProviderVideoJob {
    pub id: String,
    pub status: String,
    pub progress: Option<f64>,
    pub model_name: Option<String>,
    pub error_message: Option<String>,
    pub content_url: Option<Url>,
}

impl ProviderVideoJob {
    pub fn is_completed(&self) -> bool {
        self.status == "completed"
    }

    pub fn is_failed(&self) -> bool {
        self.status == "failed" || self.error_message.is_some()
    }

    pub fn should_poll(&self) -> bool {
        !self.is_completed() && !self.is_failed()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct VideoGenerationResult {
    pub id: String,
    pub model_config_id: String,
    pub model_name: String,
    pub created_at_unix_ms: u64,
    pub mime_type: String,
    pub video_data: Vec<u8>,
}

pub(super) fn resolve_video_model(
    owner_user_id: &str,
    requested_model_config_id: &str,
    model: GatewayModelConfig,
) -> Result<ResolvedVideoModel, String> {
    if model.owner_user_id != owner_user_id
        || model.id != requested_model_config_id
        || !model.enabled
        || !model.has_api_key
    {
        return Err("模型配置与当前账号不匹配。".into());
    }
    let model_config_id = required("model_config_id", model.id)?;
    let model_name = required("model", model.model)?;
    if !model_name.eq_ignore_ascii_case("MiniMax-H3") {
        return Err(format!(
            "当前视频生成仅支持 MiniMax-H3，模型 {model_name} 暂不支持。"
        ));
    }
    let api_key = model
        .api_key
        .and_then(non_empty)
        .ok_or_else(|| "模型配置没有可用凭据。".to_owned())?;
    let base_url = model
        .base_url
        .and_then(non_empty)
        .ok_or_else(|| "模型配置没有 API 地址。".to_owned())?;
    Ok(ResolvedVideoModel {
        model_config_id,
        model_name,
        base_url: parse_provider_base_url(&base_url)?,
        api_key,
    })
}

pub(super) fn make_video_create_request(
    runtime: &ResolvedVideoModel,
    request: &VideoGenerationRequest,
) -> Result<ProviderVideoRequest, String> {
    validate_request(runtime, request)?;
    let mut metadata = json!({
        "ratio": if request.input_image.is_some() { "adaptive" } else { request.ratio.as_str() },
    });
    if let Some(image) = &request.input_image {
        metadata["first_frame_image"] = Value::String(data_url(image));
    }
    if let Some(audio) = &request.reference_audio {
        metadata["audio_url"] = Value::String(data_url(audio));
    }
    let payload = json!({
        "model": runtime.model_name,
        "prompt": request.prompt.trim(),
        "duration": request.seconds,
        "size": request.size.to_ascii_lowercase(),
        "metadata": metadata,
    });
    Ok(ProviderVideoRequest {
        url: video_create_endpoint(&runtime.base_url)?,
        body: serde_json::to_vec(&payload).map_err(|_| "无法编码视频生成请求。")?,
    })
}

pub(super) fn video_job_endpoint(
    runtime: &ResolvedVideoModel,
    job_id: &str,
) -> Result<Url, String> {
    video_endpoint_with_job(&runtime.base_url, job_id, false)
}

pub(super) fn video_content_endpoint(
    runtime: &ResolvedVideoModel,
    job_id: &str,
) -> Result<Url, String> {
    video_endpoint_with_job(&runtime.base_url, job_id, true)
}

pub(super) fn parse_video_job(body: &[u8], api_key: &str) -> Result<ProviderVideoJob, String> {
    if body.is_empty() || body.len() > MAXIMUM_VIDEO_JOB_RESPONSE_BYTES {
        return Err("模型返回的视频任务数据无效。".into());
    }
    let root: Value =
        serde_json::from_slice(body).map_err(|_| "模型返回的视频任务数据无效。".to_owned())?;
    let id = root
        .get("task_id")
        .or_else(|| root.get("id"))
        .and_then(Value::as_str)
        .and_then(non_empty_ref)
        .filter(|value| value.len() <= 1_024)
        .ok_or_else(|| "模型返回的视频任务数据无效。".to_owned())?
        .to_owned();
    let raw_status = root
        .get("status")
        .and_then(Value::as_str)
        .and_then(non_empty_ref)
        .filter(|value| value.len() <= 128)
        .ok_or_else(|| "模型返回的视频任务数据无效。".to_owned())?
        .to_ascii_lowercase();
    let status = match raw_status.as_str() {
        "queued" | "in_progress" | "unknown" | "completed" | "failed" => raw_status,
        _ => "unknown".into(),
    };
    let progress = root
        .get("progress")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0));
    let explicit_error = root.get("error").filter(|value| !value.is_null());
    let error_message = explicit_error
        .and_then(|value| {
            value
                .as_str()
                .or_else(|| value.get("message").and_then(Value::as_str))
                .or_else(|| value.get("detail").and_then(Value::as_str))
        })
        .and_then(non_empty_ref)
        .map(|value| sanitized_error(value, api_key))
        .or_else(|| explicit_error.map(|_| "模型未提供失败原因".to_owned()));
    let model_name = root
        .get("model")
        .and_then(Value::as_str)
        .and_then(non_empty_ref)
        .filter(|value| value.len() <= 1_024)
        .map(str::to_owned);
    let content_url = root
        .get("metadata")
        .and_then(|value| value.get("url"))
        .and_then(Value::as_str)
        .map(trusted_content_url)
        .transpose()?;
    Ok(ProviderVideoJob {
        id,
        status,
        progress,
        model_name,
        error_message,
        content_url,
    })
}

pub(super) fn make_video_result(
    request: &VideoGenerationRequest,
    runtime: &ResolvedVideoModel,
    job: &ProviderVideoJob,
    mime_type: &str,
    video_data: Vec<u8>,
) -> Result<VideoGenerationResult, String> {
    if !job.is_completed() || video_data.is_empty() || video_data.len() > MAXIMUM_VIDEO_BYTES {
        return Err("模型没有返回可播放的视频文件。".into());
    }
    let mime_type = normalized_video_mime_type(mime_type)?;
    Ok(VideoGenerationResult {
        id: job.id.clone(),
        model_config_id: request.model_config_id.clone(),
        model_name: job
            .model_name
            .clone()
            .unwrap_or_else(|| runtime.model_name.clone()),
        created_at_unix_ms: unix_timestamp_milliseconds(),
        mime_type,
        video_data,
    })
}

fn validate_request(
    runtime: &ResolvedVideoModel,
    request: &VideoGenerationRequest,
) -> Result<(), String> {
    if request.model_config_id != runtime.model_config_id
        || request.client_request_id.trim().is_empty()
        || request.client_request_id.len() > 1_024
    {
        return Err("视频生成请求已失效，请重试。".into());
    }
    let prompt = request.prompt.trim();
    if prompt.is_empty() || prompt.chars().count() > 7_000 {
        return Err("MiniMax 视频提示词不能为空，且不能超过 7000 字符。".into());
    }
    if !matches!(request.size.as_str(), "768P" | "2K")
        || !(4..=15).contains(&request.seconds)
        || !MINI_MAX_RATIOS.contains(&request.ratio.as_str())
    {
        return Err("视频尺寸或时长不受当前协议支持。".into());
    }
    if request.input_image.is_some() && request.reference_audio.is_some() {
        return Err("参考音频不能与首帧同时发送，请重新选择生成方式。".into());
    }
    if let Some(image) = &request.input_image {
        validate_input_image(image)?;
    }
    if let Some(audio) = &request.reference_audio {
        validate_reference_audio(audio)?;
    }
    Ok(())
}

fn validate_input_image(input: &VideoInput) -> Result<(), String> {
    if !matches!(
        input.mime_type.as_str(),
        "image/png" | "image/jpeg" | "image/webp"
    ) || input.bytes.is_empty()
        || input.bytes.len() > MAXIMUM_IMAGE_BYTES
    {
        return Err("参考图无效或超过 20 MB。".into());
    }
    let format = guess_format(&input.bytes).map_err(|_| "参考图无效或超过 20 MB。")?;
    let (width, height) = ImageReader::with_format(Cursor::new(&input.bytes), format)
        .into_dimensions()
        .map_err(|_| "参考图无效或超过 20 MB。")?;
    let ratio = width as f64 / height as f64;
    if !(256..=5_760).contains(&width)
        || !(256..=5_760).contains(&height)
        || !(0.4..=2.5).contains(&ratio)
    {
        return Err("MiniMax 参考图宽高须为 256–5760 像素，宽高比须为 0.4–2.5。".into());
    }
    Ok(())
}

fn validate_reference_audio(input: &VideoInput) -> Result<(), String> {
    if !matches!(
        input.mime_type.as_str(),
        "audio/mpeg"
            | "audio/mp3"
            | "audio/wav"
            | "audio/x-wav"
            | "audio/vnd.wave"
            | "audio/mp4"
            | "audio/x-m4a"
            | "audio/aac"
    ) || input.bytes.is_empty()
        || input.bytes.len() > MAXIMUM_AUDIO_BYTES
    {
        return Err("参考音频必须是有效的 MP3、WAV、M4A 或 AAC，且文件不能超过 20 MB。".into());
    }
    Ok(())
}

fn data_url(input: &VideoInput) -> String {
    format!(
        "data:{};base64,{}",
        input.mime_type.to_ascii_lowercase(),
        BASE64.encode(&input.bytes)
    )
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
    if let Some(index) = value.rfind("/videos/") {
        value.truncate(index);
    } else if value.ends_with("/videos") {
        value.truncate(value.len() - "/videos".len());
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

fn video_create_endpoint(base_url: &Url) -> Result<Url, String> {
    let mut url = base_url.clone();
    url.path_segments_mut()
        .map_err(|_| "模型 API 地址无效。".to_owned())?
        .pop_if_empty()
        .push("videos");
    Ok(url)
}

fn video_endpoint_with_job(
    base_url: &Url,
    job_id: &str,
    include_content: bool,
) -> Result<Url, String> {
    let job_id = non_empty_ref(job_id)
        .filter(|value| value.len() <= 1_024)
        .ok_or_else(|| "视频任务标识无效。".to_owned())?;
    let mut url = video_create_endpoint(base_url)?;
    let mut segments = url
        .path_segments_mut()
        .map_err(|_| "模型 API 地址无效。".to_owned())?;
    segments.push(job_id);
    if include_content {
        segments.push("content");
    }
    drop(segments);
    Ok(url)
}

fn trusted_content_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|_| "模型返回的视频任务数据无效。")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("模型返回的视频任务数据无效。".into());
    }
    Ok(url)
}

fn normalized_video_mime_type(raw: &str) -> Result<String, String> {
    let value = raw
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    match value.as_str() {
        "video/mp4" | "application/mp4" => Ok("video/mp4".into()),
        "video/quicktime" => Ok(value),
        _ => Err("模型没有返回可播放的视频文件。".into()),
    }
}

fn sanitized_error(raw: &str, api_key: &str) -> String {
    let value = if api_key.is_empty() {
        raw.to_owned()
    } else {
        raw.replace(api_key, "[已隐藏]")
    };
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(2_000)
        .collect()
}

fn required(field: &str, value: String) -> Result<String, String> {
    non_empty(value).ok_or_else(|| format!("{field} 不能为空。"))
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

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, ImageFormat, RgbImage};
    use std::io::Cursor;

    fn model(name: &str) -> GatewayModelConfig {
        GatewayModelConfig {
            id: "video-model".into(),
            owner_user_id: "alice".into(),
            name: "Video".into(),
            provider: "openai".into(),
            model: name.into(),
            api_key: Some("must-not-cross".into()),
            base_url: Some("https://relay.example/route/v1/responses".into()),
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

    fn request() -> VideoGenerationRequest {
        VideoGenerationRequest {
            model_config_id: "video-model".into(),
            prompt: "a running dog".into(),
            size: "768P".into(),
            seconds: 4,
            ratio: "16:9".into(),
            input_image: None,
            reference_audio: None,
            client_request_id: "request-1".into(),
        }
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        DynamicImage::ImageRgb8(RgbImage::new(width, height))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn request_matches_macos_unified_video_contract_without_secret() {
        let runtime = resolve_video_model("alice", "video-model", model("MiniMax-H3")).unwrap();
        let wire = make_video_create_request(&runtime, &request()).unwrap();
        assert_eq!(wire.url.as_str(), "https://relay.example/route/v1/videos");
        let payload: Value = serde_json::from_slice(&wire.body).unwrap();
        assert_eq!(payload["model"], "MiniMax-H3");
        assert_eq!(payload["duration"], 4);
        assert_eq!(payload["size"], "768p");
        assert_eq!(payload["metadata"]["ratio"], "16:9");
        assert!(!String::from_utf8(wire.body)
            .unwrap()
            .contains("must-not-cross"));
    }

    #[test]
    fn only_current_owner_exact_h3_and_supported_options_are_admitted() {
        assert!(resolve_video_model("bob", "video-model", model("MiniMax-H3")).is_err());
        assert!(resolve_video_model("alice", "video-model", model("MiniMax-H3-Max")).is_err());
        let runtime = resolve_video_model("alice", "video-model", model("MiniMax-H3")).unwrap();
        for mutate in [
            |request: &mut VideoGenerationRequest| request.size = "1280x720".into(),
            |request: &mut VideoGenerationRequest| request.seconds = 3,
            |request: &mut VideoGenerationRequest| request.ratio = "2:1".into(),
        ] {
            let mut value = request();
            mutate(&mut value);
            assert!(make_video_create_request(&runtime, &value).is_err());
        }
    }

    #[test]
    fn first_frame_and_audio_are_bounded_and_mutually_exclusive() {
        let runtime = resolve_video_model("alice", "video-model", model("MiniMax-H3")).unwrap();
        let mut value = request();
        value.input_image = Some(VideoInput {
            mime_type: "image/png".into(),
            bytes: png(256, 256),
        });
        let payload: Value =
            serde_json::from_slice(&make_video_create_request(&runtime, &value).unwrap().body)
                .unwrap();
        assert_eq!(payload["metadata"]["ratio"], "adaptive");
        assert!(payload["metadata"]["first_frame_image"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));

        value.reference_audio = Some(VideoInput {
            mime_type: "audio/mpeg".into(),
            bytes: b"ID3audio".to_vec(),
        });
        assert!(make_video_create_request(&runtime, &value).is_err());
    }

    #[test]
    fn provider_job_keeps_unknown_pollable_and_rejects_untrusted_content_url() {
        let unknown = parse_video_job(
            br#"{"task_id":"job/1","status":"unknown","progress":12.5}"#,
            "secret",
        )
        .unwrap();
        assert!(unknown.should_poll());
        assert_eq!(
            video_job_endpoint(
                &resolve_video_model("alice", "video-model", model("MiniMax-H3")).unwrap(),
                &unknown.id,
            )
            .unwrap()
            .as_str(),
            "https://relay.example/route/v1/videos/job%2F1"
        );
        assert!(parse_video_job(
            br#"{"id":"job","status":"completed","metadata":{"url":"http://cdn.example/a.mp4"}}"#,
            "secret",
        )
        .is_err());
    }

    #[test]
    fn explicit_provider_error_is_terminal_and_bounded() {
        let body = serde_json::to_vec(&json!({
            "id": "job",
            "status": "in_progress",
            "error": {"message": format!("{}{}", "must-not-cross", "x".repeat(3_000))}
        }))
        .unwrap();
        let job = parse_video_job(&body, "must-not-cross").unwrap();
        assert!(job.is_failed());
        assert!(!job.should_poll());
        let message = job.error_message.unwrap();
        assert!(!message.contains("must-not-cross"));
        assert!(message.chars().count() <= 2_000);
    }

    #[test]
    fn completed_video_result_is_bounded_and_has_safe_mime_type() {
        let runtime = resolve_video_model("alice", "video-model", model("MiniMax-H3")).unwrap();
        let job = parse_video_job(
            br#"{"id":"job","status":"completed","model":"MiniMax-H3"}"#,
            "secret",
        )
        .unwrap();
        let result = make_video_result(
            &request(),
            &runtime,
            &job,
            "video/mp4; charset=binary",
            b"video".to_vec(),
        )
        .unwrap();
        assert_eq!(result.mime_type, "video/mp4");
        assert!(
            make_video_result(&request(), &runtime, &job, "text/html", b"video".to_vec(),).is_err()
        );
    }
}
