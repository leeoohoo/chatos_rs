// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
    pub(in crate::services) async fn renew_active_sandbox_lease(
        &self,
        run_id: &str,
    ) -> Result<(), String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(());
        };
        let Some(context) = sandbox_context_from_run(&run)? else {
            return Ok(());
        };
        if context.provider_kind() != Ok(chatos_mcp_management_sdk::SandboxProviderKind::Cloud)
            || !context.is_environment
        {
            return Ok(());
        }
        let auth = SandboxManagerAuth::for_context(&self.config, &context)?;
        let client = SandboxManagerClient::new(context.manager_base_url.clone(), auth)?;
        let ttl_seconds = self.effective_sandbox_lease_ttl_seconds().await?;
        client
            .renew_environment_lease(&context, ttl_seconds)
            .await
            .map(|_| ())
    }

    pub(crate) async fn release_sandboxes_for_terminal_run(
        &self,
        run: &TaskRunRecord,
    ) -> Result<usize, String> {
        if let Some(context) = sandbox_context_from_run(run)? {
            match context.provider_kind()? {
                chatos_mcp_management_sdk::SandboxProviderKind::LocalConnector => {
                    let auth = SandboxManagerAuth::for_context(&self.config, &context)?;
                    let client = SandboxManagerClient::new(context.manager_base_url.clone(), auth)?;
                    let response = client.release(&context, false, true).await?;
                    self.store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_released_after_terminal_run",
                            Some("任务运行节点中断后，本地沙箱已自动释放".to_string()),
                            Some(json!({
                                "sandbox_id": context.sandbox_id,
                                "lease_id": context.lease_id,
                                "release_status": response.status,
                                "provider": context.provider,
                            })),
                        ))
                        .await?;
                    return Ok(1);
                }
                chatos_mcp_management_sdk::SandboxProviderKind::Cloud => {}
                chatos_mcp_management_sdk::SandboxProviderKind::None => {
                    return Err("sandbox runtime provider is unresolved".to_string());
                }
            }
        }
        let base_url = self.effective_sandbox_manager_base_url().await?;
        let client =
            SandboxManagerClient::new(base_url, SandboxManagerAuth::from_config(&self.config))?;
        let leases = client.list_run_leases(run.id.as_str()).await?;
        let mut released = 0usize;
        let mut failures = Vec::new();

        for lease in leases
            .into_iter()
            .filter(SandboxLeaseListItem::requires_cleanup)
        {
            match client.release_list_item(&lease, false, true).await {
                Ok(response) => {
                    released += 1;
                    if let Err(error) = self
                        .store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_released_after_terminal_run",
                            Some("任务运行节点中断后，遗留沙箱已自动释放".to_string()),
                            Some(json!({
                                "sandbox_id": lease.sandbox_id,
                                "lease_id": lease.id,
                                "previous_status": lease.status,
                                "release_status": response.status,
                            })),
                        ))
                        .await
                    {
                        warn!(
                            run_id = run.id.as_str(),
                            error = error.as_str(),
                            "failed to append terminal run sandbox release event"
                        );
                    }
                }
                Err(error) => {
                    failures.push(format!("{}: {error}", lease.sandbox_id));
                    if let Err(event_error) = self
                        .store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_release_failed_after_terminal_run",
                            Some("任务运行节点中断后，遗留沙箱自动释放失败".to_string()),
                            Some(json!({
                                "sandbox_id": lease.sandbox_id,
                                "lease_id": lease.id,
                                "previous_status": lease.status,
                                "error": error,
                            })),
                        ))
                        .await
                    {
                        warn!(
                            run_id = run.id.as_str(),
                            error = event_error.as_str(),
                            "failed to append terminal run sandbox release failure event"
                        );
                    }
                }
            }
        }

        if failures.is_empty() {
            Ok(released)
        } else {
            Err(format!(
                "failed to release {} sandbox lease(s) for terminal run: {}",
                failures.len(),
                failures.join("; ")
            ))
        }
    }

    pub(in crate::services) async fn release_sandbox(
        &self,
        run: &TaskRunRecord,
        context: &SandboxRuntimeContext,
    ) -> Option<SandboxOutputReport> {
        let base_url = if context.manager_base_url.trim().is_empty() {
            match self.effective_sandbox_manager_base_url().await {
                Ok(base_url) => base_url,
                Err(err) => {
                    warn!(
                        run_id = run.id.as_str(),
                        sandbox_id = context.sandbox_id.as_str(),
                        "failed to load sandbox manager base url for release: {err}"
                    );
                    return None;
                }
            }
        } else {
            context.manager_base_url.clone()
        };
        let auth = match SandboxManagerAuth::for_context(&self.config, context) {
            Ok(auth) => auth,
            Err(err) => {
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "failed to resolve sandbox auth for release: {err}"
                );
                return None;
            }
        };
        let client = match SandboxManagerClient::new(base_url, auth) {
            Ok(client) => client,
            Err(err) => {
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "invalid sandbox manager base url for release: {err}"
                );
                return None;
            }
        };
        match client.release(context, true, true).await {
            Ok(mut response) => {
                if context.provider_kind()
                    == Ok(chatos_mcp_management_sdk::SandboxProviderKind::LocalConnector)
                {
                    response.redact_local_paths();
                }
                let output_report = SandboxOutputReport::from_release_response(context, &response);
                let output_error = response.output_error.clone();
                let payload = json!({
                    "sandbox": context.to_metadata(),
                    "release": {
                        "ok": response.ok,
                        "status": response.status,
                        "output_workspace": response.output_workspace,
                        "diff_summary": response.diff_summary,
                        "output_error": output_error,
                        "change_counts": output_report.as_ref().map(|output| &output.file_change_counts),
                        "change_manifest_path": output_report.as_ref().and_then(|output| output.change_manifest_path.as_deref()),
                    },
                });
                if let Err(err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "sandbox_released",
                        Some("沙箱已释放".to_string()),
                        Some(payload),
                    ))
                    .await
                {
                    warn!(
                        "failed to append sandbox release event for run {}: {}",
                        run.id, err
                    );
                }
                if let Some(output) = output_report.as_ref() {
                    if let Err(err) = self
                        .store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_output_collected",
                            Some("沙箱输出变更清单已生成".to_string()),
                            Some(json!({
                                "sandbox": context.to_metadata(),
                                "output": output,
                            })),
                        ))
                        .await
                    {
                        warn!(
                            "failed to append sandbox output event for run {}: {}",
                            run.id, err
                        );
                    }
                }
                if let Some(output_error) = response
                    .output_error
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    if let Err(err) = self
                        .store
                        .append_run_event(TaskRunEventRecord::new(
                            run.id.clone(),
                            "sandbox_output_collect_failed",
                            Some(format!("沙箱输出变更清单生成失败: {output_error}")),
                            Some(json!({
                                "sandbox": context.to_metadata(),
                                "error": output_error,
                            })),
                        ))
                        .await
                    {
                        warn!(
                            "failed to append sandbox output failure event for run {}: {}",
                            run.id, err
                        );
                    }
                }
                output_report
            }
            Err(err) => {
                if let Err(event_err) = self
                    .store
                    .append_run_event(TaskRunEventRecord::new(
                        run.id.clone(),
                        "sandbox_release_failed",
                        Some(format!("释放沙箱失败: {err}")),
                        Some(context.to_metadata()),
                    ))
                    .await
                {
                    warn!(
                        "failed to append sandbox release failure event for run {}: {}",
                        run.id, event_err
                    );
                }
                warn!(
                    run_id = run.id.as_str(),
                    sandbox_id = context.sandbox_id.as_str(),
                    "failed to release sandbox: {err}"
                );
                None
            }
        }
    }

    pub async fn get_run_output_changes(
        &self,
        run_id: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Option<RunOutputChangesResponse>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        if super::super::harness_run_diff::harness_output_report_from_run(&run)?.is_some() {
            return self
                .get_harness_run_output_changes(&run, limit, offset)
                .await;
        }
        let Some(manifest) = read_output_change_manifest_for_run(&run)? else {
            return Ok(Some(RunOutputChangesResponse {
                run_id: run.id,
                base_commit: None,
                result_commit: None,
                comparison_scope: "run_incremental".to_string(),
                counts: RunOutputFileChangeCounts::default(),
                files: Vec::new(),
                total: 0,
                limit: limit.unwrap_or(100).clamp(1, 500),
                offset: offset.unwrap_or(0),
                has_more: false,
            }));
        };
        let total = manifest.files.len();
        let limit = limit.unwrap_or(100).clamp(1, 500);
        let offset = offset.unwrap_or(0);
        let files = manifest
            .files
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>();
        Ok(Some(RunOutputChangesResponse {
            run_id: run.id,
            base_commit: None,
            result_commit: None,
            comparison_scope: "run_incremental".to_string(),
            counts: manifest.counts,
            files,
            total,
            limit,
            offset,
            has_more: offset.saturating_add(limit) < total,
        }))
    }

    pub async fn get_run_output_diff(
        &self,
        run_id: &str,
        path: &str,
    ) -> Result<Option<RunOutputDiffResponse>, String> {
        let Some(run) = self.store.get_run(run_id).await? else {
            return Ok(None);
        };
        match self.get_harness_run_output_diff(&run, path).await {
            Ok(Some(response)) => return Ok(Some(response)),
            Ok(None) => {}
            Err(err) if err == "文件不在本次运行变更清单中" => return Err(err),
            Err(err) => warn!(
                run_id = run.id.as_str(),
                error = err.as_str(),
                "read Harness run diff failed; falling back to sandbox manifest"
            ),
        }
        let Some(manifest) = read_output_change_manifest_for_run(&run)? else {
            return Ok(Some(RunOutputDiffResponse {
                run_id: run.id,
                path: normalize_output_relative_path(path)?,
                status: "unknown".to_string(),
                patch: None,
                binary: false,
                diff_available: false,
                diff_truncated: false,
                message: Some("本次运行没有文件变更清单。".to_string()),
            }));
        };
        let normalized_path = normalize_output_relative_path(path)?;
        let Some(change) = manifest
            .files
            .iter()
            .find(|file| file.path == normalized_path)
        else {
            return Err("文件不在本次运行变更清单中".to_string());
        };
        let patch = if change.diff_available {
            Some(read_output_diff_file(&manifest, change)?)
        } else {
            None
        };
        let message = if change.diff_available {
            None
        } else if change.binary {
            Some("该文件是二进制文件或包含非文本内容，未生成 diff 预览。".to_string())
        } else {
            Some("该文件没有可用 diff 预览。".to_string())
        };
        Ok(Some(RunOutputDiffResponse {
            run_id: run.id,
            path: change.path.clone(),
            status: change.status.clone(),
            patch,
            binary: change.binary,
            diff_available: change.diff_available,
            diff_truncated: change.diff_truncated,
            message,
        }))
    }

    pub(in crate::services) async fn append_sandbox_event(
        &self,
        run: &TaskRunRecord,
        event_type: &str,
        message: impl Into<String>,
        payload: Option<Value>,
    ) {
        if let Err(err) = self
            .store
            .append_run_event(TaskRunEventRecord::new(
                run.id.clone(),
                event_type.to_string(),
                Some(message.into()),
                payload,
            ))
            .await
        {
            warn!(
                "failed to append sandbox event {} for run {}: {}",
                event_type, run.id, err
            );
        }
    }
}

fn sandbox_context_from_run(run: &TaskRunRecord) -> Result<Option<SandboxRuntimeContext>, String> {
    let Some(value) = run.input_snapshot.get("sandbox") else {
        return Ok(None);
    };
    serde_json::from_value::<SandboxRuntimeContext>(value.clone())
        .map(Some)
        .map_err(|err| format!("decode persisted sandbox context failed: {err}"))
}
