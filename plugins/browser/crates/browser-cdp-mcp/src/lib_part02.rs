fn browser_skill_for_tool(name: &str) -> &'static str {
    if name == "browser_upload" || name == "browser_downloads" {
        return "browser-file-transfer";
    }
    if name.starts_with("browser_console")
        || name.starts_with("browser_network")
        || name.starts_with("browser_har")
        || name.starts_with("browser_websocket")
        || name.starts_with("browser_route")
        || name.starts_with("browser_cdp")
    {
        return "browser-network-debugging";
    }
    if name == "browser_snapshot" || name == "browser_find" || name == "browser_screenshot" {
        return "browser-observation-verification";
    }
    if name == "browser_click"
        || name == "browser_type"
        || name == "browser_fill_form"
        || name == "browser_press"
        || name == "browser_scroll"
        || name == "browser_wait"
        || name == "browser_handle_dialog"
    {
        return "browser-interaction";
    }
    "browser-navigation"
}

#[allow(clippy::too_many_arguments)]
fn tool_with_permission_rules(
    name: &str,
    description: &str,
    input_schema: Value,
    permissions: &[&str],
    risk: &str,
    approval: &str,
    parallel_safe: bool,
    timeout_ms: u64,
    permission_rules: Value,
) -> Value {
    let mut definition = tool(
        name,
        description,
        input_schema,
        permissions,
        risk,
        approval,
        parallel_safe,
        timeout_ms,
    );
    definition["_meta"]["chatos/permissionRules"] = permission_rules;
    definition
}

fn object_schema(properties: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let properties = properties
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect::<Map<_, _>>();
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn session_prop() -> (&'static str, Value) {
    (
        "browser_session_id",
        json!({
            "type": "string",
            "description": "Opaque browser_session_id returned by browser_session_open. Copy it exactly; never derive it from or combine it with a tab_id."
        }),
    )
}
fn string_prop(name: &'static str) -> (&'static str, Value) {
    (name, json!({ "type": "string" }))
}
fn timeout_prop(maximum: u64) -> (&'static str, Value) {
    (
        "timeout_ms",
        json!({ "type": "integer", "minimum": 1, "maximum": maximum }),
    )
}
fn session_schema() -> Value {
    object_schema(vec![session_prop()], &["browser_session_id"])
}
fn ref_schema() -> Value {
    object_schema(
        vec![session_prop(), string_prop("ref")],
        &["browser_session_id", "ref"],
    )
}

fn event_stream_schema() -> Value {
    object_schema(
        vec![
            session_prop(),
            (
                "action",
                json!({"type":"string","enum":["start","events","stop"]}),
            ),
            string_prop("subscription_id"),
            event_cursor_prop(),
            event_limit_prop(),
            wait_ms_prop(),
        ],
        &["browser_session_id"],
    )
}

fn event_poll_schema() -> Value {
    object_schema(
        vec![
            session_prop(),
            string_prop("subscription_id"),
            event_cursor_prop(),
            event_limit_prop(),
            wait_ms_prop(),
        ],
        &["browser_session_id", "subscription_id"],
    )
}

fn subscription_schema() -> Value {
    object_schema(
        vec![session_prop(), string_prop("subscription_id")],
        &["browser_session_id", "subscription_id"],
    )
}

fn event_cursor_prop() -> (&'static str, Value) {
    (
        "after_sequence",
        json!({"type":"integer","minimum":0,"default":0}),
    )
}

fn event_limit_prop() -> (&'static str, Value) {
    (
        "max_events",
        json!({"type":"integer","minimum":1,"maximum":1000,"default":100}),
    )
}

fn wait_ms_prop() -> (&'static str, Value) {
    (
        "wait_ms",
        json!({"type":"integer","minimum":0,"maximum":5000,"default":0}),
    )
}

fn required_string(value: &Value, name: &str) -> Result<String, CoreError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| CoreError::InvalidRequest(format!("{name} must be a non-empty string")))
}

async fn resolve_browser_session_id(
    arguments: &Value,
    active_browser_session: &ActiveBrowserSession,
) -> Result<String, CoreError> {
    if let Some(browser_session_id) = arguments
        .get("browser_session_id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        return Ok(browser_session_id.to_owned());
    }
    active_browser_session.lock().await.clone().ok_or_else(|| {
        CoreError::InvalidRequest(
            "no browser session is bound; call browser_session_open first".into(),
        )
    })
}

fn duration_ms(value: &Value, name: &str, default: u64, maximum: u64) -> Duration {
    Duration::from_millis(
        value
            .get(name)
            .and_then(Value::as_u64)
            .unwrap_or(default)
            .clamp(1, maximum),
    )
}

fn success_response(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn id_key(id: &Value) -> String {
    serde_json::to_string(id).unwrap_or_else(|_| "null".into())
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = terminate.recv() => {},
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
include!("lib_inline_tests.rs");
