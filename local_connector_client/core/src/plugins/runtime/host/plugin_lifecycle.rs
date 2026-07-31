// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

impl PluginRuntimeHost {
    pub async fn dispatch_plugin_disabled(&self, plugin_id: &str) -> PluginDisabledHookReport {
        let started = Instant::now();
        let event_id = format!("plugin-disabled-{}", Uuid::new_v4());
        if let Ok(mut disabled_plugins) = self.disabled_plugins.lock() {
            disabled_plugins.insert(plugin_id.to_string());
        }
        let cancelled_sessions = self.cancel_plugin_sessions(plugin_id).await;
        let mut report = PluginDisabledHookReport {
            event_id: event_id.clone(),
            plugin_id: plugin_id.to_string(),
            release_id: None,
            artifact_sha256: None,
            cancelled_sessions,
            blocking_failures: 0,
            dispatches: Vec::new(),
            errors: Vec::new(),
        };
        let installer = self.skill_loader.installer();
        let installation = match installer.active_installation(plugin_id) {
            Ok(Some(installation)) => installation,
            Ok(None) => {
                self.record_plugin_disabled_telemetry(&report, started);
                return report;
            }
            Err(error) => {
                report
                    .errors
                    .push(sanitize_error(error.to_string().as_str()));
                self.record_plugin_disabled_telemetry(&report, started);
                return report;
            }
        };
        report.release_id = Some(installation.version.release_id.clone());
        report.artifact_sha256 = Some(installation.version.artifact_sha256.clone());
        let permission_snapshot = installation
            .version
            .inventory
            .permissions
            .iter()
            .map(|requirement| requirement.permission.clone())
            .collect::<BTreeSet<_>>();
        let summary_sha256 = plugin_disabled_summary_sha256(&installation);
        for component in installation
            .version
            .inventory
            .components
            .iter()
            .filter(|component| component.kind == PluginComponentKind::HookSet)
        {
            let result = async {
                let entrypoint = component
                    .entrypoint
                    .as_ref()
                    .context("Plugin Hook component entrypoint is missing")?;
                let relative_path = entrypoint.path.trim_start_matches("./");
                let expected_content_sha256 = installation
                    .version
                    .package_file_sha256
                    .get(relative_path)
                    .context("Plugin Hook source is not covered by package checksums")?;
                let snapshot = self.hook_loader.load(
                    plugin_id,
                    component.component_key.as_str(),
                    expected_content_sha256.as_str(),
                    &permission_snapshot,
                )?;
                self.hook_loader
                    .dispatch(
                        &snapshot,
                        &permission_snapshot,
                        event_id.as_str(),
                        PluginHookEvent::PluginDisabled,
                        &PluginHookEventContext {
                            component_key: Some(component.component_key.clone()),
                            outcome: Some(PluginHookOutcome::Succeeded),
                            summary_sha256: Some(summary_sha256.clone()),
                            ..PluginHookEventContext::default()
                        },
                        &BTreeMap::new(),
                    )
                    .await
            }
            .await;
            match result {
                Ok(dispatch) => {
                    report.blocking_failures = report
                        .blocking_failures
                        .saturating_add(usize::from(dispatch.blocking_failure));
                    report.errors.extend(
                        dispatch
                            .executions
                            .iter()
                            .filter(|execution| execution.matched && !execution.succeeded)
                            .map(|execution| {
                                format!(
                                    "PluginDisabled Hook {} failed for component {}",
                                    execution.hook_id, component.component_key
                                )
                            }),
                    );
                    report.dispatches.push(dispatch);
                }
                Err(error) => report.errors.push(sanitize_error(
                    format!(
                        "PluginDisabled Hook dispatch failed for component {}: {error}",
                        component.component_key
                    )
                    .as_str(),
                )),
            }
        }
        self.record_plugin_disabled_telemetry(&report, started);
        report
    }

    pub fn mark_plugin_enabled(&self, plugin_id: &str) {
        if let Ok(mut disabled_plugins) = self.disabled_plugins.lock() {
            disabled_plugins.remove(plugin_id);
        }
    }

    fn record_plugin_disabled_telemetry(
        &self,
        report: &PluginDisabledHookReport,
        started: Instant,
    ) {
        let identity = PluginRuntimeTelemetryIdentity {
            run_id: report.event_id.clone(),
            plugin_id: report.plugin_id.clone(),
            release_id: report
                .release_id
                .clone()
                .unwrap_or_else(|| "not-installed".to_string()),
            component_key: "plugin-disabled".to_string(),
        };
        let error = (!report.errors.is_empty()).then(|| report.errors.join("; "));
        self.telemetry().record_lifecycle_finished(
            &identity,
            "plugin_disabled",
            elapsed_millis(started),
            error.as_deref().map_or(Ok(()), Err),
        );
    }

    async fn cancel_plugin_sessions(&self, plugin_id: &str) -> usize {
        let removed = {
            let Ok(mut sessions) = self.sessions.lock() else {
                return 0;
            };
            let ids = sessions
                .iter()
                .filter(|(_, session)| session.plugin_id == plugin_id)
                .map(|(adapter_session_id, _)| adapter_session_id.clone())
                .collect::<Vec<_>>();
            ids.into_iter()
                .filter_map(|adapter_session_id| {
                    sessions
                        .remove(adapter_session_id.as_str())
                        .map(|session| (adapter_session_id, session))
                })
                .collect::<Vec<_>>()
        };
        let count = removed.len();
        for (adapter_session_id, session) in removed {
            let identity = session.telemetry_identity();
            self.telemetry()
                .record_cancel_started(&identity, adapter_session_id.as_str());
            let started = Instant::now();
            session
                .native_action_cancelled
                .store(true, Ordering::SeqCst);
            if let Some(mcp) = &session.mcp {
                mcp.cancel();
            }
            cancel_pending_approvals_for_session(
                adapter_session_id.as_str(),
                "Plugin was disabled by the user",
            )
            .await;
            clear_session_approvals(adapter_session_id.as_str()).await;
            self.telemetry().record_cancelled(
                &identity,
                adapter_session_id.as_str(),
                elapsed_millis(started),
            );
        }
        count
    }
}
