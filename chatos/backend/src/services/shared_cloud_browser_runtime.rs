// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

use chatos_mcp::{BrowserToolsOptions, BrowserToolsService, BrowserVisionAdapterRef};
use chatos_plugin_management_sdk::SystemAgentKey;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::api::fs::policy::user_path_component;
use crate::services::shared_builtin_browser_tools::ChatosBrowserVisionAdapter;
use crate::utils::workspace::resolve_workspace_dir;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CloudBrowserRuntimeBinding {
    pub runtime_session_id: String,
    pub owner_user_id: String,
    pub agent_key: SystemAgentKey,
    pub project_id: String,
    pub source_session_id: String,
    pub expires_at_unix: i64,
}

#[derive(Clone)]
struct CloudBrowserRuntime {
    binding: CloudBrowserRuntimeBinding,
    service: BrowserToolsService,
    call_lock: Arc<Mutex<()>>,
}

static CLOUD_BROWSER_RUNTIMES: LazyLock<Mutex<HashMap<String, CloudBrowserRuntime>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
const MAX_CLOUD_BROWSER_RUNTIMES: usize = 512;
const MAX_OWNER_CLOUD_BROWSER_RUNTIMES: usize = 32;

pub(crate) async fn call_cloud_browser_tool(
    binding: CloudBrowserRuntimeBinding,
    name: &str,
    arguments: Value,
) -> Result<Value, String> {
    let runtime = get_or_create_runtime(binding).await?;
    let _guard = runtime.call_lock.lock().await;
    runtime.service.call_tool(
        name,
        arguments,
        Some(runtime.binding.source_session_id.as_str()),
    )
}

pub(crate) fn probe_cloud_browser_tools(
    binding: &CloudBrowserRuntimeBinding,
) -> Result<Vec<Value>, String> {
    validate_live_binding(binding)?;
    build_cloud_browser_service(binding).map(|service| service.list_tools())
}

pub(crate) async fn close_cloud_browser_runtime(
    binding: &CloudBrowserRuntimeBinding,
) -> Result<bool, String> {
    let runtime = {
        let mut runtimes = CLOUD_BROWSER_RUNTIMES.lock().await;
        let Some(runtime) = runtimes.get(binding.runtime_session_id.as_str()) else {
            return Ok(false);
        };
        if runtime.binding != *binding {
            return Err(
                "cloud Browser Runtime close binding does not match the immutable session"
                    .to_string(),
            );
        }
        runtimes.remove(binding.runtime_session_id.as_str())
    };
    if let Some(runtime) = runtime {
        close_runtime_browser_session(&runtime).await;
        Ok(true)
    } else {
        Ok(false)
    }
}

async fn get_or_create_runtime(
    binding: CloudBrowserRuntimeBinding,
) -> Result<CloudBrowserRuntime, String> {
    validate_live_binding(&binding)?;
    let now = chrono::Utc::now().timestamp();

    let (runtime, expired) = {
        let mut runtimes = CLOUD_BROWSER_RUNTIMES.lock().await;
        let expired_ids = runtimes
            .iter()
            .filter(|(_, runtime)| runtime.binding.expires_at_unix <= now)
            .map(|(session_id, _)| session_id.clone())
            .collect::<Vec<_>>();
        let expired = expired_ids
            .into_iter()
            .filter_map(|session_id| runtimes.remove(session_id.as_str()))
            .collect::<Vec<_>>();

        if let Some(runtime) = runtimes.get(binding.runtime_session_id.as_str()) {
            if runtime.binding != binding {
                return Err(
                    "cloud Browser Runtime binding does not match the immutable session"
                        .to_string(),
                );
            }
            (runtime.clone(), expired)
        } else {
            if runtimes.len() >= MAX_CLOUD_BROWSER_RUNTIMES {
                return Err("cloud Browser Runtime capacity is exhausted".to_string());
            }
            let owner_runtime_count = runtimes
                .values()
                .filter(|runtime| runtime.binding.owner_user_id == binding.owner_user_id)
                .count();
            if owner_runtime_count >= MAX_OWNER_CLOUD_BROWSER_RUNTIMES {
                return Err("cloud Browser Runtime owner session limit is exhausted".to_string());
            }
            let service = build_cloud_browser_service(&binding)?;
            let runtime = CloudBrowserRuntime {
                binding: binding.clone(),
                service,
                call_lock: Arc::new(Mutex::new(())),
            };
            runtimes.insert(binding.runtime_session_id.clone(), runtime.clone());
            (runtime, expired)
        }
    };

    for runtime in expired {
        tokio::spawn(async move {
            close_runtime_browser_session(&runtime).await;
        });
    }
    Ok(runtime)
}

fn build_cloud_browser_service(
    binding: &CloudBrowserRuntimeBinding,
) -> Result<BrowserToolsService, String> {
    BrowserToolsService::new(BrowserToolsOptions {
        server_name: chatos_mcp::system_mcp_descriptor(chatos_mcp::SystemMcpKey::BrowserTools)
            .server_name
            .to_string(),
        workspace_dir: cloud_browser_workspace_dir(binding)?,
        vision_adapter: Some(BrowserVisionAdapterRef::new(Arc::new(
            ChatosBrowserVisionAdapter,
        ))),
        route_interception_enabled: false,
        full_cdp_access_enabled: false,
        ..BrowserToolsOptions::default()
    })
}

async fn close_runtime_browser_session(runtime: &CloudBrowserRuntime) {
    let _guard = runtime.call_lock.lock().await;
    let _ = runtime
        .service
        .close_attached_managed_session(runtime.binding.source_session_id.as_str())
        .await;
}

fn validate_binding(binding: &CloudBrowserRuntimeBinding) -> Result<(), String> {
    for (label, value) in [
        ("runtime_session_id", binding.runtime_session_id.as_str()),
        ("owner_user_id", binding.owner_user_id.as_str()),
        ("project_id", binding.project_id.as_str()),
        ("source_session_id", binding.source_session_id.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(format!("cloud Browser Runtime {label} is required"));
        }
    }
    Ok(())
}

fn validate_live_binding(binding: &CloudBrowserRuntimeBinding) -> Result<(), String> {
    validate_binding(binding)?;
    if binding.expires_at_unix <= chrono::Utc::now().timestamp() {
        return Err("cloud Browser Runtime session has expired".to_string());
    }
    Ok(())
}

fn cloud_browser_workspace_dir(binding: &CloudBrowserRuntimeBinding) -> Result<PathBuf, String> {
    let base = PathBuf::from(resolve_workspace_dir(None));
    let owner_component = user_path_component(binding.owner_user_id.as_str());
    let session_component = opaque_path_component("session", binding.runtime_session_id.as_str());
    let user_root = base.join("users").join(owner_component);
    let public_root = user_root.join("public");
    let browser_root = public_root.join("browser");
    let workspace_dir = browser_root.join(session_component);
    fs::create_dir_all(workspace_dir.as_path())
        .map_err(|error| format!("create cloud Browser Runtime workspace failed: {error}"))?;
    for path in [
        user_root.as_path(),
        public_root.as_path(),
        browser_root.as_path(),
        workspace_dir.as_path(),
    ] {
        set_private_dir_permissions(path)
            .map_err(|error| format!("secure cloud Browser Runtime workspace failed: {error}"))?;
    }
    Ok(workspace_dir)
}

fn opaque_path_component(prefix: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    let digest = hex::encode(hasher.finalize());
    format!("{prefix}-{}", &digest[..24])
}

fn set_private_dir_permissions(_path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> CloudBrowserRuntimeBinding {
        CloudBrowserRuntimeBinding {
            runtime_session_id: "runtime-1".to_string(),
            owner_user_id: "owner-1".to_string(),
            agent_key: SystemAgentKey::ChatosConversationAgent,
            project_id: "project-1".to_string(),
            source_session_id: "conversation-1".to_string(),
            expires_at_unix: chrono::Utc::now().timestamp() + 300,
        }
    }

    #[test]
    fn immutable_binding_detects_owner_project_agent_and_source_drift() {
        let expected = binding();
        let mut drifted = expected.clone();
        drifted.owner_user_id = "owner-2".to_string();
        assert_ne!(expected, drifted);
        let mut drifted = expected.clone();
        drifted.project_id = "project-2".to_string();
        assert_ne!(expected, drifted);
        let mut drifted = expected.clone();
        drifted.agent_key = SystemAgentKey::TaskRunnerRunPhase;
        assert_ne!(expected, drifted);
        let mut drifted = expected.clone();
        drifted.source_session_id = "conversation-2".to_string();
        assert_ne!(expected, drifted);
    }

    #[test]
    fn workspace_components_do_not_expose_raw_runtime_identity() {
        let component = opaque_path_component("session", "runtime/secret");
        assert!(component.starts_with("session-"));
        assert!(!component.contains("runtime"));
        assert!(!component.contains("secret"));
        assert!(!component.contains('/'));
    }

    #[test]
    fn probe_uses_the_cloud_browser_security_profile_without_starting_a_session() {
        let binding = binding();
        let workspace_dir = cloud_browser_workspace_dir(&binding).expect("browser workspace");
        let service = build_cloud_browser_service(&binding).expect("cloud browser service");
        let tools = service.list_tools();
        let names = tools
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let backend_available = service
            .unavailable_tools()
            .iter()
            .all(|(name, _)| name != "browser_navigate");
        if backend_available {
            assert!(!names.is_empty());
            assert!(names.contains(&"browser_navigate"));
            assert!(!names.contains(&"browser_route_add"));
            assert!(!names.contains(&"browser_cdp_command"));
        } else {
            assert!(names.is_empty());
        }
        let _ = fs::remove_dir_all(workspace_dir);
    }
}
