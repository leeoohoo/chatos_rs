// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use base64::Engine;
use chrono::Utc;
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{
    SandboxAuthContext, SCOPE_IMAGES_READ, SCOPE_IMAGES_WRITE, SCOPE_LEASE_READ, SCOPE_POOL_READ,
};
use crate::backend::SandboxBackendRef;
use crate::config::AppConfig;
use crate::error::ApiError;
use crate::models::{
    InitializeSandboxImageRequest, PoolStatusResponse, SandboxEventRecord, SandboxHealthCheck,
    SandboxHealthResponse, SandboxImageCatalogResponse, SandboxImageJobRecord, SandboxLeaseRecord,
    SandboxStatus, SystemConfigResponse, UpdatePoolConfigRequest,
};
use crate::pool::SandboxPoolRef;
use crate::store::SandboxStore;

use super::images;

mod access_clients;
mod lease_inputs;
mod leases;
mod mcp_proxy;

#[derive(Clone)]
pub struct SandboxManager {
    config: AppConfig,
    store: SandboxStore,
    backend: SandboxBackendRef,
    pool: SandboxPoolRef,
    image_jobs: images::ImageJobStore,
}

impl SandboxManager {
    pub async fn new(
        config: AppConfig,
        store: SandboxStore,
        backend: SandboxBackendRef,
        pool: SandboxPoolRef,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&config.work_root)
            .map_err(|err| format!("create sandbox work root failed: {err}"))?;
        Ok(Self {
            config,
            store,
            backend,
            pool,
            image_jobs: images::ImageJobStore::default(),
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub async fn health(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<SandboxHealthResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        let checked_at = now_rfc3339();

        let (backend_instance, backend_error) = match self
            .backend
            .inspect(sandbox_id, record.backend_id.as_deref())
            .await
        {
            Ok(instance) => (instance, None),
            Err(err) => (None, Some(err)),
        };
        let backend_id = backend_instance
            .as_ref()
            .and_then(|instance| instance.backend_id.clone())
            .or_else(|| record.backend_id.clone());
        let backend_alive = backend_instance.is_some();
        let backend_message = match (&backend_id, &backend_error) {
            (_, Some(err)) => format!("backend inspect failed: {err}"),
            (Some(id), None) => format!("backend instance found: {id}"),
            (None, None) => "backend instance was not found".to_string(),
        };

        let agent_endpoint = record.agent_endpoint.clone().or_else(|| {
            backend_instance
                .as_ref()
                .and_then(|instance| instance.agent_endpoint.clone())
        });
        let (agent_alive, agent_message) =
            mcp_proxy::check_agent_health(agent_endpoint.as_deref()).await;

        let lifecycle_ok = matches!(record.status, SandboxStatus::Ready | SandboxStatus::Running);
        let workspace_alive = std::fs::metadata(record.run_workspace.as_str())
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);

        let checks = vec![
            SandboxHealthCheck {
                name: "lifecycle_status".to_string(),
                ok: lifecycle_ok,
                message: if lifecycle_ok {
                    format!("sandbox status is {}", record.status.as_str())
                } else {
                    format!("sandbox status is not ready: {}", record.status.as_str())
                },
            },
            SandboxHealthCheck {
                name: "backend_instance".to_string(),
                ok: backend_alive,
                message: backend_message,
            },
            SandboxHealthCheck {
                name: "agent_health".to_string(),
                ok: agent_alive.unwrap_or(false),
                message: agent_message,
            },
            SandboxHealthCheck {
                name: "workspace_path".to_string(),
                ok: workspace_alive,
                message: if workspace_alive {
                    "run workspace exists".to_string()
                } else {
                    "run workspace does not exist".to_string()
                },
            },
        ];

        let ok = checks.iter().all(|check| check.ok);
        let message = if ok {
            "sandbox is healthy and ready for file and terminal operations".to_string()
        } else {
            let failed_checks = checks
                .iter()
                .filter(|check| !check.ok)
                .map(|check| check.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("sandbox health check failed: {failed_checks}")
        };

        let response = SandboxHealthResponse {
            ok,
            sandbox_id: record.sandbox_id.clone(),
            lease_id: record.id.clone(),
            status: record.status,
            backend: record.backend.clone(),
            backend_id,
            backend_alive,
            agent_endpoint,
            agent_alive,
            workspace_alive,
            checked_at,
            message,
            checks,
        };

        self.event(
            &record,
            "sandbox_health_checked",
            Some(response.message.as_str()),
            Some(json!({
                "ok": response.ok,
                "backend_alive": response.backend_alive,
                "agent_alive": response.agent_alive,
                "workspace_alive": response.workspace_alive,
            })),
        )
        .await;

        Ok(response)
    }

    pub async fn sandbox_images(
        &self,
        auth: &SandboxAuthContext,
    ) -> Result<SandboxImageCatalogResponse, ApiError> {
        auth.require_scope(SCOPE_IMAGES_READ)?;
        Ok(images::catalog(&self.config, self.config.backend).await)
    }

    pub async fn sandbox_image_jobs(
        &self,
        auth: &SandboxAuthContext,
    ) -> Result<Vec<SandboxImageJobRecord>, ApiError> {
        auth.require_scope(SCOPE_IMAGES_READ)?;
        Ok(self.image_jobs.list().await)
    }

    pub async fn initialize_sandbox_image(
        &self,
        auth: &SandboxAuthContext,
        input: InitializeSandboxImageRequest,
    ) -> Result<SandboxImageJobRecord, ApiError> {
        auth.require_scope(SCOPE_IMAGES_WRITE)?;
        images::start_initialize_job(
            self.image_jobs.clone(),
            &self.config,
            self.config.backend,
            &input.features,
            input.custom_build_script.as_deref(),
        )
        .await
        .map_err(ApiError::bad_request)
    }

    pub async fn pool_status(
        &self,
        auth: &SandboxAuthContext,
    ) -> Result<PoolStatusResponse, ApiError> {
        auth.require_scope(SCOPE_POOL_READ)?;
        let active = self
            .store
            .active_capacity_count(self.pool.max_active())
            .await
            .map_err(ApiError::internal)?;
        let now = now_rfc3339();
        let pending = self
            .store
            .count_pending_leases(now.as_str())
            .await
            .map_err(ApiError::internal)?;
        Ok(PoolStatusResponse {
            backend: self.backend.kind().to_string(),
            max_active: self.pool.max_active(),
            active,
            max_pending: self.pool.max_pending(),
            pending,
            lease_ttl_seconds: self.config.lease_ttl.as_secs(),
            cleanup_interval_seconds: self.config.cleanup_interval.as_secs(),
        })
    }

    pub async fn update_pool_config(
        &self,
        auth: &SandboxAuthContext,
        input: UpdatePoolConfigRequest,
    ) -> Result<PoolStatusResponse, ApiError> {
        auth.require_admin()?;
        let max_active = input
            .max_active
            .unwrap_or_else(|| self.pool.max_active())
            .max(1);
        let max_pending = input.max_pending.unwrap_or_else(|| self.pool.max_pending());
        self.pool.set_limits(max_active, max_pending);
        if let Err(err) = self.promote_pending_leases().await {
            tracing::warn!(
                "promote pending sandboxes after pool config update failed: {}",
                err
            );
        }
        self.pool_status(auth).await
    }

    pub fn system_config(
        &self,
        auth: &SandboxAuthContext,
    ) -> Result<SystemConfigResponse, ApiError> {
        auth.require_admin()?;
        Ok(SystemConfigResponse {
            host: self.config.host.to_string(),
            port: self.config.port,
            backend: self.backend.kind().to_string(),
            work_root: self.config.work_root.to_string_lossy().to_string(),
            pool_max_active: self.pool.max_active(),
            pool_max_pending: self.pool.max_pending(),
            lease_ttl_seconds: self.config.lease_ttl.as_secs(),
            cleanup_interval_seconds: self.config.cleanup_interval.as_secs(),
            agent_port: self.config.agent_port,
            docker_image: self.config.docker_image.clone(),
            docker_network_mode: self.config.docker_network_mode.clone(),
            kata_container_cli: self.config.kata_container_cli.clone(),
            kata_runtime: self.config.kata_runtime.clone(),
            kata_image: self.config.kata_image.clone(),
            kata_network_mode: self.config.kata_network_mode.clone(),
            image_tag_prefix: self.config.image_tag_prefix.clone(),
            image_build_context: self
                .config
                .image_build_context
                .to_string_lossy()
                .to_string(),
            image_dockerfile: self.config.image_dockerfile.to_string_lossy().to_string(),
        })
    }

    fn agent_token_for_record(&self, record: &SandboxLeaseRecord) -> String {
        record
            .agent_token_nonce
            .as_deref()
            .map(|nonce| self.agent_token(record.id.as_str(), nonce))
            .unwrap_or_else(|| record.id.clone())
    }

    fn agent_token(&self, lease_id: &str, nonce: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.config.agent_token_secret.as_bytes());
        hasher.update(b":");
        hasher.update(lease_id.as_bytes());
        hasher.update(b":");
        hasher.update(nonce.as_bytes());
        let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(hasher.finalize());
        format!("sat_{lease_id}_{nonce}_{signature}")
    }

    async fn event(
        &self,
        record: &SandboxLeaseRecord,
        event_type: &str,
        message: Option<&str>,
        payload: Option<serde_json::Value>,
    ) {
        let event = SandboxEventRecord {
            id: prefixed_id("event"),
            sandbox_id: record.sandbox_id.clone(),
            lease_id: record.id.clone(),
            event_type: event_type.to_string(),
            message: message.map(ToOwned::to_owned),
            payload,
            created_at: now_rfc3339(),
        };
        if let Err(err) = self.store.append_event(&event).await {
            tracing::warn!("append sandbox event failed: {}", err);
        }
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn prefixed_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
}
