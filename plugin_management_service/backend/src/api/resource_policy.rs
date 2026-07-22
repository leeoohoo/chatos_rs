// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn required_text(value: Option<&str>, field: &str) -> Result<String, ApiError> {
    normalized(value).ok_or_else(|| ApiError::bad_request(format!("{field} is required")))
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
            if runtime
                .url
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request("HTTP MCP requires url"));
            }
        }
        RUNTIME_KIND_STDIO_CLOUD => {
            if runtime
                .command
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request("stdio MCP requires command"));
            }
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

pub(super) fn validate_skill_content(content: &SkillContent) -> Result<(), ApiError> {
    match content.kind.as_str() {
        SKILL_CONTENT_KIND_LOCAL_CONNECTOR_BUNDLE => {
            for (value, field) in [
                (content.bundle_id.as_deref(), "bundle_id"),
                (content.bundle_version.as_deref(), "bundle_version"),
                (content.entrypoint_kind.as_deref(), "entrypoint_kind"),
            ] {
                if value.and_then(|value| normalized(Some(value))).is_none() {
                    return Err(ApiError::bad_request(format!(
                        "local connector bundle skill requires {field}"
                    )));
                }
            }
            if content.inline.is_some()
                || content.package_id.is_some()
                || content.source_path.is_some()
                || content.repository.is_some()
                || content.branch.is_some()
                || content.local_connector.is_some()
            {
                return Err(ApiError::bad_request(
                    "local connector bundle skill cannot contain cloud or device-specific content",
                ));
            }
        }
        "inline_content" => {
            if content
                .inline
                .as_deref()
                .and_then(|value| normalized(Some(value)))
                .is_none()
            {
                return Err(ApiError::bad_request(
                    "inline skill requires inline content",
                ));
            }
        }
        "cloud_package" | "git_package" => {}
        "local_connector_file" | "local_connector_package" => {
            if content.local_connector.is_none() {
                return Err(ApiError::bad_request(
                    "local connector skill requires local_connector",
                ));
            }
        }
        _ => {
            return Err(ApiError::bad_request(
                "content.kind must be local_connector_bundle, inline_content, cloud_package, git_package, local_connector_file, or local_connector_package",
            ));
        }
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
