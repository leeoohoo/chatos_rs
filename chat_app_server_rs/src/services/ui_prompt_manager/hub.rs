use std::collections::HashMap;
use std::time::Duration;

use once_cell::sync::Lazy;
use tokio::sync::{oneshot, Mutex};

use super::normalizer::trimmed_non_empty;
use super::types::{
    UiPromptDecision, UiPromptPayload, UiPromptResponseSubmission, UiPromptStatus,
    UI_PROMPT_NOT_FOUND_ERR, UI_PROMPT_TIMEOUT_ERR, UI_PROMPT_TIMEOUT_MS_DEFAULT,
};

#[derive(Debug)]
struct PendingUiPromptEntry {
    payload: UiPromptPayload,
    sender: oneshot::Sender<UiPromptDecision>,
}

#[derive(Debug, Default)]
struct UiPromptHub {
    pending: Mutex<HashMap<String, PendingUiPromptEntry>>,
}

impl UiPromptHub {
    async fn register(&self, payload: UiPromptPayload) -> oneshot::Receiver<UiPromptDecision> {
        let prompt_id = payload.prompt_id.clone();
        let (sender, receiver) = oneshot::channel();
        let mut pending = self.pending.lock().await;
        pending.insert(prompt_id, PendingUiPromptEntry { payload, sender });
        receiver
    }

    async fn resolve(
        &self,
        prompt_id: &str,
        response: UiPromptResponseSubmission,
    ) -> Result<UiPromptPayload, String> {
        let entry = {
            let mut pending = self.pending.lock().await;
            pending.remove(prompt_id)
        }
        .ok_or_else(|| UI_PROMPT_NOT_FOUND_ERR.to_string())?;

        let status =
            UiPromptStatus::from_str(response.status.as_str()).unwrap_or(UiPromptStatus::Canceled);
        if status == UiPromptStatus::Pending {
            return Err("status must not be pending".to_string());
        }

        entry
            .sender
            .send(UiPromptDecision { status, response })
            .map_err(|_| "ui_prompt_listener_closed".to_string())?;

        Ok(entry.payload)
    }

    async fn payload(&self, prompt_id: &str) -> Option<UiPromptPayload> {
        let pending = self.pending.lock().await;
        pending.get(prompt_id).map(|entry| entry.payload.clone())
    }

    async fn remove(&self, prompt_id: &str) {
        let mut pending = self.pending.lock().await;
        pending.remove(prompt_id);
    }
}

static UI_PROMPT_HUB: Lazy<UiPromptHub> = Lazy::new(UiPromptHub::default);

pub async fn create_ui_prompt_request(
    payload: UiPromptPayload,
) -> Result<(UiPromptPayload, oneshot::Receiver<UiPromptDecision>), String> {
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
        .clamp(1_000, UI_PROMPT_TIMEOUT_MS_DEFAULT);

    let receiver = UI_PROMPT_HUB.register(normalized_payload.clone()).await;
    Ok((normalized_payload, receiver))
}

pub async fn wait_for_ui_prompt_decision(
    prompt_id: &str,
    receiver: oneshot::Receiver<UiPromptDecision>,
    timeout_ms: u64,
) -> Result<UiPromptDecision, String> {
    let prompt_id =
        trimmed_non_empty(prompt_id).ok_or_else(|| "prompt_id is required".to_string())?;
    let bounded_timeout = timeout_ms.clamp(1_000, UI_PROMPT_TIMEOUT_MS_DEFAULT);
    match tokio::time::timeout(Duration::from_millis(bounded_timeout), receiver).await {
        Ok(Ok(decision)) => Ok(decision),
        Ok(Err(_)) => Err("ui_prompt_listener_closed".to_string()),
        Err(_) => {
            UI_PROMPT_HUB.remove(prompt_id).await;
            Err(UI_PROMPT_TIMEOUT_ERR.to_string())
        }
    }
}

pub async fn submit_ui_prompt_response(
    prompt_id: &str,
    response: UiPromptResponseSubmission,
) -> Result<UiPromptPayload, String> {
    let prompt_id =
        trimmed_non_empty(prompt_id).ok_or_else(|| "prompt_id is required".to_string())?;
    UI_PROMPT_HUB.resolve(prompt_id, response).await
}

pub async fn get_ui_prompt_payload(prompt_id: &str) -> Option<UiPromptPayload> {
    let prompt_id = trimmed_non_empty(prompt_id)?;
    UI_PROMPT_HUB.payload(prompt_id).await
}
