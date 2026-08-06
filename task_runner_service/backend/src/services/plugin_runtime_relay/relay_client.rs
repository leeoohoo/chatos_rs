// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::time::{Duration, Instant};

use chatos_service_runtime::http_body::read_response_bytes_limited;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Map, Value};

use crate::models::{TaskRecord, TaskRunEventRecord, TaskRunRecord};
use crate::services::RunService;
use crate::store::AppStore;
use crate::trace_context::InternalTraceContextExt;

const PLUGIN_RELAY_SCOPE: &str = "plugin.execute";
const LOCAL_CONNECTOR_TOKEN_AUDIENCE: &str = "local-connector-service";
const PLUGIN_RELAY_RESPONSE_LIMIT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone)]
pub(super) struct PluginRelayClient {
    http: reqwest::Client,
    base_url: String,
    internal_secret: String,
    pub(super) owner_user_id: String,
    pub(super) device_id: String,
    pub(super) workspace_id: Option<String>,
    pub(super) run_id: String,
    pub(super) store: AppStore,
    hook_dispatch_timeout: Duration,
}

impl PluginRelayClient {
    pub(super) fn from_task(
        service: &RunService,
        task: &TaskRecord,
        run: &TaskRunRecord,
    ) -> Result<Self, String> {
        let base_url = plugin_relay_base_url(&service.config)?;
        let internal_secret = service
            .config
            .local_connector_internal_api_secret
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "TASK_RUNNER_LOCAL_CONNECTOR_INTERNAL_API_SECRET is required for Plugin execution"
                    .to_string()
            })?
            .to_string();
        let owner_user_id = task
            .owner_user_id
            .as_deref()
            .or(task.creator_user_id.as_deref())
            .or(Some(task.subject_id.as_str()))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "task owner user id is required for Plugin execution".to_string())?
            .to_string();
        let device_id = task
            .plugin_config
            .device_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Plugin device_id is required for execution".to_string())?
            .to_string();
        let workspace_id = task
            .plugin_config
            .workspace_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let http = service.config.local_connector_http_client.clone();
        Ok(Self {
            http,
            base_url,
            internal_secret,
            owner_user_id,
            device_id,
            workspace_id,
            run_id: run.id.clone(),
            store: service.store.clone(),
            hook_dispatch_timeout: service.config.plugin_hook_relay_timeout,
        })
    }

    pub(super) async fn request(&self, action: &str, mut body: Value) -> Result<Value, String> {
        let object = body
            .as_object_mut()
            .ok_or_else(|| "Plugin relay request body must be an object".to_string())?;
        object.insert("run_id".to_string(), json!(self.run_id));
        self.record_runtime_event(action, "started", &body, None, None, None);
        let started = Instant::now();
        let result = self.send_request(action, &body).await;
        match &result {
            Ok(response) => self.record_runtime_event(
                action,
                "succeeded",
                &body,
                Some(response),
                Some(elapsed_millis(started)),
                None,
            ),
            Err(error) => self.record_runtime_event(
                action,
                "failed",
                &body,
                None,
                Some(elapsed_millis(started)),
                Some(error.as_str()),
            ),
        }
        result
    }

    async fn send_request(&self, action: &str, body: &Value) -> Result<Value, String> {
        let token = chatos_service_runtime::issue_internal_service_token(
            self.internal_secret.as_str(),
            "task-runner",
            LOCAL_CONNECTOR_TOKEN_AUDIENCE,
            PLUGIN_RELAY_SCOPE,
            60,
        )
        .map_err(|error| format!("issue Plugin relay token failed: {error}"))?;
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-local-connector-caller",
            HeaderValue::from_static("task-runner"),
        );
        headers.insert(
            "x-local-connector-internal-token",
            HeaderValue::from_str(token.as_str())
                .map_err(|_| "Plugin relay token is not a valid header".to_string())?,
        );
        headers.insert(
            "x-local-connector-owner-user-id",
            HeaderValue::from_str(self.owner_user_id.as_str())
                .map_err(|_| "Plugin owner user id is not a valid header".to_string())?,
        );
        let mut url = format!(
            "{}/api/local-connectors/relay/{}/plugins/{}",
            self.base_url,
            urlencoding::encode(self.device_id.as_str()),
            action
        );
        if let Some(workspace_id) = self.workspace_id.as_deref() {
            url.push_str("?workspace_id=");
            url.push_str(urlencoding::encode(workspace_id).as_ref());
        }
        let mut request = self.http.post(url).headers(headers).json(body);
        if is_plugin_hook_dispatch(action, body) {
            request = request.timeout(self.hook_dispatch_timeout);
        }
        let response = request
            .with_internal_trace_context()
            .send()
            .await
            .map_err(|error| format!("Plugin {action} relay request failed: {error}"))?;
        let status = response.status();
        let bytes = read_response_bytes_limited(response, PLUGIN_RELAY_RESPONSE_LIMIT_BYTES)
            .await
            .map_err(|error| format!("read Plugin {action} response failed: {error}"))?;
        let value = serde_json::from_slice::<Value>(bytes.as_slice())
            .map_err(|error| format!("decode Plugin {action} response failed: {error}"))?;
        if !status.is_success() {
            let message = value
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("Plugin relay rejected the request");
            return Err(format!("Plugin {action} failed with {status}: {message}"));
        }
        Ok(value)
    }

    fn record_runtime_event(
        &self,
        action: &str,
        status: &str,
        body: &Value,
        response: Option<&Value>,
        duration_ms: Option<u64>,
        error: Option<&str>,
    ) {
        let phase = if action == "execute"
            && body.get("operation").and_then(Value::as_str) == Some("mcp_health_check")
        {
            "health"
        } else {
            action
        };
        let mut payload = Map::from_iter([
            ("run_id".to_string(), json!(self.run_id)),
            ("phase".to_string(), json!(phase)),
            ("status".to_string(), json!(status)),
        ]);
        for field in [
            "plugin_id",
            "release_id",
            "component_key",
            "adapter_session_id",
            "operation",
            "tool_name",
        ] {
            let value = body
                .get(field)
                .and_then(Value::as_str)
                .or_else(|| response.and_then(|value| value.get(field).and_then(Value::as_str)));
            if let Some(value) = value {
                payload.insert(field.to_string(), json!(value));
            }
        }
        if let Some(health_status) = response
            .and_then(|value| value.pointer("/mcp_health/status"))
            .and_then(Value::as_str)
        {
            payload.insert("health_status".to_string(), json!(health_status));
        }
        if let Some(duration_ms) = duration_ms {
            payload.insert("duration_ms".to_string(), json!(duration_ms));
        }
        if let Some(error) = error {
            payload.insert("error".to_string(), json!(sanitize_runtime_error(error)));
        }
        if let Some(hook_dispatch) = response.and_then(|value| value.get("result")) {
            if body.get("operation").and_then(Value::as_str) == Some("dispatch_hook_event") {
                payload.insert("hook_dispatch".to_string(), hook_dispatch.clone());
            }
        }
        self.store.append_run_event_sync(TaskRunEventRecord::new(
            self.run_id.clone(),
            "plugin_runtime",
            Some(format!("Plugin {phase} {status}")),
            Some(Value::Object(payload)),
        ));
    }
}

pub(super) fn is_plugin_hook_dispatch(action: &str, body: &Value) -> bool {
    action == "execute"
        && body.get("operation").and_then(Value::as_str) == Some("dispatch_hook_event")
}

fn elapsed_millis(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub(super) fn sanitize_runtime_error(value: &str) -> String {
    const MAX_ERROR_BYTES: usize = 1024;
    let mut sanitized = String::new();
    for token in value.split_whitespace() {
        let replacement = sanitized_runtime_error_token(token);
        let required_bytes = sanitized
            .len()
            .saturating_add(usize::from(!sanitized.is_empty()))
            .saturating_add(replacement.len());
        if required_bytes > MAX_ERROR_BYTES {
            break;
        }
        if !sanitized.is_empty() {
            sanitized.push(' ');
        }
        sanitized.push_str(replacement);
    }
    sanitized
}

fn sanitized_runtime_error_token(token: &str) -> &str {
    let lower = token.to_ascii_lowercase();
    if lower.contains("://") {
        return "<redacted-url>";
    }
    let contains_secret_name = ["access_token", "refresh_token", "client_secret"]
        .iter()
        .any(|name| lower.contains(name));
    if contains_secret_name || lower.starts_with("bearer") || lower.starts_with("password=") {
        "<redacted-secret>"
    } else {
        token
    }
}

pub(crate) fn plugin_relay_base_url(config: &crate::config::AppConfig) -> Result<String, String> {
    let value = config
        .local_connector_service_base_url
        .as_deref()
        .map(str::trim)
        .map(|value| value.trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            "TASK_RUNNER_LOCAL_CONNECTOR_SERVICE_BASE_URL is required from configuration center for Plugin execution"
                .to_string()
        })?;
    validate_plugin_relay_base_url(value)
}

pub(super) fn validate_plugin_relay_base_url(value: String) -> Result<String, String> {
    let parsed = reqwest::Url::parse(value.as_str())
        .map_err(|error| format!("Plugin relay base URL is invalid: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.path() != "/"
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("Plugin relay base URL must be an HTTP(S) origin without credentials, query, or fragment".to_string());
    }
    Ok(value)
}
