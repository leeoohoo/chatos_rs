// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_ai_runtime::{response_content_has_image_part, to_message_item};
use serde_json::Value;

use crate::modules::conversation_runtime::guidance::{
    build_runtime_guidance_applied_event, build_runtime_guidance_message_content,
    drain_runtime_guidance_items, resolve_runtime_guidance_locale, RuntimeGuidanceItem,
};
use crate::services::ai_client_common::AiClientCallbacks;

pub fn load_runtime_guidance_input_items(
    session_id: Option<&str>,
    turn_id: Option<&str>,
    force_text_content: bool,
    model_name: &str,
    supports_images: Option<bool>,
    callbacks: &AiClientCallbacks,
) -> futures::future::BoxFuture<'static, Vec<Value>> {
    let callbacks = callbacks.clone();
    let session_id = session_id.map(ToOwned::to_owned);
    let turn_id = turn_id.map(ToOwned::to_owned);
    let model_name = model_name.to_string();
    Box::pin(async move {
        let drained = drain_runtime_guidance_items(session_id.as_deref(), turn_id.as_deref());
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
            if let (Some(applied_item), Some(callback)) = (
                drained_item.applied_item,
                callbacks.on_runtime_guidance_applied.as_ref(),
            ) {
                callback(build_runtime_guidance_applied_event(
                    &applied_item,
                    drained_item.pending_count,
                    true,
                ));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::services::runtime_guidance_manager::{RuntimeGuidanceItem, RuntimeGuidanceStatus};

    #[tokio::test]
    async fn text_guidance_is_emitted_as_a_system_message() {
        let item = RuntimeGuidanceItem {
            guidance_id: "guidance-1".to_string(),
            session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            content: "continue with the updated task state".to_string(),
            attachments: Vec::new(),
            status: RuntimeGuidanceStatus::Queued,
            created_at: "2026-07-12T00:00:00Z".to_string(),
            applied_at: None,
            dropped_at: None,
        };

        let input = build_runtime_guidance_input_item(
            &item,
            InternalContextLocale::EnUs,
            false,
            "gpt-4o",
            Some(true),
        )
        .await;

        assert_eq!(input["role"], "system");
        assert!(input.to_string().contains("updated task state"));
    }
}
