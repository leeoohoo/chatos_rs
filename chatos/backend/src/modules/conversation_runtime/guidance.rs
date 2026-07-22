// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};

use crate::core::internal_context_locale::InternalContextLocale;
use crate::services::ai_common::build_user_content_parts;
use crate::services::runtime_guidance_manager::{runtime_guidance_manager, DEFAULT_DRAIN_LIMIT};
use crate::utils::attachments::Attachment;

use super::user_context::load_runtime_user_context;

pub use crate::services::runtime_guidance_manager::{EnqueueGuidanceError, RuntimeGuidanceItem};

#[derive(Debug, Clone)]
pub struct DrainedRuntimeGuidance {
    pub guidance_item: RuntimeGuidanceItem,
    pub applied_item: Option<RuntimeGuidanceItem>,
    pub pending_count: usize,
}

pub fn register_active_turn(session_id: &str, turn_id: &str) {
    runtime_guidance_manager().register_active_turn(session_id, turn_id);
}

pub fn close_active_turn(session_id: &str, turn_id: &str) {
    runtime_guidance_manager().close_turn(session_id, turn_id);
}

pub fn enqueue_runtime_guidance(
    session_id: &str,
    turn_id: &str,
    content: &str,
) -> Result<RuntimeGuidanceItem, EnqueueGuidanceError> {
    enqueue_runtime_guidance_with_attachments(session_id, turn_id, content, Vec::new())
}

pub fn enqueue_runtime_guidance_with_attachments(
    session_id: &str,
    turn_id: &str,
    content: &str,
    attachments: Vec<Attachment>,
) -> Result<RuntimeGuidanceItem, EnqueueGuidanceError> {
    runtime_guidance_manager().enqueue_guidance(session_id, turn_id, content, attachments)
}

pub fn drain_runtime_guidance_items(
    session_id: Option<&str>,
    turn_id: Option<&str>,
) -> Vec<DrainedRuntimeGuidance> {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };
    let Some(turn_id) = turn_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Vec::new();
    };

    let drained =
        runtime_guidance_manager().drain_guidance(session_id, turn_id, DEFAULT_DRAIN_LIMIT);
    if drained.is_empty() {
        return Vec::new();
    }

    let mut drained_items = Vec::with_capacity(drained.len());
    for guidance_item in drained {
        let applied_item = runtime_guidance_manager().mark_applied(
            session_id,
            turn_id,
            &guidance_item.guidance_id,
        );
        drained_items.push(DrainedRuntimeGuidance {
            guidance_item,
            applied_item,
            pending_count: runtime_guidance_manager().pending_count(session_id, turn_id),
        });
    }

    drained_items
}

pub async fn resolve_runtime_guidance_locale(
    guidance_item: &RuntimeGuidanceItem,
) -> InternalContextLocale {
    load_runtime_user_context(None, guidance_item.session_id.as_str())
        .await
        .internal_context_locale
}

pub fn format_runtime_guidance_instruction(
    guidance_item: &RuntimeGuidanceItem,
    locale: InternalContextLocale,
) -> String {
    if locale.is_english() {
        format!(
            "[Runtime Guidance]\n- source: user guidance during running turn\n- instruction: {}\n- rule: treat this as high-priority preference unless conflicts with safety",
            guidance_item.content
        )
    } else {
        format!(
            "[Runtime Guidance]\n- source: 用户在运行中追加的指导\n- instruction: {}\n- rule: 将其视为高优先级偏好，除非与安全要求冲突",
            guidance_item.content
        )
    }
}

pub async fn build_runtime_guidance_message_content(
    guidance_item: &RuntimeGuidanceItem,
    locale: InternalContextLocale,
    model_name: &str,
    supports_images: Option<bool>,
) -> Value {
    if guidance_item.attachments.is_empty() {
        return Value::String(format_runtime_guidance_instruction(guidance_item, locale));
    }

    let mut parts = vec![json!({
        "type": "text",
        "text": format_runtime_guidance_attachment_prelude(locale),
    })];
    let guidance_parts = build_user_content_parts(
        model_name,
        guidance_item.content.as_str(),
        guidance_item.attachments.as_slice(),
        supports_images,
    )
    .await;

    match guidance_parts {
        Value::String(text) => {
            if !text.trim().is_empty() {
                parts.push(json!({ "type": "text", "text": text }));
            }
        }
        Value::Array(items) => parts.extend(items),
        other => {
            if !other.is_null() {
                parts.push(json!({ "type": "text", "text": other.to_string() }));
            }
        }
    }

    Value::Array(parts)
}

pub fn build_runtime_guidance_applied_event(
    applied_item: &RuntimeGuidanceItem,
    pending_count: usize,
    include_conversation_id: bool,
) -> Value {
    let mut payload = json!({
        "guidance_id": applied_item.guidance_id,
        "turn_id": applied_item.turn_id,
        "status": "applied",
        "created_at": applied_item.created_at,
        "applied_at": applied_item.applied_at,
        "pending_count": pending_count,
    });
    if include_conversation_id {
        payload["conversation_id"] = Value::String(applied_item.session_id.clone());
    }
    payload
}

fn format_runtime_guidance_attachment_prelude(locale: InternalContextLocale) -> String {
    if locale.is_english() {
        "[Runtime Guidance]\n- source: user guidance during running turn\n- rule: treat the following text and attachments as a high-priority preference unless conflicts with safety".to_string()
    } else {
        "[Runtime Guidance]\n- source: 用户在运行中追加的指导\n- rule: 将下面的文本和附件视为高优先级偏好，除非与安全要求冲突".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_runtime_guidance_applied_event, build_runtime_guidance_message_content,
        format_runtime_guidance_instruction,
    };
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::services::runtime_guidance_manager::{RuntimeGuidanceItem, RuntimeGuidanceStatus};
    use crate::utils::attachments::Attachment;

    fn sample_item() -> RuntimeGuidanceItem {
        RuntimeGuidanceItem {
            guidance_id: "gd_test_1".to_string(),
            session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            content: "continue with the current task".to_string(),
            attachments: Vec::new(),
            status: RuntimeGuidanceStatus::Applied,
            created_at: "2026-04-27T12:00:00Z".to_string(),
            applied_at: Some("2026-04-27T12:00:05Z".to_string()),
            dropped_at: None,
        }
    }

    #[test]
    fn formats_runtime_guidance_instruction_with_core_fields() {
        let formatted =
            format_runtime_guidance_instruction(&sample_item(), InternalContextLocale::EnUs);
        assert!(!formatted.contains("gd_test_1"));
        assert!(!formatted.contains("2026-04-27T12:00:00Z"));
        assert!(formatted.contains("continue with the current task"));
        assert!(formatted.contains("high-priority preference"));
    }

    #[test]
    fn formats_runtime_guidance_instruction_in_chinese() {
        let formatted =
            format_runtime_guidance_instruction(&sample_item(), InternalContextLocale::ZhCn);
        assert!(formatted.contains("用户在运行中追加的指导"));
        assert!(formatted.contains("将其视为高优先级偏好"));
    }

    #[test]
    fn applied_event_can_include_conversation_id() {
        let payload = build_runtime_guidance_applied_event(&sample_item(), 2, true);
        assert_eq!(
            payload
                .get("conversation_id")
                .and_then(|value| value.as_str()),
            Some("session-1")
        );
        assert_eq!(
            payload
                .get("pending_count")
                .and_then(|value| value.as_u64()),
            Some(2)
        );
    }

    #[test]
    fn applied_event_can_omit_conversation_id() {
        let payload = build_runtime_guidance_applied_event(&sample_item(), 0, false);
        assert!(payload.get("conversation_id").is_none());
        assert_eq!(
            payload.get("guidance_id").and_then(|value| value.as_str()),
            Some("gd_test_1")
        );
    }

    #[tokio::test]
    async fn builds_guidance_message_content_with_attachments() {
        let mut item = sample_item();
        item.content = String::new();
        item.attachments = vec![Attachment {
            name: Some("diagram.png".to_string()),
            mime_type: Some("image/png".to_string()),
            size: Some(32),
            data_url: Some("data:image/png;base64,Zm9v".to_string()),
            ..Attachment::default()
        }];

        let payload = build_runtime_guidance_message_content(
            &item,
            InternalContextLocale::ZhCn,
            "gpt-4o-mini",
            Some(false),
        )
        .await;

        let parts = payload
            .as_array()
            .expect("guidance content should be array");
        assert!(parts.iter().any(|part| {
            part.get("text")
                .and_then(|value| value.as_str())
                .map(|text| text.contains("高优先级偏好"))
                .unwrap_or(false)
        }));
        assert!(parts.iter().any(|part| {
            part.get("text")
                .and_then(|value| value.as_str())
                .map(|text| text.contains("model does not support images"))
                .unwrap_or(false)
        }));
    }
}
