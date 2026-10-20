use super::{
    analyze_resolved_project,
    analyzer::{analyze_project, resolve_project_run_target},
    models::{ProjectRunTarget, ResolvedProjectRunTarget},
    preferences::{tests_support::memory_preferences, ProjectRunEnvironmentMutation},
    runtime::{test_scope, ProjectRunManager},
};
use crate::workspace_index::open_project_registry;
use rusqlite::params;
use serde_json::json;
use std::{fs, path::Path, time::Duration};

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

#[test]
fn launch_resolution_accepts_only_a_fresh_analyzer_target_id() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("src")).unwrap();
    fs::write(directory.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        directory.path().join("Cargo.toml"),
        "[package]\nname='typed-run'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    let analysis = analyze_project(directory.path()).unwrap();
    let target_id = analysis
        .targets
        .iter()
        .find(|target| target.presentation.command == "cargo run")
        .unwrap()
        .presentation
        .id
        .clone();

    let resolved =
        resolve_project_run_target(directory.path(), &target_id, &Default::default()).unwrap();

    assert_eq!(resolved.arguments, ["run"]);
    assert_eq!(resolved.cwd, fs::canonicalize(directory.path()).unwrap());
    assert_eq!(
        resolved
            .executable
            .file_stem()
            .and_then(|value| value.to_str()),
        Some("cargo")
    );
    assert!(
        resolve_project_run_target(directory.path(), "invented-target", &Default::default())
            .is_err()
    );
}

#[test]
fn preferences_persist_only_secret_references_and_project_masked_values() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("preferences.json");
    let preferences = memory_preferences(path.clone());
    let selected = std::collections::BTreeMap::from([(
        "cargo".into(),
        "tool_0123456789abcdef01234567".into(),
    )]);

    preferences
        .update_environment(
            "owner",
            "project",
            selected.clone(),
            vec![ProjectRunEnvironmentMutation {
                key: "API_TOKEN".into(),
                value: Some("never-write-this-value".into()),
            }],
        )
        .unwrap();

    let persisted = fs::read_to_string(&path).unwrap();
    assert!(persisted.contains("API_TOKEN"));
    assert!(!persisted.contains("never-write-this-value"));
    let (selection, projection) = preferences.projection("owner", "project").unwrap();
    assert_eq!(selection.selected_toolchains, selected);
    assert_eq!(projection.len(), 1);
    assert_eq!(projection[0].key, "API_TOKEN");
    assert!(projection[0].has_value);
    assert!(!serde_json::to_string(&projection)
        .unwrap()
        .contains("never-write-this-value"));
    assert_eq!(
        preferences
            .launch_preferences("owner", "project")
            .unwrap()
            .environment["API_TOKEN"],
        "never-write-this-value"
    );

    preferences
        .update_environment(
            "owner",
            "project",
            Default::default(),
            vec![ProjectRunEnvironmentMutation {
                key: "API_TOKEN".into(),
                value: None,
            }],
        )
        .unwrap();
    assert_eq!(
        preferences
            .launch_preferences("owner", "project")
            .unwrap()
            .environment["API_TOKEN"],
        "never-write-this-value"
    );

    preferences
        .update_environment("owner", "project", Default::default(), Vec::new())
        .unwrap();
    assert!(preferences
        .launch_preferences("owner", "project")
        .unwrap()
        .environment
        .is_empty());
}

#[test]
fn preferences_reject_invalid_environment_and_persist_default_target() {
    let directory = tempfile::tempdir().unwrap();
    let preferences = memory_preferences(directory.path().join("preferences.json"));
    let target_id = "0123456789abcdef0123";

    preferences
        .set_default_target("owner", "project", target_id.into())
        .unwrap();
    assert_eq!(
        preferences
            .projection("owner", "project")
            .unwrap()
            .0
            .default_target_id
            .as_deref(),
        Some(target_id)
    );
    assert!(preferences
        .update_environment(
            "owner",
            "project",
            Default::default(),
            vec![ProjectRunEnvironmentMutation {
                key: "BAD=KEY".into(),
                value: Some("value".into()),
            }],
        )
        .is_err());
    assert!(preferences
        .update_environment(
            "owner",
            "project",
            Default::default(),
            vec![ProjectRunEnvironmentMutation {
                key: "NEW_KEY".into(),
                value: None,
            }],
        )
        .is_err());
}

#[tokio::test]
async fn runtime_projects_typed_process_output_and_owner_scoped_state() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let manager = ProjectRunManager::default();
    let owner = test_scope(&root, "owner-1", "project-1");
    let other = test_scope(&root, "owner-2", "project-1");
    let target = test_target(
        std::env::current_exe().unwrap(),
        vec!["--help".into()],
        &root,
    );

    let mut state = manager.start(&owner, target).await.unwrap();
    assert_eq!(state.instances.len(), 1);
    assert!(manager.snapshot(&other).await.unwrap().instances.is_empty());
    assert!(manager.stop(&other, &state.instances[0].id).await.is_err());

    for _ in 0..20 {
        if !state.is_running {
            break;
        }
        state = manager
            .poll_changes(&owner, state.revision, 1_000)
            .await
            .unwrap()
            .state;
    }

    assert!(!state.is_running);
    assert_eq!(state.instances[0].status, "exited");
    assert_eq!(state.instances[0].exit_code, Some(0));
    assert!(state.instances[0].log.starts_with("$ typed test\n"));
    assert!(state.instances[0].log.contains("进程已退出"));
    let timed_out = manager
        .poll_changes(&owner, state.revision, 10)
        .await
        .unwrap();
    assert!(timed_out.timed_out);
    manager.reset().await;
}

#[tokio::test]
async fn teardown_wakes_a_pending_state_poll_and_resets_its_cursor() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let manager = ProjectRunManager::default();
    let scope = test_scope(&root, "owner", "project");
    let initial = manager.snapshot(&scope).await.unwrap();
    let resetter = manager.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        resetter.reset().await;
    });
    let before = std::time::Instant::now();

    let result = manager
        .poll_changes(&scope, initial.revision, 5_000)
        .await
        .unwrap();

    assert!(!result.timed_out);
    assert!(before.elapsed() < Duration::from_secs(1));
    assert_eq!(result.state.revision, 0);
    assert!(result.state.instances.is_empty());
}

#[cfg(unix)]
#[tokio::test]
async fn stop_terminates_the_owned_project_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let manager = ProjectRunManager::default();
    let scope = test_scope(&root, "owner", "project");
    let target = test_target(
        "/bin/sh".into(),
        vec!["-c".into(), "sleep 60 & wait".into()],
        &root,
    );
    let started = manager.start(&scope, target).await.unwrap();
    let instance_id = started.instances[0].id.clone();
    let before = std::time::Instant::now();

    let stopped = manager.stop(&scope, &instance_id).await.unwrap();

    assert!(before.elapsed() < Duration::from_secs(4));
    assert!(!stopped.is_running);
    assert!(matches!(stopped.instances[0].status, "exited" | "failed"));
    manager.reset().await;
}

#[cfg(unix)]
#[tokio::test]
async fn runtime_redacts_environment_secrets_from_project_output() {
    let directory = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(directory.path()).unwrap();
    let manager = ProjectRunManager::default();
    let scope = test_scope(&root, "owner", "project");
    let mut target = test_target(
        "/bin/sh".into(),
        vec!["-c".into(), "printf '%s' \"$PROJECT_RUN_SECRET\"".into()],
        &root,
    );
    target.environment.insert(
        "PROJECT_RUN_SECRET".into(),
        "secret-spanning-any-output-chunk".into(),
    );
    let mut state = manager.start(&scope, target).await.unwrap();
    for _ in 0..20 {
        if !state.is_running {
            break;
        }
        state = manager
            .poll_changes(&scope, state.revision, 1_000)
            .await
            .unwrap()
            .state;
    }

    assert!(!state.instances[0]
        .log
        .contains("secret-spanning-any-output-chunk"));
    assert!(state.instances[0].log.contains("[已隐藏]"));
    manager.reset().await;
}

fn test_target(
    executable: std::path::PathBuf,
    arguments: Vec<String>,
    cwd: &Path,
) -> ResolvedProjectRunTarget {
    ResolvedProjectRunTarget {
        presentation: ProjectRunTarget {
            id: "target-1".into(),
            label: "Typed test".into(),
            kind: "test".into(),
            language: None,
            cwd: ".".into(),
            command: "typed test".into(),
            source: "test".into(),
            is_default: true,
            entrypoint: None,
            manifest_path: None,
            required_toolchains: Vec::new(),
        },
        executable,
        arguments,
        cwd: cwd.to_path_buf(),
        environment: Default::default(),
        toolchain_directories: Vec::new(),
    }
}
