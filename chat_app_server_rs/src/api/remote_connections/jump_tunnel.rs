use crate::models::remote_connection::RemoteConnection;
use ssh2::Session;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::time::{Duration as StdDuration, Instant};

use super::terminal_io::{is_io_would_block, is_ssh_would_block};
use super::{
    apply_host_key_policy, authenticate_jump_session, configure_stream_timeout, connect_tcp_stream,
};

fn forward_jump_tunnel(
    local_stream: &mut TcpStream,
    jump_channel: &mut ssh2::Channel,
) -> Result<(), String> {
    const BUFFER_SIZE: usize = 8192;
    const MAX_PENDING: usize = 256 * 1024;

    let mut from_local = [0u8; BUFFER_SIZE];
    let mut from_remote = [0u8; BUFFER_SIZE];
    let mut pending_to_remote = Vec::<u8>::new();
    let mut pending_to_local = Vec::<u8>::new();
    let mut local_eof = false;
    let mut remote_eof = false;
    let mut remote_eof_sent = false;
    let mut local_shutdown = false;

    loop {
        let mut progressed = false;

        if !local_eof && pending_to_remote.len() < MAX_PENDING {
            match local_stream.read(&mut from_local) {
                Ok(0) => {
                    local_eof = true;
                    progressed = true;
                }
                Ok(n) => {
                    pending_to_remote.extend_from_slice(&from_local[..n]);
                    progressed = true;
                }
                Err(err) => {
                    if !is_io_would_block(&err) {
                        return Err(format!("读取本地隧道失败: {err}"));
                    }
                }
            }
        }

        while !pending_to_remote.is_empty() {
            match jump_channel.write(pending_to_remote.as_slice()) {
                Ok(0) => return Err("跳板机隧道已关闭".to_string()),
                Ok(n) => {
                    pending_to_remote.drain(..n);
                    progressed = true;
                }
                Err(err) => {
                    if is_io_would_block(&err) {
                        break;
                    }
                    return Err(format!("写入跳板机隧道失败: {err}"));
                }
            }
        }

        if !remote_eof && pending_to_local.len() < MAX_PENDING {
            match jump_channel.read(&mut from_remote) {
                Ok(0) => {
                    if jump_channel.eof() {
                        remote_eof = true;
                        progressed = true;
                    }
                }
                Ok(n) => {
                    pending_to_local.extend_from_slice(&from_remote[..n]);
                    progressed = true;
                }
                Err(err) => {
                    if !is_io_would_block(&err) {
                        return Err(format!("读取跳板机隧道失败: {err}"));
                    }
                }
            }
        }

        while !pending_to_local.is_empty() {
            match local_stream.write(pending_to_local.as_slice()) {
                Ok(0) => return Err("本地隧道已关闭".to_string()),
                Ok(n) => {
                    pending_to_local.drain(..n);
                    progressed = true;
                }
                Err(err) => {
                    if is_io_would_block(&err) {
                        break;
                    }
                    return Err(format!("写入本地隧道失败: {err}"));
                }
            }
        }

        if local_eof && pending_to_remote.is_empty() && !remote_eof_sent {
            match jump_channel.send_eof() {
                Ok(_) => {
                    remote_eof_sent = true;
                    progressed = true;
                }
                Err(err) => {
                    if !is_ssh_would_block(&err) {
                        return Err(format!("关闭跳板机发送流失败: {err}"));
                    }
                }
            }
        }

        if remote_eof && pending_to_local.is_empty() && !local_shutdown {
            let _ = local_stream.shutdown(Shutdown::Write);
            local_shutdown = true;
            progressed = true;
        }

        if local_eof && remote_eof && pending_to_remote.is_empty() && pending_to_local.is_empty() {
            let _ = jump_channel.close();
            let _ = jump_channel.wait_close();
            return Ok(());
        }

        if !progressed {
            std::thread::sleep(StdDuration::from_millis(5));
        }
    }
}

fn run_jump_tunnel_bridge(
    listener: TcpListener,
    jump_session: Session,
    mut jump_channel: ssh2::Channel,
    timeout: StdDuration,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    let mut local_stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(err) => {
                if is_io_would_block(&err) {
                    if Instant::now() >= deadline {
                        return Err("等待本地跳板连接超时".to_string());
                    }
                    std::thread::sleep(StdDuration::from_millis(5));
                    continue;
                }
                return Err(format!("接受本地跳板连接失败: {err}"));
            }
        }
    };

    local_stream
        .set_nonblocking(true)
        .map_err(|e| format!("设置本地跳板非阻塞失败: {e}"))?;
    jump_session.set_blocking(false);

    forward_jump_tunnel(&mut local_stream, &mut jump_channel)
}

pub(super) fn create_jump_tunnel_stream(
    connection: &RemoteConnection,
    timeout: StdDuration,
    timeout_ms: u32,
    verification_code: Option<&str>,
) -> Result<TcpStream, String> {
    let jump_host = connection
        .jump_host
        .as_deref()
        .ok_or_else(|| "启用跳板机时 jump_host 不能为空".to_string())?;
    let jump_username = connection
        .jump_username
        .as_deref()
        .ok_or_else(|| "启用跳板机时 jump_username 不能为空".to_string())?;
    let jump_port = connection.jump_port.unwrap_or(22);

    let jump_stream = connect_tcp_stream(jump_host, jump_port, timeout, "跳板机")?;
    configure_stream_timeout(&jump_stream, timeout, "跳板机")?;

    let mut jump_session = Session::new().map_err(|e| format!("创建跳板机 SSH 会话失败: {e}"))?;
    jump_session.set_tcp_stream(jump_stream);
    jump_session.set_timeout(timeout_ms);
    jump_session
        .handshake()
        .map_err(|e| format!("跳板机 SSH 握手失败: {e}"))?;
    apply_host_key_policy(
        &jump_session,
        jump_host,
        jump_port,
        connection.host_key_policy.as_str(),
    )?;
    authenticate_jump_session(&jump_session, connection, jump_username, verification_code)?;
    if !jump_session.authenticated() {
        return Err("跳板机 SSH 认证失败".to_string());
    }

    let target_port = u16::try_from(connection.port).map_err(|_| "目标端口无效".to_string())?;
    let jump_channel = jump_session
        .channel_direct_tcpip(connection.host.as_str(), target_port, None)
        .map_err(|e| format!("建立跳板机转发通道失败: {e}"))?;

    let listener =
        TcpListener::bind(("127.0.0.1", 0)).map_err(|e| format!("创建本地跳板通道失败: {e}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("设置本地跳板通道失败: {e}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|e| format!("获取本地跳板地址失败: {e}"))?;

    std::thread::spawn(move || {
        let _ = run_jump_tunnel_bridge(listener, jump_session, jump_channel, timeout);
    });

    let local_stream = TcpStream::connect_timeout(&local_addr, timeout)
        .map_err(|e| format!("连接本地跳板通道失败: {e}"))?;
    configure_stream_timeout(&local_stream, timeout, "本地跳板通道")?;
    Ok(local_stream)
}
