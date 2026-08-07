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
    lockfile_observed: bool,
    package_manager: Option<String>,
    install: Option<CommandEvidence>,
    unsafe_install_commands: Vec<String>,
    rebuilds: Vec<RebuildEvidence>,
    audit: Option<AuditEvidence>,
    package_manifest: Option<NodePackageManifestEvidence>,
    pending_package_manifest_updates: BTreeMap<String, Option<NodePackageManifestEvidence>>,
}

#[derive(Debug, Clone)]
struct NodePackageManifestEvidence {
    requirements: BTreeMap<String, String>,
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
            let Some(update) = package_manifest_update_from_tool_call(name, &arguments) else {
                continue;
            };
            self.pending_package_manifest_updates
                .insert(invocation_id.to_string(), update);
        }
    }

    pub(super) fn observe_tool_result(&mut self, payload: &Value) {
        let mut applied_manifest_update = false;
        if let Some(invocation_id) = payload.get("invocation_id").and_then(Value::as_str) {
            if payload.get("success").and_then(Value::as_bool) == Some(true)
                && payload.get("is_error").and_then(Value::as_bool) != Some(true)
            {
                if let Some(update) = self.pending_package_manifest_updates.remove(invocation_id) {
                    applied_manifest_update = true;
                    self.node_project_observed = true;
                    self.package_manifest = update;
                }
            } else {
                self.pending_package_manifest_updates.remove(invocation_id);
            }
        }
        if payload.get("success").and_then(Value::as_bool) == Some(true)
            && payload.get("is_error").and_then(Value::as_bool) != Some(true)
        {
            observe_project_paths(payload, self);
            if !applied_manifest_update && result_mutates_package_manifest(payload) {
                self.node_project_observed = true;
                self.package_manifest = None;
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
            self.lockfile_observed = exit_code == Some(0);
        }
        if is_node_install_command(&normalized) {
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
            self.rebuilds.push(RebuildEvidence {
                command: command.to_string(),
                exit_code,
                packages,
            });
        }
        if is_node_audit_command(&normalized) {
            self.audit = Some(AuditEvidence {
                command: command.to_string(),
                exit_code,
                output_truncated: result.output_truncated,
                vulnerabilities: parse_vulnerability_counts(result.output.as_str()),
            });
        }
    }

    pub(super) fn evaluate(&self, policy: &NodeSupplyChainPolicy) -> SupplyChainAuditReport {
        if !self.node_project_observed {
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
                if audit_command_matches_level(&audit.command, policy.audit_level.as_str())
                    && !command_masks_failure(audit.command.as_str())
                    && !audit.output_truncated
                    && audit.vulnerabilities.is_some() =>
            {
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
            Some(_) => blocking_reasons.push(
                "Node.js dependency audit did not produce complete JSON vulnerability evidence"
                    .to_string(),
            ),
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
            "Node.js supply-chain audit status: {}; baseline {}; dependency baseline verified={}; command `{}` exited {}; vulnerabilities total={}, high={}, critical={}",
            self.status,
            self.baseline_revision,
            self.dependency_baseline_verified,
            self.audit_command.as_deref().unwrap_or("not executed"),
            self.audit_exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            vulnerabilities.total,
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

fn package_manifest_update_from_tool_call(
    name: &str,
    arguments: &Value,
) -> Option<Option<NodePackageManifestEvidence>> {
    if name.ends_with("apply_patch") || name.ends_with("patch") {
        let patch = arguments.get("patch").and_then(Value::as_str)?;
        return patch.contains("package.json").then_some(None);
    }
    if !["write_file", "edit_file", "append_file", "delete_path"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    {
        return None;
    }
    let path = arguments
        .get("path")
        .or_else(|| arguments.get("relative_path"))
        .and_then(Value::as_str)?;
    if !path.replace('\\', "/").ends_with("package.json") {
        return None;
    }
    if name.ends_with("write_file") {
        return Some(
            arguments
                .get("content")
                .and_then(Value::as_str)
                .and_then(parse_package_manifest),
        );
    }
    Some(None)
}

fn result_mutates_package_manifest(payload: &Value) -> bool {
    let Some(name) = payload.get("name").and_then(Value::as_str) else {
        return false;
    };
    [
        "write_file",
        "edit_file",
        "append_file",
        "delete_path",
        "apply_patch",
        "patch",
    ]
    .iter()
    .any(|suffix| name.ends_with(suffix))
        && value_mentions_package_manifest(payload)
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
    if command.contains("pnpm ") {
        Some("pnpm")
    } else if command.contains("yarn ") {
        Some("yarn")
    } else if command.contains("bun ") {
        Some("bun")
    } else if command.contains("npm ") {
        Some("npm")
    } else {
        None
    }
}

fn is_node_install_command(command: &str) -> bool {
    [
        "npm ci",
        "npm install",
        "pnpm install",
        "yarn install",
        "bun install",
    ]
    .iter()
    .any(|needle| command.contains(needle))
        && !command.contains("--package-lock-only")
}

fn install_scripts_are_disabled(command: &str, package_manager: Option<&str>) -> bool {
    match package_manager {
        Some("yarn") => {
            command.contains("--mode=skip-builds") || command.contains("--mode skip-builds")
        }
        Some("npm" | "pnpm" | "bun") | None => command.contains("--ignore-scripts"),
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
    command.contains("--package-lock-only")
        || command.contains("pnpm install --lockfile-only")
        || command.contains("yarn install --mode=update-lockfile")
}

fn approved_rebuild_packages(command: &str) -> Option<Vec<String>> {
    let marker = if command.contains("npm rebuild ") {
        "npm rebuild "
    } else if command.contains("pnpm rebuild ") {
        "pnpm rebuild "
    } else {
        return None;
    };
    let packages = command
        .split_once(marker)?
        .1
        .split_whitespace()
        .take_while(|value| !value.starts_with('-') && !matches!(*value, "&&" | ";" | "||"))
        .map(|value| value.trim_matches(|character| matches!(character, '\'' | '"')))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    Some(packages)
}

fn is_node_audit_command(command: &str) -> bool {
    command.contains("npm audit")
        || command.contains("pnpm audit")
        || command.contains("yarn npm audit")
}

fn audit_command_matches_level(command: &str, audit_level: &str) -> bool {
    let command = command.to_ascii_lowercase();
    command.contains("--json")
        && (command.contains(format!("--audit-level={audit_level}").as_str())
            || command.contains(format!("--audit-level {audit_level}").as_str())
            || command.contains(format!("--severity={audit_level}").as_str())
            || command.contains(format!("--severity {audit_level}").as_str()))
}

fn parse_vulnerability_counts(output: &str) -> Option<NodeVulnerabilityCounts> {
    let value = serde_json::from_str::<Value>(output.trim())
        .ok()
        .or_else(|| {
            let start = output.find('{')?;
            let end = output.rfind('}')?;
            serde_json::from_str::<Value>(&output[start..=end]).ok()
        })?;
    let vulnerabilities = value.pointer("/metadata/vulnerabilities")?;
    Some(NodeVulnerabilityCounts {
        total: vulnerabilities
            .get("total")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        low: vulnerabilities
            .get("low")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        moderate: vulnerabilities
            .get("moderate")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        high: vulnerabilities
            .get("high")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        critical: vulnerabilities
            .get("critical")
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
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
            r#"{"metadata":{"vulnerabilities":{"total":1,"low":1,"moderate":0,"high":0,"critical":0}}}"#,
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
            r#"{"metadata":{"vulnerabilities":{"total":1,"low":0,"moderate":0,"high":0,"critical":1}}}"#,
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
            .any(|reason| reason.contains("complete JSON")));
    }

    #[test]
    fn split_terminal_payload_is_merged_before_evaluation() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&split_terminal_result("npm ci --ignore-scripts", 0, ""));
        evidence.observe_tool_result(&split_terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        ));

        assert_eq!(evidence.evaluate(&policy()).status, "passed");
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
                r#"{"metadata":{"vulnerabilities":{"total":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
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
    fn package_manifest_write_is_verified_only_after_successful_tool_result() {
        let mut evidence = SupplyChainEvidenceState::default();
        evidence.observe_tool_calls(&json!([{
            "invocation_id": "inv-1",
            "name": "harness_code_write_file",
            "arguments": serde_json::to_string(&json!({
                "path": "package.json",
                "content": serde_json::to_string(&json!({
                    "dependencies": {"react": "^19.2.7"},
                    "devDependencies": {"vite": "^8.1.4"}
                })).expect("manifest")
            })).expect("arguments")
        }]));
        assert!(evidence.package_manifest.is_none());

        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-1",
            "name": "harness_code_write_file",
            "success": true,
            "is_error": false
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
            "invocation_id": "inv-edit",
            "name": "harness_code_edit_file",
            "arguments": {"path": "package.json", "old_text": "^19.2.7", "new_text": "^18.0.0"}
        }]));
        evidence.observe_tool_result(&json!({
            "invocation_id": "inv-edit",
            "name": "harness_code_edit_file",
            "success": true,
            "is_error": false,
            "result": {"path": "package.json"}
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
            r#"{"metadata":{"vulnerabilities":{"total":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
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
    fn truncated_audit_output_is_incomplete_evidence() {
        let mut evidence = evidence_with_manifest();
        evidence.observe_tool_result(&terminal_result("npm ci --ignore-scripts", 0, ""));
        let mut audit = terminal_result(
            "npm audit --audit-level=high --json",
            0,
            r#"{"metadata":{"vulnerabilities":{"total":0,"low":0,"moderate":0,"high":0,"critical":0}}}"#,
        );
        audit["result"]["truncated"] = json!(true);
        evidence.observe_tool_result(&audit);

        let report = evidence.evaluate(&policy());
        assert_eq!(report.status, "blocked");
        assert!(report
            .blocking_reasons
            .iter()
            .any(|reason| reason.contains("complete JSON")));
    }
}
