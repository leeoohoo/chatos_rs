use tracing::{error, info, warn};

pub fn log_chat_begin(
    session_id: &str,
    model: &str,
    base_url: &str,
    use_tools: bool,
    http_count: usize,
    stdio_count: usize,
    has_api_key: bool,
) {
    info!("[CHAT] begin: session={}, model={}, baseUrl={}, useTools={}, httpMCP={}, stdioMCP={}, hasApiKey={}", session_id, model, if base_url.is_empty() { "(default)" } else { base_url }, use_tools, http_count, stdio_count, has_api_key);
}

pub fn log_chat_cancelled(session_id: &str) {
    warn!("[CHAT] cancelled: session={}", session_id);
}

pub fn log_chat_error(err: &str) {
    error!("[CHAT] error: {}", err);
}
