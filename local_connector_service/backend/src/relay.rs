// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayRequest {
    #[serde(rename = "type")]
    pub message_type: String,
    pub request_id: String,
    pub owner_user_id: String,
    pub device_id: String,
    pub workspace_id: String,
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginArtifactRelayAction {
    List,
    Read,
    Create,
    Update,
}

impl PluginArtifactRelayAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
        }
    }

    pub const fn is_write(self) -> bool {
        matches!(self, Self::Create | Self::Update)
    }
}

pub fn plugin_artifact_relay_request(
    owner_user_id: impl Into<String>,
    device_id: impl Into<String>,
    workspace_id: impl Into<String>,
    action: PluginArtifactRelayAction,
    body: Value,
) -> RelayRequest {
    let action = action.as_str();
    RelayRequest {
        message_type: format!("plugin_artifact_{action}_request"),
        request_id: Uuid::new_v4().to_string(),
        owner_user_id: owner_user_id.into(),
        device_id: device_id.into(),
        workspace_id: workspace_id.into(),
        method: "POST".to_string(),
        path: format!("/plugins/artifacts/{action}"),
        headers: BTreeMap::new(),
        body,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelayResponse {
    pub request_id: String,
    pub status: u16,
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    #[serde(default = "default_body")]
    pub body: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalRelayEvent {
    #[serde(rename = "type")]
    pub message_type: String,
    pub terminal_session_id: String,
    #[serde(default = "default_body")]
    pub body: Value,
}

#[derive(Debug, Deserialize)]
struct InboundRelayResponse {
    #[serde(rename = "type")]
    _message_type: Option<String>,
    request_id: String,
    status: Option<u16>,
    headers: Option<BTreeMap<String, String>>,
    body: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct InboundTerminalEvent {
    #[serde(rename = "type")]
    message_type: String,
    terminal_session_id: String,
    body: Option<Value>,
    data: Option<String>,
    code: Option<i32>,
    busy: Option<bool>,
    error: Option<String>,
}

#[derive(Debug)]
pub enum RelayError {
    Offline,
    Timeout,
    RequestEncode(String),
    ResponseChannelClosed,
}

#[derive(Clone, Default)]
pub struct ConnectorRelay {
    inner: Arc<Mutex<RelayState>>,
}

#[derive(Default)]
struct RelayState {
    sessions: HashMap<String, ActiveConnectorSession>,
    pending: HashMap<String, PendingRelayRequest>,
    terminal_events: HashMap<String, broadcast::Sender<TerminalRelayEvent>>,
}

#[derive(Clone)]
struct ActiveConnectorSession {
    owner_user_id: String,
    session_id: String,
    outbound: mpsc::Sender<String>,
}

struct PendingRelayRequest {
    device_id: String,
    sender: oneshot::Sender<RelayResponse>,
}

impl ConnectorRelay {
    pub async fn register_session(
        &self,
        device_id: String,
        owner_user_id: String,
        session_id: String,
        outbound: mpsc::Sender<String>,
    ) {
        let mut inner = self.inner.lock().await;
        inner.sessions.insert(
            device_id,
            ActiveConnectorSession {
                owner_user_id,
                session_id,
                outbound,
            },
        );
    }

    pub async fn unregister_session(&self, device_id: &str, session_id: &str) {
        let mut failed = Vec::new();
        {
            let mut inner = self.inner.lock().await;
            let should_remove = inner
                .sessions
                .get(device_id)
                .map(|session| session.session_id == session_id)
                .unwrap_or(false);
            if should_remove {
                inner.sessions.remove(device_id);
                let request_ids = inner
                    .pending
                    .iter()
                    .filter(|&(_request_id, pending)| pending.device_id == device_id)
                    .map(|(request_id, _pending)| request_id.clone())
                    .collect::<Vec<_>>();
                for request_id in request_ids {
                    if let Some(pending) = inner.pending.remove(request_id.as_str()) {
                        failed.push(pending.sender);
                    }
                }
            }
        }

        for sender in failed {
            let _ = sender.send(RelayResponse {
                request_id: String::new(),
                status: 503,
                headers: BTreeMap::new(),
                body: serde_json::json!({
                    "error": "Local Connector went offline before responding"
                }),
            });
        }
    }

    pub async fn dispatch(
        &self,
        request: RelayRequest,
        timeout_duration: Duration,
    ) -> Result<RelayResponse, RelayError> {
        let request_id = request.request_id.clone();
        let device_id = request.device_id.clone();
        let outbound = {
            let mut inner = self.inner.lock().await;
            let Some(session) = inner.sessions.get(device_id.as_str()) else {
                return Err(RelayError::Offline);
            };
            if session.owner_user_id != request.owner_user_id {
                return Err(RelayError::Offline);
            }
            let outbound = session.outbound.clone();
            let (sender, receiver) = oneshot::channel();
            inner.pending.insert(
                request_id.clone(),
                PendingRelayRequest {
                    device_id: device_id.clone(),
                    sender,
                },
            );
            (outbound, receiver)
        };

        let text = serde_json::to_string(&request)
            .map_err(|err| RelayError::RequestEncode(err.to_string()))?;
        if outbound.0.send(text).await.is_err() {
            self.remove_pending(request_id.as_str()).await;
            return Err(RelayError::Offline);
        }

        match tokio::time::timeout(timeout_duration, outbound.1).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(RelayError::ResponseChannelClosed),
            Err(_) => {
                self.remove_pending(request_id.as_str()).await;
                Err(RelayError::Timeout)
            }
        }
    }

    pub async fn send(&self, request: RelayRequest) -> Result<(), RelayError> {
        let device_id = request.device_id.clone();
        let outbound = {
            let inner = self.inner.lock().await;
            let Some(session) = inner.sessions.get(device_id.as_str()) else {
                return Err(RelayError::Offline);
            };
            if session.owner_user_id != request.owner_user_id {
                return Err(RelayError::Offline);
            }
            session.outbound.clone()
        };

        let text = serde_json::to_string(&request)
            .map_err(|err| RelayError::RequestEncode(err.to_string()))?;
        outbound.send(text).await.map_err(|_| RelayError::Offline)
    }

    pub async fn subscribe_terminal_session(
        &self,
        terminal_session_id: &str,
    ) -> broadcast::Receiver<TerminalRelayEvent> {
        let mut inner = self.inner.lock().await;
        let sender = inner
            .terminal_events
            .entry(terminal_session_id.to_string())
            .or_insert_with(|| broadcast::channel(4096).0);
        sender.subscribe()
    }

    pub async fn drop_terminal_session(&self, terminal_session_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.terminal_events.remove(terminal_session_id);
    }

    pub async fn handle_inbound_text(&self, text: &str) -> Result<bool, String> {
        let value = match serde_json::from_str::<Value>(text) {
            Ok(value) => value,
            Err(_) => return Ok(false),
        };
        let message_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if matches!(
            message_type,
            "terminal_output"
                | "terminal_snapshot"
                | "terminal_exit"
                | "terminal_state"
                | "terminal_error"
        ) {
            let event: InboundTerminalEvent =
                serde_json::from_value(value).map_err(|err| err.to_string())?;
            return Ok(self.publish_terminal_event(event).await);
        }
        if !matches!(
            message_type,
            "sandbox_response"
                | "mcp"
                | "model_runtime_response"
                | "terminal_response"
                | "terminal_session_create_response"
                | "terminal_close_response"
                | "plugin_prepare_response"
                | "plugin_execute_response"
                | "plugin_cancel_response"
                | "plugin_ui_asset_response"
                | "plugin_artifact_list_response"
                | "plugin_artifact_read_response"
                | "plugin_artifact_create_response"
                | "plugin_artifact_update_response"
                | "relay_response"
        ) {
            return Ok(false);
        }
        let inbound: InboundRelayResponse =
            serde_json::from_value(value).map_err(|err| err.to_string())?;
        let status = inbound.status.unwrap_or(200);
        let response = RelayResponse {
            request_id: inbound.request_id.clone(),
            status,
            headers: inbound.headers.unwrap_or_default(),
            body: inbound.body.unwrap_or_else(default_body),
        };
        Ok(self.complete_response(response).await)
    }

    async fn complete_response(&self, response: RelayResponse) -> bool {
        let sender = {
            let mut inner = self.inner.lock().await;
            inner
                .pending
                .remove(response.request_id.as_str())
                .map(|pending| pending.sender)
        };
        match sender {
            Some(sender) => sender.send(response).is_ok(),
            None => false,
        }
    }

    async fn remove_pending(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending.remove(request_id);
    }

    async fn publish_terminal_event(&self, inbound: InboundTerminalEvent) -> bool {
        let body = inbound.body.unwrap_or_else(|| {
            let mut body = serde_json::Map::new();
            if let Some(data) = inbound.data {
                body.insert("data".to_string(), Value::String(data));
            }
            if let Some(code) = inbound.code {
                body.insert("code".to_string(), Value::Number(code.into()));
            }
            if let Some(busy) = inbound.busy {
                body.insert("busy".to_string(), Value::Bool(busy));
            }
            if let Some(error) = inbound.error {
                body.insert("error".to_string(), Value::String(error));
            }
            Value::Object(body)
        });
        let event = TerminalRelayEvent {
            message_type: inbound.message_type,
            terminal_session_id: inbound.terminal_session_id.clone(),
            body,
        };
        let sender = {
            let mut inner = self.inner.lock().await;
            inner
                .terminal_events
                .entry(inbound.terminal_session_id)
                .or_insert_with(|| broadcast::channel(4096).0)
                .clone()
        };
        sender.send(event).is_ok()
    }
}

impl RelayError {
    pub fn message(&self) -> String {
        match self {
            Self::Offline => "Local Connector is offline".to_string(),
            Self::Timeout => "Local Connector relay request timed out".to_string(),
            Self::RequestEncode(err) => {
                format!("encode Local Connector relay request failed: {err}")
            }
            Self::ResponseChannelClosed => {
                "Local Connector relay response channel closed".to_string()
            }
        }
    }
}

fn default_body() -> Value {
    Value::Null
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn plugin_response_completes_pending_relay_request() {
        let relay = ConnectorRelay::default();
        let (outbound, mut inbound) = mpsc::channel(1);
        relay
            .register_session(
                "device-1".to_string(),
                "owner-1".to_string(),
                "session-1".to_string(),
                outbound,
            )
            .await;
        let dispatch_relay = relay.clone();
        let dispatch = tokio::spawn(async move {
            dispatch_relay
                .dispatch(
                    RelayRequest {
                        message_type: "plugin_prepare_request".to_string(),
                        request_id: "request-1".to_string(),
                        owner_user_id: "owner-1".to_string(),
                        device_id: "device-1".to_string(),
                        workspace_id: "workspace-1".to_string(),
                        method: "POST".to_string(),
                        path: "/plugins/prepare".to_string(),
                        headers: BTreeMap::new(),
                        body: serde_json::json!({"plugin_id":"plugin-browser"}),
                    },
                    Duration::from_secs(1),
                )
                .await
        });
        let outbound = inbound.recv().await.expect("Plugin relay request");
        assert!(outbound.contains("plugin_prepare_request"));
        assert!(relay
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-1","status":200,"body":{"adapter_session_id":"adapter-1"}}"#,
            )
            .await
            .expect("Plugin relay response"));

        let response = dispatch.await.expect("dispatch task").expect("response");
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["adapter_session_id"].as_str(),
            Some("adapter-1")
        );

        let asset_relay = relay.clone();
        let asset_dispatch = tokio::spawn(async move {
            asset_relay
                .dispatch(
                    RelayRequest {
                        message_type: "plugin_ui_asset_request".to_string(),
                        request_id: "request-2".to_string(),
                        owner_user_id: "owner-1".to_string(),
                        device_id: "device-1".to_string(),
                        workspace_id: String::new(),
                        method: "POST".to_string(),
                        path: "/plugins/ui/assets".to_string(),
                        headers: BTreeMap::new(),
                        body: serde_json::json!({"relative_path":"./ui/index.html"}),
                    },
                    Duration::from_secs(1),
                )
                .await
        });
        let outbound = inbound.recv().await.expect("Plugin UI asset relay request");
        assert!(outbound.contains("plugin_ui_asset_request"));
        assert!(relay
            .handle_inbound_text(
                r#"{"type":"plugin_ui_asset_response","request_id":"request-2","status":200,"body":{"kind":"entrypoint","body_base64":"PGh0bWw+"}}"#,
            )
            .await
            .expect("Plugin UI asset relay response"));
        let response = asset_dispatch
            .await
            .expect("asset dispatch task")
            .expect("asset response");
        assert_eq!(response.status, 200);
        assert_eq!(response.body["kind"].as_str(), Some("entrypoint"));

        for (index, action) in ["list", "read", "create", "update"].into_iter().enumerate() {
            let request_id = format!("artifact-request-{index}");
            let request_type = format!("plugin_artifact_{action}_request");
            let response_type = format!("plugin_artifact_{action}_response");
            let artifact_relay = relay.clone();
            let dispatch_request_id = request_id.clone();
            let dispatch_request_type = request_type.clone();
            let dispatch = tokio::spawn(async move {
                artifact_relay
                    .dispatch(
                        RelayRequest {
                            message_type: dispatch_request_type,
                            request_id: dispatch_request_id,
                            owner_user_id: "owner-1".to_string(),
                            device_id: "device-1".to_string(),
                            workspace_id: "workspace-1".to_string(),
                            method: "POST".to_string(),
                            path: format!("/plugins/artifacts/{action}"),
                            headers: BTreeMap::new(),
                            body: serde_json::json!({"access":{"run_id":"run-1"}}),
                        },
                        Duration::from_secs(1),
                    )
                    .await
            });
            let outbound = inbound.recv().await.expect("Plugin Artifact relay request");
            assert!(outbound.contains(request_type.as_str()));
            assert!(relay
                .handle_inbound_text(
                    serde_json::json!({
                        "type": response_type,
                        "request_id": request_id,
                        "status": 200,
                        "body": {"action": action},
                    })
                    .to_string()
                    .as_str(),
                )
                .await
                .expect("Plugin Artifact relay response"));
            let response = dispatch
                .await
                .expect("Artifact dispatch task")
                .expect("Artifact response");
            assert_eq!(response.status, 200);
            assert_eq!(response.body["action"].as_str(), Some(action));
        }
    }
}
