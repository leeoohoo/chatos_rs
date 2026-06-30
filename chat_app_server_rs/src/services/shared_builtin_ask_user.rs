use async_trait::async_trait;
use serde_json::json;

use chatos_builtin_tools::{
    AskUserDecision as SharedAskUserDecision, AskUserPromptPayload as SharedAskUserPromptPayload,
    AskUserResponseSubmission as SharedAskUserResponseSubmission, AskUserStore,
    AskUserStreamChunkCallback,
};

use crate::services::ask_user_prompt_manager::{
    create_ask_user_prompt_record, create_ask_user_prompt_request, redact_response_for_store,
    update_ask_user_prompt_response, wait_for_ask_user_prompt_decision,
    AskUserPromptDecision as ChatosAskUserDecision,
    AskUserPromptPayload as ChatosAskUserPromptPayload,
    AskUserPromptResponseSubmission as ChatosAskUserResponseSubmission,
    AskUserPromptStatus as ChatosAskUserPromptStatus, ASK_USER_PROMPT_TIMEOUT_ERR,
};
use crate::utils::events::Events;

#[derive(Debug, Clone, Default)]
pub struct ChatosAskUserStore;

#[async_trait]
impl AskUserStore for ChatosAskUserStore {
    async fn execute_prompt(
        &self,
        payload: SharedAskUserPromptPayload,
        on_stream_chunk: Option<AskUserStreamChunkCallback>,
    ) -> Result<SharedAskUserDecision, String> {
        let payload = shared_payload_into_chatos(payload);
        create_ask_user_prompt_record(&payload).await?;

        let (registered_payload, receiver) =
            create_ask_user_prompt_request(payload.clone()).await?;
        emit_ask_user_prompt_required_event(on_stream_chunk.as_ref(), &registered_payload);

        let decision = match wait_for_ask_user_prompt_decision(
            registered_payload.prompt_id.as_str(),
            receiver,
            registered_payload.timeout_ms,
        )
        .await
        {
            Ok(decision) => decision,
            Err(err) if err == ASK_USER_PROMPT_TIMEOUT_ERR => {
                let timeout_response = ChatosAskUserResponseSubmission {
                    status: ChatosAskUserPromptStatus::Timeout.as_str().to_string(),
                    values: None,
                    selection: None,
                    reason: Some("timeout".to_string()),
                };
                let _ = update_ask_user_prompt_response(
                    registered_payload.prompt_id.as_str(),
                    ChatosAskUserPromptStatus::Timeout,
                    Some(json!({ "status": "timeout" })),
                )
                .await;
                emit_ask_user_prompt_resolved_event(
                    on_stream_chunk.as_ref(),
                    registered_payload.prompt_id.as_str(),
                    ChatosAskUserPromptStatus::Timeout,
                );
                return Ok(chatos_decision_into_shared(ChatosAskUserDecision {
                    status: ChatosAskUserPromptStatus::Timeout,
                    response: timeout_response,
                }));
            }
            Err(err) => return Err(err),
        };

        let redacted_response = redact_response_for_store(&decision.response, &registered_payload);
        let _ = update_ask_user_prompt_response(
            registered_payload.prompt_id.as_str(),
            decision.status,
            Some(redacted_response),
        )
        .await;
        emit_ask_user_prompt_resolved_event(
            on_stream_chunk.as_ref(),
            registered_payload.prompt_id.as_str(),
            decision.status,
        );
        Ok(chatos_decision_into_shared(decision))
    }
}

fn shared_payload_into_chatos(payload: SharedAskUserPromptPayload) -> ChatosAskUserPromptPayload {
    ChatosAskUserPromptPayload {
        prompt_id: payload.prompt_id,
        conversation_id: payload.conversation_id,
        conversation_turn_id: payload.conversation_turn_id,
        tool_call_id: payload.tool_call_id,
        kind: payload.kind,
        title: payload.title,
        message: payload.message,
        allow_cancel: payload.allow_cancel,
        timeout_ms: payload.timeout_ms,
        payload: payload.payload,
    }
}

fn chatos_decision_into_shared(decision: ChatosAskUserDecision) -> SharedAskUserDecision {
    SharedAskUserDecision {
        status: decision.status.as_str().to_string(),
        response: chatos_response_into_shared(decision.response),
    }
}

fn chatos_response_into_shared(
    response: ChatosAskUserResponseSubmission,
) -> SharedAskUserResponseSubmission {
    SharedAskUserResponseSubmission {
        status: response.status,
        values: response.values,
        selection: response.selection,
        reason: response.reason,
    }
}

fn emit_ask_user_prompt_required_event(
    on_stream_chunk: Option<&AskUserStreamChunkCallback>,
    payload: &ChatosAskUserPromptPayload,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let chunk = json!({
        "event": Events::ASK_USER_PROMPT_REQUIRED,
        "data": payload,
    });
    if let Ok(serialized) = serde_json::to_string(&chunk) {
        callback(serialized);
    }
}

fn emit_ask_user_prompt_resolved_event(
    on_stream_chunk: Option<&AskUserStreamChunkCallback>,
    prompt_id: &str,
    status: ChatosAskUserPromptStatus,
) {
    let Some(callback) = on_stream_chunk else {
        return;
    };
    let chunk = json!({
        "event": Events::ASK_USER_PROMPT_RESOLVED,
        "data": {
            "prompt_id": prompt_id,
            "status": status.as_str(),
        }
    });
    if let Ok(serialized) = serde_json::to_string(&chunk) {
        callback(serialized);
    }
}
