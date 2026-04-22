use crate::models::remote_connection::{RemoteConnection, RemoteConnectionService};

pub(crate) async fn resolve_jump_connection_snapshot(
    connection: &RemoteConnection,
) -> Result<RemoteConnection, String> {
    if !connection.jump_enabled {
        return Ok(connection.clone());
    }

    let Some(jump_connection_id) = connection
        .jump_connection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(connection.clone());
    };

    let jump_connection = RemoteConnectionService::get_by_id(jump_connection_id)
        .await?
        .ok_or_else(|| format!("跳板机连接不存在: {jump_connection_id}"))?;

    if jump_connection.user_id != connection.user_id {
        return Err("无权使用该跳板机连接".to_string());
    }

    let mut resolved = connection.clone();
    resolved.jump_host = Some(jump_connection.host.clone());
    resolved.jump_port = Some(jump_connection.port);
    resolved.jump_username = Some(jump_connection.username.clone());
    resolved.jump_private_key_path = jump_connection.private_key_path.clone();
    resolved.jump_certificate_path = jump_connection.certificate_path.clone();
    resolved.jump_password = jump_connection.password.clone();

    Ok(resolved)
}
