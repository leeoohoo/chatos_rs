// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io::{Read, Write};
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use reqwest::header::AUTHORIZATION;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::chrome_bridge::{CHROME_EXTENSION_ORIGIN, CHROME_NATIVE_PROTOCOL_VERSION};
use crate::chrome_integration::{default_chrome_rendezvous_path, load_chrome_native_rendezvous};

const MAX_NATIVE_MESSAGE_BYTES: usize = 1024 * 1024;
const HOST_HTTP_TIMEOUT: Duration = Duration::from_secs(20);
const INPUT_QUEUE_CAPACITY: usize = 64;

enum NativeInput {
    Message(Value),
    Closed,
    Error(String),
}

pub async fn run_chrome_native_host() -> Result<()> {
    let origin = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow!("Chrome extension origin argument is missing"))?;
    if origin != CHROME_EXTENSION_ORIGIN {
        bail!("Chrome extension origin is not trusted");
    }
    let rendezvous_path = default_chrome_rendezvous_path()?;
    let rendezvous = load_chrome_native_rendezvous(rendezvous_path.as_path())?;
    let client = reqwest::Client::builder()
        .timeout(HOST_HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .context("build Chrome Native Host HTTP client")?;
    let connection_id = Uuid::new_v4().to_string();
    call_core(
        &client,
        rendezvous.api_base_url.as_str(),
        rendezvous.auth_token.as_str(),
        "/api/local/chrome/native/connect",
        json!({
            "connection_id": connection_id,
            "origin": origin,
            "protocol_version": CHROME_NATIVE_PROTOCOL_VERSION,
        }),
    )
    .await?;

    let mut stdout = std::io::stdout().lock();
    write_native_message(
        &mut stdout,
        &json!({
            "type": "host_ready",
            "connection_id": connection_id,
            "protocol_version": CHROME_NATIVE_PROTOCOL_VERSION,
        }),
    )?;

    let (input_tx, mut input_rx) = mpsc::channel(INPUT_QUEUE_CAPACITY);
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            let input = match read_native_message(&mut stdin) {
                Ok(Some(value)) => NativeInput::Message(value),
                Ok(None) => NativeInput::Closed,
                Err(error) => NativeInput::Error(error.to_string()),
            };
            let terminal = matches!(input, NativeInput::Closed | NativeInput::Error(_));
            if input_tx.blocking_send(input).is_err() || terminal {
                break;
            }
        }
    });

    let loop_result = loop {
        let poll = call_core(
            &client,
            rendezvous.api_base_url.as_str(),
            rendezvous.auth_token.as_str(),
            "/api/local/chrome/native/next",
            json!({"connection_id": connection_id}),
        );
        tokio::select! {
            input = input_rx.recv() => {
                match input {
                    Some(NativeInput::Message(event)) => {
                        if let Err(error) = call_core(
                            &client,
                            rendezvous.api_base_url.as_str(),
                            rendezvous.auth_token.as_str(),
                            "/api/local/chrome/native/event",
                            json!({"connection_id": connection_id, "event": event}),
                        ).await {
                            break Err(error);
                        }
                    }
                    Some(NativeInput::Closed) | None => break Ok(()),
                    Some(NativeInput::Error(error)) => break Err(anyhow!(error)),
                }
            }
            next = poll => {
                match next {
                    Ok(value) => {
                        if let Some(command) = value.get("command").filter(|value| !value.is_null()) {
                            write_native_message(&mut stdout, command)?;
                        }
                    }
                    Err(error) => break Err(error),
                }
            }
        }
    };

    let _ = call_core(
        &client,
        rendezvous.api_base_url.as_str(),
        rendezvous.auth_token.as_str(),
        "/api/local/chrome/native/disconnect",
        json!({"connection_id": connection_id}),
    )
    .await;
    if let Err(error) = &loop_result {
        let _ = write_native_message(
            &mut stdout,
            &json!({
                "type": "host_error",
                "error": sanitize_host_error(error.to_string().as_str()),
            }),
        );
    }
    loop_result
}

async fn call_core(
    client: &reqwest::Client,
    base_url: &str,
    auth_token: &str,
    path: &str,
    body: Value,
) -> Result<Value> {
    let response = client
        .post(format!("{base_url}{path}"))
        .header(AUTHORIZATION, format!("Bearer {auth_token}"))
        .json(&body)
        .send()
        .await
        .context("call Local Connector Chrome bridge")?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_NATIVE_MESSAGE_BYTES as u64)
    {
        bail!("Local Connector Chrome bridge response exceeded the safety limit");
    }
    let bytes = response
        .bytes()
        .await
        .context("read Local Connector Chrome bridge response")?;
    if bytes.len() > MAX_NATIVE_MESSAGE_BYTES {
        bail!("Local Connector Chrome bridge response exceeded the safety limit");
    }
    let value = serde_json::from_slice::<Value>(&bytes)
        .context("decode Local Connector Chrome bridge response")?;
    if !status.is_success() {
        let message = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("Local Connector Chrome bridge rejected the request");
        bail!(sanitize_host_error(message));
    }
    Ok(value)
}

fn read_native_message<R: Read>(reader: &mut R) -> Result<Option<Value>> {
    let mut length_bytes = [0_u8; 4];
    match reader.read_exact(&mut length_bytes) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error).context("read Chrome Native Messaging length"),
    }
    let length = u32::from_ne_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_NATIVE_MESSAGE_BYTES {
        bail!("Chrome Native Messaging input exceeded the safety limit");
    }
    let mut bytes = vec![0_u8; length];
    reader
        .read_exact(&mut bytes)
        .context("read Chrome Native Messaging payload")?;
    let value = serde_json::from_slice::<Value>(&bytes)
        .context("decode Chrome Native Messaging payload")?;
    if !value.is_object() {
        bail!("Chrome Native Messaging payload must be a JSON object");
    }
    Ok(Some(value))
}

fn write_native_message<W: Write>(writer: &mut W, value: &Value) -> Result<()> {
    if !value.is_object() {
        bail!("Chrome Native Messaging output must be a JSON object");
    }
    let bytes = serde_json::to_vec(value).context("encode Chrome Native Messaging output")?;
    if bytes.is_empty() || bytes.len() > MAX_NATIVE_MESSAGE_BYTES {
        bail!("Chrome Native Messaging output exceeded the safety limit");
    }
    let length = u32::try_from(bytes.len()).context("Chrome Native Messaging output length")?;
    writer
        .write_all(&length.to_ne_bytes())
        .context("write Chrome Native Messaging length")?;
    writer
        .write_all(&bytes)
        .context("write Chrome Native Messaging payload")?;
    writer
        .flush()
        .context("flush Chrome Native Messaging output")
}

fn sanitize_host_error(value: &str) -> String {
    let value = value
        .chars()
        .filter(|character| !character.is_control())
        .take(512)
        .collect::<String>();
    if value.is_empty() {
        "Chrome Native Host failed".to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn native_message_round_trip_is_length_bounded() {
        let value = json!({"type":"hello","extension_id":"demo"});
        let mut bytes = Vec::new();
        write_native_message(&mut bytes, &value).expect("write");
        assert_eq!(
            read_native_message(&mut Cursor::new(bytes))
                .expect("read")
                .expect("message"),
            value
        );
    }

    #[test]
    fn native_message_rejects_oversized_length_before_allocating() {
        let bytes = ((MAX_NATIVE_MESSAGE_BYTES as u32) + 1)
            .to_ne_bytes()
            .to_vec();
        assert!(read_native_message(&mut Cursor::new(bytes)).is_err());
    }
}
