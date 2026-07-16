// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use crate::{is_valid_ssh_port, RemoteRuntimeError, RemoteRuntimeErrorKind};

pub fn connect_tcp_stream(
    host: &str,
    port: i64,
    timeout: Duration,
) -> Result<TcpStream, RemoteRuntimeError> {
    if !is_valid_ssh_port(port) {
        return Err(RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::InvalidPort,
            "SSH 端口必须位于 1-65535",
        ));
    }

    let address = format!("{host}:{port}");
    let addresses = address.to_socket_addrs().map_err(|error| {
        RemoteRuntimeError::new(RemoteRuntimeErrorKind::AddressResolution, error.to_string())
    })?;
    let mut resolved_any = false;
    let mut last_error = None;
    for socket in addresses {
        resolved_any = true;
        match TcpStream::connect_timeout(&socket, timeout) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error.to_string()),
        }
    }

    if !resolved_any {
        return Err(RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::NoResolvedAddress,
            address,
        ));
    }
    Err(RemoteRuntimeError::new(
        RemoteRuntimeErrorKind::Connect,
        last_error.unwrap_or_else(|| "无可用地址".to_string()),
    ))
}

pub fn configure_stream_timeout(
    stream: &TcpStream,
    timeout: Duration,
) -> Result<(), RemoteRuntimeError> {
    stream.set_read_timeout(Some(timeout)).map_err(|error| {
        RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::ReadTimeoutConfiguration,
            error.to_string(),
        )
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|error| {
        RemoteRuntimeError::new(
            RemoteRuntimeErrorKind::WriteTimeoutConfiguration,
            error.to_string(),
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_ports_fail_before_address_resolution() {
        let error = connect_tcp_stream("localhost", 65_536, Duration::from_millis(1))
            .expect_err("invalid port must be rejected");
        assert_eq!(error.kind(), RemoteRuntimeErrorKind::InvalidPort);
        assert_eq!(error.to_string(), "SSH 端口必须位于 1-65535");
    }
}
