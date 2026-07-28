// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;
use crate::models::{
    now_rfc3339, TaskMcpConfig, TaskRecord, TaskScheduleConfig, TaskStatus, TaskToolState,
};
use chatos_plugin_management_sdk::{
    parse_plugin_manifest, plugin_component_descriptors, AgentBindingRecord, BindingConditions,
    McpRuntime, PluginAvailabilityStatus, PluginCatalogRecord, PluginCommandInvocation,
    PluginComponentSnapshot, PluginComponentStatus, PluginInstallStatus, PluginInstallationRecord,
    PluginLicenseMetadata, PluginManifestSource, PluginPublisher, PluginReleaseRecord,
    PluginReleaseSignature, PluginRequirementStatus, ResolvedPlugin, ResolvedPluginComponent,
    ResourceMetadata, ResourceSecurity, SelectedPluginRef, SkillContent, SkillInstallationRecord,
    SkillRecord, TaskPluginConfig, UserPluginPreferenceRecord, PLUGIN_SIGNATURE_ALGORITHM_ED25519,
};
use serde_json::json;

fn resolved_mcp(
    id: &str,
    runtime_kind: &str,
    builtin_kind: Option<&str>,
    required: bool,
    available: bool,
) -> ResolvedMcp {
    ResolvedMcp {
        resource: PluginMcpRecord {
            id: id.to_string(),
            owner_user_id: "owner-1".to_string(),
            owner_kind: "system".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "system_seed".to_string(),
            name: id.to_string(),
            display_name: id.to_string(),
            description: None,
            enabled: true,
            runtime: McpRuntime {
                kind: runtime_kind.to_string(),
                system_key: (runtime_kind == chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND)
                    .then(|| builtin_kind.map(ToOwned::to_owned))
                    .flatten(),
                builtin_kind: (runtime_kind == BUILTIN_RUNTIME_KIND)
                    .then(|| builtin_kind.map(ToOwned::to_owned))
                    .flatten(),
                url: (runtime_kind == "http").then(|| "http://127.0.0.1/mcp".to_string()),
                ..McpRuntime::default()
            },
            security: ResourceSecurity::default(),
            metadata: ResourceMetadata::default(),
            plugin_component: Default::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "mcp".to_string(),
            resource_id: id.to_string(),
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available,
        status: if available { "available" } else { "offline" }.to_string(),
        reason: (!available).then(|| "offline".to_string()),
    }
}

fn resolved_skill(id: &str, required: bool, available: bool) -> ResolvedSkill {
    ResolvedSkill {
        resource: SkillRecord {
            id: id.to_string(),
            owner_user_id: "system".to_string(),
            owner_kind: "admin".to_string(),
            visibility: "system_private".to_string(),
            source_kind: "admin_created".to_string(),
            name: "remotion-best-practices".to_string(),
            display_name: "Remotion Best Practices".to_string(),
            description: Some("Local prompt-only Skill".to_string()),
            enabled: true,
            content: SkillContent {
                kind: "local_connector_bundle".to_string(),
                bundle_id: Some("chatos.internal.remotion-best-practices".to_string()),
                bundle_version: Some("1.0.0".to_string()),
                bundle_hash: Some("bundle-hash-1".to_string()),
                entrypoint_kind: Some("prompt_only".to_string()),
                ..SkillContent::default()
            },
            metadata: ResourceMetadata::default(),
            plugin_component: Default::default(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        binding: AgentBindingRecord {
            id: format!("binding-{id}"),
            agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
            binding_scope: if required {
                "system_required".to_string()
            } else {
                "global_default".to_string()
            },
            owner_user_id: None,
            resource_kind: "skill".to_string(),
            resource_id: id.to_string(),
            enabled: true,
            required,
            priority: 0,
            conditions: BindingConditions::default(),
            component_allowlist: Vec::new(),
            created_by: "system".to_string(),
            updated_by: "system".to_string(),
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        },
        available,
        status: if available { "available" } else { "offline" }.to_string(),
        reason: (!available).then(|| "offline".to_string()),
        installation: available.then(|| SkillInstallationRecord {
            id: format!("owner-1:device-1:{id}"),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            skill_id: id.to_string(),
            bundle_id: "chatos.internal.remotion-best-practices".to_string(),
            version: "1.0.0".to_string(),
            bundle_hash: "bundle-hash-1".to_string(),
            platform: "macos-arm64".to_string(),
            status: "available".to_string(),
            dependency_status: "available".to_string(),
            last_error: None,
            last_checked_at: "now".to_string(),
        }),
    }
}

fn resolved_plugin(required: bool) -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 1,
            "name": "browser",
            "version": "1.0.0",
            "description": "Browser control plugin",
            "author": {"name": "ChatOS"},
            "mcpServers": {
                "browser": {
                    "type": "http",
                    "url": "https://browser.example.com/mcp"
                }
            },
            "interface": {
                "displayName": "Browser",
                "shortDescription": "Control browser",
                "longDescription": "Control the in-app browser through signed Plugin tools.",
                "developerName": "ChatOS",
                "category": "Productivity"
            },
            "permissions": ["browser.control"]
        }"#,
        PluginManifestSource::Chatos,
    )
    .expect("Plugin Manifest");
    let components = plugin_component_descriptors(&manifest);
    let catalog = PluginCatalogRecord {
        id: "plugin-browser".to_string(),
        plugin_key: "browser@official".to_string(),
        marketplace_id: "official".to_string(),
        owner_user_id: None,
        name: manifest.name.clone(),
        display_name: "Browser".to_string(),
        description: manifest.description.clone(),
        publisher: PluginPublisher {
            id: "publisher-chatos".to_string(),
            name: "ChatOS".to_string(),
            website: None,
            verified: true,
        },
        interface: manifest.interface.clone(),
        keywords: vec!["browser".to_string()],
        visibility: "public".to_string(),
        featured: true,
        enabled: true,
        latest_release_id: "release-browser-1".to_string(),
        license: PluginLicenseMetadata {
            license_id: "MIT".to_string(),
            license_url: None,
            redistributable: true,
            reviewed_at: Some("now".to_string()),
        },
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    let release = PluginReleaseRecord {
        id: "release-browser-1".to_string(),
        plugin_id: catalog.id.clone(),
        version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        artifact_ref: "https://plugins.example.com/browser.zip".to_string(),
        artifact_sha256: "a".repeat(64),
        signature: PluginReleaseSignature {
            key_id: "key-1".to_string(),
            publisher_id: catalog.publisher.id.clone(),
            marketplace_id: catalog.marketplace_id.clone(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            signature_base64: "signature".to_string(),
            signed_at: "now".to_string(),
            manifest_sha256: "b".repeat(64),
        },
        sbom_ref: None,
        supported_platforms: vec!["macos-arm64".to_string()],
        components: components.clone(),
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: "stable".to_string(),
        published_at: "now".to_string(),
        revoked_at: None,
    };
    let installation = PluginInstallationRecord {
        id: "owner-1:device-1:plugin-browser".to_string(),
        owner_user_id: "owner-1".to_string(),
        device_id: "device-1".to_string(),
        plugin_id: catalog.id.clone(),
        release_id: release.id.clone(),
        version: release.version.clone(),
        artifact_sha256: release.artifact_sha256.clone(),
        platform: "macos-arm64".to_string(),
        install_status: PluginInstallStatus::Installed,
        availability_status: PluginAvailabilityStatus::Ready,
        dependency_status: PluginRequirementStatus::Satisfied,
        permission_status: PluginRequirementStatus::Satisfied,
        auth_status: PluginRequirementStatus::Satisfied,
        component_statuses: components
            .iter()
            .map(|component| PluginComponentStatus {
                component_key: component.component_key.clone(),
                kind: component.kind,
                availability_status: PluginAvailabilityStatus::Ready,
                last_error: None,
                last_checked_at: "now".to_string(),
            })
            .collect(),
        active: true,
        previous_release_id: None,
        installed_at: "now".to_string(),
        last_checked_at: "now".to_string(),
        last_error: None,
    };
    let binding = AgentBindingRecord {
        id: "binding-plugin-browser".to_string(),
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        binding_scope: if required {
            "system_required".to_string()
        } else {
            "global_default".to_string()
        },
        owner_user_id: None,
        resource_kind: "plugin".to_string(),
        resource_id: catalog.id.clone(),
        enabled: true,
        required,
        priority: 0,
        conditions: BindingConditions::default(),
        component_allowlist: vec!["browser".to_string()],
        created_by: "system".to_string(),
        updated_by: "system".to_string(),
        created_at: "now".to_string(),
        updated_at: "now".to_string(),
    };
    ResolvedPlugin {
        catalog: catalog.clone(),
        release: Some(release.clone()),
        binding,
        installation: Some(installation),
        preference: Some(UserPluginPreferenceRecord {
            owner_user_id: "owner-1".to_string(),
            plugin_id: catalog.id.clone(),
            enabled: true,
            auto_update: false,
            release_channel: "stable".to_string(),
            enabled_components: vec!["browser".to_string()],
            updated_at: "now".to_string(),
        }),
        components: components
            .iter()
            .cloned()
            .map(|component| ResolvedPluginComponent {
                component,
                available: true,
                status: PluginAvailabilityStatus::Ready,
                reason: None,
            })
            .collect(),
        component_snapshots: components
            .into_iter()
            .map(|component| PluginComponentSnapshot {
                plugin_id: catalog.id.clone(),
                release_id: release.id.clone(),
                component,
                content_sha256: "c".repeat(64),
            })
            .collect(),
        auth_connection_ids: vec!["oauth-browser-account".to_string()],
        available: true,
        status: PluginAvailabilityStatus::Ready,
        reason: None,
    }
}

fn resolved_ui_plugin() -> ResolvedPlugin {
    let mut plugin = resolved_plugin(false);
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 1,
            "name": "browser",
            "version": "1.0.0",
            "description": "Signed browser workbench",
            "author": {"name": "ChatOS"},
            "ui": [{
                "componentKey": "security-workbench",
                "source": "./ui/index.html",
                "title": "Security Workbench",
                "surface": "workbench",
                "assets": ["./ui/app.js", "./ui/styles.css"],
                "bridgeCapabilities": ["artifact.read", "host.context.read"],
                "artifactMimeTypes": ["application/json"]
            }],
            "interface": {
                "displayName": "Browser",
                "shortDescription": "Signed workbench",
                "longDescription": "A signed sandboxed Plugin workbench.",
                "developerName": "ChatOS",
                "category": "Productivity"
            },
            "permissions": [{
                "permission": "artifact.read",
                "required": true,
                "components": ["security-workbench"]
            }]
        }"#,
        PluginManifestSource::Chatos,
    )
    .expect("Plugin UI Manifest");
    let components = plugin_component_descriptors(&manifest);
    let release_id = {
        let release = plugin.release.as_mut().expect("Plugin Release");
        release.normalized_manifest = manifest.clone();
        release.components = components.clone();
        release.dependencies = manifest.dependencies.clone();
        release.permissions = manifest.permissions.clone();
        release.id.clone()
    };
    let installation = plugin.installation.as_mut().expect("Plugin installation");
    installation.component_statuses = components
        .iter()
        .map(|component| PluginComponentStatus {
            component_key: component.component_key.clone(),
            kind: component.kind,
            availability_status: PluginAvailabilityStatus::Ready,
            last_error: None,
            last_checked_at: "now".to_string(),
        })
        .collect();
    plugin.binding.component_allowlist = vec!["security-workbench".to_string()];
    plugin
        .preference
        .as_mut()
        .expect("Plugin preference")
        .enabled_components = vec!["security-workbench".to_string()];
    plugin.components = components
        .iter()
        .cloned()
        .map(|component| ResolvedPluginComponent {
            component,
            available: true,
            status: PluginAvailabilityStatus::Ready,
            reason: None,
        })
        .collect();
    plugin.component_snapshots = components
        .into_iter()
        .map(|component| PluginComponentSnapshot {
            plugin_id: plugin.catalog.id.clone(),
            release_id: release_id.clone(),
            component,
            content_sha256: "c".repeat(64),
        })
        .collect();
    plugin.auth_connection_ids.clear();
    plugin
}

fn resolved_command_plugin(requires_confirmation: bool) -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        format!(
            r#"{{
                "schemaVersion": 1,
                "name": "review-command",
                "version": "1.0.0",
                "description": "Signed review command",
                "author": {{"name": "ChatOS"}},
                "commands": [{{
                    "componentKey": "review",
                    "source": "./commands/review.md",
                    "description": "Review the current change",
                    "argumentHint": "[path]",
                    "requiresConfirmation": {requires_confirmation},
                    "targetAgent": "task_runner_run_phase",
                    "allowedTools": ["browser_tools_browser_snapshot"]
                }}],
                "interface": {{
                    "displayName": "Review Command",
                    "shortDescription": "Review a change",
                    "longDescription": "Review a change through a signed Plugin Command.",
                    "developerName": "ChatOS",
                    "category": "Productivity"
                }},
                "permissions": [{{
                    "permission": "workspace.read",
                    "components": ["review"]
                }}]
            }}"#
        )
        .as_str(),
        PluginManifestSource::Chatos,
    )
    .expect("Command Plugin Manifest");
    let components = plugin_component_descriptors(&manifest);
    let mut plugin = resolved_plugin(false);
    plugin.catalog.name = manifest.name.clone();
    plugin.catalog.display_name = manifest.interface.display_name.clone();
    plugin.catalog.description = manifest.description.clone();
    plugin.catalog.interface = manifest.interface.clone();
    plugin.binding.component_allowlist = vec!["review".to_string()];
    let release = plugin.release.as_mut().expect("Plugin Release");
    release.normalized_manifest = manifest.clone();
    release.components = components.clone();
    release.permissions = manifest.permissions.clone();
    let release_id = release.id.clone();
    let installation = plugin.installation.as_mut().expect("Plugin installation");
    installation.component_statuses = components
        .iter()
        .map(|component| PluginComponentStatus {
            component_key: component.component_key.clone(),
            kind: component.kind,
            availability_status: PluginAvailabilityStatus::Ready,
            last_error: None,
            last_checked_at: "now".to_string(),
        })
        .collect();
    plugin
        .preference
        .as_mut()
        .expect("Plugin preference")
        .enabled_components = vec!["review".to_string()];
    plugin.components = components
        .iter()
        .cloned()
        .map(|component| ResolvedPluginComponent {
            component,
            available: true,
            status: PluginAvailabilityStatus::Ready,
            reason: None,
        })
        .collect();
    plugin.component_snapshots = components
        .into_iter()
        .map(|component| PluginComponentSnapshot {
            plugin_id: plugin.catalog.id.clone(),
            release_id: release_id.clone(),
            component,
            content_sha256: "d".repeat(64),
        })
        .collect();
    plugin
}

fn resolved_agent_plugin(base_agent: &str) -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        format!(
            r#"{{
                "schemaVersion": 1,
                "name": "review-agent",
                "version": "1.0.0",
                "description": "Signed review Agent",
                "author": {{"name": "ChatOS"}},
                "agents": [{{
                    "componentKey": "reviewer",
                    "source": "./agents/reviewer.md",
                    "description": "Review the current change",
                    "baseAgent": "{base_agent}",
                    "allowedTools": ["browser_tools_browser_snapshot"],
                    "maxIterations": 12
                }}],
                "interface": {{
                    "displayName": "Review Agent",
                    "shortDescription": "Review a change",
                    "longDescription": "Review a change through a signed Plugin Agent.",
                    "developerName": "ChatOS",
                    "category": "Productivity"
                }},
                "permissions": [{{
                    "permission": "workspace.read",
                    "components": ["reviewer"]
                }}]
            }}"#
        )
        .as_str(),
        PluginManifestSource::Chatos,
    )
    .expect("Agent Plugin Manifest");
    let components = plugin_component_descriptors(&manifest);
    let mut plugin = resolved_plugin(false);
    plugin.catalog.name = manifest.name.clone();
    plugin.catalog.display_name = manifest.interface.display_name.clone();
    plugin.catalog.description = manifest.description.clone();
    plugin.catalog.interface = manifest.interface.clone();
    plugin.binding.agent_key = base_agent.to_string();
    plugin.binding.component_allowlist = vec!["reviewer".to_string()];
    let release = plugin.release.as_mut().expect("Plugin Release");
    release.normalized_manifest = manifest.clone();
    release.components = components.clone();
    release.permissions = manifest.permissions.clone();
    let release_id = release.id.clone();
    let installation = plugin.installation.as_mut().expect("Plugin installation");
    installation.component_statuses = components
        .iter()
        .map(|component| PluginComponentStatus {
            component_key: component.component_key.clone(),
            kind: component.kind,
            availability_status: PluginAvailabilityStatus::Ready,
            last_error: None,
            last_checked_at: "now".to_string(),
        })
        .collect();
    plugin
        .preference
        .as_mut()
        .expect("Plugin preference")
        .enabled_components = vec!["reviewer".to_string()];
    plugin.components = components
        .iter()
        .cloned()
        .map(|component| ResolvedPluginComponent {
            component,
            available: true,
            status: PluginAvailabilityStatus::Ready,
            reason: None,
        })
        .collect();
    plugin.component_snapshots = components
        .into_iter()
        .map(|component| PluginComponentSnapshot {
            plugin_id: plugin.catalog.id.clone(),
            release_id: release_id.clone(),
            component,
            content_sha256: "e".repeat(64),
        })
        .collect();
    plugin
}

fn resolved_hook_plugin() -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 1,
            "name": "lifecycle-hooks",
            "version": "1.0.0",
            "description": "Signed lifecycle Hooks",
            "author": {"name": "ChatOS"},
            "hooks": [{
                "componentKey": "lifecycle-hooks",
                "source": "./hooks.json"
            }],
            "interface": {
                "displayName": "Lifecycle Hooks",
                "shortDescription": "Audit lifecycle events",
                "longDescription": "Audit lifecycle events through signed Plugin Hooks.",
                "developerName": "ChatOS",
                "category": "Productivity"
            },
            "permissions": [{
                "permission": "process.spawn",
                "components": ["lifecycle-hooks"]
            }]
        }"#,
        PluginManifestSource::Chatos,
    )
    .expect("Hook Plugin Manifest");
    let components = plugin_component_descriptors(&manifest);
    let mut plugin = resolved_plugin(false);
    plugin.catalog.name = manifest.name.clone();
    plugin.catalog.display_name = manifest.interface.display_name.clone();
    plugin.catalog.description = manifest.description.clone();
    plugin.catalog.interface = manifest.interface.clone();
    plugin.binding.component_allowlist = vec!["lifecycle-hooks".to_string()];
    let release = plugin.release.as_mut().expect("Plugin Release");
    release.normalized_manifest = manifest.clone();
    release.components = components.clone();
    release.permissions = manifest.permissions.clone();
    let release_id = release.id.clone();
    let installation = plugin.installation.as_mut().expect("Plugin installation");
    installation.component_statuses = components
        .iter()
        .map(|component| PluginComponentStatus {
            component_key: component.component_key.clone(),
            kind: component.kind,
            availability_status: PluginAvailabilityStatus::Ready,
            last_error: None,
            last_checked_at: "now".to_string(),
        })
        .collect();
    plugin
        .preference
        .as_mut()
        .expect("Plugin preference")
        .enabled_components = vec!["lifecycle-hooks".to_string()];
    plugin.components = components
        .iter()
        .cloned()
        .map(|component| ResolvedPluginComponent {
            component,
            available: true,
            status: PluginAvailabilityStatus::Ready,
            reason: None,
        })
        .collect();
    plugin.component_snapshots = components
        .into_iter()
        .map(|component| PluginComponentSnapshot {
            plugin_id: plugin.catalog.id.clone(),
            release_id: release_id.clone(),
            component,
            content_sha256: "f".repeat(64),
        })
        .collect();
    plugin
}

fn policy() -> TaskRunnerCapabilityPolicy {
    TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-1".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![
            resolved_mcp(
                "task-manager",
                BUILTIN_RUNTIME_KIND,
                Some("TaskManager"),
                true,
                true,
            ),
            resolved_mcp(
                "ask-user",
                BUILTIN_RUNTIME_KIND,
                Some("AskUser"),
                true,
                true,
            ),
            resolved_mcp(
                "read",
                BUILTIN_RUNTIME_KIND,
                Some("CodeMaintainerRead"),
                false,
                true,
            ),
            resolved_mcp(
                "write",
                BUILTIN_RUNTIME_KIND,
                Some("CodeMaintainerWrite"),
                false,
                false,
            ),
            resolved_mcp("external-1", "http", None, false, true),
        ],
        skills: vec![resolved_skill("internal_skill_remotion", false, true)],
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy")
}

fn task() -> TaskRecord {
    let now = now_rfc3339();
    TaskRecord {
        id: "task-1".to_string(),
        title: "Task".to_string(),
        description: None,
        objective: "Objective".to_string(),
        input_payload: None,
        status: TaskStatus::Ready,
        priority: 0,
        tags: Vec::new(),
        default_model_config_id: None,
        memory_thread_id: "thread-1".to_string(),
        tenant_id: "tenant-1".to_string(),
        subject_id: "owner-1".to_string(),
        project_id: "public".to_string(),
        task_profile: "default".to_string(),
        creator_user_id: Some("owner-1".to_string()),
        creator_username: None,
        creator_display_name: None,
        owner_user_id: Some("owner-1".to_string()),
        owner_username: None,
        owner_display_name: None,
        result_summary: None,
        process_log: None,
        last_run_id: None,
        schedule: TaskScheduleConfig::default(),
        parent_task_id: None,
        source_run_id: None,
        source_session_id: None,
        source_turn_id: None,
        source_user_message_id: None,
        prerequisite_task_ids: Vec::new(),
        task_tool_state: TaskToolState::default(),
        plugin_config: Default::default(),
        mcp_config: TaskMcpConfig {
            enabled: false,
            enabled_builtin_kinds: vec![
                "CodeMaintainerRead".to_string(),
                "CodeMaintainerWrite".to_string(),
            ],
            external_mcp_config_ids: vec!["external-1".to_string(), "revoked".to_string()],
            selected_skill_ids: vec![
                "internal_skill_remotion".to_string(),
                "revoked-skill".to_string(),
            ],
            ..TaskMcpConfig::default()
        },
        created_at: now.clone(),
        updated_at: now,
        deleted_at: None,
    }
}

#[test]
fn ai_selectable_sets_exclude_required_and_unavailable_mcp_capabilities() {
    let policy = policy();
    assert_eq!(
        policy.selectable_builtin_kind_names(),
        vec!["CodeMaintainerRead".to_string()]
    );
    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["external-1".to_string()]
    );
}

#[test]
fn runtime_injects_required_and_intersects_saved_optional_selection() {
    let mut task = task();
    policy().apply_to_task(&mut task).expect("apply policy");
    assert!(task.mcp_config.enabled);
    assert_eq!(
        task.mcp_config.enabled_builtin_kinds,
        vec![
            "CodeMaintainerRead".to_string(),
            "TaskManager".to_string(),
            "AskUser".to_string(),
        ]
    );
    assert_eq!(
        task.mcp_config.external_mcp_config_ids,
        vec!["external-1".to_string()]
    );
    assert!(task.mcp_config.selected_skill_ids.is_empty());
    let snapshots = policy().skill_snapshots(&task).expect("skill snapshots");
    assert!(snapshots.is_empty());
}

#[test]
fn planning_policy_injects_its_non_mutating_builtin_allowlist() {
    let mut policy = policy();
    policy.capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    for item in &mut policy.capabilities.mcps {
        item.binding.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
        if item.resource.id == "external-1" {
            item.resource.security.allow_writes = Some(true);
        }
        if item.resource.id == "write" {
            item.available = true;
            item.status = "available".to_string();
            item.reason = None;
        }
    }
    let mut task = task();
    task.task_profile = crate::models::TASK_PROFILE_CHATOS_PLAN.to_string();
    task.mcp_config.requires_execution = false;
    task.mcp_config.enabled_builtin_kinds.clear();

    policy.apply_to_task(&mut task).expect("apply plan policy");

    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerRead".to_string()));
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"TaskManager".to_string()));
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"AskUser".to_string()));
    assert!(!task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"CodeMaintainerWrite".to_string()));
    assert!(!task
        .mcp_config
        .enabled_builtin_kinds
        .contains(&"TerminalController".to_string()));
    assert!(policy.selectable_external_mcp_ids().is_empty());
}

#[test]
fn planning_policy_rejects_required_mutating_tools() {
    let mut capabilities = policy().capabilities;
    capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    let write = capabilities
        .mcps
        .iter_mut()
        .find(|item| item.resource.id == "write")
        .expect("write capability");
    write.binding.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    write.binding.required = true;
    write.available = true;
    write.status = "available".to_string();
    write.reason = None;

    let error = TaskRunnerCapabilityPolicy::new(capabilities)
        .expect_err("planning policy must reject mutating required tools");
    assert!(error.contains("cannot be required for task_runner_plan_phase"));
}

#[test]
fn policy_rejects_write_when_read_is_not_configured_for_the_same_agent() {
    let mut capabilities = policy().capabilities;
    capabilities
        .mcps
        .retain(|item| plugin_builtin_kind(item) != Some(BuiltinMcpKind::CodeMaintainerRead));
    let write = capabilities
        .mcps
        .iter_mut()
        .find(|item| plugin_builtin_kind(item) == Some(BuiltinMcpKind::CodeMaintainerWrite))
        .expect("write capability");
    write.available = true;
    write.status = "available".to_string();
    write.reason = None;

    let error = TaskRunnerCapabilityPolicy::new(capabilities)
        .expect_err("write-only Plugin configuration must fail closed");
    assert!(error.contains("enables CodeMaintainerWrite without CodeMaintainerRead"));
}

#[test]
fn disabled_task_runner_agent_fails_closed() {
    let mut capabilities = policy().capabilities;
    capabilities.agent_enabled = false;
    let error =
        TaskRunnerCapabilityPolicy::new(capabilities).expect_err("disabled Agent must not execute");
    assert!(error.contains("disabled by Plugin Management"));
}

#[test]
fn write_validation_rejects_required_and_unavailable_selection() {
    let mut config = TaskMcpConfig {
        enabled_builtin_kinds: vec!["TaskManager".to_string()],
        ..TaskMcpConfig::default()
    };
    assert!(policy().validate_optional_config(&config).is_err());
    config.enabled_builtin_kinds = vec!["CodeMaintainerWrite".to_string()];
    assert!(policy().validate_optional_config(&config).is_err());
}

#[test]
fn cloud_policy_excludes_local_connector_mcps() {
    let mut local = resolved_mcp("local-user", "local_connector_http", None, false, true);
    local.resource.source_kind = LOCAL_CONNECTOR_DISCOVERED_SOURCE_KIND.to_string();
    let cloud = resolved_mcp("cloud-http", "http", None, false, true);
    let policy = TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-local".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![local, cloud],
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy");

    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec!["cloud-http".to_string()]
    );
}

#[test]
fn unified_service_system_mcp_is_selected_as_a_task_runner_backend() {
    let system = resolved_mcp(
        chatos_plugin_management_sdk::PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID,
        chatos_plugin_management_sdk::SYSTEM_MCP_RUNTIME_KIND,
        Some("project_runtime_environment"),
        false,
        true,
    );
    let policy = TaskRunnerCapabilityPolicy::new(ResolvedAgentCapabilities {
        agent_key: SystemAgentKey::TaskRunnerRunPhase.as_str().to_string(),
        owner_user_id: "owner-1".to_string(),
        policy_revision: "revision-system".to_string(),
        generated_at: "now".to_string(),
        agent_enabled: true,
        mcps: vec![system],
        skills: Vec::new(),
        plugins: Vec::new(),
        local_connector_requirements: Vec::new(),
    })
    .expect("policy");

    assert_eq!(
        policy.selectable_external_mcp_ids(),
        vec![chatos_plugin_management_sdk::PROJECT_RUNTIME_ENVIRONMENT_MCP_RESOURCE_ID.to_string()]
    );
}

#[test]
fn user_created_cloud_mcp_is_allowed_and_local_connector_mcp_is_rejected() {
    for runtime_kind in ["http", "stdio_cloud"] {
        let mut item = resolved_mcp("user-cloud-mcp", runtime_kind, None, false, true);
        item.resource.source_kind = "user_created".to_string();
        item.resource.owner_kind = "user".to_string();
        validate_cloud_external_mcp_runtime(&item)
            .expect("user-created cloud MCP should remain cloud-runnable");
    }

    let local = resolved_mcp("local-mcp", "local_connector_stdio", None, false, true);
    let err = validate_cloud_external_mcp_runtime(&local)
        .expect_err("Local Connector MCP must be rejected by cloud policy");
    assert!(err.contains("unavailable in cloud Task Runner"));
}

#[test]
fn plugin_selection_requires_exact_device_and_produces_immutable_run_snapshot() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_plugin(false)];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Plugin policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Plugin snapshots");

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].plugin_id, "plugin-browser");
    assert_eq!(snapshots[0].release_id, "release-browser-1");
    assert_eq!(snapshots[0].device_id, "device-1");
    assert_eq!(snapshots[0].workspace_id.as_deref(), Some("workspace-1"));
    assert_eq!(snapshots[0].artifact_sha256, "a".repeat(64));
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    assert_eq!(snapshots[0].component_snapshots[0].component_key, "browser");
    assert_eq!(
        snapshots[0].component_snapshots[0].content_sha256,
        "c".repeat(64)
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["browser.control"]);
    assert_eq!(
        snapshots[0].auth_connection_ids,
        vec!["oauth-browser-account"]
    );
    assert!(task
        .mcp_config
        .enabled_builtin_kinds
        .iter()
        .any(|kind| kind == "BrowserTools"));
}

#[test]
fn plugin_ui_is_pinned_as_a_signed_run_component_without_executable_operations() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_ui_plugin()];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Plugin UI policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Plugin UI policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Plugin UI snapshots");
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.kind, PluginComponentKind::UiContribution);
    assert_eq!(component.component_key, "security-workbench");
    assert_eq!(component.content_sha256, "c".repeat(64));
    assert_eq!(
        component.runtime.get("runtime_kind"),
        Some(&json!("sandboxed_ui"))
    );
    assert_eq!(
        component.runtime.get("entrypoint"),
        Some(&json!("./ui/index.html"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("assets")),
        Some(&json!(["./ui/app.js", "./ui/styles.css"]))
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["artifact.read"]);
}

#[test]
fn plugin_selection_without_device_fails_closed() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_plugin(false)];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Plugin policy");
    let config = TaskPluginConfig {
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        ..TaskPluginConfig::default()
    };

    let error = policy
        .validate_plugin_config(&config)
        .expect_err("device-less Plugin selection must fail closed");
    assert!(error.contains("device_id"));
}

#[test]
fn selected_command_enters_the_immutable_run_snapshot() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_command_plugin(false)];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Command Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: vec![PluginCommandInvocation {
            plugin_id: "plugin-browser".to_string(),
            command_id: "review".to_string(),
            arguments: Some("src/lib.rs".to_string()),
        }],
    };

    policy
        .apply_to_task(&mut task)
        .expect("apply Command Plugin policy");
    let snapshots = policy
        .plugin_snapshots(&task)
        .expect("Command Plugin snapshots");

    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.component_key, "review");
    assert_eq!(component.kind, PluginComponentKind::Command);
    assert_eq!(component.content_sha256, "d".repeat(64));
    assert_eq!(
        component.runtime.get("entrypoint"),
        Some(&json!("./commands/review.md"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("description")),
        Some(&json!("Review the current change"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("argument_hint")),
        Some(&json!("[path]"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("requires_confirmation")),
        Some(&json!(false))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("target_agent")),
        Some(&json!("task_runner_run_phase"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("allowed_tools")),
        Some(&json!(["browser_tools_browser_snapshot"]))
    );
    assert_eq!(
        component.runtime.get("arguments"),
        Some(&json!("src/lib.rs"))
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["workspace.read"]);
}

#[test]
fn selected_agent_enters_the_immutable_run_snapshot_and_catalog() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_agent_plugin("task_runner_run_phase")];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Agent Plugin policy");
    let views = policy.selectable_plugin_views();
    assert_eq!(views[0].agents.len(), 1);
    assert_eq!(views[0].agents[0].agent_id, "reviewer");
    assert_eq!(views[0].agents[0].max_iterations, 12);

    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["reviewer".to_string()],
        }],
        command_invocations: Vec::new(),
    };
    policy
        .apply_to_task(&mut task)
        .expect("apply Agent Plugin policy");
    let snapshots = policy
        .plugin_snapshots(&task)
        .expect("Agent Plugin snapshots");
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.component_key, "reviewer");
    assert_eq!(component.kind, PluginComponentKind::Agent);
    assert_eq!(component.content_sha256, "e".repeat(64));
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("base_agent")),
        Some(&json!("task_runner_run_phase"))
    );
    assert_eq!(
        component
            .runtime
            .get("metadata")
            .and_then(|metadata| metadata.get("max_iterations")),
        Some(&json!(12))
    );
}

#[test]
fn hook_set_is_automatically_bound_to_the_immutable_run_snapshot() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_hook_plugin()];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Hook Plugin policy");
    let mut task = task();
    task.plugin_config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        workspace_id: Some("workspace-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: Vec::new(),
    };

    policy.apply_to_task(&mut task).expect("apply Hook policy");
    let snapshots = policy.plugin_snapshots(&task).expect("Hook snapshots");
    assert_eq!(snapshots[0].component_snapshots.len(), 1);
    let component = &snapshots[0].component_snapshots[0];
    assert_eq!(component.kind, PluginComponentKind::HookSet);
    assert_eq!(component.component_key, "lifecycle-hooks");
    assert_eq!(component.content_sha256, "f".repeat(64));
    assert_eq!(
        component.runtime.get("entrypoint"),
        Some(&json!("./hooks.json"))
    );
    assert_eq!(snapshots[0].permission_snapshot, vec!["process.spawn"]);
}

#[test]
fn plugin_agent_must_match_the_existing_plan_or_run_agent() {
    let mut run_capabilities = policy().capabilities;
    run_capabilities.plugins = vec![resolved_agent_plugin("task_runner_plan_phase")];
    let run_policy = TaskRunnerCapabilityPolicy::new(run_capabilities)
        .expect("incompatible optional Agent components are filtered");
    assert!(run_policy.selectable_plugin_views().is_empty());

    let mut plan_capabilities = policy().capabilities;
    plan_capabilities.agent_key = SystemAgentKey::TaskRunnerPlanPhase.as_str().to_string();
    plan_capabilities.plugins = vec![resolved_agent_plugin("task_runner_plan_phase")];
    let policy =
        TaskRunnerCapabilityPolicy::new(plan_capabilities).expect("plan Agent Plugin policy");
    assert_eq!(
        policy.selectable_plugin_views()[0].agents[0].agent_id,
        "reviewer"
    );
}

#[test]
fn a_task_may_select_only_one_plugin_agent() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_agent_plugin("task_runner_run_phase")];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Agent Plugin policy");
    let config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: Vec::new(),
            selected_agent_ids: vec!["reviewer".to_string(), "second".to_string()],
        }],
        ..TaskPluginConfig::default()
    };
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("multiple Plugin Agents must fail")
        .contains("more than one Agent"));
}

#[test]
fn command_requiring_confirmation_is_preserved_for_local_device_approval() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_command_plugin(true)];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Command Plugin policy");
    let config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        ..TaskPluginConfig::default()
    };

    policy
        .validate_plugin_config(&config)
        .expect("confirmation is enforced by the Local Connector at prepare time");
}

#[test]
fn command_invocation_arguments_must_reference_one_exact_selected_command() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_command_plugin(false)];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Command Plugin policy");
    let mut config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        command_invocations: vec![PluginCommandInvocation {
            plugin_id: "plugin-browser".to_string(),
            command_id: "unknown".to_string(),
            arguments: Some("src/lib.rs".to_string()),
        }],
        ..TaskPluginConfig::default()
    };
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("unselected Command invocation must fail")
        .contains("unselected Command"));

    config.command_invocations[0].command_id = "review".to_string();
    config
        .command_invocations
        .push(config.command_invocations[0].clone());
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("duplicate Command invocation must fail")
        .contains("duplicated"));

    config.command_invocations.truncate(1);
    config.command_invocations[0].arguments = Some("x".repeat(16 * 1024 + 1));
    assert!(policy
        .validate_plugin_config(&config)
        .expect_err("oversized Command arguments must fail")
        .contains("exceed"));
}

#[test]
fn command_targeting_the_plan_agent_is_not_selectable_for_run_phase() {
    let mut command_plugin = resolved_command_plugin(false);
    let component = command_plugin
        .components
        .iter_mut()
        .find(|component| component.component.kind == PluginComponentKind::Command)
        .expect("Command component");
    component
        .component
        .metadata
        .insert("target_agent".to_string(), json!("task_runner_plan_phase"));
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![command_plugin];
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("Command Plugin policy");
    let config = TaskPluginConfig {
        device_id: Some("device-1".to_string()),
        selected_plugins: vec![SelectedPluginRef {
            plugin_id: "plugin-browser".to_string(),
            selected_skill_ids: Vec::new(),
            selected_command_ids: vec!["review".to_string()],
            selected_agent_ids: Vec::new(),
        }],
        ..TaskPluginConfig::default()
    };

    let error = policy
        .validate_plugin_config(&config)
        .expect_err("incompatible target Agent must fail");
    assert!(
        error.contains("not selectable") || error.contains("incompatible"),
        "unexpected fail-closed error: {error}"
    );
}

#[test]
fn required_plugin_is_injected_into_effective_task_config() {
    let mut capabilities = policy().capabilities;
    capabilities.plugins = vec![resolved_plugin(true)];
    capabilities.mcps.push(resolved_mcp(
        "browser-tools",
        BUILTIN_RUNTIME_KIND,
        Some("BrowserTools"),
        false,
        true,
    ));
    let policy = TaskRunnerCapabilityPolicy::new(capabilities).expect("required Plugin policy");
    let mut task = task();
    task.plugin_config.device_id = Some("device-1".to_string());

    policy
        .apply_to_task(&mut task)
        .expect("apply required Plugin");

    assert_eq!(task.plugin_config.selected_plugins.len(), 1);
    assert_eq!(
        task.plugin_config.selected_plugins[0].plugin_id,
        "plugin-browser"
    );
}
