use super::*;

pub(in crate::ask_user_prompt_service) fn prompt_to_decision(
    prompt: AskUserPromptRecord,
) -> AskUserDecision {
    let response = prompt
        .response
        .unwrap_or_else(|| AskUserResponseSubmission {
            status: status_label(prompt.status).to_string(),
            values: None,
            selection: None,
            reason: None,
        });
    AskUserDecision {
        status: response.status.clone(),
        response,
    }
}

pub(in crate::ask_user_prompt_service) fn prompt_event_payload(
    prompt: &AskUserPromptRecord,
) -> Value {
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

pub(in crate::ask_user_prompt_service) fn status_label(
    status: AskUserPromptStatus,
) -> &'static str {
    match status {
        AskUserPromptStatus::Pending => "pending",
        AskUserPromptStatus::Submitted => "submitted",
        AskUserPromptStatus::Cancelled => "cancelled",
        AskUserPromptStatus::TimedOut => "timed_out",
        AskUserPromptStatus::Failed => "failed",
    }
}

pub(in crate::ask_user_prompt_service) fn normalized_optional(
    value: Option<String>,
) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}
