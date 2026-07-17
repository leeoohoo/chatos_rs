// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use chatos_sandbox_contract::{
    merge_codex_permission_profile_document_layers, parse_codex_permission_profile_toml,
    CodexPermissionProfileDocument, FileSystemPath,
};

use crate::workspace::trust::workspace_project_config_trust_is_current;
use crate::WorkspaceState;

const MAX_PROJECT_PERMISSION_CONFIG_BYTES: u64 = 1024 * 1024;

pub(crate) fn load_trusted_project_permission_document(
    workspace: &WorkspaceState,
    project_cwd: &Path,
) -> Result<Option<CodexPermissionProfileDocument>> {
    if workspace.project_config_trust.is_none() {
        return Ok(None);
    }
    if !workspace_project_config_trust_is_current(workspace) {
        return Err(anyhow!(
            "workspace project configuration trust is stale because the directory identity changed"
        ));
    }
    let root = workspace.absolute_root.canonicalize().with_context(|| {
        format!(
            "canonicalize workspace {}",
            workspace.absolute_root.display()
        )
    })?;
    let project_cwd = project_cwd
        .canonicalize()
        .with_context(|| format!("canonicalize project cwd {}", project_cwd.display()))?;
    if !project_cwd.is_dir() || !project_cwd.starts_with(root.as_path()) {
        return Err(anyhow!("project cwd escapes the trusted workspace"));
    }
    let mut merged = load_project_permission_layer(root.as_path(), root.as_path())?;
    let mut scope = root.clone();
    let relative_cwd = project_cwd
        .strip_prefix(root.as_path())
        .map_err(|_| anyhow!("project cwd escapes the trusted workspace"))?;
    for component in relative_cwd.components() {
        if let std::path::Component::Normal(segment) = component {
            scope.push(segment);
            merged = merge_optional_project_layer(
                merged,
                load_project_permission_layer(root.as_path(), scope.as_path())?,
            );
        }
    }
    Ok(merged)
}

fn load_project_permission_layer(
    workspace_root: &Path,
    scope: &Path,
) -> Result<Option<CodexPermissionProfileDocument>> {
    let config_directory = scope.join(".chatos");
    let config_directory_metadata = match fs::symlink_metadata(&config_directory) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "inspect project configuration directory {}",
                    config_directory.display()
                )
            })
        }
    };
    if config_directory_metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "project configuration directory must not be a symlink: {}",
            config_directory.display()
        ));
    }
    if !config_directory_metadata.is_dir() {
        return Err(anyhow!(
            "project configuration path is not a directory: {}",
            config_directory.display()
        ));
    }

    let config_path = config_directory.join("config.toml");
    let metadata = match fs::symlink_metadata(&config_path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!("inspect project configuration {}", config_path.display())
            })
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "project configuration file must not be a symlink: {}",
            config_path.display()
        ));
    }
    if !metadata.is_file() {
        return Err(anyhow!(
            "project configuration is not a regular file: {}",
            config_path.display()
        ));
    }
    if metadata.len() > MAX_PROJECT_PERMISSION_CONFIG_BYTES {
        return Err(anyhow!(
            "project configuration {} exceeds the 1 MiB limit",
            config_path.display()
        ));
    }
    let canonical_config = config_path.canonicalize().with_context(|| {
        format!(
            "canonicalize project configuration {}",
            config_path.display()
        )
    })?;
    if !canonical_config.starts_with(workspace_root) {
        return Err(anyhow!(
            "project configuration escapes the trusted workspace"
        ));
    }
    let source = fs::read_to_string(&canonical_config)
        .with_context(|| format!("read project configuration {}", canonical_config.display()))?;
    if source.len() as u64 > MAX_PROJECT_PERMISSION_CONFIG_BYTES {
        return Err(anyhow!(
            "project configuration {} exceeds the 1 MiB limit",
            canonical_config.display()
        ));
    }
    let mut document = parse_codex_permission_profile_toml(source.as_str())
        .map_err(anyhow::Error::msg)
        .with_context(|| {
            format!(
                "parse trusted project configuration {}",
                canonical_config.display()
            )
        })?;
    let scope_relative = scope
        .strip_prefix(workspace_root)
        .map_err(|_| anyhow!("project configuration scope escapes the trusted workspace"))?;
    rebase_project_document(&mut document, scope_relative)?;
    Ok(Some(document))
}

fn merge_optional_project_layer(
    lower: Option<CodexPermissionProfileDocument>,
    higher: Option<CodexPermissionProfileDocument>,
) -> Option<CodexPermissionProfileDocument> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => Some(merge_codex_permission_profile_document_layers(
            lower, higher,
        )),
        (Some(document), None) | (None, Some(document)) => Some(document),
        (None, None) => None,
    }
}

fn rebase_project_document(
    document: &mut CodexPermissionProfileDocument,
    scope_relative: &Path,
) -> Result<()> {
    if scope_relative.as_os_str().is_empty() {
        return Ok(());
    }
    for profile in document.configuration.profiles.values_mut() {
        let mut workspace_roots = std::collections::BTreeMap::new();
        for (path, enabled) in std::mem::take(&mut profile.workspace_roots) {
            workspace_roots.insert(
                rebase_relative_string(scope_relative, path.as_str())?,
                enabled,
            );
        }
        profile.workspace_roots = workspace_roots;
        if let Some(file_system) = profile.file_system.as_mut() {
            for entry in file_system.entries.get_or_insert_with(Vec::new) {
                match &mut entry.path {
                    FileSystemPath::Path { path } if !Path::new(path.as_str()).is_absolute() => {
                        *path = rebase_relative_string(scope_relative, path.as_str())?;
                    }
                    FileSystemPath::GlobPattern { pattern }
                        if !Path::new(pattern.as_str()).is_absolute() =>
                    {
                        *pattern = rebase_relative_string(scope_relative, pattern.as_str())?;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn rebase_relative_string(scope_relative: &Path, value: &str) -> Result<String> {
    let value_path = Path::new(value);
    if value_path.is_absolute() {
        return Ok(value.to_string());
    }
    let mut combined = scope_relative.to_path_buf();
    for component in value_path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(segment) => combined.push(segment),
            _ => {
                return Err(anyhow!(
                    "project configuration relative path contains unsafe traversal: {value:?}"
                ))
            }
        }
    }
    let rebased = combined.to_string_lossy().replace('\\', "/");
    Ok(if rebased.is_empty() {
        ".".to_string()
    } else {
        rebased
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::trust::workspace_project_config_trust_fingerprint;
    use crate::WorkspaceProjectConfigTrust;

    fn workspace(root: &Path, trusted: bool) -> WorkspaceState {
        WorkspaceState {
            id: "workspace-test".to_string(),
            absolute_root: root.canonicalize().expect("canonical root"),
            alias: "test".to_string(),
            fingerprint: "path-fingerprint".to_string(),
            project_config_trust: trusted.then(|| WorkspaceProjectConfigTrust {
                identity_fingerprint: workspace_project_config_trust_fingerprint(root)
                    .expect("trust identity"),
                trusted_at: "2026-07-15T00:00:00Z".to_string(),
            }),
        }
    }

    #[test]
    fn untrusted_workspace_ignores_project_configuration() {
        let root = std::env::temp_dir().join(format!(
            "chatos-untrusted-project-config-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join(".chatos")).expect("create config directory");
        fs::write(
            root.join(".chatos/config.toml"),
            "project_config_trusted = true\ndefault_permissions = \":danger-full-access\"",
        )
        .expect("write malicious config");

        let document = load_trusted_project_permission_document(&workspace(&root, false), &root)
            .expect("ignore untrusted config");

        assert!(document.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn trusted_workspace_loads_chatos_project_configuration() {
        let root = std::env::temp_dir().join(format!(
            "chatos-trusted-project-config-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(root.join(".chatos")).expect("create config directory");
        fs::write(
            root.join(".chatos/config.toml"),
            "default_permissions = \":read-only\"",
        )
        .expect("write config");

        let document = load_trusted_project_permission_document(&workspace(&root, true), &root)
            .expect("load trusted config")
            .expect("project document");

        assert_eq!(document.default_permissions.as_deref(), Some(":read-only"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn nested_project_configuration_closest_to_cwd_wins_and_rebases_paths() {
        let root = std::env::temp_dir().join(format!(
            "chatos-nested-project-config-{}",
            uuid::Uuid::new_v4()
        ));
        let nested = root.join("apps/web");
        fs::create_dir_all(root.join(".chatos")).expect("create root config directory");
        fs::create_dir_all(nested.join(".chatos")).expect("create nested config directory");
        fs::write(
            root.join(".chatos/config.toml"),
            "default_permissions = \":read-only\"",
        )
        .expect("write root config");
        fs::write(
            nested.join(".chatos/config.toml"),
            r#"
default_permissions = "nested-review"

[permissions.nested-review]
extends = ":read-only"

[permissions.nested-review.workspace_roots]
"." = true

[permissions.nested-review.filesystem]
"secrets" = "read"
"**/*.env" = "deny"
"#,
        )
        .expect("write nested config");

        let document = load_trusted_project_permission_document(&workspace(&root, true), &nested)
            .expect("load nested config")
            .expect("merged project config");

        assert_eq!(
            document.default_permissions.as_deref(),
            Some("nested-review")
        );
        let profile = document
            .configuration
            .profiles
            .get("nested-review")
            .expect("nested profile");
        assert_eq!(
            profile.workspace_roots,
            std::collections::BTreeMap::from([("apps/web".to_string(), true)])
        );
        let entries = profile
            .file_system
            .as_ref()
            .expect("filesystem")
            .normalized_entries();
        assert!(entries.iter().any(|entry| {
            entry.path
                == FileSystemPath::Path {
                    path: "apps/web/secrets".to_string(),
                }
        }));
        assert!(entries.iter().any(|entry| {
            entry.path
                == FileSystemPath::GlobPattern {
                    pattern: "apps/web/**/*.env".to_string(),
                }
        }));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn replaced_workspace_directory_invalidates_project_config_trust() {
        let root = std::env::temp_dir().join(format!(
            "chatos-stale-project-config-trust-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create root");
        let trusted = workspace(&root, true);
        fs::remove_dir(&root).expect("remove trusted root");
        fs::create_dir(&root).expect("replace root");

        let error = load_trusted_project_permission_document(&trusted, &root)
            .expect_err("replaced root must invalidate trust");

        assert!(error.to_string().contains("trust is stale"));
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_project_configuration_directory_fails_closed() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "chatos-project-config-symlink-{}",
            uuid::Uuid::new_v4()
        ));
        let outside = std::env::temp_dir().join(format!(
            "chatos-project-config-outside-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create root");
        fs::create_dir_all(&outside).expect("create outside");
        fs::write(
            outside.join("config.toml"),
            "default_permissions = \":danger-full-access\"",
        )
        .expect("write outside config");
        let trusted = workspace(&root, true);
        symlink(&outside, root.join(".chatos")).expect("symlink config directory");

        let error = load_trusted_project_permission_document(&trusted, &root)
            .expect_err("symlinked config directory must fail");

        assert!(error.to_string().contains("must not be a symlink"));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
