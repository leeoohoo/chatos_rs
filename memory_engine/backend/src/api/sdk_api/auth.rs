// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;

use axum::{
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::models::StoredEngineSource;
use crate::repositories::sources;
use crate::state::AppState;

use super::internal_error;

#[derive(Debug, Clone)]
pub struct SdkAuthContext {
    pub source: StoredEngineSource,
}

impl SdkAuthContext {
    pub fn source_id(&self) -> &str {
        self.source.source_id.as_str()
    }

    pub fn tenant_id(&self) -> Option<&str> {
        self.source.tenant_id.as_deref()
    }

    pub fn require_tenant(&self, tenant_id: &str) -> Result<(), (StatusCode, String)> {
        let normalized = tenant_id.trim();
        if normalized.is_empty() {
            return Err((StatusCode::BAD_REQUEST, "tenant_id is required".to_string()));
        }

        match self.tenant_id() {
            Some(expected) if expected == normalized => Ok(()),
            Some(expected) => Err((
                StatusCode::FORBIDDEN,
                format!(
                    "tenant_id {} does not match authenticated source tenant {}",
                    normalized, expected
                ),
            )),
            None => Err((
                StatusCode::FORBIDDEN,
                "authenticated source is not bound to a tenant".to_string(),
            )),
        }
    }

    pub fn require_optional_tenant<'a>(
        &'a self,
        tenant_id: Option<&'a str>,
    ) -> Result<Option<&'a str>, (StatusCode, String)> {
        match tenant_id {
            Some(value) => {
                self.require_tenant(value)?;
                Ok(Some(value.trim()))
            }
            None => match self.tenant_id() {
                Some(value) => Ok(Some(value)),
                None => Ok(None),
            },
        }
    }
}

impl FromRequestParts<Arc<AppState>> for SdkAuthContext {
    type Rejection = (StatusCode, String);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        let system_id = parts
            .headers
            .get("x-memory-system-id")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let system_key = parts
            .headers
            .get("x-memory-system-key")
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let Some(system_id) = system_id else {
            return Err((
                StatusCode::UNAUTHORIZED,
                "missing x-memory-system-id".to_string(),
            ));
        };
        let Some(system_key) = system_key else {
            return Err((
                StatusCode::UNAUTHORIZED,
                "missing x-memory-system-key".to_string(),
            ));
        };

        let Some(source) =
            sources::verify_source_secret(&state.pool, system_id.as_str(), system_key.as_str())
                .await
                .map_err(internal_error)?
        else {
            return Err((
                StatusCode::UNAUTHORIZED,
                "invalid system credentials".to_string(),
            ));
        };

        Ok(Self { source })
    }
}

#[derive(Debug, Deserialize)]
pub struct SdkTenantQuery {
    pub tenant_id: Option<String>,
    pub thread_id: Option<String>,
}

pub async fn auth_status(
    auth: SdkAuthContext,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    Ok(Json(json!({
        "source_id": auth.source.source_id,
        "tenant_id": auth.source.tenant_id,
        "source_type": auth.source.source_type,
        "name": auth.source.name,
        "status": auth.source.status,
        "sdk_enabled": auth.source.sdk_enabled,
        "secret_key_hint": auth.source.secret_key_hint,
        "key_last_rotated_at": auth.source.key_last_rotated_at,
    })))
}

#[cfg(test)]
mod tests {
    use super::SdkAuthContext;
    use crate::models::StoredEngineSource;

    fn auth_context(tenant_id: Option<&str>) -> SdkAuthContext {
        SdkAuthContext {
            source: StoredEngineSource {
                id: "src_1".to_string(),
                tenant_id: tenant_id.map(ToOwned::to_owned),
                source_id: "source_1".to_string(),
                source_type: "sdk_system".to_string(),
                name: "Test Source".to_string(),
                description: None,
                config: None,
                status: "active".to_string(),
                sdk_enabled: true,
                secret_key_hint: None,
                key_last_rotated_at: None,
                secret_key_hash: None,
                created_at: "2026-05-20T00:00:00Z".to_string(),
                updated_at: "2026-05-20T00:00:00Z".to_string(),
            },
        }
    }

    #[test]
    fn require_tenant_accepts_matching_tenant() {
        let auth = auth_context(Some("tenant_a"));
        assert!(auth.require_tenant("tenant_a").is_ok());
    }

    #[test]
    fn require_tenant_rejects_mismatched_tenant() {
        let auth = auth_context(Some("tenant_a"));
        let err = auth.require_tenant("tenant_b").unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::FORBIDDEN);
    }

    #[test]
    fn require_optional_tenant_falls_back_to_authenticated_tenant() {
        let auth = auth_context(Some("tenant_a"));
        let tenant = auth.require_optional_tenant(None).unwrap();
        assert_eq!(tenant, Some("tenant_a"));
    }
}
