// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Write};

use chatos_plugin_management_sdk::{
    build_plugin_mcp_portable_runtime_bundle, parse_plugin_manifest, plugin_component_descriptors,
    PluginManifestSource, PluginReleaseRecord, PluginReleaseSignature, SystemAgentKey,
};
use chatos_plugin_package::{
    build_plugin_mcp_portable_runtime_bundles_from_package, build_portable_component_bundles,
    load_verified_plugin_package_directory, verify_plugin_archive_bytes,
    verify_plugin_mcp_portable_artifact_bytes, verify_plugin_mcp_portable_package,
    PluginPackageLimits, VerifiedPluginPackage,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

fn manifest_raw() -> String {
    json!({
        "schemaVersion": 2,
        "execution": {"defaultHost": "portable", "componentHosts": {}},
        "name": "package-demo",
        "version": "1.0.0",
        "description": "Package verifier fixture",
        "author": {"name": "ChatOS"},
        "skills": ["./skills/demo"],
        "mcpServers": {
            "runner": {
                "type": "stdio",
                "command": "./bin/server",
                "args": [],
                "cwd": "./bin"
            }
        },
        "commands": [{
            "componentKey": "review",
            "source": "./commands/review.md",
            "targetAgent": RUN_AGENT_KEY
        }],
        "interface": {
            "displayName": "Package Demo",
            "shortDescription": "Package demo",
            "longDescription": "Package verifier and Bundle fixture.",
            "developerName": "ChatOS",
            "category": "Developer Tools"
        },
        "dependencies": {},
        "permissions": [{"permission": "process.spawn", "components": ["runner"]}]
    })
    .to_string()
}

fn package_files(skill_text: &str) -> BTreeMap<String, Vec<u8>> {
    BTreeMap::from([
        (
            ".chatos-plugin/plugin.json".to_string(),
            manifest_raw().into_bytes(),
        ),
        ("bin/server".to_string(), b"#!/bin/sh\nexit 0\n".to_vec()),
        (
            "commands/review.md".to_string(),
            b"Review the smallest correct change.".to_vec(),
        ),
        (
            "references/guide.md".to_string(),
            b"Prefer existing code and platform primitives.".to_vec(),
        ),
        (
            "sbom.spdx.json".to_string(),
            serde_json::to_vec(&json!({
                "spdxVersion": "SPDX-2.3",
                "SPDXID": "SPDXRef-DOCUMENT"
            }))
            .expect("SBOM JSON"),
        ),
        (
            "skills/demo/SKILL.md".to_string(),
            skill_text.as_bytes().to_vec(),
        ),
    ])
}

#[test]
fn portable_mcp_artifact_is_bound_to_the_runtime_bundle_and_file_checksums() {
    let files = with_checksums(package_files("---\nname: demo\n---\nInstructions"));
    let bytes = normal_archive(&files);
    let release = release(sha256(bytes.as_slice()));
    let bundle = build_plugin_mcp_portable_runtime_bundle(&release, "runner")
        .expect("portable MCP runtime Bundle");
    let package = verify_plugin_mcp_portable_artifact_bytes(
        bytes.as_slice(),
        &bundle,
        PluginPackageLimits::default(),
    )
    .expect("verified portable MCP artifact");
    assert_eq!(
        package.file_sha256["bin/server"],
        sha256(&files["bin/server"])
    );

    let mut drifted = bundle.clone();
    let chatos_plugin_management_sdk::PluginMcpServer::Stdio { command, .. } = &mut drifted.runtime
    else {
        unreachable!();
    };
    *command = "./bin/other".to_string();
    assert!(verify_plugin_mcp_portable_artifact_bytes(
        bytes.as_slice(),
        &drifted,
        PluginPackageLimits::default(),
    )
    .is_err());
}

#[test]
fn config_file_runtime_candidates_are_frozen_from_the_verified_artifact() {
    let manifest_raw = json!({
        "schemaVersion": 2,
        "execution": {"defaultHost": "portable", "componentHosts": {}},
        "name": "config-package-demo",
        "version": "1.0.0",
        "description": "Config runtime fixture",
        "author": {"name": "ChatOS"},
        "mcpServers": "./.mcp.json",
        "interface": {
            "displayName": "Config Package Demo",
            "shortDescription": "Config package demo",
            "longDescription": "Config runtime fixture.",
            "developerName": "ChatOS",
            "category": "Developer Tools"
        },
        "dependencies": {},
        "permissions": [
            {"permission": "process.spawn", "components": ["mcp-config"]},
            {"permission": "network.domain:api.example.com", "components": ["mcp-config"]}
        ]
    })
    .to_string();
    let manifest =
        parse_plugin_manifest(manifest_raw.as_str(), PluginManifestSource::Chatos).unwrap();
    let release = PluginReleaseRecord {
        id: "release-config-1".to_string(),
        plugin_id: "plugin-config-1".to_string(),
        version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        artifact_ref: "https://plugins.example.com/config-demo.zip".to_string(),
        artifact_sha256: "a".repeat(64),
        signature: PluginReleaseSignature {
            key_id: "fixture-key".to_string(),
            publisher_id: "publisher-1".to_string(),
            marketplace_id: "marketplace-1".to_string(),
            algorithm: "ed25519".to_string(),
            signature_base64: "fixture".to_string(),
            signed_at: "2026-07-30T00:00:00Z".to_string(),
            manifest_sha256: "b".repeat(64),
        },
        sbom_ref: Some("./sbom.spdx.json".to_string()),
        supported_platforms: manifest.dependencies.supported_platforms.clone(),
        components: plugin_component_descriptors(&manifest),
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: "stable".to_string(),
        published_at: "2026-07-30T00:00:00Z".to_string(),
        revoked_at: None,
    };
    let config = serde_json::to_vec(&json!({
        "mcpServers": {
            "api": {"url": "https://api.example.com/mcp"},
            "runner": {"command": "./bin/server", "args": ["--stdio"]}
        }
    }))
    .unwrap();
    let package = VerifiedPluginPackage {
        manifest,
        manifest_source: PluginManifestSource::Chatos,
        artifact_sha256: release.artifact_sha256.clone(),
        file_sha256: BTreeMap::from([(".mcp.json".to_string(), sha256(config.as_slice()))]),
        files: BTreeMap::from([(".mcp.json".to_string(), config)]),
        unpacked_bytes: 1,
    };
    let bundles =
        build_plugin_mcp_portable_runtime_bundles_from_package(&release, "mcp-config", &package)
            .unwrap();
    assert_eq!(
        bundles
            .iter()
            .map(|bundle| bundle.server_key.as_str())
            .collect::<Vec<_>>(),
        vec!["api", "runner"]
    );
    assert!(bundles
        .iter()
        .all(|bundle| verify_plugin_mcp_portable_package(&package, bundle).is_ok()));
    assert_ne!(bundles[0].bundle_sha256, bundles[1].bundle_sha256);

    let mut drifted = bundles[0].clone();
    drifted.server_key = "runner".to_string();
    assert!(verify_plugin_mcp_portable_package(&package, &drifted).is_err());
}

fn with_checksums(mut files: BTreeMap<String, Vec<u8>>) -> BTreeMap<String, Vec<u8>> {
    let checksums = files
        .iter()
        .map(|(path, bytes)| (path.clone(), sha256(bytes)))
        .collect::<BTreeMap<_, _>>();
    files.insert(
        ".chatos-plugin/checksums.json".to_string(),
        serde_json::to_vec(&json!({"schemaVersion": 1, "files": checksums}))
            .expect("checksum JSON"),
    );
    files
}

fn archive(entries: &[(String, Vec<u8>, u32)]) -> Vec<u8> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut cursor);
        for (path, bytes, mode) in entries {
            writer
                .start_file(
                    path,
                    SimpleFileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .unix_permissions(*mode),
                )
                .expect("start ZIP file");
            writer.write_all(bytes).expect("write ZIP file");
        }
        writer.finish().expect("finish ZIP");
    }
    cursor.into_inner()
}

fn normal_archive(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    archive(
        &files
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.clone(), 0o100644))
            .collect::<Vec<_>>(),
    )
}

fn release(artifact_sha256: String) -> PluginReleaseRecord {
    let manifest = parse_plugin_manifest(manifest_raw().as_str(), PluginManifestSource::Chatos)
        .expect("fixture Manifest");
    PluginReleaseRecord {
        id: "release-1".to_string(),
        plugin_id: "plugin-1".to_string(),
        version: manifest.version.clone(),
        manifest_schema_version: manifest.schema_version,
        normalized_manifest: manifest.clone(),
        artifact_ref: "https://plugins.example.com/package-demo.zip".to_string(),
        artifact_sha256,
        signature: PluginReleaseSignature {
            key_id: "fixture-key".to_string(),
            publisher_id: "publisher-1".to_string(),
            marketplace_id: "marketplace-1".to_string(),
            algorithm: "ed25519".to_string(),
            signature_base64: "fixture".to_string(),
            signed_at: "2026-07-30T00:00:00Z".to_string(),
            manifest_sha256: "a".repeat(64),
        },
        sbom_ref: Some("./sbom.spdx.json".to_string()),
        supported_platforms: manifest.dependencies.supported_platforms.clone(),
        components: plugin_component_descriptors(&manifest),
        dependencies: manifest.dependencies.clone(),
        permissions: manifest.permissions.clone(),
        release_channel: "stable".to_string(),
        published_at: "2026-07-30T00:00:00Z".to_string(),
        revoked_at: None,
    }
}

#[test]
fn archive_and_installed_directory_produce_the_same_stable_bundle_hash() {
    let files = with_checksums(package_files(
        "---\nname: demo\n---\nUse the guide. [Guide](../../references/guide.md)",
    ));
    let bytes = normal_archive(&files);
    let release = release(sha256(bytes.as_slice()));
    let package =
        verify_plugin_archive_bytes(bytes.as_slice(), &release, PluginPackageLimits::default())
            .expect("verified archive");
    let archive_bundles =
        build_portable_component_bundles(&release, &package, "ingested").expect("archive Bundles");

    let directory = TempDir::new().expect("temporary Plugin directory");
    for (path, body) in &files {
        let output = directory.path().join(path);
        fs::create_dir_all(output.parent().expect("file parent")).expect("create parent");
        fs::write(output, body).expect("write package file");
    }
    let directory_package = load_verified_plugin_package_directory(
        directory.path(),
        release.artifact_sha256.as_str(),
        &package.file_sha256,
        PluginPackageLimits::default(),
    )
    .expect("verified installed directory");
    let directory_bundles =
        build_portable_component_bundles(&release, &directory_package, "installed")
            .expect("directory Bundles");
    assert_eq!(
        archive_bundles
            .iter()
            .map(|bundle| (&bundle.component_key, &bundle.bundle_sha256))
            .collect::<Vec<_>>(),
        directory_bundles
            .iter()
            .map(|bundle| (&bundle.component_key, &bundle.bundle_sha256))
            .collect::<Vec<_>>()
    );
}

#[test]
fn checksum_index_must_cover_every_file_exactly() {
    let mut files = with_checksums(package_files("---\nname: demo\n---\nInstructions"));
    let index_path = ".chatos-plugin/checksums.json";
    let mut index: serde_json::Value =
        serde_json::from_slice(&files[index_path]).expect("checksum index");
    index["files"]
        .as_object_mut()
        .expect("checksum files")
        .remove("commands/review.md");
    files.insert(
        index_path.to_string(),
        serde_json::to_vec(&index).expect("checksum JSON"),
    );
    let bytes = normal_archive(&files);
    let error = verify_plugin_archive_bytes(
        bytes.as_slice(),
        &release(sha256(bytes.as_slice())),
        PluginPackageLimits::default(),
    )
    .expect_err("incomplete checksums must fail");
    assert!(error.to_string().contains("cover every package file"));
}

#[test]
fn zip_traversal_case_collisions_and_symlinks_are_rejected() {
    for entries in [
        vec![("../escape".to_string(), b"bad".to_vec(), 0o100644)],
        vec![
            ("A.txt".to_string(), b"one".to_vec(), 0o100644),
            ("a.txt".to_string(), b"two".to_vec(), 0o100644),
        ],
        vec![("link".to_string(), b"target".to_vec(), 0o120777)],
    ] {
        let bytes = archive(entries.as_slice());
        assert!(verify_plugin_archive_bytes(
            bytes.as_slice(),
            &release(sha256(bytes.as_slice())),
            PluginPackageLimits::default(),
        )
        .is_err());
    }
}

#[test]
fn portable_skill_references_cannot_escape_or_load_scripts_and_binary_files() {
    for skill in [
        "---\nname: demo\n---\n[Escape](../../../outside.md)",
        "---\nname: demo\n---\n[Script](../../scripts/run.sh)",
        "---\nname: demo\n---\n[Binary](../../references/image.png)",
    ] {
        let mut files = package_files(skill);
        files.insert("scripts/run.sh".to_string(), b"exit 0".to_vec());
        files.insert("references/image.png".to_string(), vec![0, 1, 2]);
        let files = with_checksums(files);
        let bytes = normal_archive(&files);
        let release = release(sha256(bytes.as_slice()));
        let package =
            verify_plugin_archive_bytes(bytes.as_slice(), &release, PluginPackageLimits::default())
                .expect("archive verification");
        assert!(build_portable_component_bundles(&release, &package, "ingested").is_err());
    }
}

#[test]
fn portable_skill_reference_cycles_are_rejected() {
    let mut files = package_files("---\nname: demo\n---\nStart with [A](../../references/a.md).");
    files.insert(
        "references/a.md".to_string(),
        b"Continue to [B](b.md).".to_vec(),
    );
    files.insert(
        "references/b.md".to_string(),
        b"Return to [A](a.md).".to_vec(),
    );
    let files = with_checksums(files);
    let bytes = normal_archive(&files);
    let release = release(sha256(bytes.as_slice()));
    let package =
        verify_plugin_archive_bytes(bytes.as_slice(), &release, PluginPackageLimits::default())
            .expect("archive verification");
    let error = build_portable_component_bundles(&release, &package, "ingested")
        .expect_err("reference cycle must fail");
    assert!(error
        .to_string()
        .contains("reference graph contains a cycle"));
}

#[test]
fn archive_entry_and_size_limits_are_enforced() {
    let files = with_checksums(package_files("---\nname: demo\n---\nInstructions"));
    let bytes = normal_archive(&files);
    let release = release(sha256(bytes.as_slice()));

    let entry_error = verify_plugin_archive_bytes(
        bytes.as_slice(),
        &release,
        PluginPackageLimits {
            max_entries: 1,
            ..PluginPackageLimits::default()
        },
    )
    .expect_err("entry limit must fail");
    assert!(entry_error.to_string().contains("too many entries"));

    let size_error = verify_plugin_archive_bytes(
        bytes.as_slice(),
        &release,
        PluginPackageLimits {
            max_file_bytes: 8,
            ..PluginPackageLimits::default()
        },
    )
    .expect_err("file size limit must fail");
    assert!(size_error
        .to_string()
        .contains("file exceeds the size limit"));
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
