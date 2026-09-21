// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Value};

mod command_evidence;
mod manifest_evidence;

use self::command_evidence::*;
use self::manifest_evidence::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeSupplyChainPolicy {
    pub(crate) baseline_revision: String,
    pub(crate) dependency_requirements: BTreeMap<String, String>,
    pub(crate) audit_level: String,
    pub(crate) install_script_allowlist: BTreeSet<String>,
    pub(crate) install_registry: String,
    pub(crate) audit_registry: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub(in crate::services) struct SupplyChainEvidenceState {
    node_project_observed: bool,
    dependency_activity_observed: bool,
    lockfile_observed: bool,
    package_manager: Option<String>,
    install: Option<CommandEvidence>,
    unsafe_install_commands: Vec<String>,
    rebuilds: Vec<RebuildEvidence>,
    audit: Option<AuditEvidence>,
    package_manifest: Option<NodePackageManifestEvidence>,
    pending_package_manifest_events: BTreeMap<String, PackageManifestSessionEvent>,
    staged_package_manifest_updates: BTreeMap<String, Option<NodePackageManifestEvidence>>,
    pending_terminal_commands: BTreeMap<String, String>,
    #[serde(default)]
    inherited_from_run_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SupplyChainEvidenceReceipt {
    baseline_revision: String,
    status: String,
    evidence: SupplyChainEvidenceState,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct NodePackageManifestEvidence {
    requirements: BTreeMap<String, String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum PackageManifestSessionEvent {
    Stage {
        session_id: String,
        update: Option<NodePackageManifestEvidence>,
    },
    Commit {
        session_id: String,
    },
    Abort {
        session_id: String,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CommandEvidence {
    command: String,
    exit_code: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AuditEvidence {
    command: String,
    exit_code: Option<i64>,
    output_truncated: bool,
    vulnerabilities: Option<NodeVulnerabilityCounts>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RebuildEvidence {
    command: String,
    packages: Vec<String>,
    completed_successfully: bool,
}

#[derive(Debug, Clone)]
struct TerminalCommandResult {
    command: String,
    exit_code: Option<i64>,
    output: String,
    output_truncated: bool,
}

#[derive(Debug, Clone)]
struct TerminalWaitResult {
    process_id: String,
    exit_code: Option<i64>,
    output: String,
    output_truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, PartialEq, Eq)]
pub(super) struct NodeVulnerabilityCounts {
    pub(super) total: u64,
    pub(super) info: u64,
    pub(super) low: u64,
    pub(super) moderate: u64,
    pub(super) high: u64,
    pub(super) critical: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::services) struct SupplyChainAuditReport {
    pub(in crate::services) applicable: bool,
    pub(in crate::services) status: &'static str,
    pub(super) baseline_revision: String,
    pub(super) audit_level: String,
    pub(super) package_manager: Option<String>,
    pub(super) lockfile_observed: bool,
    pub(super) install_command: Option<String>,
    pub(super) install_exit_code: Option<i64>,
    pub(super) approved_install_script_packages: Vec<String>,
    pub(super) audit_command: Option<String>,
    pub(super) audit_exit_code: Option<i64>,
    pub(super) vulnerabilities: Option<NodeVulnerabilityCounts>,
    pub(super) dependency_baseline_verified: bool,
    pub(super) dependency_baseline_violations: Vec<String>,
    pub(super) blocking_reasons: Vec<String>,
}

impl SupplyChainEvidenceState {
    pub(in crate::services) fn inherit_for_run(
        run: &crate::models::TaskRunRecord,
        policy: &NodeSupplyChainPolicy,
    ) -> Option<Self> {
        let workspace = run.workspace_execution.as_ref()?;
        let execution_group_id = workspace.execution_group_id.as_deref()?;
        let execution_base_commit = workspace.execution_base_commit.as_deref()?;
        let prerequisites = run
            .input_snapshot
            .get("resolved_prerequisites")
            .and_then(Value::as_array)?;
        prerequisites.iter().rev().find_map(|prerequisite| {
            if prerequisite
                .get("execution_group_id")
                .and_then(Value::as_str)
                != Some(execution_group_id)
                || prerequisite
                    .get("integrated_commit")
                    .and_then(Value::as_str)
                    != Some(execution_base_commit)
            {
                return None;
            }
            let run_id = prerequisite.get("run_id").and_then(Value::as_str)?;
            let receipt = prerequisite.get("supply_chain_receipt")?.clone();
            let receipt = serde_json::from_value::<SupplyChainEvidenceReceipt>(receipt).ok()?;
            if receipt.status != "passed" || receipt.baseline_revision != policy.baseline_revision {
                return None;
            }
            let mut evidence = receipt.evidence;
            evidence.pending_package_manifest_events.clear();
            evidence.staged_package_manifest_updates.clear();
            evidence.pending_terminal_commands.clear();
            evidence.inherited_from_run_id = Some(run_id.to_string());
            Some(evidence)
        })
    }

    pub(in crate::services) fn passed_receipt(
        &self,
        policy: &NodeSupplyChainPolicy,
        report: &SupplyChainAuditReport,
    ) -> Option<Value> {
        (report.status == "passed").then(|| {
            serde_json::to_value(SupplyChainEvidenceReceipt {
                baseline_revision: policy.baseline_revision.clone(),
                status: "passed".to_string(),
                evidence: self.clone(),
            })
            .unwrap_or(Value::Null)
        })
    }

    fn invalidate_inherited_evidence(&mut self) {
        if self.inherited_from_run_id.take().is_none() {
            return;
        }
        self.install = None;
        self.audit = None;
        self.rebuilds.clear();
        self.unsafe_install_commands.clear();
        self.lockfile_observed = false;
    }

    pub(super) fn observe_tool_calls(&mut self, payload: &Value) {
        let Some(calls) = payload.as_array() else {
            return;
        };
        for call in calls {
            let Some(invocation_id) = call.get("invocation_id").and_then(Value::as_str) else {
                continue;
            };
            let Some(name) = chatos_ai_runtime::tool_call::extract_tool_call_name(call) else {
                continue;
            };
            let arguments = chatos_ai_runtime::tool_call::clone_tool_call_arguments(call);
            let arguments = arguments
                .as_str()
                .and_then(|value| serde_json::from_str::<Value>(value).ok())
                .unwrap_or(arguments);
            let Some(event) = package_manifest_event_from_tool_call(name, &arguments) else {
                continue;
            };
            self.pending_package_manifest_events
                .insert(invocation_id.to_string(), event);
        }
    }

    pub(super) fn observe_tool_result(&mut self, payload: &Value) {
        let mut applied_manifest_update = false;
        if let Some(invocation_id) = payload.get("invocation_id").and_then(Value::as_str) {
            let succeeded = payload.get("success").and_then(Value::as_bool) == Some(true)
                && payload.get("is_error").and_then(Value::as_bool) != Some(true);
            if let Some(event) = self.pending_package_manifest_events.remove(invocation_id) {
                match event {
                    PackageManifestSessionEvent::Stage { session_id, update } if succeeded => {
                        self.staged_package_manifest_updates
                            .insert(session_id, update);
                    }
                    PackageManifestSessionEvent::Commit { session_id } => {
                        let update = self.staged_package_manifest_updates.remove(&session_id);
                        if succeeded {
                            if let Some(update) = update {
                                self.invalidate_inherited_evidence();
                                applied_manifest_update = true;
                                self.node_project_observed = true;
                                self.dependency_activity_observed = true;
                                self.package_manifest = update;
                            }
                        }
                    }
                    PackageManifestSessionEvent::Abort { session_id } if succeeded => {
                        self.staged_package_manifest_updates.remove(&session_id);
                    }
                    PackageManifestSessionEvent::Stage { .. }
                    | PackageManifestSessionEvent::Abort { .. } => {}
                }
            }
        }
        if payload.get("success").and_then(Value::as_bool) == Some(true)
            && payload.get("is_error").and_then(Value::as_bool) != Some(true)
        {
            observe_project_paths(payload, self);
            if !applied_manifest_update && result_mutates_package_manifest(payload) {
                self.node_project_observed = true;
                self.dependency_activity_observed = true;
                self.package_manifest = None;
            }
            if result_mutates_node_dependency_files(payload) {
                self.invalidate_inherited_evidence();
                self.node_project_observed = true;
                self.dependency_activity_observed = true;
            }
            if let Some(manifest) = package_manifest_from_tool_result(payload) {
                self.node_project_observed = true;
                self.package_manifest = Some(manifest);
            }
        }
        let Some(name) = payload.get("name").and_then(Value::as_str) else {
            return;
        };
        if name.ends_with("terminal_controller_process_wait") {
            let Some(result) = terminal_wait_result(payload) else {
                return;
            };
            let Some(command) = self
                .pending_terminal_commands
                .remove(result.process_id.as_str())
            else {
                return;
            };
            if result.exit_code.is_none() {
                self.pending_terminal_commands
                    .insert(result.process_id, command);
                return;
            }
            self.observe_terminal_command_result(TerminalCommandResult {
                command,
                exit_code: result.exit_code,
                output: result.output,
                output_truncated: result.output_truncated,
            });
            return;
        }
        if !name.ends_with("terminal_controller_execute_command") {
            return;
        }
        let Some(result) = terminal_result(payload) else {
            return;
        };
        if result.exit_code.is_none() {
            if let Some(process_id) = terminal_process_id(payload) {
                self.pending_terminal_commands
                    .insert(process_id, result.command);
            }
            return;
        }
        self.observe_terminal_command_result(result);
    }

    fn observe_terminal_command_result(&mut self, result: TerminalCommandResult) {
        let command = result.command.as_str();
        let normalized = command.to_ascii_lowercase();
        let exit_code = result.exit_code;

        if let Some(manager) = node_package_manager(&normalized) {
            self.node_project_observed = true;
            self.package_manager = Some(manager.to_string());
        }
        if is_lockfile_command(&normalized) {
            self.dependency_activity_observed = true;
            self.lockfile_observed = exit_code == Some(0);
        }
        if is_node_install_command(&normalized) {
            self.dependency_activity_observed = true;
            let succeeded = exit_code == Some(0) && !command_masks_failure(command);
            if succeeded
                || self.install.as_ref().is_none_or(|install| {
                    install.exit_code != Some(0) || command_masks_failure(install.command.as_str())
                })
            {
                self.install = Some(CommandEvidence {
                    command: command.to_string(),
                    exit_code,
                });
            }
            if exit_code == Some(0) && !normalized.contains("--no-package-lock") {
                self.lockfile_observed = true;
            }
            if !install_scripts_are_disabled(&normalized, self.package_manager.as_deref()) {
                self.unsafe_install_commands.push(command.to_string());
            }
        }
        if let Some(packages) = approved_rebuild_packages(&normalized) {
            self.dependency_activity_observed = true;
            self.rebuilds.push(RebuildEvidence {
                command: command.to_string(),
                packages,
                completed_successfully: rebuild_completed_successfully(
                    command,
                    exit_code,
                    result.output.as_str(),
                ),
            });
        }
        if is_node_audit_command(&normalized) {
            self.dependency_activity_observed = true;
            self.audit = Some(AuditEvidence {
                command: command.to_string(),
                exit_code,
                output_truncated: result.output_truncated,
                vulnerabilities: parse_vulnerability_counts(result.output.as_str()),
            });
        }
    }

    pub(in crate::services) fn evaluate(
        &self,
        policy: &NodeSupplyChainPolicy,
    ) -> SupplyChainAuditReport {
        if !self.node_project_observed || !self.dependency_activity_observed {
            return SupplyChainAuditReport {
                applicable: false,
                status: "not_applicable",
                baseline_revision: policy.baseline_revision.clone(),
                audit_level: policy.audit_level.clone(),
                package_manager: None,
                lockfile_observed: false,
                install_command: None,
                install_exit_code: None,
                approved_install_script_packages: Vec::new(),
                audit_command: None,
                audit_exit_code: None,
                vulnerabilities: None,
                dependency_baseline_verified: false,
                dependency_baseline_violations: Vec::new(),
                blocking_reasons: Vec::new(),
            };
        }

        let mut blocking_reasons = Vec::new();
        let dependency_baseline_violations = self
            .package_manifest
            .as_ref()
            .map(|manifest| dependency_baseline_violations(manifest, policy))
            .unwrap_or_default();
        let dependency_baseline_verified =
            self.package_manifest.is_some() && dependency_baseline_violations.is_empty();
        if self.package_manifest.is_none() {
            blocking_reasons.push(
                "Node.js dependency baseline was not verified from the final package.json"
                    .to_string(),
            );
        } else if !dependency_baseline_violations.is_empty() {
            blocking_reasons.extend(
                dependency_baseline_violations
                    .iter()
                    .map(|violation| format!("Node.js dependency baseline violation: {violation}")),
            );
        }
        if !self.lockfile_observed {
            blocking_reasons.push("Node.js dependency lockfile was not verified".to_string());
        }
        match self.install.as_ref() {
            Some(install)
                if install.exit_code == Some(0)
                    && !command_masks_failure(install.command.as_str()) =>
            {
                if self.package_manager.as_deref() == Some("npm")
                    && !policy.install_registry.trim().is_empty()
                    && !command_uses_registry(
                        install.command.as_str(),
                        policy.install_registry.as_str(),
                    )
                {
                    blocking_reasons.push(format!(
                        "Node.js dependency installation did not use the configured registry `{}`",
                        policy.install_registry
                    ));
                }
            }
            Some(_) => blocking_reasons.push("Node.js dependency installation failed".to_string()),
            None => blocking_reasons.push(
                "Node.js dependency installation was not executed with recorded evidence"
                    .to_string(),
            ),
        }
        if !self.unsafe_install_commands.is_empty() {
            blocking_reasons.push(format!(
                "Node.js dependency installation executed scripts outside the approved policy: {}",
                self.unsafe_install_commands.join("; ")
            ));
        }
        let rebuilt_packages = self
            .rebuilds
            .iter()
            .flat_map(|rebuild| rebuild.packages.iter().cloned())
            .collect::<BTreeSet<_>>();
        let unapproved_rebuilds = rebuilt_packages
            .difference(&policy.install_script_allowlist)
            .cloned()
            .collect::<Vec<_>>();
        if !unapproved_rebuilds.is_empty() {
            blocking_reasons.push(format!(
                "Node.js install scripts were requested for packages outside the allowlist: {}",
                unapproved_rebuilds.join(", ")
            ));
        }
        let successfully_rebuilt_packages = self
            .rebuilds
            .iter()
            .filter(|rebuild| rebuild.completed_successfully)
            .flat_map(|rebuild| rebuild.packages.iter().cloned())
            .collect::<BTreeSet<_>>();
        let failed_rebuilds = self
            .rebuilds
            .iter()
            .filter(|rebuild| {
                !rebuild.completed_successfully
                    && rebuild
                        .packages
                        .iter()
                        .any(|package| !successfully_rebuilt_packages.contains(package))
            })
            .map(|rebuild| rebuild.command.clone())
            .collect::<Vec<_>>();
        if !failed_rebuilds.is_empty() {
            blocking_reasons.push(format!(
                "Node.js approved install scripts did not complete successfully: {}",
                failed_rebuilds.join("; ")
            ));
        }

        match self.audit.as_ref() {
            Some(audit)
                if !audit_command_matches_level(&audit.command, policy.audit_level.as_str()) =>
            {
                blocking_reasons.push(format!(
                    "Node.js dependency audit was not executed with JSON output at the required `{}` level",
                    policy.audit_level
                ));
            }
            Some(audit) if command_masks_failure(audit.command.as_str()) => {
                blocking_reasons
                    .push("Node.js dependency audit command masked its failure status".to_string());
            }
            Some(audit) if audit.output_truncated => {
                blocking_reasons
                    .push("Node.js dependency audit JSON output was truncated".to_string());
            }
            Some(audit) if audit.vulnerabilities.is_none() => {
                blocking_reasons.push(
                    "Node.js dependency audit output did not contain a complete JSON `metadata.vulnerabilities` object"
                        .to_string(),
                );
            }
            Some(audit) => {
                if self.package_manager.as_deref() == Some("npm")
                    && !policy.audit_registry.trim().is_empty()
                    && !command_uses_registry(
                        audit.command.as_str(),
                        policy.audit_registry.as_str(),
                    )
                {
                    blocking_reasons.push(format!(
                        "Node.js dependency audit did not use the configured audit registry `{}`",
                        policy.audit_registry
                    ));
                }
                let vulnerabilities = audit.vulnerabilities.as_ref().expect("checked above");
                if vulnerabilities.high > 0 || vulnerabilities.critical > 0 {
                    blocking_reasons.push(format!(
                        "Node.js dependency audit found {} high and {} critical vulnerabilities",
                        vulnerabilities.high, vulnerabilities.critical
                    ));
                } else if audit.exit_code != Some(0) {
                    blocking_reasons.push(format!(
                        "Node.js dependency audit exited with code {}",
                        audit
                            .exit_code
                            .map(|code| code.to_string())
                            .unwrap_or_else(|| "unknown".to_string())
                    ));
                }
            }
            None => blocking_reasons.push(
                "Node.js dependency audit was not executed with recorded evidence".to_string(),
            ),
        }

        SupplyChainAuditReport {
            applicable: true,
            status: if blocking_reasons.is_empty() {
                "passed"
            } else {
                "blocked"
            },
            baseline_revision: policy.baseline_revision.clone(),
            audit_level: policy.audit_level.clone(),
            package_manager: self.package_manager.clone(),
            lockfile_observed: self.lockfile_observed,
            install_command: self.install.as_ref().map(|item| item.command.clone()),
            install_exit_code: self.install.as_ref().and_then(|item| item.exit_code),
            approved_install_script_packages: rebuilt_packages.into_iter().collect(),
            audit_command: self.audit.as_ref().map(|item| item.command.clone()),
            audit_exit_code: self.audit.as_ref().and_then(|item| item.exit_code),
            vulnerabilities: self
                .audit
                .as_ref()
                .and_then(|item| item.vulnerabilities.clone()),
            dependency_baseline_verified,
            dependency_baseline_violations,
            blocking_reasons,
        }
    }
}

impl SupplyChainAuditReport {
    pub(super) fn evidence_summary(&self) -> String {
        let Some(vulnerabilities) = self.vulnerabilities.as_ref() else {
            return format!(
                "Node.js supply-chain audit status: {}; baseline {}; dependency baseline verified={}; audit evidence incomplete",
                self.status, self.baseline_revision, self.dependency_baseline_verified
            );
        };
        format!(
            "Node.js supply-chain audit status: {}; baseline {}; dependency baseline verified={}; command `{}` exited {}; vulnerabilities total={}, info={}, low={}, moderate={}, high={}, critical={}",
            self.status,
            self.baseline_revision,
            self.dependency_baseline_verified,
            self.audit_command.as_deref().unwrap_or("not executed"),
            self.audit_exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            vulnerabilities.total,
            vulnerabilities.info,
            vulnerabilities.low,
            vulnerabilities.moderate,
            vulnerabilities.high,
            vulnerabilities.critical,
        )
    }

    pub(in crate::services) fn event_payload(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| json!({"status": "serialization_failed"}))
    }
}

pub(super) fn policy_guidance(policy: &NodeSupplyChainPolicy) -> Value {
    let allowlist = if policy.install_script_allowlist.is_empty() {
        "none".to_string()
    } else {
        policy
            .install_script_allowlist
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let dependency_requirements = serde_json::to_string(&policy.dependency_requirements)
        .expect("managed dependency requirements must serialize");
    json!({
        "type": "message",
        "role": "system",
        "content": format!(
            "[Node.js supply-chain requirements]\nFor any Node.js project, keep the dependency lockfile and use these exact centrally reviewed requirements whenever the package is present: {dependency_requirements}. After the final dependency change, read the complete package.json so the runtime can verify the baseline. Install dependencies with lifecycle scripts disabled using registry `{}` (for npm: `npm ci --ignore-scripts --registry={}`), run lifecycle scripts only for these approved packages: {allowlist}, and finish with a JSON dependency audit at `{}` severity using the independently configured audit registry `{}` (for npm: `npm audit --audit-level={} --json --registry={}`). The active dependency baseline revision is `{}`. A Node.js implementation is not complete until the final package.json, installation, and audit commands have successful, parseable tool evidence and high/critical vulnerabilities are zero.",
            policy.install_registry,
            policy.install_registry,
            policy.audit_level,
            policy.audit_registry,
            policy.audit_level,
            policy.audit_registry,
            policy.baseline_revision,
        ),
    })
}

#[cfg(test)]
include!("supply_chain_inline_tests.rs");
