use ssh2::Session;
use std::path::Path as FsPath;

use crate::models::remote_connection::RemoteConnection;

pub(super) fn authenticate_target_session(
    session: &Session,
    connection: &RemoteConnection,
) -> Result<(), String> {
    match connection.auth_type.as_str() {
        "password" => {
            let password = connection
                .password
                .as_ref()
                .ok_or_else(|| "password 模式需要提供 password".to_string())?;
            session
                .userauth_password(connection.username.as_str(), password.as_str())
                .map_err(|e| format!("密码认证失败: {e}"))?;
        }
        "private_key" | "private_key_cert" => {
            let private_key = connection
                .private_key_path
                .as_ref()
                .ok_or_else(|| "私钥路径不能为空".to_string())?;
            let cert_path = connection.certificate_path.as_ref().map(FsPath::new);
            session
                .userauth_pubkey_file(
                    connection.username.as_str(),
                    cert_path,
                    FsPath::new(private_key),
                    None,
                )
                .map_err(|e| format!("密钥认证失败: {e}"))?;
        }
        _ => return Err("不支持的认证方式".to_string()),
    }
    Ok(())
}

pub(super) fn authenticate_jump_session(
    session: &Session,
    connection: &RemoteConnection,
    jump_username: &str,
) -> Result<(), String> {
    let mut failures = Vec::new();

    if let Some(jump_key_path) = connection.jump_private_key_path.as_ref() {
        match session.userauth_pubkey_file(jump_username, None, FsPath::new(jump_key_path), None) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("jump_private_key_path 认证失败: {err}")),
        }
    }

    if let Some(jump_password) = connection.jump_password.as_ref() {
        match session.userauth_password(jump_username, jump_password.as_str()) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("jump_password 认证失败: {err}")),
        }
    }

    if connection.auth_type != "password" {
        if let Some(private_key_path) = connection.private_key_path.as_ref() {
            let cert_path = connection.certificate_path.as_ref().map(FsPath::new);
            match session.userauth_pubkey_file(
                jump_username,
                cert_path,
                FsPath::new(private_key_path),
                None,
            ) {
                Ok(_) => return Ok(()),
                Err(err) => failures.push(format!("复用目标密钥认证失败: {err}")),
            }
        }
    }

    match session.userauth_agent(jump_username) {
        Ok(_) => return Ok(()),
        Err(err) => failures.push(format!("SSH Agent 认证失败: {err}")),
    }

    if let Some(password) = connection.password.as_ref() {
        match session.userauth_password(jump_username, password.as_str()) {
            Ok(_) => return Ok(()),
            Err(err) => failures.push(format!("使用同密码认证失败: {err}")),
        }
    }

    if failures.is_empty() {
        return Err("跳板机认证失败".to_string());
    }

    Err(format!(
        "跳板机认证失败：{}。请配置 jump_private_key_path、jump_password 或 SSH Agent",
        failures.join("；")
    ))
}
