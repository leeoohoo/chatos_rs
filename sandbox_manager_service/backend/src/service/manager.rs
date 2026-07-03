// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::http::StatusCode;
use base64::Engine;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::auth::{
    SandboxAuthContext, SandboxSystemClient, SCOPE_IMAGES_READ, SCOPE_IMAGES_WRITE,
    SCOPE_LEASE_CREATE, SCOPE_LEASE_DESTROY, SCOPE_LEASE_READ, SCOPE_LEASE_RELEASE, SCOPE_MCP_CALL,
    SCOPE_MCP_TOOLS, SCOPE_POOL_READ,
};
use crate::backend::{SandboxBackendRef, SandboxCreateSpec};
use crate::config::AppConfig;
use crate::error::ApiError;
use crate::models::{
    CreateSandboxAccessClientRequest, CreateSandboxAccessClientResponse, CreateSandboxLeaseRequest,
    CreateSandboxLeaseResponse, DeleteSandboxAccessClientResponse, DestroySandboxResponse,
    HeartbeatRequest, HeartbeatResponse, InitializeSandboxImageRequest, ListSandboxQuery,
    PoolStatusResponse, ReleaseSandboxRequest, ReleaseSandboxResponse,
    RotateSandboxAccessClientKeyResponse, SandboxAccessClientRecord, SandboxAccessClientResponse,
    SandboxEventRecord, SandboxHealthCheck, SandboxHealthResponse, SandboxImageCatalogResponse,
    SandboxImageJobRecord, SandboxLeaseRecord, SandboxMcpCallRequest, SandboxMcpCallResponse,
    SandboxMcpToolsResponse, SandboxStatus, SystemConfigResponse, UpdateSandboxAccessClientRequest,
};
use crate::pool::SandboxPoolRef;
use crate::store::{is_duplicate_key_error, SandboxStore};

use super::images;

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

    pub async fn create_lease(
        &self,
        auth: &SandboxAuthContext,
        input: CreateSandboxLeaseRequest,
        idempotency_key: Option<String>,
    ) -> Result<CreateSandboxLeaseResponse, ApiError> {
        validate_required("tenant_id", &input.tenant_id)?;
        validate_required("user_id", &input.user_id)?;
        validate_required("project_id", &input.project_id)?;
        validate_required("run_id", &input.run_id)?;
        validate_required("workspace_root", &input.workspace_root)?;
        auth.ensure_create_lease_allowed(&input)?;
        let idempotency_key = normalize_idempotency_key(idempotency_key)?;
        let tenant_id = input.tenant_id.trim().to_string();
        let project_id = input.project_id.trim().to_string();
        let run_id = input.run_id.trim().to_string();
        if let Some(key) = idempotency_key.as_deref() {
            if let Some(existing) = self
                .store
                .get_by_idempotency_key(
                    tenant_id.as_str(),
                    project_id.as_str(),
                    run_id.as_str(),
                    key,
                )
                .await
                .map_err(ApiError::internal)?
            {
                return self.create_lease_response_from_existing(existing);
            }
        }

        let lease_id = prefixed_id("lease");
        let sandbox_id = prefixed_id("sandbox");
        let agent_token_nonce = Uuid::new_v4().simple().to_string();
        let agent_token = self.agent_token(lease_id.as_str(), agent_token_nonce.as_str());
        let now = now_rfc3339();
        let ttl = Duration::from_secs(input.ttl_seconds.unwrap_or(self.config.lease_ttl.as_secs()));
        let expires_at = (Utc::now()
            + ChronoDuration::from_std(ttl).unwrap_or_else(|_| ChronoDuration::seconds(7_200)))
        .to_rfc3339();
        let run_workspace =
            self.prepare_run_workspace(input.workspace_root.as_str(), run_id.as_str())?;
        let resource_limits = input.resource_limits.unwrap_or_default();
        let network = input.network.unwrap_or_default();
        let requested_image_id = input.image_id.clone();
        let image = images::resolve_for_create(
            &self.config,
            self.config.backend,
            requested_image_id.as_deref(),
        )
        .await
        .map_err(ApiError::bad_request)?;
        let tools = if input.tools.is_empty() {
            vec!["filesystem".to_string(), "terminal".to_string()]
        } else {
            input.tools
        };
        for tool in &tools {
            auth.ensure_tool_allowed(tool)?;
        }
        let capacity_claim_until = (Utc::now() + ChronoDuration::minutes(5)).to_rfc3339();
        let acquired_capacity = self
            .store
            .try_acquire_active_slot(
                self.config.pool_max_active,
                lease_id.as_str(),
                sandbox_id.as_str(),
                capacity_claim_until.as_str(),
            )
            .await
            .map_err(ApiError::internal)?;
        if !acquired_capacity {
            return Err(ApiError::capacity(format!(
                "sandbox global pool is full: max_active={}",
                self.config.pool_max_active
            )));
        }

        let mut record = SandboxLeaseRecord {
            id: lease_id.clone(),
            sandbox_id: sandbox_id.clone(),
            tenant_id: tenant_id.clone(),
            user_id: input.user_id.trim().to_string(),
            project_id: project_id.clone(),
            run_id: run_id.clone(),
            workspace_root: input.workspace_root.trim().to_string(),
            run_workspace: run_workspace.to_string_lossy().to_string(),
            backend: self.backend.kind().to_string(),
            backend_id: None,
            image_id: Some(image.id.clone()),
            image_ref: Some(image.image_ref.clone()),
            status: SandboxStatus::Leasing,
            agent_endpoint: None,
            resource_limits: resource_limits.clone(),
            network: network.clone(),
            tools,
            agent_token_nonce: Some(agent_token_nonce),
            idempotency_key: idempotency_key.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at,
            destroyed_at: None,
            last_error: None,
        };
        if let Err(err) = self.store.create_lease(&record).await {
            let _ = self.store.release_active_slot(lease_id.as_str()).await;
            if idempotency_key.is_some() && is_duplicate_key_error(&err) {
                if let Some(existing) = self
                    .store
                    .get_by_idempotency_key(
                        tenant_id.as_str(),
                        project_id.as_str(),
                        run_id.as_str(),
                        idempotency_key.as_deref().unwrap_or_default(),
                    )
                    .await
                    .map_err(ApiError::internal)?
                {
                    return self.create_lease_response_from_existing(existing);
                }
            }
            return Err(ApiError::internal(err));
        }
        if let Err(err) = self
            .store
            .extend_active_slot(lease_id.as_str(), record.expires_at.as_str())
            .await
        {
            record.status = SandboxStatus::Failed;
            record.last_error = Some(err.clone());
            record.idempotency_key = None;
            record.updated_at = now_rfc3339();
            let _ = self.store.replace_lease(&record).await;
            let _ = self.store.release_active_slot(lease_id.as_str()).await;
            return Err(ApiError::internal(err));
        }
        self.event(
            &record,
            "lease_created",
            Some("sandbox lease created"),
            Some(json!({
                "backend": self.backend.kind(),
                "image_id": image.id,
                "image_ref": image.image_ref,
            })),
        )
        .await;

        let create_result = self
            .backend
            .create(SandboxCreateSpec {
                sandbox_id: sandbox_id.clone(),
                run_workspace: record.run_workspace.clone(),
                image: record.image_ref.clone().unwrap_or_default(),
                agent_token: Some(agent_token.clone()),
                resource_limits,
                network,
            })
            .await;

        match create_result {
            Ok(instance) => {
                if let Err(err) = self.backend.start(sandbox_id.as_str()).await {
                    record.status = SandboxStatus::Failed;
                    record.last_error = Some(err.clone());
                    record.idempotency_key = None;
                    record.updated_at = now_rfc3339();
                    let _ = self.store.replace_lease(&record).await;
                    let _ = self.store.release_active_slot(lease_id.as_str()).await;
                    self.event(&record, "sandbox_start_failed", Some(&err), None)
                        .await;
                    return Err(ApiError::with_code(
                        StatusCode::BAD_GATEWAY,
                        "sandbox_create_failed",
                        err,
                    ));
                }
                record.status = SandboxStatus::Ready;
                record.backend_id = instance.backend_id.clone();
                record.agent_endpoint = instance.agent_endpoint;
                record.updated_at = now_rfc3339();
                self.store
                    .replace_lease(&record)
                    .await
                    .map_err(ApiError::internal)?;
                self.event(
                    &record,
                    "sandbox_ready",
                    Some("sandbox is ready"),
                    Some(json!({ "backend_id": instance.backend_id })),
                )
                .await;
                Ok(CreateSandboxLeaseResponse {
                    lease_id,
                    sandbox_id,
                    backend_id: record.backend_id,
                    image_id: record.image_id,
                    image_ref: record.image_ref,
                    status: record.status,
                    agent_endpoint: record.agent_endpoint,
                    agent_token,
                    run_workspace: record.run_workspace,
                    expires_at: record.expires_at,
                })
            }
            Err(err) => {
                record.status = SandboxStatus::Failed;
                record.last_error = Some(err.clone());
                record.idempotency_key = None;
                record.updated_at = now_rfc3339();
                let _ = self.store.replace_lease(&record).await;
                let _ = self.store.release_active_slot(lease_id.as_str()).await;
                self.event(&record, "sandbox_create_failed", Some(&err), None)
                    .await;
                Err(ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_create_failed",
                    err,
                ))
            }
        }
    }

    fn create_lease_response_from_existing(
        &self,
        record: SandboxLeaseRecord,
    ) -> Result<CreateSandboxLeaseResponse, ApiError> {
        if !matches!(record.status, SandboxStatus::Ready | SandboxStatus::Running) {
            return Err(ApiError::with_code(
                StatusCode::CONFLICT,
                "sandbox_lease_idempotency_in_progress",
                format!(
                    "sandbox lease for idempotency key is not ready yet: status={}",
                    record.status.as_str()
                ),
            ));
        }
        Ok(CreateSandboxLeaseResponse {
            lease_id: record.id.clone(),
            sandbox_id: record.sandbox_id.clone(),
            backend_id: record.backend_id.clone(),
            image_id: record.image_id.clone(),
            image_ref: record.image_ref.clone(),
            status: record.status,
            agent_endpoint: record.agent_endpoint.clone(),
            agent_token: self.agent_token_for_record(&record),
            run_workspace: record.run_workspace,
            expires_at: record.expires_at,
        })
    }

    pub async fn heartbeat(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        input: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ApiError> {
        let mut record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match sandbox"));
        }
        if record.run_id != input.run_id {
            return Err(ApiError::bad_request("run_id does not match sandbox"));
        }
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(&record, "heartbeat", Some("sandbox heartbeat"), None)
            .await;
        Ok(HeartbeatResponse {
            ok: true,
            status: record.status,
            expires_at: record.expires_at,
        })
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
        let (agent_alive, agent_message) = check_agent_health(agent_endpoint.as_deref()).await;

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

    pub async fn mcp_tools(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<SandboxMcpToolsResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_MCP_TOOLS)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        let result = jsonrpc_agent_call(
            agent_endpoint.as_str(),
            Some(agent_token.as_str()),
            "tools/list",
            json!({}),
        )
        .await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_mcp_invalid_response",
                    "sandbox MCP tools/list response did not contain tools",
                )
            })?;
        Ok(SandboxMcpToolsResponse {
            ok: true,
            sandbox_id: record.sandbox_id,
            agent_endpoint,
            tools,
        })
    }

    pub async fn mcp_call(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        input: SandboxMcpCallRequest,
    ) -> Result<SandboxMcpCallResponse, ApiError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("tool name is required"));
        }
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_MCP_CALL)?;
        auth.ensure_tool_allowed(name)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        let result = jsonrpc_agent_call(
            agent_endpoint.as_str(),
            Some(agent_token.as_str()),
            "tools/call",
            json!({ "name": name, "arguments": input.arguments }),
        )
        .await?;
        Ok(SandboxMcpCallResponse {
            ok: true,
            sandbox_id: record.sandbox_id,
            agent_endpoint,
            result,
        })
    }

    pub async fn mcp_proxy(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        authorize_mcp_proxy_payload(auth, &record, &payload)?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let agent_token = self.agent_token_for_record(&record);
        jsonrpc_agent_proxy(agent_endpoint.as_str(), Some(agent_token.as_str()), payload).await
    }

    pub async fn release(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
        input: ReleaseSandboxRequest,
    ) -> Result<ReleaseSandboxResponse, ApiError> {
        let mut record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_RELEASE)?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match sandbox"));
        }
        record.status = SandboxStatus::Releasing;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "sandbox_releasing",
            Some("sandbox release started"),
            Some(json!({ "export_result": input.export_result, "destroy": input.destroy })),
        )
        .await;

        let output_workspace = if input.export_result {
            Some(self.prepare_output_workspace(&record)?)
        } else {
            None
        };

        if input.destroy {
            self.destroy_record(record.clone(), "sandbox_released")
                .await?;
            Ok(ReleaseSandboxResponse {
                ok: true,
                status: SandboxStatus::Destroyed,
                output_workspace: output_workspace.map(|path| path.to_string_lossy().to_string()),
                diff_summary: None,
            })
        } else {
            record.status = SandboxStatus::Ready;
            record.updated_at = now_rfc3339();
            self.store
                .replace_lease(&record)
                .await
                .map_err(ApiError::internal)?;
            Ok(ReleaseSandboxResponse {
                ok: true,
                status: record.status,
                output_workspace: output_workspace.map(|path| path.to_string_lossy().to_string()),
                diff_summary: None,
            })
        }
    }

    pub async fn destroy(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<DestroySandboxResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_DESTROY)?;
        self.destroy_record(record, "sandbox_destroyed").await?;
        Ok(DestroySandboxResponse {
            ok: true,
            status: SandboxStatus::Destroyed,
        })
    }

    pub async fn get(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<SandboxLeaseRecord, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        Ok(record)
    }

    pub async fn list(
        &self,
        auth: &SandboxAuthContext,
        query: ListSandboxQuery,
    ) -> Result<Vec<SandboxLeaseRecord>, ApiError> {
        let query = auth.scoped_list_query(query)?;
        self.store
            .list_leases(query)
            .await
            .map_err(ApiError::internal)
    }

    pub async fn events(
        &self,
        auth: &SandboxAuthContext,
        sandbox_id: &str,
    ) -> Result<Vec<SandboxEventRecord>, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        self.store
            .list_events(sandbox_id)
            .await
            .map_err(ApiError::internal)
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
            .active_capacity_count(self.config.pool_max_active)
            .await
            .map_err(ApiError::internal)?;
        Ok(PoolStatusResponse {
            backend: self.backend.kind().to_string(),
            max_active: self.pool.max_active(),
            active,
            max_pending: self.pool.max_pending(),
            pending: self.pool.pending(),
            lease_ttl_seconds: self.config.lease_ttl.as_secs(),
            cleanup_interval_seconds: self.config.cleanup_interval.as_secs(),
        })
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
            pool_max_active: self.config.pool_max_active,
            pool_max_pending: self.config.pool_max_pending,
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

    pub async fn cleanup_expired(&self) -> Result<(), String> {
        let now = now_rfc3339();
        let expired = self.store.list_expired_active(now.as_str(), 100).await?;
        for record in expired {
            let mut expired_record = record.clone();
            expired_record.status = SandboxStatus::Expired;
            expired_record.updated_at = now_rfc3339();
            expired_record.last_error = Some("lease expired".to_string());
            self.store.replace_lease(&expired_record).await?;
            self.event(
                &expired_record,
                "sandbox_expired",
                Some("sandbox lease expired"),
                None,
            )
            .await;
            if let Err(err) = self
                .destroy_record(expired_record, "sandbox_expired_destroyed")
                .await
            {
                tracing::warn!("destroy expired sandbox failed: {}", err.message);
            }
        }
        Ok(())
    }

    async fn require_sandbox(&self, sandbox_id: &str) -> Result<SandboxLeaseRecord, ApiError> {
        self.store
            .get_by_sandbox_id(sandbox_id)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("sandbox not found: {sandbox_id}")))
    }

    async fn agent_endpoint_for(&self, record: &SandboxLeaseRecord) -> Result<String, ApiError> {
        if let Some(endpoint) = record
            .agent_endpoint
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return validate_http_agent_endpoint(endpoint);
        }

        let inspected = self
            .backend
            .inspect(record.sandbox_id.as_str(), record.backend_id.as_deref())
            .await
            .map_err(|err| {
                ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_backend_inspect_failed",
                    err,
                )
            })?;
        let endpoint = inspected.and_then(|instance| instance.agent_endpoint);
        let endpoint = endpoint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("sandbox agent endpoint is not available"))?;
        validate_http_agent_endpoint(endpoint)
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

    async fn destroy_record(
        &self,
        mut record: SandboxLeaseRecord,
        event_type: &str,
    ) -> Result<(), ApiError> {
        record.status = SandboxStatus::Destroying;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "sandbox_destroying",
            Some("destroying sandbox"),
            None,
        )
        .await;

        if let Err(err) = self
            .backend
            .destroy(record.sandbox_id.as_str(), record.backend_id.as_deref())
            .await
        {
            record.status = SandboxStatus::Failed;
            record.last_error = Some(err.clone());
            record.updated_at = now_rfc3339();
            let _ = self.store.replace_lease(&record).await;
            self.event(&record, "sandbox_destroy_failed", Some(&err), None)
                .await;
            return Err(ApiError::with_code(
                StatusCode::BAD_GATEWAY,
                "sandbox_destroy_failed",
                err,
            ));
        }

        record.status = SandboxStatus::Destroyed;
        record.destroyed_at = Some(now_rfc3339());
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        let _ = self.store.release_active_slot(record.id.as_str()).await;
        self.event(&record, event_type, Some("sandbox destroyed"), None)
            .await;
        Ok(())
    }

    fn prepare_run_workspace(
        &self,
        workspace_root: &str,
        run_id: &str,
    ) -> Result<PathBuf, ApiError> {
        let root = PathBuf::from(workspace_root.trim());
        let base = if self.config.work_root.is_absolute() {
            self.config.work_root.clone()
        } else {
            root.join(&self.config.work_root)
        };
        let run_workspace = base
            .join("runs")
            .join(sanitize_path_segment(run_id))
            .join("input")
            .join("workspace");
        std::fs::create_dir_all(&run_workspace)
            .map_err(|err| ApiError::internal(format!("create run workspace failed: {err}")))?;
        Ok(run_workspace)
    }

    fn prepare_output_workspace(&self, record: &SandboxLeaseRecord) -> Result<PathBuf, ApiError> {
        let run_workspace = Path::new(record.run_workspace.as_str());
        let run_root = run_workspace
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| ApiError::internal("invalid run workspace path"))?;
        let output = run_root.join("output").join("workspace");
        std::fs::create_dir_all(&output)
            .map_err(|err| ApiError::internal(format!("create output workspace failed: {err}")))?;
        Ok(output)
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

fn validate_required(name: &'static str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::bad_request(format!("{name} is required")));
    }
    Ok(())
}

fn normalize_idempotency_key(value: Option<String>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(|value| value.trim().to_string()) else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(None);
    }
    if value.len() > 160 {
        return Err(ApiError::bad_request(
            "x-idempotency-key must be at most 160 bytes",
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(ApiError::bad_request(
            "x-idempotency-key must not contain control characters",
        ));
    }
    Ok(Some(value))
}

fn authorize_mcp_proxy_payload(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    payload: &Value,
) -> Result<(), ApiError> {
    match payload {
        Value::Object(_) => authorize_mcp_proxy_request(auth, record, payload),
        Value::Array(items) => {
            if items.is_empty() {
                return Err(ApiError::bad_request("MCP JSON-RPC batch is empty"));
            }
            for item in items {
                authorize_mcp_proxy_request(auth, record, item)?;
            }
            Ok(())
        }
        _ => Err(ApiError::bad_request(
            "MCP JSON-RPC payload must be an object or array",
        )),
    }
}

fn authorize_mcp_proxy_request(
    auth: &SandboxAuthContext,
    record: &SandboxLeaseRecord,
    payload: &Value,
) -> Result<(), ApiError> {
    let method = payload
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("MCP JSON-RPC method is required"))?;

    match method {
        "tools/list" => auth.ensure_lease_access(record, SCOPE_MCP_TOOLS),
        "tools/call" => {
            auth.ensure_lease_access(record, SCOPE_MCP_CALL)?;
            let tool_name = payload
                .get("params")
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ApiError::bad_request("tools/call.name is required"))?;
            auth.ensure_tool_allowed(tool_name)
        }
        _ => auth.ensure_lease_access(record, SCOPE_MCP_CALL),
    }
}

async fn check_agent_health(agent_endpoint: Option<&str>) -> (Option<bool>, String) {
    let Some(endpoint) = agent_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (None, "agent endpoint is not configured".to_string());
    };

    if endpoint.starts_with("mock://") {
        return (Some(true), "mock agent endpoint is reachable".to_string());
    }

    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return (
            Some(false),
            format!("unsupported agent endpoint scheme: {endpoint}"),
        );
    }

    let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return (
                Some(false),
                format!("build agent health client failed: {err}"),
            )
        }
    };

    match client.get(&health_url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                (
                    Some(true),
                    format!("agent health endpoint returned {status}"),
                )
            } else {
                (
                    Some(false),
                    format!("agent health endpoint returned {status}"),
                )
            }
        }
        Err(err) => (Some(false), format!("agent health request failed: {err}")),
    }
}

async fn jsonrpc_agent_call(
    agent_endpoint: &str,
    agent_token: Option<&str>,
    method: &str,
    params: Value,
) -> Result<Value, ApiError> {
    let url = format!("{}/mcp", agent_endpoint.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| ApiError::internal(format!("build MCP client failed: {err}")))?;
    let mut request = client.post(url.as_str());
    if let Some(agent_token) = agent_token.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(agent_token);
    }
    let response = request
        .json(&json!({
            "jsonrpc": "2.0",
            "id": prefixed_id("mcp"),
            "method": method,
            "params": params,
        }))
        .send()
        .await
        .map_err(|err| {
            ApiError::with_code(
                StatusCode::BAD_GATEWAY,
                "sandbox_mcp_request_failed",
                format!("{method} request failed: {err}"),
            )
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_response_failed",
            format!("{method} response read failed: {err}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_http_error",
            format!("{method} returned HTTP {status}: {}", preview_text(&body)),
        ));
    }
    let value: Value = serde_json::from_str(body.as_str()).map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_invalid_json",
            format!(
                "{method} returned invalid JSON: {err}; body={}",
                preview_text(&body)
            ),
        )
    })?;
    if let Some(error) = value.get("error") {
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_jsonrpc_error",
            format!(
                "{method} returned JSON-RPC error: {}",
                preview_text(&error.to_string())
            ),
        ));
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

async fn jsonrpc_agent_proxy(
    agent_endpoint: &str,
    agent_token: Option<&str>,
    payload: Value,
) -> Result<Value, ApiError> {
    let url = format!("{}/mcp", agent_endpoint.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| ApiError::internal(format!("build MCP proxy client failed: {err}")))?;
    let mut request = client.post(url.as_str());
    if let Some(agent_token) = agent_token.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(agent_token);
    }
    let response = request.json(&payload).send().await.map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_proxy_request_failed",
            format!("MCP proxy request failed: {err}"),
        )
    })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_proxy_response_failed",
            format!("MCP proxy response read failed: {err}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_proxy_http_error",
            format!("MCP proxy returned HTTP {status}: {}", preview_text(&body)),
        ));
    }
    serde_json::from_str(body.as_str()).map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_proxy_invalid_json",
            format!(
                "MCP proxy returned invalid JSON: {err}; body={}",
                preview_text(&body)
            ),
        )
    })
}

fn validate_http_agent_endpoint(endpoint: String) -> Result<String, ApiError> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Ok(endpoint)
    } else {
        Err(ApiError::bad_request(format!(
            "sandbox agent endpoint is not an HTTP endpoint: {endpoint}"
        )))
    }
}

fn preview_text(value: &str) -> String {
    const LIMIT: usize = 1200;
    if value.chars().count() <= LIMIT {
        return value.to_string();
    }
    value.chars().take(LIMIT).collect::<String>() + "...[truncated]"
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn prefixed_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
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

fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::SandboxSystemClient;
    use crate::models::{NetworkPolicy, ResourceLimits};

    fn lease_record() -> SandboxLeaseRecord {
        SandboxLeaseRecord {
            id: "lease-1".to_string(),
            sandbox_id: "sandbox-1".to_string(),
            tenant_id: "tenant-1".to_string(),
            user_id: "user-1".to_string(),
            project_id: "project-1".to_string(),
            run_id: "run-1".to_string(),
            workspace_root: "/tmp/workspace".to_string(),
            run_workspace: "/tmp/workspace/.chatos/task-runner/runs/run-1".to_string(),
            backend: "mock".to_string(),
            backend_id: Some("backend-1".to_string()),
            image_id: None,
            image_ref: None,
            status: SandboxStatus::Ready,
            agent_endpoint: Some("http://127.0.0.1:49888".to_string()),
            resource_limits: ResourceLimits::default(),
            network: NetworkPolicy::default(),
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            agent_token_nonce: Some("nonce-1".to_string()),
            idempotency_key: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            expires_at: "2026-01-01T01:00:00Z".to_string(),
            destroyed_at: None,
            last_error: None,
        }
    }

    fn system_auth(scopes: &[&str], tools: &[&str]) -> SandboxAuthContext {
        SandboxAuthContext::System(SandboxSystemClient {
            client_id: "task_runner".to_string(),
            scopes: scopes.iter().map(|value| value.to_string()).collect(),
            allowed_tenant_ids: vec!["tenant-1".to_string()],
            allowed_project_ids: vec!["project-1".to_string()],
            allowed_tools: tools.iter().map(|value| value.to_string()).collect(),
            max_lease_ttl_seconds: 3_600,
        })
    }

    #[test]
    fn mcp_proxy_authorizes_tools_list_with_tools_scope() {
        let auth = system_auth(&[SCOPE_MCP_TOOLS], &["sandbox_read_file"]);
        let payload = json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/list",
            "params": {}
        });

        assert!(authorize_mcp_proxy_payload(&auth, &lease_record(), &payload).is_ok());
    }

    #[test]
    fn mcp_proxy_enforces_tools_call_tool_policy() {
        let auth = system_auth(&[SCOPE_MCP_CALL], &["sandbox_read_file"]);
        let allowed = json!({
            "jsonrpc": "2.0",
            "id": "request-1",
            "method": "tools/call",
            "params": { "name": "sandbox_read_file", "arguments": {} }
        });
        let denied = json!({
            "jsonrpc": "2.0",
            "id": "request-2",
            "method": "tools/call",
            "params": { "name": "sandbox_terminal_exec", "arguments": {} }
        });

        assert!(authorize_mcp_proxy_payload(&auth, &lease_record(), &allowed).is_ok());
        let err = authorize_mcp_proxy_payload(&auth, &lease_record(), &denied)
            .expect_err("unexpected allowed tool call");
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn mcp_proxy_rejects_payload_without_method() {
        let auth = system_auth(&[SCOPE_MCP_CALL], &["*"]);
        let payload = json!({ "jsonrpc": "2.0", "id": "request-1", "params": {} });

        let err = authorize_mcp_proxy_payload(&auth, &lease_record(), &payload)
            .expect_err("unexpected accepted invalid payload");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn idempotency_key_is_trimmed_and_optional() {
        assert_eq!(
            normalize_idempotency_key(Some("  sandbox-lease:run-1  ".to_string()))
                .expect("valid key"),
            Some("sandbox-lease:run-1".to_string())
        );
        assert_eq!(
            normalize_idempotency_key(Some("   ".to_string())).expect("blank key"),
            None
        );
        assert_eq!(normalize_idempotency_key(None).expect("missing key"), None);
    }

    #[test]
    fn idempotency_key_rejects_oversized_values() {
        let err = normalize_idempotency_key(Some("x".repeat(161)))
            .expect_err("unexpected accepted oversized idempotency key");
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
