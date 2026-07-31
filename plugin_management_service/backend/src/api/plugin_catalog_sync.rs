// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use chatos_plugin_management_sdk::{
    normalized_plugin_catalog_sha256, verify_plugin_catalog_document, verify_plugin_catalog_update,
    PluginCatalogDocument, PluginMcpCloudRuntimeBundle, PluginReleaseRecord, SigningKeyRef,
    PLUGIN_SIGNING_KEY_USAGE_CATALOG,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use futures_util::StreamExt;
use reqwest::redirect::Policy;
use reqwest::Url;
use semver::Version;

use super::plugin_marketplaces::validate_marketplace_signing_key_progression;
use super::*;

const SYSTEM_CATALOG_SYNC_ACTOR: &str = "system:plugin-catalog-sync";
const MAX_CATALOG_PLUGINS: usize = 2_000;
const MAX_CATALOG_RELEASES: usize = 10_000;
const MAX_CATALOG_COMPONENT_SNAPSHOTS: usize = 50_000;
const MAX_CATALOG_SIGNING_KEYS: usize = 2_000;

static ACTIVE_CATALOG_SYNCS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(super) async fn sync_admin_plugin_marketplace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(marketplace_id): Path<String>,
) -> Result<Json<PluginCatalogSyncResponse>, ApiError> {
    ensure_super_admin(&user)?;
    sync_plugin_marketplace(State(state), Extension(user), Path(marketplace_id)).await
}

pub(super) async fn sync_plugin_marketplace(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(marketplace_id): Path<String>,
) -> Result<Json<PluginCatalogSyncResponse>, ApiError> {
    let marketplace = state
        .store
        .get_plugin_marketplace(marketplace_id.as_str())
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Marketplace not found"))?;
    ensure_marketplace_visible(&user, &marketplace)?;
    ensure_marketplace_writable(&user, &marketplace)?;
    sync_plugin_marketplace_by_id(
        &state,
        marketplace_id.as_str(),
        user.effective_owner_user_id(),
    )
    .await
    .map(Json)
}

pub fn start_plugin_catalog_sync_loop(state: AppState) {
    if !state.config.plugin_catalog_sync_enabled {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(state.config.plugin_catalog_sync_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let marketplaces = match state.store.list_plugin_marketplaces().await {
                Ok(items) => items,
                Err(error) => {
                    tracing::warn!(
                        error = error.as_str(),
                        "list Plugin Marketplaces for Catalog sync failed"
                    );
                    continue;
                }
            };
            for marketplace in marketplaces
                .into_iter()
                .filter(is_syncable_network_marketplace)
            {
                if let Err(error) = sync_plugin_marketplace_by_id(
                    &state,
                    marketplace.id.as_str(),
                    SYSTEM_CATALOG_SYNC_ACTOR,
                )
                .await
                {
                    tracing::warn!(
                        marketplace_id = marketplace.id.as_str(),
                        error = error.message.as_str(),
                        "scheduled Plugin Catalog sync failed"
                    );
                }
            }
        }
    });
}

async fn sync_plugin_marketplace_by_id(
    state: &AppState,
    marketplace_id: &str,
    actor: &str,
) -> Result<PluginCatalogSyncResponse, ApiError> {
    let _lease = CatalogSyncLease::acquire(marketplace_id)?;
    let result = sync_plugin_marketplace_inner(state, marketplace_id).await;
    let (outcome, details) = match &result {
        Ok(response) => (
            "success",
            BTreeMap::from([
                ("revision".to_string(), json!(response.revision)),
                ("issued_at".to_string(), json!(response.issued_at)),
                ("catalog_sha256".to_string(), json!(response.catalog_sha256)),
                ("plugin_count".to_string(), json!(response.plugin_count)),
                ("release_count".to_string(), json!(response.release_count)),
                (
                    "component_snapshot_count".to_string(),
                    json!(response.component_snapshot_count),
                ),
                (
                    "signing_key_count".to_string(),
                    json!(response.signing_key_count),
                ),
            ]),
        ),
        Err(error) => (
            "failed",
            BTreeMap::from([("error".to_string(), json!(error.message))]),
        ),
    };
    let audit = plugin_audit_record(
        PLUGIN_AUDIT_SYNC_MARKETPLACE,
        actor,
        None,
        format!("marketplace:{marketplace_id}").as_str(),
        None,
        outcome,
        details,
    );
    if let Err(error) = state.store.insert_plugin_audit(&audit).await {
        tracing::warn!(
            marketplace_id,
            error = error.as_str(),
            "persist Plugin Catalog sync audit failed"
        );
    }
    result
}

async fn sync_plugin_marketplace_inner(
    state: &AppState,
    marketplace_id: &str,
) -> Result<PluginCatalogSyncResponse, ApiError> {
    let mut marketplace = state
        .store
        .get_plugin_marketplace(marketplace_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or_else(|| ApiError::not_found("Plugin Marketplace not found"))?;
    if !is_syncable_network_marketplace(&marketplace) {
        return Err(ApiError::conflict(
            "Plugin Marketplace is not enabled and trusted for Catalog sync",
        ));
    }
    let catalog_url = validate_catalog_url(
        marketplace
            .catalog_url
            .as_deref()
            .ok_or_else(|| ApiError::conflict("Plugin Marketplace is missing catalog_url"))?,
    )?;
    let previous = state
        .store
        .get_plugin_catalog_sync(marketplace_id)
        .await
        .map_err(ApiError::internal)?;
    let current_trusted_keys = previous
        .as_ref()
        .map(|record| {
            record
                .document
                .signing_keys
                .iter()
                .filter(|key| {
                    key.publisher_id == record.catalog_authority_publisher_id
                        && key
                            .usages
                            .iter()
                            .any(|usage| usage == PLUGIN_SIGNING_KEY_USAGE_CATALOG)
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| marketplace.trusted_signing_keys.clone());
    if current_trusted_keys.is_empty() {
        return Err(ApiError::conflict(
            "Plugin Marketplace has no bootstrap Catalog signing key",
        ));
    }

    let document = fetch_catalog_document(state, &catalog_url).await?;
    if document.marketplace_id != marketplace.id {
        return Err(ApiError::conflict(
            "signed Catalog marketplace_id does not match the configured Marketplace",
        ));
    }
    validate_catalog_limits(&document)?;
    let catalog_sha256 = normalized_plugin_catalog_sha256(&document).map_err(|error| {
        ApiError::conflict(format!("normalize signed Plugin Catalog failed: {error}"))
    })?;
    let unchanged = previous
        .as_ref()
        .is_some_and(|record| record.revision == document.revision);
    if unchanged {
        let previous = previous
            .as_ref()
            .ok_or_else(|| ApiError::internal("unchanged Catalog snapshot is missing"))?;
        if previous.catalog_sha256 != catalog_sha256 || previous.document != document {
            return Err(ApiError::conflict(
                "signed Plugin Catalog reuses a revision with different content",
            ));
        }
        verify_plugin_catalog_document(&document, current_trusted_keys.as_slice()).map_err(
            |error| {
                ApiError::conflict(format!(
                    "signed Plugin Catalog verification failed: {error}"
                ))
            },
        )?;
    } else {
        verify_plugin_catalog_update(
            &document,
            current_trusted_keys.as_slice(),
            previous.as_ref().map(|record| record.revision.as_str()),
            previous.as_ref().map(|record| record.issued_at.as_str()),
        )
        .map_err(|error| {
            ApiError::conflict(format!(
                "signed Plugin Catalog verification failed: {error}"
            ))
        })?;
    }
    let catalog_authority_publisher_id =
        validate_catalog_authority_continuity(&document, current_trusted_keys.as_slice())?;
    if previous.as_ref().is_some_and(|record| {
        record.catalog_authority_publisher_id != catalog_authority_publisher_id
    }) {
        return Err(ApiError::conflict(
            "signed Plugin Catalog changes the Marketplace authority publisher",
        ));
    }
    validate_catalog_urls(&document)?;
    if !unchanged {
        let previous = previous.as_ref();
        if let Some(previous) = previous {
            validate_catalog_progression(&previous.document, &document)?;
        }
    }
    validate_catalog_against_store(state, &document, unchanged).await?;

    let mut staged_cloud_bundles = Vec::new();
    let mut staged_mcp_runtime_bundles = Vec::new();
    for release in &document.releases {
        let bundles =
            super::plugin_cloud_bundles::stage_release_cloud_bundles(state, release).await?;
        for bundle in &bundles {
            let snapshot = document
                .component_snapshots
                .iter()
                .find(|snapshot| {
                    snapshot.plugin_id == bundle.plugin_id
                        && snapshot.release_id == bundle.release_id
                        && snapshot.component.component_key == bundle.component_key
                })
                .ok_or_else(|| {
                    ApiError::conflict(format!(
                        "Catalog is missing cloud component snapshot {}/{}/{}",
                        bundle.plugin_id, bundle.release_id, bundle.component_key
                    ))
                })?;
            if snapshot.content_sha256 != bundle.bundle_sha256
                || snapshot.component.execution_host != bundle.execution_host
            {
                return Err(ApiError::conflict(format!(
                    "Catalog cloud component snapshot does not match artifact Bundle {}/{}/{}",
                    bundle.plugin_id, bundle.release_id, bundle.component_key
                )));
            }
        }
        let mcp_runtime_bundles =
            super::plugin_cloud_bundles::stage_release_cloud_mcp_runtime_bundles(
                state,
                release,
                Some(document.component_snapshots.as_slice()),
            )
            .await?;
        staged_cloud_bundles.extend(bundles);
        staged_mcp_runtime_bundles.extend(mcp_runtime_bundles);
    }

    let synced_at = now_rfc3339();
    let sync_record = PluginCatalogSyncRecord {
        marketplace_id: marketplace.id.clone(),
        revision: document.revision.clone(),
        issued_at: document.issued_at.clone(),
        catalog_sha256: catalog_sha256.clone(),
        catalog_authority_publisher_id,
        document: document.clone(),
        synced_at: synced_at.clone(),
    };
    let committed = state
        .store
        .commit_plugin_catalog_sync(
            &sync_record,
            previous.as_ref().map(|record| record.revision.as_str()),
        )
        .await
        .map_err(ApiError::internal)?;
    if !committed {
        return Err(ApiError::conflict(
            "Plugin Catalog changed concurrently; retry the sync",
        ));
    }
    materialize_catalog(
        state,
        &marketplace,
        &document,
        staged_cloud_bundles.as_slice(),
        staged_mcp_runtime_bundles.as_slice(),
    )
    .await?;

    marketplace.trusted_signing_keys = document.signing_keys.clone();
    marketplace.last_catalog_revision = Some(document.revision.clone());
    marketplace.last_synced_at = Some(synced_at.clone());
    state
        .store
        .replace_plugin_marketplace(&marketplace)
        .await
        .map_err(ApiError::internal)?;

    Ok(PluginCatalogSyncResponse {
        marketplace_id: marketplace.id,
        revision: document.revision,
        issued_at: document.issued_at,
        catalog_sha256,
        plugin_count: document.plugins.len(),
        release_count: document.releases.len(),
        component_snapshot_count: document.component_snapshots.len(),
        signing_key_count: document.signing_keys.len(),
        synced_at,
    })
}

async fn fetch_catalog_document(
    state: &AppState,
    url: &Url,
) -> Result<PluginCatalogDocument, ApiError> {
    let client = build_catalog_client(url, state.config.plugin_catalog_request_timeout).await?;
    let response = client
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("Plugin Catalog request failed: {error}"))
        })?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(ApiError::bad_gateway(format!(
            "Plugin Catalog source returned status {}",
            response.status().as_u16()
        )));
    }
    if response
        .content_length()
        .is_some_and(|length| length > state.config.plugin_catalog_max_bytes as u64)
    {
        return Err(ApiError::bad_gateway(
            "Plugin Catalog exceeds the configured download size limit",
        ));
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            ApiError::bad_gateway(format!("read Plugin Catalog response failed: {error}"))
        })?;
        if body.len().saturating_add(chunk.len()) > state.config.plugin_catalog_max_bytes {
            return Err(ApiError::bad_gateway(
                "Plugin Catalog exceeded the configured download size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|error| {
        ApiError::bad_gateway(format!("parse Plugin Catalog JSON failed: {error}"))
    })
}

pub(super) async fn build_catalog_client(
    url: &Url,
    timeout: Duration,
) -> Result<reqwest::Client, ApiError> {
    let host = url
        .host_str()
        .ok_or_else(|| ApiError::conflict("Plugin Catalog URL has no host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| ApiError::conflict("Plugin Catalog URL has no usable port"))?;
    let mut addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| {
            ApiError::bad_gateway(format!("resolve Plugin Catalog host failed: {error}"))
        })?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty() || addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(ApiError::conflict(
            "Plugin Catalog host resolved to a non-public network address",
        ));
    }
    reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .connect_timeout(Duration::from_secs(10).min(timeout))
        .timeout(timeout)
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|error| ApiError::internal(format!("build Plugin Catalog client failed: {error}")))
}

pub(super) fn validate_catalog_url(value: &str) -> Result<Url, ApiError> {
    let url = Url::parse(value).map_err(|_| ApiError::conflict("Plugin Catalog URL is invalid"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::conflict(
            "Plugin Catalog URL must use HTTPS without credentials or fragments",
        ));
    }
    Ok(url)
}

fn validate_catalog_limits(document: &PluginCatalogDocument) -> Result<(), ApiError> {
    if document.plugins.len() > MAX_CATALOG_PLUGINS
        || document.releases.len() > MAX_CATALOG_RELEASES
        || document.component_snapshots.len() > MAX_CATALOG_COMPONENT_SNAPSHOTS
        || document.signing_keys.len() > MAX_CATALOG_SIGNING_KEYS
    {
        return Err(ApiError::conflict(
            "signed Plugin Catalog exceeds the supported entry limits",
        ));
    }
    if document
        .plugins
        .iter()
        .any(|plugin| plugin.owner_user_id.is_some())
    {
        return Err(ApiError::conflict(
            "signed Plugin Catalog cannot assert control-plane owner_user_id",
        ));
    }
    let issued_at = DateTime::parse_from_rfc3339(document.issued_at.as_str())
        .map_err(|_| ApiError::conflict("signed Plugin Catalog issued_at is invalid"))?;
    if issued_at.with_timezone(&Utc) > Utc::now() + ChronoDuration::minutes(10) {
        return Err(ApiError::conflict(
            "signed Plugin Catalog issued_at is too far in the future",
        ));
    }
    Ok(())
}

fn validate_catalog_authority_continuity(
    document: &PluginCatalogDocument,
    current_trusted_keys: &[SigningKeyRef],
) -> Result<String, ApiError> {
    let signer = current_trusted_keys
        .iter()
        .find(|key| key.key_id == document.signature.key_id)
        .ok_or_else(|| ApiError::conflict("Catalog signer is not in the current trust root"))?;
    let issued_at = DateTime::parse_from_rfc3339(document.issued_at.as_str())
        .map_err(|_| ApiError::conflict("signed Plugin Catalog issued_at is invalid"))?;
    let has_successor = document.signing_keys.iter().any(|key| {
        key.publisher_id == signer.publisher_id
            && key.revoked_at.is_none()
            && key
                .usages
                .iter()
                .any(|usage| usage == PLUGIN_SIGNING_KEY_USAGE_CATALOG)
            && signing_key_is_valid_at(key, issued_at)
    });
    if !has_successor {
        return Err(ApiError::conflict(
            "signed Plugin Catalog removes every usable Marketplace authority key",
        ));
    }
    Ok(signer.publisher_id.clone())
}

fn signing_key_is_valid_at(key: &SigningKeyRef, at: DateTime<chrono::FixedOffset>) -> bool {
    let Ok(valid_from) = DateTime::parse_from_rfc3339(key.valid_from.as_str()) else {
        return false;
    };
    let valid_until = key
        .valid_until
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok());
    valid_from <= at && valid_until.is_none_or(|until| at <= until)
}

fn validate_catalog_urls(document: &PluginCatalogDocument) -> Result<(), ApiError> {
    for release in &document.releases {
        validate_catalog_url(release.artifact_ref.as_str()).map_err(|_| {
            ApiError::conflict(format!(
                "Plugin Release {} artifact_ref must be a plain HTTPS URL",
                release.id
            ))
        })?;
    }
    Ok(())
}

fn validate_catalog_progression(
    previous: &PluginCatalogDocument,
    next: &PluginCatalogDocument,
) -> Result<(), ApiError> {
    validate_marketplace_signing_key_progression(
        previous.signing_keys.as_slice(),
        next.signing_keys.as_slice(),
    )?;
    let next_plugins = next
        .plugins
        .iter()
        .map(|plugin| (plugin.id.as_str(), plugin))
        .collect::<HashMap<_, _>>();
    let next_releases = next
        .releases
        .iter()
        .map(|release| (release.id.as_str(), release))
        .collect::<HashMap<_, _>>();
    for plugin in &previous.plugins {
        let updated = next_plugins.get(plugin.id.as_str()).ok_or_else(|| {
            ApiError::conflict(format!(
                "Catalog update removes immutable Plugin {}",
                plugin.id
            ))
        })?;
        if plugin.name != updated.name
            || plugin.plugin_key != updated.plugin_key
            || plugin.marketplace_id != updated.marketplace_id
            || plugin.publisher.id != updated.publisher.id
            || plugin.created_at != updated.created_at
        {
            return Err(ApiError::conflict(format!(
                "Catalog update changes immutable Plugin identity {}",
                plugin.id
            )));
        }
        validate_latest_release_progression(plugin, updated, previous, next)?;
    }
    for release in &previous.releases {
        let updated = next_releases.get(release.id.as_str()).ok_or_else(|| {
            ApiError::conflict(format!(
                "Catalog update removes immutable Release {}",
                release.id
            ))
        })?;
        validate_release_progression(release, updated)?;
    }
    let next_snapshots = next
        .component_snapshots
        .iter()
        .map(|snapshot| {
            (
                (
                    snapshot.plugin_id.as_str(),
                    snapshot.release_id.as_str(),
                    snapshot.component.component_key.as_str(),
                ),
                snapshot,
            )
        })
        .collect::<HashMap<_, _>>();
    for snapshot in &previous.component_snapshots {
        let coordinate = (
            snapshot.plugin_id.as_str(),
            snapshot.release_id.as_str(),
            snapshot.component.component_key.as_str(),
        );
        if next_snapshots.get(&coordinate).copied() != Some(snapshot) {
            return Err(ApiError::conflict(format!(
                "Catalog update changes immutable component snapshot {}/{}/{}",
                snapshot.plugin_id, snapshot.release_id, snapshot.component.component_key
            )));
        }
    }
    Ok(())
}

fn validate_latest_release_progression(
    previous_plugin: &PluginCatalogRecord,
    next_plugin: &PluginCatalogRecord,
    previous: &PluginCatalogDocument,
    next: &PluginCatalogDocument,
) -> Result<(), ApiError> {
    if previous_plugin.latest_release_id.is_empty()
        || previous_plugin.latest_release_id == next_plugin.latest_release_id
    {
        return Ok(());
    }
    if next_plugin.latest_release_id.is_empty() {
        let previous_release = next
            .releases
            .iter()
            .find(|release| release.id == previous_plugin.latest_release_id)
            .ok_or_else(|| ApiError::conflict("previous latest Release disappeared"))?;
        if previous_release.revoked_at.is_none() {
            return Err(ApiError::conflict(
                "Catalog update clears a non-revoked latest stable Release",
            ));
        }
        return Ok(());
    }
    let previous_release = previous
        .releases
        .iter()
        .find(|release| release.id == previous_plugin.latest_release_id)
        .ok_or_else(|| ApiError::conflict("previous latest Release is missing"))?;
    let next_release = next
        .releases
        .iter()
        .find(|release| release.id == next_plugin.latest_release_id)
        .ok_or_else(|| ApiError::conflict("next latest Release is missing"))?;
    let previous_version = Version::parse(previous_release.version.as_str())
        .map_err(|_| ApiError::conflict("previous latest Release version is invalid"))?;
    let next_version = Version::parse(next_release.version.as_str())
        .map_err(|_| ApiError::conflict("next latest Release version is invalid"))?;
    if next_version <= previous_version {
        return Err(ApiError::conflict(
            "Catalog update rolls back the latest stable Release version",
        ));
    }
    Ok(())
}

fn validate_release_progression(
    previous: &PluginReleaseRecord,
    next: &PluginReleaseRecord,
) -> Result<(), ApiError> {
    let mut previous_immutable = previous.clone();
    previous_immutable.revoked_at = None;
    let mut next_immutable = next.clone();
    next_immutable.revoked_at = None;
    if previous_immutable != next_immutable {
        return Err(ApiError::conflict(format!(
            "Catalog update changes immutable Release {}",
            previous.id
        )));
    }
    if previous.revoked_at.is_some() && previous.revoked_at != next.revoked_at {
        return Err(ApiError::conflict(format!(
            "Catalog update removes or changes Release revocation {}",
            previous.id
        )));
    }
    Ok(())
}

async fn validate_catalog_against_store(
    state: &AppState,
    document: &PluginCatalogDocument,
    allow_committed_snapshot_repair: bool,
) -> Result<(), ApiError> {
    for plugin in &document.plugins {
        if let Some(existing) = state
            .store
            .get_plugin_catalog_entry(plugin.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if existing.marketplace_id != document.marketplace_id
                || existing.name != plugin.name
                || existing.plugin_key != plugin.plugin_key
                || existing.publisher.id != plugin.publisher.id
                || existing.created_at != plugin.created_at
            {
                return Err(ApiError::conflict(format!(
                    "Catalog Plugin identity conflicts with stored record {}",
                    plugin.id
                )));
            }
        }
    }
    for release in &document.releases {
        if let Some(existing) = state
            .store
            .get_plugin_release(release.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            validate_release_progression(&existing, release)?;
        }
        if let Some(existing) = state
            .store
            .find_plugin_release_by_version(release.plugin_id.as_str(), release.version.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            if existing.id != release.id {
                return Err(ApiError::conflict(format!(
                    "Catalog Release version conflicts with immutable stored Release {}@{}",
                    release.plugin_id, release.version
                )));
            }
            validate_release_progression(&existing, release)?;
        }
        let mut incoming_snapshots = document
            .component_snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.plugin_id == release.plugin_id && snapshot.release_id == release.id
            })
            .cloned()
            .collect::<Vec<_>>();
        incoming_snapshots.sort_by(|left, right| {
            left.component
                .component_key
                .cmp(&right.component.component_key)
        });
        let mut existing_snapshots = state
            .store
            .list_plugin_component_snapshots(release.plugin_id.as_str(), release.id.as_str())
            .await
            .map_err(ApiError::internal)?;
        existing_snapshots.sort_by(|left, right| {
            left.component
                .component_key
                .cmp(&right.component.component_key)
        });
        if !allow_committed_snapshot_repair
            && !existing_snapshots.is_empty()
            && existing_snapshots != incoming_snapshots
        {
            return Err(ApiError::conflict(format!(
                "Catalog component snapshots conflict with immutable stored Release {}",
                release.id
            )));
        }
    }
    Ok(())
}

async fn materialize_catalog(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    document: &PluginCatalogDocument,
    cloud_bundles: &[PluginCloudComponentBundle],
    mcp_runtime_bundles: &[PluginMcpCloudRuntimeBundle],
) -> Result<(), ApiError> {
    let mut staged_release_ids = Vec::new();
    for release in &document.releases {
        let ready = state
            .store
            .get_plugin_release(release.id.as_str())
            .await
            .map_err(ApiError::internal)?
            .is_some();
        if !ready {
            state
                .store
                .set_plugin_release_publication_ready(release.id.as_str(), false)
                .await
                .map_err(ApiError::internal)?;
            staged_release_ids.push(release.id.clone());
        }
        match state
            .store
            .get_plugin_release_any_state(release.id.as_str())
            .await
            .map_err(ApiError::internal)?
        {
            Some(existing) if existing == *release => {}
            Some(_) => state
                .store
                .replace_plugin_release(release)
                .await
                .map_err(ApiError::internal)?,
            None => state
                .store
                .insert_plugin_release(release)
                .await
                .map_err(ApiError::internal)?,
        }
        let snapshots = document
            .component_snapshots
            .iter()
            .filter(|snapshot| {
                snapshot.plugin_id == release.plugin_id && snapshot.release_id == release.id
            })
            .cloned()
            .collect::<Vec<_>>();
        state
            .store
            .replace_plugin_component_snapshots(
                release.plugin_id.as_str(),
                release.id.as_str(),
                snapshots.as_slice(),
            )
            .await
            .map_err(ApiError::internal)?;
    }
    state
        .store
        .insert_plugin_cloud_component_bundles(cloud_bundles)
        .await
        .map_err(ApiError::internal)?;
    state
        .store
        .insert_plugin_mcp_cloud_runtime_bundles(mcp_runtime_bundles)
        .await
        .map_err(ApiError::internal)?;
    for release_id in staged_release_ids {
        state
            .store
            .set_plugin_release_publication_ready(release_id.as_str(), true)
            .await
            .map_err(ApiError::internal)?;
    }
    for plugin in &document.plugins {
        let mut plugin = plugin.clone();
        apply_marketplace_catalog_scope(marketplace, &mut plugin);
        state
            .store
            .replace_plugin_catalog_entry(&plugin)
            .await
            .map_err(ApiError::internal)?;
    }
    Ok(())
}

fn is_syncable_network_marketplace(marketplace: &PluginMarketplaceRecord) -> bool {
    marketplace.enabled
        && marketplace.trust_level == PLUGIN_TRUST_TRUSTED
        && marketplace.catalog_url.is_some()
        && matches!(
            marketplace.source_kind.as_str(),
            PLUGIN_MARKETPLACE_SOURCE_OFFICIAL_REGISTRY | PLUGIN_MARKETPLACE_SOURCE_ADMIN_REGISTRY
        )
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

struct CatalogSyncLease {
    marketplace_id: String,
}

impl CatalogSyncLease {
    fn acquire(marketplace_id: &str) -> Result<Self, ApiError> {
        let mut active = ACTIVE_CATALOG_SYNCS
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
            .map_err(|_| ApiError::internal("Plugin Catalog sync lock is poisoned"))?;
        if !active.insert(marketplace_id.to_string()) {
            return Err(ApiError::conflict(
                "Plugin Marketplace Catalog sync is already in progress",
            ));
        }
        Ok(Self {
            marketplace_id: marketplace_id.to_string(),
        })
    }
}

impl Drop for CatalogSyncLease {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_CATALOG_SYNCS
            .get_or_init(|| Mutex::new(HashSet::new()))
            .lock()
        {
            active.remove(self.marketplace_id.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    use super::*;
    use chatos_plugin_management_sdk::{
        PluginCatalogSignature, PLUGIN_CATALOG_SCHEMA_VERSION_V1,
        PLUGIN_SIGNATURE_ALGORITHM_ED25519,
    };

    #[test]
    fn catalog_urls_reject_non_https_credentials_and_fragments() {
        assert!(validate_catalog_url("https://plugins.example.com/catalog.json").is_ok());
        assert!(validate_catalog_url("http://plugins.example.com/catalog.json").is_err());
        assert!(validate_catalog_url("https://user@plugins.example.com/catalog.json").is_err());
        assert!(validate_catalog_url("https://plugins.example.com/catalog.json#v1").is_err());
    }

    #[test]
    fn catalog_fetch_rejects_private_and_special_networks() {
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

    #[test]
    fn signing_key_rotation_requires_revocation_before_removal() {
        let key = signing_key("root-v1", None);
        let previous = catalog_with_keys("revision-1", vec![key.clone()]);
        let next = catalog_with_keys("revision-2", Vec::new());
        assert!(validate_marketplace_signing_key_progression(
            previous.signing_keys.as_slice(),
            next.signing_keys.as_slice(),
        )
        .is_err());

        let revoked = signing_key("root-v1", Some("2026-07-25T01:00:00Z"));
        let previous = catalog_with_keys("revision-1", vec![revoked]);
        assert!(validate_marketplace_signing_key_progression(
            previous.signing_keys.as_slice(),
            next.signing_keys.as_slice(),
        )
        .is_ok());
    }

    fn signing_key(key_id: &str, revoked_at: Option<&str>) -> SigningKeyRef {
        SigningKeyRef {
            key_id: key_id.to_string(),
            publisher_id: "marketplace-authority".to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            public_key_base64: STANDARD.encode([1_u8; 32]),
            usages: vec![PLUGIN_SIGNING_KEY_USAGE_CATALOG.to_string()],
            valid_from: "2026-01-01T00:00:00Z".to_string(),
            valid_until: Some("2027-01-01T00:00:00Z".to_string()),
            revoked_at: revoked_at.map(ToOwned::to_owned),
        }
    }

    fn catalog_with_keys(
        revision: &str,
        signing_keys: Vec<SigningKeyRef>,
    ) -> PluginCatalogDocument {
        PluginCatalogDocument {
            schema_version: PLUGIN_CATALOG_SCHEMA_VERSION_V1,
            marketplace_id: "marketplace-demo".to_string(),
            revision: revision.to_string(),
            issued_at: "2026-07-25T00:00:00Z".to_string(),
            signing_keys,
            plugins: Vec::new(),
            releases: Vec::new(),
            component_snapshots: Vec::new(),
            revoked_release_ids: Vec::new(),
            signature: PluginCatalogSignature {
                key_id: "root-v1".to_string(),
                marketplace_id: "marketplace-demo".to_string(),
                algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
                signature_base64: STANDARD.encode([0_u8; 64]),
                signed_at: "2026-07-25T00:00:00Z".to_string(),
                catalog_sha256: "0".repeat(64),
            },
        }
    }
}
