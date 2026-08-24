// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::{CONTENT_LENGTH, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode};
use axum::response::Response;
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    PluginInstallSource, PluginInstallSourceList, UpdateUserPluginPreferenceRequest,
    UpdateUserPluginPreferenceResponse,
};
use futures::StreamExt;
use reqwest::redirect::Policy;
use serde::Deserialize;
use url::{Host, Url};

use crate::models::CurrentUser;
use crate::state::AppState;

use super::{ensure_device_active_lease, load_owned_device, ApiError};

const MAX_PLUGIN_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

pub(super) async fn list_plugin_install_sources(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Json<PluginInstallSourceList>, ApiError> {
    require_human_user(&user)?;
    let sources = state
        .plugin_management_client
        .list_plugin_install_sources_for_service(user.effective_owner_user_id())
        .await
        .map_err(plugin_management_error)?;
    for source in &sources.items {
        ensure_source_preference_identity(source, user.effective_owner_user_id())?;
    }
    Ok(Json(sources))
}

#[derive(Debug, Deserialize)]
pub(super) struct UpdatePluginPreferenceRequest {
    device_id: String,
    enabled: bool,
    #[serde(default)]
    auto_update: Option<bool>,
    #[serde(default)]
    release_channel: Option<String>,
    #[serde(default)]
    enabled_components: Option<Vec<String>>,
}

pub(super) async fn update_plugin_preference(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(plugin_id): Path<String>,
    Json(request): Json<UpdatePluginPreferenceRequest>,
) -> Result<Json<UpdateUserPluginPreferenceResponse>, ApiError> {
    require_human_user(&user)?;
    load_owned_device(&state, &user, request.device_id.as_str(), true).await?;
    ensure_device_active_lease(
        &state,
        user.effective_owner_user_id(),
        request.device_id.as_str(),
    )
    .await?;
    state
        .plugin_management_client
        .update_user_plugin_preference_for_service(
            plugin_id.as_str(),
            &UpdateUserPluginPreferenceRequest {
                owner_user_id: user.effective_owner_user_id().to_string(),
                enabled: request.enabled,
                auto_update: request.auto_update,
                release_channel: request.release_channel,
                enabled_components: request.enabled_components,
            },
        )
        .await
        .map(Json)
        .map_err(plugin_management_error)
}

pub(super) async fn proxy_plugin_release_artifact(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path((plugin_id, release_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    require_human_user(&user)?;
    let source = state
        .plugin_management_client
        .get_plugin_install_source_for_service(
            plugin_id.as_str(),
            release_id.as_str(),
            user.effective_owner_user_id(),
        )
        .await
        .map_err(plugin_management_error)?;
    ensure_source_identity(&source, plugin_id.as_str(), release_id.as_str())?;
    ensure_source_preference_identity(&source, user.effective_owner_user_id())?;
    let url = validate_artifact_url(source.release.artifact_ref.as_str())?;
    let client = build_artifact_client(&url).await?;
    let upstream = client
        .get(url)
        .header(
            reqwest::header::ACCEPT,
            "application/gzip, application/octet-stream",
        )
        .send()
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("Plugin artifact request failed: {error}"))
        })?;
    if upstream.status() != reqwest::StatusCode::OK {
        return Err(ApiError::bad_gateway(format!(
            "Plugin artifact source returned status {}",
            upstream.status().as_u16()
        )));
    }
    let content_length = upstream.content_length();
    if content_length.is_some_and(|length| length > MAX_PLUGIN_ARTIFACT_BYTES) {
        return Err(ApiError::bad_gateway(
            "Plugin artifact exceeds the proxy download size limit",
        ));
    }

    let downloaded = Arc::new(AtomicU64::new(0));
    let stream_counter = downloaded.clone();
    let stream = upstream.bytes_stream().map(move |chunk| match chunk {
        Ok(chunk) => {
            let chunk_len = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
            let previous = stream_counter.fetch_add(chunk_len, Ordering::Relaxed);
            if previous.saturating_add(chunk_len) > MAX_PLUGIN_ARTIFACT_BYTES {
                Err(io::Error::other(
                    "Plugin artifact exceeded the proxy download size limit",
                ))
            } else {
                Ok(chunk)
            }
        }
        Err(error) => Err(io::Error::other(format!(
            "read Plugin artifact response failed: {error}"
        ))),
    });
    let mut response = Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/gzip")
        .header(
            "x-chatos-plugin-id",
            header_value(source.catalog.id.as_str())?,
        )
        .header(
            "x-chatos-plugin-release-id",
            header_value(source.release.id.as_str())?,
        )
        .header(
            "x-chatos-plugin-artifact-sha256",
            header_value(source.release.artifact_sha256.as_str())?,
        );
    if let Some(content_length) = content_length {
        response = response.header(CONTENT_LENGTH, content_length);
    }
    response
        .body(Body::from_stream(stream))
        .map_err(|error| ApiError::internal(format!("build Plugin artifact proxy failed: {error}")))
}

fn ensure_source_preference_identity(
    source: &PluginInstallSource,
    owner_user_id: &str,
) -> Result<(), ApiError> {
    if source.preference.as_ref().is_some_and(|preference| {
        preference.owner_user_id != owner_user_id || preference.plugin_id != source.catalog.id
    }) {
        return Err(ApiError::service_unavailable(
            "Plugin Management returned a mismatched user preference identity",
        ));
    }
    Ok(())
}

fn ensure_source_identity(
    source: &PluginInstallSource,
    plugin_id: &str,
    release_id: &str,
) -> Result<(), ApiError> {
    if source.catalog.id != plugin_id
        || source.release.id != release_id
        || source.release.plugin_id != plugin_id
        || source.catalog.latest_release_id != release_id
        || source.catalog.marketplace_id != source.marketplace.id
    {
        return Err(ApiError::service_unavailable(
            "Plugin Management returned a mismatched install source identity",
        ));
    }
    Ok(())
}

fn require_human_user(user: &CurrentUser) -> Result<(), ApiError> {
    if user.principal_type != "human_user" {
        return Err(ApiError::forbidden(
            "Plugin Marketplace downloads require a human user session",
        ));
    }
    Ok(())
}

fn plugin_management_error(
    error: chatos_plugin_management_sdk::PluginManagementClientError,
) -> ApiError {
    match error {
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 400,
            message,
        } => ApiError::bad_request(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 403,
            message,
        } => ApiError::forbidden(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 404,
            message,
        } => ApiError::not_found(message),
        chatos_plugin_management_sdk::PluginManagementClientError::Rejected {
            status: 409,
            message,
        } => ApiError::conflict("plugin_preference_rejected", message),
        other => ApiError::service_unavailable(other.to_string()),
    }
}

fn header_value(value: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(value).map_err(|_| {
        ApiError::service_unavailable("Plugin install source contains invalid identity metadata")
    })
}

fn validate_artifact_url(value: &str) -> Result<Url, ApiError> {
    let url = Url::parse(value)
        .map_err(|_| ApiError::service_unavailable("Plugin artifact URL is invalid"))?;
    let loopback_development_url = url.scheme() == "http" && is_loopback_artifact_url(&url);
    if (url.scheme() != "https" && !loopback_development_url)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::service_unavailable(
            "Plugin artifact URL must use HTTPS, except for HTTP loopback development URLs, and cannot contain credentials or fragments",
        ));
    }
    Ok(url)
}

fn is_loopback_artifact_url(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        None => false,
    }
}

async fn build_artifact_client(url: &Url) -> Result<reqwest::Client, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::service_unavailable("Plugin artifact URL has no host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::service_unavailable("Plugin artifact URL has no usable port"))?;
    let mut addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("resolve Plugin artifact host failed: {error}"))
        })?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    let loopback_development_url = url.scheme() == "http" && is_loopback_artifact_url(url);
    let addresses_are_allowed = if loopback_development_url {
        addresses.iter().all(|address| address.ip().is_loopback())
    } else {
        addresses.iter().all(|address| is_public_ip(address.ip()))
    };
    if addresses.is_empty() || !addresses_are_allowed {
        return Err(ApiError::service_unavailable(
            "Plugin artifact host resolved outside its allowed public or loopback network scope",
        ));
    }
    reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .https_only(!loopback_development_url)
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(5 * 60))
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|error| {
            ApiError::internal(format!("build Plugin artifact client failed: {error}"))
        })
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifact_url_allows_http_only_for_loopback_development() {
        assert!(validate_artifact_url("https://registry.npmjs.org/demo/-/demo-1.0.0.tgz").is_ok());
        assert!(validate_artifact_url("http://127.0.0.1:39260/api/plugin-artifacts/demo").is_ok());
        assert!(validate_artifact_url("http://localhost:39260/api/plugin-artifacts/demo").is_ok());
        assert!(validate_artifact_url("http://[::1]:39260/api/plugin-artifacts/demo").is_ok());
        assert!(validate_artifact_url("http://registry.npmjs.org/demo/-/demo-1.0.0.tgz").is_err());
    }

    #[test]
    fn artifact_url_rejects_embedded_credentials_and_fragments() {
        assert!(
            validate_artifact_url("https://user@registry.npmjs.org/demo/-/demo-1.0.0.tgz").is_err()
        );
        assert!(validate_artifact_url("http://user@127.0.0.1:39260/demo.tgz").is_err());
        assert!(validate_artifact_url("https://plugins.example.com/demo.zip#hash").is_err());
    }

    #[test]
    fn artifact_proxy_rejects_private_and_special_networks() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.1",
            "169.254.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
        ] {
            let ip: IpAddr = value.parse().expect("test IP");
            assert!(!is_public_ip(ip), "{value}");
        }
        assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
        assert!(is_public_ip(
            "2606:4700:4700::1111".parse().expect("public IPv6")
        ));
    }
}
