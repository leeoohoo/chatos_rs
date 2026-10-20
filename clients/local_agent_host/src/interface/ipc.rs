// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

//! Framed IPC adapters for the client-owned Local Agent Host.

use chatos_local_agent_protocol::{
    HostError, HostRequestEnvelope, HostResponseEnvelope, LOCAL_AGENT_MAX_FRAME_BYTES,
    LOCAL_AGENT_MAX_MESSAGE_BYTES,
};
use std::{io, sync::Arc};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, Semaphore},
};

const MAX_IN_FLIGHT_REQUESTS: usize = 128;
const FRAME_CONTINUATION_BIT: u32 = 1 << 31;
const FRAME_LENGTH_MASK: u32 = !FRAME_CONTINUATION_BIT;

#[async_trait::async_trait]
pub trait HostRequestHandler: Send + Sync {
    async fn handle_request(&self, request: HostRequestEnvelope) -> HostResponseEnvelope;
}

#[derive(Debug, Error)]
pub enum HostTransportError {
    #[error("IPC I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error(
        "IPC message exceeds the {LOCAL_AGENT_MAX_MESSAGE_BYTES} byte limit or contains an invalid frame"
    )]
    FrameTooLarge,
    #[error("IPC frame contains invalid JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),
    #[error("IPC response writer stopped unexpectedly: {0}")]
    WriterTask(#[from] tokio::task::JoinError),
    #[error("IPC response writer stopped before the request stream ended")]
    WriterStopped,
}

pub async fn serve_stream<S, H>(stream: S, handler: Arc<H>) -> Result<(), HostTransportError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    H: HostRequestHandler + ?Sized + 'static,
{
    let (reader, writer) = tokio::io::split(stream);
    serve_reader_writer(reader, writer, handler).await
}

pub async fn serve_reader_writer<R, W, H>(
    reader: R,
    writer: W,
    handler: Arc<H>,
) -> Result<(), HostTransportError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    H: HostRequestHandler + ?Sized + 'static,
{
    serve_reader_writer_with_response_limit(reader, writer, handler, LOCAL_AGENT_MAX_MESSAGE_BYTES)
        .await
}

async fn serve_reader_writer_with_response_limit<R, W, H>(
    mut reader: R,
    mut writer: W,
    handler: Arc<H>,
    maximum_response_bytes: usize,
) -> Result<(), HostTransportError>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin + Send + 'static,
    H: HostRequestHandler + ?Sized + 'static,
{
    let (response_tx, mut response_rx) =
        mpsc::channel::<Result<Vec<u8>, HostTransportError>>(MAX_IN_FLIGHT_REQUESTS);
    let mut writer_task = tokio::spawn(async move {
        while let Some(response) = response_rx.recv().await {
            let response = response?;
            write_frame(&mut writer, &response).await?;
        }
        Ok::<(), HostTransportError>(())
    });
    let permits = Arc::new(Semaphore::new(MAX_IN_FLIGHT_REQUESTS));
    let reader_response_tx = response_tx.clone();
    let reader_loop = async move {
        while let Some(frame) = read_frame(&mut reader).await? {
            let request: HostRequestEnvelope = serde_json::from_slice(&frame)?;
            let permit = Arc::clone(&permits)
                .acquire_owned()
                .await
                .map_err(|_| io::Error::other("IPC request limiter closed"))?;
            let handler = Arc::clone(&handler);
            let response_tx = reader_response_tx.clone();
            tokio::spawn(async move {
                let _permit = permit;
                let response = handler.handle_request(request).await;
                let encoded = encode_bounded_response(response, maximum_response_bytes);
                let _ = response_tx.send(encoded).await;
            });
        }
        Ok::<(), HostTransportError>(())
    };
    tokio::pin!(reader_loop);
    tokio::select! {
        result = &mut reader_loop => {
            result?;
            drop(response_tx);
            writer_task.await??;
            Ok(())
        }
        result = &mut writer_task => {
            match result? {
                Ok(()) => Err(HostTransportError::WriterStopped),
                Err(error) => Err(error),
            }
        }
    }
}

fn encode_bounded_response(
    response: HostResponseEnvelope,
    maximum_response_bytes: usize,
) -> Result<Vec<u8>, HostTransportError> {
    if let Some(encoded) = encode_json_with_limit(&response, maximum_response_bytes)? {
        return Ok(encoded);
    }
    let fallback = HostResponseEnvelope::failure(
        response.command_id,
        HostError::new(
            "response_too_large",
            "Local Agent Host response exceeded the IPC message limit; use a smaller page, reduced payload, or artifact reference.",
            true,
        ),
    );
    encode_json_with_limit(&fallback, maximum_response_bytes)?
        .ok_or(HostTransportError::FrameTooLarge)
}

fn encode_json_with_limit<T: serde::Serialize>(
    value: &T,
    maximum_bytes: usize,
) -> Result<Option<Vec<u8>>, HostTransportError> {
    struct LimitedWriter {
        bytes: Vec<u8>,
        maximum_bytes: usize,
        exceeded: bool,
    }

    impl io::Write for LimitedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if buffer.len() > self.maximum_bytes.saturating_sub(self.bytes.len()) {
                self.exceeded = true;
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "serialized IPC response exceeds the message limit",
                ));
            }
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    let mut writer = LimitedWriter {
        bytes: Vec::with_capacity(maximum_bytes.min(64 * 1024)),
        maximum_bytes,
        exceeded: false,
    };
    match serde_json::to_writer(&mut writer, value) {
        Ok(()) => Ok(Some(writer.bytes)),
        Err(_) if writer.exceeded => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub async fn read_frame<R>(reader: &mut R) -> Result<Option<Vec<u8>>, HostTransportError>
where
    R: AsyncRead + Unpin,
{
    let Some((length, has_more)) = read_frame_prefix(reader, true).await? else {
        return Ok(None);
    };
    let mut message = Vec::with_capacity(length);
    read_frame_body(reader, length, &mut message).await?;
    let mut has_more = has_more;
    while has_more {
        let (length, continuation) = read_frame_prefix(reader, false)
            .await?
            .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))?;
        if message.len().saturating_add(length) > LOCAL_AGENT_MAX_MESSAGE_BYTES {
            return Err(HostTransportError::FrameTooLarge);
        }
        read_frame_body(reader, length, &mut message).await?;
        has_more = continuation;
    }
    Ok(Some(message))
}

pub async fn write_frame<W>(writer: &mut W, body: &[u8]) -> Result<(), HostTransportError>
where
    W: AsyncWrite + Unpin,
{
    if body.is_empty() || body.len() > LOCAL_AGENT_MAX_MESSAGE_BYTES {
        return Err(HostTransportError::FrameTooLarge);
    }
    let mut chunks = body.chunks(LOCAL_AGENT_MAX_FRAME_BYTES).peekable();
    while let Some(chunk) = chunks.next() {
        let length = u32::try_from(chunk.len()).map_err(|_| HostTransportError::FrameTooLarge)?;
        let prefix = if chunks.peek().is_some() {
            length | FRAME_CONTINUATION_BIT
        } else {
            length
        };
        writer.write_all(&prefix.to_be_bytes()).await?;
        writer.write_all(chunk).await?;
    }
    writer.flush().await?;
    Ok(())
}

async fn read_frame_prefix<R>(
    reader: &mut R,
    allow_clean_eof: bool,
) -> Result<Option<(usize, bool)>, HostTransportError>
where
    R: AsyncRead + Unpin,
{
    let mut prefix = [0_u8; 4];
    let first = reader.read(&mut prefix[..1]).await?;
    if first == 0 {
        if allow_clean_eof {
            return Ok(None);
        }
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
    }
    reader.read_exact(&mut prefix[1..]).await?;
    let encoded = u32::from_be_bytes(prefix);
    let has_more = encoded & FRAME_CONTINUATION_BIT != 0;
    let length = (encoded & FRAME_LENGTH_MASK) as usize;
    if length == 0 || length > LOCAL_AGENT_MAX_FRAME_BYTES {
        return Err(HostTransportError::FrameTooLarge);
    }
    Ok(Some((length, has_more)))
}

async fn read_frame_body<R>(
    reader: &mut R,
    length: usize,
    message: &mut Vec<u8>,
) -> Result<(), HostTransportError>
where
    R: AsyncRead + Unpin,
{
    if message.len().saturating_add(length) > LOCAL_AGENT_MAX_MESSAGE_BYTES {
        return Err(HostTransportError::FrameTooLarge);
    }
    let start = message.len();
    message.resize(start + length, 0);
    reader.read_exact(&mut message[start..]).await?;
    Ok(())
}

#[cfg(unix)]
pub mod unix {
    use super::*;
    use std::{
        fs,
        os::unix::fs::{FileTypeExt, PermissionsExt},
        path::Path,
    };
    use tokio::net::UnixListener;

    pub async fn serve<H>(socket_path: &Path, handler: Arc<H>) -> Result<(), HostTransportError>
    where
        H: HostRequestHandler + ?Sized + 'static,
    {
        prepare_socket_path(socket_path)?;
        let listener = UnixListener::bind(socket_path)?;
        fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600))?;
        let result = accept_until_shutdown(listener, handler).await;
        let _ = fs::remove_file(socket_path);
        result
    }

    async fn accept_until_shutdown<H>(
        listener: UnixListener,
        handler: Arc<H>,
    ) -> Result<(), HostTransportError>
    where
        H: HostRequestHandler + ?Sized + 'static,
    {
        loop {
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    return Ok(());
                }
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    let handler = Arc::clone(&handler);
                    tokio::spawn(async move {
                        if let Err(error) = serve_stream(stream, handler).await {
                            eprintln!("Local Agent IPC connection ended: {error}");
                        }
                    });
                }
            }
        }
    }

    fn prepare_socket_path(socket_path: &Path) -> Result<(), HostTransportError> {
        if let Some(parent) = socket_path
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        match fs::symlink_metadata(socket_path) {
            Ok(metadata) if metadata.file_type().is_socket() => fs::remove_file(socket_path)?,
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "refusing to replace a non-socket IPC path",
                )
                .into())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }
}

#[cfg(windows)]
pub mod windows {
    use super::*;
    use std::{ffi::c_void, mem::size_of, ptr};
    use tokio::net::windows::named_pipe::ServerOptions;
    use windows_sys::Win32::{
        Foundation::{GetLastError, LocalFree},
        Security::{
            Authorization::{
                ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
            },
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
    };

    pub async fn serve<H>(pipe_name: &str, handler: Arc<H>) -> Result<(), HostTransportError>
    where
        H: HostRequestHandler + ?Sized + 'static,
    {
        validate_pipe_name(pipe_name)?;
        let mut first = true;
        loop {
            let server = create_owner_only_pipe(pipe_name, first)?;
            first = false;
            tokio::select! {
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    return Ok(());
                }
                connected = server.connect() => {
                    connected?;
                    let handler = Arc::clone(&handler);
                    tokio::spawn(async move {
                        if let Err(error) = serve_stream(server, handler).await {
                            eprintln!("Local Agent IPC connection ended: {error}");
                        }
                    });
                }
            }
        }
    }

    fn create_owner_only_pipe(
        pipe_name: &str,
        first_pipe_instance: bool,
    ) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, HostTransportError> {
        let security = OwnerOnlyPipeSecurity::new()?;
        let mut options = ServerOptions::new();
        options
            .first_pipe_instance(first_pipe_instance)
            .reject_remote_clients(true);
        // SAFETY: `security` owns both the SECURITY_ATTRIBUTES value and the
        // descriptor it references until CreateNamedPipeW returns.
        let server = unsafe {
            options.create_with_security_attributes_raw(pipe_name, security.attributes_ptr())?
        };
        Ok(server)
    }

    struct OwnerOnlyPipeSecurity {
        descriptor: PSECURITY_DESCRIPTOR,
        attributes: SECURITY_ATTRIBUTES,
    }

    impl OwnerOnlyPipeSecurity {
        fn new() -> Result<Self, HostTransportError> {
            // Protected DACL: LocalSystem, administrators, and the object owner
            // have full access. No Everyone or anonymous ACE is inherited.
            let sddl: Vec<u16> = "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;OW)"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let mut descriptor = ptr::null_mut();
            // SAFETY: `sddl` is NUL-terminated and descriptor points to a valid
            // out parameter. Windows allocates the returned descriptor.
            let converted = unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    sddl.as_ptr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    ptr::null_mut(),
                )
            };
            if converted == 0 {
                // SAFETY: GetLastError has no preconditions.
                let code = unsafe { GetLastError() };
                return Err(io::Error::from_raw_os_error(code as i32).into());
            }
            Ok(Self {
                descriptor,
                attributes: SECURITY_ATTRIBUTES {
                    nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                    lpSecurityDescriptor: descriptor,
                    bInheritHandle: 0,
                },
            })
        }

        fn attributes_ptr(&self) -> *mut c_void {
            ptr::from_ref(&self.attributes).cast_mut().cast()
        }
    }

    impl Drop for OwnerOnlyPipeSecurity {
        fn drop(&mut self) {
            // SAFETY: the descriptor was allocated by
            // ConvertStringSecurityDescriptorToSecurityDescriptorW and is
            // released exactly once here.
            unsafe {
                let _ = LocalFree(self.descriptor.cast());
            }
        }
    }

    fn validate_pipe_name(pipe_name: &str) -> Result<(), HostTransportError> {
        if !pipe_name.starts_with(r"\\.\pipe\chatos-local-agent-")
            || pipe_name.len() > 240
            || pipe_name.chars().any(char::is_control)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "pipe name must use the ChatOS Local Agent namespace",
            )
            .into());
        }
        Ok(())
    }
}

pub fn decode_response(frame: &[u8]) -> Result<HostResponseEnvelope, HostTransportError> {
    Ok(serde_json::from_slice(frame)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_client_storage::SqliteClientStorage;
    use chatos_local_agent_protocol::{
        HostCommand, HostRequestEnvelope, HostResult, LOCAL_AGENT_PROTOCOL_VERSION,
    };
    use chatos_local_agent_runtime::LocalAgentRuntime;

    struct TestRuntimeHandler(LocalAgentRuntime);

    #[async_trait::async_trait]
    impl HostRequestHandler for TestRuntimeHandler {
        async fn handle_request(&self, request: HostRequestEnvelope) -> HostResponseEnvelope {
            self.0.handle(request).await
        }
    }

    #[tokio::test]
    async fn framed_ipc_serves_health_over_multiple_messages() {
        let storage = Arc::new(
            SqliteClientStorage::connect_memory()
                .await
                .expect("storage"),
        );
        let runtime = LocalAgentRuntime::new(storage);
        runtime.initialize("user-1").await.expect("runtime");
        let handler = Arc::new(TestRuntimeHandler(runtime));
        let (mut client, server) = tokio::io::duplex(16 * 1024);
        let server_task = tokio::spawn(serve_stream(server, handler));

        for index in 0..2 {
            let request = HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: format!("health-{index}"),
                command: HostCommand::Health,
            };
            write_frame(&mut client, &serde_json::to_vec(&request).expect("request"))
                .await
                .expect("write");
            let response = read_frame(&mut client)
                .await
                .expect("read")
                .expect("response");
            let response = decode_response(&response).expect("decode");
            assert!(response.ok);
            assert!(matches!(response.result, Some(HostResult::Health { .. })));
        }
        drop(client);
        server_task.await.expect("join").expect("server");
    }

    #[tokio::test]
    async fn framed_ipc_does_not_let_a_slow_request_block_later_requests() {
        struct DelayedHealthHandler;

        #[async_trait::async_trait]
        impl HostRequestHandler for DelayedHealthHandler {
            async fn handle_request(&self, request: HostRequestEnvelope) -> HostResponseEnvelope {
                if request.command_id == "slow" {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
                HostResponseEnvelope::success(
                    request.command_id,
                    HostResult::Health {
                        service: "test".to_string(),
                        storage_ready: true,
                        recovered_claims: 0,
                        max_frame_bytes: LOCAL_AGENT_MAX_FRAME_BYTES,
                        max_message_bytes: LOCAL_AGENT_MAX_MESSAGE_BYTES,
                    },
                )
            }
        }

        let (mut client, server) = tokio::io::duplex(16 * 1024);
        let server_task = tokio::spawn(serve_stream(server, Arc::new(DelayedHealthHandler)));
        for command_id in ["slow", "fast"] {
            let request = HostRequestEnvelope {
                protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
                command_id: command_id.to_string(),
                command: HostCommand::Health,
            };
            write_frame(&mut client, &serde_json::to_vec(&request).expect("request"))
                .await
                .expect("write");
        }

        let first = read_frame(&mut client)
            .await
            .expect("read")
            .expect("response");
        assert_eq!(decode_response(&first).expect("decode").command_id, "fast");
        let second = read_frame(&mut client)
            .await
            .expect("read")
            .expect("response");
        assert_eq!(decode_response(&second).expect("decode").command_id, "slow");
        drop(client);
        server_task.await.expect("join").expect("server");
    }

    #[tokio::test]
    async fn oversized_frame_is_rejected_before_allocation() {
        let (mut client, mut server) = tokio::io::duplex(16);
        client
            .write_all(&((LOCAL_AGENT_MAX_FRAME_BYTES as u32) + 1).to_be_bytes())
            .await
            .expect("write prefix");
        assert!(matches!(
            read_frame(&mut server).await,
            Err(HostTransportError::FrameTooLarge)
        ));
    }

    #[tokio::test]
    async fn messages_larger_than_one_frame_are_chunked_and_reassembled() {
        let payload = vec![0x5a; LOCAL_AGENT_MAX_FRAME_BYTES + 257];
        let expected = payload.clone();
        let (mut writer, mut reader) = tokio::io::duplex(16 * 1024);
        let write_task = tokio::spawn(async move { write_frame(&mut writer, &payload).await });

        let received = read_frame(&mut reader)
            .await
            .expect("read chunked message")
            .expect("message");
        assert_eq!(received, expected);
        write_task
            .await
            .expect("join writer")
            .expect("write chunks");
    }

    #[tokio::test]
    async fn oversized_response_returns_an_error_without_stopping_transport() {
        struct OversizedResponseHandler;

        #[async_trait::async_trait]
        impl HostRequestHandler for OversizedResponseHandler {
            async fn handle_request(&self, request: HostRequestEnvelope) -> HostResponseEnvelope {
                let oversized = request.command_id == "oversized-response";
                HostResponseEnvelope::success(
                    request.command_id,
                    HostResult::Health {
                        service: if oversized {
                            "x".repeat(2_048)
                        } else {
                            "healthy".to_string()
                        },
                        storage_ready: true,
                        recovered_claims: 0,
                        max_frame_bytes: LOCAL_AGENT_MAX_FRAME_BYTES,
                        max_message_bytes: LOCAL_AGENT_MAX_MESSAGE_BYTES,
                    },
                )
            }
        }

        let (mut client, server) = tokio::io::duplex(16 * 1024);
        let (server_reader, server_writer) = tokio::io::split(server);
        let server_task = tokio::spawn(serve_reader_writer_with_response_limit(
            server_reader,
            server_writer,
            Arc::new(OversizedResponseHandler),
            1_024,
        ));
        let request = HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "oversized-response".to_string(),
            command: HostCommand::Health,
        };
        write_frame(&mut client, &serde_json::to_vec(&request).expect("request"))
            .await
            .expect("write");

        let response = read_frame(&mut client)
            .await
            .expect("read fallback")
            .expect("fallback response");
        let response = decode_response(&response).expect("decode fallback");
        assert!(!response.ok);
        assert_eq!(
            response.error.expect("oversize error").code,
            "response_too_large"
        );

        let health = HostRequestEnvelope {
            protocol_version: LOCAL_AGENT_PROTOCOL_VERSION,
            command_id: "health-after-oversized-response".to_string(),
            command: HostCommand::Health,
        };
        write_frame(&mut client, &serde_json::to_vec(&health).expect("request"))
            .await
            .expect("write health");
        let response = read_frame(&mut client)
            .await
            .expect("read health fallback")
            .expect("health fallback response");
        let response = decode_response(&response).expect("decode health fallback");
        assert!(response.ok);
        assert!(response.error.is_none());

        drop(client);
        server_task.await.expect("join").expect("server");
    }
}
