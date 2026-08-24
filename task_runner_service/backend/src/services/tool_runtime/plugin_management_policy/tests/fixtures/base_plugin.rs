// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    parse_plugin_manifest, plugin_component_descriptors, AgentBindingRecord, BindingConditions,
    PluginAvailabilityStatus, PluginCatalogRecord, PluginComponentSnapshot, PluginComponentStatus,
    PluginInstallStatus, PluginInstallationRecord, PluginLicenseMetadata, PluginPublisher,
    PluginReleaseRecord, PluginReleaseSignature, PluginRequirementStatus, ResolvedPlugin,
    ResolvedPluginComponent, SystemAgentKey, UserPluginPreferenceRecord,
    PLUGIN_SIGNATURE_ALGORITHM_ED25519,
};

pub(in super::super) fn resolved_plugin(required: bool) -> ResolvedPlugin {
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 3,
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
        npm_package: chatos_plugin_management_sdk::PluginNpmPackage {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            integrity: "sha512-dGVzdA==".to_string(),
        },
        artifact_ref: "https://registry.npmjs.org/browser/-/browser-1.0.0.tgz".to_string(),
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
        granted_permissions: release
            .permissions
            .iter()
            .map(|permission| permission.permission.clone())
            .collect(),
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
        tool_allowlist: Vec::new(),
        tool_blocklist: Vec::new(),
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
