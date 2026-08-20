// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::Duration;

use chatos_plugin_management_sdk::{
    PluginComponentKind, PluginComponentSnapshot, PluginExecutionHost,
    PluginMcpPortableRuntimeBundle, PluginPortableComponentBundle, PluginReleaseRecord,
};
use chatos_plugin_package::{
    build_plugin_mcp_portable_runtime_bundles_from_package, build_portable_component_bundles,
    verify_plugin_archive_bytes, PluginPackageLimits,
};
use futures_util::StreamExt;

use super::*;

const MAX_PLUGIN_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;

pub(super) async fn stage_release_portable_mcp_runtime_bundles(
    state: &AppState,
    release: &PluginReleaseRecord,
    expected_snapshots: Option<&[PluginComponentSnapshot]>,
) -> Result<Vec<PluginMcpPortableRuntimeBundle>, ApiError> {
    let components = release
        .components
        .iter()
        .filter(|component| {
            component.kind == PluginComponentKind::McpServer
                && component.execution_host == PluginExecutionHost::Portable
        })
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Ok(Vec::new());
    }
    let artifact = fetch_plugin_artifact(
        state,
        release.artifact_ref.as_str(),
        state.config.plugin_catalog_request_timeout,
    )
    .await?;
    let package = verify_plugin_archive_bytes(
        artifact.as_slice(),
        release,
        portable_plugin_package_limits(),
    )
    .map_err(|error| ApiError::conflict(format!("verify Plugin artifact failed: {error}")))?;
    let mut bundles = Vec::with_capacity(components.len());
    for component in components {
        let candidates = build_plugin_mcp_portable_runtime_bundles_from_package(
            release,
            component.component_key.as_str(),
            &package,
        )
        .map_err(|error| {
            ApiError::conflict(format!(
                "build Plugin MCP portable runtime Bundle failed: {error}"
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
                            "Catalog is missing portable MCP runtime snapshot {}/{}/{}",
                            release.plugin_id, release.id, component.component_key
                        ))
                    })?;
                candidates
                    .into_iter()
                    .find(|bundle| bundle.bundle_sha256 == snapshot.content_sha256)
                    .ok_or_else(|| {
                        ApiError::conflict(format!(
                            "Catalog portable MCP snapshot does not match any verified config runtime {}/{}/{}",
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

pub(super) async fn stage_release_portable_bundles(
    state: &AppState,
    release: &PluginReleaseRecord,
) -> Result<Vec<PluginPortableComponentBundle>, ApiError> {
    let has_portable_text_components = release.components.iter().any(|component| {
        component.execution_host == PluginExecutionHost::Portable
            && matches!(
                component.kind,
                PluginComponentKind::SkillCollection
                    | PluginComponentKind::Command
                    | PluginComponentKind::Agent
            )
    });
    if !has_portable_text_components {
        return Ok(Vec::new());
    }
    let artifact = fetch_plugin_artifact(
        state,
        release.artifact_ref.as_str(),
        state.config.plugin_catalog_request_timeout,
    )
    .await?;
    let package = verify_plugin_archive_bytes(
        artifact.as_slice(),
        release,
        portable_plugin_package_limits(),
    )
    .map_err(|error| ApiError::conflict(format!("verify Plugin artifact failed: {error}")))?;
    build_portable_component_bundles(release, &package, now_rfc3339().as_str()).map_err(|error| {
        ApiError::conflict(format!("build Plugin portable Bundle failed: {error}"))
    })
}

fn portable_plugin_package_limits() -> PluginPackageLimits {
    PluginPackageLimits {
        max_archive_bytes: MAX_PLUGIN_ARTIFACT_BYTES,
        max_entries: 512,
        max_file_bytes: 2 * 1024 * 1024,
        max_unpacked_bytes: 32 * 1024 * 1024,
        ..PluginPackageLimits::default()
    }
}

async fn fetch_plugin_artifact(
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
        .is_some_and(|length| length > MAX_PLUGIN_ARTIFACT_BYTES as u64)
    {
        return Err(ApiError::bad_gateway(
            "Plugin artifact exceeds the package size limit",
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            ApiError::bad_gateway(format!("read Plugin artifact failed: {error}"))
        })?;
        if body.len().saturating_add(chunk.len()) > MAX_PLUGIN_ARTIFACT_BYTES {
            return Err(ApiError::bad_gateway(
                "Plugin artifact exceeded the package size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    let _ = state;
    Ok(body)
}
