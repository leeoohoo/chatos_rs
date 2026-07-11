// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::config::normalized_env;

const DEFAULT_BUCKET: &str = "chatos-releases";
const DEFAULT_REGION: &str = "us-east-1";
const DEFAULT_CHANNEL: &str = "stable";
const DEFAULT_PRESIGN_EXPIRES_SECONDS: u64 = 900;
const DEFAULT_MAX_ARTIFACT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const S3_SERVICE: &str = "s3";
const SIGNED_HEADERS: &str = "host";
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

#[derive(Debug, Clone)]
pub struct ReleaseStorageConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub channel: String,
    pub presign_expires_seconds: u64,
    pub max_artifact_bytes: u64,
}

impl ReleaseStorageConfig {
    pub fn from_env() -> Result<Option<Self>, String> {
        let endpoint = first_env(&[
            "OFFICIAL_WEBSITE_RELEASES_ENDPOINT",
            "CHATOS_OBJECT_STORAGE_ENDPOINT",
            "MINIO_ENDPOINT",
        ]);
        let Some(endpoint) = endpoint else {
            return Ok(None);
        };
        let endpoint = endpoint.trim_end_matches('/').to_string();
        Url::parse(endpoint.as_str())
            .map_err(|err| format!("release storage endpoint is invalid: {err}"))?;

        let access_key = first_env(&[
            "OFFICIAL_WEBSITE_RELEASES_ACCESS_KEY",
            "CHATOS_OBJECT_STORAGE_ACCESS_KEY",
            "MINIO_ACCESS_KEY",
            "MINIO_ROOT_USER",
        ])
        .ok_or_else(|| "release storage access key is not configured".to_string())?;
        let secret_key = first_env(&[
            "OFFICIAL_WEBSITE_RELEASES_SECRET_KEY",
            "CHATOS_OBJECT_STORAGE_SECRET_KEY",
            "MINIO_SECRET_KEY",
            "MINIO_ROOT_PASSWORD",
        ])
        .ok_or_else(|| "release storage secret key is not configured".to_string())?;
        let bucket = first_env(&["OFFICIAL_WEBSITE_RELEASES_BUCKET", "MINIO_RELEASES_BUCKET"])
            .unwrap_or_else(|| DEFAULT_BUCKET.to_string());
        let region = first_env(&[
            "OFFICIAL_WEBSITE_RELEASES_REGION",
            "CHATOS_OBJECT_STORAGE_REGION",
            "AWS_REGION",
        ])
        .unwrap_or_else(|| DEFAULT_REGION.to_string());
        let channel = normalized_env("OFFICIAL_WEBSITE_RELEASE_CHANNEL")
            .unwrap_or_else(|| DEFAULT_CHANNEL.to_string());
        validate_segment(channel.as_str(), "release channel")?;
        let presign_expires_seconds = env_u64("OFFICIAL_WEBSITE_RELEASE_PRESIGN_EXPIRES_SECONDS")
            .unwrap_or(DEFAULT_PRESIGN_EXPIRES_SECONDS)
            .clamp(60, 604_800);
        let max_artifact_bytes = env_u64("OFFICIAL_WEBSITE_RELEASE_MAX_ARTIFACT_BYTES")
            .unwrap_or(DEFAULT_MAX_ARTIFACT_BYTES)
            .max(1);

        Ok(Some(Self {
            endpoint,
            region,
            bucket,
            access_key,
            secret_key,
            channel,
            presign_expires_seconds,
            max_artifact_bytes,
        }))
    }

    pub fn manifest_key(&self) -> String {
        format!("releases/local-connector/{}/latest.json", self.channel)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientReleaseManifest {
    pub product: String,
    pub channel: String,
    pub version: String,
    pub published_at: String,
    pub artifacts: Vec<ClientReleaseArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientReleaseArtifact {
    pub platform: String,
    pub label: String,
    pub file_name: String,
    pub object_key: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresignReleaseRequest {
    pub version: String,
    #[serde(default)]
    pub artifacts: Vec<PresignArtifactRequest>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PresignArtifactRequest {
    pub platform: String,
    pub label: String,
    pub file_name: String,
    pub content_type: Option<String>,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresignReleaseResponse {
    pub manifest: ClientReleaseManifest,
    pub artifact_uploads: Vec<PresignedArtifactUpload>,
    pub manifest_upload: PresignedManifestUpload,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresignedArtifactUpload {
    pub platform: String,
    pub object_key: String,
    pub upload_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresignedManifestUpload {
    pub object_key: String,
    pub upload_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadCatalog {
    pub storage_configured: bool,
    pub available: bool,
    pub message: String,
    pub release: Option<PublicClientRelease>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicClientRelease {
    pub product: String,
    pub channel: String,
    pub version: String,
    pub published_at: String,
    pub artifacts: Vec<PublicClientArtifact>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicClientArtifact {
    pub platform: String,
    pub label: String,
    pub file_name: String,
    pub content_type: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub download_url: String,
}

#[derive(Clone)]
pub struct ReleaseStorage {
    http: reqwest::Client,
    config: ReleaseStorageConfig,
}

#[derive(Debug, Clone, Copy)]
enum S3Method {
    Get,
    Put,
}

impl S3Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Put => "PUT",
        }
    }
}

impl ReleaseStorage {
    pub fn new(config: ReleaseStorageConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
        }
    }

    pub async fn catalog(&self) -> DownloadCatalog {
        match self.fetch_manifest().await {
            Ok(manifest) => DownloadCatalog {
                storage_configured: true,
                available: !manifest.artifacts.is_empty(),
                message: if manifest.artifacts.is_empty() {
                    "客户端版本正在准备中".to_string()
                } else {
                    "最新稳定版已就绪".to_string()
                },
                release: Some(public_release(manifest)),
            },
            Err(err) => {
                tracing::info!(error = %err, "client release manifest is unavailable");
                DownloadCatalog {
                    storage_configured: true,
                    available: false,
                    message: "客户端版本正在准备中".to_string(),
                    release: None,
                }
            }
        }
    }

    pub async fn download_url(&self, platform: &str) -> Result<String, String> {
        validate_segment(platform, "platform")?;
        let manifest = self.fetch_manifest().await?;
        let artifact = manifest
            .artifacts
            .iter()
            .find(|artifact| artifact.platform == platform)
            .ok_or_else(|| format!("client artifact is unavailable for platform: {platform}"))?;
        self.presigned_object_url(
            S3Method::Get,
            artifact.object_key.as_str(),
            self.config.presign_expires_seconds,
        )
    }

    pub fn presign_release(
        &self,
        request: PresignReleaseRequest,
    ) -> Result<PresignReleaseResponse, String> {
        validate_segment(request.version.as_str(), "release version")?;
        if request.artifacts.is_empty() || request.artifacts.len() > 10 {
            return Err("release must contain between 1 and 10 artifacts".to_string());
        }

        let mut artifacts = Vec::with_capacity(request.artifacts.len());
        let mut artifact_uploads = Vec::with_capacity(request.artifacts.len());
        for input in request.artifacts {
            validate_segment(input.platform.as_str(), "platform")?;
            validate_file_name(input.file_name.as_str())?;
            if input.label.trim().is_empty() || input.label.chars().count() > 120 {
                return Err("artifact label is invalid".to_string());
            }
            if input.size_bytes == 0 || input.size_bytes > self.config.max_artifact_bytes {
                return Err(format!(
                    "artifact size must be between 1 and {} bytes",
                    self.config.max_artifact_bytes
                ));
            }
            if input.sha256.len() != 64 || !input.sha256.chars().all(|ch| ch.is_ascii_hexdigit()) {
                return Err("artifact sha256 must contain 64 hexadecimal characters".to_string());
            }
            let object_key = format!(
                "releases/local-connector/{}/{}/{}",
                self.config.channel, request.version, input.file_name
            );
            let upload_url = self.presigned_object_url(
                S3Method::Put,
                object_key.as_str(),
                self.config.presign_expires_seconds,
            )?;
            artifact_uploads.push(PresignedArtifactUpload {
                platform: input.platform.clone(),
                object_key: object_key.clone(),
                upload_url,
            });
            artifacts.push(ClientReleaseArtifact {
                platform: input.platform,
                label: input.label.trim().to_string(),
                file_name: input.file_name,
                object_key,
                content_type: input
                    .content_type
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                size_bytes: input.size_bytes,
                sha256: input.sha256.to_ascii_lowercase(),
            });
        }

        let manifest = ClientReleaseManifest {
            product: "ChatOS Local Connector".to_string(),
            channel: self.config.channel.clone(),
            version: request.version,
            published_at: Utc::now().to_rfc3339(),
            artifacts,
        };
        let manifest_key = self.config.manifest_key();
        let manifest_upload = PresignedManifestUpload {
            upload_url: self.presigned_object_url(
                S3Method::Put,
                manifest_key.as_str(),
                self.config.presign_expires_seconds,
            )?,
            object_key: manifest_key,
        };

        Ok(PresignReleaseResponse {
            manifest,
            artifact_uploads,
            manifest_upload,
        })
    }

    async fn fetch_manifest(&self) -> Result<ClientReleaseManifest, String> {
        let key = self.config.manifest_key();
        let url = self.presigned_object_url(S3Method::Get, key.as_str(), 300)?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|err| format!("read client release manifest failed: {err}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "read client release manifest failed with status {}",
                response.status()
            ));
        }
        response
            .json::<ClientReleaseManifest>()
            .await
            .map_err(|err| format!("decode client release manifest failed: {err}"))
    }

    fn presigned_object_url(
        &self,
        method: S3Method,
        object_key: &str,
        expires_seconds: u64,
    ) -> Result<String, String> {
        let endpoint = Url::parse(self.config.endpoint.as_str())
            .map_err(|err| format!("release storage endpoint is invalid: {err}"))?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| "release storage endpoint is missing host".to_string())?;
        let host_header = match endpoint.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_scope = now.format("%Y%m%d").to_string();
        let credential_scope = format!(
            "{}/{}/{}/aws4_request",
            date_scope, self.config.region, S3_SERVICE
        );
        let credential = format!("{}/{}", self.config.access_key, credential_scope);
        let canonical_uri = format!(
            "/{}/{}",
            encode_path_segment(self.config.bucket.as_str()),
            encode_object_key(object_key)
        );
        let query_pairs = vec![
            (
                "X-Amz-Algorithm".to_string(),
                "AWS4-HMAC-SHA256".to_string(),
            ),
            ("X-Amz-Credential".to_string(), credential),
            ("X-Amz-Date".to_string(), amz_date.clone()),
            (
                "X-Amz-Expires".to_string(),
                expires_seconds.clamp(1, 604_800).to_string(),
            ),
            (
                "X-Amz-SignedHeaders".to_string(),
                SIGNED_HEADERS.to_string(),
            ),
        ];
        let canonical_query = canonical_query_string(&query_pairs);
        let canonical_headers = format!("host:{host_header}\n");
        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method.as_str(),
            canonical_uri,
            canonical_query,
            canonical_headers,
            SIGNED_HEADERS,
            UNSIGNED_PAYLOAD
        );
        let request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date, credential_scope, request_hash
        );
        let signature = hex::encode(signing_key_signature(
            self.config.secret_key.as_bytes(),
            date_scope.as_str(),
            self.config.region.as_str(),
            S3_SERVICE,
            string_to_sign.as_bytes(),
        ));
        let mut signed_pairs = query_pairs;
        signed_pairs.push(("X-Amz-Signature".to_string(), signature));
        Ok(format!(
            "{}{}?{}",
            self.config.endpoint,
            canonical_uri,
            canonical_query_string(&signed_pairs)
        ))
    }
}

pub fn unavailable_catalog() -> DownloadCatalog {
    DownloadCatalog {
        storage_configured: false,
        available: false,
        message: "客户端版本正在准备中".to_string(),
        release: None,
    }
}

fn public_release(manifest: ClientReleaseManifest) -> PublicClientRelease {
    PublicClientRelease {
        product: manifest.product,
        channel: manifest.channel,
        version: manifest.version,
        published_at: manifest.published_at,
        artifacts: manifest
            .artifacts
            .into_iter()
            .map(|artifact| PublicClientArtifact {
                download_url: format!("/api/site/downloads/{}", artifact.platform),
                platform: artifact.platform,
                label: artifact.label,
                file_name: artifact.file_name,
                content_type: artifact.content_type,
                size_bytes: artifact.size_bytes,
                sha256: artifact.sha256,
            })
            .collect(),
    }
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| normalized_env(key))
}

fn env_u64(key: &str) -> Option<u64> {
    normalized_env(key).and_then(|value| value.parse::<u64>().ok())
}

fn validate_segment(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 80
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn validate_file_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 180
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        return Err("artifact file name is invalid".to_string());
    }
    Ok(())
}

fn signing_key_signature(
    secret_key: &[u8],
    date_scope: &str,
    region: &str,
    service: &str,
    string_to_sign: &[u8],
) -> [u8; 32] {
    let k_date = hmac_sha256(
        format!("AWS4{}", String::from_utf8_lossy(secret_key)).as_bytes(),
        date_scope.as_bytes(),
    );
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    hmac_sha256(&k_signing, string_to_sign)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    let mut key_block = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let digest = Sha256::digest(key);
        key_block[..32].copy_from_slice(digest.as_slice());
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut outer = [0x5cu8; BLOCK_SIZE];
    let mut inner = [0x36u8; BLOCK_SIZE];
    for index in 0..BLOCK_SIZE {
        outer[index] ^= key_block[index];
        inner[index] ^= key_block[index];
    }
    let mut inner_hasher = Sha256::new();
    inner_hasher.update(inner);
    inner_hasher.update(data);
    let inner_digest = inner_hasher.finalize();
    let mut outer_hasher = Sha256::new();
    outer_hasher.update(outer);
    outer_hasher.update(inner_digest);
    let digest = outer_hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn canonical_query_string(pairs: &[(String, String)]) -> String {
    let mut encoded = pairs
        .iter()
        .map(|(key, value)| (percent_encode(key, true), percent_encode(value, true)))
        .collect::<Vec<_>>();
    encoded.sort();
    encoded
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode_object_key(value: &str) -> String {
    value
        .split('/')
        .map(encode_path_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn encode_path_segment(value: &str) -> String {
    percent_encode(value, false)
}

fn percent_encode(value: &str, encode_slash: bool) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let ch = *byte as char;
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '~') {
            out.push(ch);
        } else if ch == '/' && !encode_slash {
            out.push('/');
        } else {
            out.push_str(format!("%{:02X}", byte).as_str());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        validate_file_name, validate_segment, PresignArtifactRequest, PresignReleaseRequest,
        ReleaseStorage, ReleaseStorageConfig,
    };

    #[test]
    fn accepts_release_segments_and_artifact_names() {
        assert!(validate_segment("2.0.4", "version").is_ok());
        assert!(validate_segment("windows-x64", "platform").is_ok());
        assert!(validate_file_name("ChatOS-Local-Connector-windows-x64.zip").is_ok());
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(validate_segment("../stable", "channel").is_err());
        assert!(validate_file_name("../client.zip").is_err());
        assert!(validate_file_name("folder/client.zip").is_err());
    }

    #[test]
    fn presigns_release_inside_fixed_prefix() {
        let storage = ReleaseStorage::new(ReleaseStorageConfig {
            endpoint: "https://minio.example.com".to_string(),
            region: "us-east-1".to_string(),
            bucket: "chatos-releases".to_string(),
            access_key: "access".to_string(),
            secret_key: "secret".to_string(),
            channel: "stable".to_string(),
            presign_expires_seconds: 900,
            max_artifact_bytes: 1024 * 1024,
        });
        let response = storage
            .presign_release(PresignReleaseRequest {
                version: "2.0.4".to_string(),
                artifacts: vec![PresignArtifactRequest {
                    platform: "windows-x64".to_string(),
                    label: "Windows 10/11 (64-bit)".to_string(),
                    file_name: "ChatOS-Local-Connector-windows-x64.zip".to_string(),
                    content_type: Some("application/zip".to_string()),
                    size_bytes: 1024,
                    sha256: "a".repeat(64),
                }],
            })
            .expect("presign release");

        assert_eq!(
            response.manifest.artifacts[0].object_key,
            "releases/local-connector/stable/2.0.4/ChatOS-Local-Connector-windows-x64.zip"
        );
        assert_eq!(
            response.manifest_upload.object_key,
            "releases/local-connector/stable/latest.json"
        );
        assert!(response.artifact_uploads[0]
            .upload_url
            .contains("X-Amz-Signature="));
    }
}
