// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

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
