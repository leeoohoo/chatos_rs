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
            let environment_lease = record.lease_kind == "environment";
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
            if environment_lease {
                let _ = self.store.release_active_slot(record.id.as_str()).await;
            }
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

        let destroy_result = if record.lease_kind == "environment" {
            self.backend
                .destroy_environment(record.sandbox_id.as_str())
                .await
        } else {
            self.backend
                .destroy(record.sandbox_id.as_str(), record.backend_id.as_deref())
                .await
        };
        if let Err(err) = destroy_result {
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

    pub(in crate::service::manager) fn prepare_run_workspace(
        &self,
        workspace_root: &str,
        run_id: &str,
    ) -> Result<PathBuf, ApiError> {
        let source_workspace = PathBuf::from(workspace_root.trim());
        if !source_workspace.is_dir() {
            return Err(ApiError::bad_request(format!(
                "sandbox source workspace is not a directory: {}",
                source_workspace.display()
            )));
        }
        let run_root = self
            .config
            .work_root
            .join("runs")
            .join(sanitize_path_segment(run_id));
        let run_workspace = run_root.join("input").join("workspace");
        let baseline_workspace = run_root.join("baseline").join("workspace");
        reset_workspace_directory(run_workspace.as_path())?;
        reset_workspace_directory(baseline_workspace.as_path())?;
        copy_source_workspace(
            source_workspace.as_path(),
            run_workspace.as_path(),
            source_workspace.as_path(),
        )?;
        copy_source_workspace(
            source_workspace.as_path(),
            baseline_workspace.as_path(),
            source_workspace.as_path(),
        )?;
        prepare_sandbox_workspace_owner(run_workspace.as_path()).map_err(ApiError::internal)?;
        Ok(run_workspace)
    }
}

fn reset_workspace_directory(path: &Path) -> Result<(), ApiError> {
    if path.exists() {
        std::fs::remove_dir_all(path).map_err(|error| {
            ApiError::internal(format!(
                "clear sandbox workspace {} failed: {error}",
                path.display()
            ))
        })?;
    }
    std::fs::create_dir_all(path).map_err(|error| {
        ApiError::internal(format!(
            "create sandbox workspace {} failed: {error}",
            path.display()
        ))
    })
}

fn copy_source_workspace(source: &Path, destination: &Path, root: &Path) -> Result<(), ApiError> {
    for entry in std::fs::read_dir(source).map_err(|error| {
        ApiError::internal(format!(
            "read sandbox source workspace {} failed: {error}",
            source.display()
        ))
    })? {
        let entry = entry.map_err(|error| {
            ApiError::internal(format!(
                "read sandbox source workspace entry failed: {error}"
            ))
        })?;
        let source_path = entry.path();
        if should_skip_source_workspace_path(root, source_path.as_path()) {
            continue;
        }
        let file_type = entry.file_type().map_err(|error| {
            ApiError::internal(format!(
                "read sandbox source file type {} failed: {error}",
                source_path.display()
            ))
        })?;
        if file_type.is_symlink() {
            return Err(ApiError::bad_request(format!(
                "sandbox source workspace contains an unsupported symlink: {}",
                source_path.display()
            )));
        }
        let destination_path = destination.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(destination_path.as_path()).map_err(|error| {
                ApiError::internal(format!(
                    "create sandbox workspace directory {} failed: {error}",
                    destination_path.display()
                ))
            })?;
            copy_source_workspace(source_path.as_path(), destination_path.as_path(), root)?;
        } else if file_type.is_file() {
            std::fs::copy(source_path.as_path(), destination_path.as_path()).map_err(|error| {
                ApiError::internal(format!(
                    "copy sandbox source file {} failed: {error}",
                    source_path.display()
                ))
            })?;
        }
    }
    Ok(())
}

fn should_skip_source_workspace_path(root: &Path, path: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return true;
    };
    relative.components().any(|component| match component {
        std::path::Component::Normal(name) => name.to_str().is_some_and(|name| {
            name == ".chatos"
                || name.starts_with(".chatos-")
                || matches!(
                    name,
                    ".runtime-cache"
                        | "node_modules"
                        | ".pnpm-store"
                        | ".yarn"
                        | ".vite"
                        | "__pycache__"
                        | ".pytest_cache"
                        | ".mypy_cache"
                        | ".ruff_cache"
                        | ".venv"
                        | "venv"
                        | "target"
                )
        }),
        _ => false,
    })
}

#[cfg(test)]
mod workspace_tests {
    use super::*;

    #[test]
    fn source_workspace_copy_initializes_input_and_baseline_without_runtime_caches() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let input = temp.path().join("input");
        let baseline = temp.path().join("baseline");
        std::fs::create_dir_all(source.join("src")).unwrap();
        std::fs::create_dir_all(source.join("node_modules/pkg")).unwrap();
        std::fs::create_dir_all(source.join(".git/refs")).unwrap();
        std::fs::write(source.join("src/main.rs"), "fn main() {}\n").unwrap();
        std::fs::write(source.join("node_modules/pkg/index.js"), "cached\n").unwrap();
        std::fs::write(source.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

        reset_workspace_directory(input.as_path()).unwrap();
        reset_workspace_directory(baseline.as_path()).unwrap();
        copy_source_workspace(source.as_path(), input.as_path(), source.as_path()).unwrap();
        copy_source_workspace(source.as_path(), baseline.as_path(), source.as_path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(input.join("src/main.rs")).unwrap(),
            "fn main() {}\n"
        );
        assert!(input.join(".git/HEAD").is_file());
        assert!(baseline.join(".git/HEAD").is_file());
        assert!(!input.join("node_modules").exists());
        assert!(!baseline.join("node_modules").exists());
    }
}
