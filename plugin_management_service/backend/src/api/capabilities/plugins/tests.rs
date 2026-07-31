// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::{
    parse_plugin_manifest, plugin_component_descriptors, PluginManifestSource,
    PLUGIN_SIGNATURE_ALGORITHM_ED25519,
};

use super::*;

struct PluginRecords {
    catalog: PluginCatalogRecord,
    release: PluginReleaseRecord,
    binding: AgentBindingRecord,
    installation: PluginInstallationRecord,
    preference: UserPluginPreferenceRecord,
}

#[test]
fn exact_ready_installation_resolves_selected_components() {
    let records = plugin_records();
    let snapshots = component_snapshots(&records);
    let resolved = resolve_plugin_records(
        records.catalog,
        Some(records.release),
        records.binding,
        Some(records.installation),
        Some(records.preference),
        snapshots,
        Vec::new(),
        Some("device-1"),
        &std::collections::HashSet::new(),
        true,
    );

    assert!(resolved.available);
    assert_eq!(resolved.status, PluginAvailabilityStatus::Ready);
    assert_eq!(resolved.components.len(), 1);
    assert_eq!(resolved.components[0].component.component_key, "main");
}

#[test]
fn missing_device_fails_closed() {
    let records = plugin_records();
    let snapshots = component_snapshots(&records);
    let resolved = resolve_plugin_records(
        records.catalog,
        Some(records.release),
        records.binding,
        Some(records.installation),
        Some(records.preference),
        snapshots,
        Vec::new(),
        None,
        &std::collections::HashSet::new(),
        true,
    );

    assert!(!resolved.available);
    assert!(resolved
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("device_id")));
}

#[test]
fn artifact_hash_mismatch_fails_closed() {
    let mut records = plugin_records();
    records.installation.artifact_sha256 = "b".repeat(64);
    let snapshots = component_snapshots(&records);
    let resolved = resolve_plugin_records(
        records.catalog,
        Some(records.release),
        records.binding,
        Some(records.installation),
        Some(records.preference),
        snapshots,
        Vec::new(),
        Some("device-1"),
        &std::collections::HashSet::new(),
        true,
    );

    assert!(!resolved.available);
    assert!(resolved
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("artifact hash")));
}

#[test]
fn missing_component_status_fails_closed() {
    let mut records = plugin_records();
    records.installation.component_statuses.clear();
    let snapshots = component_snapshots(&records);
    let resolved = resolve_plugin_records(
        records.catalog,
        Some(records.release),
        records.binding,
        Some(records.installation),
        Some(records.preference),
        snapshots,
        Vec::new(),
        Some("device-1"),
        &std::collections::HashSet::new(),
        true,
    );

    assert!(!resolved.available);
    assert_eq!(
        resolved.components[0].status,
        PluginAvailabilityStatus::Unavailable
    );
}

#[test]
fn missing_immutable_component_snapshot_fails_closed() {
    let records = plugin_records();
    let resolved = resolve_plugin_records(
        records.catalog,
        Some(records.release),
        records.binding,
        Some(records.installation),
        Some(records.preference),
        Vec::new(),
        Vec::new(),
        Some("device-1"),
        &std::collections::HashSet::new(),
        true,
    );

    assert!(!resolved.available);
    assert!(resolved
        .reason
        .as_deref()
        .is_some_and(|reason| reason.contains("immutable Plugin component snapshot")));
}

#[test]
fn portable_host_uses_runtime_project_provider_before_legacy_agent_name() {
    assert!(portable_uses_local(
        Some("local_connector"),
        "chatos_conversation_agent"
    ));
    assert!(!portable_uses_local(
        Some("cloud_sandbox"),
        "task_runner_local_run_phase"
    ));
    assert!(portable_uses_local(None, "task_runner_local_run_phase"));
}

fn component_snapshots(records: &PluginRecords) -> Vec<PluginComponentSnapshot> {
    records
        .release
        .components
        .iter()
        .cloned()
        .map(|component| PluginComponentSnapshot {
            plugin_id: records.catalog.id.clone(),
            release_id: records.release.id.clone(),
            component,
            content_sha256: "d".repeat(64),
        })
        .collect()
}

fn plugin_records() -> PluginRecords {
    let manifest = parse_plugin_manifest(
        r#"{
            "schemaVersion": 1,
            "name": "demo-plugin",
            "version": "1.0.0",
            "description": "Demo plugin",
            "author": {"name": "ChatOS"},
            "mcpServers": {
                "main": {
                    "type": "http",
                    "url": "https://mcp.example.com/v1"
                }
            },
            "interface": {
                "displayName": "Demo Plugin",
                "shortDescription": "Demo",
                "longDescription": "Demo plugin for capability tests.",
                "developerName": "ChatOS",
                "category": "Developer Tools"
            },
            "permissions": ["network.domain:mcp.example.com"]
        }"#,
        PluginManifestSource::Chatos,
    )
    .expect("valid test manifest");
    let components = plugin_component_descriptors(&manifest);
    let catalog = PluginCatalogRecord {
        id: "plugin-1".to_string(),
        plugin_key: "demo-plugin@official".to_string(),
        marketplace_id: "official".to_string(),
        owner_user_id: None,
        name: manifest.name.clone(),
        display_name: "Demo Plugin".to_string(),
        description: manifest.description.clone(),
        publisher: PluginPublisher {
            id: "publisher-1".to_string(),
            name: "ChatOS".to_string(),
            website: None,
            verified: true,
        },
        interface: manifest.interface.clone(),
        keywords: Vec::new(),
        visibility: PLUGIN_VISIBILITY_PUBLIC.to_string(),
        featured: false,
        enabled: true,
        latest_release_id: "release-1".to_string(),
        license: PluginLicenseMetadata {
            license_id: "MIT".to_string(),
            license_url: None,
            redistributable: true,
            reviewed_at: Some("2026-07-22T00:00:00Z".to_string()),
        },
        created_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T00:00:00Z".to_string(),
    };
    let release = PluginReleaseRecord {
        id: "release-1".to_string(),
        plugin_id: catalog.id.clone(),
        version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        artifact_ref: "https://plugins.example.com/demo-plugin.zip".to_string(),
        artifact_sha256: "a".repeat(64),
        signature: PluginReleaseSignature {
            key_id: "key-1".to_string(),
            publisher_id: catalog.publisher.id.clone(),
            marketplace_id: catalog.marketplace_id.clone(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            signature_base64: "signature".to_string(),
            signed_at: "2026-07-22T00:00:00Z".to_string(),
            manifest_sha256: "c".repeat(64),
        },
        sbom_ref: None,
        supported_platforms: Vec::new(),
        components: components.clone(),
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: "stable".to_string(),
        published_at: "2026-07-22T00:00:00Z".to_string(),
        revoked_at: None,
    };
    let binding = AgentBindingRecord {
        id: "binding-1".to_string(),
        agent_key: "task_runner_run_phase".to_string(),
        binding_scope: BINDING_SCOPE_GLOBAL_DEFAULT.to_string(),
        owner_user_id: None,
        resource_kind: RESOURCE_KIND_PLUGIN_COMPONENT.to_string(),
        resource_id: catalog.id.clone(),
        enabled: true,
        required: false,
        priority: 500,
        conditions: BindingConditions::default(),
        component_allowlist: vec!["main".to_string()],
        created_by: "admin".to_string(),
        updated_by: "admin".to_string(),
        created_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T00:00:00Z".to_string(),
    };
    let installation = PluginInstallationRecord {
        id: "user-1:device-1:plugin-1".to_string(),
        owner_user_id: "user-1".to_string(),
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
                last_checked_at: "2026-07-22T00:00:00Z".to_string(),
            })
            .collect(),
        active: true,
        previous_release_id: None,
        installed_at: "2026-07-22T00:00:00Z".to_string(),
        last_checked_at: "2026-07-22T00:00:00Z".to_string(),
        last_error: None,
    };
    let preference = UserPluginPreferenceRecord {
        owner_user_id: "user-1".to_string(),
        plugin_id: catalog.id.clone(),
        enabled: true,
        auto_update: false,
        release_channel: "stable".to_string(),
        enabled_components: vec!["main".to_string()],
        updated_at: "2026-07-22T00:00:00Z".to_string(),
    };
    PluginRecords {
        catalog,
        release,
        binding,
        installation,
        preference,
    }
}
