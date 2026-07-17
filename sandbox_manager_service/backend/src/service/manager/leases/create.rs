// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

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
            lease_kind: "sandbox".to_string(),
            primary_service_id: None,
            environment_services: Vec::new(),
            agent_token_nonce: Some(agent_token_nonce),
            idempotency_key: idempotency_key.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at,
            destroyed_at: None,
            last_error: None,
            effective_policy: effective_policy.clone(),
            effective_permissions: Some(effective_permissions),
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
        let effective_permissions = record.effective_permissions.clone().unwrap_or_else(|| {
            legacy_policy_permission_snapshot(
                &record.effective_policy,
                vec![record.run_workspace.clone()],
            )
        });
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
            effective_permissions,
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
                let effective_permissions =
                    record.effective_permissions.clone().unwrap_or_else(|| {
                        legacy_policy_permission_snapshot(
                            &record.effective_policy,
                            vec![record.run_workspace.clone()],
                        )
                    });
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
                    effective_permissions,
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
}
