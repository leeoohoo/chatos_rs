async fn wait_for_remote_terminal_verification(
    socket: &mut WebSocket,
    cols: &mut u16,
    rows: &mut u16,
) -> Option<String> {
    let deadline = tokio::time::sleep(Duration::from_secs(300));
    tokio::pin!(deadline);
    loop {
        let message = tokio::select! {
            _ = &mut deadline => return None,
            message = socket.recv() => message,
        };
        match message {
            Some(Ok(Message::Text(text))) => {
                let Ok(value) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                match value.get("type").and_then(Value::as_str) {
                    Some("verification") => {
                        if let Some(code) = value
                            .get("code")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            return Some(code.to_string());
                        }
                    }
                    Some("resize") => {
                        *cols = value
                            .get("cols")
                            .and_then(Value::as_u64)
                            .unwrap_or(*cols as u64)
                            .clamp(1, u16::MAX as u64) as u16;
                        *rows = value
                            .get("rows")
                            .and_then(Value::as_u64)
                            .unwrap_or(*rows as u64)
                            .clamp(1, u16::MAX as u64) as u16;
                    }
                    Some("ping") => {
                        match socket
                            .send(Message::Text(json!({"type": "pong"}).to_string().into()))
                            .await
                        {
                            Ok(()) => {}
                            Err(_) => return None,
                        }
                    }
                    _ => {}
                }
            }
            Some(Ok(Message::Ping(bytes))) => {
                if socket.send(Message::Pong(bytes)).await.is_err() {
                    return None;
                }
            }
            Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
            Some(Ok(_)) => {}
        }
    }
}

async fn handle_remote_terminal_ws_message(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    message: Message,
) -> bool {
    match message {
        Message::Text(text) => {
            let value = serde_json::from_str::<Value>(text.as_str())
                .unwrap_or_else(|_| json!({ "type": "input", "data": text.as_str() }));
            match value.get("type").and_then(Value::as_str) {
                Some("input") => send_remote_terminal_control(
                    state,
                    owner_user_id,
                    device_id,
                    workspace_id,
                    terminal_session_id,
                    "remote_terminal_input",
                    json!({ "data": value.get("data").and_then(Value::as_str).unwrap_or_default() }),
                )
                .await,
                Some("resize") => send_remote_terminal_control(
                    state,
                    owner_user_id,
                    device_id,
                    workspace_id,
                    terminal_session_id,
                    "remote_terminal_resize",
                    json!({
                        "cols": value.get("cols").and_then(Value::as_u64).unwrap_or(80),
                        "rows": value.get("rows").and_then(Value::as_u64).unwrap_or(24),
                    }),
                )
                .await,
                Some("snapshot") => send_remote_terminal_control(
                    state,
                    owner_user_id,
                    device_id,
                    workspace_id,
                    terminal_session_id,
                    "remote_terminal_snapshot_request",
                    json!({
                        "lines": value.get("lines").and_then(Value::as_u64).unwrap_or(500),
                    }),
                )
                .await,
                Some("command") => {
                    let mut command = value
                        .get("command")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    if !command.ends_with('\n') {
                        command.push('\n');
                    }
                    send_remote_terminal_control(
                        state,
                        owner_user_id,
                        device_id,
                        workspace_id,
                        terminal_session_id,
                        "remote_terminal_input",
                        json!({ "data": command }),
                    )
                    .await
                }
                Some("close") => {
                    let _ = send_remote_terminal_control(
                        state,
                        owner_user_id,
                        device_id,
                        workspace_id,
                        terminal_session_id,
                        "remote_terminal_close",
                        json!({}),
                    )
                    .await;
                    false
                }
                Some("verification") | Some("ping") => true,
                _ => true,
            }
        }
        Message::Binary(bytes) => {
            let data = String::from_utf8_lossy(&bytes).into_owned();
            send_remote_terminal_control(
                state,
                owner_user_id,
                device_id,
                workspace_id,
                terminal_session_id,
                "remote_terminal_input",
                json!({ "data": data }),
            )
            .await
        }
        Message::Ping(_) | Message::Pong(_) => true,
        Message::Close(_) => false,
    }
}

async fn send_remote_terminal_control(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
    workspace_id: &str,
    terminal_session_id: &str,
    message_type: &str,
    mut body: Value,
) -> bool {
    if let Value::Object(ref mut map) = body {
        map.insert(
            "terminal_session_id".to_string(),
            Value::String(terminal_session_id.to_string()),
        );
    }
    send_relay(
        state,
        RelayRequest {
            message_type: message_type.to_string(),
            request_id: Uuid::new_v4().to_string(),
            owner_user_id: owner_user_id.to_string(),
            device_id: device_id.to_string(),
            workspace_id: workspace_id.to_string(),
            method: "POST".to_string(),
            path: format!("/remote-connections/terminal/{message_type}"),
            headers: BTreeMap::new(),
            body,
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        },
    )
    .await
    .is_ok()
}

#[cfg(test)]
include!("remote_connection_relay_inline_tests.rs");
