use serde_json::{json, Value};
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::chat_runtime::project_id_from_metadata;
use crate::core::internal_context_locale::InternalContextLocale;
use crate::core::session_access::{ensure_owned_session, SessionAccessError};
use crate::services::ai_common::normalize_turn_id;
use crate::services::runtime_guidance_manager::{runtime_guidance_manager, DEFAULT_DRAIN_LIMIT};
use crate::utils::abort_registry;

use super::messages::{self, CreateUserMessageInput};
use super::session_scope::{normalize_optional_text, normalize_project_scope};
use super::user_context::load_runtime_user_context;

pub use crate::services::runtime_guidance_manager::{EnqueueGuidanceError, RuntimeGuidanceItem};

pub const CONTENT_MAX_LEN: usize = 1000;

#[derive(Debug, Clone)]
pub struct SubmitRuntimeGuidanceInput {
    pub conversation_id: Option<String>,
    pub turn_id: Option<String>,
    pub content: Option<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SubmitRuntimeGuidanceOutput {
    pub conversation_id: String,
    pub guidance_id: String,
    pub pending_count: usize,
    pub turn_id: String,
}

#[derive(Debug, Clone)]
pub struct DrainedRuntimeGuidance {
    pub guidance_item: RuntimeGuidanceItem,
    pub applied_item: Option<RuntimeGuidanceItem>,
    pub pending_count: usize,
}

#[derive(Debug)]
pub enum SubmitRuntimeGuidanceError {
    InvalidPayload,
    TooLong {
        max_length: usize,
    },
    SessionNotFound,
    Forbidden,
    SessionLookupFailed,
    ProjectScopeMismatch {
        requested_project_id: String,
        session_project_id: String,
    },
    TurnNotRunning,
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
    runtime_guidance_manager().enqueue_guidance(session_id, turn_id, content)
}

pub async fn submit_runtime_guidance(
    auth: &AuthUser,
    input: SubmitRuntimeGuidanceInput,
) -> Result<SubmitRuntimeGuidanceOutput, SubmitRuntimeGuidanceError> {
    let conversation_id =
        normalize_optional_text(input.conversation_id.as_deref()).unwrap_or_default();
    let turn_id = normalize_turn_id(input.turn_id.as_deref()).unwrap_or_default();
    let content = normalize_optional_text(input.content.as_deref()).unwrap_or_default();
    let requested_project_id = normalize_optional_text(input.project_id.as_deref());

    if conversation_id.is_empty() || turn_id.is_empty() || content.is_empty() {
        return Err(SubmitRuntimeGuidanceError::InvalidPayload);
    }
    if content.chars().count() > CONTENT_MAX_LEN {
        return Err(SubmitRuntimeGuidanceError::TooLong {
            max_length: CONTENT_MAX_LEN,
        });
    }

    let target_session = load_owned_session(auth, conversation_id.as_str()).await?;
    if let Some(requested_project_id) = requested_project_id.as_deref() {
        let session_project_id = target_session
            .project_id
            .clone()
            .or_else(|| project_id_from_metadata(target_session.metadata.as_ref()));
        let requested_scope = normalize_project_scope(Some(requested_project_id));
        let session_scope = normalize_project_scope(session_project_id.as_deref());
        if requested_scope != session_scope {
            return Err(SubmitRuntimeGuidanceError::ProjectScopeMismatch {
                requested_project_id: requested_scope,
                session_project_id: session_scope,
            });
        }
    }

    if abort_registry::is_aborted(conversation_id.as_str()) {
        return Err(SubmitRuntimeGuidanceError::TurnNotRunning);
    }

    let guidance_item =
        enqueue_runtime_guidance(conversation_id.as_str(), turn_id.as_str(), content.as_str())
            .map_err(|err| match err {
                EnqueueGuidanceError::TurnNotRunning => SubmitRuntimeGuidanceError::TurnNotRunning,
            })?;

    let pending_count =
        runtime_guidance_manager().pending_count(conversation_id.as_str(), turn_id.as_str());
    let guidance_id = guidance_item.guidance_id.clone();

    let metadata = json!({
        "conversation_turn_id": turn_id,
        "hidden": true,
        "runtime_guidance": {
            "guidance_id": guidance_item.guidance_id,
            "status": "queued",
            "created_at": guidance_item.created_at,
        }
    });
    if let Err(err) = messages::create_user_message(
        conversation_id.as_str(),
        CreateUserMessageInput {
            content: content.clone(),
            message_id: Some(guidance_id.clone()),
            message_mode: Some("runtime_guidance".to_string()),
            message_source: Some("runtime_guidance".to_string()),
            metadata: Some(metadata),
        },
    )
    .await
    {
        warn!(
            "persist runtime guidance failed: session_id={} turn_id={} guidance_id={} detail={}",
            conversation_id, turn_id, guidance_id, err
        );
    }

    Ok(SubmitRuntimeGuidanceOutput {
        conversation_id,
        guidance_id,
        pending_count,
        turn_id,
    })
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
        .locale
}

pub fn format_runtime_guidance_instruction(
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

async fn load_owned_session(
    auth: &AuthUser,
    conversation_id: &str,
) -> Result<crate::models::session::Session, SubmitRuntimeGuidanceError> {
    match ensure_owned_session(conversation_id, auth).await {
        Ok(session) => Ok(session),
        Err(SessionAccessError::NotFound) => Err(SubmitRuntimeGuidanceError::SessionNotFound),
        Err(SessionAccessError::Forbidden) => Err(SubmitRuntimeGuidanceError::Forbidden),
        Err(err) => {
            warn!(
                "runtime guidance session lookup failed: session_id={} detail={:?}",
                conversation_id, err
            );
            Err(SubmitRuntimeGuidanceError::SessionLookupFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_runtime_guidance_applied_event, format_runtime_guidance_instruction};
    use crate::core::internal_context_locale::InternalContextLocale;
    use crate::services::runtime_guidance_manager::{RuntimeGuidanceItem, RuntimeGuidanceStatus};

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
}
