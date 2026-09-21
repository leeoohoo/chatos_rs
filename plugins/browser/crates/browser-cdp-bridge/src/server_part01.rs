async fn handle_browser(mut socket: BridgeSocket, state: Arc<ServerState>) -> Result<(), String> {
    let authentication = next_wire(&mut socket).await?;
    let auth_id = request_id(&authentication)?;
    let token_valid = authentication.method.as_deref() == Some("bridge.authenticate")
        && authentication
            .params
            .get("protocol_version")
            .and_then(Value::as_str)
            == Some(PROTOCOL_VERSION)
        && authentication.params.get("token").and_then(Value::as_str)
            == Some(state.mcp_token.as_str())
        && state.mcp_expires_at_unix_ms > unix_ms();
    if !token_valid {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("token_expired", "Browser Bridge authentication failed"),
            ),
        )
        .await?;
        return Ok(());
    }
    if state.extension.lock().await.is_none() {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("extension_unavailable", "Chrome extension is not connected"),
            ),
        )
        .await?;
        return Ok(());
    }
    if state.mcp.lock().await.is_some() {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("invalid_request", "An MCP client is already connected"),
            ),
        )
        .await?;
        return Ok(());
    }
    let extension = state
        .extension
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Chrome extension disconnected during authentication".to_string())?;
    let extension_info = match extension
        .request("extension.getCapabilities", json!({}))
        .await
    {
        Ok(value) => value,
        Err(error) => {
            send_value(&mut socket, error_response(auth_id, error)).await?;
            return Ok(());
        }
    };
    let capabilities = extension_info
        .get("capabilities")
        .cloned()
        .unwrap_or_else(|| json!([]));
    if state
        .mcp_token_used
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        send_value(
            &mut socket,
            error_response(
                auth_id,
                WireError::new("invalid_request", "An MCP client is already connecting"),
            ),
        )
        .await?;
        return Ok(());
    }
    if let Err(error) = send_value(
        &mut socket,
        response(
            auth_id,
            json!({
                "protocol_version":PROTOCOL_VERSION,
                "connection_id":format!("bridge_{}", Uuid::new_v4().simple()),
                "product":"Chrome via Chatos Extension",
                "user_agent":"unavailable",
                "capabilities":capabilities
            }),
        ),
    )
    .await
    {
        state.mcp_token_used.store(false, Ordering::Release);
        return Err(error);
    }
    let (mut sink, mut stream) = socket.split();
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<Value>(CHANNEL_CAPACITY);
    let peer = McpPeer {
        id: format!("mcp_{}", Uuid::new_v4().simple()),
        outbound: outbound_tx,
    };
    *state.mcp.lock().await = Some(peer.clone());
    let mut writer = tokio::spawn(async move {
        while let Some(value) = outbound_rx.recv().await {
            let Ok(encoded) = serde_json::to_string(&value) else {
                break;
            };
            if encoded.len() > MAX_MESSAGE_BYTES || sink.send(Message::text(encoded)).await.is_err()
            {
                break;
            }
        }
    });
    while let Some(frame) = stream.next().await {
        let message = match frame {
            Ok(Message::Text(text)) if text.len() <= MAX_MESSAGE_BYTES => {
                match serde_json::from_str::<WireMessage>(&text) {
                    Ok(message) => message,
                    Err(_) => break,
                }
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            _ => break,
        };
        if message.kind != "request" {
            break;
        }
        let id = match request_id(&message) {
            Ok(id) => id,
            Err(_) => break,
        };
        let method = message.method.clone().unwrap_or_default();
        let closing = method == "bridge.close";
        let result = handle_browser_request(&state, &method, message.params).await;
        let value = match result {
            Ok(result) => response(id, result),
            Err(error) => error_response(id, error),
        };
        if peer.outbound.send(value).await.is_err() || closing {
            break;
        }
    }
    let peer_id = peer.id.clone();
    {
        let mut mcp = state.mcp.lock().await;
        if mcp.as_ref().is_some_and(|current| current.id == peer_id) {
            mcp.take();
            state.mcp_token_used.store(false, Ordering::Release);
        }
    }
    drop(peer);
    if tokio::time::timeout(Duration::from_secs(1), &mut writer)
        .await
        .is_err()
    {
        writer.abort();
    }
    Ok(())
}

async fn handle_browser_request(
    state: &ServerState,
    method: &str,
    params: Value,
) -> Result<Value, WireError> {
    let extension = match state.extension.lock().await.clone() {
        Some(extension) => extension,
        None if method == "bridge.close" => return Ok(json!({})),
        None => {
            return Err(WireError::new(
                "extension_unavailable",
                "Chrome extension is not connected",
            ));
        }
    };
    if method == "bridge.close" {
        return extension.request("extension.endSession", params).await;
    }
    let extension_method = match method {
        "bridge.configureSession" => "extension.configureSession",
        "bridge.listTargets" => "extension.listTargets",
        "bridge.createTarget" => "extension.createTarget",
        "bridge.closeTarget" => "extension.closeTarget",
        "bridge.attachTarget" => "extension.attachTarget",
        "bridge.detachTarget" => "extension.detachTarget",
        "cdp.send" => "extension.cdpSend",
        "bridge.subscribe" => "extension.subscribe",
        "bridge.unsubscribe" => "extension.unsubscribe",
        _ => {
            return Err(WireError::new(
                "unsupported_by_backend",
                format!("Unsupported Browser Bridge method: {method}"),
            ));
        }
    };
    extension.request(extension_method, params).await
}

async fn relay_extension_event(state: &ServerState, message: &WireMessage) {
    match message.method.as_deref() {
        Some("extension.unpair") => {
            state.paired.store(false, Ordering::Release);
            match tokio::fs::remove_file(&state.pairing_file).await {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => tracing_compat_warn(&format!(
                    "could not remove Browser Bridge pairing file: {error}"
                )),
            }
        }
        Some("extension.cdpEvent") => {
            if let Some(peer) = state.mcp.lock().await.clone() {
                let _ = peer
                    .outbound
                    .send(event("cdp.event", message.params.clone()))
                    .await;
            }
        }
        Some("extension.detached") => {
            notify_mcp_disconnected(state, "debugger_detached").await;
        }
        _ => {}
    }
}

async fn load_persisted_pairing(path: &Path, allowed_extension_origin: &str) -> bool {
    let Ok(metadata) = tokio::fs::symlink_metadata(path).await else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 16 * 1024 {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return false;
        }
    }
    let Ok(bytes) = tokio::fs::read(path).await else {
        return false;
    };
    let Ok(pairing) = serde_json::from_slice::<PersistedPairing>(&bytes) else {
        return false;
    };
    pairing.protocol_version == PROTOCOL_VERSION
        && pairing.allowed_extension_origin == allowed_extension_origin
}

async fn persist_pairing(state: &ServerState) -> Result<(), String> {
    write_private_json(
        &state.pairing_file,
        &PersistedPairing {
            protocol_version: PROTOCOL_VERSION.into(),
            allowed_extension_origin: state.allowed_extension_origin.clone(),
        },
    )
    .await
}

async fn notify_mcp_disconnected(state: &ServerState, reason: &str) {
    if let Some(peer) = state.mcp.lock().await.clone() {
        let _ = peer
            .outbound
            .send(event("bridge.disconnected", json!({"reason":reason})))
            .await;
    }
}

async fn next_wire(socket: &mut BridgeSocket) -> Result<WireMessage, String> {
    let frame = tokio::time::timeout(AUTH_TIMEOUT, socket.next())
        .await
        .map_err(|_| "WebSocket message timed out".to_owned())?
        .ok_or_else(|| "WebSocket closed".to_owned())?
        .map_err(|_| "WebSocket read failed".to_owned())?;
    let Message::Text(text) = frame else {
        return Err("WebSocket message must be JSON text".into());
    };
    if text.len() > MAX_MESSAGE_BYTES {
        return Err("WebSocket message exceeds 8 MiB".into());
    }
    serde_json::from_str(&text).map_err(|_| "WebSocket message is invalid JSON".into())
}

async fn send_value(socket: &mut BridgeSocket, value: Value) -> Result<(), String> {
    let encoded =
        serde_json::to_string(&value).map_err(|_| "could not encode response".to_owned())?;
    if encoded.len() > MAX_MESSAGE_BYTES {
        return Err("response exceeds 8 MiB".into());
    }
    socket
        .send(Message::text(encoded))
        .await
        .map_err(|_| "WebSocket write failed".to_owned())
}

fn request_id(message: &WireMessage) -> Result<Value, String> {
    if message.kind != "request" {
        return Err("expected a request message".into());
    }
    message
        .id
        .clone()
        .filter(|id| id.is_number() || id.is_string())
        .ok_or_else(|| "request omitted its ID".into())
}

fn offered_protocol(header: &str, expected: &str) -> bool {
    header.split(',').any(|value| value.trim() == expected)
}

fn random_token(prefix: &str) -> String {
    format!(
        "{prefix}_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn validate_extension_id(extension_id: &str) -> Result<(), String> {
    if extension_id.len() != 32
        || !extension_id
            .chars()
            .all(|character| matches!(character, 'a'..='p'))
    {
        return Err("Chrome extension ID must contain 32 characters in the range a-p".into());
    }
    Ok(())
}

async fn write_private_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    tokio::fs::write(path, bytes)
        .await
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| format!("could not secure {}: {error}", path.display()))?;
    }
    Ok(())
}

fn tracing_compat_warn(error: &str) {
    eprintln!("Browser Bridge connection closed: {error}");
}

#[cfg(test)]
include!("server_inline_tests.rs");
