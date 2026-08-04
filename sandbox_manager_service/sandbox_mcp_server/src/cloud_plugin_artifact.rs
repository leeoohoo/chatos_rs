// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chatos_plugin_management_sdk::{normalize_plugin_relative_path, PluginMcpCloudRuntimeBundle};
use chatos_plugin_package::{
    load_verified_plugin_package_directory, verify_plugin_mcp_cloud_artifact_bytes,
    verify_plugin_mcp_cloud_package, PluginPackageLimits, VerifiedPluginPackage,
};
use futures_util::StreamExt;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use url::Url;
use uuid::Uuid;

const MAX_ARTIFACT_BYTES: usize = 16 * 1024 * 1024;
const MAX_PACKAGE_INDEX_BYTES: u64 = 2 * 1024 * 1024;
const ARTIFACT_REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const ARTIFACT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub(crate) struct CloudPluginArtifactStore {
    root: PathBuf,
    materialize_lock: Arc<Mutex<()>>,
    verified_file_sha256: Arc<Mutex<HashMap<String, BTreeMap<String, String>>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct MaterializedCloudPluginArtifact {
    pub(crate) plugin_root: PathBuf,
    pub(crate) package_index: PathBuf,
    pub(crate) command: PathBuf,
    pub(crate) cwd: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignedPackageIndex {
    schema_version: u32,
    files: BTreeMap<String, String>,
}

impl CloudPluginArtifactStore {
    pub(crate) fn new(state_dir: &Path) -> Self {
        Self {
            root: state_dir.join("cloud-plugin-artifacts"),
            materialize_lock: Arc::new(Mutex::new(())),
            verified_file_sha256: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn materialize(
        &self,
        bundle: &PluginMcpCloudRuntimeBundle,
        command: &str,
        cwd: Option<&str>,
    ) -> Result<MaterializedCloudPluginArtifact, String> {
        let command = normalized_package_path(command, "command")?;
        let cwd = cwd
            .map(|value| normalized_package_path(value, "cwd"))
            .transpose()?;
        let url = validate_artifact_url(bundle.artifact_ref.as_str())?;
        let _guard = self.materialize_lock.lock().await;
        create_private_directory_all(self.root.as_path())?;
        let artifact_root = self.root.join(bundle.artifact_sha256.as_str());
        if let Some(expected) = self
            .verified_file_sha256
            .lock()
            .await
            .get(bundle.artifact_sha256.as_str())
            .cloned()
        {
            return validate_materialized_artifact(
                artifact_root.as_path(),
                bundle,
                command.as_str(),
                cwd.as_deref(),
                &expected,
            );
        }

        let bytes = download_artifact(&url).await?;
        let bundle_for_verification = bundle.clone();
        let package = tokio::task::spawn_blocking(move || {
            verify_plugin_mcp_cloud_artifact_bytes(
                bytes.as_slice(),
                &bundle_for_verification,
                cloud_plugin_package_limits(),
            )
            .map_err(|error| format!("verify Plugin cloud artifact failed: {error}"))
        })
        .await
        .map_err(|error| format!("Plugin cloud artifact verification task failed: {error}"))??;

        let artifact_root_for_write = artifact_root.clone();
        let bundle_for_write = bundle.clone();
        let command_for_write = command.clone();
        let cwd_for_write = cwd.clone();
        let expected_file_sha256 = package.file_sha256.clone();
        let expected_file_sha256_for_task = expected_file_sha256.clone();
        tokio::task::spawn_blocking(move || {
            if artifact_root_for_write.exists() {
                validate_materialized_artifact(
                    artifact_root_for_write.as_path(),
                    &bundle_for_write,
                    command_for_write.as_str(),
                    cwd_for_write.as_deref(),
                    &expected_file_sha256_for_task,
                )
                .map(|_| ())
            } else {
                write_materialized_artifact(
                    artifact_root_for_write.as_path(),
                    &bundle_for_write,
                    package,
                    command_for_write.as_str(),
                    cwd_for_write.as_deref(),
                )
            }
        })
        .await
        .map_err(|error| format!("Plugin cloud artifact materialization task failed: {error}"))??;

        self.verified_file_sha256
            .lock()
            .await
            .insert(bundle.artifact_sha256.clone(), expected_file_sha256);
        let expected = self
            .verified_file_sha256
            .lock()
            .await
            .get(bundle.artifact_sha256.as_str())
            .cloned()
            .ok_or_else(|| "Plugin artifact verification cache is unavailable".to_string())?;
        validate_materialized_artifact(
            artifact_root.as_path(),
            bundle,
            command.as_str(),
            cwd.as_deref(),
            &expected,
        )
    }
}

async fn download_artifact(url: &Url) -> Result<Vec<u8>, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "Plugin artifact URL has no host".to_string())?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| "Plugin artifact URL has no usable port".to_string())?;
    let mut addresses = tokio::net::lookup_host((host, port))
        .await
        .map_err(|error| format!("resolve Plugin artifact host failed: {error}"))?
        .collect::<Vec<_>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| address.port() != port || !is_public_ip(address.ip()))
    {
        return Err("Plugin artifact host must resolve only to public addresses".to_string());
    }
    let client = reqwest::Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .https_only(true)
        .connect_timeout(ARTIFACT_CONNECT_TIMEOUT)
        .timeout(ARTIFACT_REQUEST_TIMEOUT)
        .resolve_to_addrs(host, addresses.as_slice())
        .build()
        .map_err(|error| format!("build Plugin artifact client failed: {error}"))?;
    let response = client
        .get(url.clone())
        .header(
            reqwest::header::ACCEPT,
            "application/zip, application/octet-stream",
        )
        .send()
        .await
        .map_err(|_| "Plugin artifact request failed".to_string())?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!(
            "Plugin artifact source returned status {}",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_ARTIFACT_BYTES as u64)
    {
        return Err("Plugin artifact exceeds the cloud package size limit".to_string());
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| "read Plugin artifact failed".to_string())?;
        if body.len().saturating_add(chunk.len()) > MAX_ARTIFACT_BYTES {
            return Err("Plugin artifact exceeded the cloud package size limit".to_string());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn write_materialized_artifact(
    artifact_root: &Path,
    bundle: &PluginMcpCloudRuntimeBundle,
    package: VerifiedPluginPackage,
    command: &str,
    cwd: Option<&str>,
) -> Result<(), String> {
    verify_plugin_mcp_cloud_package(&package, bundle)
        .map_err(|error| format!("verify Plugin cloud package failed: {error}"))?;
    let command_key = package_key(command);
    if !package.file_sha256.contains_key(command_key.as_str()) {
        return Err("Plugin package-relative command is absent from the verified artifact".into());
    }
    let parent = artifact_root
        .parent()
        .ok_or_else(|| "Plugin artifact cache root is invalid".to_string())?;
    let staging = parent.join(format!(".{}.{}", bundle.artifact_sha256, Uuid::new_v4()));
    create_private_directory(staging.as_path())?;
    let result = (|| {
        let plugin_root = staging.join("package");
        create_private_directory(plugin_root.as_path())?;
        for (path, body) in &package.files {
            let destination = plugin_root.join(path);
            let parent = destination
                .parent()
                .ok_or_else(|| "Plugin package file has no parent".to_string())?;
            create_private_directory_all(parent)?;
            write_private_file(destination.as_path(), body.as_slice())?;
        }
        if let Some(cwd) = cwd {
            create_private_directory_all(plugin_root.join(package_key(cwd)).as_path())?;
        }
        let package_index = staging.join("package-index.json");
        let index = SignedPackageIndex {
            schema_version: 1,
            files: package.file_sha256.clone(),
        };
        let bytes = serde_json::to_vec(&index)
            .map_err(|error| format!("serialize Plugin package index failed: {error}"))?;
        write_private_file(package_index.as_path(), bytes.as_slice())?;
        make_tree_read_only(plugin_root.as_path())?;
        set_read_only_file(package_index.as_path())?;
        fs::rename(staging.as_path(), artifact_root)
            .map_err(|error| format!("publish Plugin artifact mount failed: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = make_tree_writable(staging.as_path());
        let _ = fs::remove_dir_all(staging.as_path());
    }
    result
}

fn validate_materialized_artifact(
    artifact_root: &Path,
    bundle: &PluginMcpCloudRuntimeBundle,
    command: &str,
    cwd: Option<&str>,
    expected_file_sha256: &BTreeMap<String, String>,
) -> Result<MaterializedCloudPluginArtifact, String> {
    let metadata = fs::symlink_metadata(artifact_root)
        .map_err(|error| format!("read Plugin artifact mount failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Plugin artifact mount is not a non-symlink directory".to_string());
    }
    let plugin_root = artifact_root.join("package");
    let package_index = artifact_root.join("package-index.json");
    let index_metadata = fs::symlink_metadata(package_index.as_path())
        .map_err(|error| format!("read Plugin package index failed: {error}"))?;
    if index_metadata.file_type().is_symlink()
        || !index_metadata.is_file()
        || index_metadata.len() > MAX_PACKAGE_INDEX_BYTES
    {
        return Err("Plugin package index is unsafe or oversized".to_string());
    }
    let index = serde_json::from_slice::<SignedPackageIndex>(
        fs::read(package_index.as_path())
            .map_err(|error| format!("read Plugin package index failed: {error}"))?
            .as_slice(),
    )
    .map_err(|error| format!("parse Plugin package index failed: {error}"))?;
    if index.schema_version != 1 || index.files.is_empty() || &index.files != expected_file_sha256 {
        return Err("Plugin package index is invalid".to_string());
    }
    let package = load_verified_plugin_package_directory(
        plugin_root.as_path(),
        bundle.artifact_sha256.as_str(),
        &index.files,
        cloud_plugin_package_limits(),
    )
    .map_err(|error| format!("verify materialized Plugin package failed: {error}"))?;
    verify_plugin_mcp_cloud_package(&package, bundle)
        .map_err(|error| format!("verify materialized Plugin identity failed: {error}"))?;
    let command = canonical_package_file(plugin_root.as_path(), command)?;
    let cwd = canonical_package_directory(plugin_root.as_path(), cwd.unwrap_or("."))?;
    Ok(MaterializedCloudPluginArtifact {
        plugin_root: plugin_root
            .canonicalize()
            .map_err(|error| format!("canonicalize Plugin artifact root failed: {error}"))?,
        package_index: package_index
            .canonicalize()
            .map_err(|error| format!("canonicalize Plugin package index failed: {error}"))?,
        command,
        cwd,
    })
}

fn normalized_package_path(value: &str, field: &str) -> Result<String, String> {
    normalize_plugin_relative_path(value)
        .map_err(|error| format!("Plugin package-relative {field} is invalid: {error}"))
}

fn package_key(value: &str) -> String {
    value.trim_start_matches("./").to_string()
}

fn canonical_package_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize Plugin root failed: {error}"))?;
    let path = root.join(package_key(relative));
    let metadata = fs::symlink_metadata(path.as_path())
        .map_err(|error| format!("read Plugin package command failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Plugin package command is not a non-symlink file".to_string());
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("canonicalize Plugin package command failed: {error}"))?;
    if !path.starts_with(root.as_path()) {
        return Err("Plugin package command escapes the immutable artifact".to_string());
    }
    Ok(path)
}

fn canonical_package_directory(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("canonicalize Plugin root failed: {error}"))?;
    let path = if relative == "." {
        root.clone()
    } else {
        root.join(package_key(relative))
    };
    let metadata = fs::symlink_metadata(path.as_path())
        .map_err(|error| format!("read Plugin package cwd failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Plugin package cwd is not a non-symlink directory".to_string());
    }
    let path = path
        .canonicalize()
        .map_err(|error| format!("canonicalize Plugin package cwd failed: {error}"))?;
    if !path.starts_with(root.as_path()) {
        return Err("Plugin package cwd escapes the immutable artifact".to_string());
    }
    Ok(path)
}

fn validate_artifact_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Plugin artifact URL is invalid".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(
            "Plugin artifact URL must use HTTPS without credentials or fragments".to_string(),
        );
    }
    Ok(url)
}

fn cloud_plugin_package_limits() -> PluginPackageLimits {
    PluginPackageLimits {
        max_archive_bytes: MAX_ARTIFACT_BYTES,
        max_entries: 512,
        max_file_bytes: 2 * 1024 * 1024,
        max_unpacked_bytes: 32 * 1024 * 1024,
        ..PluginPackageLimits::default()
    }
}

fn create_private_directory_all(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("create Plugin artifact directory failed: {error}"))?;
    set_private_directory(path)
}

fn create_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir(path)
        .map_err(|error| format!("create Plugin artifact directory failed: {error}"))?;
    set_private_directory(path)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(path)
        .map_err(|error| format!("write Plugin artifact file failed: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("write Plugin artifact file failed: {error}"))?;
    file.sync_all()
        .map_err(|error| format!("sync Plugin artifact file failed: {error}"))
}

fn set_private_directory(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("protect Plugin artifact directory failed: {error}"))?;
    }
    Ok(())
}

fn make_tree_read_only(path: &Path) -> Result<(), String> {
    for entry in
        fs::read_dir(path).map_err(|error| format!("read Plugin artifact tree failed: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read Plugin artifact entry failed: {error}"))?;
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("read Plugin artifact entry failed: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("Plugin artifact tree contains a symlink".to_string());
        }
        if metadata.is_dir() {
            make_tree_read_only(entry.path().as_path())?;
            set_read_only_directory(entry.path().as_path())?;
        } else if metadata.is_file() {
            set_read_only_executable_file(entry.path().as_path())?;
        } else {
            return Err("Plugin artifact tree contains a special file".to_string());
        }
    }
    set_read_only_directory(path)
}

fn set_read_only_directory(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o555))
            .map_err(|error| format!("seal Plugin artifact directory failed: {error}"))?;
    }
    Ok(())
}

fn set_read_only_executable_file(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o555))
            .map_err(|error| format!("seal Plugin artifact file failed: {error}"))?;
    }
    Ok(())
}

fn set_read_only_file(_path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o444))
            .map_err(|error| format!("seal Plugin package index failed: {error}"))?;
    }
    Ok(())
}

fn make_tree_writable(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
    }
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = if metadata.is_dir() { 0o700 } else { 0o600 };
            let _ = fs::set_permissions(entry.path(), fs::Permissions::from_mode(mode));
        }
        if metadata.is_dir() {
            make_tree_writable(entry.path().as_path())?;
        }
    }
    Ok(())
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
        || octets[0] == 0
        || octets[0] >= 224
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 198 && matches!(octets[1], 18 | 19)))
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_multicast()
        || ip
            .to_ipv4_mapped()
            .is_some_and(|value| !is_public_ipv4(value))
    {
        return false;
    }
    let segments = ip.segments();
    let unique_local = segments[0] & 0xfe00 == 0xfc00;
    let link_local = segments[0] & 0xffc0 == 0xfe80;
    let documentation = segments[0] == 0x2001 && segments[1] == 0x0db8;
    !unique_local && !link_local && !documentation
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use chatos_plugin_management_sdk::{
        build_plugin_mcp_cloud_runtime_bundle, parse_plugin_manifest, plugin_component_descriptors,
        PluginManifestSource, PluginReleaseRecord, PluginReleaseSignature,
    };
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    fn fixture_artifact() -> (Vec<u8>, PluginMcpCloudRuntimeBundle) {
        let manifest_raw = json!({
            "schemaVersion": 2,
            "execution": {"defaultHost": "cloud", "componentHosts": {}},
            "name": "cloud-artifact-demo",
            "version": "1.0.0",
            "description": "Cloud artifact fixture",
            "author": {"name": "ChatOS"},
            "mcpServers": {
                "runner": {
                    "type": "stdio",
                    "command": "./bin/server",
                    "args": [],
                    "cwd": "./bin"
                }
            },
            "interface": {
                "displayName": "Cloud Artifact Demo",
                "shortDescription": "Cloud artifact demo",
                "longDescription": "Cloud artifact materialization fixture.",
                "developerName": "ChatOS",
                "category": "Developer Tools"
            },
            "dependencies": {},
            "permissions": [{"permission": "process.spawn", "components": ["runner"]}]
        })
        .to_string();
        let mut files = BTreeMap::from([
            (
                ".chatos-plugin/plugin.json".to_string(),
                manifest_raw.as_bytes().to_vec(),
            ),
            ("bin/server".to_string(), b"#!/bin/sh\nexit 0\n".to_vec()),
            (
                "sbom.spdx.json".to_string(),
                serde_json::to_vec(&json!({
                    "spdxVersion": "SPDX-2.3",
                    "SPDXID": "SPDXRef-DOCUMENT"
                }))
                .unwrap(),
            ),
        ]);
        let checksums = files
            .iter()
            .map(|(path, bytes)| (path.clone(), hex::encode(Sha256::digest(bytes))))
            .collect::<BTreeMap<_, _>>();
        files.insert(
            ".chatos-plugin/checksums.json".to_string(),
            serde_json::to_vec(&json!({"schemaVersion": 1, "files": checksums})).unwrap(),
        );
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut cursor);
            for (path, bytes) in &files {
                writer
                    .start_file(
                        path,
                        SimpleFileOptions::default()
                            .compression_method(CompressionMethod::Deflated)
                            .unix_permissions(0o100644),
                    )
                    .unwrap();
                writer.write_all(bytes).unwrap();
            }
            writer.finish().unwrap();
        }
        let bytes = cursor.into_inner();
        let manifest = parse_plugin_manifest(&manifest_raw, PluginManifestSource::Chatos).unwrap();
        let release = PluginReleaseRecord {
            id: "release-1".to_string(),
            plugin_id: "plugin-1".to_string(),
            version: manifest.version.clone(),
            manifest_schema_version: manifest.schema_version,
            normalized_manifest: manifest.clone(),
            artifact_ref: "https://plugins.example.com/cloud-artifact-demo.zip".to_string(),
            artifact_sha256: hex::encode(Sha256::digest(bytes.as_slice())),
            signature: PluginReleaseSignature {
                key_id: "fixture-key".to_string(),
                publisher_id: "publisher-1".to_string(),
                marketplace_id: "marketplace-1".to_string(),
                algorithm: "ed25519".to_string(),
                signature_base64: "fixture".to_string(),
                signed_at: "2026-08-01T00:00:00Z".to_string(),
                manifest_sha256: "a".repeat(64),
            },
            sbom_ref: Some("./sbom.spdx.json".to_string()),
            supported_platforms: manifest.dependencies.supported_platforms.clone(),
            components: plugin_component_descriptors(&manifest),
            dependencies: manifest.dependencies.clone(),
            permissions: manifest.permissions.clone(),
            release_channel: "stable".to_string(),
            published_at: "2026-08-01T00:00:00Z".to_string(),
            revoked_at: None,
        };
        let bundle = build_plugin_mcp_cloud_runtime_bundle(&release, "runner").unwrap();
        (bytes, bundle)
    }

    #[test]
    fn artifact_urls_and_package_paths_fail_closed() {
        assert!(validate_artifact_url("https://plugins.example.com/demo.zip").is_ok());
        assert!(validate_artifact_url("http://plugins.example.com/demo.zip").is_err());
        assert!(validate_artifact_url("https://user@plugins.example.com/demo.zip").is_err());
        assert!(normalized_package_path("./bin/server", "command").is_ok());
        assert!(normalized_package_path("../server", "command").is_err());
    }

    #[test]
    fn private_and_special_artifact_addresses_are_rejected() {
        for address in [
            "127.0.0.1:443",
            "10.0.0.1:443",
            "169.254.169.254:443",
            "[::1]:443",
            "[fc00::1]:443",
        ] {
            let address = address.parse::<std::net::SocketAddr>().unwrap();
            assert!(!is_public_ip(address.ip()));
        }
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn materialized_artifact_is_read_only_and_detects_file_tampering() {
        let (bytes, bundle) = fixture_artifact();
        let package = verify_plugin_mcp_cloud_artifact_bytes(
            bytes.as_slice(),
            &bundle,
            cloud_plugin_package_limits(),
        )
        .unwrap();
        let expected = package.file_sha256.clone();
        let temp = tempfile::tempdir().unwrap();
        let artifact_root = temp.path().join(bundle.artifact_sha256.as_str());
        write_materialized_artifact(
            artifact_root.as_path(),
            &bundle,
            package,
            "./bin/server",
            Some("./bin"),
        )
        .unwrap();
        let materialized = validate_materialized_artifact(
            artifact_root.as_path(),
            &bundle,
            "./bin/server",
            Some("./bin"),
            &expected,
        )
        .unwrap();
        assert!(materialized.command.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(materialized.command.as_path())
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o222, 0);
            fs::set_permissions(
                materialized.command.as_path(),
                fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        fs::write(materialized.command.as_path(), b"tampered").unwrap();
        assert!(validate_materialized_artifact(
            artifact_root.as_path(),
            &bundle,
            "./bin/server",
            Some("./bin"),
            &expected,
        )
        .is_err());
        make_tree_writable(artifact_root.as_path()).unwrap();
    }
}
