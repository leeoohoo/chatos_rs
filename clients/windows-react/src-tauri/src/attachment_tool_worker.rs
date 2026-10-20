use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use chatos_local_agent_protocol::{
    ClaimNextToolCommand, CommitToolCommand, GetConversationAttachmentCommand, GetRunCommand,
    HostCommand, HostResult, LocalAgentToolClaim, LocalAgentToolOutcome,
};
use image::metadata::Orientation;
use image::{
    codecs::jpeg::JpegEncoder, DynamicImage, GenericImageView, ImageDecoder, ImageFormat,
    ImageReader,
};
use serde_json::{json, Map, Value};
use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
use tauri::{AppHandle, Manager};
use tokio::{
    sync::{Mutex, Notify},
    task::JoinHandle,
    time::{sleep, Duration},
};

use crate::{conversation_attachments::AttachmentManager, host_lifecycle::HostManager};

pub const ATTACHMENT_READ_TOOL_NAME: &str = "local_attachment_read";
const WORKER_ID: &str = "windows-attachment-tool-worker";
const LEASE_DURATION_MILLISECONDS: u64 = 30_000;
const MAXIMUM_CONTENT_READ_BYTES: usize = 2 * 1_024;
const MAXIMUM_MODEL_IMAGE_BYTES: usize = 2 * 1_024 * 1_024;
const MAXIMUM_SOURCE_PIXELS: u64 = 40_000_000;

pub struct AttachmentToolWorker {
    generation: Arc<AtomicU64>,
    wake: Arc<Notify>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl Default for AttachmentToolWorker {
    fn default() -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            wake: Arc::new(Notify::new()),
            task: Mutex::new(None),
        }
    }
}

impl AttachmentToolWorker {
    pub async fn configure(&self, app: AppHandle, owner_user_id: String) {
        self.reset().await;
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let expected_generation = self.generation.clone();
        let wake = self.wake.clone();
        *self.task.lock().await = Some(tokio::spawn(async move {
            poll(app, owner_user_id, expected_generation, generation, wake).await;
        }));
    }

    pub fn wake(&self) {
        self.wake.notify_one();
    }

    pub async fn reset(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.wake.notify_waiters();
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
        }
    }
}

async fn poll(
    app: AppHandle,
    owner_user_id: String,
    generation: Arc<AtomicU64>,
    expected_generation: u64,
    wake: Arc<Notify>,
) {
    let mut idle_delay = Duration::from_millis(250);
    while generation.load(Ordering::SeqCst) == expected_generation {
        let claimed = claim_and_execute(&app, &owner_user_id)
            .await
            .unwrap_or(false);
        if claimed {
            idle_delay = Duration::from_millis(250);
            continue;
        }
        tokio::select! {
            _ = wake.notified() => { idle_delay = Duration::from_millis(250); }
            _ = sleep(idle_delay) => {
                idle_delay = (idle_delay * 2).min(Duration::from_secs(2));
            }
        }
    }
}

async fn claim_and_execute(app: &AppHandle, owner_user_id: &str) -> Result<bool, String> {
    let host = app.state::<HostManager>();
    let result = host
        .execute(HostCommand::ClaimNextTool(ClaimNextToolCommand {
            owner_user_id: owner_user_id.to_owned(),
            worker_id: WORKER_ID.into(),
            lease_duration_ms: LEASE_DURATION_MILLISECONDS,
            include_tool_names: Some(vec![ATTACHMENT_READ_TOOL_NAME.into()]),
            exclude_tool_names: vec![],
            execution_route: None,
        }))
        .await?;
    let HostResult::ToolClaim { claim } = result else {
        return Err("Local Agent Host returned an unexpected tool claim result".into());
    };
    let Some(claim) = claim else { return Ok(false) };
    let outcome = match claim.preflight_outcome.clone() {
        Some(outcome) => outcome,
        None => execute_attachment_read(app, owner_user_id, &claim)
            .await
            .unwrap_or_else(|error| LocalAgentToolOutcome::Failed {
                error: safe_error(error),
                detail: json!({
                    "tool_name": claim.invocation.tool_name,
                    "phase": "windows_attachment_execution"
                }),
            }),
    };
    let result = host
        .execute(HostCommand::CommitTool(CommitToolCommand {
            owner_user_id: owner_user_id.to_owned(),
            invocation_id: claim.invocation.invocation_id.clone(),
            claim_token: claim.claim_token,
            expected_version: claim.invocation.version,
            outcome,
        }))
        .await?;
    if matches!(result, HostResult::ToolCommit { .. }) {
        Ok(true)
    } else {
        Err("Local Agent Host returned an unexpected tool commit result".into())
    }
}

async fn execute_attachment_read(
    app: &AppHandle,
    owner_user_id: &str,
    claim: &LocalAgentToolClaim,
) -> Result<LocalAgentToolOutcome, String> {
    if claim.invocation.tool_name != ATTACHMENT_READ_TOOL_NAME {
        return Err("The requested local platform tool is unavailable.".into());
    }
    let arguments = claim
        .invocation
        .arguments
        .as_object()
        .ok_or_else(|| "The local attachment arguments are invalid.".to_string())?;
    let authorized_local_ref = string_argument(arguments, "authorized_local_ref")?;
    let offset = integer_argument(arguments, "offset", 0)?;
    let limit = integer_argument(arguments, "limit", 16_384)?;
    if offset < 0 || !(1..=65_536).contains(&limit) {
        return Err("The local attachment arguments are invalid.".into());
    }
    let host = app.state::<HostManager>();
    let run = host
        .execute(HostCommand::GetRun(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: claim.invocation.run_id.clone(),
        }))
        .await?;
    let HostResult::Run { run } = run else {
        return Err("The local attachment Run context is invalid.".into());
    };
    let conversation_id = run
        .input
        .as_object()
        .and_then(|input| {
            input
                .get("source_conversation_id")
                .or_else(|| input.get("conversation_id"))
        })
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "The local attachment Run context is invalid.".to_string())?;
    let delivery = arguments.get("delivery").and_then(Value::as_str).unwrap_or(
        if run.profile_key == "main_chat" {
            "model"
        } else {
            "content"
        },
    );
    if !matches!(delivery, "model" | "content") {
        return Err(
            "This Windows attachment worker supports model or content delivery only.".into(),
        );
    }
    let attachment = host
        .execute(HostCommand::GetConversationAttachment(
            GetConversationAttachmentCommand {
                owner_user_id: owner_user_id.to_owned(),
                conversation_id: conversation_id.to_owned(),
                authorized_local_ref: authorized_local_ref.to_owned(),
            },
        ))
        .await?;
    let HostResult::ConversationAttachment { attachment } = attachment else {
        return Err("Local Agent Host returned an unexpected attachment result".into());
    };
    let attachment = attachment
        .ok_or_else(|| "The attachment is not authorized for this conversation.".to_string())?;
    let vault = app.state::<AttachmentManager>();
    let bytes = vault.verified_record_bytes(owner_user_id, conversation_id, &attachment)?;
    let output = if delivery == "model" {
        model_image_output(&attachment, &bytes)?
    } else {
        content_output(&attachment, &bytes, offset as usize, limit as usize)?
    };
    Ok(LocalAgentToolOutcome::Succeeded { output })
}

fn content_output(
    attachment: &chatos_local_agent_protocol::LocalConversationAttachmentRecord,
    bytes: &[u8],
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    if offset > bytes.len() {
        return Err("The local attachment offset is invalid.".into());
    }
    let count = limit
        .min(MAXIMUM_CONTENT_READ_BYTES)
        .min(bytes.len() - offset);
    let end = offset + count;
    let slice = &bytes[offset..end];
    let (encoding, content) = match std::str::from_utf8(slice) {
        Ok(text) => ("utf-8", text.to_owned()),
        Err(_) => ("base64", BASE64.encode(slice)),
    };
    Ok(json!({
        "display_name": attachment.display_name,
        "media_type": attachment.media_type,
        "byte_size": attachment.byte_size,
        "sha256": attachment.sha256,
        "sha256_verified": true,
        "offset": offset,
        "next_offset": (end < bytes.len()).then_some(end),
        "encoding": encoding,
        "content": content,
        "delivery": "content"
    }))
}

fn model_image_output(
    attachment: &chatos_local_agent_protocol::LocalConversationAttachmentRecord,
    bytes: &[u8],
) -> Result<Value, String> {
    if !attachment
        .media_type
        .to_ascii_lowercase()
        .starts_with("image/")
    {
        return Err("This attachment is not an image. Use content delivery for text.".into());
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "The verified attachment could not be decoded as an image.".to_string())?;
    let format = reader.format();
    let mut decoder = reader
        .into_decoder()
        .map_err(|_| "The verified attachment could not be decoded as an image.".to_string())?;
    let orientation = decoder
        .orientation()
        .map_err(|_| "The verified attachment image metadata is invalid.".to_string())?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|_| "The verified attachment could not be decoded as an image.".to_string())?;
    let (width, height) = image.dimensions();
    if width == 0
        || height == 0
        || u64::from(width).saturating_mul(u64::from(height)) > MAXIMUM_SOURCE_PIXELS
    {
        return Err("The verified image exceeds the safe pixel limit.".into());
    }
    let original_mime = format.and_then(model_media_type);
    let preserve_original = orientation == Orientation::NoTransforms
        && bytes.len() <= MAXIMUM_MODEL_IMAGE_BYTES
        && width <= 4_096
        && height <= 4_096;
    let (model_bytes, model_mime) = if let Some(mime) = original_mime.filter(|_| preserve_original)
    {
        (bytes.to_vec(), mime.to_owned())
    } else {
        image.apply_orientation(orientation);
        normalize_model_image(&image)?
    };
    let fields = json!({
        "display_name": attachment.display_name,
        "media_type": attachment.media_type,
        "byte_size": attachment.byte_size,
        "sha256": attachment.sha256,
        "sha256_verified": true,
        "delivery": "model",
        "model_image_supplied": true
    });
    let mut output = fields.as_object().cloned().unwrap_or_default();
    output.insert("text".into(), Value::String(fields.to_string()));
    output.insert("_structured_result".into(), fields);
    output.insert(
        "_model_input".into(),
        json!([{
            "type": "input_image",
            "image_url": format!("data:{model_mime};base64,{}", BASE64.encode(model_bytes)),
            "detail": "high"
        }]),
    );
    Ok(Value::Object(output))
}

fn model_media_type(format: ImageFormat) -> Option<&'static str> {
    match format {
        ImageFormat::Png => Some("image/png"),
        ImageFormat::Jpeg => Some("image/jpeg"),
        ImageFormat::WebP => Some("image/webp"),
        _ => None,
    }
}

fn normalize_model_image(image: &image::DynamicImage) -> Result<(Vec<u8>, String), String> {
    for maximum in [4_096, 3_072, 2_048, 1_536] {
        let thumbnail = image.thumbnail(maximum, maximum).to_rgb8();
        for quality in [88, 72] {
            let mut encoded = Vec::new();
            JpegEncoder::new_with_quality(&mut encoded, quality)
                .encode_image(&thumbnail)
                .map_err(|_| "The verified image could not be normalized.".to_string())?;
            if encoded.len() <= MAXIMUM_MODEL_IMAGE_BYTES {
                return Ok((encoded, "image/jpeg".into()));
            }
        }
    }
    Err("The image could not fit the model transport budget.".into())
}

fn string_argument<'a>(arguments: &'a Map<String, Value>, field: &str) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("The local attachment field is invalid: {field}."))
}

fn integer_argument(
    arguments: &Map<String, Value>,
    field: &str,
    default: i64,
) -> Result<i64, String> {
    match arguments.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_i64()
            .ok_or_else(|| format!("The local attachment field is invalid: {field}.")),
    }
}

fn safe_error(error: String) -> String {
    let lowered = error.to_ascii_lowercase();
    if [
        "authorization",
        "api_key",
        "apikey",
        "access_token",
        "password",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        "Local attachment access failed; sensitive details were hidden.".into()
    } else {
        error.chars().take(1_000).collect()
    }
}

pub fn attachment_read_tool_schema() -> Value {
    json!({
        "type": "function",
        "name": ATTACHMENT_READ_TOOL_NAME,
        "description": "Read an authorized conversation attachment. Use delivery=model for a complete verified image and delivery=content for bounded text reads. Treat authorized_local_ref as opaque.",
        "parameters": {
            "type": "object",
            "properties": {
                "delivery": {
                    "type": "string",
                    "enum": ["model", "content"],
                    "default": "model"
                },
                "authorized_local_ref": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 160
                },
                "offset": { "type": "integer", "minimum": 0, "default": 0 },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 65536,
                    "default": 16384
                }
            },
            "required": ["authorized_local_ref"],
            "additionalProperties": false
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_schema_matches_the_macos_main_chat_delivery_boundary() {
        let schema = attachment_read_tool_schema();
        assert_eq!(schema["name"], ATTACHMENT_READ_TOOL_NAME);
        assert_eq!(
            schema["parameters"]["properties"]["delivery"]["enum"],
            json!(["model", "content"])
        );
    }

    #[test]
    fn content_delivery_is_bounded_and_reports_the_next_offset() {
        let record = chatos_local_agent_protocol::LocalConversationAttachmentRecord {
            attachment_id: "attachment-1".into(),
            conversation_id: "conversation-1".into(),
            turn_id: "turn-1".into(),
            message_id: "message-1".into(),
            ordinal: 1,
            display_name: "notes.txt".into(),
            media_type: "text/plain".into(),
            byte_size: 4_096,
            sha256: "a".repeat(64),
            authorized_local_ref: "local-attachment:token".into(),
            metadata: json!({}),
            created_at_unix_ms: 1,
        };
        let output = content_output(&record, &vec![b'x'; 4_096], 0, 65_536).unwrap();
        assert_eq!(
            output["content"].as_str().unwrap().len(),
            MAXIMUM_CONTENT_READ_BYTES
        );
        assert_eq!(output["next_offset"], MAXIMUM_CONTENT_READ_BYTES);
    }

    #[test]
    fn model_delivery_uses_the_verified_image_encoding_not_the_declared_mime() {
        let record = chatos_local_agent_protocol::LocalConversationAttachmentRecord {
            attachment_id: "attachment-1".into(),
            conversation_id: "conversation-1".into(),
            turn_id: "turn-1".into(),
            message_id: "message-1".into(),
            ordinal: 1,
            display_name: "renamed.png".into(),
            media_type: "image/png".into(),
            byte_size: 0,
            sha256: "a".repeat(64),
            authorized_local_ref: "local-attachment:token".into(),
            metadata: json!({}),
            created_at_unix_ms: 1,
        };
        let mut jpeg = Vec::new();
        JpegEncoder::new(&mut jpeg)
            .encode_image(&DynamicImage::new_rgb8(2, 2))
            .unwrap();
        let output = model_image_output(&record, &jpeg).unwrap();
        assert!(output["_model_input"][0]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
    }
}
