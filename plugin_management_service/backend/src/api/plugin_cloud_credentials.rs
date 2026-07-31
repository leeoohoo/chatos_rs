// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chatos_plugin_management_sdk::{
    build_plugin_mcp_cloud_runtime_bundle, PluginMcpCloudRuntimeBundle, PluginMcpServer,
};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::*;

const MAX_SECRET_BYTES: usize = 64 * 1024;
const MAX_TEMPLATE_BYTES: usize = 8 * 1024;
const CREDENTIAL_PLACEHOLDER_PREFIX: &str = "${credential:";

pub(super) async fn list_plugin_cloud_credentials(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
    Query(query): Query<PluginCloudCredentialQuery>,
) -> Result<Json<ListResponse<PluginCloudCredentialMetadata>>, ApiError> {
    let release_id = required_text(Some(query.release_id.as_str()), "release_id")?;
    let component_key = validate_scope_segment(query.component_key.as_str(), "component_key")?;
    require_visible_cloud_mcp_release(
        &state,
        &user,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    let items = state
        .store
        .list_plugin_cloud_credentials(
            user.effective_owner_user_id(),
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
        )
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .map(|record| record.metadata)
        .collect::<Vec<_>>();
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn upsert_plugin_cloud_credential(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, release_id, component_key, secret_name)): Path<(
        String,
        String,
        String,
        String,
    )>,
    Json(mut payload): Json<UpsertPluginCloudCredentialPayload>,
) -> Result<Json<PluginCloudCredentialMetadata>, ApiError> {
    let value = Zeroizing::new(std::mem::take(&mut payload.value));
    let component_key = validate_scope_segment(component_key.as_str(), "component_key")?;
    let secret_name = validate_scope_segment(secret_name.as_str(), "secret_name")?;
    validate_secret_value(value.as_str())?;
    let (release, bundle) = require_visible_cloud_mcp_release(
        &state,
        &user,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    let referenced = runtime_secret_names(&bundle.runtime)?;
    if !referenced.contains(secret_name.as_str()) {
        return Err(ApiError::bad_request(
            "Plugin cloud credential is not referenced by the immutable MCP runtime",
        ));
    }
    let permissions = permissions_for_release(&release, component_key.as_str());
    require_credential_permission(&bundle, permissions.as_slice())?;
    let owner_user_id = user.effective_owner_user_id();
    let existing = state
        .store
        .get_plugin_cloud_credential(
            owner_user_id,
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
            secret_name.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    let now = now_rfc3339();
    let metadata = PluginCloudCredentialMetadata {
        id: existing
            .as_ref()
            .map(|record| record.metadata.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_user_id: owner_user_id.to_string(),
        plugin_id: plugin_id.clone(),
        release_id: release_id.clone(),
        component_key: component_key.clone(),
        secret_name: secret_name.clone(),
        revision: Uuid::new_v4().to_string(),
        created_at: existing
            .as_ref()
            .map(|record| record.metadata.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    let encrypted_value = state
        .cloud_secret_cipher
        .encrypt(value.as_str(), credential_aad(&metadata).as_str())
        .map_err(ApiError::internal)?;
    state
        .store
        .replace_plugin_cloud_credential(&StoredPluginCloudCredential {
            metadata: metadata.clone(),
            encrypted_value,
        })
        .await
        .map_err(ApiError::internal)?;
    write_cloud_credential_audit(
        &state,
        PLUGIN_AUDIT_UPSERT_CLOUD_CREDENTIAL,
        owner_user_id,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    Ok(Json(metadata))
}

pub(super) async fn delete_plugin_cloud_credential(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, release_id, component_key, secret_name)): Path<(
        String,
        String,
        String,
        String,
    )>,
) -> Result<StatusCode, ApiError> {
    let component_key = validate_scope_segment(component_key.as_str(), "component_key")?;
    let secret_name = validate_scope_segment(secret_name.as_str(), "secret_name")?;
    require_visible_cloud_mcp_release(
        &state,
        &user,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    let deleted = state
        .store
        .delete_plugin_cloud_credential(
            user.effective_owner_user_id(),
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
            secret_name.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    if !deleted {
        return Err(ApiError::not_found("Plugin cloud credential not found"));
    }
    write_cloud_credential_audit(
        &state,
        PLUGIN_AUDIT_DELETE_CLOUD_CREDENTIAL,
        user.effective_owner_user_id(),
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn list_plugin_cloud_oauth_connections(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
    Query(query): Query<PluginCloudCredentialQuery>,
) -> Result<Json<ListResponse<PluginCloudOAuthConnectionRecord>>, ApiError> {
    let release_id = required_text(Some(query.release_id.as_str()), "release_id")?;
    let component_key = validate_scope_segment(query.component_key.as_str(), "component_key")?;
    require_visible_cloud_mcp_release(
        &state,
        &user,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    let items = state
        .store
        .list_plugin_cloud_oauth_connections(
            user.effective_owner_user_id(),
            plugin_id.as_str(),
            release_id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .filter(|record| record.connection.component_key == component_key)
        .map(|record| record.connection)
        .collect::<Vec<_>>();
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn upsert_plugin_cloud_oauth_connection(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, release_id, component_key)): Path<(String, String, String)>,
    Json(mut payload): Json<UpsertPluginCloudOAuthPayload>,
) -> Result<Json<PluginCloudOAuthConnectionRecord>, ApiError> {
    let access_token = Zeroizing::new(std::mem::take(&mut payload.access_token));
    let component_key = validate_scope_segment(component_key.as_str(), "component_key")?;
    let provider = validate_identifier(payload.provider.as_str(), "provider", 96)?;
    let resource = validate_text(payload.resource.as_str(), "resource", 2_048)?;
    let scopes = normalize_scopes(std::mem::take(&mut payload.scopes))?;
    validate_access_token(access_token.as_str())?;
    let expires_at = validate_future_expiry(payload.expires_at.as_deref())?;
    let account_display = payload
        .account_display
        .as_deref()
        .map(|value| validate_text(value, "account_display", 200))
        .transpose()?;
    let (release, bundle) = require_visible_cloud_mcp_release(
        &state,
        &user,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    let PluginMcpServer::Http {
        oauth_resource: Some(expected_resource),
        headers,
        ..
    } = &bundle.runtime
    else {
        return Err(ApiError::bad_request(
            "Plugin cloud OAuth requires an HTTP MCP runtime with oauth_resource",
        ));
    };
    if expected_resource != &resource || contains_authorization_header(headers) {
        return Err(ApiError::bad_request(
            "Plugin cloud OAuth resource does not match the immutable MCP runtime",
        ));
    }
    let permissions = permissions_for_release(&release, component_key.as_str());
    require_oauth_permissions(provider.as_str(), scopes.as_slice(), permissions.as_slice())?;
    let owner_user_id = user.effective_owner_user_id();
    let existing = state
        .store
        .get_plugin_cloud_oauth_connection(
            owner_user_id,
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
            provider.as_str(),
            resource.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    let connection = PluginCloudOAuthConnectionRecord {
        id: existing
            .as_ref()
            .map(|record| record.connection.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        owner_user_id: owner_user_id.to_string(),
        plugin_id: plugin_id.clone(),
        release_id: release_id.clone(),
        component_key: component_key.clone(),
        provider,
        resource,
        scopes,
        connected: true,
        needs_auth: false,
        expires_at,
        account_display,
        revision: Uuid::new_v4().to_string(),
        updated_at: now_rfc3339(),
    };
    let encrypted_access_token = state
        .cloud_secret_cipher
        .encrypt(access_token.as_str(), oauth_aad(&connection).as_str())
        .map_err(ApiError::internal)?;
    state
        .store
        .replace_plugin_cloud_oauth_connection(&StoredPluginCloudOAuthConnection {
            connection: connection.clone(),
            encrypted_access_token,
        })
        .await
        .map_err(ApiError::internal)?;
    write_cloud_credential_audit(
        &state,
        PLUGIN_AUDIT_UPSERT_CLOUD_OAUTH,
        owner_user_id,
        plugin_id.as_str(),
        release_id.as_str(),
        component_key.as_str(),
    )
    .await?;
    Ok(Json(connection))
}

pub(super) async fn delete_plugin_cloud_oauth_connection(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, connection_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    let connection_id = required_text(Some(connection_id.as_str()), "connection_id")?;
    let deleted = state
        .store
        .delete_plugin_cloud_oauth_connection(
            user.effective_owner_user_id(),
            plugin_id.as_str(),
            connection_id.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    if !deleted {
        return Err(ApiError::not_found(
            "Plugin cloud OAuth connection not found",
        ));
    }
    let mut audit = plugin_audit_record(
        PLUGIN_AUDIT_DELETE_CLOUD_OAUTH,
        user.effective_owner_user_id(),
        None,
        plugin_id.as_str(),
        None,
        "success",
        BTreeMap::new(),
    );
    audit.component_key = None;
    state
        .store
        .insert_plugin_audit(&audit)
        .await
        .map_err(ApiError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub(super) async fn resolve_plugin_mcp_cloud_credentials_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((plugin_id, release_id, component_key)): Path<(String, String, String)>,
    Json(mut request): Json<ResolvePluginMcpCloudCredentialsRequest>,
) -> Result<Response, ApiError> {
    let caller = require_internal_caller_service(&headers)?;
    if caller != "mcp-management-service" {
        return Err(ApiError::forbidden(
            "Plugin cloud credentials require mcp-management-service caller",
        ));
    }
    require_internal_api_secret(
        &state,
        &headers,
        caller,
        PLUGIN_CLOUD_CREDENTIALS_RESOLVE_SCOPE,
    )?;
    request.owner_user_id = required_text(Some(request.owner_user_id.as_str()), "owner_user_id")?;
    request.permission_snapshot = normalize_string_list(request.permission_snapshot);
    request.auth_connection_ids = normalize_string_list(request.auth_connection_ids);
    let release = state
        .store
        .get_plugin_release(release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
    if release.plugin_id != plugin_id || release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "Plugin Release is revoked or does not match the credential request",
        ));
    }
    let bundle = build_plugin_mcp_cloud_runtime_bundle(&release, component_key.as_str())
        .map_err(ApiError::conflict)?;
    if bundle.bundle_sha256 != request.expected_component_content_sha256 {
        return Err(ApiError::conflict(
            "Plugin cloud credential request does not match the immutable component snapshot",
        ));
    }
    let expected_permissions = permissions_for_release(&release, component_key.as_str());
    if expected_permissions != request.permission_snapshot {
        return Err(ApiError::conflict(
            "Plugin cloud credential request permission snapshot drifted from the signed Release",
        ));
    }
    let records = state
        .store
        .list_plugin_cloud_credentials(
            request.owner_user_id.as_str(),
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
        )
        .await
        .map_err(ApiError::internal)?;
    let by_name = records
        .iter()
        .map(|record| (record.metadata.secret_name.as_str(), record))
        .collect::<HashMap<_, _>>();
    let mut resolved_secrets = HashMap::new();
    for name in runtime_secret_names(&bundle.runtime)? {
        let record = by_name.get(name.as_str()).ok_or_else(|| {
            ApiError::conflict(format!("Plugin cloud credential is missing: {name}"))
        })?;
        let value = state
            .cloud_secret_cipher
            .decrypt(
                record.encrypted_value.as_str(),
                credential_aad(&record.metadata).as_str(),
            )
            .map_err(ApiError::internal)?;
        resolved_secrets.insert(name, value);
    }
    let mut oauth_record = None;
    let (headers, environment) = match &bundle.runtime {
        PluginMcpServer::Stdio { env, .. } => {
            if !resolved_secrets.is_empty() {
                require_credential_permission(&bundle, expected_permissions.as_slice())?;
            }
            let mut resolved = BTreeMap::new();
            for (name, template) in env {
                let parsed = parse_template(template)?;
                let secret_name = parsed.secret_name.ok_or_else(|| {
                    ApiError::conflict(
                        "Plugin stdio environment must use exact credential templates",
                    )
                })?;
                if !parsed.prefix.is_empty() || !parsed.suffix.is_empty() {
                    return Err(ApiError::conflict(
                        "Plugin stdio environment must use exact credential templates",
                    ));
                }
                let value = resolved_secrets
                    .get(secret_name.as_str())
                    .ok_or_else(|| ApiError::conflict("resolved Plugin credential is missing"))?;
                if value.contains('\0') {
                    return Err(ApiError::conflict(
                        "resolved Plugin stdio credential contains NUL",
                    ));
                }
                resolved.insert(name.clone(), value.as_str().to_string());
            }
            (BTreeMap::new(), resolved)
        }
        PluginMcpServer::Http {
            headers,
            oauth_resource,
            ..
        } => {
            if !resolved_secrets.is_empty() {
                require_credential_permission(&bundle, expected_permissions.as_slice())?;
            }
            let mut resolved = BTreeMap::new();
            for (name, template) in headers {
                let normalized_name = normalize_header_name(name)?;
                let parsed = parse_template(template)?;
                let value = match parsed.secret_name {
                    Some(secret_name) => {
                        let secret =
                            resolved_secrets.get(secret_name.as_str()).ok_or_else(|| {
                                ApiError::conflict("resolved Plugin credential is missing")
                            })?;
                        format!("{}{}{}", parsed.prefix, secret.as_str(), parsed.suffix)
                    }
                    None => parsed.prefix,
                };
                reqwest::header::HeaderValue::from_str(value.as_str()).map_err(|_| {
                    ApiError::conflict(
                        "resolved Plugin HTTP credential is not a valid header value",
                    )
                })?;
                resolved.insert(normalized_name, value);
            }
            if let Some(resource) = oauth_resource.as_deref() {
                if resolved.contains_key("authorization") {
                    return Err(ApiError::conflict(
                        "Plugin HTTP MCP cannot combine OAuth and Authorization templates",
                    ));
                }
                let allowed_ids = request
                    .auth_connection_ids
                    .iter()
                    .map(String::as_str)
                    .collect::<HashSet<_>>();
                let candidates = state
                    .store
                    .list_plugin_cloud_oauth_connections(
                        request.owner_user_id.as_str(),
                        plugin_id.as_str(),
                        release_id.as_str(),
                    )
                    .await
                    .map_err(ApiError::internal)?
                    .into_iter()
                    .filter(|record| {
                        record.connection.connected
                            && !record.connection.needs_auth
                            && record.connection.component_key == component_key
                            && record.connection.resource == resource
                            && allowed_ids.contains(record.connection.id.as_str())
                    })
                    .collect::<Vec<_>>();
                let [record] = candidates.as_slice() else {
                    return Err(ApiError::conflict(
                        "Plugin OAuth resource requires exactly one authorized cloud connection",
                    ));
                };
                validate_connection_expiry(&record.connection)?;
                require_oauth_permissions(
                    record.connection.provider.as_str(),
                    record.connection.scopes.as_slice(),
                    expected_permissions.as_slice(),
                )?;
                let token = state
                    .cloud_secret_cipher
                    .decrypt(
                        record.encrypted_access_token.as_str(),
                        oauth_aad(&record.connection).as_str(),
                    )
                    .map_err(ApiError::internal)?;
                reqwest::header::HeaderValue::from_str(
                    format!("Bearer {}", token.as_str()).as_str(),
                )
                .map_err(|_| ApiError::conflict("Plugin OAuth access token is invalid"))?;
                resolved.insert(
                    "authorization".to_string(),
                    format!("Bearer {}", token.as_str()),
                );
                oauth_record = Some(record.connection.clone());
            }
            (resolved, BTreeMap::new())
        }
        PluginMcpServer::ConfigFile { .. } => {
            return Err(ApiError::conflict(
                "Plugin config-file MCP credentials are not supported",
            ));
        }
    };
    let snapshot_sha256 =
        credential_snapshot_sha256(&bundle, records.as_slice(), oauth_record.as_ref());
    let mut response = Json(ResolvedPluginMcpCloudCredentials {
        credential_snapshot_sha256: snapshot_sha256,
        headers,
        environment,
        oauth_connection_id: oauth_record.map(|record| record.id),
    })
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
        .headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    Ok(response)
}

async fn require_visible_cloud_mcp_release(
    state: &AppState,
    user: &CurrentUser,
    plugin_id: &str,
    release_id: &str,
    component_key: &str,
) -> Result<(PluginReleaseRecord, PluginMcpCloudRuntimeBundle), ApiError> {
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    ensure_catalog_visible(user, &plugin)?;
    let release = state
        .store
        .get_plugin_release(release_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
    if release.plugin_id != plugin.id || release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "Plugin Release is revoked or does not match the Plugin",
        ));
    }
    let bundle = build_plugin_mcp_cloud_runtime_bundle(&release, component_key)
        .map_err(ApiError::conflict)?;
    Ok((release, bundle))
}

fn permissions_for_release(release: &PluginReleaseRecord, component_key: &str) -> Vec<String> {
    let mut permissions = release
        .permissions
        .iter()
        .filter(|permission| {
            permission.components.is_empty()
                || permission
                    .components
                    .iter()
                    .any(|key| key.trim() == component_key)
        })
        .map(|permission| permission.permission.trim().to_string())
        .chain(
            release
                .components
                .iter()
                .filter(|component| component.component_key == component_key)
                .flat_map(|component| component.permissions.iter())
                .map(|permission| permission.permission.trim().to_string()),
        )
        .filter(|permission| !permission.is_empty())
        .collect::<Vec<_>>();
    permissions.sort();
    permissions.dedup();
    permissions
}

fn runtime_secret_names(runtime: &PluginMcpServer) -> Result<BTreeSet<String>, ApiError> {
    let values = match runtime {
        PluginMcpServer::Stdio { env, .. } => env.values().collect::<Vec<_>>(),
        PluginMcpServer::Http { headers, .. } => headers.values().collect::<Vec<_>>(),
        PluginMcpServer::ConfigFile { .. } => Vec::new(),
    };
    let mut names = BTreeSet::new();
    for value in values {
        if let Some(name) = parse_template(value)?.secret_name {
            names.insert(name);
        }
    }
    Ok(names)
}

struct ParsedTemplate {
    prefix: String,
    secret_name: Option<String>,
    suffix: String,
}

fn parse_template(value: &str) -> Result<ParsedTemplate, ApiError> {
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

fn require_credential_permission(
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

fn require_oauth_permissions(
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

fn normalize_scopes(values: Vec<String>) -> Result<Vec<String>, ApiError> {
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

fn validate_scope_segment(value: &str, field: &str) -> Result<String, ApiError> {
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

fn validate_identifier(value: &str, field: &str, max_bytes: usize) -> Result<String, ApiError> {
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

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request(format!("{field} is invalid")));
    }
    Ok(value.to_string())
}

fn validate_secret_value(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > MAX_SECRET_BYTES || value.contains('\0') {
        return Err(ApiError::bad_request(
            "Plugin cloud credential is empty, oversized, or contains NUL",
        ));
    }
    Ok(())
}

fn validate_access_token(value: &str) -> Result<(), ApiError> {
    if value.is_empty() || value.len() > MAX_SECRET_BYTES || value.chars().any(char::is_control) {
        return Err(ApiError::bad_request("OAuth access token is invalid"));
    }
    Ok(())
}

fn validate_future_expiry(value: Option<&str>) -> Result<Option<String>, ApiError> {
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

fn validate_connection_expiry(
    connection: &PluginCloudOAuthConnectionRecord,
) -> Result<(), ApiError> {
    if let Some(expires_at) = connection.expires_at.as_deref() {
        let expires_at = DateTime::parse_from_rfc3339(expires_at)
            .map_err(|_| ApiError::conflict("Plugin OAuth expiry is invalid"))?;
        if expires_at.timestamp() <= Utc::now().timestamp() {
            return Err(ApiError::conflict(
                "Plugin OAuth access token is expired and requires reauthorization",
            ));
        }
    }
    Ok(())
}

fn normalize_header_name(value: &str) -> Result<String, ApiError> {
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

fn contains_authorization_header(headers: &BTreeMap<String, String>) -> bool {
    headers
        .keys()
        .any(|name| name.trim().eq_ignore_ascii_case("authorization"))
}

fn credential_aad(metadata: &PluginCloudCredentialMetadata) -> String {
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

fn oauth_aad(connection: &PluginCloudOAuthConnectionRecord) -> String {
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

fn credential_snapshot_sha256(
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

async fn write_cloud_credential_audit(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parser_never_accepts_multiple_secret_placeholders() {
        let parsed = parse_template("Bearer ${credential:access_token}").unwrap();
        assert_eq!(parsed.secret_name.as_deref(), Some("access_token"));
        assert_eq!(parsed.prefix, "Bearer ");
        assert!(parse_template("${credential:first}-${credential:second}").is_err());
    }

    #[test]
    fn oauth_permissions_are_provider_and_scope_exact() {
        assert!(require_oauth_permissions(
            "figma",
            &["files:read".to_string()],
            &["oauth.scope:figma:files:read".to_string()],
        )
        .is_ok());
        assert!(require_oauth_permissions(
            "figma",
            &["files:write".to_string()],
            &["oauth.scope:figma:files:read".to_string()],
        )
        .is_err());
    }
}
