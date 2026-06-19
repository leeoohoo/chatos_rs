use chatos_ai_runtime::response_content_has_image_part;
use serde_json::Value;

use crate::modules::conversation_runtime::guidance::{
    RuntimeGuidanceItem, build_runtime_guidance_applied_event,
    build_runtime_guidance_message_content, drain_runtime_guidance_items,
    resolve_runtime_guidance_locale,
};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::is_non_terminal_response_status;

use super::input_transform::to_message_item;

pub(super) use chatos_ai_runtime::append_input_items;

pub(super) fn load_runtime_guidance_input_items(
    session_id: Option<&str>,
    turn_id: Option<&str>,
    force_text_content: bool,
    model_name: &str,
    supports_images: Option<bool>,
    callbacks: &AiClientCallbacks,
) -> futures::future::BoxFuture<'static, Vec<Value>> {
    let callbacks = callbacks.clone();
    let session_id = session_id.map(|value| value.to_string());
    let turn_id = turn_id.map(|value| value.to_string());
    let model_name = model_name.to_string();
    Box::pin(async move {
        let drained = drain_runtime_guidance_items(session_id.as_deref(), turn_id.as_deref());
        if drained.is_empty() {
            return Vec::new();
        }

        let mut items = Vec::with_capacity(drained.len());
        for drained_item in drained {
            let locale = resolve_runtime_guidance_locale(&drained_item.guidance_item).await;
            items.push(
                build_runtime_guidance_input_item(
                    &drained_item.guidance_item,
                    locale,
                    force_text_content,
                    model_name.as_str(),
                    supports_images,
                )
                .await,
            );
            if let Some(applied_item) = drained_item.applied_item {
                if let Some(cb) = &callbacks.on_runtime_guidance_applied {
                    cb(build_runtime_guidance_applied_event(
                        &applied_item,
                        drained_item.pending_count,
                        true,
                    ));
                }
            }
        }

        items
    })
}

async fn build_runtime_guidance_input_item(
    guidance_item: &RuntimeGuidanceItem,
    locale: crate::core::internal_context_locale::InternalContextLocale,
    force_text_content: bool,
    model_name: &str,
    supports_images: Option<bool>,
) -> Value {
    let content =
        build_runtime_guidance_message_content(guidance_item, locale, model_name, supports_images)
            .await;
    let role = if response_content_has_image_part(&content) {
        "user"
    } else {
        "system"
    };
    to_message_item(role, &content, force_text_content)
}

pub(super) fn is_non_terminal_finish_reason(finish_reason: Option<&str>) -> bool {
    is_non_terminal_response_status(finish_reason)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{append_input_items, build_runtime_guidance_input_item};
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::services::runtime_guidance_manager::{RuntimeGuidanceItem, RuntimeGuidanceStatus};
    use crate::utils::attachments::Attachment;

    fn sample_guidance_item(attachments: Vec<Attachment>) -> RuntimeGuidanceItem {
        RuntimeGuidanceItem {
            guidance_id: "gd_test_1".to_string(),
            session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            content: "please inspect this screenshot".to_string(),
            attachments,
            status: RuntimeGuidanceStatus::Queued,
            created_at: "2026-06-04T02:51:05Z".to_string(),
            applied_at: None,
            dropped_at: None,
        }
    }

    #[tokio::test]
    async fn image_guidance_uses_user_role() {
        let item = sample_guidance_item(vec![Attachment {
            name: Some("screenshot.png".to_string()),
            mime_type: Some("image/png".to_string()),
            size: Some(32),
            data_url: Some("data:image/png;base64,Zm9v".to_string()),
            r#type: Some("image".to_string()),
            ..Attachment::default()
        }]);

        let payload = build_runtime_guidance_input_item(
            &item,
            InternalContextLocale::ZhCn,
            false,
            "gpt-4o",
            Some(true),
        )
        .await;

        assert_eq!(
            payload.get("role").and_then(|value| value.as_str()),
            Some("user")
        );
        let content = payload
            .get("content")
            .and_then(|value| value.as_array())
            .expect("guidance content should be response content parts");
        assert!(
            content.iter().any(
                |part| part.get("type").and_then(|value| value.as_str()) == Some("input_image")
            )
        );
    }

    #[tokio::test]
    async fn text_guidance_stays_system_role() {
        let item = sample_guidance_item(Vec::new());

        let payload = build_runtime_guidance_input_item(
            &item,
            InternalContextLocale::ZhCn,
            false,
            "gpt-4o",
            Some(true),
        )
        .await;

        assert_eq!(
            payload.get("role").and_then(|value| value.as_str()),
            Some("system")
        );
    }

    #[test]
    fn append_input_items_force_text_normalizes_appended_messages() {
        let appended = json!({
            "role": "user",
            "type": "message",
            "content": [
                {"type": "input_text", "text": "runtime guidance"},
                {"type": "input_image", "image_url": "data:image/png;base64,Zm9v"}
            ]
        });

        let payload = append_input_items(&Value::String("current".to_string()), &[appended], true);
        let items = payload.as_array().expect("input should be a message list");
        let appended_content = items[1]
            .get("content")
            .and_then(|value| value.as_str())
            .expect("force text should convert appended message content to string");

        assert!(appended_content.contains("runtime guidance"));
        assert!(appended_content.contains("[image:data:image/png;base64,Zm9v]"));
    }
}
