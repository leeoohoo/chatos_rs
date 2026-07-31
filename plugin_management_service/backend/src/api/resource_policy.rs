// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn required_text(value: Option<&str>, field: &str) -> Result<String, ApiError> {
    normalized(value).ok_or_else(|| ApiError::bad_request(format!("{field} is required")))
}

pub(super) fn redact_mcp_runtime_secrets(record: &mut McpRecord) {
    record.runtime.headers.clear();
    record.runtime.env.clear();
    record.runtime.args.clear();
    if let Some(url) = record.runtime.url.as_deref() {
        if let Ok(mut url) = reqwest::Url::parse(url) {
            url.set_query(None);
            record.runtime.url = Some(url.to_string());
        }
    }
}

pub(super) fn redact_mcp_runtime_secrets_for_user(record: &mut McpRecord, user: &CurrentUser) {
    if !user.is_super_admin() && record.owner_user_id != user.effective_owner_user_id() {
        redact_mcp_runtime_secrets(record);
    }
}

pub(super) fn normalize_visibility(
    value: Option<&str>,
    user: &CurrentUser,
) -> Result<String, ApiError> {
    let visibility = normalized(value).unwrap_or_else(|| VISIBILITY_PRIVATE.to_string());
    match visibility.as_str() {
        VISIBILITY_PRIVATE => Ok(visibility),
        VISIBILITY_PUBLIC | VISIBILITY_SYSTEM_PRIVATE if user.is_super_admin() => Ok(visibility),
        VISIBILITY_PUBLIC | VISIBILITY_SYSTEM_PRIVATE => Err(ApiError::forbidden(
            "only super_admin can create public or system-private resources",
        )),
        _ => Err(ApiError::bad_request(
            "visibility must be private, public, or system_private",
        )),
    }
}

pub(super) fn requested_owner_user_id(
    value: Option<&str>,
    user: &CurrentUser,
) -> Result<String, ApiError> {
    let requested = normalized(value).unwrap_or_else(|| user.effective_owner_user_id().to_string());
    if user.is_super_admin() || requested == user.effective_owner_user_id() {
        Ok(requested)
    } else {
        Err(ApiError::forbidden(
            "cannot write resources for another user",
        ))
    }
}

pub(super) fn owner_kind_for(visibility: &str, user: &CurrentUser) -> String {
    if visibility == VISIBILITY_SYSTEM_PRIVATE {
        OWNER_KIND_SYSTEM.to_string()
    } else if user.is_super_admin() {
        OWNER_KIND_ADMIN.to_string()
    } else {
        OWNER_KIND_USER.to_string()
    }
}

pub(super) fn default_source_kind(value: Option<String>, user: &CurrentUser) -> String {
    if user.is_super_admin() {
        value.unwrap_or_else(|| SOURCE_KIND_ADMIN_CREATED.to_string())
    } else {
        SOURCE_KIND_USER_CREATED.to_string()
    }
}

pub(super) fn ensure_super_admin(user: &CurrentUser) -> Result<(), ApiError> {
    if user.is_super_admin() {
        Ok(())
    } else {
        Err(ApiError::forbidden("super_admin permission required"))
    }
}

pub(super) fn ensure_can_read_resource(
    user: &CurrentUser,
    owner_user_id: &str,
    visibility: &str,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || visibility == VISIBILITY_PUBLIC
        || (visibility == VISIBILITY_PRIVATE && owner_user_id == user.effective_owner_user_id())
    {
        Ok(())
    } else {
        Err(ApiError::not_found("resource not found"))
    }
}

pub(super) fn ensure_can_update_resource(
    user: &CurrentUser,
    owner_user_id: &str,
    visibility: &str,
) -> Result<(), ApiError> {
    if user.is_super_admin()
        || (visibility == VISIBILITY_PRIVATE && owner_user_id == user.effective_owner_user_id())
    {
        Ok(())
    } else {
        Err(ApiError::forbidden("resource is not writable"))
    }
}

pub(super) fn validate_client_managed_mcp_payload(
    payload: &McpPayload,
    user: &CurrentUser,
) -> Result<(), ApiError> {
    if matches!(
        normalized(payload.source_kind.as_deref()).as_deref(),
        Some(SOURCE_KIND_SYSTEM_SEED)
    ) {
        return Err(ApiError::bad_request(
            "system seed MCPs are managed by the service",
        ));
    }
    if matches!(
        payload
            .runtime
            .as_ref()
            .map(|runtime| runtime.kind.as_str()),
        Some(RUNTIME_KIND_SYSTEM | RUNTIME_KIND_BUILTIN)
    ) {
        return Err(ApiError::bad_request(
            "system MCPs are managed by the service",
        ));
    }
    if let Some(runtime) = payload.runtime.as_ref() {
        validate_client_managed_mcp_runtime(runtime, user)?;
    }
    Ok(())
}

pub(super) fn validate_client_managed_mcp_runtime(
    runtime: &McpRuntime,
    user: &CurrentUser,
) -> Result<(), ApiError> {
    if !user.is_super_admin()
        && !matches!(
            runtime.kind.as_str(),
            RUNTIME_KIND_LOCAL_CONNECTOR_STDIO | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
        )
    {
        return Err(ApiError::forbidden(
            "user-created MCPs must run through Local Connector",
        ));
    }
    Ok(())
}

pub(super) fn validate_system_seed_mcp_update(payload: &McpPayload) -> Result<(), ApiError> {
    let modifies_managed_fields = payload.owner_user_id.is_some()
        || payload.visibility.is_some()
        || payload.source_kind.is_some()
        || payload.name.is_some()
        || payload.display_name.is_some()
        || payload.description.is_some()
        || payload.runtime.is_some()
        || payload.security.is_some()
        || payload.metadata.is_some();
    if modifies_managed_fields {
        Err(ApiError::bad_request(
            "system seed MCPs only allow updating enabled",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_release_managed_mcp_update(
    ownership: &PluginComponentOwnership,
    payload: &McpPayload,
) -> Result<(), ApiError> {
    if !ownership.is_release_managed() {
        return Ok(());
    }
    let modifies_release_fields = payload.owner_user_id.is_some()
        || payload.visibility.is_some()
        || payload.source_kind.is_some()
        || payload.name.is_some()
        || payload.runtime.is_some()
        || payload.security.is_some();
    if modifies_release_fields {
        Err(ApiError::conflict(
            "Plugin Release MCP identity, runtime, and security are immutable",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_release_managed_agent_update(
    ownership: &PluginComponentOwnership,
    payload: &SystemAgentPayload,
) -> Result<(), ApiError> {
    if !ownership.is_release_managed() {
        return Ok(());
    }
    if payload.agent_key.is_some() || payload.service_name.is_some() || payload.managed_by.is_some()
    {
        Err(ApiError::conflict(
            "Plugin Release Agent identity and runtime ownership are immutable",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn ensure_release_managed_resource_not_deleted(
    ownership: &PluginComponentOwnership,
) -> Result<(), ApiError> {
    if ownership.is_release_managed() {
        Err(ApiError::conflict(
            "Plugin Release components must be removed through Plugin uninstall or release lifecycle",
        ))
    } else {
        Ok(())
    }
}

pub(super) fn validate_mcp_runtime(runtime: &McpRuntime) -> Result<(), ApiError> {
    match runtime.kind.as_str() {
        RUNTIME_KIND_SYSTEM => {
            let system_key = runtime
                .system_key
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .ok_or_else(|| ApiError::bad_request("system MCP requires system_key"))?;
            if chatos_mcp::system_mcp_descriptor_by_any(system_key.as_str()).is_none() {
                return Err(ApiError::bad_request(format!(
                    "unknown system MCP key: {system_key}"
                )));
            }
        }
        RUNTIME_KIND_BUILTIN => {
            return Err(ApiError::bad_request(
                "legacy system MCP runtime kinds are read-only; use system",
            ));
        }
        RUNTIME_KIND_HTTP => {
            let url = runtime
                .url
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .ok_or_else(|| ApiError::bad_request("HTTP MCP requires url"))?;
            let url = reqwest::Url::parse(url.as_str())
                .map_err(|_| ApiError::bad_request("HTTP MCP url is invalid"))?;
            if url.scheme() != "https"
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
            {
                return Err(ApiError::bad_request(
                    "HTTP MCP url must use HTTPS without credentials or fragments",
                ));
            }
            validate_external_http_headers(&runtime.headers)?;
        }
        RUNTIME_KIND_STDIO_CLOUD => {
            let command = runtime
                .command
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .ok_or_else(|| ApiError::bad_request("stdio MCP requires command"))?;
            validate_cloud_stdio_command(command.as_str(), runtime.args.as_slice())?;
            validate_cloud_stdio_arguments(runtime.args.as_slice())?;
            validate_cloud_stdio_environment(&runtime.env)?;
            validate_cloud_stdio_cwd(runtime.cwd.as_deref())?;
        }
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
        | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
        | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY => validate_local_connector_ref(runtime)?,
        _ => {
            return Err(ApiError::bad_request(
                "runtime.kind must be system, http, stdio_cloud, local_connector_stdio, local_connector_http, or local_connector_builtin_proxy",
            ));
        }
    }
    Ok(())
}

fn validate_cloud_stdio_command(command: &str, args: &[String]) -> Result<(), ApiError> {
    if command.len() > 256
        || command
            .chars()
            .any(|character| matches!(character, '/' | '\\' | '\0'))
        || matches!(command, "." | "..")
    {
        return Err(ApiError::bad_request(
            "stdio MCP command must be a PATH-resolved executable name",
        ));
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
        return Err(ApiError::bad_request(
            "stdio MCP shell inline command execution is forbidden",
        ));
    }
    Ok(())
}

fn validate_cloud_stdio_arguments(args: &[String]) -> Result<(), ApiError> {
    if args.len() > 256
        || args
            .iter()
            .any(|arg| arg.len() > 16 * 1024 || arg.contains('\0'))
        || args.iter().map(String::len).sum::<usize>() > 128 * 1024
    {
        return Err(ApiError::bad_request(
            "stdio MCP arguments exceed the supported limits",
        ));
    }
    Ok(())
}

fn validate_cloud_stdio_environment(
    env: &std::collections::BTreeMap<String, String>,
) -> Result<(), ApiError> {
    if env.len() > 128
        || env
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > 64 * 1024
    {
        return Err(ApiError::bad_request(
            "stdio MCP environment exceeds the supported limits",
        ));
    }
    for (name, value) in env {
        let valid = !name.is_empty()
            && name.len() <= 128
            && name
                .bytes()
                .next()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
        let normalized = name.to_ascii_uppercase();
        let controlled = matches!(
            normalized.as_str(),
            "PATH"
                | "HOME"
                | "SHELL"
                | "TMPDIR"
                | "TMP"
                | "TEMP"
                | "COMSPEC"
                | "PATHEXT"
                | "SYSTEMROOT"
                | "WINDIR"
                | "USERPROFILE"
                | "APPDATA"
                | "LOCALAPPDATA"
                | "CHATOS_WORKSPACE"
                | "CHATOS_SANDBOX_MCP_TOKEN"
                | "CHATOS_AGENT_TOKEN"
                | "NODE_OPTIONS"
                | "PYTHONHOME"
                | "PYTHONPATH"
                | "RUBYOPT"
                | "PERL5OPT"
                | "BASH_ENV"
                | "ENV"
                | "PROMPT_COMMAND"
        ) || normalized.starts_with("LD_")
            || normalized.starts_with("DYLD_")
            || normalized.starts_with("XDG_")
            || normalized.starts_with("MCP_MANAGEMENT_")
            || normalized.starts_with("SANDBOX_MANAGER_");
        if !valid || controlled || value.contains('\0') {
            return Err(ApiError::bad_request(
                "stdio MCP environment contains an invalid or Host-controlled entry",
            ));
        }
    }
    Ok(())
}

fn validate_cloud_stdio_cwd(cwd: Option<&str>) -> Result<(), ApiError> {
    let Some(cwd) = cwd.and_then(|value| normalized(Some(value))) else {
        return Ok(());
    };
    let path = std::path::Path::new(cwd.as_str());
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ApiError::bad_request(
            "stdio MCP cwd must remain relative to the sandbox workspace",
        ));
    }
    Ok(())
}

pub(super) fn validate_mcp_security(
    runtime: &McpRuntime,
    security: &ResourceSecurity,
) -> Result<(), ApiError> {
    for (field, values) in [
        ("allowed_tool_names", security.allowed_tool_names.as_slice()),
        ("blocked_tool_names", security.blocked_tool_names.as_slice()),
    ] {
        if values.len() > 512
            || values
                .iter()
                .any(|value| value.trim().is_empty() || value.trim().len() > 256)
        {
            return Err(ApiError::bad_request(format!(
                "MCP security {field} contains an invalid tool policy"
            )));
        }
    }
    if matches!(
        runtime.kind.as_str(),
        RUNTIME_KIND_HTTP | RUNTIME_KIND_STDIO_CLOUD
    ) && !security.allow_writes.unwrap_or(false)
        && security.allowed_tool_names.is_empty()
    {
        return Err(ApiError::bad_request(
            "read-only remote MCP requires allowed_tool_names",
        ));
    }
    Ok(())
}

fn validate_external_http_headers(
    headers: &std::collections::BTreeMap<String, String>,
) -> Result<(), ApiError> {
    if headers.len() > 64
        || headers
            .iter()
            .map(|(name, value)| name.len().saturating_add(value.len()))
            .sum::<usize>()
            > 32 * 1024
    {
        return Err(ApiError::bad_request(
            "HTTP MCP headers exceed the supported limits",
        ));
    }
    for (name, value) in headers {
        let name = reqwest::header::HeaderName::from_bytes(name.trim().as_bytes())
            .map_err(|_| ApiError::bad_request("HTTP MCP headers contain an invalid name"))?;
        reqwest::header::HeaderValue::from_str(value)
            .map_err(|_| ApiError::bad_request("HTTP MCP headers contain an invalid value"))?;
        if matches!(
            name.as_str(),
            "accept"
                | "connection"
                | "content-length"
                | "content-type"
                | "host"
                | "proxy-authenticate"
                | "proxy-authorization"
                | "te"
                | "trailer"
                | "transfer-encoding"
                | "upgrade"
                | "x-local-connector-internal-scope"
                | "x-local-connector-internal-secret"
                | "x-local-connector-internal-token"
                | "x-project-service-internal-scope"
                | "x-project-service-internal-token"
                | "x-project-service-sync-secret"
                | "x-sandbox-client-key"
                | "x-sandbox-internal-scope"
                | "x-sandbox-internal-token"
        ) {
            return Err(ApiError::bad_request(format!(
                "HTTP MCP header {} is managed or unsafe",
                name.as_str()
            )));
        }
    }
    Ok(())
}

pub(super) fn validate_local_connector_ref(runtime: &McpRuntime) -> Result<(), ApiError> {
    let local = runtime
        .local_connector
        .as_ref()
        .ok_or_else(|| ApiError::bad_request("local connector runtime requires local_connector"))?;
    for (value, field) in [
        (local.device_id.as_deref(), "device_id"),
        (local.manifest_id.as_deref(), "manifest_id"),
    ] {
        if value.and_then(|value| normalized(Some(value))).is_none() {
            return Err(ApiError::bad_request(format!(
                "local connector runtime requires {field}"
            )));
        }
    }
    if runtime.kind == RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
        && local
            .workspace_id
            .as_deref()
            .and_then(|value| normalized(Some(value)))
            .is_none()
    {
        return Err(ApiError::bad_request(
            "local connector builtin proxy requires workspace_id",
        ));
    }
    if !local.requires_online {
        return Err(ApiError::bad_request(
            "local connector runtime requires requires_online=true",
        ));
    }
    if runtime.command.is_some()
        || !runtime.args.is_empty()
        || !runtime.env.is_empty()
        || runtime.cwd.is_some()
        || runtime.url.is_some()
        || !runtime.headers.is_empty()
    {
        return Err(ApiError::bad_request(
            "local connector runtime secrets and execution config must remain on the client",
        ));
    }
    Ok(())
}

pub(super) fn validate_mcp_visibility_for_runtime(
    visibility: &str,
    runtime: &McpRuntime,
) -> Result<(), ApiError> {
    if matches!(
        runtime.kind.as_str(),
        RUNTIME_KIND_LOCAL_CONNECTOR_STDIO
            | RUNTIME_KIND_LOCAL_CONNECTOR_HTTP
            | RUNTIME_KIND_LOCAL_CONNECTOR_BUILTIN_PROXY
    ) && visibility != VISIBILITY_PRIVATE
    {
        return Err(ApiError::bad_request(
            "local connector MCPs must use private visibility",
        ));
    }
    Ok(())
}

pub(super) fn validate_mcp_binding_mode(value: &str) -> Result<(), ApiError> {
    match value {
        MCP_BINDING_MODE_DISABLED | MCP_BINDING_MODE_OPTIONAL | MCP_BINDING_MODE_REQUIRED => Ok(()),
        _ => Err(ApiError::bad_request(
            "binding mode must be disabled, optional, or required",
        )),
    }
}

pub(super) fn mcp_binding_state(value: &str) -> Result<(bool, bool, &'static str), ApiError> {
    validate_mcp_binding_mode(value)?;
    Ok(match value {
        MCP_BINDING_MODE_DISABLED => (false, false, BINDING_SCOPE_GLOBAL_DEFAULT),
        MCP_BINDING_MODE_OPTIONAL => (true, false, BINDING_SCOPE_GLOBAL_DEFAULT),
        MCP_BINDING_MODE_REQUIRED => (true, true, BINDING_SCOPE_SYSTEM_REQUIRED),
        _ => unreachable!("validated MCP binding mode"),
    })
}
