// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod error;
mod host_keys;
mod network;
mod output;
mod paths;
mod session;
mod types;

pub use error::{RemoteRuntimeError, RemoteRuntimeErrorKind};
pub use host_keys::{apply_host_key_policy, evaluate_host_key_policy, HostKeyPolicyDecision};
pub use network::{configure_stream_timeout, connect_tcp_stream};
pub use output::{read_stream_limited, BoundedReadError};
pub use paths::{join_remote_path, normalize_remote_path, remote_parent_path};
pub use session::{authenticate_private_key_file, establish_ssh_session, ssh_timeout_millis};
pub use types::{is_valid_ssh_port, HostKeyPolicy, SshAuthType, DEFAULT_SSH_PORT};
