// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;

use serde_json::Value;
use tracing::error;

use crate::utils::attachments::{self, Attachment};

pub(crate) fn normalize_turn_id(turn_id: Option<&str>) -> Option<String> {
    turn_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
}

pub(crate) fn build_user_message_metadata(
    attachments_list: &[Attachment],
    turn_id: Option<&str>,
) -> Option<Value> {
    let sanitized = attachments::sanitize_attachments_for_db(attachments_list);

    if sanitized.is_empty() && turn_id.is_none() {
        return None;
    }

    let mut map = serde_json::Map::new();
    if !sanitized.is_empty() {
        map.insert("attachments".to_string(), Value::Array(sanitized));
    }
    if let Some(turn) = turn_id {
        map.insert(
            "conversation_turn_id".to_string(),
            Value::String(turn.to_string()),
        );
    }

    Some(Value::Object(map))
}

pub(crate) async fn build_user_content_parts(
    model: &str,
    user_message: &str,
    attachments_list: &[Attachment],
    supports_images: Option<bool>,
) -> Value {
    let content_parts =
        attachments::build_content_parts_async(user_message, attachments_list).await;
    attachments::adapt_parts_for_model(model, &content_parts, supports_images)
}

pub(crate) struct PreparedUserMessageInput {
    pub turn_id: Option<String>,
    pub content_parts: Value,
}

pub(crate) async fn persist_user_message_and_build_content_parts<F, Fut, T>(
    session_id: &str,
    user_message: &str,
    model: &str,
    attachments_list: Vec<Attachment>,
    supports_images: Option<bool>,
    turn_id: Option<String>,
    save_user_message: F,
) -> Result<PreparedUserMessageInput, String>
where
    F: FnOnce(Option<Value>) -> Fut,
    Fut: Future<Output = Result<T, String>>,
{
    let metadata = build_user_message_metadata(&attachments_list, turn_id.as_deref());
    save_user_message(metadata).await.map_err(|err| {
        let detail = format!(
            "persist user message failed: session_id={} detail={}",
            session_id, err
        );
        error!("{}", detail);
        detail
    })?;

    let content_parts =
        build_user_content_parts(model, user_message, &attachments_list, supports_images).await;

    Ok(PreparedUserMessageInput {
        turn_id,
        content_parts,
    })
}
