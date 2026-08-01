// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn binding_key(runtime_session_id: &str, resource_id: &str) -> Result<String, String> {
    let runtime_session_id = validated_identity(runtime_session_id, "runtime_session_id")?;
    let resource_id = validated_identity(resource_id, "resource_id")?;
    Ok(format!("{runtime_session_id}:{resource_id}"))
}

pub(super) fn binding_request_fingerprint(
    request: &CloudStdioCallRequest,
) -> Result<String, String> {
    let payload = CloudStdioBindingFingerprint {
        command: request.command.as_str(),
        args: request.args.as_slice(),
        env: &request.env,
        cwd: request.cwd.as_deref(),
        plugin_artifact: request.plugin_artifact.as_ref(),
        plugin_workspace_write: request.plugin_workspace_write,
    };
    serde_json::to_vec(&payload)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .map_err(|error| format!("serialize cloud stdio MCP binding failed: {error}"))
}

pub(super) fn deterministic_sandbox_id(binding_key: &str, bundle_sha256: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"chatos.cloud-plugin-stdio-sandbox.v1\n");
    hasher.update(binding_key.as_bytes());
    hasher.update(b"\n");
    hasher.update(bundle_sha256.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    uuid::Uuid::from_bytes(bytes).to_string()
}

pub(super) fn validated_identity<'a>(value: &'a str, field: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_IDENTITY_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!("cloud stdio MCP {field} is invalid"));
    }
    Ok(value)
}

pub(super) fn validate_expiry(expires_at_unix: i64) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    if expires_at_unix <= now || expires_at_unix > now.saturating_add(MAX_SESSION_LIFETIME_SECONDS)
    {
        return Err("cloud stdio MCP session expiry is invalid".to_string());
    }
    Ok(())
}

pub(super) fn validate_method(method: &str, params: &Value) -> Result<(), String> {
    if !matches!(method.trim(), "tools/list" | "tools/call") {
        return Err("cloud stdio MCP method is not allowed".to_string());
    }
    if !params.is_object() {
        return Err("cloud stdio MCP params must be an object".to_string());
    }
    if method.trim() == "tools/call"
        && params
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .is_none_or(str::is_empty)
    {
        return Err("cloud stdio MCP tools/call.name is required".to_string());
    }
    Ok(())
}

pub(super) fn validate_launch_command(spec: &CloudStdioLaunchSpec) -> Result<(), String> {
    let Some(identity) = spec.binding_identity.as_deref() else {
        return validate_command(spec.command.as_str(), spec.args.as_slice());
    };
    if identity.len() != 64
        || !identity
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        || spec.args.first().map(String::as_str) != Some(PLUGIN_WRAPPER_MODE)
    {
        return Err("Plugin cloud stdio launch identity is invalid".to_string());
    }
    let expected = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .map_err(|error| format!("resolve sandbox Agent executable failed: {error}"))?;
    let command = Path::new(spec.command.as_str())
        .canonicalize()
        .map_err(|error| format!("resolve Plugin stdio wrapper failed: {error}"))?;
    if command != expected {
        return Err("Plugin cloud stdio wrapper identity changed".to_string());
    }
    Ok(())
}

pub(super) fn validate_command(command: &str, args: &[String]) -> Result<(), String> {
    let command = command.trim();
    if command.is_empty()
        || command.len() > MAX_COMMAND_BYTES
        || command.contains(['/', '\\', '\0'])
        || matches!(command, "." | "..")
    {
        return Err("cloud stdio MCP command must be a PATH-resolved executable name".to_string());
    }
    let shell = command.trim_end_matches(".exe").to_ascii_lowercase();
    let is_shell = matches!(
        shell.as_str(),
        "sh" | "bash" | "dash" | "zsh" | "ksh" | "fish" | "cmd" | "powershell" | "pwsh"
    );
    let invokes_inline_command = args.iter().any(|arg| {
        matches!(
            arg.trim().to_ascii_lowercase().as_str(),
            "-c" | "/c" | "-command" | "-encodedcommand"
        )
    });
    if is_shell && invokes_inline_command {
        return Err("cloud stdio MCP shell inline command execution is forbidden".to_string());
    }
    Ok(())
}

pub(super) fn validate_arguments(args: &[String]) -> Result<(), String> {
    chatos_mcp_runtime::validate_stdio_arguments(args)
        .map_err(|_| "cloud stdio MCP arguments exceed the supported limits".to_string())
}

pub(super) fn validate_environment(env: &BTreeMap<String, String>) -> Result<(), String> {
    chatos_mcp_runtime::validate_stdio_environment(env).map_err(|error| match error {
        chatos_mcp_runtime::StdioPolicyViolation::EnvironmentLimits => {
            "cloud stdio MCP environment exceeds the supported limits".to_string()
        }
        chatos_mcp_runtime::StdioPolicyViolation::EnvironmentEntry
        | chatos_mcp_runtime::StdioPolicyViolation::Arguments => {
            "cloud stdio MCP environment contains an invalid or Host-controlled entry".to_string()
        }
    })
}

pub(super) fn write_launch_spec(prepared: &PreparedBinding) -> Result<(), String> {
    if let Err(error) = std::fs::remove_file(prepared.launch_spec_path.as_path()) {
        if error.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("replace cloud stdio launch spec failed: {error}"));
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(prepared.launch_spec_path.as_path())
        .map_err(|error| format!("write cloud stdio launch spec failed: {error}"))?;
    use std::io::Write;
    let result = file
        .write_all(prepared.launch_spec_bytes.as_slice())
        .map_err(|error| format!("write cloud stdio launch spec failed: {error}"))
        .and_then(|_| {
            file.sync_all()
                .map_err(|error| format!("sync cloud stdio launch spec failed: {error}"))
        });
    if result.is_err() {
        drop(file);
        remove_launch_spec(prepared.launch_spec_path.as_path());
    }
    result
}

pub(super) fn remove_launch_spec(path: &Path) {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %path.display(), error = %error, "remove cloud stdio launch spec failed");
        }
    }
}

pub(super) fn resolve_workspace_cwd(
    workspace: &Path,
    value: Option<&str>,
) -> Result<PathBuf, String> {
    let relative = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(".");
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("cloud stdio MCP cwd must remain relative to the workspace".to_string());
    }
    let resolved = canonical_directory(workspace.join(path).as_path(), "cwd")?;
    if !resolved.starts_with(workspace) {
        return Err("cloud stdio MCP cwd escapes the workspace".to_string());
    }
    Ok(resolved)
}

pub(super) fn canonical_directory(path: &Path, field: &str) -> Result<PathBuf, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("read cloud stdio MCP {field} failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "cloud stdio MCP {field} must be a non-symlink directory"
        ));
    }
    path.canonicalize()
        .map_err(|error| format!("canonicalize cloud stdio MCP {field} failed: {error}"))
}

pub(super) fn validate_launch_paths(spec: &CloudStdioLaunchSpec) -> Result<(), String> {
    let workspace = canonical_directory(spec.workspace.as_path(), "workspace")?;
    let cwd = canonical_directory(spec.cwd.as_path(), "cwd")?;
    let home = canonical_directory(spec.home.as_path(), "HOME")?;
    let temp = canonical_directory(spec.temp.as_path(), "temp")?;
    if !cwd.starts_with(workspace.as_path())
        || !home.starts_with(temp.parent().unwrap_or(home.as_path()))
        || !temp.starts_with(home.parent().unwrap_or(temp.as_path()))
    {
        return Err("cloud stdio MCP launch paths do not match the sandbox binding".to_string());
    }
    Ok(())
}
