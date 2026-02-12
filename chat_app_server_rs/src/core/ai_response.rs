use std::future::Future;

use tokio::time::{timeout, Duration};

pub async fn run_with_timeout<T, E, F>(timeout_ms: i64, task: F) -> Result<T, String>
where
    F: Future<Output = Result<T, E>>,
    E: ToString,
{
    timeout(Duration::from_millis(timeout_ms.max(1) as u64), task)
        .await
        .map_err(|_| format!("AI timeout after {} ms", timeout_ms))?
        .map_err(|err| err.to_string())
}

pub fn normalize_non_empty_content(content: &str) -> String {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        "(empty)".to_string()
    } else {
        trimmed.to_string()
    }
}
