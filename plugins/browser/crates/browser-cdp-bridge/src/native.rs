use std::{env, path::Path, time::Duration};

use async_tungstenite::{
    tokio::connect_async,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, header::SEC_WEBSOCKET_PROTOCOL},
    },
};
use futures::StreamExt;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{
    platform_data_dir, resolve_active_bridge_state,
    server::BridgeStateFile,
    wire::{CONTROL_SUBPROTOCOL, PROTOCOL_VERSION, WireError, WireMessage},
};

const NATIVE_MESSAGE_LIMIT: usize = 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(5);

pub async fn run_native_host(origin: String) -> Result<(), String> {
    let state_path = match env::var_os("CHATOS_BROWSER_BRIDGE_STATE") {
        Some(path) => path.into(),
        None => resolve_active_bridge_state(platform_data_dir().as_path()).await?,
    };
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    loop {
        let message = match read_native_message(&mut stdin).await? {
            Some(message) => message,
            None => return Ok(()),
        };
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let response = match handle_bootstrap(&state_path, &origin, &message).await {
            Ok(result) => json!({"type":"response", "id":id, "result":result}),
            Err(error) => json!({"type":"response", "id":id, "error":error}),
        };
        write_native_message(&mut stdout, &response).await?;
    }
}

async fn handle_bootstrap(
    state_path: &Path,
    origin: &str,
    message: &Value,
) -> Result<Value, WireError> {
    if message.get("type").and_then(Value::as_str) != Some("request")
        || message.get("method").and_then(Value::as_str) != Some("extension.bootstrap")
        || message
            .pointer("/params/protocol_version")
            .and_then(Value::as_str)
            != Some(PROTOCOL_VERSION)
    {
        return Err(WireError::new(
            "invalid_request",
            "Native host accepts only extension.bootstrap",
        ));
    }
    let bytes = tokio::fs::read(state_path)
        .await
        .map_err(|_| WireError::new("extension_unavailable", "Browser Bridge is not running"))?;
    if bytes.len() > 64 * 1024 {
        return Err(WireError::new(
            "invalid_request",
            "Browser Bridge state file is invalid",
        ));
    }
    let state: BridgeStateFile = serde_json::from_slice(&bytes)
        .map_err(|_| WireError::new("invalid_request", "Browser Bridge state file is invalid"))?;
    if state.protocol_version != PROTOCOL_VERSION || origin != state.allowed_extension_origin {
        return Err(WireError::new(
            "permission_denied",
            "Chrome extension identity was rejected",
        ));
    }
    let pairing_requested = message
        .pointer("/params/pairing_requested")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut request = state
        .control_endpoint
        .as_str()
        .into_client_request()
        .map_err(|_| {
            WireError::new(
                "extension_unavailable",
                "Browser Bridge endpoint is invalid",
            )
        })?;
    request.headers_mut().insert(
        SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static(CONTROL_SUBPROTOCOL),
    );
    let (mut socket, response) = tokio::time::timeout(CONTROL_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| WireError::new("timeout", "Browser Bridge connection timed out"))?
        .map_err(|_| WireError::new("extension_unavailable", "Browser Bridge is unavailable"))?;
    if response
        .headers()
        .get(SEC_WEBSOCKET_PROTOCOL)
        .and_then(|value| value.to_str().ok())
        != Some(CONTROL_SUBPROTOCOL)
    {
        return Err(WireError::new(
            "unsupported_by_backend",
            "Browser Bridge control protocol mismatch",
        ));
    }
    send_control(
        &mut socket,
        json!({
            "type":"request",
            "id":1,
            "method":"control.authenticate",
            "params":{"protocol_version":PROTOCOL_VERSION, "token":state.control_token}
        }),
    )
    .await?;
    read_control_response(&mut socket, 1).await?;
    send_control(
        &mut socket,
        json!({
            "type":"request",
            "id":2,
            "method":"control.bootstrapExtension",
            "params":{"origin":origin, "pairing_requested":pairing_requested}
        }),
    )
    .await?;
    let mut bootstrap = read_control_response(&mut socket, 2).await?;
    let Some(object) = bootstrap.as_object_mut() else {
        return Err(WireError::new(
            "backend_error",
            "Browser Bridge returned an invalid bootstrap response",
        ));
    };
    object.insert(
        "endpoint".into(),
        Value::String(state.extension_endpoint.clone()),
    );
    Ok(bootstrap)
}

async fn send_control<S>(
    socket: &mut async_tungstenite::WebSocketStream<S>,
    value: Value,
) -> Result<(), WireError>
where
    S: futures::AsyncRead + futures::AsyncWrite + Unpin,
{
    socket
        .send(Message::text(value.to_string()))
        .await
        .map_err(|_| WireError::new("extension_unavailable", "Browser Bridge write failed"))
}

async fn read_control_response<S>(
    socket: &mut async_tungstenite::WebSocketStream<S>,
    expected_id: u64,
) -> Result<Value, WireError>
where
    S: futures::AsyncRead + futures::AsyncWrite + Unpin,
{
    let frame = tokio::time::timeout(CONTROL_TIMEOUT, socket.next())
        .await
        .map_err(|_| WireError::new("timeout", "Browser Bridge response timed out"))?
        .ok_or_else(|| WireError::new("extension_unavailable", "Browser Bridge closed"))?
        .map_err(|_| WireError::new("extension_unavailable", "Browser Bridge read failed"))?;
    let Message::Text(text) = frame else {
        return Err(WireError::new(
            "backend_error",
            "Browser Bridge returned a non-JSON response",
        ));
    };
    let message: WireMessage = serde_json::from_str(&text)
        .map_err(|_| WireError::new("backend_error", "Browser Bridge response is invalid"))?;
    if message.kind != "response"
        || message.id.as_ref().and_then(Value::as_u64) != Some(expected_id)
    {
        return Err(WireError::new(
            "backend_error",
            "Browser Bridge response ID is invalid",
        ));
    }
    if let Some(error) = message.error {
        return Err(error);
    }
    Ok(message.result.unwrap_or(Value::Null))
}

async fn read_native_message<R>(reader: &mut R) -> Result<Option<Value>, String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(format!("native message header read failed: {error}")),
    }
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > NATIVE_MESSAGE_LIMIT {
        return Err("native message length is invalid".into());
    }
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|error| format!("native message body read failed: {error}"))?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "native message is invalid JSON".into())
}

async fn write_native_message<W>(writer: &mut W, value: &Value) -> Result<(), String>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let bytes =
        serde_json::to_vec(value).map_err(|_| "could not encode native response".to_owned())?;
    if bytes.len() > NATIVE_MESSAGE_LIMIT {
        return Err("native response exceeds 1 MiB".into());
    }
    writer
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .await
        .map_err(|error| format!("native response header write failed: {error}"))?;
    writer
        .write_all(&bytes)
        .await
        .map_err(|error| format!("native response body write failed: {error}"))?;
    writer
        .flush()
        .await
        .map_err(|error| format!("native response flush failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn native_messages_use_little_endian_length_framing() {
        let value = json!({"type":"request","id":"one"});
        let mut bytes = Vec::new();
        write_native_message(&mut bytes, &value).await.unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize,
            bytes.len() - 4
        );
        let mut input = bytes.as_slice();
        assert_eq!(read_native_message(&mut input).await.unwrap(), Some(value));
    }

    #[tokio::test]
    async fn oversized_native_message_is_rejected_before_allocation() {
        let mut bytes = ((NATIVE_MESSAGE_LIMIT + 1) as u32).to_le_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        let mut input = bytes.as_slice();
        assert!(read_native_message(&mut input).await.is_err());
    }
}
