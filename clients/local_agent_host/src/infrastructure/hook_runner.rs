// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use async_trait::async_trait;
use chatos_local_agent_protocol::{LocalPluginHookBundleSpec, LocalPluginHookDefinition};
#[cfg(any(target_os = "macos", windows))]
use sha2::{Digest, Sha256};
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::{
    collections::BTreeMap,
    path::Path,
    process::Stdio,
    time::{Duration, Instant},
};
#[cfg(windows)]
use std::{
    collections::BTreeSet,
    ffi::OsString,
    io::{Read, Write},
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
#[cfg(target_os = "macos")]
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::Command,
    time::timeout,
};

#[cfg(target_os = "macos")]
const SANDBOX_EXECUTABLE: &str = "/usr/bin/sandbox-exec";
#[cfg(target_os = "macos")]
const SHELL_EXECUTABLE: &str = "/bin/sh";
#[cfg(target_os = "macos")]
const MAXIMUM_STDOUT_BYTES: usize = 512 * 1_024;
#[cfg(target_os = "macos")]
const MAXIMUM_STDERR_BYTES: usize = 64 * 1_024;
#[cfg(target_os = "macos")]
const TERMINATION_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub struct LocalHookInvocation {
    pub bundle: LocalPluginHookBundleSpec,
    pub hook: LocalPluginHookDefinition,
    pub stdin_json: Vec<u8>,
    pub workspace_root: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalHookInvocationResult {
    pub exit_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub stdout_overflow: bool,
    pub stderr_overflow: bool,
    pub timed_out: bool,
    pub duration_ms: u64,
}

#[async_trait]
pub trait LocalHookRunner: Send + Sync {
    async fn run(
        &self,
        invocation: LocalHookInvocation,
    ) -> Result<LocalHookInvocationResult, String>;
}

include!("hook_runner/windows.rs");
include!("hook_runner/macos_runner.rs");
include!("hook_runner/macos_support.rs");

#[cfg(all(test, windows))]
mod windows_tests;

#[cfg(all(test, target_os = "macos"))]
mod tests;
