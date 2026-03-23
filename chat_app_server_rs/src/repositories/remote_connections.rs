use mongodb::bson::Document;

use crate::core::secrets::{decrypt_optional_secret, encrypt_optional_secret};
use crate::models::remote_connection::RemoteConnection;

mod read_ops;
mod write_ops;

pub use self::read_ops::{get_remote_connection_by_id, list_remote_connections};
pub use self::write_ops::{
    create_remote_connection, delete_remote_connection, touch_remote_connection,
    update_remote_connection,
};

pub(super) fn decrypt_optional_secret_lossy(value: Option<String>) -> Option<String> {
    let fallback = value.clone();
    decrypt_optional_secret(value).unwrap_or(fallback)
}

pub(super) fn decrypt_connection_for_read(mut connection: RemoteConnection) -> RemoteConnection {
    connection.password = decrypt_optional_secret_lossy(connection.password);
    connection.jump_password = decrypt_optional_secret_lossy(connection.jump_password);
    connection
}

pub(super) fn encrypt_connection_for_storage(
    mut connection: RemoteConnection,
) -> Result<RemoteConnection, String> {
    connection.password = encrypt_optional_secret(connection.password)?;
    connection.jump_password = encrypt_optional_secret(connection.jump_password)?;
    Ok(connection)
}

pub(super) fn normalize_doc(doc: &Document) -> Option<RemoteConnection> {
    let jump_enabled = doc
        .get_bool("jump_enabled")
        .ok()
        .or_else(|| doc.get_i64("jump_enabled").ok().map(|v| v != 0))
        .or_else(|| doc.get_i32("jump_enabled").ok().map(|v| v != 0))
        .unwrap_or(false);

    let connection = RemoteConnection {
        id: doc.get_str("id").ok()?.to_string(),
        name: doc.get_str("name").ok()?.to_string(),
        host: doc.get_str("host").ok()?.to_string(),
        port: doc.get_i64("port").unwrap_or(22),
        username: doc.get_str("username").ok()?.to_string(),
        auth_type: doc
            .get_str("auth_type")
            .unwrap_or("private_key")
            .to_string(),
        password: doc.get_str("password").ok().map(|s| s.to_string()),
        private_key_path: doc.get_str("private_key_path").ok().map(|s| s.to_string()),
        certificate_path: doc.get_str("certificate_path").ok().map(|s| s.to_string()),
        default_remote_path: doc
            .get_str("default_remote_path")
            .ok()
            .map(|s| s.to_string()),
        host_key_policy: doc
            .get_str("host_key_policy")
            .unwrap_or("strict")
            .to_string(),
        jump_enabled,
        jump_host: doc.get_str("jump_host").ok().map(|s| s.to_string()),
        jump_port: doc.get_i64("jump_port").ok(),
        jump_username: doc.get_str("jump_username").ok().map(|s| s.to_string()),
        jump_private_key_path: doc
            .get_str("jump_private_key_path")
            .ok()
            .map(|s| s.to_string()),
        jump_password: doc.get_str("jump_password").ok().map(|s| s.to_string()),
        user_id: doc.get_str("user_id").ok().map(|s| s.to_string()),
        created_at: doc.get_str("created_at").unwrap_or("").to_string(),
        updated_at: doc.get_str("updated_at").unwrap_or("").to_string(),
        last_active_at: doc.get_str("last_active_at").unwrap_or("").to_string(),
    };

    Some(decrypt_connection_for_read(connection))
}
