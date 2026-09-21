async fn handle_terminal_relay_socket(
    state: AppState,
    owner_user_id: String,
    device_id: String,
    workspace_id: String,
    terminal_session_id: String,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    mut socket: WebSocket,
) {
    let subscription = match state
        .relay
        .subscribe_terminal_session_for(
            terminal_session_id.as_str(),
            owner_user_id.as_str(),
            device_id.as_str(),
        )
        .await
    {
        Ok(subscription) => subscription,
        Err(error) => {
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": error}).to_string().into(),
                ))
                .await;
            return;
        }
    };
    let subscription_id = subscription.id;
    let mut events = subscription.events;
    let create_request = RelayRequest {
        message_type: "terminal_session_create_request".to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.clone(),
        device_id: device_id.clone(),
        workspace_id: workspace_id.clone(),
        method: "POST".to_string(),
        path: "/terminal/sessions".to_string(),
        headers: BTreeMap::new(),
        body: json!({
            "terminal_session_id": terminal_session_id.as_str(),
            "cwd": cwd,
            "cols": cols,
            "rows": rows,
        }),
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    let create_response =
        dispatch_relay(&state, create_request, state.config.relay_request_timeout).await;
    let initial_sequence;
    match create_response {
        Ok(response) if (200..300).contains(&response.status) => {
            let snapshot = response
                .body
                .get("snapshot")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let base_sequence = response
                .body
                .get("base_sequence")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let sequence = response
                .body
                .get("sequence")
                .and_then(Value::as_u64)
                .unwrap_or(base_sequence);
            initial_sequence = sequence;
            let truncated = response
                .body
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !snapshot.is_empty()
                && socket
                    .send(Message::Text(
                        json!({
                            "type": "snapshot",
                            "data": snapshot,
                            "base_sequence": base_sequence,
                            "sequence": sequence,
                            "truncated": truncated,
                            "protocol_version": 2,
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .is_err()
            {
                drop_terminal_subscription(
                    &state,
                    terminal_session_id.as_str(),
                    subscription_id.as_str(),
                )
                .await;
                return;
            }
            let busy = response
                .body
                .get("busy")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if socket
                .send(Message::Text(
                    json!({
                        "type": "state",
                        "state": "ready",
                        "busy": busy,
                        "snapshot_paging": true,
                        "protocol_version": 2,
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .is_err()
            {
                drop_terminal_subscription(
                    &state,
                    terminal_session_id.as_str(),
                    subscription_id.as_str(),
                )
                .await;
                return;
            }
        }
        Ok(response) => {
            let message = response
                .body
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Local Connector terminal startup failed");
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": message})
                        .to_string()
                        .into(),
                ))
                .await;
            drop_terminal_subscription(
                &state,
                terminal_session_id.as_str(),
                subscription_id.as_str(),
            )
            .await;
            return;
        }
        Err(err) => {
            let _ = socket
                .send(Message::Text(
                    json!({"type": "error", "error": err.message()})
                        .to_string()
                        .into(),
                ))
                .await;
            drop_terminal_subscription(
                &state,
                terminal_session_id.as_str(),
                subscription_id.as_str(),
            )
            .await;
            return;
        }
    }

    let (mut sender, mut receiver) = socket.split();
    let relay = state.relay.clone();
    let subscriber_terminal_session_id = terminal_session_id.clone();
    let subscriber_id = subscription_id.clone();
    let event_state = state.clone();
    let event_owner_user_id = owner_user_id.clone();
    let event_device_id = device_id.clone();
    let event_workspace_id = workspace_id.clone();
    let refresh_interval = state.config.terminal_subscriber_refresh_interval;
    let mut event_task = tokio::spawn(async move {
        let mut refresh = tokio::time::interval(refresh_interval);
        let mut last_sequence = initial_sequence;
        let mut awaiting_snapshot = false;
        let mut pending_exit = None;
        refresh.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(event) => {
                        if awaiting_snapshot && event.message_type == "terminal_exit" {
                            pending_exit = Some(event);
                            continue;
                        }
                        if event.message_type == "terminal_snapshot" {
                            last_sequence = event.body
                                .get("sequence")
                                .and_then(Value::as_u64)
                                .unwrap_or(last_sequence);
                            awaiting_snapshot = false;
                        } else if event.message_type == "terminal_output" {
                            let sequence = event.body.get("sequence").and_then(Value::as_u64);
                            if awaiting_snapshot {
                                continue;
                            }
                            if let Some(sequence) = sequence {
                                if sequence <= last_sequence {
                                    continue;
                                }
                                if last_sequence > 0 && sequence != last_sequence.saturating_add(1) {
                                    awaiting_snapshot = true;
                                    if !send_terminal_control(
                                        &event_state,
                                        event_owner_user_id.as_str(),
                                        event_device_id.as_str(),
                                        event_workspace_id.as_str(),
                                        "terminal_snapshot_request",
                                        subscriber_terminal_session_id.as_str(),
                                        json!({ "lines": 500 }),
                                    ).await {
                                        break;
                                    }
                                    continue;
                                }
                                last_sequence = sequence;
                            }
                        }
                        let is_snapshot = event.message_type == "terminal_snapshot";
                        let is_exit = event.message_type == "terminal_exit";
                        let payload =
                            terminal_event_to_ws_payload(event.message_type.as_str(), &event.body);
                        let Some(payload) = payload else {
                            continue;
                        };
                        if sender
                            .send(Message::Text(payload.to_string().into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                        if is_snapshot {
                            if let Some(exit) = pending_exit.take() {
                                let Some(payload) = terminal_event_to_ws_payload(
                                    exit.message_type.as_str(),
                                    &exit.body,
                                ) else {
                                    continue;
                                };
                                let _ = sender
                                    .send(Message::Text(payload.to_string().into()))
                                    .await;
                                break;
                            }
                        }
                        if is_exit {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        awaiting_snapshot = true;
                        if !send_terminal_control(
                            &event_state,
                            event_owner_user_id.as_str(),
                            event_device_id.as_str(),
                            event_workspace_id.as_str(),
                            "terminal_snapshot_request",
                            subscriber_terminal_session_id.as_str(),
                            json!({ "lines": 500 }),
                        ).await {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                },
                _ = refresh.tick() => {
                    match relay
                        .refresh_terminal_subscription(
                            subscriber_terminal_session_id.as_str(),
                            subscriber_id.as_str(),
                        )
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) => break,
                        Err(error) => {
                            let _ = sender
                                .send(Message::Text(
                                    json!({"type": "error", "error": error})
                                        .to_string()
                                        .into(),
                                ))
                                .await;
                            break;
                        }
                    }
                }
            }
        }
    });

    loop {
        let message = tokio::select! {
            _ = &mut event_task => break,
            message = receiver.next() => message,
        };
        let Some(message) = message else {
            break;
        };
        match message {
            Ok(Message::Text(text)) => {
                if !handle_terminal_ws_input(
                    &state,
                    owner_user_id.as_str(),
                    device_id.as_str(),
                    workspace_id.as_str(),
                    terminal_session_id.as_str(),
                    text.as_str(),
                )
                .await
                {
                    break;
                }
            }
            Ok(Message::Binary(bytes)) => {
                let data = String::from_utf8_lossy(&bytes).to_string();
                if !data.is_empty()
                    && !send_terminal_control(
                        &state,
                        owner_user_id.as_str(),
                        device_id.as_str(),
                        workspace_id.as_str(),
                        "terminal_input",
                        terminal_session_id.as_str(),
                        json!({ "data": data }),
                    )
                    .await
                {
                    break;
                }
            }
            Ok(Message::Ping(_)) => {}
            Ok(Message::Close(_)) | Err(_) => break,
            Ok(_) => {}
        }
    }

    event_task.abort();
    drop_terminal_subscription(
        &state,
        terminal_session_id.as_str(),
        subscription_id.as_str(),
    )
    .await;
}

pub(super) async fn drop_terminal_subscription(
    state: &AppState,
    terminal_session_id: &str,
    subscription_id: &str,
) {
    if let Err(error) = state
        .relay
        .drop_terminal_subscription(terminal_session_id, subscription_id)
        .await
    {
        tracing::warn!(
            terminal_session_id,
            subscription_id,
            error = error.as_str(),
            "drop Local Connector terminal subscriber lease failed"
        );
    }
}

async fn handle_terminal_ws_input(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    text: &str,
) -> bool {
    let parsed = serde_json::from_str::<Value>(text);
    let Ok(value) = parsed else {
        return send_terminal_control(
            state,
            owner_user_id,
            device_id,
            workspace_id,
            "terminal_input",
            terminal_session_id,
            json!({ "data": text }),
        )
        .await;
    };
    let message_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match message_type {
        "input" => {
            let data = value
                .get("data")
                .and_then(Value::as_str)
                .unwrap_or_default();
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_input",
                terminal_session_id,
                json!({ "data": data }),
            )
            .await
        }
        "resize" => {
            let cols = value.get("cols").and_then(Value::as_u64).unwrap_or(80);
            let rows = value.get("rows").and_then(Value::as_u64).unwrap_or(24);
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_resize",
                terminal_session_id,
                json!({ "cols": cols, "rows": rows }),
            )
            .await
        }
        "snapshot" => {
            let lines = value.get("lines").and_then(Value::as_u64).unwrap_or(500);
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_snapshot_request",
                terminal_session_id,
                json!({ "lines": lines }),
            )
            .await
        }
        "command" => {
            let command = value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default();
            send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_command",
                terminal_session_id,
                json!({ "command": command }),
            )
            .await
        }
        "close" => {
            let _ = send_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                "terminal_close",
                terminal_session_id,
                json!({}),
            )
            .await;
            false
        }
        "ping" => true,
        _ => true,
    }
}

async fn send_terminal_control(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    message_type: &str,
    terminal_session_id: &str,
    mut body: Value,
) -> bool {
    if let Value::Object(ref mut map) = body {
        map.insert(
            "terminal_session_id".to_string(),
            Value::String(terminal_session_id.to_string()),
        );
    }
    let request = RelayRequest {
        message_type: message_type.to_string(),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.to_string(),
        device_id: device_id.to_string(),
        workspace_id: workspace_id.to_string(),
        method: "POST".to_string(),
        path: format!("/terminal/{message_type}"),
        headers: BTreeMap::new(),
        body,
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
    };
    send_relay(state, request).await.is_ok()
}

pub(super) fn terminal_event_to_ws_payload(message_type: &str, body: &Value) -> Option<Value> {
    match message_type {
        "terminal_output" => Some(json!({
            "type": "output",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
            "sequence": body.get("sequence").and_then(Value::as_u64),
            "protocol_version": body.get("protocol_version").and_then(Value::as_u64).unwrap_or(1),
        })),
        "terminal_snapshot" => Some(json!({
            "type": "snapshot",
            "data": body.get("data").and_then(Value::as_str).unwrap_or_default(),
            "base_sequence": body.get("base_sequence").and_then(Value::as_u64).unwrap_or(0),
            "sequence": body.get("sequence").and_then(Value::as_u64).unwrap_or(0),
            "truncated": body.get("truncated").and_then(Value::as_bool).unwrap_or(false),
            "protocol_version": body.get("protocol_version").and_then(Value::as_u64).unwrap_or(1),
        })),
        "terminal_exit" => Some(json!({
            "type": "exit",
            "code": body.get("code").and_then(Value::as_i64).unwrap_or(0),
        })),
        "terminal_state" => Some(json!({
            "type": "state",
            "state": body.get("state").and_then(Value::as_str).unwrap_or("ready"),
            "busy": body.get("busy").and_then(Value::as_bool).unwrap_or(false),
            "snapshot_paging": true,
            "protocol_version": body.get("protocol_version").and_then(Value::as_u64).unwrap_or(1),
        })),
        "terminal_error" => Some(json!({
            "type": "error",
            "error": body.get("error").and_then(Value::as_str).unwrap_or("Local Connector terminal error"),
            "code": body.get("code").and_then(Value::as_str),
            "prompt": body.get("prompt").and_then(Value::as_str),
            "recoverable": body.get("recoverable").and_then(Value::as_bool).unwrap_or(false),
        })),
        _ => None,
    }
}

#[cfg(test)]
include!("terminal_relay_inline_tests.rs");
