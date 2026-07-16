// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;

use ssh2::Session;

use crate::{apply_host_key_policy, RemoteRuntimeError, RemoteRuntimeErrorKind};

pub fn ssh_timeout_millis(timeout: Duration) -> u32 {
    timeout.as_millis().clamp(1_000, u32::MAX as u128) as u32
}

pub fn authenticate_private_key_file(
    session: &Session,
    username: &str,
    private_key_path: &Path,
    certificate_path: Option<&Path>,
    passphrase: Option<&str>,
) -> Result<(), ssh2::Error> {
    session.userauth_pubkey_file(username, certificate_path, private_key_path, passphrase)
}

pub fn establish_ssh_session<F>(
    stream: TcpStream,
    timeout: Duration,
    host: &str,
    port: i64,
    host_key_policy: &str,
    authenticate: F,
) -> Result<Session, RemoteRuntimeError>
where
    F: FnOnce(&Session) -> Result<(), String>,
{
    let mut session = Session::new().map_err(|error| {
        RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::SessionCreation,
            format!("创建 SSH 会话失败: {error}"),
        )
    })?;
    session.set_tcp_stream(stream);
    session.set_timeout(ssh_timeout_millis(timeout));
    session.handshake().map_err(|error| {
        RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::Handshake,
            format!("SSH 握手失败: {error}"),
        )
    })?;
    apply_host_key_policy(&session, host, port, host_key_policy)
        .map_err(|message| RemoteRuntimeError::new(RemoteRuntimeErrorKind::HostKey, message))?;
    authenticate(&session).map_err(|message| {
        RemoteRuntimeError::new(RemoteRuntimeErrorKind::Authentication, message)
    })?;
    if !session.authenticated() {
        return Err(RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::Authentication,
            "SSH 认证失败",
        ));
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_timeout_is_clamped_to_ssh2_range() {
        assert_eq!(ssh_timeout_millis(Duration::from_millis(1)), 1_000);
        assert_eq!(ssh_timeout_millis(Duration::from_secs(12)), 12_000);
        assert_eq!(
            ssh_timeout_millis(Duration::from_millis(u64::MAX)),
            u32::MAX
        );
    }
}
