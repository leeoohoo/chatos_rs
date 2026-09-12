// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::{LocalAgentIpcServer, DEFAULT_MAXIMUM_IPC_FRAME_BYTES};

const DEFAULT_IO_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAXIMUM_CONNECTIONS: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum UnixLocalAgentIpcError {
    #[error("local Agent Unix socket path must be an absolute path with an existing directory")]
    InvalidSocketPath,
    #[error("local Agent Unix socket path already exists")]
    SocketPathExists,
    #[error("local Agent Unix IPC frame limit is invalid")]
    InvalidFrameLimit,
    #[error("local Agent Unix IPC timeout or connection limit is invalid")]
    InvalidTransportLimit,
    #[error("local Agent Unix socket operation failed: {0}")]
    Io(#[from] io::Error),
}

pub struct UnixLocalAgentIpcTransport {
    listener: UnixListener,
    socket_path: PathBuf,
    server: Arc<LocalAgentIpcServer>,
    expected_peer_uid: u32,
    maximum_frame_bytes: usize,
    io_timeout: Duration,
    maximum_connections: usize,
    socket_device: u64,
    socket_inode: u64,
}

impl UnixLocalAgentIpcTransport {
    pub fn bind(
        socket_path: impl Into<PathBuf>,
        server: Arc<LocalAgentIpcServer>,
        expected_peer_uid: u32,
    ) -> Result<Self, UnixLocalAgentIpcError> {
        Self::bind_with_limits(
            socket_path,
            server,
            expected_peer_uid,
            DEFAULT_MAXIMUM_IPC_FRAME_BYTES,
            DEFAULT_IO_TIMEOUT,
            DEFAULT_MAXIMUM_CONNECTIONS,
        )
    }

    pub fn bind_with_limits(
        socket_path: impl Into<PathBuf>,
        server: Arc<LocalAgentIpcServer>,
        expected_peer_uid: u32,
        maximum_frame_bytes: usize,
        io_timeout: Duration,
        maximum_connections: usize,
    ) -> Result<Self, UnixLocalAgentIpcError> {
        let socket_path = socket_path.into();
        validate_socket_path(socket_path.as_path())?;
        if socket_path.exists() {
            return Err(UnixLocalAgentIpcError::SocketPathExists);
        }
        if maximum_frame_bytes == 0 || maximum_frame_bytes > u32::MAX as usize {
            return Err(UnixLocalAgentIpcError::InvalidFrameLimit);
        }
        if io_timeout.is_zero() || maximum_connections == 0 {
            return Err(UnixLocalAgentIpcError::InvalidTransportLimit);
        }
        let listener = UnixListener::bind(socket_path.as_path())?;
        let mut cleanup = SocketCleanup {
            path: socket_path.clone(),
            expected_identity: None,
        };
        std::fs::set_permissions(
            socket_path.as_path(),
            std::fs::Permissions::from_mode(0o600),
        )?;
        let metadata = std::fs::symlink_metadata(socket_path.as_path())?;
        if !metadata.file_type().is_socket() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(UnixLocalAgentIpcError::Io(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Unix socket is not private to the current user",
            )));
        }
        let socket_device = metadata.dev();
        let socket_inode = metadata.ino();
        cleanup.expected_identity = Some((socket_device, socket_inode));
        let transport = Self {
            listener,
            socket_path,
            server,
            expected_peer_uid,
            maximum_frame_bytes,
            io_timeout,
            maximum_connections,
            socket_device,
            socket_inode,
        };
        std::mem::forget(cleanup);
        Ok(transport)
    }

    pub async fn serve(
        self,
        cancellation: CancellationToken,
    ) -> Result<(), UnixLocalAgentIpcError> {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break,
                Some(joined) = connections.join_next(), if !connections.is_empty() => {
                    if let Err(error) = joined {
                        if !error.is_cancelled() {
                            return Err(UnixLocalAgentIpcError::Io(io::Error::other(
                                format!("Unix IPC connection task failed: {error}"),
                            )));
                        }
                    }
                }
                accepted = self.listener.accept(), if connections.len() < self.maximum_connections => {
                    let (stream, _) = accepted?;
                    if stream.peer_cred()?.uid() != self.expected_peer_uid {
                        continue;
                    }
                    let server = self.server.clone();
                    let maximum_frame_bytes = self.maximum_frame_bytes;
                    let io_timeout = self.io_timeout;
                    connections.spawn(async move {
                        handle_connection(stream, server, maximum_frame_bytes, io_timeout).await
                    });
                }
            }
        }
        connections.abort_all();
        while connections.join_next().await.is_some() {}
        Ok(())
    }

    pub fn socket_path(&self) -> &Path {
        self.socket_path.as_path()
    }
}

impl Drop for UnixLocalAgentIpcTransport {
    fn drop(&mut self) {
        SocketCleanup {
            path: self.socket_path.clone(),
            expected_identity: Some((self.socket_device, self.socket_inode)),
        }
        .remove();
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    server: Arc<LocalAgentIpcServer>,
    maximum_frame_bytes: usize,
    io_timeout: Duration,
) -> Result<(), io::Error> {
    tokio::time::timeout(io_timeout, async {
        let frame_length = stream.read_u32().await? as usize;
        if frame_length == 0 || frame_length > maximum_frame_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unix IPC request frame exceeds its boundary",
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
                "Unix IPC reply frame exceeds its boundary",
            ));
        }
        stream.write_u32(reply.len() as u32).await?;
        stream.write_all(reply.as_slice()).await?;
        stream.shutdown().await
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Unix IPC request timed out"))?
}

fn validate_socket_path(path: &Path) -> Result<(), UnixLocalAgentIpcError> {
    if !path.is_absolute()
        || path.file_name().is_none()
        || path.parent().is_none_or(|parent| !parent.is_dir())
    {
        return Err(UnixLocalAgentIpcError::InvalidSocketPath);
    }
    Ok(())
}

struct SocketCleanup {
    path: PathBuf,
    expected_identity: Option<(u64, u64)>,
}

impl SocketCleanup {
    fn remove(&self) {
        let Ok(metadata) = std::fs::symlink_metadata(self.path.as_path()) else {
            return;
        };
        if metadata.file_type().is_socket()
            && self
                .expected_identity
                .is_none_or(|identity| identity == (metadata.dev(), metadata.ino()))
        {
            let _ = std::fs::remove_file(self.path.as_path());
        }
    }
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        self.remove();
    }
}
