use super::{analyze_resolved_project, analyzer::analyze_project};
use crate::workspace_index::open_project_registry;
use rusqlite::params;
use serde_json::json;
use std::{fs, path::Path};

#[test]
fn analyzer_matches_macos_target_priority_and_uses_only_relative_paths() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir_all(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='demo'\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("package.json"),
        json!({"scripts": {"dev": "vite", "test": "vitest"}}).to_string(),
    )
    .unwrap();

    let analysis = analyze_project(directory.path()).unwrap();

    assert_eq!(analysis.targets[0].presentation.command, "npm run dev");
    assert!(analysis.targets[0].presentation.is_default);
    assert!(analysis
        .targets
        .iter()
        .any(|target| target.presentation.command == "cargo run"));
    assert!(analysis
        .configuration_files
        .iter()
        .all(|file| !Path::new(&file.path).is_absolute()));
    let serialized = serde_json::to_string(&analysis.configuration_files).unwrap();
    assert!(!serialized.contains(&directory.path().to_string_lossy().to_string()));
}

#[test]
fn oversized_manifest_is_not_parsed_or_previewed() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("package.json"),
        vec![b' '; 2 * 1_024 * 1_024 + 1],
    )
    .unwrap();

    let analysis = analyze_project(directory.path()).unwrap();

    assert!(analysis.targets.is_empty());
    assert_eq!(analysis.configuration_files.len(), 1);
    assert!(analysis.configuration_files[0].preview.is_none());
}

#[cfg(unix)]
#[test]
fn symlinked_manifest_and_directory_are_ignored() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(
        outside.path().join("package.json"),
        json!({"scripts": {"dev": "outside"}}).to_string(),
    )
    .unwrap();
    symlink(
        outside.path().join("package.json"),
        directory.path().join("package.json"),
    )
    .unwrap();
    symlink(outside.path(), directory.path().join("linked")).unwrap();

    let analysis = analyze_project(directory.path()).unwrap();

    assert!(analysis.targets.is_empty());
    assert!(analysis.configuration_files.is_empty());
}

#[test]
fn resolved_projection_enforces_owner_project_workspace_and_hides_absolute_paths() {
    let app_data = tempfile::tempdir().unwrap();
    let project = tempfile::tempdir().unwrap();
    let connector = app_data.path().join("NativeConnector");
    fs::create_dir_all(&connector).unwrap();
    let connection = open_project_registry(&connector.join("Projects.sqlite3")).unwrap();
    connection
        .execute(
            "INSERT INTO local_project_records(\
             id, owner_user_id, name, description, workspace_id, relative_root, \
             project_type_key, revision, status, created_at_unix_ms, updated_at_unix_ms\
             ) VALUES (?1, ?2, 'Project', '', ?3, '', 'software_development', 1, 'active', 1, 1)",
            params!["project", "owner", "workspace"],
        )
        .unwrap();
    fs::write(
        connector.join("state.json"),
        json!({
            "user": {"id": "owner"},
            "deviceID": "device",
            "workspaces": [{
                "id": "workspace",
                "absoluteRoot": project.path(),
                "fingerprint": "fingerprint"
            }]
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        project.path().join("package.json"),
        json!({"scripts": {"start": "node index.js"}}).to_string(),
    )
    .unwrap();

    let snapshot = analyze_resolved_project(app_data.path(), "owner", "project").unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();

    assert_eq!(snapshot.catalog.project_id, "project");
    assert_eq!(snapshot.catalog.targets[0].cwd, ".");
    assert!(!serialized.contains(&project.path().to_string_lossy().to_string()));
    assert!(analyze_resolved_project(app_data.path(), "other", "project").is_err());
}
