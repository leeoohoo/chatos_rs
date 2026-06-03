use serde_json::Value;

use crate::modules::conversation_runtime::guidance::{
    build_runtime_guidance_applied_event, build_runtime_guidance_message_content,
    drain_runtime_guidance_items, resolve_runtime_guidance_locale, RuntimeGuidanceItem,
};
use crate::services::ai_client_common::AiClientCallbacks;
use crate::services::ai_common::is_non_terminal_response_status;

use super::input_transform::{build_current_input_items, to_message_item};

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
            items.push(build_runtime_guidance_input_item(
                &drained_item.guidance_item,
                locale,
                force_text_content,
                model_name.as_str(),
                supports_images,
            )
            .await);
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
    let content = build_runtime_guidance_message_content(
        guidance_item,
        locale,
        model_name,
        supports_images,
    )
    .await;
    to_message_item("system", &content, force_text_content)
}

pub(super) fn prepend_input_items(
    input: &Value,
    prefixed_items: &[Value],
    force_text_content: bool,
) -> Value {
    if prefixed_items.is_empty() {
        return input.clone();
    }
    let mut merged = prefixed_items.to_vec();
    merged.extend(build_current_input_items(input, force_text_content));
    Value::Array(merged)
}

pub(super) fn append_input_items(
    input: &Value,
    appended_items: &[Value],
    force_text_content: bool,
) -> Value {
    if appended_items.is_empty() {
        return input.clone();
    }
    let mut merged = build_current_input_items(input, force_text_content);
    merged.extend_from_slice(appended_items);
    Value::Array(merged)
}

pub(super) fn is_non_terminal_finish_reason(finish_reason: Option<&str>) -> bool {
    is_non_terminal_response_status(finish_reason)
}
