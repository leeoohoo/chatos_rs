// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};
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
use crate::backend::{
    SandboxEnvironmentCreateSpec, SandboxEnvironmentServiceSpec, SandboxExecResult,
};
use crate::error::ApiError;
use crate::models::{
    CreateSandboxEnvironmentLeaseRequest, SandboxEnvironmentExecRequest,
    SandboxEnvironmentExecResponse, SandboxEnvironmentLeaseResponse, SandboxEnvironmentMcpPolicy,
    SandboxEnvironmentServiceInput, SandboxEnvironmentServiceRecord, SandboxEnvironmentStopRequest,
    SandboxLeaseRecord, SandboxStatus, StartSandboxEnvironmentRequest,
};

use super::leases::policy::{sandbox_manager_effective_policy, validate_requested_network_policy};
use super::{images, now_rfc3339, prefixed_id, SandboxManager};

const MAX_ENVIRONMENT_SERVICES: usize = 64;
const MAX_DOCKERFILE_BYTES: usize = 512 * 1024;
const MAX_ENVIRONMENT_VARIABLES: usize = 512;
const MAX_ENVIRONMENT_VALUE_BYTES: usize = 64 * 1024;

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

struct PreparedEnvironmentService {
    input: SandboxEnvironmentServiceInput,
    image_ref: String,
}

fn backend_environment_service_spec(
    service: &PreparedEnvironmentService,
) -> SandboxEnvironmentServiceSpec {
    SandboxEnvironmentServiceSpec {
        service_id: service.input.service_id.clone(),
        service_role: service.input.service_role.clone(),
        image: service.image_ref.clone(),
        dockerfile: service.input.dockerfile.clone(),
        environment: service.input.environment.clone(),
        mcp_enabled: service.input.service_role == "application",
    }
}

fn ensure_terminal_target(service: &SandboxEnvironmentServiceRecord) -> Result<(), ApiError> {
    if service.service_role == "application"
        && service.mcp_policy.terminal
        && service.mcp_policy.managed_by == "system"
    {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "terminal execution is allowed only for system-managed application targets",
    ))
}

fn ensure_mcp_target(service: &SandboxEnvironmentServiceRecord) -> Result<(), ApiError> {
    if service.service_role == "application"
        && service.mcp_policy.managed_by == "system"
        && service.mcp_policy.attachment == "project_gateway_target"
    {
        return Ok(());
    }
    Err(ApiError::forbidden(
        "MCP is allowed only for system-managed application targets",
    ))
}

fn validate_environment_identity(
    input: &CreateSandboxEnvironmentLeaseRequest,
) -> Result<(), ApiError> {
    for (name, value) in [
        ("tenant_id", input.tenant_id.as_str()),
        ("user_id", input.user_id.as_str()),
        ("project_id", input.project_id.as_str()),
        ("run_id", input.run_id.as_str()),
        ("workspace_root", input.workspace_root.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(ApiError::bad_request(format!("{name} is required")));
        }
    }
    Ok(())
}

fn environment_backend_error(message: impl Into<String>) -> ApiError {
    ApiError::with_code(
        StatusCode::BAD_GATEWAY,
        "sandbox_environment_backend_error",
        message,
    )
}

fn validate_service_id(value: &str) -> Result<(), ApiError> {
    if value.is_empty()
        || value.len() > 63
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
        || !value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
        || !value
            .chars()
            .last()
            .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit())
    {
        return Err(ApiError::bad_request(format!(
            "invalid environment service_id: {value}"
        )));
    }
    Ok(())
}

fn validate_environment_values(environment: &BTreeMap<String, String>) -> Result<(), ApiError> {
    if environment.len() > MAX_ENVIRONMENT_VARIABLES {
        return Err(ApiError::bad_request("too many environment variables"));
    }
    for (name, value) in environment {
        if name.starts_with("CHATOS_SANDBOX_MCP_")
            || matches!(
                name.as_str(),
                "MCP_TOKEN" | "MCP_PORT" | "MCP_IMAGE" | "MCP_COMMAND"
            )
        {
            return Err(ApiError::bad_request(format!(
                "program-managed MCP environment variable cannot be supplied: {name}"
            )));
        }
        if value.len() > MAX_ENVIRONMENT_VALUE_BYTES || value.contains('\0') {
            return Err(ApiError::bad_request(format!(
                "invalid environment variable value: {name}"
            )));
        }
    }
    Ok(())
}

fn validate_application_service(input: &SandboxEnvironmentServiceInput) -> Result<(), ApiError> {
    if input.mcp_policy
        != (SandboxEnvironmentMcpPolicy {
            managed_by: "system".to_string(),
            attachment: "project_gateway_target".to_string(),
            filesystem: true,
            terminal: true,
        })
    {
        return Err(ApiError::bad_request(
            "application service must use the system-managed project gateway target policy",
        ));
    }
    let dockerfile = input
        .dockerfile
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("application Dockerfile is required"))?;
    if dockerfile.len() > MAX_DOCKERFILE_BYTES {
        return Err(ApiError::bad_request("application Dockerfile is too large"));
    }
    if dockerfile_contains_agent_control(dockerfile) {
        return Err(ApiError::bad_request(
            "application Dockerfile cannot install or configure the program-managed MCP Agent",
        ));
    }
    if input.image_id.as_deref().is_none_or(str::is_empty) {
        return Err(ApiError::bad_request(
            "application image_id is required as the program-managed Agent source image",
        ));
    }
    Ok(())
}

fn validate_dependency_service(input: &SandboxEnvironmentServiceInput) -> Result<(), ApiError> {
    if input.mcp_policy != SandboxEnvironmentMcpPolicy::default() {
        return Err(ApiError::bad_request(
            "dependency service must not receive MCP policy or Agent access",
        ));
    }
    if input
        .dockerfile
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(ApiError::bad_request(
            "dependency service cannot provide a Dockerfile",
        ));
    }
    let image_ref = input.image_ref.as_deref().unwrap_or_default();
    if !images::known_dependency_image_ref(image_ref) {
        return Err(ApiError::bad_request(format!(
            "dependency image_ref is not a platform-managed image: {image_ref}"
        )));
    }
    Ok(())
}

fn dockerfile_contains_agent_control(dockerfile: &str) -> bool {
    let dockerfile = dockerfile.to_ascii_lowercase();
    [
        "chatos-sandbox-mcp",
        "chatos_sandbox_mcp",
        "mcp_token",
        "mcp_port",
        "agent_install_script",
        "agent_injection_mode",
        "/opt/chatos/",
    ]
    .iter()
    .any(|marker| dockerfile.contains(marker))
}

fn resolve_primary_service_id(
    requested: Option<&str>,
    services: &[PreparedEnvironmentService],
) -> Result<String, ApiError> {
    let applications = services
        .iter()
        .filter(|service| service.input.service_role == "application")
        .collect::<Vec<_>>();
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        if applications
            .iter()
            .any(|service| service.input.service_id == requested)
        {
            return Ok(requested.to_string());
        }
        return Err(ApiError::bad_request(format!(
            "primary_service_id is not an application service: {requested}"
        )));
    }
    if applications.len() == 1 {
        return Ok(applications[0].input.service_id.clone());
    }
    Err(ApiError::bad_request(
        "primary_service_id is required when multiple application services exist",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn application(service_id: &str) -> PreparedEnvironmentService {
        PreparedEnvironmentService {
            input: SandboxEnvironmentServiceInput {
                service_id: service_id.to_string(),
                environment_key: service_id.to_string(),
                display_name: service_id.to_string(),
                service_role: "application".to_string(),
                image_id: Some("default".to_string()),
                image_ref: None,
                dockerfile: Some("FROM alpine\n".to_string()),
                environment: BTreeMap::new(),
                mcp_policy: SandboxEnvironmentMcpPolicy {
                    managed_by: "system".to_string(),
                    attachment: "project_gateway_target".to_string(),
                    filesystem: true,
                    terminal: true,
                },
            },
            image_ref: "chatos-sandbox-agent:latest".to_string(),
        }
    }

    #[test]
    fn multiple_applications_require_program_selected_primary_service() {
        let services = vec![application("api"), application("worker")];
        assert!(resolve_primary_service_id(None, services.as_slice()).is_err());
        assert_eq!(
            resolve_primary_service_id(Some("worker"), services.as_slice())
                .expect("selected primary"),
            "worker"
        );
    }

    #[test]
    fn dependencies_cannot_receive_mcp_policy() {
        let dependency = SandboxEnvironmentServiceInput {
            service_id: "redis".to_string(),
            environment_key: "redis".to_string(),
            display_name: "Redis".to_string(),
            service_role: "dependency".to_string(),
            image_id: None,
            image_ref: Some("redis:7-alpine".to_string()),
            dockerfile: None,
            environment: BTreeMap::new(),
            mcp_policy: SandboxEnvironmentMcpPolicy {
                managed_by: "system".to_string(),
                attachment: "project_gateway_target".to_string(),
                filesystem: true,
                terminal: true,
            },
        };
        assert!(validate_dependency_service(&dependency).is_err());
    }

    #[test]
    fn application_dockerfile_is_forwarded_but_dependency_never_enables_mcp() {
        let application = application("api");
        let application_spec = backend_environment_service_spec(&application);
        assert_eq!(
            application_spec.dockerfile.as_deref(),
            Some("FROM alpine\n")
        );
        assert!(application_spec.mcp_enabled);

        let dependency = PreparedEnvironmentService {
            input: SandboxEnvironmentServiceInput {
                service_id: "redis".to_string(),
                environment_key: "redis".to_string(),
                display_name: "Redis".to_string(),
                service_role: "dependency".to_string(),
                image_id: None,
                image_ref: Some("redis:7-alpine".to_string()),
                dockerfile: None,
                environment: BTreeMap::new(),
                mcp_policy: SandboxEnvironmentMcpPolicy::default(),
            },
            image_ref: "redis:7-alpine".to_string(),
        };
        let dependency_spec = backend_environment_service_spec(&dependency);
        assert!(!dependency_spec.mcp_enabled);
        assert!(dependency_spec.dockerfile.is_none());
    }

    #[test]
    fn callers_cannot_inject_mcp_environment_or_agent_installation() {
        let mut environment = BTreeMap::new();
        environment.insert(
            "CHATOS_SANDBOX_MCP_TOKEN".to_string(),
            "caller-token".to_string(),
        );
        assert!(validate_environment_values(&environment).is_err());

        let mut application = application("api").input;
        application.dockerfile =
            Some("FROM alpine\nCOPY chatos-sandbox-mcp-server /usr/local/bin/\n".to_string());
        assert!(validate_application_service(&application).is_err());
    }

    #[test]
    fn dependency_targets_are_forbidden_for_terminal_and_mcp_routes() {
        let dependency = SandboxEnvironmentServiceRecord {
            service_id: "redis".to_string(),
            environment_key: "redis".to_string(),
            display_name: "Redis".to_string(),
            service_role: "dependency".to_string(),
            image_id: None,
            image_ref: "redis:7-alpine".to_string(),
            backend_id: Some("container-1".to_string()),
            status: "running".to_string(),
            agent_endpoint: None,
            mcp_policy: SandboxEnvironmentMcpPolicy::default(),
        };
        assert!(ensure_terminal_target(&dependency).is_err());
        assert!(ensure_mcp_target(&dependency).is_err());
    }
}
