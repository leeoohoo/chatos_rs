// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::Engine;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{
    SandboxAuthContext, SandboxSystemClient, SCOPE_IMAGES_READ, SCOPE_LEASE_CREATE,
    SCOPE_LEASE_READ, SCOPE_LEASE_RELEASE, SCOPE_MCP_CALL, SCOPE_MCP_TOOLS, SCOPE_POOL_READ,
};
use crate::error::ApiError;
use crate::models::{
    CreateSandboxAccessClientRequest, CreateSandboxAccessClientResponse,
    DeleteSandboxAccessClientResponse, RotateSandboxAccessClientKeyResponse,
    SandboxAccessClientRecord, SandboxAccessClientResponse, UpdateSandboxAccessClientRequest,
};

use super::{now_rfc3339, prefixed_id, SandboxManager};

impl SandboxManager {
    pub async fn authenticate_access_client(
        &self,
        client_id: &str,
        client_key: &str,
    ) -> Result<Option<SandboxSystemClient>, ApiError> {
        let Some(record) = self
            .store
            .get_access_client_by_client_id(client_id.trim())
            .await
            .map_err(ApiError::internal)?
        else {
            return Ok(None);
        };
        if !record.enabled {
            return Err(ApiError::unauthorized("sandbox access client is disabled"));
        }
        let provided_hash =
            access_client_key_hash(client_key, self.config.agent_token_secret.as_str());
        if !constant_time_equal(record.key_hash.as_str(), provided_hash.as_str()) {
            return Err(ApiError::unauthorized("invalid sandbox system credentials"));
        }
        let now = now_rfc3339();
        let _ = self
            .store
            .mark_access_client_used(record.id.as_str(), now.as_str())
            .await;
        Ok(Some(SandboxSystemClient {
            client_id: record.client_id,
            scopes: record.scopes,
            allowed_tenant_ids: record.allowed_tenant_ids,
            allowed_project_ids: record.allowed_project_ids,
            allowed_tools: record.allowed_tools,
            max_lease_ttl_seconds: record.max_lease_ttl_seconds,
        }))
    }

    pub async fn list_access_clients(
        &self,
        auth: &SandboxAuthContext,
    ) -> Result<Vec<SandboxAccessClientResponse>, ApiError> {
        auth.require_admin()?;
        let clients = self
            .store
            .list_access_clients()
            .await
            .map_err(ApiError::internal)?;
        Ok(clients.into_iter().map(access_client_response).collect())
    }

    pub async fn create_access_client(
        &self,
        auth: &SandboxAuthContext,
        input: CreateSandboxAccessClientRequest,
    ) -> Result<CreateSandboxAccessClientResponse, ApiError> {
        auth.require_admin()?;
        let name = normalize_required_text("name", input.name)?;
        let client_id = input
            .client_id
            .and_then(|value| normalize_optional_text(value.as_str()))
            .unwrap_or_else(|| format!("sandbox_client_{}", Uuid::new_v4().simple()));
        let client_key = generate_access_client_key();
        let now = now_rfc3339();
        let record = SandboxAccessClientRecord {
            id: prefixed_id("sandbox_access_client"),
            name,
            client_id,
            key_hash: access_client_key_hash(
                client_key.as_str(),
                self.config.agent_token_secret.as_str(),
            ),
            enabled: true,
            scopes: normalize_list_or_default(input.scopes, default_access_client_scopes()),
            allowed_tenant_ids: normalize_list_or_default(input.allowed_tenant_ids, &["*"]),
            allowed_project_ids: normalize_list_or_default(input.allowed_project_ids, &["*"]),
            allowed_tools: normalize_list_or_default(input.allowed_tools, &["*"]),
            max_lease_ttl_seconds: input
                .max_lease_ttl_seconds
                .unwrap_or(self.config.lease_ttl.as_secs())
                .max(60),
            created_at: now.clone(),
            updated_at: now,
            last_used_at: None,
        };
        self.store
            .create_access_client(&record)
            .await
            .map_err(ApiError::internal)?;
        Ok(CreateSandboxAccessClientResponse {
            client: access_client_response(record),
            client_key,
        })
    }

    pub async fn update_access_client(
        &self,
        auth: &SandboxAuthContext,
        id: &str,
        input: UpdateSandboxAccessClientRequest,
    ) -> Result<SandboxAccessClientResponse, ApiError> {
        auth.require_admin()?;
        let mut record = self
            .store
            .get_access_client_by_id(id)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("sandbox access client not found: {id}")))?;
        if let Some(name) = input.name {
            record.name = normalize_required_text("name", name)?;
        }
        if let Some(enabled) = input.enabled {
            record.enabled = enabled;
        }
        if let Some(scopes) = input.scopes {
            record.scopes = normalize_list_or_default(scopes, default_access_client_scopes());
        }
        if let Some(values) = input.allowed_tenant_ids {
            record.allowed_tenant_ids = normalize_list_or_default(values, &["*"]);
        }
        if let Some(values) = input.allowed_project_ids {
            record.allowed_project_ids = normalize_list_or_default(values, &["*"]);
        }
        if let Some(values) = input.allowed_tools {
            record.allowed_tools = normalize_list_or_default(values, &["*"]);
        }
        if let Some(ttl) = input.max_lease_ttl_seconds {
            record.max_lease_ttl_seconds = ttl.max(60);
        }
        record.updated_at = now_rfc3339();
        self.store
            .replace_access_client(&record)
            .await
            .map_err(ApiError::internal)?;
        Ok(access_client_response(record))
    }

    pub async fn rotate_access_client_key(
        &self,
        auth: &SandboxAuthContext,
        id: &str,
    ) -> Result<RotateSandboxAccessClientKeyResponse, ApiError> {
        auth.require_admin()?;
        let mut record = self
            .store
            .get_access_client_by_id(id)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("sandbox access client not found: {id}")))?;
        let client_key = generate_access_client_key();
        record.key_hash =
            access_client_key_hash(client_key.as_str(), self.config.agent_token_secret.as_str());
        record.updated_at = now_rfc3339();
        self.store
            .replace_access_client(&record)
            .await
            .map_err(ApiError::internal)?;
        Ok(RotateSandboxAccessClientKeyResponse {
            client: access_client_response(record),
            client_key,
        })
    }

    pub async fn delete_access_client(
        &self,
        auth: &SandboxAuthContext,
        id: &str,
    ) -> Result<DeleteSandboxAccessClientResponse, ApiError> {
        auth.require_admin()?;
        let deleted = self
            .store
            .delete_access_client(id)
            .await
            .map_err(ApiError::internal)?;
        if !deleted {
            return Err(ApiError::not_found(format!(
                "sandbox access client not found: {id}"
            )));
        }
        Ok(DeleteSandboxAccessClientResponse { ok: true })
    }
}

fn access_client_response(record: SandboxAccessClientRecord) -> SandboxAccessClientResponse {
    SandboxAccessClientResponse {
        id: record.id,
        name: record.name,
        client_id: record.client_id,
        enabled: record.enabled,
        scopes: record.scopes,
        allowed_tenant_ids: record.allowed_tenant_ids,
        allowed_project_ids: record.allowed_project_ids,
        allowed_tools: record.allowed_tools,
        max_lease_ttl_seconds: record.max_lease_ttl_seconds,
        created_at: record.created_at,
        updated_at: record.updated_at,
        last_used_at: record.last_used_at,
    }
}

fn generate_access_client_key() -> String {
    format!(
        "sbk_{}_{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn access_client_key_hash(value: &str, secret: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret.as_bytes());
    hasher.update([0]);
    hasher.update(value.trim().as_bytes());
    base64::engine::general_purpose::STANDARD.encode(hasher.finalize())
}

fn default_access_client_scopes() -> &'static [&'static str] {
    &[
        SCOPE_LEASE_CREATE,
        SCOPE_LEASE_READ,
        SCOPE_LEASE_RELEASE,
        SCOPE_MCP_TOOLS,
        SCOPE_MCP_CALL,
        SCOPE_POOL_READ,
        SCOPE_IMAGES_READ,
    ]
}

fn normalize_required_text(name: &str, value: String) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::bad_request(format!("{name} is required")));
    }
    Ok(value.to_string())
}

fn normalize_optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_list_or_default(values: Vec<String>, default_values: &[&str]) -> Vec<String> {
    let mut out = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() || out.iter().any(|item: &String| item == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    if out.is_empty() {
        default_values
            .iter()
            .map(|value| value.to_string())
            .collect()
    } else {
        out
    }
}

fn constant_time_equal(expected: &str, provided: &str) -> bool {
    let expected = expected.as_bytes();
    let provided = provided.as_bytes();
    if expected.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (left, right) in expected.iter().zip(provided.iter()) {
        diff |= left ^ right;
    }
    diff == 0
}
