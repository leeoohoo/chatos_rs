use super::*;
use std::sync::OnceLock;

static CHATOS_CALLBACK_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub(super) async fn send_chatos_task_callback(
    config: AppConfig,
    payload: ChatosTaskCallbackPayload,
) -> Result<(), String> {
    let Some(url) = config.chatos_callback_url.clone() else {
        return Err("TASK_RUNNER_CHATOS_CALLBACK_URL not configured".to_string());
    };
    let mut request = chatos_callback_client()
        .post(url)
        .timeout(config.callback_timeout)
        .json(&payload);
    if let Some(secret) = config.chatos_callback_secret.clone() {
        request = request.header("X-Task-Runner-Callback-Secret", secret);
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let body = response.text().await.unwrap_or_default();
    Err(format!("callback request failed: {status} {body}"))
}

fn chatos_callback_client() -> &'static reqwest::Client {
    CHATOS_CALLBACK_CLIENT.get_or_init(reqwest::Client::new)
}
