// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::ptr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
};
use windows_sys::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

use crate::ipc_transport_common::{
    handle_framed_connection, DEFAULT_IO_TIMEOUT, DEFAULT_MAXIMUM_CONNECTIONS,
};
use crate::{LocalAgentIpcServer, DEFAULT_MAXIMUM_IPC_FRAME_BYTES};

const PIPE_PREFIX: &str = r"\\.\pipe\chatos-local-agent-";
const MINIMUM_OPAQUE_ID_BYTES: usize = 8;
const MAXIMUM_OPAQUE_ID_BYTES: usize = 128;
const SDDL_REVISION_1: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum WindowsLocalAgentIpcError {
    #[error("local Agent Windows pipe name must use the private ChatOS namespace")]
    InvalidPipeName,
    #[error("local Agent Windows IPC frame limit is invalid")]
    InvalidFrameLimit,
    #[error("local Agent Windows IPC timeout or connection limit is invalid")]
    InvalidTransportLimit,
    #[error("local Agent Windows IPC operation failed: {0}")]
    Io(#[from] io::Error),
}

pub struct WindowsLocalAgentIpcTransport {
    pipe_name: String,
    server: Arc<LocalAgentIpcServer>,
    expected_user_sid: SidBuffer,
    first_instance: Option<NamedPipeServer>,
    maximum_frame_bytes: usize,
    io_timeout: Duration,
    maximum_connections: usize,
}

impl WindowsLocalAgentIpcTransport {
    pub fn bind(
        pipe_name: impl Into<String>,
        server: Arc<LocalAgentIpcServer>,
    ) -> Result<Self, WindowsLocalAgentIpcError> {
        Self::bind_with_limits(
            pipe_name,
            server,
            DEFAULT_MAXIMUM_IPC_FRAME_BYTES,
            DEFAULT_IO_TIMEOUT,
            DEFAULT_MAXIMUM_CONNECTIONS,
        )
    }

    pub fn bind_with_limits(
        pipe_name: impl Into<String>,
        server: Arc<LocalAgentIpcServer>,
        maximum_frame_bytes: usize,
        io_timeout: Duration,
        maximum_connections: usize,
    ) -> Result<Self, WindowsLocalAgentIpcError> {
        let pipe_name = pipe_name.into();
        validate_pipe_name(pipe_name.as_str())?;
        if maximum_frame_bytes == 0 || maximum_frame_bytes > u32::MAX as usize {
            return Err(WindowsLocalAgentIpcError::InvalidFrameLimit);
        }
        if io_timeout.is_zero() || !(1..=254).contains(&maximum_connections) {
            return Err(WindowsLocalAgentIpcError::InvalidTransportLimit);
        }
        let expected_user_sid = current_process_user_sid()?;
        let first_instance = create_pipe_instance(
            pipe_name.as_str(),
            &expected_user_sid,
            maximum_connections,
            true,
        )?;
        Ok(Self {
            pipe_name,
            server,
            expected_user_sid,
            first_instance: Some(first_instance),
            maximum_frame_bytes,
            io_timeout,
            maximum_connections,
        })
    }

    pub async fn serve(
        mut self,
        cancellation: CancellationToken,
    ) -> Result<(), WindowsLocalAgentIpcError> {
        let mut pending = self.first_instance.take().ok_or_else(|| {
            WindowsLocalAgentIpcError::Io(io::Error::other(
                "Windows IPC transport has no initial pipe instance",
            ))
        })?;
        let mut connections = JoinSet::new();

        loop {
            if connections.len() >= self.maximum_connections {
                tokio::select! {
                    _ = cancellation.cancelled() => break,
                    Some(joined) = connections.join_next() => check_connection_task(joined)?,
                }
                continue;
            }
            tokio::select! {
                _ = cancellation.cancelled() => break,
                Some(joined) = connections.join_next(), if !connections.is_empty() => {
                    check_connection_task(joined)?;
                }
                connected = pending.connect() => {
                    connected?;
                    let connected_pipe = pending;
                    pending = create_pipe_instance(
                        self.pipe_name.as_str(),
                        &self.expected_user_sid,
                        self.maximum_connections,
                        false,
                    )?;
                    if !client_has_expected_user(&connected_pipe, &self.expected_user_sid)
                        .unwrap_or(false)
                    {
                        let _ = connected_pipe.disconnect();
                        continue;
                    }
                    let server = self.server.clone();
                    let maximum_frame_bytes = self.maximum_frame_bytes;
                    let io_timeout = self.io_timeout;
                    connections.spawn(async move {
                        handle_framed_connection(
                            connected_pipe,
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

    pub fn pipe_name(&self) -> &str {
        self.pipe_name.as_str()
    }
}

fn check_connection_task(
    joined: Result<Result<(), io::Error>, tokio::task::JoinError>,
) -> Result<(), WindowsLocalAgentIpcError> {
    if let Err(error) = joined {
        if !error.is_cancelled() {
            return Err(WindowsLocalAgentIpcError::Io(io::Error::other(format!(
                "Windows IPC connection task failed: {error}"
            ))));
        }
    }
    Ok(())
}

fn validate_pipe_name(pipe_name: &str) -> Result<(), WindowsLocalAgentIpcError> {
    let Some(opaque_id) = pipe_name.strip_prefix(PIPE_PREFIX) else {
        return Err(WindowsLocalAgentIpcError::InvalidPipeName);
    };
    if !(MINIMUM_OPAQUE_ID_BYTES..=MAXIMUM_OPAQUE_ID_BYTES).contains(&opaque_id.len())
        || !opaque_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(WindowsLocalAgentIpcError::InvalidPipeName);
    }
    Ok(())
}

fn create_pipe_instance(
    pipe_name: &str,
    expected_user_sid: &SidBuffer,
    maximum_connections: usize,
    first_instance: bool,
) -> Result<NamedPipeServer, io::Error> {
    let security_descriptor = PrivateSecurityDescriptor::new(expected_user_sid)?;
    let mut attributes = SECURITY_ATTRIBUTES {
        nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: security_descriptor.as_ptr(),
        bInheritHandle: 0,
    };
    let mut options = ServerOptions::new();
    options
        .first_pipe_instance(first_instance)
        .reject_remote_clients(true)
        .max_instances(maximum_connections);
    // SAFETY: `attributes` and its owned security descriptor remain alive for the
    // complete synchronous CreateNamedPipeW call and inheritance is disabled.
    unsafe {
        options.create_with_security_attributes_raw(
            pipe_name,
            ptr::from_mut(&mut attributes).cast::<c_void>(),
        )
    }
}

fn current_process_user_sid() -> Result<SidBuffer, io::Error> {
    let mut token = ptr::null_mut();
    // SAFETY: GetCurrentProcess returns a valid pseudo handle and `token` is an
    // initialized out pointer. The returned real token handle is owned below.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = OwnedHandle(token);
    SidBuffer::from_token(token.0)
}

fn client_has_expected_user(
    pipe: &NamedPipeServer,
    expected_user_sid: &SidBuffer,
) -> Result<bool, io::Error> {
    let mut client_process_id = 0;
    // SAFETY: the Tokio server owns a valid connected pipe handle and the PID
    // out pointer is valid for writes.
    if unsafe {
        GetNamedPipeClientProcessId(pipe.as_raw_handle() as HANDLE, &mut client_process_id)
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the PID comes directly from the connected pipe. No handle is
    // inherited, and the returned process handle is closed by OwnedHandle.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, client_process_id) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = OwnedHandle(process);
    let mut token = ptr::null_mut();
    // SAFETY: `process` is a live process handle and `token` is an initialized
    // out pointer. The returned token is closed by OwnedHandle.
    if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = OwnedHandle(token);
    let client_sid = SidBuffer::from_token(token.0)?;
    // SAFETY: both SID pointers refer to validated TOKEN_USER buffers that stay
    // alive for the duration of EqualSid.
    Ok(unsafe { EqualSid(expected_user_sid.as_sid(), client_sid.as_sid()) } != 0)
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: this wrapper exclusively owns the real Windows handle.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

struct SidBuffer {
    words: Vec<usize>,
}

impl SidBuffer {
    fn from_token(token: HANDLE) -> Result<Self, io::Error> {
        let mut required_bytes = 0;
        // SAFETY: the null-buffer probe is the documented way to obtain the
        // TOKEN_USER buffer size.
        unsafe {
            GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut required_bytes);
        }
        if required_bytes < size_of::<TOKEN_USER>() as u32 {
            return Err(io::Error::last_os_error());
        }
        let word_bytes = size_of::<usize>();
        let word_count = (required_bytes as usize).div_ceil(word_bytes);
        let mut words = vec![0usize; word_count];
        // SAFETY: the word buffer is suitably aligned for TOKEN_USER and has at
        // least `required_bytes` writable bytes.
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                words.as_mut_ptr().cast::<c_void>(),
                required_bytes,
                &mut required_bytes,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        let buffer = Self { words };
        if buffer.as_sid().is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Windows token did not contain a user SID",
            ));
        }
        Ok(buffer)
    }

    fn as_sid(&self) -> *mut c_void {
        // SAFETY: `words` was populated by GetTokenInformation(TokenUser), is
        // aligned for TOKEN_USER, and is kept alive by `self`.
        unsafe { (*(self.words.as_ptr().cast::<TOKEN_USER>())).User.Sid }
    }
}

struct PrivateSecurityDescriptor(*mut c_void);

impl PrivateSecurityDescriptor {
    fn new(user_sid: &SidBuffer) -> Result<Self, io::Error> {
        let user_sid = sid_to_string(user_sid.as_sid())?;
        let sddl = format!("D:P(A;;GA;;;SY)(A;;GA;;;{user_sid})");
        let wide_sddl: Vec<u16> = sddl.encode_utf16().chain(Some(0)).collect();
        let mut descriptor = ptr::null_mut();
        // SAFETY: `wide_sddl` is NUL terminated and `descriptor` is a valid out
        // pointer. Windows allocates the returned descriptor with LocalAlloc.
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                wide_sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(descriptor))
    }

    fn as_ptr(&self) -> *mut c_void {
        self.0
    }
}

impl Drop for PrivateSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: ConvertStringSecurityDescriptor allocated this pointer
            // with LocalAlloc and this wrapper owns it exclusively.
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

fn sid_to_string(sid: *mut c_void) -> Result<String, io::Error> {
    let mut wide_sid = ptr::null_mut();
    // SAFETY: `sid` points into a live validated TOKEN_USER buffer and the out
    // pointer receives a LocalAlloc-owned NUL-terminated UTF-16 string.
    if unsafe { ConvertSidToStringSidW(sid, &mut wide_sid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut length = 0;
    // SAFETY: ConvertSidToStringSidW guarantees a NUL-terminated string.
    unsafe {
        while *wide_sid.add(length) != 0 {
            length += 1;
        }
    }
    // SAFETY: the preceding loop established the initialized string length.
    let value = String::from_utf16(unsafe { std::slice::from_raw_parts(wide_sid, length) })
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Windows SID is not UTF-16"));
    // SAFETY: this string was allocated by ConvertSidToStringSidW using
    // LocalAlloc and has not otherwise been freed.
    unsafe {
        LocalFree(wide_sid.cast::<c_void>());
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_accepts_names_in_the_private_chatos_namespace() {
        assert!(validate_pipe_name(r"\\.\pipe\chatos-local-agent-7bb214f0").is_ok());
        assert!(validate_pipe_name(r"\\server\pipe\chatos-local-agent-7bb214f0").is_err());
        assert!(validate_pipe_name(r"\\.\pipe\other-7bb214f0").is_err());
        assert!(validate_pipe_name(r"\\.\pipe\chatos-local-agent-bad\name").is_err());
    }
}
