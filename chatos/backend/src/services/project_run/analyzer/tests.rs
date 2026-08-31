// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde_json::json;

use super::local_connector::{
    is_ignored_local_connector_dir, local_listing_entries, push_local_connector_maven_targets,
    push_local_connector_node_targets, sort_local_connector_targets,
};
use super::*;

fn local_connector_project() -> Project {
    Project::new(
        "vrad-backend".to_string(),
        "local://connector/device/workspace/zj/ewo/vrad-backend".to_string(),
        None,
        None,
        Some("user_1".to_string()),
    )
}

#[test]
fn local_connector_maven_spring_boot_targets_use_pom_manifest() {
    let project = local_connector_project();
    let root_entries = HashSet::from(["pom.xml".to_string()]);
    let pom = r#"
        <project>
          <parent>
            <groupId>org.springframework.boot</groupId>
            <artifactId>spring-boot-starter-parent</artifactId>
          </parent>
          <properties>
            <mainClass>com.example.VradApplication</mainClass>
          </properties>
        </project>
    "#;
    let mut targets = Vec::new();

    push_local_connector_maven_targets(
        project.root_path.as_str(),
        format!("{}/pom.xml", project.root_path).as_str(),
        &root_entries,
        Some(pom),
        &mut targets,
    );
    sort_local_connector_targets(&mut targets);

    let commands = targets
        .iter()
        .map(|target| target.command.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        commands,
        vec![
            "mvn -Dspring-boot.run.main-class=com.example.VradApplication spring-boot:run",
            "mvn test",
        ]
    );
    assert_eq!(targets[0].kind, "java");
    assert_eq!(targets[0].source, "local_connector_maven");
    assert_eq!(
        targets[0].manifest_path.as_deref(),
        Some("local://connector/device/workspace/zj/ewo/vrad-backend/pom.xml")
    );
    assert!(targets[0].required_toolchains.is_empty());
}

#[test]
fn local_connector_nested_manifests_keep_their_own_cwd_and_unique_ids() {
    let project = local_connector_project();
    let frontend_cwd = format!("{}/frontend", project.root_path);
    let admin_cwd = format!("{}/apps/admin", project.root_path);
    let entries = HashSet::from(["package.json".to_string(), "package-lock.json".to_string()]);
    let package = r#"{
        "scripts": {
            "dev": "vite",
            "test": "vitest run"
        }
    }"#;
    let mut targets = Vec::new();

    push_local_connector_node_targets(
        frontend_cwd.as_str(),
        format!("{frontend_cwd}/package.json").as_str(),
        &entries,
        package,
        &mut targets,
    );
    push_local_connector_node_targets(
        admin_cwd.as_str(),
        format!("{admin_cwd}/package.json").as_str(),
        &entries,
        package,
        &mut targets,
    );

    assert_eq!(targets.len(), 4);
    assert!(targets.iter().any(|target| {
        target.cwd == frontend_cwd
            && target.command == "npm run dev"
            && target.manifest_path.as_deref()
                == Some(format!("{frontend_cwd}/package.json").as_str())
    }));
    assert!(targets.iter().any(|target| {
        target.cwd == admin_cwd
            && target.command == "npm run dev"
            && target.manifest_path.as_deref() == Some(format!("{admin_cwd}/package.json").as_str())
    }));
    let ids = targets
        .iter()
        .map(|target| target.id.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(ids.len(), targets.len());
}

#[test]
fn local_connector_directory_entries_are_scoped_and_ignored_consistently() {
    let entries = local_listing_entries(
        &json!({
            "entries": [
                { "name": "src", "type": "dir" },
                { "name": "package.json", "type": "file" },
                { "name": "../escape", "type": "dir" }
            ]
        }),
        "frontend",
    );

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].path, "frontend/src");
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].path, "frontend/package.json");
    assert!(!entries[1].is_dir);
    assert!(is_ignored_local_connector_dir("node_modules"));
    assert!(is_ignored_local_connector_dir("TARGET"));
    assert!(!is_ignored_local_connector_dir("backend"));
}
