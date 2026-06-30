use std::collections::HashMap;
use std::time::Duration;

use once_cell::sync::Lazy;
use tokio::sync::{oneshot, Mutex};

use super::normalizer::trimmed_non_empty;
use super::types::{
    AskUserPromptDecision, AskUserPromptPayload, ASK_USER_PROMPT_TIMEOUT_ERR,
    ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT,
};
use super::types::{
    AskUserPromptResponseSubmission, AskUserPromptStatus, ASK_USER_PROMPT_NOT_FOUND_ERR,
};

#[derive(Debug)]
struct PendingAskUserPromptEntry {
    _payload: AskUserPromptPayload,
    _sender: oneshot::Sender<AskUserPromptDecision>,
}

#[derive(Debug, Default)]
struct AskUserPromptHub {
    pending: Mutex<HashMap<String, PendingAskUserPromptEntry>>,
}

impl AskUserPromptHub {
    async fn register(
        &self,
        payload: AskUserPromptPayload,
    ) -> oneshot::Receiver<AskUserPromptDecision> {
        let prompt_id = payload.prompt_id.clone();
        let (sender, receiver) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        pending.insert(
            prompt_id,
            PendingAskUserPromptEntry {
                _payload: payload,
                _sender: sender,
            },
        );
        receiver
    }

    async fn resolve(
        &self,
        prompt_id: &str,
        response: AskUserPromptResponseSubmission,
    ) -> Result<AskUserPromptPayload, String> {
        let entry = {
            let mut pending = self.pending.lock().await;
            pending.remove(prompt_id)
        }
        .ok_or_else(|| ASK_USER_PROMPT_NOT_FOUND_ERR.to_string())?;

        let status = AskUserPromptStatus::from_str(response.status.as_str())
            .unwrap_or(AskUserPromptStatus::Canceled);
        if status == AskUserPromptStatus::Pending {
            return Err("status must not be pending".to_string());
        }

        entry
            ._sender
            .send(AskUserPromptDecision { status, response })
            .map_err(|_| "ask_user_prompt_listener_closed".to_string())?;

        Ok(entry._payload)
    }

    async fn remove(&self, prompt_id: &str) {
        let mut pending = self.pending.lock().await;
        pending.remove(prompt_id);
    }
}

static ASK_USER_PROMPT_HUB: Lazy<AskUserPromptHub> = Lazy::new(AskUserPromptHub::default);

pub async fn create_ask_user_prompt_request(
    payload: AskUserPromptPayload,
) -> Result<
    (
        AskUserPromptPayload,
        oneshot::Receiver<AskUserPromptDecision>,
    ),
    String,
> {
    let prompt_id = trimmed_non_empty(payload.prompt_id.as_str())
        .ok_or_else(|| "prompt_id is required".to_string())?
        .to_string();
    let conversation_id = trimmed_non_empty(payload.conversation_id.as_str())
        .ok_or_else(|| "conversation_id is required".to_string())?
        .to_string();
    let conversation_turn_id = trimmed_non_empty(payload.conversation_turn_id.as_str())
        .ok_or_else(|| "conversation_turn_id is required".to_string())?
        .to_string();

    let mut normalized_payload = payload;
    normalized_payload.prompt_id = prompt_id;
    normalized_payload.conversation_id = conversation_id;
    normalized_payload.conversation_turn_id = conversation_turn_id;
    normalized_payload.timeout_ms = normalized_payload
        .timeout_ms
        .clamp(1_000, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT);

    let receiver = ASK_USER_PROMPT_HUB
        .register(normalized_payload.clone())
        .await;
    Ok((normalized_payload, receiver))
}

pub async fn wait_for_ask_user_prompt_decision(
    prompt_id: &str,
    receiver: oneshot::Receiver<AskUserPromptDecision>,
    timeout_ms: u64,
) -> Result<AskUserPromptDecision, String> {
    let prompt_id =
        trimmed_non_empty(prompt_id).ok_or_else(|| "prompt_id is required".to_string())?;
    let bounded_timeout = timeout_ms.clamp(1_000, ASK_USER_PROMPT_TIMEOUT_MS_DEFAULT);
    match tokio::time::timeout(Duration::from_millis(bounded_timeout), receiver).await {
        Ok(Ok(decision)) => Ok(decision),
        Ok(Err(_)) => Err("ask_user_prompt_listener_closed".to_string()),
        Err(_) => {
            ASK_USER_PROMPT_HUB.remove(prompt_id).await;
            Err(ASK_USER_PROMPT_TIMEOUT_ERR.to_string())
        }
    }
}

pub async fn submit_ask_user_prompt_response(
    prompt_id: &str,
    response: AskUserPromptResponseSubmission,
) -> Result<AskUserPromptPayload, String> {
    let prompt_id =
        trimmed_non_empty(prompt_id).ok_or_else(|| "prompt_id is required".to_string())?;
    ASK_USER_PROMPT_HUB.resolve(prompt_id, response).await
}
