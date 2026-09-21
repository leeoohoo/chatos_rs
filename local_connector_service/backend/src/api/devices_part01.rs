async fn release_device_session(
    state: &AppState,
    owner_user_id: &str,
    device_id: &str,
) -> Result<(), ApiError> {
    let active_session = state
        .store
        .active_session(owner_user_id)
        .await
        .map_err(ApiError::internal)?;
    if active_session
        .as_ref()
        .is_some_and(|item| item.device_id == device_id)
    {
        let session = active_session.expect("active session checked above");
        state
            .store
            .close_session(owner_user_id, session.id.as_str(), device_id)
            .await
            .map_err(ApiError::internal)?;
        state
            .relay
            .unregister_session(device_id, session.id.as_str())
            .await;
        if let Some(presence) = state
            .device_presence(device_id)
            .await
            .map_err(ApiError::internal)?
        {
            if presence.owner_user_id == owner_user_id && presence.session_id == session.id {
                state
                    .unregister_device_presence(&presence)
                    .await
                    .map_err(ApiError::internal)?;
            }
        }
    } else {
        state
            .store
            .close_device_session(owner_user_id, device_id)
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(())
}

fn resolve_owner_user_id(
    requested_user_id: Option<String>,
    user: &CurrentUser,
) -> Result<String, ApiError> {
    let owner_user_id = user.effective_owner_user_id();
    if let Some(requested) = normalize_optional_text(requested_user_id) {
        if requested != owner_user_id {
            return Err(ApiError::forbidden("user_id 与登录用户不一致"));
        }
    }
    Ok(owner_user_id.to_string())
}

fn is_heartbeat_message(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("ping") || trimmed.eq_ignore_ascii_case("heartbeat") {
        return true;
    }
    serde_json::from_str::<Value>(trimmed)
        .ok()
        .and_then(|value| {
            value.get("type").and_then(Value::as_str).map(|item| {
                item.eq_ignore_ascii_case("ping") || item.eq_ignore_ascii_case("heartbeat")
            })
        })
        .unwrap_or(false)
}

async fn verify_device_connect_signature(
    state: &AppState,
    headers: &HeaderMap,
    device: &LocalConnectorDevice,
) -> Result<Option<String>, ApiError> {
    if !state.config.require_device_connect_signature {
        return Ok(None);
    }
    let public_key = device_public_key_bytes(device.public_key.as_str())?;
    let algorithm = required_header(headers, "x-local-connector-device-signature-alg")?;
    if algorithm != "ed25519" {
        return Err(ApiError::unauthorized(
            "Local Connector device signature algorithm is not supported",
        ));
    }
    let header_device_id = required_header(headers, "x-local-connector-device-id")?;
    if header_device_id != device.id {
        return Err(ApiError::unauthorized(
            "Local Connector device signature device id does not match",
        ));
    }
    let timestamp = required_header(headers, "x-local-connector-device-timestamp")?
        .parse::<i64>()
        .map_err(|_| ApiError::unauthorized("Local Connector device timestamp is invalid"))?;
    let now = Utc::now().timestamp();
    let max_skew = state
        .config
        .device_connect_signature_max_skew
        .as_secs()
        .try_into()
        .unwrap_or(300_i64);
    if now.saturating_sub(timestamp).abs() > max_skew {
        return Err(ApiError::unauthorized(
            "Local Connector device signature timestamp is outside the allowed window",
        ));
    }
    let nonce = required_header(headers, "x-local-connector-device-nonce")?;
    if nonce.len() < 16 || nonce.len() > 128 {
        return Err(ApiError::unauthorized(
            "Local Connector device signature nonce is invalid",
        ));
    }
    let signature = required_header(headers, "x-local-connector-device-signature")?;
    let signature = URL_SAFE_NO_PAD.decode(signature.as_bytes()).map_err(|_| {
        ApiError::unauthorized("Local Connector device signature encoding is invalid")
    })?;
    let path = format!("/api/local-connectors/devices/{}/connect", device.id);
    let signature_version = headers
        .get("x-local-connector-device-signature-version")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("v1");
    let windows_user_sid = match signature_version {
        "v1" => None,
        "v2" => Some(
            normalize_windows_sid(
                required_header(headers, "x-local-connector-windows-user-sid")?.as_str(),
            )
            .map_err(ApiError::unauthorized)?,
        ),
        _ => {
            return Err(ApiError::unauthorized(
                "Local Connector device signature version is not supported",
            ))
        }
    };
    let payload = match windows_user_sid.as_deref() {
        Some(sid) => device_signature_payload_v2(
            device.id.as_str(),
            timestamp,
            nonce.as_str(),
            path.as_str(),
            sid,
        ),
        None => {
            device_signature_payload(device.id.as_str(), timestamp, nonce.as_str(), path.as_str())
        }
    };
    UnparsedPublicKey::new(&ED25519, public_key.as_slice())
        .verify(payload.as_bytes(), signature.as_slice())
        .map_err(|_| ApiError::unauthorized("Local Connector device signature is invalid"))?;
    let nonce_consumed = state
        .consume_device_connect_nonce(device.id.as_str(), nonce.as_str())
        .await
        .map_err(ApiError::internal)?;
    if !nonce_consumed {
        return Err(ApiError::unauthorized(
            "Local Connector device signature nonce was already used",
        ));
    }
    Ok(windows_user_sid)
}

fn device_public_key_bytes(value: &str) -> Result<Vec<u8>, ApiError> {
    let encoded = value.trim().strip_prefix("ed25519:").ok_or_else(|| {
        ApiError::unauthorized(
            "Local Connector device key is not an ed25519 public key; re-register the device",
        )
    })?;
    let bytes = URL_SAFE_NO_PAD.decode(encoded.as_bytes()).map_err(|_| {
        ApiError::unauthorized("Local Connector device public key encoding is invalid")
    })?;
    if bytes.len() != 32 {
        return Err(ApiError::unauthorized(
            "Local Connector device public key length is invalid",
        ));
    }
    Ok(bytes)
}

fn required_header(headers: &HeaderMap, name: &'static str) -> Result<String, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| ApiError::unauthorized(format!("{name} is required")))
}

fn device_signature_payload(device_id: &str, timestamp: i64, nonce: &str, path: &str) -> String {
    format!("v1\n{device_id}\n{timestamp}\n{nonce}\n{path}")
}

fn device_signature_payload_v2(
    device_id: &str,
    timestamp: i64,
    nonce: &str,
    path: &str,
    windows_user_sid: &str,
) -> String {
    format!("v2\n{device_id}\n{timestamp}\n{nonce}\n{path}\n{windows_user_sid}")
}
