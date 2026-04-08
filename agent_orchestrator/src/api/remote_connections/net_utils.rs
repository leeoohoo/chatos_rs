use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration as StdDuration;

pub(super) fn connect_tcp_stream(
    host: &str,
    port: i64,
    timeout: StdDuration,
    label: &str,
) -> Result<TcpStream, String> {
    let addr = format!("{host}:{port}");
    let mut last_error = None;
    let mut stream_opt = None;
    let addrs = addr
        .to_socket_addrs()
        .map_err(|e| format!("解析{label}地址失败: {e}"))?;
    for socket in addrs {
        match TcpStream::connect_timeout(&socket, timeout) {
            Ok(stream) => {
                stream_opt = Some(stream);
                break;
            }
            Err(err) => {
                last_error = Some(err.to_string());
            }
        }
    }
    stream_opt.ok_or_else(|| {
        format!(
            "连接{label}失败: {}",
            last_error.unwrap_or_else(|| "无可用地址".to_string())
        )
    })
}

pub(super) fn configure_stream_timeout(
    stream: &TcpStream,
    timeout: StdDuration,
    label: &str,
) -> Result<(), String> {
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("设置{label}读超时失败: {e}"))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|e| format!("设置{label}写超时失败: {e}"))?;
    Ok(())
}
