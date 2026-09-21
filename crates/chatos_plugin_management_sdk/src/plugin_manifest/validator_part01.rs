fn validate_plugin_name(value: &str, issues: &mut Vec<PluginManifestValidationIssue>) {
    let valid = !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        issue(
            issues,
            "name",
            "name must be 1-64 characters of lower-case kebab-case",
        );
    }
}

fn validate_component_key(
    field: String,
    value: &str,
    keys: &mut HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let valid = !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if !valid {
        issue(
            issues,
            field.as_str(),
            "component key contains unsupported characters",
        );
    } else if !keys.insert(value.to_string()) {
        issue(issues, field.as_str(), "duplicate component key");
    }
}

fn validate_path(
    field: String,
    value: &PluginPathRef,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    match normalize_plugin_relative_path(value.path.as_str()) {
        Ok(normalized) if normalized == value.path => {}
        Ok(_) => issue(issues, field.as_str(), "path is not normalized"),
        Err(message) => issue(issues, field.as_str(), message),
    }
}

fn validate_interface_assets(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (field, path) in [
        (
            "interface.composerIcon",
            manifest.interface.composer_icon.as_ref(),
        ),
        ("interface.logo", manifest.interface.logo.as_ref()),
        ("interface.logoDark", manifest.interface.logo_dark.as_ref()),
    ] {
        if let Some(path) = path {
            validate_path(field.to_string(), path, issues);
            if !path.path.starts_with("./assets/") {
                issue(issues, field, "asset must be stored under ./assets/");
            }
        }
    }
    for (index, path) in manifest.interface.screenshots.iter().enumerate() {
        let field = format!("interface.screenshots[{index}]");
        validate_path(field.clone(), path, issues);
        if !path.path.starts_with("./assets/") || !path.path.to_ascii_lowercase().ends_with(".png")
        {
            issue(
                issues,
                field.as_str(),
                "screenshot must be a PNG under ./assets/",
            );
        }
    }
}

fn validate_dependencies(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    if let Some(version) = manifest.dependencies.minimum_host_version.as_deref() {
        if VersionReq::parse(version).is_err() && Version::parse(version).is_err() {
            issue(
                issues,
                "dependencies.minimumHostVersion",
                "minimum host version must be semver or a semver requirement",
            );
        }
    }
    for (index, dependency) in manifest.dependencies.plugins.iter().enumerate() {
        required_text(
            issues,
            format!("dependencies.plugins[{index}].pluginId").as_str(),
            dependency.plugin_id.as_str(),
        );
        if let Some(requirement) = dependency.version_requirement.as_deref() {
            if VersionReq::parse(requirement).is_err() {
                issue(
                    issues,
                    format!("dependencies.plugins[{index}].versionRequirement").as_str(),
                    "version requirement must use semver syntax",
                );
            }
        }
    }
}

fn validate_permissions(
    manifest: &PluginManifest,
    component_keys: &HashSet<String>,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    let mut permissions = HashSet::new();
    for (index, requirement) in manifest.permissions.iter().enumerate() {
        let field = format!("permissions[{index}].permission");
        let permission = requirement.permission.as_str();
        let valid = !permission.is_empty()
            && permission.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b':' | b'-' | b'_' | b'*')
            });
        if !valid {
            issue(
                issues,
                field.as_str(),
                "permission must use a lower-case capability identifier",
            );
        } else if !permissions.insert(permission.to_string()) {
            issue(issues, field.as_str(), "duplicate permission declaration");
        }
        for component in &requirement.components {
            if !component_keys.contains(component) {
                issue(
                    issues,
                    format!("permissions[{index}].components").as_str(),
                    format!("unknown component key {component}"),
                );
            }
        }
    }
}

fn validate_mcp_runtime_permissions(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, server) in manifest.mcp_servers.iter().enumerate() {
        let PluginMcpServer::Stdio { component_key, .. } = server else {
            continue;
        };
        let declares_process_spawn = manifest.permissions.iter().any(|permission| {
            permission.permission == "process.spawn"
                && permission.required
                && (permission.components.is_empty()
                    || permission.components.iter().any(|key| key == component_key))
        });
        if !declares_process_spawn {
            issue(
                issues,
                format!("mcpServers[{index}]").as_str(),
                "stdio MCP component requires a required process.spawn permission",
            );
        }
    }
}

fn validate_ui_runtime_permissions(
    manifest: &PluginManifest,
    issues: &mut Vec<PluginManifestValidationIssue>,
) {
    for (index, ui) in manifest.ui.iter().enumerate() {
        if ui.runtime.is_none() {
            continue;
        }
        let declares_process_spawn = manifest.permissions.iter().any(|permission| {
            permission.permission == "process.spawn"
                && permission.required
                && (permission.components.is_empty()
                    || permission
                        .components
                        .iter()
                        .any(|key| key == &ui.component_key))
        });
        if !declares_process_spawn {
            issue(
                issues,
                format!("ui[{index}].runtime").as_str(),
                "local Plugin UI runtime requires a required process.spawn permission",
            );
        }
    }
}
