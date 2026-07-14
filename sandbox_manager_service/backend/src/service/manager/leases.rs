// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::PathBuf;
use std::time::Duration;

use axum::http::StatusCode;
use chatos_sandbox_contract::{
    ApprovalPolicy, ApprovalReviewer, EffectiveSandboxPolicy, PermissionProfileId,
    SandboxBackendKind, SandboxLeasePolicyRequest,
};
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::auth::{SandboxAuthContext, SCOPE_LEASE_DESTROY, SCOPE_LEASE_READ, SCOPE_LEASE_RELEASE};
use crate::backend::SandboxCreateSpec;
use crate::config::{AppConfig, SandboxBackendKind as ManagerBackendKind};
use crate::error::ApiError;
use crate::models::{
    CreateSandboxLeaseRequest, CreateSandboxLeaseResponse, DestroySandboxResponse,
    HeartbeatRequest, HeartbeatResponse, ListSandboxQuery, NetworkPolicy, ReleaseSandboxRequest,
    ReleaseSandboxResponse, SandboxEventRecord, SandboxLeaseRecord, SandboxStatus,
};
use crate::store::is_duplicate_key_error;

use super::super::{images, output_manifest};
use super::lease_inputs::{normalize_idempotency_key, sanitize_path_segment, validate_required};
use super::{now_rfc3339, prefixed_id, SandboxManager};

impl SandboxManager {
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
        let requested_policy =
            EffectiveSandboxPolicy::resolve(&input.policy, &EffectiveSandboxPolicy::default());
        if requested_policy.sandbox_mode != SandboxBackendKind::Docker {
            return Err(ApiError::with_code(
                StatusCode::CONFLICT,
                "sandbox_backend_not_ready",
                "this sandbox manager does not provide a local process backend",
            ));
        }
        let effective_policy = sandbox_manager_effective_policy(&input.policy);
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
        let now = now_rfc3339();
        let ttl = Duration::from_secs(input.ttl_seconds.unwrap_or(self.config.lease_ttl.as_secs()));
        let expires_at = (Utc::now()
            + ChronoDuration::from_std(ttl).unwrap_or_else(|_| ChronoDuration::seconds(7_200)))
        .to_rfc3339();
        let run_workspace =
            self.prepare_run_workspace(input.workspace_root.as_str(), run_id.as_str())?;
        let resource_limits = input.resource_limits.unwrap_or_default();
        let network = input.network.unwrap_or_default();
        validate_requested_network_policy(&self.config, &network)?;
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
            status: SandboxStatus::Pending,
            agent_endpoint: None,
            resource_limits,
            network,
            tools,
            agent_token_nonce: Some(agent_token_nonce),
            idempotency_key: idempotency_key.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at,
            destroyed_at: None,
            last_error: None,
            effective_policy: effective_policy.clone(),
        };

        let capacity_claim_until = (Utc::now() + ChronoDuration::minutes(5)).to_rfc3339();
        let acquired_capacity = self
            .store
            .try_acquire_active_slot(
                self.pool.max_active(),
                lease_id.as_str(),
                sandbox_id.as_str(),
                capacity_claim_until.as_str(),
            )
            .await
            .map_err(ApiError::internal)?;
        if acquired_capacity {
            record.status = SandboxStatus::Leasing;
        } else {
            let pending = self
                .store
                .count_pending_leases(now.as_str())
                .await
                .map_err(ApiError::internal)?;
            let max_pending = self.pool.max_pending();
            if pending >= max_pending {
                return Err(ApiError::capacity(format!(
                    "sandbox global pool and queue are full: max_active={}, pending={}, max_pending={max_pending}",
                    self.pool.max_active(),
                    pending
                )));
            }
        }

        if let Err(err) = self.store.create_lease(&record).await {
            if acquired_capacity {
                let _ = self.store.release_active_slot(lease_id.as_str()).await;
            }
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

        if !acquired_capacity {
            self.event(
                &record,
                "lease_queued",
                Some("sandbox lease queued"),
                Some(json!({
                    "backend": self.backend.kind(),
                    "image_id": image.id,
                    "image_ref": image.image_ref,
                    "max_active": self.pool.max_active(),
                    "max_pending": self.pool.max_pending(),
                    "effective_policy": effective_policy,
                })),
            )
            .await;
            return self.create_lease_response_from_existing(record);
        }

        self.event(
            &record,
            "lease_created",
            Some("sandbox lease created"),
            Some(json!({
                "backend": self.backend.kind(),
                "image_id": image.id,
                "image_ref": image.image_ref,
                "effective_policy": effective_policy,
            })),
        )
        .await;

        self.start_claimed_lease(record).await
    }

    fn create_lease_response_from_existing(
        &self,
        record: SandboxLeaseRecord,
    ) -> Result<CreateSandboxLeaseResponse, ApiError> {
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
            effective_policy: record.effective_policy,
        })
    }

    async fn start_claimed_lease(
        &self,
        mut record: SandboxLeaseRecord,
    ) -> Result<CreateSandboxLeaseResponse, ApiError> {
        if let Err(err) = self
            .store
            .extend_active_slot(record.id.as_str(), record.expires_at.as_str())
            .await
        {
            record.status = SandboxStatus::Failed;
            record.last_error = Some(err.clone());
            record.idempotency_key = None;
            record.updated_at = now_rfc3339();
            let _ = self.store.replace_lease(&record).await;
            let _ = self.store.release_active_slot(record.id.as_str()).await;
            return Err(ApiError::internal(err));
        }

        let agent_token = self.agent_token_for_record(&record);
        let create_result = self
            .backend
            .create(SandboxCreateSpec {
                sandbox_id: record.sandbox_id.clone(),
                run_workspace: record.run_workspace.clone(),
                image: record.image_ref.clone().unwrap_or_default(),
                agent_token: Some(agent_token.clone()),
                resource_limits: record.resource_limits.clone(),
                network: record.network.clone(),
            })
            .await;

        match create_result {
            Ok(instance) => {
                if let Err(err) = self.backend.start(record.sandbox_id.as_str()).await {
                    record.status = SandboxStatus::Failed;
                    record.last_error = Some(err.clone());
                    record.idempotency_key = None;
                    record.updated_at = now_rfc3339();
                    let _ = self.store.replace_lease(&record).await;
                    let _ = self.store.release_active_slot(record.id.as_str()).await;
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
                    lease_id: record.id,
                    sandbox_id: record.sandbox_id,
                    backend_id: record.backend_id,
                    image_id: record.image_id,
                    image_ref: record.image_ref,
                    status: record.status,
                    agent_endpoint: record.agent_endpoint,
                    agent_token,
                    run_workspace: record.run_workspace,
                    expires_at: record.expires_at,
                    effective_policy: record.effective_policy,
                })
            }
            Err(err) => {
                record.status = SandboxStatus::Failed;
                record.last_error = Some(err.clone());
                record.idempotency_key = None;
                record.updated_at = now_rfc3339();
                let _ = self.store.replace_lease(&record).await;
                let _ = self.store.release_active_slot(record.id.as_str()).await;
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

    pub async fn promote_pending_leases(&self) -> Result<usize, String> {
        let mut promoted = 0usize;
        loop {
            let now = now_rfc3339();
            let pending = self.store.list_pending_leases(now.as_str(), 1).await?;
            let Some(candidate) = pending.into_iter().next() else {
                break;
            };
            let capacity_claim_until = (Utc::now() + ChronoDuration::minutes(5)).to_rfc3339();
            let acquired_capacity = self
                .store
                .try_acquire_active_slot(
                    self.pool.max_active(),
                    candidate.id.as_str(),
                    candidate.sandbox_id.as_str(),
                    capacity_claim_until.as_str(),
                )
                .await?;
            if !acquired_capacity {
                break;
            }

            let now = now_rfc3339();
            let Some(record) = self
                .store
                .claim_pending_lease(candidate.id.as_str(), now.as_str())
                .await?
            else {
                let _ = self.store.release_active_slot(candidate.id.as_str()).await;
                continue;
            };

            self.event(
                &record,
                "lease_promoted",
                Some("queued sandbox lease promoted"),
                Some(json!({
                    "max_active": self.pool.max_active(),
                    "max_pending": self.pool.max_pending(),
                })),
            )
            .await;

            if let Err(err) = self.start_claimed_lease(record.clone()).await {
                tracing::warn!(
                    lease_id = record.id.as_str(),
                    sandbox_id = record.sandbox_id.as_str(),
                    "promote pending sandbox failed: {}",
                    err.message
                );
            }
            promoted += 1;
        }
        Ok(promoted)
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

        let mut output_error = None;
        let output_manifest = if input.export_result {
            match output_manifest::export_output_workspace(&record) {
                Ok(manifest) => Some(manifest),
                Err(err) => {
                    let message = format!("sandbox output export failed: {}", err.message);
                    tracing::warn!(
                        sandbox_id = record.sandbox_id.as_str(),
                        lease_id = record.id.as_str(),
                        run_id = record.run_id.as_str(),
                        "sandbox output export failed during release: {}",
                        err.message
                    );
                    self.event(
                        &record,
                        "sandbox_output_export_failed",
                        Some(message.as_str()),
                        Some(json!({
                            "code": err.code,
                            "status": err.status.as_u16(),
                        })),
                    )
                    .await;
                    output_error = Some(message);
                    None
                }
            }
        } else {
            None
        };
        let output_workspace = output_manifest
            .as_ref()
            .and_then(|manifest| manifest.output_workspace.clone());
        let diff_summary = output_manifest
            .as_ref()
            .map(output_manifest::summarize_output_manifest);

        if input.destroy {
            self.destroy_record(record.clone(), "sandbox_released")
                .await?;
            Ok(ReleaseSandboxResponse {
                ok: true,
                status: SandboxStatus::Destroyed,
                output_workspace,
                diff_summary,
                output_error,
                change_manifest: output_manifest,
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
                output_workspace,
                diff_summary,
                output_error,
                change_manifest: output_manifest,
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
        let expired_pending = self.store.list_expired_pending(now.as_str(), 100).await?;
        for mut record in expired_pending {
            record.status = SandboxStatus::Expired;
            record.updated_at = now_rfc3339();
            record.last_error = Some("queued lease expired".to_string());
            record.idempotency_key = None;
            self.store.replace_lease(&record).await?;
            self.event(
                &record,
                "sandbox_expired",
                Some("queued sandbox lease expired"),
                None,
            )
            .await;
        }
        if let Err(err) = self.promote_pending_leases().await {
            tracing::warn!("promote pending sandboxes after cleanup failed: {}", err);
        }
        Ok(())
    }

    pub(super) async fn require_sandbox(
        &self,
        sandbox_id: &str,
    ) -> Result<SandboxLeaseRecord, ApiError> {
        self.store
            .get_by_sandbox_id(sandbox_id)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("sandbox not found: {sandbox_id}")))
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
        if let Err(err) = self.promote_pending_leases().await {
            tracing::warn!("promote pending sandboxes after destroy failed: {}", err);
        }
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
}

fn validate_requested_network_policy(
    config: &AppConfig,
    network: &NetworkPolicy,
) -> Result<(), ApiError> {
    let requested = network.mode.trim();
    let configured = configured_network_mode(config);
    if requested_network_mode_is_allowed(requested, configured) {
        return Ok(());
    }
    Err(ApiError::bad_request(format!(
        "sandbox network mode {requested:?} is not allowed for lease requests; omit network.mode to use the configured default"
    )))
}

fn configured_network_mode(config: &AppConfig) -> Option<&str> {
    match config.backend {
        ManagerBackendKind::Docker => Some(config.docker_network_mode.trim()),
        ManagerBackendKind::Kata => Some(config.kata_network_mode.trim()),
        ManagerBackendKind::Mock => None,
    }
    .filter(|value| !value.is_empty())
}

fn requested_network_mode_is_allowed(requested: &str, configured: Option<&str>) -> bool {
    let requested = requested.trim();
    requested.is_empty()
        || requested.eq_ignore_ascii_case("bridge")
        || configured
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|value| value.eq_ignore_ascii_case(requested))
}

fn sandbox_manager_effective_policy(request: &SandboxLeasePolicyRequest) -> EffectiveSandboxPolicy {
    EffectiveSandboxPolicy {
        sandbox_mode: SandboxBackendKind::Docker,
        // The Docker manager currently exposes a writable run workspace. It does not implement
        // read-only file policy or host full-access escalation.
        permission_profile_id: PermissionProfileId::WorkspaceWrite,
        // The cloud Sandbox Manager has no user/AI approval loop in the MCP proxy. Report the
        // actual behavior so Task Runner can fail closed when a task explicitly requires approval.
        approval_policy: ApprovalPolicy::Never,
        approval_reviewer: ApprovalReviewer::User,
        policy_revision: request.policy_revision.clone(),
        additional_writable_roots: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requested_network_mode_allows_empty_bridge_or_configured_default() {
        assert!(requested_network_mode_is_allowed("", None));
        assert!(requested_network_mode_is_allowed(" bridge ", None));
        assert!(requested_network_mode_is_allowed(
            "sandbox-internal",
            Some("sandbox-internal")
        ));
        assert!(requested_network_mode_is_allowed(
            "SANDBOX-INTERNAL",
            Some("sandbox-internal")
        ));
    }

    #[test]
    fn requested_network_mode_rejects_boundary_expanding_overrides() {
        assert!(!requested_network_mode_is_allowed("host", Some("bridge")));
        assert!(!requested_network_mode_is_allowed(
            "container:other",
            Some("bridge")
        ));
        assert!(!requested_network_mode_is_allowed(
            "prod-db-network",
            Some("sandbox-internal")
        ));
        assert!(!requested_network_mode_is_allowed("none", Some("bridge")));
    }

    #[test]
    fn effective_policy_reports_only_capabilities_enforced_by_manager() {
        let request = SandboxLeasePolicyRequest {
            sandbox_mode: Some(SandboxBackendKind::Docker),
            permission_profile_id: Some(PermissionProfileId::ReadOnly),
            approval_policy: Some(ApprovalPolicy::OnRequest),
            approval_reviewer: Some(ApprovalReviewer::AutoReview),
            policy_revision: Some("request-revision".to_string()),
            additional_writable_roots: vec!["/outside".to_string()],
        };

        let effective = sandbox_manager_effective_policy(&request);

        assert_eq!(effective.sandbox_mode, SandboxBackendKind::Docker);
        assert_eq!(
            effective.permission_profile_id,
            PermissionProfileId::WorkspaceWrite
        );
        assert_eq!(effective.approval_policy, ApprovalPolicy::Never);
        assert_eq!(effective.approval_reviewer, ApprovalReviewer::User);
        assert_eq!(
            effective.policy_revision.as_deref(),
            Some("request-revision")
        );
        assert!(effective.additional_writable_roots.is_empty());
    }
}
