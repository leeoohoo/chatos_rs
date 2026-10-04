// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io;
use std::pin::Pin;
use std::time::Duration;

use axum::body::{Body, Bytes};
use axum::extract::{Path, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_LENGTH, CONTENT_TYPE, VARY};
use axum::http::{HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chatos_plugin_management_sdk::{
    verify_plugin_release_signature, PluginInstallSource, PluginReleaseVerificationContext,
    UpdateUserPluginPreferenceRequest, UpdateUserPluginPreferenceResponse,
    PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY,
};
use chatos_service_runtime::is_public_ip;
use futures::{stream, Stream, StreamExt};
use reqwest::redirect::Policy;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::OwnedSemaphorePermit;
use url::{Host, Url};

use crate::models::CurrentUser;
use crate::state::AppState;

use super::{ensure_device_active_lease, load_owned_device, ApiError};

const MAX_PLUGIN_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

pub(super) async fn list_plugin_install_sources(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> Result<Response, ApiError> {
    require_human_user(&user)?;
    let sources = state
        .plugin_management_client
        .list_plugin_install_sources_for_service(user.effective_owner_user_id())
        .await
        .map_err(plugin_management_error)?;
    for source in &sources.items {
        ensure_source_preference_identity(source, user.effective_owner_user_id())?;
    }
    let mut response = Json(sources).into_response();
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
        .headers_mut()
        .insert(VARY, HeaderValue::from_static("authorization"));
    Ok(response)
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
    verify_install_source_signature(&source)?;
    let url = validate_artifact_url(source.release.artifact_ref.as_str())?;
    let download_permit = tokio::time::timeout(
        Duration::from_secs(10),
        state.plugin_artifact_download_slots.clone().acquire_owned(),
    )
    .await
    .map_err(|_| {
        ApiError::service_unavailable(
            "Plugin artifact download capacity is busy; retry the request shortly",
        )
    })?
    .map_err(|_| ApiError::service_unavailable("Plugin artifact downloads are unavailable"))?;
    let upstream = if source.marketplace.source_kind == PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY {
        state
            .plugin_management_client
            .download_plugin_artifact_for_service(source.release.artifact_sha256.as_str())
            .await
            .map_err(plugin_management_error)?
    } else {
        let client = build_artifact_client(&url).await?;
        client
            .get(url)
            .header(
                reqwest::header::ACCEPT,
                "application/gzip, application/octet-stream",
            )
            .send()
            .await
            .map_err(|error| {
                ApiError::bad_gateway(format!("Plugin artifact request failed: {error}"))
            })?
    };
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
    let artifact_stream = stream_artifact_with_limit_and_digest(
        upstream,
        source.release.artifact_sha256.clone(),
        download_permit,
    );
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
        .body(Body::from_stream(artifact_stream))
        .map_err(|error| ApiError::internal(format!("build Plugin artifact proxy failed: {error}")))
}

struct ArtifactStreamState {
    upstream: Pin<Box<dyn Stream<Item = Result<Bytes, io::Error>> + Send>>,
    hasher: Sha256,
    bytes_read: u64,
    expected_sha256: String,
    _download_permit: OwnedSemaphorePermit,
}

fn stream_artifact_with_limit_and_digest(
    upstream: reqwest::Response,
    expected_sha256: String,
    download_permit: OwnedSemaphorePermit,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send {
    checked_artifact_stream(upstream.bytes_stream(), expected_sha256, download_permit)
}

fn checked_artifact_stream<S, E>(
    upstream: S,
    expected_sha256: String,
    download_permit: OwnedSemaphorePermit,
) -> impl Stream<Item = Result<Bytes, io::Error>> + Send
where
    S: Stream<Item = Result<Bytes, E>> + Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    let state = ArtifactStreamState {
        upstream: Box::pin(
            upstream.map(|item| item.map_err(|error| io::Error::other(error.to_string()))),
        ),
        hasher: Sha256::new(),
        bytes_read: 0,
        expected_sha256: expected_sha256.to_ascii_lowercase(),
        _download_permit: download_permit,
    };
    stream::try_unfold(state, |mut state| async move {
        match state.upstream.next().await {
            Some(Ok(chunk)) => {
                let chunk_length = u64::try_from(chunk.len()).unwrap_or(u64::MAX);
                state.bytes_read = state
                    .bytes_read
                    .checked_add(chunk_length)
                    .ok_or_else(|| io::Error::other("Plugin artifact download size overflowed"))?;
                if state.bytes_read > MAX_PLUGIN_ARTIFACT_BYTES {
                    return Err(io::Error::other(
                        "Plugin artifact exceeded the proxy download size limit",
                    ));
                }
                state.hasher.update(&chunk);
                Ok(Some((chunk, state)))
            }
            Some(Err(error)) => Err(io::Error::other(format!(
                "read Plugin artifact response failed: {error}"
            ))),
            None => {
                let actual_sha256 = hex::encode(state.hasher.finalize());
                if actual_sha256 != state.expected_sha256 {
                    return Err(io::Error::other(
                        "Plugin artifact content SHA-256 did not match the signed release",
                    ));
                }
                Ok(None)
            }
        }
    })
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

fn verify_install_source_signature(source: &PluginInstallSource) -> Result<(), ApiError> {
    let key = source
        .marketplace
        .trusted_signing_keys
        .iter()
        .find(|key| key.key_id == source.release.signature.key_id)
        .ok_or_else(|| {
            ApiError::service_unavailable(
                "Plugin Release signing key is not trusted by its Marketplace",
            )
        })?;
    verify_plugin_release_signature(
        PluginReleaseVerificationContext {
            plugin_id: source.catalog.id.as_str(),
            version: source.release.version.as_str(),
            marketplace_id: source.marketplace.id.as_str(),
            publisher_id: source.catalog.publisher.id.as_str(),
            artifact_sha256: source.release.artifact_sha256.as_str(),
        },
        &source.release.normalized_manifest,
        &source.release.signature,
        key,
    )
    .map_err(|error| {
        ApiError::service_unavailable(format!(
            "Plugin install source signature verification failed: {error}"
        ))
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::TryStreamExt;
    use std::net::IpAddr;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

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

    #[tokio::test]
    async fn artifact_stream_verifies_digest_and_holds_download_slot_until_eof() {
        let slots = Arc::new(Semaphore::new(1));
        let permit = slots
            .clone()
            .acquire_owned()
            .await
            .expect("download permit");
        let expected = hex::encode(Sha256::digest(b"plugin artifact"));
        let stream = checked_artifact_stream(
            stream::iter([
                Ok::<_, io::Error>(Bytes::from_static(b"plugin ")),
                Ok::<_, io::Error>(Bytes::from_static(b"artifact")),
            ]),
            expected,
            permit,
        );
        assert!(slots.clone().try_acquire_owned().is_err());
        let chunks = stream
            .try_collect::<Vec<_>>()
            .await
            .expect("verified stream");
        assert_eq!(chunks.concat(), b"plugin artifact");
        assert!(slots.try_acquire_owned().is_ok());
    }

    #[tokio::test]
    async fn artifact_stream_rejects_content_that_does_not_match_signed_digest() {
        let permit = Arc::new(Semaphore::new(1))
            .acquire_owned()
            .await
            .expect("download permit");
        let stream = checked_artifact_stream(
            stream::iter([Ok::<_, io::Error>(Bytes::from_static(b"tampered"))]),
            hex::encode(Sha256::digest(b"expected")),
            permit,
        );
        let error = stream
            .try_collect::<Vec<_>>()
            .await
            .expect_err("digest mismatch must fail the response body");
        assert!(error.to_string().contains("did not match"));
    }
}
