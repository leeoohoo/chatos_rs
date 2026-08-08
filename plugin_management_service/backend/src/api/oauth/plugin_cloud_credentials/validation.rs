// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(in crate::api) struct ParsedTemplate {
    pub(in crate::api) prefix: String,
    pub(in crate::api) secret_name: Option<String>,
    pub(in crate::api) suffix: String,
}

pub(in crate::api) fn parse_template(value: &str) -> Result<ParsedTemplate, ApiError> {
    if value.is_empty() || value.len() > MAX_TEMPLATE_BYTES || value.chars().any(char::is_control) {
        return Err(ApiError::conflict("Plugin MCP template is invalid"));
    }
    let Some(start) = value.find(CREDENTIAL_PLACEHOLDER_PREFIX) else {
        return Ok(ParsedTemplate {
            prefix: value.to_string(),
            secret_name: None,
            suffix: String::new(),
        });
    };
    let remainder = &value[start + CREDENTIAL_PLACEHOLDER_PREFIX.len()..];
    let end = remainder
        .find('}')
        .ok_or_else(|| ApiError::conflict("Plugin credential template is invalid"))?;
    let secret_name = validate_scope_segment(&remainder[..end], "secret_name")?;
    let suffix = &remainder[end + 1..];
    if suffix.contains(CREDENTIAL_PLACEHOLDER_PREFIX) {
        return Err(ApiError::conflict(
            "Plugin credential template contains multiple placeholders",
        ));
    }
    Ok(ParsedTemplate {
        prefix: value[..start].to_string(),
        secret_name: Some(secret_name),
        suffix: suffix.to_string(),
    })
}

pub(in crate::api) fn require_credential_permission(
    _bundle: &PluginMcpCloudRuntimeBundle,
    permissions: &[String],
) -> Result<(), ApiError> {
    if permissions.iter().any(|permission| {
        permission == "credential.use" || permission.starts_with("credential.use:")
    }) {
        Ok(())
    } else {
        Err(ApiError::conflict(
            "Plugin cloud credential requires signed credential.use permission",
        ))
    }
}

pub(in crate::api) fn require_oauth_permissions(
    provider: &str,
    scopes: &[String],
    permissions: &[String],
) -> Result<(), ApiError> {
    for scope in scopes {
        let required = format!("oauth.scope:{provider}:{scope}");
        if !permissions.iter().any(|permission| permission == &required) {
            return Err(ApiError::conflict(format!(
                "Plugin cloud OAuth requires signed permission: {required}"
            )));
        }
    }
    Ok(())
}

pub(in crate::api) fn normalize_scopes(values: Vec<String>) -> Result<Vec<String>, ApiError> {
    let mut scopes = BTreeSet::new();
    for value in values {
        let value = validate_text(value.as_str(), "scope", 256)?;
        if !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/' | b'+' | b'=')
        }) {
            return Err(ApiError::bad_request(
                "OAuth scope contains unsupported characters",
            ));
        }
        scopes.insert(value);
    }
    if scopes.len() > 64 {
        return Err(ApiError::bad_request("OAuth scopes exceed the limit"));
    }
    if scopes.is_empty() {
        return Err(ApiError::bad_request(
            "Plugin cloud OAuth requires at least one signed scope",
        ));
    }
    Ok(scopes.into_iter().collect())
}

pub(in crate::api) fn validate_scope_segment(value: &str, field: &str) -> Result<String, ApiError> {
    let value = required_text(Some(value), field)?;
    if value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ApiError::bad_request(format!(
            "{field} contains unsupported characters"
        )));
    }
    Ok(value)
}

pub(in crate::api) fn validate_identifier(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<String, ApiError> {
    let value = validate_text(value, field, max_bytes)?.to_ascii_lowercase();
    if !value.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
    }) {
        return Err(ApiError::bad_request(format!(
            "{field} contains unsupported characters"
        )));
    }
    Ok(value)
}

pub(in crate::api) fn validate_text(
    value: &str,
    field: &str,
    max_bytes: usize,
) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request(format!("{field} is invalid")));
    }
    Ok(value.to_string())
}

pub(in crate::api) fn validate_secret_value(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > MAX_SECRET_BYTES || value.contains('\0') {
        return Err(ApiError::bad_request(
            "Plugin cloud credential is empty, oversized, or contains NUL",
        ));
    }
    Ok(())
}

pub(in crate::api) fn validate_access_token(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > MAX_SECRET_BYTES || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request("OAuth access token is invalid"));
    }
    Ok(())
}

pub(in crate::api) fn validate_future_expiry(
    value: Option<&str>,
) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| ApiError::bad_request("OAuth expiry must be RFC3339"))?;
    if parsed.timestamp() <= Utc::now().timestamp() {
        return Err(ApiError::bad_request(
            "OAuth access token is already expired",
        ));
    }
    Ok(Some(parsed.to_rfc3339()))
}

pub(in crate::api) fn normalize_header_name(value: &str) -> Result<String, ApiError> {
    let normalized = value.trim().to_ascii_lowercase();
    reqwest::header::HeaderName::from_bytes(normalized.as_bytes())
        .map_err(|_| ApiError::conflict("Plugin HTTP header name is invalid"))?;
    if matches!(
        normalized.as_str(),
        "host" | "content-length" | "transfer-encoding" | "connection" | "proxy-authorization"
    ) {
        return Err(ApiError::conflict(
            "Plugin HTTP header is controlled by the cloud Host",
        ));
    }
    Ok(normalized)
}

pub(in crate::api) fn contains_authorization_header(headers: &BTreeMap<String, String>) -> bool {
    headers
        .keys()
        .any(|name| name.trim().eq_ignore_ascii_case("authorization"))
}

pub(in crate::api) fn credential_aad(metadata: &PluginCloudCredentialMetadata) -> String {
    format!(
        "chatos.plugin.cloud-credential.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        metadata.id,
        metadata.owner_user_id,
        metadata.plugin_id,
        metadata.release_id,
        metadata.component_key,
        metadata.secret_name,
        metadata.revision,
    )
}

pub(in crate::api) fn oauth_aad(connection: &PluginCloudOAuthConnectionRecord) -> String {
    format!(
        "chatos.plugin.cloud-oauth.v1\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        connection.id,
        connection.owner_user_id,
        connection.plugin_id,
        connection.release_id,
        connection.component_key,
        connection.provider,
        connection.resource,
        connection.revision,
    )
}

pub(in crate::api) fn credential_snapshot_sha256(
    bundle: &PluginMcpCloudRuntimeBundle,
    credentials: &[StoredPluginCloudCredential],
    oauth: Option<&PluginCloudOAuthConnectionRecord>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"chatos.plugin.cloud-credential-snapshot.v1\n");
    hasher.update(bundle.bundle_sha256.as_bytes());
    let mut records = credentials
        .iter()
        .map(|record| {
            (
                record.metadata.secret_name.as_str(),
                record.metadata.id.as_str(),
                record.metadata.revision.as_str(),
            )
        })
        .collect::<Vec<_>>();
    records.sort();
    for (name, id, revision) in records {
        hasher.update(b"\ncredential:");
        hasher.update(name.as_bytes());
        hasher.update(b":");
        hasher.update(id.as_bytes());
        hasher.update(b":");
        hasher.update(revision.as_bytes());
    }
    if let Some(connection) = oauth {
        hasher.update(b"\noauth:");
        hasher.update(connection.id.as_bytes());
        hasher.update(b":");
        hasher.update(connection.revision.as_bytes());
    }
    hex::encode(hasher.finalize())
}

pub(in crate::api) async fn write_cloud_credential_audit(
    state: &AppState,
    event: &str,
    owner_user_id: &str,
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
) -> Result<(), ApiError> {
    let mut audit = plugin_audit_record(
        event,
        owner_user_id,
        None,
        plugin_id,
        Some(release_id),
        "success",
        BTreeMap::new(),
    );
    audit.component_key = Some(component_key.to_string());
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)
}
