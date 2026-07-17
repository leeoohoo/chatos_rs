// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::net::TcpStream;
use std::time::Duration as StdDuration;

pub(super) fn connect_tcp_stream(
    host: &str,
    port: i64,
    timeout: StdDuration,
    label: &str,
) -> Result<TcpStream, String> {
    chatos_remote_runtime::connect_tcp_stream(host, port, timeout)
        .map_err(|error| error.format_tcp_context(label, label))
}

pub(super) fn configure_stream_timeout(
    stream: &TcpStream,
    timeout: StdDuration,
    label: &str,
) -> Result<(), String> {
    chatos_remote_runtime::configure_stream_timeout(stream, timeout)
        .map_err(|error| error.format_tcp_context(label, label))
}
