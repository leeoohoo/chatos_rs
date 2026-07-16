// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chatos_mcp_service::METHOD_PING;
use chatos_sandbox_contract::{EffectivePermissionSnapshot, EffectiveSandboxPolicy};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;

use crate::sandbox::types::{LocalSandboxResourceLimits, LocalSandboxRuntime};

mod launcher;
mod readiness;

use launcher::{native_sandbox_command, NativeLauncherSpec};
pub(crate) use readiness::native_process_sandbox_capability;
use readiness::native_sandbox_agent_executable;

const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct NativeSandboxProcess {
    pid: u32,
    state_root: PathBuf,
    child: Mutex<Child>,
    io: Mutex<NativeProcessIo>,
    stderr_tail: Arc<Mutex<VecDeque<String>>>,
}

struct NativeProcessIo {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

pub(crate) async fn start_native_sandbox_process(
    runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
    workspace: &Path,
    policy: &EffectiveSandboxPolicy,
    effective_permissions: &EffectivePermissionSnapshot,
    resource_limits: &LocalSandboxResourceLimits,
    project_id: &str,
    user_id: &str,
) -> Result<String> {
    let workspace = workspace
        .canonicalize()
        .context("canonicalize native sandbox workspace")?;
    let additional_writable_roots = canonical_writable_roots(&policy.additional_writable_roots)?;
    let agent = native_sandbox_agent_executable().map_err(anyhow::Error::msg)?;
    let state_root = create_native_state_root(sandbox_id)?;
    let home = state_root.join("home");
    let temp = state_root.join("tmp");
    let agent_state = state_root.join("agent");
    for directory in [&home, &temp, &agent_state] {
        std::fs::create_dir_all(directory)
            .with_context(|| format!("create native sandbox directory {}", directory.display()))?;
    }

    let disk_limit_bytes = resource_limits.disk_mb.saturating_mul(1024 * 1024);
    let extra_quota_roots = std::env::join_paths([state_root.as_path()])
        .context("encode native sandbox quota roots")?;
    let mut environment = BTreeMap::new();
    environment.insert("CHATOS_SANDBOX_TRANSPORT".to_string(), "stdio".to_string());
    environment.insert(
        "CHATOS_SANDBOX_COMMAND_BACKEND".to_string(),
        "native".to_string(),
    );
    environment.insert(
        "CHATOS_SANDBOX_PROCESS_GROUP_OWNED".to_string(),
        "1".to_string(),
    );
    environment.insert("CHATOS_SANDBOX_ID".to_string(), sandbox_id.to_string());
    environment.insert(
        "CHATOS_SANDBOX_PERMISSION_PROFILE".to_string(),
        policy.permission_profile_id.as_str().to_string(),
    );
    environment.insert(
        "CHATOS_SANDBOX_EFFECTIVE_PERMISSIONS_JSON".to_string(),
        serde_json::to_string(effective_permissions)
            .context("encode native sandbox effective permissions")?,
    );
    if !additional_writable_roots.is_empty() {
        let encoded = std::env::join_paths(additional_writable_roots.iter())
            .context("encode native sandbox writable roots")?;
        environment.insert(
            "CHATOS_SANDBOX_ADDITIONAL_WRITABLE_ROOTS".to_string(),
            encoded.to_string_lossy().to_string(),
        );
    }
    if let Some(host_home) = std::env::var_os("HOME").filter(|value| !value.is_empty()) {
        environment.insert(
            "CHATOS_SANDBOX_HOST_HOME".to_string(),
            host_home.to_string_lossy().to_string(),
        );
    }
    environment.insert(
        "CHATOS_SANDBOX_DISK_LIMIT_BYTES".to_string(),
        disk_limit_bytes.to_string(),
    );
    environment.insert(
        "CHATOS_SANDBOX_EXTRA_QUOTA_ROOTS".to_string(),
        extra_quota_roots.to_string_lossy().to_string(),
    );
    environment.insert(
        "CHATOS_SANDBOX_STATE_DIR".to_string(),
        agent_state.to_string_lossy().to_string(),
    );
    environment.insert(
        "CHATOS_WORKSPACE".to_string(),
        workspace.to_string_lossy().to_string(),
    );
    environment.insert("CHATOS_PROJECT_ID".to_string(), project_id.to_string());
    environment.insert("CHATOS_USER_ID".to_string(), user_id.to_string());
    environment.insert("RUST_LOG".to_string(), "warn".to_string());

    let mut command = native_sandbox_command(NativeLauncherSpec {
        agent: agent.as_path(),
        workspace: workspace.as_path(),
        home: home.as_path(),
        temp: temp.as_path(),
        resource_limits,
        environment,
    })?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            let _ = std::fs::remove_dir_all(&state_root);
            return Err(err).context("start native sandbox process");
        }
    };
    let Some(pid) = child.id() else {
        let _ = child.kill().await;
        let _ = child.wait().await;
        let _ = std::fs::remove_dir_all(&state_root);
        return Err(anyhow!(
            "native sandbox process did not expose a process id"
        ));
    };
    let Some(stdin) = child.stdin.take() else {
        terminate_unregistered_child(&mut child, pid).await;
        let _ = std::fs::remove_dir_all(&state_root);
        return Err(anyhow!("native sandbox process stdin is unavailable"));
    };
    let Some(stdout) = child.stdout.take() else {
        terminate_unregistered_child(&mut child, pid).await;
        let _ = std::fs::remove_dir_all(&state_root);
        return Err(anyhow!("native sandbox process stdout is unavailable"));
    };
    let stderr = child.stderr.take();
    let stderr_tail = Arc::new(Mutex::new(VecDeque::new()));
    if let Some(stderr) = stderr {
        collect_stderr(stderr, stderr_tail.clone());
    }
    let process = Arc::new(NativeSandboxProcess {
        pid,
        state_root,
        child: Mutex::new(child),
        io: Mutex::new(NativeProcessIo {
            stdin,
            stdout: BufReader::new(stdout),
        }),
        stderr_tail,
    });

    match tokio::time::timeout(STARTUP_TIMEOUT, process.ping()).await {
        Ok(Ok(())) => {}
        Ok(Err(err)) => {
            process.terminate().await;
            return Err(err).context("native sandbox agent startup failed");
        }
        Err(_) => {
            process.terminate().await;
            return Err(anyhow!("native sandbox agent startup timed out"));
        }
    }

    runtime
        .processes
        .write()
        .await
        .insert(sandbox_id.to_string(), process);
    Ok(pid.to_string())
}

pub(crate) async fn call_native_sandbox_mcp(
    runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
    request: &Value,
) -> Result<Value> {
    let process = require_process(runtime, sandbox_id).await?;
    process.call(request, MCP_CALL_TIMEOUT).await
}

pub(crate) async fn native_sandbox_process_alive(
    runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> bool {
    let Ok(process) = require_process(runtime, sandbox_id).await else {
        return false;
    };
    process.is_alive().await
}

pub(crate) async fn native_sandbox_agent_alive(
    runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> bool {
    let Ok(process) = require_process(runtime, sandbox_id).await else {
        return false;
    };
    tokio::time::timeout(Duration::from_secs(5), process.ping())
        .await
        .is_ok_and(|result| result.is_ok())
}

pub(crate) async fn destroy_native_sandbox_process(
    runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<()> {
    let process = runtime.processes.write().await.remove(sandbox_id);
    if let Some(process) = process {
        process.terminate().await;
    }
    Ok(())
}

async fn require_process(
    runtime: &LocalSandboxRuntime,
    sandbox_id: &str,
) -> Result<Arc<NativeSandboxProcess>> {
    runtime
        .processes
        .read()
        .await
        .get(sandbox_id)
        .cloned()
        .ok_or_else(|| anyhow!("native sandbox process not found"))
}

impl NativeSandboxProcess {
    async fn ping(&self) -> Result<()> {
        let response = self
            .call(
                &json!({
                    "jsonrpc": "2.0",
                    "id": format!("ping-{}", uuid::Uuid::new_v4()),
                    "method": METHOD_PING,
                    "params": {},
                }),
                Duration::from_secs(5),
            )
            .await?;
        if response.get("error").is_some() {
            return Err(anyhow!(
                "native sandbox agent ping returned an error: {response}"
            ));
        }
        Ok(())
    }

    async fn call(&self, request: &Value, timeout: Duration) -> Result<Value> {
        if !self.is_alive().await {
            return Err(anyhow!(
                "native sandbox process exited: {}",
                self.stderr_summary().await
            ));
        }
        let mut encoded =
            serde_json::to_vec(request).context("encode native sandbox MCP request")?;
        encoded.push(b'\n');
        let operation = async {
            let mut io = self.io.lock().await;
            io.stdin
                .write_all(&encoded)
                .await
                .context("write native sandbox MCP request")?;
            io.stdin
                .flush()
                .await
                .context("flush native sandbox MCP request")?;
            let mut line = String::new();
            let count = io
                .stdout
                .read_line(&mut line)
                .await
                .context("read native sandbox MCP response")?;
            if count == 0 {
                return Err(anyhow!("native sandbox MCP agent closed stdout"));
            }
            serde_json::from_str::<Value>(&line).context("decode native sandbox MCP response")
        };
        match tokio::time::timeout(timeout, operation).await {
            Ok(result) => result,
            Err(_) => {
                self.terminate().await;
                Err(anyhow!("native sandbox MCP request timed out"))
            }
        }
    }

    async fn is_alive(&self) -> bool {
        self.child
            .lock()
            .await
            .try_wait()
            .is_ok_and(|status| status.is_none())
    }

    async fn terminate(&self) {
        let mut child = self.child.lock().await;
        let running = match child.try_wait() {
            Ok(Some(_)) => false,
            Ok(None) | Err(_) => true,
        };
        if running {
            #[cfg(unix)]
            unsafe {
                libc::kill(-(self.pid as i32), libc::SIGKILL);
            }
            let _ = child.kill().await;
        }
        let _ = child.wait().await;
        let _ = std::fs::remove_dir_all(&self.state_root);
    }

    async fn stderr_summary(&self) -> String {
        let lines = self.stderr_tail.lock().await;
        if lines.is_empty() {
            "no stderr output".to_string()
        } else {
            lines.iter().cloned().collect::<Vec<_>>().join(" | ")
        }
    }
}

async fn terminate_unregistered_child(child: &mut Child, pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn collect_stderr(stderr: tokio::process::ChildStderr, tail: Arc<Mutex<VecDeque<String>>>) {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let mut tail = tail.lock().await;
            if tail.len() >= 20 {
                tail.pop_front();
            }
            tail.push_back(line.chars().take(1_000).collect());
        }
    });
}

fn create_native_state_root(sandbox_id: &str) -> Result<PathBuf> {
    let parent = std::env::temp_dir().join("chatos-native-sandboxes");
    std::fs::create_dir_all(&parent).context("create native sandbox state parent")?;
    let root = parent.join(sandbox_id);
    std::fs::create_dir(&root).with_context(|| {
        format!(
            "create unique native sandbox state directory {}",
            root.display()
        )
    })?;
    root.canonicalize()
        .context("canonicalize native sandbox state directory")
}

fn canonical_writable_roots(roots: &[String]) -> Result<Vec<PathBuf>> {
    roots
        .iter()
        .map(|root| {
            let root = Path::new(root);
            if !root.is_absolute() {
                return Err(anyhow!(
                    "native sandbox additional writable root must be absolute: {}",
                    root.display()
                ));
            }
            let root = root.canonicalize().with_context(|| {
                format!(
                    "canonicalize native sandbox additional writable root {}",
                    root.display()
                )
            })?;
            if !root.is_dir() {
                return Err(anyhow!(
                    "native sandbox additional writable root is not a directory: {}",
                    root.display()
                ));
            }
            Ok(root)
        })
        .collect()
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        legacy_policy_permission_snapshot, ApprovalPolicy, ApprovalReviewer,
        NetworkDomainPermission, NetworkPermissionPolicy, NetworkRequirements, PermissionProfileId,
        SandboxBackendKind,
    };
    use std::collections::BTreeMap;

    fn rpc(id: &str, method: &str, params: Value) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        })
    }

    #[tokio::test]
    #[ignore = "requires `cargo build -p chatos_sandbox_mcp_server` before this test"]
    async fn native_agent_enforces_seatbelt_network_filesystem_and_process_tree() {
        native_sandbox_agent_executable().expect("sandbox agent binary");
        let root =
            std::env::temp_dir().join(format!("chatos-native-agent-test-{}", uuid::Uuid::new_v4()));
        let workspace = root.join("workspace");
        let outside = root.join("outside");
        std::fs::create_dir_all(workspace.join(".git")).expect("workspace");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(outside.join("secret.txt"), "outside-secret").expect("outside secret");
        std::os::unix::fs::symlink(&outside, workspace.join("escape")).expect("escape symlink");

        let runtime = LocalSandboxRuntime::default();
        let sandbox_id = format!("sandbox-test-{}", uuid::Uuid::new_v4());
        let policy = EffectiveSandboxPolicy {
            sandbox_mode: SandboxBackendKind::LocalProcess,
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            approval_policy: ApprovalPolicy::OnRequest,
            approval_reviewer: ApprovalReviewer::User,
            policy_revision: None,
            additional_writable_roots: Vec::new(),
        };
        let limits = LocalSandboxResourceLimits {
            cpu: 1.0,
            memory_mb: 512,
            disk_mb: 128,
            max_processes: 32,
        };
        let effective_permissions = legacy_policy_permission_snapshot(
            &policy,
            vec![workspace.to_string_lossy().to_string()],
        );
        start_native_sandbox_process(
            &runtime,
            sandbox_id.as_str(),
            workspace.as_path(),
            &policy,
            &effective_permissions,
            &limits,
            "project-test",
            "user-test",
        )
        .await
        .expect("start native sandbox");

        let tools = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc("tools", "tools/list", json!({})),
        )
        .await
        .expect("list tools");
        assert!(tools.to_string().contains("execute_command"));

        let symlink_read = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "symlink-read",
                "tools/call",
                json!({
                    "name": "read_file",
                    "arguments": { "path": "escape/secret.txt" },
                }),
            ),
        )
        .await
        .expect("symlink read response");
        assert!(
            !symlink_read.to_string().contains("outside-secret"),
            "workspace-scoped file tool followed a symlink outside the workspace: {symlink_read}"
        );

        let command = format!(
            "touch '{}' && ! touch '{}' && ! touch '{}'",
            workspace.join("inside").display(),
            outside.join("blocked").display(),
            workspace.join(".git/blocked").display()
        );
        let response = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "filesystem",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": { "path": ".", "command": command },
                }),
            ),
        )
        .await
        .expect("filesystem probe");
        assert!(response.get("error").is_none(), "{response}");
        assert!(workspace.join("inside").exists(), "{response}");
        assert!(!outside.join("blocked").exists());
        assert!(!workspace.join(".git/blocked").exists());

        let outside_permission = json!({
            "fileSystem": {
                "entries": [{
                    "access": "write",
                    "path": {
                        "type": "path",
                        "path": outside.to_string_lossy()
                    }
                }]
            }
        });
        let elevated_response = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "filesystem-elevated",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!("touch '{}'", outside.join("allowed-once").display()),
                        "additionalPermissions": outside_permission,
                        "_grantedPermissions": outside_permission,
                    },
                }),
            ),
        )
        .await
        .expect("elevated filesystem probe");
        assert!(
            elevated_response.get("error").is_none(),
            "{elevated_response}"
        );
        assert!(outside.join("allowed-once").exists());

        let post_elevation_response = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "filesystem-post-elevation",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!("! touch '{}'", outside.join("must-stay-blocked").display()),
                    },
                }),
            ),
        )
        .await
        .expect("post elevation filesystem probe");
        assert!(
            post_elevation_response.get("error").is_none(),
            "{post_elevation_response}"
        );
        assert!(!outside.join("must-stay-blocked").exists());

        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        listener.set_nonblocking(true).expect("nonblocking");
        let port = listener.local_addr().expect("listener address").port();
        let response = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "network",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!("if /usr/bin/nc -z 127.0.0.1 {port}; then exit 42; else exit 0; fi")
                    },
                }),
            ),
        )
        .await
        .expect("network probe");
        assert!(response.get("error").is_none(), "{response}");
        assert!(
            listener.accept().is_err(),
            "sandbox unexpectedly reached loopback"
        );

        let network_permission = json!({ "network": { "enabled": true } });
        let response = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "network-elevated",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!("/usr/bin/nc -z 127.0.0.1 {port}"),
                        "additionalPermissions": network_permission,
                        "_grantedPermissions": network_permission,
                    },
                }),
            ),
        )
        .await
        .expect("elevated network probe");
        assert!(response.get("error").is_none(), "{response}");
        assert!(
            listener.accept().is_ok(),
            "approved command did not receive network access"
        );

        let child_pid_path = workspace.join("child.pid");
        let child_survival_marker = workspace.join("child-survived");
        call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "background",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!(
                            "(sleep 2; touch '{}') & echo $! > '{}'; wait",
                            child_survival_marker.display(),
                            child_pid_path.display()
                        ),
                        "background": true
                    },
                }),
            ),
        )
        .await
        .expect("background process");
        for _ in 0..50 {
            if child_pid_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let child_pid: i32 = std::fs::read_to_string(&child_pid_path)
            .expect("child pid file")
            .trim()
            .parse()
            .expect("child pid");
        #[cfg(target_os = "linux")]
        let _ = child_pid;
        #[cfg(target_os = "macos")]
        assert_eq!(unsafe { libc::kill(child_pid, 0) }, 0);

        destroy_native_sandbox_process(&runtime, sandbox_id.as_str())
            .await
            .expect("destroy native sandbox");
        #[cfg(target_os = "macos")]
        {
            for _ in 0..50 {
                if unsafe { libc::kill(child_pid, 0) } == -1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert_eq!(unsafe { libc::kill(child_pid, 0) }, -1);
        }
        tokio::time::sleep(Duration::from_millis(2_300)).await;
        assert!(
            !child_survival_marker.exists(),
            "descendant survived normal sandbox release"
        );

        let crash_runtime = LocalSandboxRuntime::default();
        let crash_sandbox_id = format!("sandbox-crash-test-{}", uuid::Uuid::new_v4());
        start_native_sandbox_process(
            &crash_runtime,
            crash_sandbox_id.as_str(),
            workspace.as_path(),
            &policy,
            &effective_permissions,
            &limits,
            "project-test",
            "user-test",
        )
        .await
        .expect("start crash sandbox");
        let crash_child_pid_path = workspace.join("crash-child.pid");
        let crash_survival_marker = workspace.join("crash-child-survived");
        call_native_sandbox_mcp(
            &crash_runtime,
            crash_sandbox_id.as_str(),
            &rpc(
                "crash-background",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!(
                            "(sleep 2; touch '{}') & echo $! > '{}'; wait",
                            crash_survival_marker.display(),
                            crash_child_pid_path.display()
                        ),
                        "background": true
                    },
                }),
            ),
        )
        .await
        .expect("crash background process");
        for _ in 0..50 {
            if crash_child_pid_path.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let crash_child_pid: i32 = std::fs::read_to_string(&crash_child_pid_path)
            .expect("crash child pid file")
            .trim()
            .parse()
            .expect("crash child pid");
        #[cfg(target_os = "linux")]
        let _ = crash_child_pid;
        let crash_state_root = crash_runtime
            .processes
            .read()
            .await
            .get(crash_sandbox_id.as_str())
            .expect("crash process")
            .state_root
            .clone();
        drop(crash_runtime);
        #[cfg(target_os = "macos")]
        {
            for _ in 0..100 {
                if unsafe { libc::kill(crash_child_pid, 0) } == -1 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert_eq!(unsafe { libc::kill(crash_child_pid, 0) }, -1);
        }
        tokio::time::sleep(Duration::from_millis(2_300)).await;
        assert!(
            !crash_survival_marker.exists(),
            "descendant survived broker stdio/process-group teardown"
        );
        let _ = std::fs::remove_dir_all(crash_state_root);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    #[ignore = "requires `cargo build --release -p chatos_sandbox_mcp_server` before this test"]
    async fn native_agent_applies_effective_domain_network_snapshot() {
        native_sandbox_agent_executable().expect("sandbox agent binary");
        let root = std::env::temp_dir().join(format!(
            "chatos-native-network-agent-test-{}",
            uuid::Uuid::new_v4()
        ));
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workspace).expect("workspace");

        let policy = EffectiveSandboxPolicy {
            sandbox_mode: SandboxBackendKind::LocalProcess,
            permission_profile_id: PermissionProfileId::WorkspaceWrite,
            approval_policy: ApprovalPolicy::OnRequest,
            approval_reviewer: ApprovalReviewer::User,
            policy_revision: Some("network-test".to_string()),
            additional_writable_roots: Vec::new(),
        };
        let mut effective_permissions = legacy_policy_permission_snapshot(
            &policy,
            vec![workspace.to_string_lossy().to_string()],
        );
        effective_permissions.network = NetworkPermissionPolicy::Restricted {
            requirements: NetworkRequirements {
                enabled: Some(true),
                domains: Some(BTreeMap::from([(
                    "127.0.0.1".to_string(),
                    NetworkDomainPermission::Allow,
                )])),
                enable_socks5: Some(false),
                ..Default::default()
            },
        };

        let allowed_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        let allowed_port = allowed_listener.local_addr().expect("address").port();
        let allowed_server = std::thread::spawn(move || {
            let (mut stream, _) = allowed_listener.accept().expect("accept proxied request");
            use std::io::{Read, Write};
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).expect("read request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nproxied",
                )
                .expect("write response");
        });

        let bypass_listener = std::net::TcpListener::bind("127.0.0.1:0").expect("listener");
        bypass_listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let bypass_port = bypass_listener.local_addr().expect("address").port();

        let runtime = LocalSandboxRuntime::default();
        let sandbox_id = format!("sandbox-network-test-{}", uuid::Uuid::new_v4());
        start_native_sandbox_process(
            &runtime,
            sandbox_id.as_str(),
            workspace.as_path(),
            &policy,
            &effective_permissions,
            &LocalSandboxResourceLimits {
                cpu: 1.0,
                memory_mb: 512,
                disk_mb: 128,
                max_processes: 32,
            },
            "project-test",
            "user-test",
        )
        .await
        .expect("start native sandbox");

        let proxied = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "network-proxy",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!(
                            "/usr/bin/curl --silent --show-error --max-time 3 http://127.0.0.1:{allowed_port}/"
                        )
                    },
                }),
            ),
        )
        .await
        .expect("proxied request");
        assert!(proxied.get("error").is_none(), "{proxied}");
        assert!(proxied.to_string().contains("proxied"), "{proxied}");
        allowed_server.join().expect("allowed server");

        let bypass = call_native_sandbox_mcp(
            &runtime,
            sandbox_id.as_str(),
            &rpc(
                "network-bypass",
                "tools/call",
                json!({
                    "name": "execute_command",
                    "arguments": {
                        "path": ".",
                        "command": format!(
                            "if /usr/bin/curl --noproxy '*' --silent --show-error --max-time 1 http://127.0.0.1:{bypass_port}/; then exit 42; else exit 0; fi"
                        )
                    },
                }),
            ),
        )
        .await
        .expect("bypass request");
        assert!(bypass.get("error").is_none(), "{bypass}");
        assert!(
            bypass_listener.accept().is_err(),
            "command bypassed the managed proxy"
        );

        destroy_native_sandbox_process(&runtime, sandbox_id.as_str())
            .await
            .expect("destroy sandbox");
        let _ = std::fs::remove_dir_all(root);
    }
}
