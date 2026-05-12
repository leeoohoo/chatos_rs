use serde_json::{json, Value};

use crate::core::chat_context::resolve_effective_user_id;
use crate::core::internal_context_locale::{
    internal_context_locale_from_settings, InternalContextLocale,
};
use crate::services::user_settings::get_effective_user_settings;

use super::{runtime_guidance_manager, RuntimeGuidanceItem, DEFAULT_DRAIN_LIMIT};

#[derive(Debug, Clone)]
pub(crate) struct DrainedRuntimeGuidance {
    pub(crate) guidance_item: RuntimeGuidanceItem,
    pub(crate) applied_item: Option<RuntimeGuidanceItem>,
    pub(crate) pending_count: usize,
}

pub(crate) fn drain_runtime_guidance_items(
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
        let applied_item =
            runtime_guidance_manager().mark_applied(session_id, turn_id, &guidance_item.guidance_id);
        drained_items.push(DrainedRuntimeGuidance {
            guidance_item,
            applied_item,
            pending_count: runtime_guidance_manager().pending_count(session_id, turn_id),
        });
    }

    drained_items
}

pub(crate) async fn resolve_runtime_guidance_locale(
    guidance_item: &RuntimeGuidanceItem,
) -> InternalContextLocale {
    let effective_user_id = resolve_effective_user_id(None, guidance_item.session_id.as_str()).await;
    let effective_settings = get_effective_user_settings(effective_user_id)
        .await
        .unwrap_or_else(|_| json!({}));
    internal_context_locale_from_settings(&effective_settings)
}

pub(crate) fn format_runtime_guidance_instruction(
    guidance_item: &RuntimeGuidanceItem,
    locale: InternalContextLocale,
) -> String {
    if locale.is_english() {
        format!(
            "[Runtime Guidance]\n- guidance_id: {}\n- time: {}\n- source: user guidance during running turn\n- instruction: {}\n- rule: treat this as high-priority preference unless conflicts with safety",
            guidance_item.guidance_id,
            guidance_item.created_at,
            guidance_item.content
        )
    } else {
        format!(
            "[Runtime Guidance]\n- guidance_id: {}\n- time: {}\n- source: 用户在运行中追加的指导\n- instruction: {}\n- rule: 将其视为高优先级偏好，除非与安全要求冲突",
            guidance_item.guidance_id,
            guidance_item.created_at,
            guidance_item.content
        )
    }
}

pub(crate) fn build_runtime_guidance_applied_event(
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

#[cfg(test)]
mod tests {
    use super::{build_runtime_guidance_applied_event, format_runtime_guidance_instruction};
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::services::runtime_guidance_manager::{
        RuntimeGuidanceItem, RuntimeGuidanceStatus,
    };

    fn sample_item() -> RuntimeGuidanceItem {
        RuntimeGuidanceItem {
            guidance_id: "gd_test_1".to_string(),
            session_id: "session-1".to_string(),
            turn_id: "turn-1".to_string(),
            content: "continue with the current task".to_string(),
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
        assert!(formatted.contains("gd_test_1"));
        assert!(formatted.contains("2026-04-27T12:00:00Z"));
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
            payload.get("conversation_id").and_then(|value| value.as_str()),
            Some("session-1")
        );
        assert_eq!(
            payload.get("pending_count").and_then(|value| value.as_u64()),
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
}
