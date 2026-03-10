use std::io::Write;
use std::time::Duration as StdDuration;

pub(super) fn is_ssh_would_block(err: &ssh2::Error) -> bool {
    matches!(err.code(), ssh2::ErrorCode::Session(code) if code == -37)
}

pub(super) fn is_io_would_block(err: &std::io::Error) -> bool {
    matches!(err.kind(), std::io::ErrorKind::WouldBlock)
}

pub(super) fn write_channel_nonblocking(
    channel: &mut ssh2::Channel,
    mut data: &[u8],
) -> Result<(), String> {
    while !data.is_empty() {
        match channel.write(data) {
            Ok(0) => return Err("remote channel closed".to_string()),
            Ok(n) => {
                data = &data[n..];
            }
            Err(err) => {
                if is_io_would_block(&err) {
                    std::thread::sleep(StdDuration::from_millis(6));
                    continue;
                }
                return Err(format!("write channel failed: {err}"));
            }
        }
    }
    Ok(())
}

pub(super) fn request_pty_resize_nonblocking(
    channel: &mut ssh2::Channel,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    for _ in 0..60 {
        match channel.request_pty_size(cols, rows, None, None) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if is_ssh_would_block(&err) {
                    std::thread::sleep(StdDuration::from_millis(5));
                    continue;
                }
                return Err(format!("request pty resize failed: {err}"));
            }
        }
    }
    Err("request pty resize timed out".to_string())
}
