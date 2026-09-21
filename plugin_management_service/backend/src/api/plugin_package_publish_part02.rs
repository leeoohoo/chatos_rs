#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    fn package_fixture(manifest: &str) -> Vec<u8> {
        package_fixture_with_bin_file(manifest, true)
    }

    fn package_fixture_with_bin_file(manifest: &str, include_bin_file: bool) -> Vec<u8> {
        package_fixture_with_files(manifest, include_bin_file, &[])
    }

    fn package_fixture_with_files(
        manifest: &str,
        include_bin_file: bool,
        files: &[(&str, &[u8])],
    ) -> Vec<u8> {
        let mut encoded = Vec::new();
        {
            let gzip = GzEncoder::new(&mut encoded, Compression::default());
            let mut archive = tar::Builder::new(gzip);
            append(
                &mut archive,
                PACKAGE_JSON_PATH,
                br#"{"name":"demo-mcp","version":"1.0.0","bin":{"demo-mcp":"dist/cli.js"}}"#,
            );
            append(&mut archive, MANIFEST_PATHS[0], manifest.as_bytes());
            if include_bin_file {
                append(
                    &mut archive,
                    "package/dist/cli.js",
                    b"#!/usr/bin/env node\n",
                );
            }
            for (path, bytes) in files {
                append(&mut archive, path, bytes);
            }
            archive
                .into_inner()
                .expect("gzip")
                .finish()
                .expect("finish");
        }
        encoded
    }

    fn append<W: Write>(archive: &mut tar::Builder<W>, path: &str, bytes: &[u8]) {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        archive
            .append_data(&mut header, path, bytes)
            .expect("append");
    }

    #[test]
    fn uploaded_package_parses_manifest_and_requires_declared_stdio_bin() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let parsed = parse_npm_package(package_fixture(manifest).as_slice(), None).expect("parse");
        assert_eq!(parsed.package_name, "demo-mcp");
        assert_eq!(parsed.package_bins, vec!["demo-mcp"]);
        assert_eq!(parsed.manifest.mcp_servers.len(), 1);
    }

    #[test]
    fn uploaded_package_analyzes_each_declared_skill_snapshot() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "skills":["./skills/demo-router","./skills/demo-leaf"],
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let router = br#"---
name: demo-router
description: Route demo work.
metadata:
  chatos.role: router
  chatos.required-skills: demo-leaf
---
# Demo router
Activate the leaf when needed.
"#;
        let leaf = br#"---
name: demo-leaf
description: Complete one focused demo operation.
---
# Demo leaf
Read the example only when it is needed.
"#;
        let package = package_fixture_with_files(
            manifest,
            true,
            &[
                ("package/skills/demo-router/SKILL.md", router),
                ("package/skills/demo-leaf/SKILL.md", leaf),
                (
                    "package/skills/demo-leaf/references/example.md",
                    b"# Example\n",
                ),
            ],
        );
        let parsed = parse_npm_package(package.as_slice(), None).expect("parse");
        assert_eq!(parsed.skill_snapshots.len(), 2);
        let router = parsed
            .skill_snapshots
            .iter()
            .find(|snapshot| snapshot.skill_id == "demo-router")
            .expect("router");
        assert_eq!(
            router.metadata.role,
            chatos_plugin_management_sdk::SkillRole::Router
        );
        assert_eq!(router.metadata.required_skills, ["demo-leaf"]);
        let leaf = parsed
            .skill_snapshots
            .iter()
            .find(|snapshot| snapshot.skill_id == "demo-leaf")
            .expect("leaf");
        assert_eq!(leaf.resources.len(), 1);
        assert_eq!(leaf.resources[0].kind, SkillResourceKind::Reference);
        assert_eq!(leaf.instructions_sha256.len(), 64);
        assert_eq!(leaf.resource_manifest_sha256.len(), 64);
        assert_eq!(leaf.snapshot_sha256.len(), 64);
    }

    #[test]
    fn uploaded_package_rejects_missing_required_skill_dependency() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "skills":["./skills/demo-router"],
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let router = br#"---
name: demo-router
description: Route demo work.
metadata:
  chatos.required-skills: missing-leaf
---
# Demo router
Activate the leaf.
"#;
        let package = package_fixture_with_files(
            manifest,
            true,
            &[("package/skills/demo-router/SKILL.md", router)],
        );
        let error = parse_npm_package(package.as_slice(), None).unwrap_err();
        assert!(error
            .message
            .contains("references missing Skill missing-leaf"));
    }

    #[test]
    fn uploaded_package_rejects_stdio_bin_missing_from_package_json() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "mcpServers":{"demo":{"type":"stdio","bin":"missing-bin"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let error = parse_npm_package(package_fixture(manifest).as_slice(), None).unwrap_err();
        assert!(error.message.contains("not declared"));
    }

    #[test]
    fn uploaded_package_rejects_declared_bin_file_missing_from_archive() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo MCP",
          "author":{"name":"Demo"},
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "interface":{"displayName":"Demo MCP","shortDescription":"Demo","longDescription":"Demo MCP","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}]
        }"#;
        let error = parse_npm_package(
            package_fixture_with_bin_file(manifest, false).as_slice(),
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("missing from the npm package"));
    }

    #[test]
    fn uploaded_package_accepts_declared_local_ui_runtime_bin() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo UI",
          "author":{"name":"Demo"},
          "ui":[{"componentKey":"workbench","source":"./ui/index.html","surface":"workbench","runtime":{"type":"local_http","bin":"demo-mcp","args":["studio"]}}],
          "interface":{"displayName":"Demo UI","shortDescription":"Demo","longDescription":"Demo UI","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start UI","components":["workbench"]}]
        }"#;
        let parsed = parse_npm_package(package_fixture(manifest).as_slice(), None).expect("parse");
        assert_eq!(parsed.package_bins, vec!["demo-mcp"]);
        assert_eq!(parsed.manifest.ui.len(), 1);
    }

    #[test]
    fn uploaded_package_rejects_local_ui_runtime_bin_missing_from_package_json() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo UI",
          "author":{"name":"Demo"},
          "ui":[{"componentKey":"workbench","source":"./ui/index.html","surface":"workbench","runtime":{"type":"local_http","bin":"missing-bin"}}],
          "interface":{"displayName":"Demo UI","shortDescription":"Demo","longDescription":"Demo UI","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start UI","components":["workbench"]}]
        }"#;
        let error = parse_npm_package(package_fixture(manifest).as_slice(), None).unwrap_err();
        assert!(error.message.contains("UI runtime bin missing-bin"));
        assert!(error.message.contains("not declared"));
    }

    #[test]
    fn uploaded_package_rejects_local_ui_runtime_bin_file_missing_from_archive() {
        let manifest = r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Demo UI",
          "author":{"name":"Demo"},
          "ui":[{"componentKey":"workbench","source":"./ui/index.html","surface":"workbench","runtime":{"type":"local_http","bin":"demo-mcp"}}],
          "interface":{"displayName":"Demo UI","shortDescription":"Demo","longDescription":"Demo UI","developerName":"Demo","category":"Developer Tools"},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start UI","components":["workbench"]}]
        }"#;
        let error = parse_npm_package(
            package_fixture_with_bin_file(manifest, false).as_slice(),
            None,
        )
        .unwrap_err();
        assert!(error.message.contains("missing from the npm package"));
    }

    #[test]
    fn existing_release_refresh_preserves_catalog_governance_metadata() {
        let old_manifest = parse_plugin_manifest(r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.0.0",
          "description":"Old description",
          "author":{"name":"Old Publisher"},
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}],
          "interface":{"displayName":"Old Name","shortDescription":"Old","longDescription":"Old description","developerName":"Old Publisher","category":"Developer Tools"}
        }"#).expect("old manifest");
        let new_manifest = parse_plugin_manifest(r#"{
          "schemaVersion":3,
          "name":"demo-mcp",
          "version":"1.1.0",
          "description":"New description",
          "author":{"name":"New Publisher"},
          "keywords":["browser","automation"],
          "mcpServers":{"demo":{"type":"stdio","bin":"demo-mcp"}},
          "permissions":[{"permission":"process.spawn","required":true,"reason":"Start MCP","components":["demo"]}],
          "interface":{"displayName":"New Name","shortDescription":"New","longDescription":"New description","developerName":"New Publisher","category":"Productivity"}
        }"#).expect("new manifest");
        let mut catalog = PluginCatalogRecord {
            id: "plugin-id".to_string(),
            plugin_key: "demo-mcp@marketplace".to_string(),
            marketplace_id: "marketplace".to_string(),
            owner_user_id: None,
            name: "demo-mcp".to_string(),
            display_name: old_manifest.interface.display_name.clone(),
            description: old_manifest.description.clone(),
            publisher: PluginPublisher {
                id: "publisher".to_string(),
                name: "Old Publisher".to_string(),
                website: None,
                verified: true,
            },
            interface: old_manifest.interface,
            keywords: Vec::new(),
            visibility: "private".to_string(),
            featured: true,
            enabled: true,
            has_ui: false,
            latest_release_id: "release-id".to_string(),
            license: PluginLicenseMetadata {
                license_id: "Apache-2.0".to_string(),
                license_url: Some("https://www.apache.org/licenses/LICENSE-2.0".to_string()),
                redistributable: true,
                reviewed_at: Some("2026-09-03T00:00:00Z".to_string()),
            },
            created_at: "2026-09-03T00:00:00Z".to_string(),
            updated_at: "2026-09-03T00:00:00Z".to_string(),
        };
        let governance_before = (
            catalog.visibility.clone(),
            catalog.featured,
            catalog.license.clone(),
            catalog.latest_release_id.clone(),
        );

        apply_uploaded_presentation_metadata(
            &mut catalog,
            &new_manifest,
            PluginPublisher {
                id: "publisher".to_string(),
                name: "New Publisher".to_string(),
                website: Some("https://example.com".to_string()),
                verified: true,
            },
        );

        assert_eq!(catalog.display_name, "New Name");
        assert_eq!(catalog.description, "New description");
        assert_eq!(catalog.publisher.name, "New Publisher");
        assert_eq!(catalog.keywords, vec!["automation", "browser"]);
        assert_eq!(
            (
                catalog.visibility,
                catalog.featured,
                catalog.license,
                catalog.latest_release_id,
            ),
            governance_before,
        );
    }
}
