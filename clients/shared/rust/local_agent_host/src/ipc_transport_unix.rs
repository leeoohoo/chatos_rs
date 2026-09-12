// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UnixListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::ipc_transport_common::{
    handle_framed_connection, DEFAULT_IO_TIMEOUT, DEFAULT_MAXIMUM_CONNECTIONS,
};
use crate::{LocalAgentIpcServer, DEFAULT_MAXIMUM_IPC_FRAME_BYTES};

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
                        handle_framed_connection(
                            stream,
                            server.as_ref(),
                            maximum_frame_bytes,
                            io_timeout,
                        )
                        .await
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
