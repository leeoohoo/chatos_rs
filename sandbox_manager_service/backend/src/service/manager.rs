use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::http::StatusCode;
use chrono::{Duration as ChronoDuration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::backend::{SandboxBackendRef, SandboxCreateSpec};
use crate::config::AppConfig;
use crate::error::ApiError;
use crate::models::{
    CreateSandboxLeaseRequest, CreateSandboxLeaseResponse, DestroySandboxResponse,
    HeartbeatRequest, HeartbeatResponse, ListSandboxQuery, PoolStatusResponse,
    ReleaseSandboxRequest, ReleaseSandboxResponse, SandboxEventRecord, SandboxHealthCheck,
    SandboxHealthResponse, SandboxLeaseRecord, SandboxMcpCallRequest, SandboxMcpCallResponse,
    SandboxMcpToolsResponse, SandboxStatus, SystemConfigResponse,
};
use crate::pool::SandboxPoolRef;
use crate::store::SandboxStore;

#[derive(Clone)]
pub struct SandboxManager {
    config: AppConfig,
    store: SandboxStore,
    backend: SandboxBackendRef,
    pool: SandboxPoolRef,
}

impl SandboxManager {
    pub async fn new(
        config: AppConfig,
        store: SandboxStore,
        backend: SandboxBackendRef,
        pool: SandboxPoolRef,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&config.work_root)
            .map_err(|err| format!("create sandbox work root failed: {err}"))?;
        Ok(Self {
            config,
            store,
            backend,
            pool,
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    pub async fn create_lease(
        &self,
        input: CreateSandboxLeaseRequest,
    ) -> Result<CreateSandboxLeaseResponse, ApiError> {
        validate_required("tenant_id", &input.tenant_id)?;
        validate_required("user_id", &input.user_id)?;
        validate_required("project_id", &input.project_id)?;
        validate_required("run_id", &input.run_id)?;
        validate_required("workspace_root", &input.workspace_root)?;

        let slot = self.pool.try_acquire_active().map_err(ApiError::capacity)?;
        let lease_id = prefixed_id("lease");
        let sandbox_id = prefixed_id("sandbox");
        let now = now_rfc3339();
        let ttl = Duration::from_secs(input.ttl_seconds.unwrap_or(self.config.lease_ttl.as_secs()));
        let expires_at = (Utc::now()
            + ChronoDuration::from_std(ttl).unwrap_or_else(|_| ChronoDuration::seconds(7_200)))
        .to_rfc3339();
        let run_workspace =
            self.prepare_run_workspace(input.workspace_root.as_str(), input.run_id.as_str())?;
        let resource_limits = input.resource_limits.unwrap_or_default();
        let network = input.network.unwrap_or_default();
        let tools = if input.tools.is_empty() {
            vec!["filesystem".to_string(), "terminal".to_string()]
        } else {
            input.tools
        };

        let mut record = SandboxLeaseRecord {
            id: lease_id.clone(),
            sandbox_id: sandbox_id.clone(),
            tenant_id: input.tenant_id.trim().to_string(),
            user_id: input.user_id.trim().to_string(),
            project_id: input.project_id.trim().to_string(),
            run_id: input.run_id.trim().to_string(),
            workspace_root: input.workspace_root.trim().to_string(),
            run_workspace: run_workspace.to_string_lossy().to_string(),
            backend: self.backend.kind().to_string(),
            backend_id: None,
            status: SandboxStatus::Leasing,
            agent_endpoint: None,
            resource_limits: resource_limits.clone(),
            network: network.clone(),
            tools,
            created_at: now.clone(),
            updated_at: now.clone(),
            expires_at,
            destroyed_at: None,
            last_error: None,
        };
        self.store
            .create_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "lease_created",
            Some("sandbox lease created"),
            Some(json!({ "backend": self.backend.kind() })),
        )
        .await;

        let create_result = self
            .backend
            .create(SandboxCreateSpec {
                sandbox_id: sandbox_id.clone(),
                run_workspace: record.run_workspace.clone(),
                resource_limits,
                network,
            })
            .await;

        match create_result {
            Ok(instance) => {
                if let Err(err) = self.backend.start(sandbox_id.as_str()).await {
                    record.status = SandboxStatus::Failed;
                    record.last_error = Some(err.clone());
                    record.updated_at = now_rfc3339();
                    let _ = self.store.replace_lease(&record).await;
                    self.event(&record, "sandbox_start_failed", Some(&err), None)
                        .await;
                    return Err(ApiError::with_code(
                        StatusCode::BAD_GATEWAY,
                        "sandbox_create_failed",
                        err,
                    ));
                }
                record.status = SandboxStatus::Ready;
                record.backend_id = instance.backend_id.clone();
                record.agent_endpoint = instance.agent_endpoint;
                record.updated_at = now_rfc3339();
                self.store
                    .replace_lease(&record)
                    .await
                    .map_err(ApiError::internal)?;
                self.event(
                    &record,
                    "sandbox_ready",
                    Some("sandbox is ready"),
                    Some(json!({ "backend_id": instance.backend_id })),
                )
                .await;
                slot.commit();
                Ok(CreateSandboxLeaseResponse {
                    lease_id,
                    sandbox_id,
                    backend_id: record.backend_id,
                    status: record.status,
                    agent_endpoint: record.agent_endpoint,
                    run_workspace: record.run_workspace,
                    expires_at: record.expires_at,
                })
            }
            Err(err) => {
                record.status = SandboxStatus::Failed;
                record.last_error = Some(err.clone());
                record.updated_at = now_rfc3339();
                let _ = self.store.replace_lease(&record).await;
                self.event(&record, "sandbox_create_failed", Some(&err), None)
                    .await;
                Err(ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_create_failed",
                    err,
                ))
            }
        }
    }

    pub async fn heartbeat(
        &self,
        sandbox_id: &str,
        input: HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ApiError> {
        let mut record = self.require_sandbox(sandbox_id).await?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match sandbox"));
        }
        if record.run_id != input.run_id {
            return Err(ApiError::bad_request("run_id does not match sandbox"));
        }
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(&record, "heartbeat", Some("sandbox heartbeat"), None)
            .await;
        Ok(HeartbeatResponse {
            ok: true,
            status: record.status,
            expires_at: record.expires_at,
        })
    }

    pub async fn health(&self, sandbox_id: &str) -> Result<SandboxHealthResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        let checked_at = now_rfc3339();

        let (backend_instance, backend_error) = match self
            .backend
            .inspect(sandbox_id, record.backend_id.as_deref())
            .await
        {
            Ok(instance) => (instance, None),
            Err(err) => (None, Some(err)),
        };
        let backend_id = backend_instance
            .as_ref()
            .and_then(|instance| instance.backend_id.clone())
            .or_else(|| record.backend_id.clone());
        let backend_alive = backend_instance.is_some();
        let backend_message = match (&backend_id, &backend_error) {
            (_, Some(err)) => format!("backend inspect failed: {err}"),
            (Some(id), None) => format!("backend instance found: {id}"),
            (None, None) => "backend instance was not found".to_string(),
        };

        let agent_endpoint = record.agent_endpoint.clone().or_else(|| {
            backend_instance
                .as_ref()
                .and_then(|instance| instance.agent_endpoint.clone())
        });
        let (agent_alive, agent_message) = check_agent_health(agent_endpoint.as_deref()).await;

        let lifecycle_ok = matches!(record.status, SandboxStatus::Ready | SandboxStatus::Running);
        let workspace_alive = std::fs::metadata(record.run_workspace.as_str())
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);

        let checks = vec![
            SandboxHealthCheck {
                name: "lifecycle_status".to_string(),
                ok: lifecycle_ok,
                message: if lifecycle_ok {
                    format!("sandbox status is {}", record.status.as_str())
                } else {
                    format!("sandbox status is not ready: {}", record.status.as_str())
                },
            },
            SandboxHealthCheck {
                name: "backend_instance".to_string(),
                ok: backend_alive,
                message: backend_message,
            },
            SandboxHealthCheck {
                name: "agent_health".to_string(),
                ok: agent_alive.unwrap_or(false),
                message: agent_message,
            },
            SandboxHealthCheck {
                name: "workspace_path".to_string(),
                ok: workspace_alive,
                message: if workspace_alive {
                    "run workspace exists".to_string()
                } else {
                    "run workspace does not exist".to_string()
                },
            },
        ];

        let ok = checks.iter().all(|check| check.ok);
        let message = if ok {
            "sandbox is healthy and ready for file and terminal operations".to_string()
        } else {
            let failed_checks = checks
                .iter()
                .filter(|check| !check.ok)
                .map(|check| check.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("sandbox health check failed: {failed_checks}")
        };

        let response = SandboxHealthResponse {
            ok,
            sandbox_id: record.sandbox_id.clone(),
            lease_id: record.id.clone(),
            status: record.status,
            backend: record.backend.clone(),
            backend_id,
            backend_alive,
            agent_endpoint,
            agent_alive,
            workspace_alive,
            checked_at,
            message,
            checks,
        };

        self.event(
            &record,
            "sandbox_health_checked",
            Some(response.message.as_str()),
            Some(json!({
                "ok": response.ok,
                "backend_alive": response.backend_alive,
                "agent_alive": response.agent_alive,
                "workspace_alive": response.workspace_alive,
            })),
        )
        .await;

        Ok(response)
    }

    pub async fn mcp_tools(&self, sandbox_id: &str) -> Result<SandboxMcpToolsResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let result = jsonrpc_agent_call(agent_endpoint.as_str(), "tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .ok_or_else(|| {
                ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_mcp_invalid_response",
                    "sandbox MCP tools/list response did not contain tools",
                )
            })?;
        Ok(SandboxMcpToolsResponse {
            ok: true,
            sandbox_id: record.sandbox_id,
            agent_endpoint,
            tools,
        })
    }

    pub async fn mcp_call(
        &self,
        sandbox_id: &str,
        input: SandboxMcpCallRequest,
    ) -> Result<SandboxMcpCallResponse, ApiError> {
        let name = input.name.trim();
        if name.is_empty() {
            return Err(ApiError::bad_request("tool name is required"));
        }
        let record = self.require_sandbox(sandbox_id).await?;
        let agent_endpoint = self.agent_endpoint_for(&record).await?;
        let result = jsonrpc_agent_call(
            agent_endpoint.as_str(),
            "tools/call",
            json!({ "name": name, "arguments": input.arguments }),
        )
        .await?;
        Ok(SandboxMcpCallResponse {
            ok: true,
            sandbox_id: record.sandbox_id,
            agent_endpoint,
            result,
        })
    }

    pub async fn release(
        &self,
        sandbox_id: &str,
        input: ReleaseSandboxRequest,
    ) -> Result<ReleaseSandboxResponse, ApiError> {
        let mut record = self.require_sandbox(sandbox_id).await?;
        if record.id != input.lease_id {
            return Err(ApiError::bad_request("lease_id does not match sandbox"));
        }
        record.status = SandboxStatus::Releasing;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "sandbox_releasing",
            Some("sandbox release started"),
            Some(json!({ "export_result": input.export_result, "destroy": input.destroy })),
        )
        .await;

        let output_workspace = if input.export_result {
            Some(self.prepare_output_workspace(&record)?)
        } else {
            None
        };

        if input.destroy {
            self.destroy_record(record.clone(), "sandbox_released")
                .await?;
            Ok(ReleaseSandboxResponse {
                ok: true,
                status: SandboxStatus::Destroyed,
                output_workspace: output_workspace.map(|path| path.to_string_lossy().to_string()),
                diff_summary: None,
            })
        } else {
            record.status = SandboxStatus::Ready;
            record.updated_at = now_rfc3339();
            self.store
                .replace_lease(&record)
                .await
                .map_err(ApiError::internal)?;
            Ok(ReleaseSandboxResponse {
                ok: true,
                status: record.status,
                output_workspace: output_workspace.map(|path| path.to_string_lossy().to_string()),
                diff_summary: None,
            })
        }
    }

    pub async fn destroy(&self, sandbox_id: &str) -> Result<DestroySandboxResponse, ApiError> {
        let record = self.require_sandbox(sandbox_id).await?;
        self.destroy_record(record, "sandbox_destroyed").await?;
        Ok(DestroySandboxResponse {
            ok: true,
            status: SandboxStatus::Destroyed,
        })
    }

    pub async fn get(&self, sandbox_id: &str) -> Result<SandboxLeaseRecord, ApiError> {
        self.require_sandbox(sandbox_id).await
    }

    pub async fn list(&self, query: ListSandboxQuery) -> Result<Vec<SandboxLeaseRecord>, ApiError> {
        self.store
            .list_leases(query)
            .await
            .map_err(ApiError::internal)
    }

    pub async fn events(&self, sandbox_id: &str) -> Result<Vec<SandboxEventRecord>, ApiError> {
        self.store
            .list_events(sandbox_id)
            .await
            .map_err(ApiError::internal)
    }

    pub fn pool_status(&self) -> PoolStatusResponse {
        PoolStatusResponse {
            backend: self.backend.kind().to_string(),
            max_active: self.pool.max_active(),
            active: self.pool.active(),
            max_pending: self.pool.max_pending(),
            pending: self.pool.pending(),
            lease_ttl_seconds: self.config.lease_ttl.as_secs(),
            cleanup_interval_seconds: self.config.cleanup_interval.as_secs(),
        }
    }

    pub fn system_config(&self) -> SystemConfigResponse {
        SystemConfigResponse {
            host: self.config.host.to_string(),
            port: self.config.port,
            backend: self.backend.kind().to_string(),
            work_root: self.config.work_root.to_string_lossy().to_string(),
            pool_max_active: self.config.pool_max_active,
            pool_max_pending: self.config.pool_max_pending,
            lease_ttl_seconds: self.config.lease_ttl.as_secs(),
            cleanup_interval_seconds: self.config.cleanup_interval.as_secs(),
            agent_port: self.config.agent_port,
            docker_image: self.config.docker_image.clone(),
            docker_network_mode: self.config.docker_network_mode.clone(),
            kata_container_cli: self.config.kata_container_cli.clone(),
            kata_runtime: self.config.kata_runtime.clone(),
            kata_image: self.config.kata_image.clone(),
            kata_network_mode: self.config.kata_network_mode.clone(),
        }
    }

    pub async fn cleanup_expired(&self) -> Result<(), String> {
        let now = now_rfc3339();
        let expired = self.store.list_expired_active(now.as_str(), 100).await?;
        for record in expired {
            let mut expired_record = record.clone();
            expired_record.status = SandboxStatus::Expired;
            expired_record.updated_at = now_rfc3339();
            expired_record.last_error = Some("lease expired".to_string());
            self.store.replace_lease(&expired_record).await?;
            self.event(
                &expired_record,
                "sandbox_expired",
                Some("sandbox lease expired"),
                None,
            )
            .await;
            if let Err(err) = self
                .destroy_record(expired_record, "sandbox_expired_destroyed")
                .await
            {
                tracing::warn!("destroy expired sandbox failed: {}", err.message);
            }
        }
        Ok(())
    }

    async fn require_sandbox(&self, sandbox_id: &str) -> Result<SandboxLeaseRecord, ApiError> {
        self.store
            .get_by_sandbox_id(sandbox_id)
            .await
            .map_err(ApiError::internal)?
            .ok_or_else(|| ApiError::not_found(format!("sandbox not found: {sandbox_id}")))
    }

    async fn agent_endpoint_for(&self, record: &SandboxLeaseRecord) -> Result<String, ApiError> {
        if let Some(endpoint) = record
            .agent_endpoint
            .clone()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            return validate_http_agent_endpoint(endpoint);
        }

        let inspected = self
            .backend
            .inspect(record.sandbox_id.as_str(), record.backend_id.as_deref())
            .await
            .map_err(|err| {
                ApiError::with_code(
                    StatusCode::BAD_GATEWAY,
                    "sandbox_backend_inspect_failed",
                    err,
                )
            })?;
        let endpoint = inspected.and_then(|instance| instance.agent_endpoint);
        let endpoint = endpoint
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| ApiError::bad_request("sandbox agent endpoint is not available"))?;
        validate_http_agent_endpoint(endpoint)
    }

    async fn destroy_record(
        &self,
        mut record: SandboxLeaseRecord,
        event_type: &str,
    ) -> Result<(), ApiError> {
        let was_active = record.status.is_active();
        record.status = SandboxStatus::Destroying;
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        self.event(
            &record,
            "sandbox_destroying",
            Some("destroying sandbox"),
            None,
        )
        .await;

        if let Err(err) = self
            .backend
            .destroy(record.sandbox_id.as_str(), record.backend_id.as_deref())
            .await
        {
            record.status = SandboxStatus::Failed;
            record.last_error = Some(err.clone());
            record.updated_at = now_rfc3339();
            let _ = self.store.replace_lease(&record).await;
            self.event(&record, "sandbox_destroy_failed", Some(&err), None)
                .await;
            return Err(ApiError::with_code(
                StatusCode::BAD_GATEWAY,
                "sandbox_destroy_failed",
                err,
            ));
        }

        record.status = SandboxStatus::Destroyed;
        record.destroyed_at = Some(now_rfc3339());
        record.updated_at = now_rfc3339();
        self.store
            .replace_lease(&record)
            .await
            .map_err(ApiError::internal)?;
        if was_active {
            self.pool.release_active();
        }
        self.event(&record, event_type, Some("sandbox destroyed"), None)
            .await;
        Ok(())
    }

    fn prepare_run_workspace(
        &self,
        workspace_root: &str,
        run_id: &str,
    ) -> Result<PathBuf, ApiError> {
        let root = PathBuf::from(workspace_root.trim());
        let base = if self.config.work_root.is_absolute() {
            self.config.work_root.clone()
        } else {
            root.join(&self.config.work_root)
        };
        let run_workspace = base
            .join("runs")
            .join(sanitize_path_segment(run_id))
            .join("input")
            .join("workspace");
        std::fs::create_dir_all(&run_workspace)
            .map_err(|err| ApiError::internal(format!("create run workspace failed: {err}")))?;
        Ok(run_workspace)
    }

    fn prepare_output_workspace(&self, record: &SandboxLeaseRecord) -> Result<PathBuf, ApiError> {
        let run_workspace = Path::new(record.run_workspace.as_str());
        let run_root = run_workspace
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| ApiError::internal("invalid run workspace path"))?;
        let output = run_root.join("output").join("workspace");
        std::fs::create_dir_all(&output)
            .map_err(|err| ApiError::internal(format!("create output workspace failed: {err}")))?;
        Ok(output)
    }

    async fn event(
        &self,
        record: &SandboxLeaseRecord,
        event_type: &str,
        message: Option<&str>,
        payload: Option<serde_json::Value>,
    ) {
        let event = SandboxEventRecord {
            id: prefixed_id("event"),
            sandbox_id: record.sandbox_id.clone(),
            lease_id: record.id.clone(),
            event_type: event_type.to_string(),
            message: message.map(ToOwned::to_owned),
            payload,
            created_at: now_rfc3339(),
        };
        if let Err(err) = self.store.append_event(&event).await {
            tracing::warn!("append sandbox event failed: {}", err);
        }
    }
}

fn validate_required(name: &'static str, value: &str) -> Result<(), ApiError> {
    if value.trim().is_empty() {
        return Err(ApiError::bad_request(format!("{name} is required")));
    }
    Ok(())
}

async fn check_agent_health(agent_endpoint: Option<&str>) -> (Option<bool>, String) {
    let Some(endpoint) = agent_endpoint
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (None, "agent endpoint is not configured".to_string());
    };

    if endpoint.starts_with("mock://") {
        return (Some(true), "mock agent endpoint is reachable".to_string());
    }

    if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
        return (
            Some(false),
            format!("unsupported agent endpoint scheme: {endpoint}"),
        );
    }

    let health_url = format!("{}/health", endpoint.trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            return (
                Some(false),
                format!("build agent health client failed: {err}"),
            )
        }
    };

    match client.get(&health_url).send().await {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                (
                    Some(true),
                    format!("agent health endpoint returned {status}"),
                )
            } else {
                (
                    Some(false),
                    format!("agent health endpoint returned {status}"),
                )
            }
        }
        Err(err) => (Some(false), format!("agent health request failed: {err}")),
    }
}

async fn jsonrpc_agent_call(
    agent_endpoint: &str,
    method: &str,
    params: Value,
) -> Result<Value, ApiError> {
    let url = format!("{}/mcp", agent_endpoint.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| ApiError::internal(format!("build MCP client failed: {err}")))?;
    let response = client
        .post(url.as_str())
        .json(&json!({
            "jsonrpc": "2.0",
            "id": prefixed_id("mcp"),
            "method": method,
            "params": params,
        }))
        .send()
        .await
        .map_err(|err| {
            ApiError::with_code(
                StatusCode::BAD_GATEWAY,
                "sandbox_mcp_request_failed",
                format!("{method} request failed: {err}"),
            )
        })?;

    let status = response.status();
    let body = response.text().await.map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_response_failed",
            format!("{method} response read failed: {err}"),
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_http_error",
            format!("{method} returned HTTP {status}: {}", preview_text(&body)),
        ));
    }
    let value: Value = serde_json::from_str(body.as_str()).map_err(|err| {
        ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_invalid_json",
            format!(
                "{method} returned invalid JSON: {err}; body={}",
                preview_text(&body)
            ),
        )
    })?;
    if let Some(error) = value.get("error") {
        return Err(ApiError::with_code(
            StatusCode::BAD_GATEWAY,
            "sandbox_mcp_jsonrpc_error",
            format!(
                "{method} returned JSON-RPC error: {}",
                preview_text(&error.to_string())
            ),
        ));
    }
    Ok(value.get("result").cloned().unwrap_or(value))
}

fn validate_http_agent_endpoint(endpoint: String) -> Result<String, ApiError> {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Ok(endpoint)
    } else {
        Err(ApiError::bad_request(format!(
            "sandbox agent endpoint is not an HTTP endpoint: {endpoint}"
        )))
    }
}

fn preview_text(value: &str) -> String {
    const LIMIT: usize = 1200;
    if value.chars().count() <= LIMIT {
        return value.to_string();
    }
    value.chars().take(LIMIT).collect::<String>() + "...[truncated]"
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn prefixed_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
}

fn sanitize_path_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>()
}
