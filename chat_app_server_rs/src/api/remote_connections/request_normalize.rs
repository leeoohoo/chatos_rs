use std::path::Path as FsPath;

use crate::core::validation::normalize_non_empty;
use crate::models::remote_connection::RemoteConnection;

use super::{CreateRemoteConnectionRequest, UpdateRemoteConnectionRequest};

pub(super) fn normalize_create_request(
    req: CreateRemoteConnectionRequest,
    user_id: Option<String>,
) -> Result<RemoteConnection, String> {
    let host = normalize_non_empty(req.host).ok_or_else(|| "host 不能为空".to_string())?;
    let username =
        normalize_non_empty(req.username).ok_or_else(|| "username 不能为空".to_string())?;
    let port = normalize_port(req.port.unwrap_or(22))?;
    let auth_type = normalize_auth_type(req.auth_type)?;
    let host_key_policy = normalize_host_key_policy(req.host_key_policy)?;
    let jump_enabled = req.jump_enabled.unwrap_or(false);

    let raw_password = normalize_non_empty(req.password);
    let raw_private_key_path = normalize_non_empty(req.private_key_path);
    let raw_certificate_path = normalize_non_empty(req.certificate_path);
    let jump_private_key_path = normalize_non_empty(req.jump_private_key_path);
    let jump_password = normalize_non_empty(req.jump_password);
    let (password, private_key_path, certificate_path) = match auth_type.as_str() {
        "password" => (raw_password, None, None),
        "private_key" => (None, raw_private_key_path, None),
        "private_key_cert" => (None, raw_private_key_path, raw_certificate_path),
        _ => return Err("不支持的 auth_type".to_string()),
    };

    validate_auth_fields(
        auth_type.as_str(),
        password.as_deref(),
        private_key_path.as_deref(),
        certificate_path.as_deref(),
    )?;
    validate_file_path_if_present(private_key_path.as_deref(), "private_key_path 文件不存在")?;
    validate_file_path_if_present(certificate_path.as_deref(), "certificate_path 文件不存在")?;
    validate_file_path_if_present(
        jump_private_key_path.as_deref(),
        "jump_private_key_path 文件不存在",
    )?;

    let jump_host = normalize_non_empty(req.jump_host);
    let jump_username = normalize_non_empty(req.jump_username);
    let jump_port = req.jump_port.map(normalize_port).transpose()?.or(Some(22));

    if jump_enabled && (jump_host.is_none() || jump_username.is_none()) {
        return Err("启用跳板机时 jump_host 和 jump_username 为必填".to_string());
    }

    let name = normalize_non_empty(req.name).unwrap_or_else(|| format!("{username}@{host}"));

    Ok(RemoteConnection::new(
        name,
        host,
        port,
        username,
        auth_type,
        password,
        private_key_path,
        certificate_path,
        normalize_non_empty(req.default_remote_path),
        host_key_policy,
        jump_enabled,
        jump_host,
        if jump_enabled { jump_port } else { None },
        jump_username,
        if jump_enabled {
            jump_private_key_path
        } else {
            None
        },
        if jump_enabled { jump_password } else { None },
        user_id,
    ))
}

pub(super) fn normalize_update_request(
    req: UpdateRemoteConnectionRequest,
    existing: RemoteConnection,
) -> Result<RemoteConnection, String> {
    let host = req
        .host
        .and_then(|v| normalize_non_empty(Some(v)))
        .unwrap_or(existing.host.clone());
    let username = req
        .username
        .and_then(|v| normalize_non_empty(Some(v)))
        .unwrap_or(existing.username.clone());
    let port = match req.port {
        Some(p) => normalize_port(p)?,
        None => existing.port,
    };
    let auth_type = normalize_auth_type(req.auth_type.or(Some(existing.auth_type.clone())))?;
    let host_key_policy = normalize_host_key_policy(
        req.host_key_policy
            .or(Some(existing.host_key_policy.clone())),
    )?;
    let password_candidate = merge_optional_text(req.password, existing.password.clone());
    let private_key_candidate =
        merge_optional_text(req.private_key_path, existing.private_key_path.clone());
    let certificate_candidate =
        merge_optional_text(req.certificate_path, existing.certificate_path.clone());
    let (password, private_key_path, certificate_path) = match auth_type.as_str() {
        "password" => (password_candidate, None, None),
        "private_key" => (None, private_key_candidate, None),
        "private_key_cert" => (None, private_key_candidate, certificate_candidate),
        _ => return Err("不支持的 auth_type".to_string()),
    };

    let jump_enabled = req.jump_enabled.unwrap_or(existing.jump_enabled);

    let jump_host = merge_optional_text(req.jump_host, existing.jump_host.clone());
    let jump_username = merge_optional_text(req.jump_username, existing.jump_username.clone());
    let jump_private_key_path = merge_optional_text(
        req.jump_private_key_path,
        existing.jump_private_key_path.clone(),
    );
    let jump_password = merge_optional_text(req.jump_password, existing.jump_password.clone());

    let jump_port = match req.jump_port {
        Some(v) => Some(normalize_port(v)?),
        None => existing.jump_port.or(Some(22)),
    };

    validate_auth_fields(
        auth_type.as_str(),
        password.as_deref(),
        private_key_path.as_deref(),
        certificate_path.as_deref(),
    )?;
    validate_file_path_if_present(private_key_path.as_deref(), "private_key_path 文件不存在")?;
    validate_file_path_if_present(certificate_path.as_deref(), "certificate_path 文件不存在")?;
    validate_file_path_if_present(
        jump_private_key_path.as_deref(),
        "jump_private_key_path 文件不存在",
    )?;

    if jump_enabled && (jump_host.is_none() || jump_username.is_none()) {
        return Err("启用跳板机时 jump_host 和 jump_username 为必填".to_string());
    }

    let name = req
        .name
        .and_then(|v| normalize_non_empty(Some(v)))
        .unwrap_or(existing.name.clone());

    Ok(RemoteConnection {
        id: existing.id,
        name,
        host,
        port,
        username,
        auth_type,
        password,
        private_key_path,
        certificate_path,
        default_remote_path: merge_optional_text(
            req.default_remote_path,
            existing.default_remote_path,
        ),
        host_key_policy,
        jump_enabled,
        jump_host: if jump_enabled { jump_host } else { None },
        jump_port: if jump_enabled { jump_port } else { None },
        jump_username: if jump_enabled { jump_username } else { None },
        jump_private_key_path: if jump_enabled {
            jump_private_key_path
        } else {
            None
        },
        jump_password: if jump_enabled { jump_password } else { None },
        user_id: existing.user_id,
        created_at: existing.created_at,
        updated_at: existing.updated_at,
        last_active_at: existing.last_active_at,
    })
}

pub(super) fn normalize_transfer_direction(direction: Option<String>) -> Result<String, String> {
    match normalize_non_empty(direction)
        .unwrap_or_else(|| "upload".to_string())
        .to_lowercase()
        .as_str()
    {
        "upload" => Ok("upload".to_string()),
        "download" => Ok("download".to_string()),
        _ => Err("direction 仅支持 upload 或 download".to_string()),
    }
}

fn merge_optional_text(value: Option<String>, fallback: Option<String>) -> Option<String> {
    match value {
        Some(v) => normalize_non_empty(Some(v)),
        None => fallback,
    }
}

fn normalize_port(port: i64) -> Result<i64, String> {
    if !(1..=65535).contains(&port) {
        return Err("端口范围必须在 1-65535".to_string());
    }
    Ok(port)
}

fn normalize_auth_type(value: Option<String>) -> Result<String, String> {
    let raw = normalize_non_empty(value).unwrap_or_else(|| "private_key".to_string());
    match raw.as_str() {
        "private_key" | "private_key_cert" | "password" => Ok(raw),
        _ => Err("auth_type 仅支持 private_key、private_key_cert 或 password".to_string()),
    }
}

fn normalize_host_key_policy(value: Option<String>) -> Result<String, String> {
    let raw = normalize_non_empty(value).unwrap_or_else(|| "strict".to_string());
    match raw.as_str() {
        "strict" | "accept_new" => Ok(raw),
        _ => Err("host_key_policy 仅支持 strict 或 accept_new".to_string()),
    }
}

fn validate_auth_fields(
    auth_type: &str,
    password: Option<&str>,
    private_key_path: Option<&str>,
    certificate_path: Option<&str>,
) -> Result<(), String> {
    match auth_type {
        "password" => {
            if password.is_none() {
                return Err("password 模式需要提供 password".to_string());
            }
        }
        "private_key" => {
            if private_key_path.is_none() {
                return Err("private_key 模式需要提供 private_key_path".to_string());
            }
        }
        "private_key_cert" => {
            if private_key_path.is_none() {
                return Err("private_key_cert 模式需要提供 private_key_path".to_string());
            }
            if certificate_path.is_none() {
                return Err("private_key_cert 模式需要提供 certificate_path".to_string());
            }
        }
        _ => return Err("不支持的 auth_type".to_string()),
    }
    Ok(())
}

fn validate_file_path_if_present(path: Option<&str>, error_message: &str) -> Result<(), String> {
    if let Some(path) = path {
        if !FsPath::new(path).is_file() {
            return Err(error_message.to_string());
        }
    }
    Ok(())
}
