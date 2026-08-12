// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command;

use crate::config::{AppConfig, SandboxBackendKind};
use crate::models::{NetworkPolicy, ResourceLimits};
use chatos_sandbox_contract::EffectivePermissionSnapshot;

mod docker;
mod kata;
mod mock;

pub use docker::DockerSandboxBackend;
pub use kata::KataSandboxBackend;
pub use mock::MockSandboxBackend;

pub type SandboxBackendRef = Arc<dyn SandboxBackend>;

#[cfg(test)]
const SANDBOX_RUNTIME_CACHE_ROOT: &str = "/home/sandbox/.cache";

fn sandbox_runtime_environment() -> [(&'static str, &'static str); 10] {
    [
        ("HOME", "/home/sandbox"),
        ("XDG_CACHE_HOME", "/home/sandbox/.cache/xdg"),
        ("NPM_CONFIG_CACHE", "/home/sandbox/.cache/npm"),
        ("COREPACK_HOME", "/home/sandbox/.cache/corepack"),
        ("YARN_CACHE_FOLDER", "/home/sandbox/.cache/yarn"),
        ("PIP_CACHE_DIR", "/home/sandbox/.cache/pip"),
        ("UV_CACHE_DIR", "/home/sandbox/.cache/uv"),
        ("CARGO_HOME", "/home/sandbox/.cache/cargo"),
        ("GRADLE_USER_HOME", "/home/sandbox/.cache/gradle"),
        ("MAVEN_CONFIG", "/home/sandbox/.cache/maven"),
    ]
}

const SANDBOX_RUNTIME_NO_PROXY_DEFAULTS: [&str; 4] =
    ["localhost", "127.0.0.1", "postgresql", "workspace"];

fn sandbox_runtime_no_proxy(config: &AppConfig) -> String {
    let mut entries = SANDBOX_RUNTIME_NO_PROXY_DEFAULTS
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    if let Some(configured) = config.runtime_no_proxy.as_deref() {
        for entry in configured
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            if !entries.iter().any(|existing| existing == entry) {
                entries.push(entry.to_string());
            }
        }
    }
    entries.join(",")
}

fn sandbox_runtime_environment_values(config: &AppConfig) -> Vec<String> {
    let mut values = sandbox_runtime_environment()
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>();
    for (upper, lower, value) in [
        (
            "HTTP_PROXY",
            "http_proxy",
            config.runtime_http_proxy.as_deref(),
        ),
        (
            "HTTPS_PROXY",
            "https_proxy",
            config.runtime_https_proxy.as_deref(),
        ),
    ] {
        if let Some(value) = value {
            values.push(format!("{upper}={value}"));
            values.push(format!("{lower}={value}"));
        }
    }
    if config.runtime_http_proxy.is_some()
        || config.runtime_https_proxy.is_some()
        || config.runtime_no_proxy.is_some()
    {
        let no_proxy = sandbox_runtime_no_proxy(config);
        values.push(format!("NO_PROXY={no_proxy}"));
        values.push(format!("no_proxy={no_proxy}"));
    }
    values
}

fn append_sandbox_runtime_environment(command: &mut Command, config: &AppConfig) {
    for value in sandbox_runtime_environment_values(config) {
        command.arg("-e").arg(value);
    }
}

#[derive(Debug, Clone)]
pub struct SandboxCreateSpec {
    pub sandbox_id: String,
    pub run_workspace: String,
    pub image: String,
    pub agent_token: Option<String>,
    pub resource_limits: ResourceLimits,
    pub network: NetworkPolicy,
    pub effective_permissions: EffectivePermissionSnapshot,
}

#[derive(Debug, Clone)]
pub struct SandboxInstance {
    pub sandbox_id: String,
    pub backend_id: Option<String>,
    pub agent_endpoint: Option<String>,
}

fn append_sandbox_create_runtime_args(
    command: &mut Command,
    config: &AppConfig,
    spec: &SandboxCreateSpec,
    network: &str,
    cpu: &str,
    memory: &str,
    pids: &str,
    disk_limit_bytes: u64,
) -> Result<(), String> {
    command
        .arg("--network")
        .arg(network)
        .arg("--cpus")
        .arg(cpu)
        .arg("--memory")
        .arg(memory)
        .arg("--pids-limit")
        .arg(pids)
        .arg("--workdir")
        .arg("/workspace")
        .arg("-e")
        .arg(format!("CHATOS_SANDBOX_ID={}", spec.sandbox_id))
        .arg("-e")
        .arg("CHATOS_SANDBOX_PERMISSION_PROFILE=workspace_write")
        .arg("-e")
        .arg(format!(
            "CHATOS_SANDBOX_DISK_LIMIT_BYTES={disk_limit_bytes}"
        ))
        .arg("-e")
        .arg(effective_permissions_environment_value(
            &spec.effective_permissions,
        )?);
    append_sandbox_runtime_environment(command, config);
    if let Some(agent_token) = spec.agent_token.as_deref() {
        command
            .arg("-e")
            .arg(format!("CHATOS_SANDBOX_MCP_TOKEN={agent_token}"));
    }
    Ok(())
}

fn effective_permissions_environment_value(
    permissions: &EffectivePermissionSnapshot,
) -> Result<String, String> {
    let mut container_permissions = permissions.clone();
    container_permissions.runtime_workspace_roots = vec!["/workspace".to_string()];
    serde_json::to_string(&container_permissions)
        .map(|value| format!("CHATOS_SANDBOX_EFFECTIVE_PERMISSIONS_JSON={value}"))
        .map_err(|err| format!("serialize sandbox effective permissions failed: {err}"))
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentCreateSpec {
    pub environment_id: String,
    pub run_workspace: String,
    pub services: Vec<SandboxEnvironmentServiceSpec>,
    pub agent_token: String,
    pub resource_limits: ResourceLimits,
    pub network: NetworkPolicy,
    pub effective_permissions: EffectivePermissionSnapshot,
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentServiceSpec {
    pub service_id: String,
    pub service_role: String,
    pub image: String,
    pub dockerfile: Option<String>,
    pub environment: BTreeMap<String, String>,
    pub mcp_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentServiceInstance {
    pub service_id: String,
    pub backend_id: Option<String>,
    pub status: String,
    pub agent_endpoint: Option<String>,
    pub image_ref: String,
}

#[derive(Debug, Clone)]
pub struct SandboxEnvironmentInstance {
    pub environment_id: String,
    pub backend_id: Option<String>,
    pub services: Vec<SandboxEnvironmentServiceInstance>,
}

#[derive(Debug, Clone)]
pub struct SandboxExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[async_trait]
pub trait SandboxBackend: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn create(&self, spec: SandboxCreateSpec) -> Result<SandboxInstance, String>;
    async fn start(&self, sandbox_id: &str) -> Result<(), String>;
    async fn stop(&self, sandbox_id: &str) -> Result<(), String>;
    async fn destroy(&self, sandbox_id: &str, backend_id: Option<&str>) -> Result<(), String>;
    async fn inspect(
        &self,
        sandbox_id: &str,
        backend_id: Option<&str>,
    ) -> Result<Option<SandboxInstance>, String>;
    async fn create_environment(
        &self,
        _spec: SandboxEnvironmentCreateSpec,
    ) -> Result<SandboxEnvironmentInstance, String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn start_environment(&self, _environment_id: &str) -> Result<(), String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn stop_environment(&self, _environment_id: &str) -> Result<(), String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn destroy_environment(&self, _environment_id: &str) -> Result<(), String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn inspect_environment(
        &self,
        _environment_id: &str,
    ) -> Result<Option<SandboxEnvironmentInstance>, String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
    async fn exec_environment_service(
        &self,
        _environment_id: &str,
        _service_id: &str,
        _command: &[String],
    ) -> Result<SandboxExecResult, String> {
        Err("sandbox environment groups are unsupported by this backend".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_sandbox_contract::{
        legacy_policy_permission_snapshot, EffectiveSandboxPolicy, NetworkPermissionPolicy,
    };

    #[test]
    fn runtime_caches_live_outside_the_project_workspace() {
        let environment = sandbox_runtime_environment_values(&AppConfig::for_tests());
        assert!(environment
            .iter()
            .any(|value| value == "HOME=/home/sandbox"));
        assert!(environment
            .iter()
            .any(|value| { value == "NPM_CONFIG_CACHE=/home/sandbox/.cache/npm" }));
        assert!(environment
            .iter()
            .any(|value| { value == "XDG_CACHE_HOME=/home/sandbox/.cache/xdg" }));
        assert!(environment
            .iter()
            .filter(|value| !value.starts_with("HOME="))
            .all(|value| value.contains(SANDBOX_RUNTIME_CACHE_ROOT)));
        assert!(!environment
            .iter()
            .any(|value| value.contains("/workspace/")));
    }

    #[test]
    fn runtime_proxy_is_injected_in_both_cases_with_safe_no_proxy_defaults() {
        let mut config = AppConfig::for_tests();
        config.runtime_http_proxy = Some("http://192.168.64.1:17897".to_string());
        config.runtime_https_proxy = Some("http://192.168.64.1:17897".to_string());
        config.runtime_no_proxy = Some("mongodb,workspace".to_string());

        let environment = sandbox_runtime_environment_values(&config);

        for expected in [
            "HTTP_PROXY=http://192.168.64.1:17897",
            "http_proxy=http://192.168.64.1:17897",
            "HTTPS_PROXY=http://192.168.64.1:17897",
            "https_proxy=http://192.168.64.1:17897",
            "NO_PROXY=localhost,127.0.0.1,postgresql,workspace,mongodb",
            "no_proxy=localhost,127.0.0.1,postgresql,workspace,mongodb",
        ] {
            assert!(environment.iter().any(|value| value == expected));
        }
    }

    #[test]
    fn effective_permissions_are_serialized_for_the_sandbox_agent() {
        let mut permissions = legacy_policy_permission_snapshot(
            &EffectiveSandboxPolicy::default(),
            vec!["/host/task-runner/run/workspace".to_string()],
        );
        permissions.network = NetworkPermissionPolicy::Unrestricted;

        let env = effective_permissions_environment_value(&permissions)
            .expect("serialize effective permissions");
        let payload = env
            .strip_prefix("CHATOS_SANDBOX_EFFECTIVE_PERMISSIONS_JSON=")
            .expect("effective permissions env prefix");
        let decoded: EffectivePermissionSnapshot =
            serde_json::from_str(payload).expect("decode effective permissions");

        assert!(matches!(
            decoded.network,
            NetworkPermissionPolicy::Unrestricted
        ));
        assert_eq!(decoded.runtime_workspace_roots, vec!["/workspace"]);
    }
}

pub fn build_backend(config: &AppConfig) -> SandboxBackendRef {
    match config.backend {
        SandboxBackendKind::Docker => Arc::new(DockerSandboxBackend::new(config.clone())),
        SandboxBackendKind::Kata => Arc::new(KataSandboxBackend::new(config.clone())),
        SandboxBackendKind::Mock => Arc::new(MockSandboxBackend::default()),
    }
}
