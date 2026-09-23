fn artifact_registration_candidates(value: &Value) -> Vec<Value> {
    fn visit(value: &Value, output: &mut Vec<Value>) {
        if output.len() >= 64 {
            return;
        }
        match value {
            Value::Object(object) => {
                let candidate = (
                    object.get("artifact_id").and_then(Value::as_str),
                    object.get("relative_path").and_then(Value::as_str),
                    object.get("display_name").and_then(Value::as_str),
                    object.get("media_type").and_then(Value::as_str),
                    object.get("size_bytes").and_then(Value::as_u64),
                    object.get("sha256").and_then(Value::as_str),
                );
                if let (
                    Some(producer_artifact_id),
                    Some(relative_path),
                    Some(display_name),
                    Some(media_type),
                    Some(size_bytes),
                    Some(sha256),
                ) = candidate
                {
                    output.push(json!({
                        "producer_artifact_id": producer_artifact_id,
                        "relative_path": relative_path,
                        "display_name": display_name,
                        "media_type": media_type,
                        "size_bytes": size_bytes,
                        "sha256": sha256,
                    }));
                    return;
                }
                for child in object.values() {
                    visit(child, output);
                }
            }
            Value::Array(values) => {
                for child in values {
                    visit(child, output);
                }
            }
            _ => {}
        }
    }

    let mut output = Vec::new();
    visit(value, &mut output);
    output
}

async fn event_stream_action(
    runtime: Arc<BrowserRuntime>,
    browser_session_id: &str,
    arguments: &Value,
    methods: &[&str],
) -> Result<Value, CoreError> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            if arguments.get("subscription_id").is_some() {
                "events"
            } else {
                "start"
            }
        });
    match action {
        "start" => Ok(json!({
            "subscription_id": runtime.cdp_subscribe(
                browser_session_id,
                None,
                methods.iter().map(|method| (*method).to_owned()).collect(),
            ).await?
        })),
        "events" => Ok(serde_json::to_value(
            runtime
                .cdp_events(
                    browser_session_id,
                    &required_string(arguments, "subscription_id")?,
                    arguments
                        .get("after_sequence")
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                    arguments
                        .get("max_events")
                        .and_then(Value::as_u64)
                        .unwrap_or(100) as usize,
                    duration_ms(arguments, "wait_ms", 0, 5_000),
                )
                .await?,
        )
        .unwrap()),
        "stop" => {
            runtime
                .cdp_unsubscribe(
                    browser_session_id,
                    &required_string(arguments, "subscription_id")?,
                )
                .await?;
            Ok(json!({ "unsubscribed": true }))
        }
        _ => Err(CoreError::InvalidRequest(
            "action must be start, events, or stop".into(),
        )),
    }
}

fn tool_catalog() -> Vec<Value> {
    let mut tools = vec![
        tool_with_permission_rules(
            "browser_session_open",
            "Open the browser selected by ChatOS from verified local authorization: paired Google Chrome with a task-named native tab group when available, otherwise an isolated managed browser. The input has no browser-mode fields. Read the returned mode: chrome_extension is the user's Chrome; managed is the isolated fallback.",
            object_schema(
                vec![(
                    "session_name",
                    json!({
                        "type":"string",
                        "minLength":1,
                        "maxLength":80,
                        "description":"Human-readable task name used for the native Chrome tab group in chrome_extension mode."
                    }),
                )],
                &[],
            ),
            &[],
            "high",
            "per_call",
            false,
            20_000,
            json!([
                {
                    "argumentPointer": "/mode",
                    "equals": "managed",
                    "requiredPermissions": ["browser.managed.launch"]
                },
                {
                    "argumentPointer": "/mode",
                    "equals": "chrome_extension",
                    "matchWhenMissing": true,
                    "requiredPermissions": ["browser.chrome.attach"]
                }
            ]),
        ),
        tool(
            "browser_session_status",
            "Get browser session status, actual current mode, and capabilities.",
            session_schema(),
            &["browser.page.read"],
            "low",
            "none",
            true,
            5_000,
        ),
        tool(
            "browser_session_close",
            "Close a browser session and owned browser process.",
            session_schema(),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_tabs",
            "List tabs without exposing backend target identifiers.",
            session_schema(),
            &["browser.page.read"],
            "low",
            "none",
            true,
            5_000,
        ),
        tool(
            "browser_tab_new",
            "Open a new tab.",
            object_schema(
                vec![
                    session_prop(),
                    ("url", json!({"type":"string","default":"about:blank"})),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            15_000,
        ),
        tool(
            "browser_tab_switch",
            "Select the active tab.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id", "tab_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            5_000,
        ),
        tool(
            "browser_tab_close",
            "Close a tab.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id", "tab_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_navigate",
            "Navigate a tab to an HTTP, HTTPS, or about URL and wait for readiness.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("tab_id"),
                    string_prop("url"),
                    timeout_prop(60_000),
                ],
                &["browser_session_id", "url"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            60_000,
        ),
        tool(
            "browser_snapshot",
            "Return a compact accessibility-oriented snapshot with opaque element refs.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id"],
            ),
            &["browser.page.read"],
            "low",
            "none",
            true,
            20_000,
        ),
        tool(
            "browser_find",
            "Find elements by accessible role, name, text, or value.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("query"),
                    (
                        "max_results",
                        json!({"type":"integer","minimum":1,"maximum":100,"default":20}),
                    ),
                ],
                &["browser_session_id", "query"],
            ),
            &["browser.page.read"],
            "low",
            "none",
            true,
            20_000,
        ),
        tool(
            "browser_click",
            "Click an element from the latest snapshot.",
            ref_schema(),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_type",
            "Type into an element from the latest snapshot.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("ref"),
                    string_prop("text"),
                    ("clear", json!({"type":"boolean","default":false})),
                ],
                &["browser_session_id", "ref", "text"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_fill_form",
            "Fill several fields identified by refs.",
            object_schema(
                vec![
                    session_prop(),
                    (
                        "fields",
                        json!({"type":"array","maxItems":50,"items":{"type":"object","properties":{"ref":{"type":"string"},"value":{"type":"string"}},"required":["ref","value"],"additionalProperties":false}}),
                    ),
                ],
                &["browser_session_id", "fields"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            15_000,
        ),
        tool(
            "browser_upload",
            "Upload files to an input using short-lived Local Connector file grants only.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("ref"),
                    (
                        "file_grant_ids",
                        json!({"type":"array","minItems":1,"maxItems":20,"items":{"type":"string"}}),
                    ),
                ],
                &["browser_session_id", "ref", "file_grant_ids"],
            ),
            &["browser.file.transfer"],
            "critical",
            "per_call",
            false,
            15_000,
        ),
        tool(
            "browser_press",
            "Dispatch a key to the focused page element.",
            object_schema(
                vec![session_prop(), string_prop("key")],
                &["browser_session_id", "key"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_scroll",
            "Scroll the active page.",
            object_schema(
                vec![
                    session_prop(),
                    ("delta_x", json!({"type":"integer","default":0})),
                    ("delta_y", json!({"type":"integer","default":600})),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.control"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_wait",
            "Wait for a selector, visible text, or document readiness.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("selector"),
                    string_prop("text"),
                    timeout_prop(20_000),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.read"],
            "low",
            "none",
            true,
            20_000,
        ),
        tool(
            "browser_handle_dialog",
            "Accept or dismiss the currently open JavaScript dialog.",
            object_schema(
                vec![
                    session_prop(),
                    ("accept", json!({"type":"boolean"})),
                    string_prop("prompt_text"),
                ],
                &["browser_session_id", "accept"],
            ),
            &["browser.page.control"],
            "high",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_screenshot",
            "Capture a PNG into the session artifact directory.",
            object_schema(
                vec![
                    session_prop(),
                    ("full_page", json!({"type":"boolean","default":false})),
                ],
                &["browser_session_id"],
            ),
            &["browser.page.read", "browser.file.transfer"],
            "high",
            "per_call",
            false,
            20_000,
        ),
        tool(
            "browser_downloads",
            "Start, poll, collect, or stop downloads confined to the plugin artifact directory.",
            object_schema(
                vec![
                    session_prop(),
                    (
                        "action",
                        json!({"type":"string","enum":["start","events","collect","stop"],"default":"start"}),
                    ),
                    string_prop("subscription_id"),
                    event_cursor_prop(),
                    wait_ms_prop(),
                ],
                &["browser_session_id"],
            ),
            &["browser.file.transfer"],
            "high",
            "per_call",
            false,
            20_000,
        ),
        tool(
            "browser_console",
            "Start, poll, or stop a bounded console and page-exception event stream.",
            event_stream_schema(),
            &["browser.network.observe"],
            "low",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_network",
            "Start, poll, or stop a bounded redacted network event stream.",
            event_stream_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_network_request",
            "Poll network events for one CDP request identifier.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("subscription_id"),
                    string_prop("request_id"),
                    event_cursor_prop(),
                    event_limit_prop(),
                    wait_ms_prop(),
                ],
                &["browser_session_id", "subscription_id", "request_id"],
            ),
            &["browser.network.observe"],
            "medium",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_har_start",
            "Start bounded HAR capture with sensitive request fields redacted.",
            session_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_har_stop",
            "Stop HAR capture and write a HAR 1.2 artifact.",
            subscription_schema(),
            &["browser.network.observe", "browser.file.transfer"],
            "high",
            "per_call",
            false,
            20_000,
        ),
        tool(
            "browser_websocket_start",
            "Start a bounded redacted WebSocket lifecycle and frame event stream.",
            session_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_websocket_events",
            "Poll a WebSocket event stream by sequence.",
            event_poll_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_websocket_stop",
            "Stop a WebSocket event stream.",
            subscription_schema(),
            &["browser.network.observe"],
            "medium",
            "none",
            false,
            5_000,
        ),
        tool(
            "browser_route_add",
            "Add an approved URL-pattern route that only aborts or returns fixed JSON.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("tab_id"),
                    string_prop("url_pattern"),
                    (
                        "action",
                        json!({
                            "oneOf": [
                                {
                                    "type": "object",
                                    "properties": { "type": { "const": "abort" } },
                                    "required": ["type"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "type": { "const": "mock_json" },
                                        "status": { "type": "integer", "minimum": 100, "maximum": 599, "default": 200 },
                                        "body": {}
                                    },
                                    "required": ["type", "body"],
                                    "additionalProperties": false
                                }
                            ]
                        }),
                    ),
                ],
                &["browser_session_id", "url_pattern", "action"],
            ),
            &["browser.network.intercept"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_route_list",
            "List active request routes without backend identifiers.",
            session_schema(),
            &["browser.network.intercept"],
            "medium",
            "none",
            true,
            5_000,
        ),
        tool(
            "browser_route_remove",
            "Remove one request route.",
            object_schema(
                vec![session_prop(), string_prop("route_id")],
                &["browser_session_id", "route_id"],
            ),
            &["browser.network.intercept"],
            "high",
            "none",
            false,
            5_000,
        ),
        tool(
            "browser_route_clear",
            "Remove all request routes in a browser session.",
            session_schema(),
            &["browser.network.intercept"],
            "high",
            "none",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_targets",
            "List page targets using opaque tab IDs.",
            session_schema(),
            &["browser.cdp.raw"],
            "high",
            "per_call",
            true,
            5_000,
        ),
        tool(
            "browser_cdp_attach",
            "Create an opaque raw CDP session for a tab.",
            object_schema(
                vec![session_prop(), string_prop("tab_id")],
                &["browser_session_id", "tab_id"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_detach",
            "Detach an opaque raw CDP session.",
            object_schema(
                vec![session_prop(), string_prop("cdp_session_id")],
                &["browser_session_id", "cdp_session_id"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_send",
            "Execute a raw Chrome DevTools Protocol command without logging params or results.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("cdp_session_id"),
                    (
                        "target",
                        json!({"type":"string","enum":["page","browser"],"default":"page"}),
                    ),
                    string_prop("method"),
                    ("params", json!({"type":"object","default":{}})),
                    timeout_prop(15_000),
                ],
                &["browser_session_id", "method"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            15_000,
        ),
        tool(
            "browser_cdp_subscribe",
            "Subscribe to supported raw CDP events using a bounded local queue.",
            object_schema(
                vec![
                    session_prop(),
                    string_prop("cdp_session_id"),
                    (
                        "methods",
                        json!({"type":"array","minItems":1,"maxItems":32,"items":{"type":"string"}}),
                    ),
                ],
                &["browser_session_id", "methods"],
            ),
            &["browser.cdp.raw"],
            "critical",
            "per_call",
            false,
            10_000,
        ),
        tool(
            "browser_cdp_events",
            "Poll raw CDP events by sequence without relying on MCP notifications.",
            event_poll_schema(),
            &["browser.cdp.raw"],
            "high",
            "none",
            true,
            10_000,
        ),
        tool(
            "browser_cdp_unsubscribe",
            "Stop a raw CDP event subscription.",
            subscription_schema(),
            &["browser.cdp.raw"],
            "high",
            "none",
            false,
            5_000,
        ),
    ];
    for tool in &mut tools {
        hide_browser_session_id_from_schema(tool);
    }
    tools
}

fn hide_browser_session_id_from_schema(tool: &mut Value) {
    if let Some(properties) = tool
        .pointer_mut("/inputSchema/properties")
        .and_then(Value::as_object_mut)
    {
        properties.remove("browser_session_id");
    }
    if let Some(required) = tool
        .pointer_mut("/inputSchema/required")
        .and_then(Value::as_array_mut)
    {
        required.retain(|name| name.as_str() != Some("browser_session_id"));
    }
}

fn hide_browser_session_id_from_result(result: &mut Value) {
    if let Some(object) = result.as_object_mut() {
        object.remove("browser_session_id");
    }
}

#[allow(clippy::too_many_arguments)]
fn tool(
    name: &str,
    description: &str,
    input_schema: Value,
    permissions: &[&str],
    risk: &str,
    approval: &str,
    parallel_safe: bool,
    timeout_ms: u64,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "_meta": {
            "chatos/policyVersion": 1,
            "chatos/requiredPermissions": permissions,
            "chatos/riskLevel": risk,
            "chatos/approvalMode": approval,
            "chatos/parallelSafe": parallel_safe,
            "chatos/timeoutMs": timeout_ms,
            "chatos/toolResultMaxChars": MAX_TOOL_RESULT_CHARS,
            "chatos/skillGate": {
                "allOf": ["browser-cdp", browser_skill_for_tool(name)]
            }
        }
    })
}
