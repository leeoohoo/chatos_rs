// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeSet;
use std::time::Duration;

use axum::http::StatusCode;
use chatos_sandbox_contract::{
    legacy_policy_permission_snapshot, EffectiveSandboxPolicy, SandboxBackendKind,
};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::auth::{
    SandboxAuthContext, SCOPE_LEASE_CREATE, SCOPE_LEASE_READ, SCOPE_LEASE_RELEASE, SCOPE_MCP_CALL,
};
use crate::backend::{SandboxEnvironmentCreateSpec, SandboxExecResult};
use crate::error::ApiError;
use crate::models::{
    CreateSandboxEnvironmentLeaseRequest, SandboxEnvironmentExecRequest,
    SandboxEnvironmentExecResponse, SandboxEnvironmentLeaseResponse,
    SandboxEnvironmentServiceInput, SandboxEnvironmentServiceRecord, SandboxEnvironmentStopRequest,
    SandboxLeaseRecord, SandboxStatus, StartSandboxEnvironmentRequest,
};

use super::leases::policy::{sandbox_manager_effective_policy, validate_requested_network_policy};
use super::{images, now_rfc3339, prefixed_id, SandboxManager};

mod support;

use self::support::{
    backend_environment_service_spec, ensure_mcp_target, ensure_terminal_target,
    environment_backend_error, resolve_primary_service_id, validate_application_service,
    validate_dependency_service, validate_environment_identity, validate_environment_values,
    validate_service_id, PreparedEnvironmentService, MAX_ENVIRONMENT_SERVICES,
};

impl SandboxManager {
    pub async fn create_environment_lease(
        &self,
        auth: &SandboxAuthContext,
        input: CreateSandboxEnvironmentLeaseRequest,
        idempotency_key: Option<String>,
    ) -> Result<SandboxEnvironmentLeaseResponse, ApiError> {
        validate_environment_identity(&input)?;
        auth.ensure_create_environment_lease_allowed(&input)?;
        let requested_policy =
            EffectiveSandboxPolicy::resolve(&input.policy, &EffectiveSandboxPolicy::default());
        if requested_policy.sandbox_mode != SandboxBackendKind::Docker {
            return Err(ApiError::with_code(
                StatusCode::CONFLICT,
                "sandbox_backend_not_ready",
                "sandbox environment groups currently require the Docker backend",
            ));
        }
        let effective_policy = sandbox_manager_effective_policy(&input.policy);
        let idempotency_key = super::lease_inputs::normalize_idempotency_key(idempotency_key)?;
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
                if existing.lease_kind != "environment" {
                    return Err(ApiError::with_code(
                        StatusCode::CONFLICT,
                        "sandbox_lease_kind_conflict",
                        "idempotency key is already used by a single-container sandbox lease",
                    ));
                }
                return Ok(self.environment_response(&existing));
            }
        }

        let lease_id = prefixed_id("environment_lease");
        let environment_id = prefixed_id("environment");
        let now = now_rfc3339();
        let ttl = Duration::from_secs(input.ttl_seconds.unwrap_or(self.config.lease_ttl.as_secs()));
        let expires_at = (Utc::now()
            + ChronoDuration::from_std(ttl).unwrap_or_else(|_| ChronoDuration::seconds(7_200)))
        .to_rfc3339();
        let run_workspace =
            self.prepare_run_workspace(input.workspace_root.as_str(), run_id.as_str())?;
        let effective_permissions = legacy_policy_permission_snapshot(
            &effective_policy,
            vec![run_workspace.to_string_lossy().to_string()],
        );
        let mut resource_limits = input.resource_limits.unwrap_or_default();
        resource_limits.cpu = resource_limits.cpu.max(0.1);
        resource_limits.memory_mb = resource_limits.memory_mb.max(128);
        resource_limits.disk_mb = resource_limits.disk_mb.max(128);
        resource_limits.max_processes = resource_limits.max_processes.max(16);
        let network = input.network.unwrap_or_default();
        validate_requested_network_policy(&self.config, &network)?;
        let capacity_claim_until = (Utc::now() + ChronoDuration::minutes(5)).to_rfc3339();
        let acquired = self
            .store
            .try_acquire_active_slot(
                self.pool.max_active(),
                lease_id.as_str(),
                environment_id.as_str(),
                capacity_claim_until.as_str(),
            )
            .await
            .map_err(ApiError::internal)?;
        if !acquired {
            return Err(ApiError::capacity(
                "sandbox environment capacity is full; environment leases are not queued",
            ));
        }
        let record = SandboxLeaseRecord {
            id: lease_id,
            sandbox_id: environment_id,
            tenant_id,
            user_id: input.user_id.trim().to_string(),
            project_id,
            run_id,
            workspace_root: input.workspace_root.trim().to_string(),
            run_workspace: run_workspace.to_string_lossy().to_string(),
            backend: self.backend.kind().to_string(),
            backend_id: None,
            image_id: None,
            image_ref: None,
            status: SandboxStatus::Pending,
            agent_endpoint: None,
            resource_limits,
            network,
            tools: vec!["filesystem".to_string(), "terminal".to_string()],
            lease_kind: "environment".to_string(),
            primary_service_id: None,
            environment_services: Vec::new(),
            agent_token_nonce: Some(Uuid::new_v4().simple().to_string()),
            idempotency_key,
            created_at: now.clone(),
            updated_at: now,
            expires_at,
            destroyed_at: None,
            last_error: None,
            effective_policy,
            effective_permissions: Some(effective_permissions),
        };
        if let Err(error) = self.store.create_lease(&record).await {
            let _ = self.store.release_active_slot(record.id.as_str()).await;
            return Err(ApiError::internal(error));
        }
        self.event(
            &record,
            "environment_lease_prepared",
            Some("sandbox environment workspace is ready for source synchronization"),
            None,
        )
        .await;
        Ok(self.environment_response(&record))
    }

    pub async fn start_environment(
        &self,
        auth: &SandboxAuthContext,
        environment_id: &str,
        input: StartSandboxEnvironmentRequest,
    ) -> Result<SandboxEnvironmentLeaseResponse, ApiError> {
        let mut record = self.require_environment(environment_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_CREATE)?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match environment"));
        }
        if !matches!(
            record.status,
            SandboxStatus::Pending | SandboxStatus::Stopped
        ) {
            return Err(ApiError::with_code(
                StatusCode::CONFLICT,
                "sandbox_environment_not_startable",
                format!(
                    "sandbox environment cannot start from status {}",
                    record.status.as_str()
                ),
            ));
        }
        if record.status == SandboxStatus::Stopped {
            if !input.services.is_empty() {
                return Err(ApiError::bad_request(
                    "restarting a stopped environment cannot replace its program-managed topology",
                ));
            }
            if let Some(requested) = input
                .primary_service_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if record.primary_service_id.as_deref() != Some(requested) {
                    return Err(ApiError::bad_request(
                        "restarting a stopped environment cannot change primary_service_id",
                    ));
                }
            }
            self.backend
                .start_environment(record.sandbox_id.as_str())
                .await
                .map_err(environment_backend_error)?;
            let instance = self
                .backend
                .inspect_environment(record.sandbox_id.as_str())
                .await
                .map_err(environment_backend_error)?
                .ok_or_else(|| environment_backend_error("restarted environment not found"))?;
            for service in &mut record.environment_services {
                if let Some(runtime) = instance
                    .services
                    .iter()
                    .find(|runtime| runtime.service_id == service.service_id)
                {
                    service.status = runtime.status.clone();
                    service.backend_id = runtime.backend_id.clone();
                    service.agent_endpoint = runtime.agent_endpoint.clone();
                    service.image_ref = runtime.image_ref.clone();
                }
            }
            record.backend_id = instance.backend_id;
            record.agent_endpoint = record.primary_service_id.as_deref().and_then(|service_id| {
                record
                    .environment_services
                    .iter()
                    .find(|service| service.service_id == service_id)
                    .and_then(|service| service.agent_endpoint.clone())
            });
            record.status = SandboxStatus::Ready;
            record.last_error = None;
            record.updated_at = now_rfc3339();
            self.store
                .replace_lease(&record)
                .await
                .map_err(ApiError::internal)?;
            self.event(
                &record,
                "environment_restarted",
                Some("sandbox environment services restarted"),
                None,
            )
            .await;
            return Ok(self.environment_response(&record));
        }
        let prepared = self.prepare_environment_services(input.services).await?;
        let primary_service_id =
            resolve_primary_service_id(input.primary_service_id.as_deref(), prepared.as_slice())?;
        let agent_token = self.agent_token_for_record(&record);
        record.status = SandboxStatus::Starting;
        record.primary_service_id = Some(primary_service_id.clone());
        record.last_error = None;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "environment_starting",
            Some("building and starting sandbox environment services"),
            Some(json!({
                "primary_service_id": primary_service_id,
                "service_ids": prepared.iter().map(|service| service.input.service_id.as_str()).collect::<Vec<_>>(),
            })),
        )
        .await;

        let create_result = self
            .backend
            .create_environment(SandboxEnvironmentCreateSpec {
                environment_id: record.sandbox_id.clone(),
                run_workspace: record.run_workspace.clone(),
                services: prepared
                    .iter()
                    .map(backend_environment_service_spec)
                    .collect(),
                agent_token,
                resource_limits: record.resource_limits.clone(),
                network: record.network.clone(),
            })
            .await;
        let instance = match create_result {
            Ok(instance) => instance,
            Err(error) => {
                record.status = SandboxStatus::Failed;
                record.last_error = Some(error.clone());
                record.updated_at = now_rfc3339();
                let _ = self.store.replace_lease(&record).await;
                self.event(&record, "environment_start_failed", Some(&error), None)
                    .await;
                return Err(ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_environment_start_failed",
                    error,
                ));
            }
        };
        record.backend_id = instance.backend_id;
        record.environment_services = prepared
            .iter()
            .map(|service| {
                let runtime = instance
                    .services
                    .iter()
                    .find(|runtime| runtime.service_id == service.input.service_id);
                SandboxEnvironmentServiceRecord {
                    service_id: service.input.service_id.clone(),
                    environment_key: service.input.environment_key.clone(),
                    display_name: service.input.display_name.clone(),
                    service_role: service.input.service_role.clone(),
                    image_id: service.input.image_id.clone(),
                    image_ref: runtime
                        .map(|runtime| runtime.image_ref.clone())
                        .unwrap_or_else(|| service.image_ref.clone()),
                    backend_id: runtime.and_then(|runtime| runtime.backend_id.clone()),
                    status: runtime
                        .map(|runtime| runtime.status.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    agent_endpoint: runtime.and_then(|runtime| runtime.agent_endpoint.clone()),
                    mcp_policy: service.input.mcp_policy.clone(),
                }
            })
            .collect();
        record.agent_endpoint = record
            .environment_services
            .iter()
            .find(|service| service.service_id == primary_service_id)
            .and_then(|service| service.agent_endpoint.clone());
        record.status = SandboxStatus::Ready;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "environment_ready",
            Some("sandbox environment is ready"),
            Some(json!({ "primary_service_id": primary_service_id })),
        )
        .await;
        Ok(self.environment_response(&record))
    }

    pub async fn get_environment(
        &self,
        auth: &SandboxAuthContext,
        environment_id: &str,
    ) -> Result<SandboxEnvironmentLeaseResponse, ApiError> {
        let mut record = self.require_environment(environment_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_READ)?;
        if let Some(instance) = self
            .backend
            .inspect_environment(record.sandbox_id.as_str())
            .await
            .map_err(environment_backend_error)?
        {
            for service in &mut record.environment_services {
                if let Some(runtime) = instance
                    .services
                    .iter()
                    .find(|runtime| runtime.service_id == service.service_id)
                {
                    service.status = runtime.status.clone();
                    service.backend_id = runtime.backend_id.clone();
                    service.agent_endpoint = runtime.agent_endpoint.clone();
                    service.image_ref = runtime.image_ref.clone();
                }
            }
            record.backend_id = instance.backend_id;
            record.updated_at = now_rfc3339();
            let _ = self.store.replace_lease(&record).await;
        }
        Ok(self.environment_response(&record))
    }

    pub async fn stop_environment(
        &self,
        auth: &SandboxAuthContext,
        environment_id: &str,
        input: SandboxEnvironmentStopRequest,
    ) -> Result<SandboxEnvironmentLeaseResponse, ApiError> {
        let mut record = self.require_environment(environment_id).await?;
        auth.ensure_lease_access(&record, SCOPE_LEASE_RELEASE)?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match environment"));
        }
        self.backend
            .stop_environment(record.sandbox_id.as_str())
            .await
            .map_err(environment_backend_error)?;
        for service in &mut record.environment_services {
            service.status = "stopped".to_string();
        }
        record.status = SandboxStatus::Stopped;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "environment_stopped",
            Some("sandbox environment stopped"),
            None,
        )
        .await;
        Ok(self.environment_response(&record))
    }

    pub async fn exec_environment_service(
        &self,
        auth: &SandboxAuthContext,
        environment_id: &str,
        service_id: &str,
        input: SandboxEnvironmentExecRequest,
    ) -> Result<SandboxEnvironmentExecResponse, ApiError> {
        let record = self.require_environment(environment_id).await?;
        auth.ensure_lease_access(&record, SCOPE_MCP_CALL)?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match environment"));
        }
        let service = record
            .environment_services
            .iter()
            .find(|service| service.service_id == service_id)
            .ok_or_else(|| {
                ApiError::not_found(format!("environment service not found: {service_id}"))
            })?;
        ensure_terminal_target(service)?;
        let SandboxExecResult {
            exit_code,
            stdout,
            stderr,
        } = self
            .backend
            .exec_environment_service(
                record.sandbox_id.as_str(),
                service_id,
                input.command.as_slice(),
            )
            .await
            .map_err(environment_backend_error)?;
        self.event(
            &record,
            "environment_service_exec",
            Some("command executed in application service"),
            Some(json!({ "service_id": service_id, "exit_code": exit_code })),
        )
        .await;
        Ok(SandboxEnvironmentExecResponse {
            service_id: service_id.to_string(),
            exit_code,
            stdout,
            stderr,
        })
    }

    pub async fn environment_mcp_proxy(
        &self,
        auth: &SandboxAuthContext,
        environment_id: &str,
        service_id: Option<&str>,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let record = self.require_environment(environment_id).await?;
        super::mcp_proxy::authorize_mcp_proxy_payload(auth, &record, &payload)?;
        let service_id = service_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or(record.primary_service_id.as_deref())
            .ok_or_else(|| ApiError::bad_request("environment service_id is required"))?;
        let service = record
            .environment_services
            .iter()
            .find(|service| service.service_id == service_id)
            .ok_or_else(|| {
                ApiError::not_found(format!("environment service not found: {service_id}"))
            })?;
        ensure_mcp_target(service)?;
        let endpoint = service
            .agent_endpoint
            .as_deref()
            .ok_or_else(|| environment_backend_error("application MCP endpoint is unavailable"))?;
        let agent_token = self.agent_token_for_record(&record);
        super::mcp_proxy::jsonrpc_agent_proxy(endpoint, Some(agent_token.as_str()), payload).await
    }

    async fn prepare_environment_services(
        &self,
        services: Vec<SandboxEnvironmentServiceInput>,
    ) -> Result<Vec<PreparedEnvironmentService>, ApiError> {
        if services.is_empty() || services.len() > MAX_ENVIRONMENT_SERVICES {
            return Err(ApiError::bad_request(format!(
                "environment must contain between 1 and {MAX_ENVIRONMENT_SERVICES} services"
            )));
        }
        let mut ids = BTreeSet::new();
        let mut prepared = Vec::with_capacity(services.len());
        for mut input in services {
            validate_service_id(input.service_id.as_str())?;
            if !ids.insert(input.service_id.clone()) {
                return Err(ApiError::bad_request(format!(
                    "duplicate environment service_id: {}",
                    input.service_id
                )));
            }
            input.service_role = input.service_role.trim().to_ascii_lowercase();
            validate_environment_values(&input.environment)?;
            let image_ref = match input.service_role.as_str() {
                "application" => {
                    validate_application_service(&input)?;
                    let image = images::resolve_for_create(
                        &self.config,
                        self.config.backend,
                        input.image_id.as_deref(),
                    )
                    .await
                    .map_err(ApiError::bad_request)?;
                    image.image_ref
                }
                "dependency" => {
                    validate_dependency_service(&input)?;
                    input
                        .image_ref
                        .clone()
                        .ok_or_else(|| ApiError::bad_request("dependency image_ref is required"))?
                }
                _ => {
                    return Err(ApiError::bad_request(format!(
                        "unsupported environment service_role: {}",
                        input.service_role
                    )))
                }
            };
            prepared.push(PreparedEnvironmentService { input, image_ref });
        }
        if !prepared
            .iter()
            .any(|service| service.input.service_role == "application")
        {
            return Err(ApiError::bad_request(
                "environment must contain at least one application service",
            ));
        }
        Ok(prepared)
    }

    async fn require_environment(
        &self,
        environment_id: &str,
    ) -> Result<SandboxLeaseRecord, ApiError> {
        let record = self.require_sandbox(environment_id).await?;
        if record.lease_kind != "environment" {
            return Err(ApiError::not_found(format!(
                "sandbox environment not found: {environment_id}"
            )));
        }
        Ok(record)
    }

    fn environment_response(&self, record: &SandboxLeaseRecord) -> SandboxEnvironmentLeaseResponse {
        let effective_permissions = record.effective_permissions.clone().unwrap_or_else(|| {
            legacy_policy_permission_snapshot(
                &record.effective_policy,
                vec![record.run_workspace.clone()],
            )
        });
        SandboxEnvironmentLeaseResponse {
            lease_id: record.id.clone(),
            environment_id: record.sandbox_id.clone(),
            backend_id: record.backend_id.clone(),
            status: record.status,
            run_workspace: record.run_workspace.clone(),
            expires_at: record.expires_at.clone(),
            primary_service_id: record.primary_service_id.clone(),
            agent_endpoint: record.agent_endpoint.clone(),
            services: record.environment_services.clone(),
            agent_token: self.agent_token_for_record(record),
            effective_policy: record.effective_policy.clone(),
            effective_permissions,
        }
    }
}

#[cfg(test)]
include!("environments.test.rs");
