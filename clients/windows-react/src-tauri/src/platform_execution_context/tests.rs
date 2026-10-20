use super::{authorization::TaskToolAuthorization, filesystem::*};
use crate::workspace_index::PROJECT_REGISTRY_SCHEMA;
use rusqlite::Connection;
use serde_json::json;
use std::fs;

#[test]
fn authorization_requires_the_macos_terminal_selection_shape() {
    let input = json!({
        "owned_paths": [],
        "tool_options": {
            "requires_execution": true,
            "enabled_builtin_kinds": ["TerminalController"],
            "plugin_hints": [],
            "plugin_release_snapshots": [],
            "external_mcp_config_ids": []
        }
    });
    let authorization = TaskToolAuthorization::resolve(input.as_object().unwrap()).unwrap();
    assert!(authorization.allows_terminal());

    let mut disabled = input.clone();
    disabled["tool_options"]["requires_execution"] = json!(false);
    assert!(
        !TaskToolAuthorization::resolve(disabled.as_object().unwrap())
            .unwrap()
            .allows_terminal()
    );
    let mut malformed = input;
    malformed["tool_options"]["plugin_hints"] = json!([{
        "plugin_key": "plugin-a", "component_key": "tool-a"
    }]);
    assert!(TaskToolAuthorization::resolve(malformed.as_object().unwrap()).is_err());

    for path in [
        "../outside",
        "/absolute",
        "src//file",
        "src\\file",
        "C:/repo",
        "src/./file",
        "~/.secret",
    ] {
        let invalid = json!({
            "owned_paths": [path],
            "tool_options": {
                "requires_execution": true,
                "enabled_builtin_kinds": [],
                "plugin_hints": [],
                "plugin_release_snapshots": []
            }
        });
        assert!(TaskToolAuthorization::resolve(invalid.as_object().unwrap()).is_err());
    }
}

#[test]
fn project_root_is_bound_to_owner_registry_and_connector_grant() {
    let directory = tempfile::tempdir().unwrap();
    let connector = directory.path();
    let workspace = connector.join("workspace");
    let project = workspace.join("project");
    fs::create_dir_all(&project).unwrap();
    write_registry(connector, "alice", "project-1", "workspace-1", "project");
    write_state(connector, "alice", "workspace-1", &workspace);

    let resolved = resolve_project_root(connector, "alice", "project-1").unwrap();
    assert_eq!(resolved.root, fs::canonicalize(project).unwrap());
    assert_eq!(resolved.workspace_scope_id, "workspace-1");
    assert!(resolve_project_root(connector, "bob", "project-1").is_err());
}

#[cfg(unix)]
#[test]
fn project_root_rejects_a_symlink_that_escapes_the_granted_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let connector = directory.path();
    let workspace = connector.join("workspace");
    let outside = connector.join("outside");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, workspace.join("project")).unwrap();
    write_registry(connector, "alice", "project-1", "workspace-1", "project");
    write_state(connector, "alice", "workspace-1", &workspace);
    assert!(resolve_project_root(connector, "alice", "project-1").is_err());
}

#[test]
fn execution_directory_rejects_traversal_files_and_external_symlinks() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("project");
    let child = root.join("child");
    let outside = directory.path().join("outside");
    fs::create_dir_all(&child).unwrap();
    fs::create_dir_all(&outside).unwrap();
    fs::write(root.join("file.txt"), "file").unwrap();
    assert_eq!(
        resolve_execution_directory(&root, "child").unwrap(),
        fs::canonicalize(&child).unwrap()
    );
    assert!(resolve_execution_directory(&root, "../outside").is_err());
    assert!(resolve_execution_directory(&root, "file.txt").is_err());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, root.join("escape")).unwrap();
        assert!(resolve_execution_directory(&root, "escape").is_err());
    }
}

#[test]
fn contact_root_is_owner_scoped_and_deterministic() {
    let directory = tempfile::tempdir().unwrap();
    write_state(directory.path(), "alice", "workspace-1", directory.path());
    let first = resolve_contact_root(directory.path(), "alice", "conversation-1").unwrap();
    let second = resolve_contact_root(directory.path(), "alice", "conversation-1").unwrap();
    assert_eq!(first.root, second.root);
    assert!(first
        .root
        .starts_with(fs::canonicalize(directory.path()).unwrap()));
    assert!(first.workspace_scope_id.starts_with("contact:"));
    assert!(resolve_contact_root(directory.path(), "bob", "conversation-1").is_err());
}

#[cfg(unix)]
#[test]
fn contact_root_rejects_a_symlink_that_escapes_connector_storage() {
    let directory = tempfile::tempdir().unwrap();
    let connector = directory.path().join("connector");
    let outside = directory.path().join("outside");
    fs::create_dir_all(&connector).unwrap();
    fs::create_dir_all(&outside).unwrap();
    write_state(&connector, "alice", "workspace-1", &outside);
    std::os::unix::fs::symlink(&outside, connector.join("LocalAgent")).unwrap();

    assert!(resolve_contact_root(&connector, "alice", "conversation-1").is_err());
}

fn write_registry(
    connector: &std::path::Path,
    owner: &str,
    project: &str,
    workspace: &str,
    relative_root: &str,
) {
    let connection = Connection::open(connector.join("Projects.sqlite3")).unwrap();
    connection.execute_batch(PROJECT_REGISTRY_SCHEMA).unwrap();
    connection
        .execute(
            "INSERT INTO local_project_records VALUES (?1, ?2, 'Project', '', ?3, ?4, \
         'software_development', 1, 'active', 1, 1)",
            rusqlite::params![owner, project, workspace, relative_root],
        )
        .unwrap();
}

fn write_state(
    connector: &std::path::Path,
    owner: &str,
    workspace: &str,
    absolute_root: &std::path::Path,
) {
    fs::write(
        connector.join("state.json"),
        serde_json::to_vec(&json!({
            "user": {"id": owner},
            "workspaces": [{
                "id": workspace,
                "absoluteRoot": absolute_root,
                "fingerprint": "fingerprint-1"
            }]
        }))
        .unwrap(),
    )
    .unwrap();
}
