use super::*;

pub(in crate::ui_prompt_service) fn prompt_to_decision(prompt: UiPromptRecord) -> UiPromptDecision {
    let response = prompt
        .response
        .unwrap_or_else(|| UiPromptResponseSubmission {
            status: status_label(prompt.status).to_string(),
            values: None,
            selection: None,
            reason: None,
        });
    UiPromptDecision {
        status: response.status.clone(),
        response,
    }
}

pub(in crate::ui_prompt_service) fn prompt_event_payload(prompt: &UiPromptRecord) -> Value {
    json!({
        "prompt_id": prompt.id,
        "task_id": prompt.task_id,
        "run_id": prompt.run_id,
        "kind": prompt.kind,
        "title": prompt.title,
        "message": prompt.message,
        "status": status_label(prompt.status),
        "allow_cancel": prompt.allow_cancel,
        "timeout_ms": prompt.timeout_ms,
        "payload": prompt.payload,
        "response": prompt.response,
        "expires_at": prompt.expires_at,
    })
}

pub(in crate::ui_prompt_service) fn status_label(status: UiPromptStatus) -> &'static str {
    match status {
        UiPromptStatus::Pending => "pending",
        UiPromptStatus::Submitted => "submitted",
        UiPromptStatus::Cancelled => "cancelled",
        UiPromptStatus::TimedOut => "timed_out",
        UiPromptStatus::Failed => "failed",
    }
}

pub(in crate::ui_prompt_service) fn normalized_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}
