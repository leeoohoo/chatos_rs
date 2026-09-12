// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::LocalAgentIpcServer;

pub(crate) const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const DEFAULT_MAXIMUM_CONNECTIONS: usize = 64;

pub(crate) async fn handle_framed_connection<T>(
    mut stream: T,
    server: &LocalAgentIpcServer,
    maximum_frame_bytes: usize,
    io_timeout: Duration,
) -> Result<(), io::Error>
where
    T: AsyncRead + AsyncWrite + Unpin,
{
    tokio::time::timeout(io_timeout, async {
        let frame_length = stream.read_u32().await? as usize;
        if frame_length == 0 || frame_length > maximum_frame_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "local Agent IPC request frame exceeds its boundary",
            ));
        }
        let mut frame = vec![0u8; frame_length];
        stream.read_exact(frame.as_mut_slice()).await?;
        let reply = server
            .handle_frame(frame.as_slice())
            .await
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        if reply.len() > maximum_frame_bytes || reply.len() > u32::MAX as usize {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "local Agent IPC reply frame exceeds its boundary",
            ));
        }
        stream.write_u32(reply.len() as u32).await?;
        stream.write_all(reply.as_slice()).await?;
        stream.shutdown().await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "local Agent IPC request timed out"))?
}
