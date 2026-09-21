async fn issue_client_session(
    state: &AppState,
    user: &UserRecord,
    external_identity_id: Option<&str>,
    device_id: &str,
    device_public_key: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    let issued = issue_user_token_with_scopes(
        &state.config,
        user,
        state.config.wechat_mini_program_client_session_ttl_seconds,
        vec![WECHAT_COMPANION_SCOPE.to_string()],
    )
    .map_err(internal_error)?;
    let now = now_rfc3339();
    let session = ClientSessionRecord {
        id: Uuid::new_v4().to_string(),
        user_id: user.id.clone(),
        client_type: CLIENT_TYPE_WECHAT_MINI_PROGRAM.to_string(),
        external_identity_id: external_identity_id.map(ToOwned::to_owned),
        token_jti: issued.jti,
        device_id: Some(device_id.to_string()),
        device_public_key: Some(device_public_key.to_string()),
        created_at: now.clone(),
        updated_at: now.clone(),
        last_seen_at: now,
        expires_at_unix: issued.expires_at_unix,
        revoked_at: None,
        revoked_by: None,
    };
    state
        .store
        .insert_client_session(&session)
        .await
        .map_err(internal_error)?;
    Ok((issued.token, session.id))
}

fn validate_device_identity(
    device_id: &str,
    device_public_key: &str,
) -> Result<(String, String), (StatusCode, Json<Value>)> {
    let device_id = device_id.trim();
    if !(16..=128).contains(&device_id.len())
        || device_id.contains(char::is_whitespace)
        || !device_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_'))
    {
        return Err(bad_request("device_id is invalid"));
    }
    let public_key = device_public_key.trim();
    let encoded = public_key
        .strip_prefix("ed25519:")
        .ok_or_else(|| bad_request("device_public_key is invalid"))?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|_| bad_request("device_public_key is invalid"))?;
    if decoded.len() != 32 {
        return Err(bad_request("device_public_key is invalid"));
    }
    Ok((device_id.to_string(), public_key.to_string()))
}

async fn exchange_code(
    state: &AppState,
    code: &str,
) -> Result<WeChatMiniProgramIdentity, WeChatExchangeError> {
    let client = state
        .wechat_mini_program
        .as_ref()
        .ok_or(WeChatExchangeError::NotConfigured)?;
    client.exchange_code(code).await
}

fn configured_client(
    state: &AppState,
) -> Result<&WeChatMiniProgramClient, (StatusCode, Json<Value>)> {
    state
        .wechat_mini_program
        .as_ref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))
}

fn configured_app_id(state: &AppState) -> Result<&str, (StatusCode, Json<Value>)> {
    Ok(configured_client(state)?.app_id())
}

fn hash_provider_identity(
    state: &AppState,
    identity: &WeChatMiniProgramIdentity,
) -> Result<(String, Option<String>), (StatusCode, Json<Value>)> {
    Ok((
        hash_secret(state, "openid", identity.open_id.as_str())?,
        identity
            .union_id
            .as_deref()
            .map(|value| hash_secret(state, "unionid", value))
            .transpose()?,
    ))
}

fn hash_secret(
    state: &AppState,
    domain: &str,
    value: &str,
) -> Result<String, (StatusCode, Json<Value>)> {
    let secret = state
        .config
        .wechat_mini_program_identity_hash_secret
        .as_deref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))?;
    let app_id = state
        .config
        .wechat_mini_program_app_id
        .as_deref()
        .ok_or_else(|| service_unavailable("WeChat Mini Program login is not configured"))?;
    Ok(hex::encode(Sha256::digest(
        format!("chatos-wechat-v1:{domain}:{secret}:{app_id}:{value}").as_bytes(),
    )))
}

fn validate_secret(value: &str, field: &str) -> Result<(), (StatusCode, Json<Value>)> {
    let value = value.trim();
    if value.is_empty() || value.len() > SECRET_MAX_BYTES || value.contains(char::is_whitespace) {
        return Err(bad_request(format!("{field} is invalid")));
    }
    Ok(())
}

fn generate_secret(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::fill(value.as_mut_slice());
    hex::encode(value)
}

fn human_user_id(principal: &CurrentPrincipal) -> Result<&str, (StatusCode, Json<Value>)> {
    if principal.principal_type != PRINCIPAL_TYPE_HUMAN_USER {
        return Err(forbidden("human user session required"));
    }
    principal
        .user_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| forbidden("human user session required"))
}

async fn load_enabled_user(
    state: &AppState,
    user_id: &str,
) -> Result<UserRecord, (StatusCode, Json<Value>)> {
    let user = state
        .store
        .find_user_by_id(user_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| forbidden("account is unavailable"))?;
    if !user.enabled {
        return Err(forbidden("account is unavailable"));
    }
    Ok(user)
}

fn auth_user(user: &UserRecord) -> AuthUser {
    AuthUser {
        id: user.id.clone(),
        username: user.username.clone(),
        display_name: user.display_name.clone(),
        role: user.role.clone(),
        principal_type: PRINCIPAL_TYPE_HUMAN_USER.to_string(),
    }
}

async fn reject_locked(
    state: &AppState,
    identity: &str,
    source: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    if state
        .login_throttle
        .is_locked(
            identity,
            Some(source),
            Utc::now().timestamp(),
            &state.config,
        )
        .await
        .map_err(internal_error)?
    {
        Err(bad_request("WeChat authentication is temporarily locked"))
    } else {
        Ok(())
    }
}

async fn record_failure(
    state: &AppState,
    identity: &str,
    source: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    state
        .login_throttle
        .record_failure(
            identity,
            Some(source),
            Utc::now().timestamp(),
            &state.config,
        )
        .await
        .map_err(internal_error)
}

fn wechat_source(headers: &HeaderMap, addr: SocketAddr) -> String {
    format!(
        "wechat:{}",
        crate::login_throttle::request_source(headers, addr)
    )
}

fn map_exchange_error(error: WeChatExchangeError) -> (StatusCode, Json<Value>) {
    match error {
        WeChatExchangeError::NotConfigured => service_unavailable(error.public_message()),
        WeChatExchangeError::InvalidCode | WeChatExchangeError::ProviderRejected { .. } => {
            bad_request(error.public_message())
        }
        WeChatExchangeError::Transport | WeChatExchangeError::InvalidResponse => {
            service_unavailable(error.public_message())
        }
    }
}

#[cfg(test)]
include!("wechat_auth_inline_tests.rs");
