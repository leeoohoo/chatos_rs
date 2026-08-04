// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::time::Duration;

use axum::response::Response;
use chatos_plugin_management_sdk::{
    normalized_plugin_manifest_sha256, plugin_mcp_cloud_runtime_bundle_sha256,
    PluginCloudComponentBundle, PluginComponentKind, PluginComponentSnapshot, PluginExecutionHost,
    PluginMcpCloudRuntimeBundle, PluginMcpCloudRuntimeMetadata, PluginMcpServer,
    PluginReleaseRecord,
};
use chatos_plugin_package::{
    build_cloud_component_bundles, build_plugin_mcp_cloud_runtime_bundles_from_package,
    plugin_cloud_bundle_sha256, verify_plugin_archive_bytes, PluginPackageLimits,
};
use futures_util::StreamExt;

use super::*;

const MAX_CLOUD_PLUGIN_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

pub(super) async fn list_plugin_mcp_cloud_runtime_metadata(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, release_id)): Path<(String, String)>,
) -> Result<Json<ListResponse<PluginMcpCloudRuntimeMetadata>>, ApiError> {
    let plugin = state
        .store
        .get_plugin_catalog_entry(plugin_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin not found"))?;
    ensure_catalog_visible(&user, &plugin)?;
    let release = state
        .store
        .get_plugin_release(release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
    if release.plugin_id != plugin.id || release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "Plugin Release is revoked or does not match the Plugin",
        ));
    }
    let snapshots = state
        .store
        .list_plugin_component_snapshots(plugin_id.as_str(), release_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let bundles = state
        .store
        .list_plugin_mcp_cloud_runtime_bundles(plugin_id.as_str(), release_id.as_str())
        .await
        .map_err(ApiError::internal)?;
    let mut items = Vec::with_capacity(bundles.len());
    for bundle in bundles {
        let snapshot = snapshots
            .iter()
            .find(|snapshot| snapshot.component.component_key == bundle.component.component_key)
            .ok_or_else(|| ApiError::conflict("Plugin MCP component snapshot is unavailable"))?;
        if snapshot.component != bundle.component
            || snapshot.content_sha256 != bundle.bundle_sha256
            || plugin_mcp_cloud_runtime_bundle_sha256(&bundle).map_err(ApiError::internal)?
                != bundle.bundle_sha256
        {
            return Err(ApiError::conflict(
                "Plugin MCP cloud runtime Bundle failed immutable identity validation",
            ));
        }
        let (transport, oauth_resource) = match bundle.effective_runtime() {
            PluginMcpServer::Stdio { .. } => ("stdio", None),
            PluginMcpServer::Http { oauth_resource, .. } => ("http", oauth_resource.clone()),
            PluginMcpServer::ConfigFile { .. } => {
                return Err(ApiError::conflict(
                    "Plugin MCP resolved runtime cannot remain a config file",
                ))
            }
        };
        let secret_names =
            super::plugin_cloud_credentials::runtime_secret_names(bundle.effective_runtime())?
                .into_iter()
                .collect();
        items.push(PluginMcpCloudRuntimeMetadata {
            plugin_id: bundle.plugin_id,
            release_id: bundle.release_id,
            component_key: bundle.component.component_key,
            server_key: bundle.server_key,
            transport: transport.to_string(),
            secret_names,
            oauth_resource,
            bundle_sha256: bundle.bundle_sha256,
        });
    }
    items.sort_by(|left, right| left.component_key.cmp(&right.component_key));
    Ok(Json(ListResponse {
        total: items.len() as u64,
        items,
    }))
}

pub(super) async fn get_plugin_cloud_component_bundle_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((plugin_id, release_id, component_key)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let caller = require_internal_caller_service(&headers)?;
    if caller != "task-runner" && caller != "mcp-management-service" {
        return Err(ApiError::forbidden(
            "Plugin cloud Bundles require task-runner or mcp-management-service caller",
        ));
    }
    require_internal_api_secret(&state, &headers, caller, PLUGIN_CLOUD_READ_SCOPE)?;
    let release = state
        .store
        .get_plugin_release(release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
    if release.plugin_id != plugin_id || release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "Plugin Release is revoked or does not match the requested Plugin",
        ));
    }
    let bundle = state
        .store
        .get_plugin_cloud_component_bundle(
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
        )
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin cloud component Bundle not found"))?;
    if bundle.execution_host == PluginExecutionHost::Local
        || bundle.artifact_sha256 != release.artifact_sha256
        || plugin_cloud_bundle_sha256(&bundle)
            .map_err(|error| ApiError::internal(error.to_string()))?
            != bundle.bundle_sha256
    {
        return Err(ApiError::conflict(
            "Plugin cloud component Bundle failed immutable identity validation",
        ));
    }
    let snapshot = state
        .store
        .list_plugin_component_snapshots(plugin_id.as_str(), release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .find(|snapshot| snapshot.component.component_key == component_key)
        .ok_or_else(|| ApiError::conflict("Plugin component snapshot is missing"))?;
    if snapshot.content_sha256 != bundle.bundle_sha256
        || snapshot.component.execution_host != bundle.execution_host
    {
        return Err(ApiError::conflict(
            "Plugin Bundle does not match the immutable component snapshot",
        ));
    }
    let mut response = Json(bundle).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(format!("\"{}\"", snapshot.content_sha256).as_str())
            .map_err(|_| ApiError::internal("Plugin Bundle ETag is invalid"))?,
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

pub(super) async fn get_plugin_mcp_cloud_runtime_bundle_internal(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((plugin_id, release_id, component_key)): Path<(String, String, String)>,
) -> Result<Response, ApiError> {
    let caller = require_internal_caller_service(&headers)?;
    if caller != "mcp-management-service" {
        return Err(ApiError::forbidden(
            "Plugin MCP cloud runtime Bundles require mcp-management-service caller",
        ));
    }
    require_internal_api_secret(&state, &headers, caller, PLUGIN_CLOUD_READ_SCOPE)?;
    let release = state
        .store
        .get_plugin_release(release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Release not found"))?;
    if release.plugin_id != plugin_id || release.revoked_at.is_some() {
        return Err(ApiError::conflict(
            "Plugin Release is revoked or does not match the requested Plugin",
        ));
    }
    let bundle = state
        .store
        .get_plugin_mcp_cloud_runtime_bundle(
            plugin_id.as_str(),
            release_id.as_str(),
            component_key.as_str(),
        )
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin MCP cloud runtime Bundle not found"))?;
    if plugin_mcp_cloud_runtime_bundle_sha256(&bundle).map_err(ApiError::internal)?
        != bundle.bundle_sha256
    {
        return Err(ApiError::conflict(
            "Plugin MCP cloud runtime Bundle failed immutable identity validation",
        ));
    }
    let snapshot = state
        .store
        .list_plugin_component_snapshots(plugin_id.as_str(), release_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .into_iter()
        .find(|snapshot| snapshot.component.component_key == component_key)
        .ok_or_else(|| ApiError::conflict("Plugin component snapshot is missing"))?;
    if snapshot.component != bundle.component || snapshot.content_sha256 != bundle.bundle_sha256 {
        return Err(ApiError::conflict(
            "Plugin MCP cloud runtime Bundle does not match the immutable component snapshot",
        ));
    }
    let mut response = Json(bundle).into_response();
    response.headers_mut().insert(
        header::ETAG,
        HeaderValue::from_str(format!("\"{}\"", snapshot.content_sha256).as_str())
            .map_err(|_| ApiError::internal("Plugin MCP runtime Bundle ETag is invalid"))?,
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

pub(super) async fn stage_release_cloud_mcp_runtime_bundles(
    state: &AppState,
    release: &PluginReleaseRecord,
    expected_snapshots: Option<&[PluginComponentSnapshot]>,
) -> Result<Vec<PluginMcpCloudRuntimeBundle>, ApiError> {
    let components = release
        .components
        .iter()
        .filter(|component| {
            component.kind == PluginComponentKind::McpServer
                && component.execution_host != PluginExecutionHost::Local
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Ok(Vec::new());
    }
    let existing = state
        .store
        .list_plugin_mcp_cloud_runtime_bundles(release.plugin_id.as_str(), release.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    if existing.len() == components.len()
        && existing.iter().all(|bundle| {
            bundle.plugin_id == release.plugin_id
                && bundle.release_id == release.id
                && bundle.version == release.version
                && bundle.artifact_sha256 == release.artifact_sha256
                && bundle.component.execution_host != PluginExecutionHost::Local
                && plugin_mcp_cloud_runtime_bundle_sha256(bundle)
                    .is_ok_and(|sha256| sha256 == bundle.bundle_sha256)
                && expected_snapshots.is_none_or(|snapshots| {
                    snapshots.iter().any(|snapshot| {
                        snapshot.plugin_id == bundle.plugin_id
                            && snapshot.release_id == bundle.release_id
                            && snapshot.component == bundle.component
                            && snapshot.content_sha256 == bundle.bundle_sha256
                    })
                })
        })
    {
        return Ok(existing);
    }
    if !existing.is_empty() {
        return Err(ApiError::conflict(
            "stored Plugin MCP cloud runtime Bundles conflict with the immutable Release",
        ));
    }
    let artifact = fetch_cloud_artifact(
        state,
        release.artifact_ref.as_str(),
        state.config.plugin_catalog_request_timeout,
    )
    .await?;
    let package =
        verify_plugin_archive_bytes(artifact.as_slice(), release, cloud_plugin_package_limits())
            .map_err(|error| {
                ApiError::conflict(format!("verify Plugin artifact failed: {error}"))
            })?;
    let mut bundles = Vec::with_capacity(components.len());
    for component in components {
        let candidates = build_plugin_mcp_cloud_runtime_bundles_from_package(
            release,
            component.component_key.as_str(),
            &package,
        )
        .map_err(|error| {
            ApiError::conflict(format!(
                "build Plugin MCP cloud runtime Bundle failed: {error}"
            ))
        })?;
        let selected = match expected_snapshots {
            Some(snapshots) => {
                let snapshot = snapshots
                    .iter()
                    .find(|snapshot| {
                        snapshot.plugin_id == release.plugin_id
                            && snapshot.release_id == release.id
                            && snapshot.component == *component
                    })
                    .ok_or_else(|| {
                        ApiError::conflict(format!(
                            "Catalog is missing cloud MCP runtime snapshot {}/{}/{}",
                            release.plugin_id, release.id, component.component_key
                        ))
                    })?;
                candidates
                    .into_iter()
                    .find(|bundle| bundle.bundle_sha256 == snapshot.content_sha256)
                    .ok_or_else(|| {
                        ApiError::conflict(format!(
                            "Catalog cloud MCP snapshot does not match any verified config runtime {}/{}/{}",
                            release.plugin_id, release.id, component.component_key
                        ))
                    })?
            }
            None if candidates.len() == 1 => candidates.into_iter().next().expect("one candidate"),
            None => {
                return Err(ApiError::conflict(format!(
                    "Plugin MCP config contains multiple servers and requires a signed server selection: {}",
                    component.component_key
                )))
            }
        };
        bundles.push(selected);
    }
    bundles.sort_by(|left, right| {
        left.component
            .component_key
            .cmp(&right.component.component_key)
    });
    Ok(bundles)
}

pub(super) async fn stage_release_cloud_bundles(
    state: &AppState,
    release: &PluginReleaseRecord,
) -> Result<Vec<PluginCloudComponentBundle>, ApiError> {
    let expected = release
        .components
        .iter()
        .filter(|component| {
            component.execution_host != PluginExecutionHost::Local
                && matches!(
                    component.kind,
                    PluginComponentKind::SkillCollection
                        | PluginComponentKind::Command
                        | PluginComponentKind::Agent
                )
        })
        .count();
    if expected == 0 {
        return Ok(Vec::new());
    }
    let existing = state
        .store
        .list_plugin_cloud_component_bundles(release.plugin_id.as_str(), release.id.as_str())
        .await
        .map_err(ApiError::internal)?;
    if existing.len() == expected {
        let manifest_sha256 = normalized_plugin_manifest_sha256(&release.normalized_manifest)
            .map_err(|error| ApiError::internal(error.to_string()))?;
        let mut component_keys = BTreeSet::new();
        let valid = existing.iter().all(|bundle| {
            let component = release.components.iter().find(|component| {
                component.component_key == bundle.component_key
                    && component.execution_host != PluginExecutionHost::Local
            });
            component.is_some_and(|component| {
                component_keys.insert(bundle.component_key.as_str())
                    && bundle.plugin_id == release.plugin_id
                    && bundle.release_id == release.id
                    && bundle.version == release.version
                    && bundle.kind == component.kind
                    && bundle.execution_host == component.execution_host
                    && bundle.artifact_sha256 == release.artifact_sha256
                    && bundle.normalized_manifest_sha256 == manifest_sha256
                    && plugin_cloud_bundle_sha256(bundle)
                        .is_ok_and(|sha256| sha256 == bundle.bundle_sha256)
            })
        });
        if valid {
            return Ok(existing);
        }
        return Err(ApiError::conflict(
            "stored Plugin cloud Bundles conflict with the immutable Release",
        ));
    }
    let artifact = fetch_cloud_artifact(
        state,
        release.artifact_ref.as_str(),
        state.config.plugin_catalog_request_timeout,
    )
    .await?;
    let limits = cloud_plugin_package_limits();
    let package = verify_plugin_archive_bytes(artifact.as_slice(), release, limits)
        .map_err(|error| ApiError::conflict(format!("verify Plugin artifact failed: {error}")))?;
    build_cloud_component_bundles(release, &package, now_rfc3339().as_str())
        .map_err(|error| ApiError::conflict(format!("build Plugin cloud Bundle failed: {error}")))
}

fn cloud_plugin_package_limits() -> PluginPackageLimits {
    PluginPackageLimits {
        max_archive_bytes: MAX_CLOUD_PLUGIN_ARTIFACT_BYTES,
        max_entries: 512,
        max_file_bytes: 2 * 1024 * 1024,
        max_unpacked_bytes: 32 * 1024 * 1024,
        ..PluginPackageLimits::default()
    }
}

async fn fetch_cloud_artifact(
    state: &AppState,
    value: &str,
    timeout: Duration,
) -> Result<Vec<u8>, ApiError> {
    let url = super::plugin_catalog_sync::validate_catalog_url(value)?;
    let client = super::plugin_catalog_sync::build_catalog_client(&url, timeout).await?;
    let response = client
        .get(url)
        .header(
            reqwest::header::ACCEPT,
            "application/zip, application/octet-stream",
        )
        .send()
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("Plugin artifact request failed: {error}"))
        })?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(ApiError::bad_gateway(format!(
            "Plugin artifact source returned status {}",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_CLOUD_PLUGIN_ARTIFACT_BYTES as u64)
    {
        return Err(ApiError::bad_gateway(
            "Plugin artifact exceeds the cloud package size limit",
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            ApiError::bad_gateway(format!("read Plugin artifact failed: {error}"))
        })?;
        if body.len().saturating_add(chunk.len()) > MAX_CLOUD_PLUGIN_ARTIFACT_BYTES {
            return Err(ApiError::bad_gateway(
                "Plugin artifact exceeded the cloud package size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let _ = state;
    Ok(body)
}
