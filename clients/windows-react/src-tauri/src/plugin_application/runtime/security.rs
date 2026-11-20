use super::{
    plugin_runtime_error, validate_text, PluginApplicationBridgeRequest,
    PluginApplicationContextSelection, PluginApplicationLaunchRequest, RunningPluginApplication,
    PLUGIN_SESSION_BOOTSTRAP_PATH,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use ring::{
    hmac,
    rand::{SecureRandom, SystemRandom},
};
use sha2::{Digest, Sha256};

pub(super) const MAXIMUM_BRIDGE_REQUEST_BYTES: usize = 256 * 1_024;

pub(super) fn authenticated_launch_url(process: &RunningPluginApplication) -> String {
    format!(
        "{}{}?token={}",
        process.base_url.trim_end_matches('/'),
        PLUGIN_SESSION_BOOTSTRAP_PATH,
        process.authentication_token
    )
}

pub(super) fn secure_random_token() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "无法生成 Plugin 应用启动凭据。".to_owned())?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

pub(super) fn health_proof(authentication_token: &str, challenge: &str) -> String {
    URL_SAFE_NO_PAD.encode(
        hmac::sign(
            &hmac::Key::new(hmac::HMAC_SHA256, authentication_token.as_bytes()),
            challenge.as_bytes(),
        )
        .as_ref(),
    )
}

pub(super) fn validate_launch_request(
    request: &PluginApplicationLaunchRequest,
) -> Result<(), String> {
    validate_text(&request.owner_user_id, 512)?;
    validate_text(&request.presentation_id, 512)?;
    validate_text(&request.plugin_id, 1_024)?;
    validate_text(&request.component_key, 1_024)?;
    if request.generation == 0 {
        return Err(plugin_runtime_error());
    }
    if let PluginApplicationContextSelection::Project { project_id } = &request.context {
        validate_text(project_id, 1_024)?;
    }
    Ok(())
}

pub(super) fn validate_bridge_request(
    request: &PluginApplicationBridgeRequest,
) -> Result<(), String> {
    validate_text(&request.owner_user_id, 512)?;
    validate_text(&request.presentation_id, 512)?;
    validate_text(&request.session_id, 256)?;
    validate_text(&request.request_id, 256)?;
    validate_text(&request.method, 256)?;
    if request.generation == 0
        || request.host_session_nonce.len() != 64
        || !request
            .host_session_nonce
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || validated_loopback_origin(&request.origin)? != request.origin
        || !request.payload.is_object()
        || serde_json::to_vec(request)
            .map(|bytes| bytes.len() > MAXIMUM_BRIDGE_REQUEST_BYTES)
            .unwrap_or(true)
    {
        return Err(stale_bridge_error());
    }
    Ok(())
}

pub(super) fn validated_loopback_origin(value: &str) -> Result<String, String> {
    let url = reqwest::Url::parse(value).map_err(|_| stale_bridge_error())?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(stale_bridge_error());
    }
    Ok(url.origin().ascii_serialization())
}

pub(super) fn storage_partition_id(application_id: &str, scope_key: &str) -> String {
    let value = sha256(&format!("{application_id}\n{scope_key}"));
    format!(
        "{}-{}-{}-{}-{}",
        &value[0..8],
        &value[8..12],
        &value[12..16],
        &value[16..20],
        &value[20..32]
    )
}

pub(super) fn sha256(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

pub(super) fn stale_launch_error() -> String {
    "Plugin 应用启动请求已失效。".into()
}

pub(super) fn stale_bridge_error() -> String {
    "Plugin 应用 Bridge 会话无效。".into()
}
