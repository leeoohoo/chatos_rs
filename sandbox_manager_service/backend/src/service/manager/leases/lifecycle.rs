// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl SandboxManager {
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

    pub(in crate::service::manager) async fn require_sandbox(
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

    pub(super) fn prepare_run_workspace(
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
        prepare_sandbox_workspace_owner(run_workspace.as_path()).map_err(ApiError::internal)?;
        Ok(run_workspace)
    }
}
