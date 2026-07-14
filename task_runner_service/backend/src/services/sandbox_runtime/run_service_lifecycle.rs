// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl RunService {
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
        let auth = if context.manager_auth_mode.as_deref() == Some("local_connector") {
            match (
                context.manager_client_key.clone(),
                context.manager_owner_user_id.clone(),
            ) {
                (Some(client_key), Some(owner_user_id)) => Some(SandboxManagerAuth {
                    client_id: context
                        .manager_client_id
                        .clone()
                        .unwrap_or_else(|| "task-runner".to_string()),
                    client_key,
                    mode: SandboxManagerAuthMode::LocalConnector,
                    owner_user_id: Some(owner_user_id),
                }),
                _ => None,
            }
        } else {
            SandboxManagerAuth::from_config(&self.config)
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
            Ok(response) => {
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
        match self
            .get_harness_run_output_changes(&run, limit, offset)
            .await
        {
            Ok(Some(response)) => return Ok(Some(response)),
            Ok(None) => {}
            Err(err) => warn!(
                run_id = run.id.as_str(),
                error = err.as_str(),
                "read Harness run changes failed; falling back to sandbox manifest"
            ),
        }
        let Some(manifest) = read_output_change_manifest_for_run(&run)? else {
            return Ok(Some(RunOutputChangesResponse {
                run_id: run.id,
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
