// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeSupplyChainPolicy {
    pub(crate) baseline_revision: String,
    pub(crate) dependency_requirements: BTreeMap<String, String>,
    pub(crate) audit_level: String,
    pub(crate) install_script_allowlist: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct SupplyChainEvidenceState {
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
}

#[derive(Debug, Clone)]
struct NodePackageManifestEvidence {
    requirements: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
struct CommandEvidence {
    command: String,
    exit_code: Option<i64>,
}

#[derive(Debug, Clone)]
struct AuditEvidence {
    command: String,
    exit_code: Option<i64>,
    output_truncated: bool,
    vulnerabilities: Option<NodeVulnerabilityCounts>,
}

#[derive(Debug, Clone)]
struct RebuildEvidence {
    command: String,
    exit_code: Option<i64>,
    packages: Vec<String>,
}

#[derive(Debug, Clone)]
struct TerminalCommandResult {
    command: String,
    exit_code: Option<i64>,
    output: String,
    output_truncated: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(super) struct NodeVulnerabilityCounts {
    pub(super) total: u64,
    pub(super) info: u64,
    pub(super) low: u64,
    pub(super) moderate: u64,
    pub(super) high: u64,
    pub(super) critical: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(super) struct SupplyChainAuditReport {
    pub(super) applicable: bool,
    pub(super) status: &'static str,
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
        if !name.ends_with("terminal_controller_execute_command") {
            return;
        }
        let Some(result) = terminal_result(payload) else {
            return;
        };
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
            self.install = Some(CommandEvidence {
                command: command.to_string(),
                exit_code,
            });
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
                exit_code,
                packages,
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

    pub(super) fn evaluate(&self, policy: &NodeSupplyChainPolicy) -> SupplyChainAuditReport {
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
                    && !command_masks_failure(install.command.as_str()) => {}
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
        let failed_rebuilds = self
            .rebuilds
            .iter()
            .filter(|rebuild| {
                rebuild.exit_code != Some(0) || command_masks_failure(rebuild.command.as_str())
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

    pub(super) fn event_payload(&self) -> Value {
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
            "[Node.js supply-chain requirements]\nFor any Node.js project, keep the dependency lockfile and use these exact centrally reviewed requirements whenever the package is present: {dependency_requirements}. After the final dependency change, read the complete package.json so the runtime can verify the baseline. Install dependencies with lifecycle scripts disabled, run lifecycle scripts only for these approved packages: {allowlist}, and finish with a JSON dependency audit at `{}` severity. The active dependency baseline revision is `{}`. A Node.js implementation is not complete until the final package.json, installation, and audit commands have successful, parseable tool evidence and high/critical vulnerabilities are zero.",
            policy.audit_level,
            policy.baseline_revision,
        ),
    })
}

fn package_manifest_event_from_tool_call(
    name: &str,
    arguments: &Value,
) -> Option<PackageManifestSessionEvent> {
    let session_id = arguments.get("session_id").and_then(Value::as_str)?;
    if name.ends_with("commit_edit_session") {
        return Some(PackageManifestSessionEvent::Commit {
            session_id: session_id.to_string(),
        });
    }
    if name.ends_with("abort_edit_session") {
        return Some(PackageManifestSessionEvent::Abort {
            session_id: session_id.to_string(),
        });
    }
    if !name.ends_with("stage_edit_batch") {
        return None;
    }
    let operations = arguments.get("operations").and_then(Value::as_array)?;
    let mut touched = false;
    let mut update = None;
    for operation in operations {
        let path = operation.get("path").and_then(Value::as_str)?;
        if !path.replace('\\', "/").ends_with("package.json") {
            continue;
        }
        touched = true;
        update = if operation.get("kind").and_then(Value::as_str) == Some("write") {
            operation
                .get("content")
                .and_then(Value::as_str)
                .and_then(parse_package_manifest)
        } else {
            None
        };
    }
    touched.then(|| PackageManifestSessionEvent::Stage {
        session_id: session_id.to_string(),
        update,
    })
}

fn result_mutates_package_manifest(payload: &Value) -> bool {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    name.ends_with("commit_edit_session") && value_mentions_package_manifest(payload)
}

fn result_mutates_node_dependency_files(payload: &Value) -> bool {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    name.ends_with("commit_edit_session") && value_mentions_node_dependency_file(payload)
}

fn value_mentions_node_dependency_file(value: &Value) -> bool {
    match value {
        Value::String(value) => {
            let normalized = value.replace('\\', "/").to_ascii_lowercase();
            normalized.ends_with("package.json")
                || [
                    "package-lock.json",
                    "pnpm-lock.yaml",
                    "yarn.lock",
                    "bun.lock",
                    "bun.lockb",
                ]
                .iter()
                .any(|lockfile| normalized.ends_with(lockfile))
        }
        Value::Array(items) => items.iter().any(value_mentions_node_dependency_file),
        Value::Object(map) => map.values().any(value_mentions_node_dependency_file),
        _ => false,
    }
}

fn value_mentions_package_manifest(value: &Value) -> bool {
    match value {
        Value::String(value) => value.replace('\\', "/").contains("package.json"),
        Value::Array(items) => items.iter().any(value_mentions_package_manifest),
        Value::Object(map) => map.values().any(value_mentions_package_manifest),
        _ => false,
    }
}

fn package_manifest_from_tool_result(payload: &Value) -> Option<NodePackageManifestEvidence> {
    let content = payload.get("content")?.as_str()?;
    let result = serde_json::from_str::<Value>(content).ok()?;
    let path = result.get("path").and_then(Value::as_str)?;
    if !path.replace('\\', "/").ends_with("package.json") {
        return None;
    }
    parse_package_manifest(result.get("content")?.as_str()?)
}

fn parse_package_manifest(content: &str) -> Option<NodePackageManifestEvidence> {
    let manifest = serde_json::from_str::<Value>(content).ok()?;
    let mut requirements = BTreeMap::new();
    for section in [
        "dependencies",
        "devDependencies",
        "peerDependencies",
        "optionalDependencies",
    ] {
        let Some(entries) = manifest.get(section).and_then(Value::as_object) else {
            continue;
        };
        for (name, requirement) in entries {
            let requirement = requirement.as_str()?.trim();
            if name.trim().is_empty() || requirement.is_empty() {
                return None;
            }
            requirements.insert(name.to_string(), requirement.to_string());
        }
    }
    Some(NodePackageManifestEvidence { requirements })
}

fn dependency_baseline_violations(
    manifest: &NodePackageManifestEvidence,
    policy: &NodeSupplyChainPolicy,
) -> Vec<String> {
    manifest
        .requirements
        .iter()
        .filter_map(|(name, actual)| {
            let expected = policy.dependency_requirements.get(name)?;
            (actual != expected)
                .then(|| format!("{name} requires `{actual}` but baseline requires `{expected}`"))
        })
        .collect()
}

fn observe_project_paths(value: &Value, evidence: &mut SupplyChainEvidenceState) {
    match value {
        Value::String(value) => {
            let normalized = value.replace('\\', "/").to_ascii_lowercase();
            if normalized.ends_with("package.json") {
                evidence.node_project_observed = true;
            }
            if [
                "package-lock.json",
                "pnpm-lock.yaml",
                "yarn.lock",
                "bun.lock",
                "bun.lockb",
            ]
            .iter()
            .any(|lockfile| normalized.ends_with(lockfile))
            {
                evidence.node_project_observed = true;
                evidence.lockfile_observed = true;
            }
        }
        Value::Array(items) => {
            for item in items {
                observe_project_paths(item, evidence);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                observe_project_paths(item, evidence);
            }
        }
        _ => {}
    }
}

fn terminal_result(payload: &Value) -> Option<TerminalCommandResult> {
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .and_then(|content| serde_json::from_str::<Value>(content).ok())
        .filter(Value::is_object);
    let result = payload.get("result").filter(|value| value.is_object());
    let command = content
        .as_ref()
        .and_then(|value| value.get("common").or_else(|| value.get("command")))
        .and_then(Value::as_str)
        .or_else(|| {
            result
                .and_then(|value| value.get("common").or_else(|| value.get("command")))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|command| !command.is_empty())?;
    let exit_code = result
        .and_then(|value| value.get("exit_code"))
        .and_then(Value::as_i64)
        .or_else(|| {
            content
                .as_ref()
                .and_then(|value| value.get("exit_code"))
                .and_then(Value::as_i64)
        });
    let output = result
        .and_then(|value| value.get("output"))
        .and_then(Value::as_str)
        .or_else(|| {
            content
                .as_ref()
                .and_then(|value| value.get("output"))
                .and_then(Value::as_str)
        })
        .unwrap_or_default();
    let output_truncated = result
        .and_then(|value| value.get("truncated"))
        .and_then(Value::as_bool)
        .or_else(|| {
            content
                .as_ref()
                .and_then(|value| value.get("truncated"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    Some(TerminalCommandResult {
        command: command.to_string(),
        exit_code,
        output: output.to_string(),
        output_truncated,
    })
}

fn node_package_manager(command: &str) -> Option<&'static str> {
    if command_invokes_executable(command, "pnpm") {
        Some("pnpm")
    } else if command_invokes_executable(command, "yarn") {
        Some("yarn")
    } else if command_invokes_executable(command, "bun") {
        Some("bun")
    } else if command_invokes_executable(command, "npm") {
        Some("npm")
    } else {
        None
    }
}

fn is_node_install_command(command: &str) -> bool {
    [
        &["npm", "ci"][..],
        &["npm", "install"][..],
        &["pnpm", "install"][..],
        &["yarn", "install"][..],
        &["bun", "install"][..],
    ]
    .iter()
    .any(|invocation| command_invocation_segment(command, invocation).is_some())
        && !command.contains("--package-lock-only")
}

fn install_scripts_are_disabled(command: &str, package_manager: Option<&str>) -> bool {
    let install_segment = [
        &["npm", "ci"][..],
        &["npm", "install"][..],
        &["pnpm", "install"][..],
        &["yarn", "install"][..],
        &["bun", "install"][..],
    ]
    .iter()
    .find_map(|invocation| command_invocation_segment(command, invocation))
    .unwrap_or(command);
    match package_manager {
        Some("yarn") => {
            install_segment.contains("--mode=skip-builds")
                || install_segment.contains("--mode skip-builds")
        }
        Some("npm" | "pnpm" | "bun") | None => install_segment.contains("--ignore-scripts"),
        Some(_) => false,
    }
}

fn command_masks_failure(command: &str) -> bool {
    let compact = command
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    compact.contains("||true") || compact.contains(";true")
}

fn is_lockfile_command(command: &str) -> bool {
    command_invocation_segment(command, &["npm", "install"])
        .is_some_and(|segment| segment.contains("--package-lock-only"))
        || command_invocation_segment(command, &["pnpm", "install"])
            .is_some_and(|segment| segment.contains("--lockfile-only"))
        || command_invocation_segment(command, &["yarn", "install"])
            .is_some_and(|segment| segment.contains("--mode=update-lockfile"))
}

fn approved_rebuild_packages(command: &str) -> Option<Vec<String>> {
    let segment = command_invocation_segment(command, &["npm", "rebuild"])
        .or_else(|| command_invocation_segment(command, &["pnpm", "rebuild"]))?;
    let packages = segment
        .split_whitespace()
        .skip(2)
        .take_while(|value| !value.starts_with('-') && !matches!(*value, "&&" | ";" | "||"))
        .map(|value| value.trim_matches(|character| matches!(character, '\'' | '"')))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Some(packages)
}

fn is_node_audit_command(command: &str) -> bool {
    command_invocation_segment(command, &["npm", "audit"]).is_some()
        || command_invocation_segment(command, &["pnpm", "audit"]).is_some()
        || command_invocation_segment(command, &["yarn", "npm", "audit"]).is_some()
}

fn audit_command_matches_level(command: &str, audit_level: &str) -> bool {
    let command = command.to_ascii_lowercase();
    let Some(segment) = command_invocation_segment(&command, &["npm", "audit"])
        .or_else(|| command_invocation_segment(&command, &["pnpm", "audit"]))
        .or_else(|| command_invocation_segment(&command, &["yarn", "npm", "audit"]))
    else {
        return false;
    };
    segment.contains("--json")
        && (segment.contains(format!("--audit-level={audit_level}").as_str())
            || segment.contains(format!("--audit-level {audit_level}").as_str())
            || segment.contains(format!("--severity={audit_level}").as_str())
            || segment.contains(format!("--severity {audit_level}").as_str()))
}

fn parse_vulnerability_counts(output: &str) -> Option<NodeVulnerabilityCounts> {
    if let Ok(value) = serde_json::from_str::<Value>(output.trim()) {
        if let Some(counts) = vulnerability_counts_from_value(&value) {
            return Some(counts);
        }
    }
    for (start, character) in output.char_indices() {
        if character != '{' {
            continue;
        }
        let mut values = serde_json::Deserializer::from_str(&output[start..]).into_iter::<Value>();
        let Some(Ok(value)) = values.next() else {
            continue;
        };
        if let Some(counts) = vulnerability_counts_from_value(&value) {
            return Some(counts);
        }
    }
    None
}

fn vulnerability_counts_from_value(value: &Value) -> Option<NodeVulnerabilityCounts> {
    let vulnerabilities = value.pointer("/metadata/vulnerabilities")?;
    Some(NodeVulnerabilityCounts {
        total: vulnerabilities.get("total")?.as_u64()?,
        info: vulnerabilities.get("info")?.as_u64()?,
        low: vulnerabilities.get("low")?.as_u64()?,
        moderate: vulnerabilities.get("moderate")?.as_u64()?,
        high: vulnerabilities.get("high")?.as_u64()?,
        critical: vulnerabilities.get("critical")?.as_u64()?,
    })
}

fn command_invokes_executable(command: &str, executable: &str) -> bool {
    shell_command_segments(command).any(|segment| {
        command_tokens(segment)
            .first()
            .is_some_and(|token| executable_name(token) == executable)
    })
}

fn command_invocation_segment<'a>(command: &'a str, invocation: &[&str]) -> Option<&'a str> {
    shell_command_segments(command).find(|segment| {
        let tokens = command_tokens(segment);
        tokens.len() >= invocation.len()
            && tokens
                .iter()
                .zip(invocation)
                .enumerate()
                .all(|(index, (token, expected))| {
                    if index == 0 {
                        executable_name(token) == *expected
                    } else {
                        token.trim_matches(|character| matches!(character, '\'' | '"')) == *expected
                    }
                })
    })
}

fn shell_command_segments(command: &str) -> impl Iterator<Item = &str> {
    command
        .split(['\n', ';'])
        .flat_map(|segment| segment.split("&&"))
        .flat_map(|segment| segment.split("||"))
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
}

fn command_tokens(segment: &str) -> Vec<&str> {
    let tokens = segment.split_whitespace().collect::<Vec<_>>();
    let mut start = 0;
    while let Some(token) = tokens.get(start) {
        let cleaned = token.trim_matches(|character| matches!(character, '(' | '{'));
        if matches!(cleaned, "then" | "do" | "!" | "env" | "command")
            || (cleaned.contains('=') && !cleaned.starts_with('-'))
        {
            start += 1;
            continue;
        }
        break;
    }
    tokens[start..].to_vec()
}

fn executable_name(token: &str) -> &str {
    token
        .trim_matches(|character| matches!(character, '(' | '{' | '\'' | '"'))
        .rsplit('/')
        .next()
        .unwrap_or(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> NodeSupplyChainPolicy {
        NodeSupplyChainPolicy {
            baseline_revision: "baseline-2026-08".to_string(),
            dependency_requirements: BTreeMap::from([
                ("react".to_string(), "^19.2.7".to_string()),
                ("vite".to_string(), "^8.1.4".to_string()),
            ]),
            audit_level: "high".to_string(),
            install_script_allowlist: BTreeSet::from(["esbuild".to_string()]),
        }
    }

    fn evidence_with_manifest() -> SupplyChainEvidenceState {
        SupplyChainEvidenceState {
            node_project_observed: true,
            package_manifest: Some(NodePackageManifestEvidence {
                requirements: BTreeMap::from([
                    ("react".to_string(), "^19.2.7".to_string()),
                    ("vite".to_string(), "^8.1.4".to_string()),
                ]),
            }),
            ..SupplyChainEvidenceState::default()
        }
    }

    fn terminal_result(command: &str, exit_code: i64, output: &str) -> Value {
        json!({
            "name": "sandbox_terminal_controller_execute_command",
            "success": exit_code == 0,
            "is_error": exit_code != 0,
            "result": {
                "common": command,
                "exit_code": exit_code,
                "output": output,
            }
        })
    }

    fn split_terminal_result(command: &str, exit_code: i64, output: &str) -> Value {
        json!({
            "name": "sandbox_terminal_controller_execute_command",
            "success": exit_code == 0,
            "is_error": exit_code != 0,
            "content": serde_json::to_string(&json!({
                "common": command,
                "output": output,
            })).expect("terminal content"),
            "result": {
                "exit_code": exit_code,
                "truncated": false,
            }
        })
    }

    #[test]
    fn clean_audit_passes_with_safe_install_and_lockfile() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&json!({"result": {"changed_files": [{"path": "package.json"}, {"path": "package-lock.json"}]}}));
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result("npm rebuild esbuild", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":1,"info":0,"low":1,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert!(report.blocking_reasons.is_empty());
    }

    #[test]
    fn critical_vulnerability_blocks_success() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            1,
            r#"{"metadata":{"vulnerabilities":{"total":1,"info":0,"low":0,"moderate":0,"high":0,"critical":1}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report.blocking_reasons[0].contains("critical"));
    }

    #[test]
    fn unavailable_audit_and_unsafe_install_are_not_treated_as_clean() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm install", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json || true",
            0,
            "network unavailable",
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("outside the approved policy")));
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("masked")));
    }

    #[test]
    fn split_terminal_payload_is_merged_before_evaluation() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&split_terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&split_terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        assert_eq!(evidence.evaluate(&policy()).status, "passed");
    }

    #[test]
    fn audit_json_is_found_after_a_json_preflight_output() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "node -e \"console.log(JSON.stringify({ scripts: true }))\" && npm audit --json --audit-level=high",
            0,
            "{\"scripts\":true}\n{\"metadata\":{\"vulnerabilities\":{\"total\":0,\"info\":0,\"low\":0,\"moderate\":0,\"high\":0,\"critical\":0}}}",
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert_eq!(report.vulnerabilities.expect("audit counts").total, 0);
    }

    #[test]
    fn documentation_checks_do_not_replace_real_node_command_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --json --audit-level=high",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));
        evidence.observe_tool_result(&terminal_result(
            "grep -q 'npm ci --ignore-scripts' README.md && grep -q 'npm audit --json --audit-level=high' README.md",
            0,
            "",
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "passed");
        assert_eq!(
            report.install_command.as_deref(),
            Some("npm ci --ignore-scripts")
        );
        assert_eq!(
            report.audit_command.as_deref(),
            Some("npm audit --json --audit-level=high")
        );
    }

    #[test]
    fn incomplete_vulnerability_metadata_is_rejected() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --json --audit-level=high",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("complete JSON `metadata.vulnerabilities`")));
    }

    #[test]
    fn failed_or_masked_rebuild_blocks_success() {
        for (command, exit_code) in [
            ("npm rebuild esbuild", 1),
            ("npm rebuild esbuild ||true", 0),
        ] {
            let mut evidence = evidence_with_manifest();
            evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
            evidence.observe_tool_result(&terminal_result(command, exit_code, ""));
            evidence.observe_tool_result(&terminal_result(
                "npm audit --audit-level=high --json",
                0,
                r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
            ));

            let report = evidence.evaluate(&policy());
            assert_eq!(report.status, "blocked");
            assert!(report
                .blocking_reasons
                .iter()
                .any(|reason| reason.contains("did not complete successfully")));
        }
    }

    #[test]
    fn package_manifest_write_is_verified_only_after_successful_session_commit() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "arguments": serde_json::to_string(&json!({
                "session_id": "session-1",
                "operations": [{
                    "kind": "write",
                    "path": "package.json",
                    "content": serde_json::to_string(&json!({
                        "dependencies": {"react": "^19.2.7"},
                        "devDependencies": {"vite": "^8.1.4"}
                    })).expect("manifest")
                }]
            })).expect("arguments")
        }]));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "success": true,
            "is_error": false
        }));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "arguments": {"session_id": "session-1"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {"committed_paths": ["package.json"]}
        }));

        assert_eq!(
            evidence
                .package_manifest
                .as_ref()
                .expect("successful manifest")
                .requirements["react"],
            "^19.2.7"
        );
    }

    #[test]
    fn later_partial_manifest_edit_invalidates_baseline_until_final_read() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "arguments": {
                "session_id": "session-edit",
                "operations": [{
                    "kind": "replace_text",
                    "path": "package.json",
                    "old_text": "^19.2.7",
                    "new_text": "^18.0.0"
                }]
            }
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "success": true,
            "is_error": false
        }));
        assert!(evidence.package_manifest.is_some());
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "arguments": {"session_id": "session-edit"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-commit",
            "name": "harness_code_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": {"committed_paths": ["package.json"]}
        }));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_result(&json!({
            "name": "harness_code_read_file_raw",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "path": "package.json",
                "content": serde_json::to_string(&json!({
                    "dependencies": {"react": "^18.0.0"},
                    "devDependencies": {"vite": "^8.1.4"}
                })).expect("manifest")
            })).expect("tool content")
        }));

        let violations = dependency_baseline_violations(
            evidence.package_manifest.as_ref().expect("final manifest"),
            &policy(),
        );
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("react"));
    }

    #[test]
    fn aborted_manifest_session_discards_staged_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "arguments": {
                "session_id": "session-abort",
                "operations": [{
                    "kind": "delete",
                    "path": "package.json"
                }]
            }
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-stage",
            "name": "harness_code_stage_edit_batch",
            "success": true,
            "is_error": false
        }));
        assert!(evidence
            .staged_package_manifest_updates
            .contains_key("session-abort"));

        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-abort",
            "name": "harness_code_abort_edit_session",
            "arguments": {"session_id": "session-abort"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-abort",
            "name": "harness_code_abort_edit_session",
            "success": true,
            "is_error": false
        }));

        assert!(evidence.staged_package_manifest_updates.is_empty());
        assert!(evidence.package_manifest.is_some());
    }

    #[test]
    fn dependency_requirement_mismatch_blocks_supply_chain_success() {
        let mut evidence = evidence_with_manifest();
        evidence
            .package_manifest
            .as_mut()
            .expect("manifest")
            .requirements
            .insert("react".to_string(), "^18.0.0".to_string());
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(!report.dependency_baseline_verified);
        assert_eq!(report.dependency_baseline_violations.len(), 1);
        assert!(report.dependency_baseline_violations[0].contains("react"));
        assert!(report.dependency_baseline_violations[0].contains("^19.2.7"));
    }

    #[test]
    fn failed_file_read_does_not_make_supply_chain_gate_applicable() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file",
            "success": false,
            "is_error": true,
            "result": { "path": "package.json", "message": "not found" },
        }));

        assert!(!evidence.evaluate(&policy()).applicable);
    }

    #[test]
    fn read_only_node_project_inspection_does_not_make_gate_applicable() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_list_dir",
            "success": true,
            "is_error": false,
            "result": {
                "entries": [
                    { "path": "package.json", "type": "file" },
                    { "path": "pnpm-lock.yaml", "type": "file" }
                ]
            }
        }));
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_read_read_file_raw",
            "success": true,
            "is_error": false,
            "content": serde_json::to_string(&json!({
                "path": "package.json",
                "content": serde_json::to_string(&json!({
                    "dependencies": { "react": "^19.2.7" },
                    "devDependencies": { "vite": "^8.1.4" }
                })).expect("manifest")
            })).expect("tool content")
        }));

        let report = evidence.evaluate(&policy());
        assert!(!report.applicable);
        assert_eq!(report.status, "not_applicable");
    }

    #[test]
    fn committed_dependency_file_change_makes_gate_applicable() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&json!({
            "name": "code_maintainer_write_commit_edit_session",
            "success": true,
            "is_error": false,
            "result": { "committed_paths": [{ "path": "pnpm-lock.yaml" }] }
        }));

        let report = evidence.evaluate(&policy());
        assert!(report.applicable);
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("installation was not executed")));
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("audit was not executed")));
    }

    #[test]
    fn truncated_audit_output_is_incomplete_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        let mut audit = terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"info":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        );
        audit["result"]["truncated"] = json!(true);
        evidence.observe_tool_result(&audit);

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("truncated")));
    }
}
