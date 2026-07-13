// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::OnceCell;
use url::Url;
use uuid::Uuid;

use crate::config::Config;

const DEFAULT_BUCKET: &str = "chatos-attachments";
const DEFAULT_REGION: &str = "us-east-1";
const DEFAULT_PRESIGN_EXPIRES_SECONDS: u64 = 900;
const DEFAULT_MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;
const DEFAULT_MAX_READ_BYTES: u64 = 30 * 1024 * 1024;
const ATTACHMENT_OBJECT_TOKEN_EXP: u64 = 4_102_444_800; // 2100-01-01T00:00:00Z
const S3_SERVICE: &str = "s3";
const SIGNED_HEADERS: &str = "host";
const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

static OBJECT_STORAGE: OnceCell<Result<ObjectStorageService, String>> = OnceCell::const_new();

#[derive(Debug, Clone)]
pub struct ObjectStorageConfig {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    pub secret_key: String,
    pub presign_expires_seconds: u64,
    pub public_base_url: Option<String>,
    pub token_secret: String,
    pub max_upload_bytes: u64,
    pub max_read_bytes: u64,
}

#[derive(Clone)]
pub struct ObjectStorageService {
    http: Client,
    config: ObjectStorageConfig,
}

#[derive(Debug, Clone)]
pub struct PresignedUploadInput {
    pub user_id: String,
    pub conversation_id: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresignedUpload {
    pub id: String,
    pub bucket: String,
    #[serde(rename = "objectKey")]
    pub object_key: String,
    #[serde(rename = "uploadUrl")]
    pub upload_url: String,
    #[serde(rename = "uploadHeaders")]
    pub upload_headers: HashMap<String, String>,
    #[serde(rename = "viewUrl")]
    pub view_url: String,
    #[serde(rename = "expiresInSeconds")]
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone)]
pub struct StoredObjectRef {
    pub bucket: Option<String>,
    pub object_key: String,
    pub name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StoredObjectBytes {
    pub bytes: bytes::Bytes,
    pub content_type: Option<String>,
    pub content_length: u64,
}

#[derive(Debug, Clone)]
pub struct SignedObject {
    pub object_ref: StoredObjectRef,
    pub content_type: String,
    pub file_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AttachmentObjectClaims {
    bucket: String,
    object_key: String,
    name: String,
    mime_type: String,
    exp: u64,
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

pub async fn service() -> Result<&'static ObjectStorageService, String> {
    let result = OBJECT_STORAGE
        .get_or_init(|| async { ObjectStorageService::from_env().await })
        .await;
    result.as_ref().map_err(Clone::clone)
}

impl ObjectStorageService {
    async fn from_env() -> Result<Self, String> {
        let endpoint = read_optional_env("CHATOS_OBJECT_STORAGE_ENDPOINT")
            .or_else(|| read_optional_env("MINIO_ENDPOINT"))
            .ok_or_else(|| "object storage endpoint is not configured".to_string())?;
        let endpoint = endpoint.trim_end_matches('/').to_string();
        Url::parse(endpoint.as_str())
            .map_err(|err| format!("object storage endpoint is invalid: {err}"))?;
        let access_key = read_optional_env("CHATOS_OBJECT_STORAGE_ACCESS_KEY")
            .or_else(|| read_optional_env("MINIO_ACCESS_KEY"))
            .or_else(|| read_optional_env("MINIO_ROOT_USER"))
            .ok_or_else(|| "object storage access key is not configured".to_string())?;
        let secret_key = read_optional_env("CHATOS_OBJECT_STORAGE_SECRET_KEY")
            .or_else(|| read_optional_env("MINIO_SECRET_KEY"))
            .or_else(|| read_optional_env("MINIO_ROOT_PASSWORD"))
            .ok_or_else(|| "object storage secret key is not configured".to_string())?;
        let bucket = read_optional_env("CHATOS_OBJECT_STORAGE_BUCKET")
            .or_else(|| read_optional_env("MINIO_BUCKET"))
            .unwrap_or_else(|| DEFAULT_BUCKET.to_string());
        let region = read_optional_env("CHATOS_OBJECT_STORAGE_REGION")
            .or_else(|| read_optional_env("AWS_REGION"))
            .unwrap_or_else(|| DEFAULT_REGION.to_string());
        let presign_expires_seconds = read_u64_env("CHATOS_OBJECT_STORAGE_PRESIGN_EXPIRES_SECONDS")
            .unwrap_or(DEFAULT_PRESIGN_EXPIRES_SECONDS)
            .clamp(60, 604_800);
        let public_base_url = read_optional_env("CHATOS_ATTACHMENT_PUBLIC_BASE_URL")
            .or_else(|| read_optional_env("CHATOS_PUBLIC_BASE_URL"))
            .or_else(|| read_optional_env("APP_PUBLIC_BASE_URL"))
            .map(|value| value.trim_end_matches('/').to_string());
        let token_secret = read_optional_env("CHATOS_ATTACHMENT_OBJECT_URL_SECRET")
            .unwrap_or_else(|| Config::get().auth_jwt_secret.clone());
        let max_upload_bytes =
            read_u64_env("CHATOS_ATTACHMENT_UPLOAD_MAX_BYTES").unwrap_or(DEFAULT_MAX_UPLOAD_BYTES);
        let max_read_bytes =
            read_u64_env("CHATOS_ATTACHMENT_READ_MAX_BYTES").unwrap_or(DEFAULT_MAX_READ_BYTES);

        Ok(Self {
            http: Client::new(),
            config: ObjectStorageConfig {
                endpoint,
                region,
                bucket,
                access_key,
                secret_key,
                presign_expires_seconds,
                public_base_url,
                token_secret,
                max_upload_bytes,
                max_read_bytes,
            },
        })
    }

    pub fn max_upload_bytes(&self) -> u64 {
        self.config.max_upload_bytes
    }

    pub fn max_read_bytes(&self) -> u64 {
        self.config.max_read_bytes
    }

    pub fn bucket(&self) -> &str {
        self.config.bucket.as_str()
    }

    pub async fn create_presigned_upload(
        &self,
        input: PresignedUploadInput,
    ) -> Result<PresignedUpload, String> {
        if input.size > self.config.max_upload_bytes {
            return Err(format!(
                "attachment exceeds upload limit: {} > {} bytes",
                input.size, self.config.max_upload_bytes
            ));
        }

        let id = Uuid::new_v4().to_string();
        let object_key = build_object_key(
            input.user_id.as_str(),
            input.conversation_id.as_str(),
            id.as_str(),
            input.name.as_str(),
        );
        let upload_url = self.presigned_object_url(
            S3Method::Put,
            self.config.bucket.as_str(),
            object_key.as_str(),
            self.config.presign_expires_seconds,
        )?;
        let view_url = self.signed_object_url(SignedObject {
            object_ref: StoredObjectRef {
                bucket: Some(self.config.bucket.clone()),
                object_key: object_key.clone(),
                name: Some(input.name),
                mime_type: Some(input.mime_type),
            },
            content_type: "application/octet-stream".to_string(),
            file_name: "attachment".to_string(),
        })?;

        Ok(PresignedUpload {
            id,
            bucket: self.config.bucket.clone(),
            object_key,
            upload_url,
            upload_headers: HashMap::new(),
            view_url,
            expires_in_seconds: self.config.presign_expires_seconds,
        })
    }

    pub fn signed_object_url(&self, object: SignedObject) -> Result<String, String> {
        let bucket = object
            .object_ref
            .bucket
            .unwrap_or_else(|| self.config.bucket.clone());
        let claims = AttachmentObjectClaims {
            bucket,
            object_key: object.object_ref.object_key,
            name: object.object_ref.name.unwrap_or(object.file_name),
            mime_type: object.object_ref.mime_type.unwrap_or(object.content_type),
            exp: ATTACHMENT_OBJECT_TOKEN_EXP,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(self.config.token_secret.as_bytes()),
        )
        .map_err(|err| format!("sign attachment object url failed: {err}"))?;
        let path = format!(
            "/api/attachments/object?token={}",
            urlencoding::encode(token.as_str())
        );
        Ok(match self.config.public_base_url.as_deref() {
            Some(base_url) => format!("{base_url}{path}"),
            None => path,
        })
    }

    pub fn decode_signed_object(&self, token: &str) -> Result<SignedObject, String> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        let data = decode::<AttachmentObjectClaims>(
            token,
            &DecodingKey::from_secret(self.config.token_secret.as_bytes()),
            &validation,
        )
        .map_err(|err| format!("invalid attachment object token: {err}"))?;
        let claims = data.claims;
        Ok(SignedObject {
            object_ref: StoredObjectRef {
                bucket: Some(claims.bucket),
                object_key: claims.object_key,
                name: Some(claims.name.clone()),
                mime_type: Some(claims.mime_type.clone()),
            },
            content_type: claims.mime_type,
            file_name: claims.name,
        })
    }

    pub async fn get_object_bytes(
        &self,
        object_ref: &StoredObjectRef,
        max_bytes: Option<u64>,
    ) -> Result<StoredObjectBytes, String> {
        let bucket = object_ref
            .bucket
            .as_deref()
            .unwrap_or(self.config.bucket.as_str());
        let max_bytes = max_bytes.unwrap_or(self.config.max_read_bytes).max(1);
        let url =
            self.presigned_object_url(S3Method::Get, bucket, object_ref.object_key.as_str(), 300)?;
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|err| format!("read attachment object request failed: {err}"))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "read attachment object failed: status={} body={}",
                status,
                truncate_error_body(body.as_str())
            ));
        }
        if let Some(content_length) = response.content_length() {
            if content_length > max_bytes {
                return Err(format!(
                    "attachment object exceeds read limit: {} > {} bytes",
                    content_length, max_bytes
                ));
            }
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(ToOwned::to_owned);
        let bytes = response
            .bytes()
            .await
            .map_err(|err| format!("collect attachment object failed: {err}"))?;
        if bytes.len() as u64 > max_bytes {
            return Err(format!(
                "attachment object exceeds read limit: {} > {} bytes",
                bytes.len(),
                max_bytes
            ));
        }

        Ok(StoredObjectBytes {
            content_length: bytes.len() as u64,
            bytes,
            content_type,
        })
    }

    fn presigned_object_url(
        &self,
        method: S3Method,
        bucket: &str,
        object_key: &str,
        expires_seconds: u64,
    ) -> Result<String, String> {
        let endpoint = Url::parse(self.config.endpoint.as_str()).map_err(|err| format!("{err}"))?;
        let host = endpoint
            .host_str()
            .ok_or_else(|| "object storage endpoint is missing host".to_string())?;
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
            encode_path_segment(bucket),
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
        let signed_query = canonical_query_string(&signed_pairs);
        Ok(format!(
            "{}{}?{}",
            self.config.endpoint, canonical_uri, signed_query
        ))
    }
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
        .map(|(key, value)| (encode_query_component(key), encode_query_component(value)))
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
    aws_percent_encode(value, false)
}

fn encode_query_component(value: &str) -> String {
    aws_percent_encode(value, true)
}

fn aws_percent_encode(value: &str, encode_slash: bool) -> String {
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

fn build_object_key(user_id: &str, conversation_id: &str, id: &str, name: &str) -> String {
    let extension = std::path::Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .map(sanitize_key_segment)
        .filter(|value| !value.is_empty());
    let suffix = extension
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    format!(
        "chatos/users/{}/sessions/{}/{}{}",
        sanitize_key_segment(user_id),
        sanitize_key_segment(conversation_id),
        id,
        suffix
    )
}

fn sanitize_key_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        .collect::<String>();
    if sanitized.is_empty() {
        URL_SAFE_NO_PAD.encode(value.as_bytes())
    } else {
        sanitized
    }
}

fn truncate_error_body(value: &str) -> String {
    value.chars().take(400).collect()
}

fn read_optional_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_u64_env(key: &str) -> Option<u64> {
    read_optional_env(key).and_then(|value| value.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_service() -> ObjectStorageService {
        ObjectStorageService {
            http: Client::new(),
            config: ObjectStorageConfig {
                endpoint: "http://127.0.0.1:9000".to_string(),
                region: DEFAULT_REGION.to_string(),
                bucket: DEFAULT_BUCKET.to_string(),
                access_key: "test-access-key".to_string(),
                secret_key: "test-secret-key".to_string(),
                presign_expires_seconds: DEFAULT_PRESIGN_EXPIRES_SECONDS,
                public_base_url: None,
                token_secret: "test-token-secret".to_string(),
                max_upload_bytes: DEFAULT_MAX_UPLOAD_BYTES,
                max_read_bytes: DEFAULT_MAX_READ_BYTES,
            },
        }
    }

    #[test]
    fn signed_object_url_round_trips_without_crypto_provider_panic() {
        let _ = jsonwebtoken::crypto::rust_crypto::DEFAULT_PROVIDER.install_default();
        let service = test_service();
        let url = service
            .signed_object_url(SignedObject {
                object_ref: StoredObjectRef {
                    bucket: None,
                    object_key: "chatos/users/user/sessions/session/file.txt".to_string(),
                    name: Some("file.txt".to_string()),
                    mime_type: Some("text/plain".to_string()),
                },
                content_type: "application/octet-stream".to_string(),
                file_name: "attachment".to_string(),
            })
            .expect("attachment URL should be signed");
        let encoded_token = url
            .split_once("?token=")
            .map(|(_, token)| token)
            .expect("signed URL should contain a token");
        let token = urlencoding::decode(encoded_token).expect("token should be URL encoded");
        let decoded = service
            .decode_signed_object(token.as_ref())
            .expect("signed object token should decode");

        assert_eq!(decoded.object_ref.bucket.as_deref(), Some(DEFAULT_BUCKET));
        assert_eq!(decoded.object_ref.name.as_deref(), Some("file.txt"));
        assert_eq!(decoded.object_ref.mime_type.as_deref(), Some("text/plain"));
    }
}
