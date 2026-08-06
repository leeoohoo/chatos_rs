// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};
use uuid::Uuid;

use crate::managed_config::RelayRuntimeLimits;
use crate::models::LocalConnectorRelayStats;
use crate::pressure::PlatformPressureLevel;
use crate::relay_signature::PlatformRelaySigner;
use crate::valkey_coordination::{RelayCorrelation, ValkeyCoordinator};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_signature_key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_signature_alg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_timestamp: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_nonce: Option<String>,
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
        platform_signature: None,
        platform_signature_key_id: None,
        platform_signature_alg: None,
        platform_timestamp: None,
        platform_nonce: None,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalRelayEvent {
    #[serde(rename = "type")]
    pub message_type: String,
    pub terminal_session_id: String,
    #[serde(default = "default_body")]
    pub body: Value,
}

pub struct TerminalRelaySubscription {
    pub id: String,
    pub events: broadcast::Receiver<TerminalRelayEvent>,
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
    Signing(String),
    TooManyPendingRequests { device_id: String, limit: usize },
    DuplicateRequestId(String),
    Coordination(String),
    ResponseChannelClosed,
}

#[derive(Clone)]
pub struct ConnectorRelay {
    runtime: Arc<RwLock<RelayRuntimeConfig>>,
    inner: Arc<Mutex<RelayState>>,
    distributed: Option<DistributedRelay>,
    platform_pressure_critical: Arc<AtomicBool>,
}

#[derive(Clone)]
struct DistributedRelay {
    instance_id: String,
    coordinator: ValkeyCoordinator,
    correlation_grace_ttl: Duration,
    delivery_ack_timeout: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum InterInstanceRelayMessage {
    Dispatch {
        request: RelayRequest,
        requester_instance_id: String,
    },
    Send {
        request: RelayRequest,
        requester_instance_id: String,
    },
    Response {
        response: RelayResponse,
    },
    TerminalEvent {
        event: TerminalRelayEvent,
    },
}

#[derive(Clone)]
struct RelayRuntimeConfig {
    limits: RelayRuntimeLimits,
    signer: Option<Arc<PlatformRelaySigner>>,
}

impl Default for ConnectorRelay {
    fn default() -> Self {
        Self::new(None, RelayRuntimeLimits::default())
    }
}

#[derive(Default)]
struct RelayState {
    sessions: HashMap<String, ActiveConnectorSession>,
    pending: HashMap<String, PendingRelayRequest>,
    terminal_events: HashMap<String, broadcast::Sender<TerminalRelayEvent>>,
    terminal_subscriptions: HashMap<String, HashSet<String>>,
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
    pub(crate) fn new(
        signer: Option<Arc<PlatformRelaySigner>>,
        limits: RelayRuntimeLimits,
    ) -> Self {
        Self {
            runtime: Arc::new(RwLock::new(RelayRuntimeConfig { limits, signer })),
            inner: Arc::new(Mutex::new(RelayState::default())),
            distributed: None,
            platform_pressure_critical: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn new_distributed(
        signer: Option<Arc<PlatformRelaySigner>>,
        limits: RelayRuntimeLimits,
        instance_id: String,
        coordinator: ValkeyCoordinator,
        correlation_grace_ttl: Duration,
        delivery_ack_timeout: Duration,
    ) -> Self {
        Self {
            runtime: Arc::new(RwLock::new(RelayRuntimeConfig { limits, signer })),
            inner: Arc::new(Mutex::new(RelayState::default())),
            distributed: Some(DistributedRelay {
                instance_id,
                coordinator,
                correlation_grace_ttl,
                delivery_ack_timeout,
            }),
            platform_pressure_critical: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn set_platform_pressure_level(&self, level: PlatformPressureLevel) {
        self.platform_pressure_critical
            .store(level == PlatformPressureLevel::Critical, Ordering::Relaxed);
    }

    pub(crate) async fn new_terminal_sessions_paused(&self) -> bool {
        if self.platform_pressure_critical.load(Ordering::Relaxed) {
            return true;
        }
        let limits = self.runtime_config().limits;
        self.inner.lock().await.terminal_subscriptions.len()
            >= limits.terminal_new_session_soft_limit
    }

    pub(crate) fn update_runtime_config(
        &self,
        signer: Option<Arc<PlatformRelaySigner>>,
        limits: RelayRuntimeLimits,
    ) {
        let mut runtime = self
            .runtime
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *runtime = RelayRuntimeConfig { limits, signer };
    }

    pub(crate) fn active_signer(&self) -> Option<Arc<PlatformRelaySigner>> {
        self.runtime_config().signer
    }

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
        let request = self.sign_request(request)?;
        let local_outbound = self
            .local_session_outbound(device_id.as_str(), request.owner_user_id.as_str())
            .await;
        let remote_instance = if local_outbound.is_none() {
            Some(self.remote_instance_for_request(&request).await?)
        } else {
            None
        };
        let receiver = self
            .insert_pending_request(request_id.as_str(), device_id.as_str())
            .await?;

        if let Some(outbound) = local_outbound {
            let text = match serde_json::to_string(&request) {
                Ok(text) => text,
                Err(error) => {
                    self.remove_pending(request_id.as_str()).await;
                    return Err(RelayError::RequestEncode(error.to_string()));
                }
            };
            if outbound.send(text).await.is_err() {
                self.remove_pending(request_id.as_str()).await;
                return Err(RelayError::Offline);
            }
        } else {
            let distributed = self.distributed.as_ref().ok_or(RelayError::Offline)?;
            let correlation = RelayCorrelation {
                requester_instance_id: distributed.instance_id.clone(),
                device_id: device_id.clone(),
            };
            let correlation_ttl =
                timeout_duration.saturating_add(distributed.correlation_grace_ttl);
            let registered = match distributed
                .coordinator
                .register_relay_correlation(request_id.as_str(), &correlation, correlation_ttl)
                .await
            {
                Ok(registered) => registered,
                Err(error) => {
                    self.remove_pending(request_id.as_str()).await;
                    return Err(RelayError::Coordination(error));
                }
            };
            if !registered {
                self.remove_pending(request_id.as_str()).await;
                return Err(RelayError::DuplicateRequestId(request_id));
            }
            let target_instance = remote_instance.expect("remote instance resolved above");
            let publish_result = distributed
                .coordinator
                .publish_instance_message(
                    target_instance.as_str(),
                    &InterInstanceRelayMessage::Dispatch {
                        request,
                        requester_instance_id: distributed.instance_id.clone(),
                    },
                )
                .await;
            if let Err(error) = publish_result {
                self.remove_pending(request_id.as_str()).await;
                let _ = distributed
                    .coordinator
                    .delete_relay_correlation(request_id.as_str(), distributed.instance_id.as_str())
                    .await;
                return Err(RelayError::Coordination(error));
            }
        }

        match tokio::time::timeout(timeout_duration, receiver).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                self.cleanup_request(request_id.as_str()).await;
                Err(RelayError::ResponseChannelClosed)
            }
            Err(_) => {
                self.cleanup_request(request_id.as_str()).await;
                Err(RelayError::Timeout)
            }
        }
    }

    pub async fn send(&self, request: RelayRequest) -> Result<(), RelayError> {
        let request = self.sign_request(request)?;
        if let Some(outbound) = self
            .local_session_outbound(request.device_id.as_str(), request.owner_user_id.as_str())
            .await
        {
            let text = serde_json::to_string(&request)
                .map_err(|err| RelayError::RequestEncode(err.to_string()))?;
            return outbound.send(text).await.map_err(|_| RelayError::Offline);
        }
        let target_instance = self.remote_instance_for_request(&request).await?;
        let distributed = self.distributed.as_ref().ok_or(RelayError::Offline)?;
        let request_id = request.request_id.clone();
        let receiver = self
            .insert_pending_request(request_id.as_str(), request.device_id.as_str())
            .await?;
        let correlation = RelayCorrelation {
            requester_instance_id: distributed.instance_id.clone(),
            device_id: request.device_id.clone(),
        };
        let correlation_ttl = distributed
            .delivery_ack_timeout
            .saturating_add(distributed.correlation_grace_ttl);
        let registered = match distributed
            .coordinator
            .register_relay_correlation(request_id.as_str(), &correlation, correlation_ttl)
            .await
        {
            Ok(registered) => registered,
            Err(error) => {
                self.remove_pending(request_id.as_str()).await;
                return Err(RelayError::Coordination(error));
            }
        };
        if !registered {
            self.remove_pending(request_id.as_str()).await;
            return Err(RelayError::DuplicateRequestId(request_id));
        }
        let publish_result = distributed
            .coordinator
            .publish_instance_message(
                target_instance.as_str(),
                &InterInstanceRelayMessage::Send {
                    request,
                    requester_instance_id: distributed.instance_id.clone(),
                },
            )
            .await;
        if let Err(error) = publish_result {
            self.remove_pending(request_id.as_str()).await;
            let _ = distributed
                .coordinator
                .delete_relay_correlation(request_id.as_str(), distributed.instance_id.as_str())
                .await;
            return Err(RelayError::Coordination(error));
        }
        match tokio::time::timeout(distributed.delivery_ack_timeout, receiver).await {
            Ok(Ok(response)) if response.status < 400 => Ok(()),
            Ok(Ok(response)) => Err(RelayError::Coordination(
                response
                    .body
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("remote Local Connector rejected relay delivery")
                    .to_string(),
            )),
            Ok(Err(_)) => {
                self.cleanup_request(request_id.as_str()).await;
                Err(RelayError::ResponseChannelClosed)
            }
            Err(_) => {
                self.cleanup_request(request_id.as_str()).await;
                Err(RelayError::Timeout)
            }
        }
    }

    pub async fn stats(&self) -> LocalConnectorRelayStats {
        let runtime = self.runtime_config();
        let inner = self.inner.lock().await;
        let terminal_ws_subscribers = inner
            .terminal_events
            .values()
            .map(broadcast::Sender::receiver_count)
            .sum();
        LocalConnectorRelayStats {
            active_device_sessions: inner.sessions.len(),
            pending_relay_requests: inner.pending.len(),
            terminal_sessions: inner.terminal_events.len(),
            terminal_ws_subscribers,
            max_pending_requests_per_device: runtime.limits.max_pending_requests_per_device,
            terminal_max_event_bytes: runtime.limits.terminal_max_event_bytes,
            terminal_event_channel_capacity: runtime.limits.terminal_event_channel_capacity,
            terminal_max_active_sessions: runtime.limits.terminal_max_active_sessions,
            terminal_new_session_soft_limit: runtime.limits.terminal_new_session_soft_limit,
            new_terminal_sessions_paused: self.platform_pressure_critical.load(Ordering::Relaxed)
                || inner.terminal_subscriptions.len()
                    >= runtime.limits.terminal_new_session_soft_limit,
            terminal_max_subscribers_per_session: runtime
                .limits
                .terminal_max_subscribers_per_session,
            relay_signing_enabled: runtime.signer.is_some(),
        }
    }

    fn sign_request(&self, mut request: RelayRequest) -> Result<RelayRequest, RelayError> {
        if let Some(signer) = self.runtime_config().signer {
            signer
                .sign_request(&mut request)
                .map_err(RelayError::Signing)?;
        }
        Ok(request)
    }

    pub async fn subscribe_terminal_session(
        &self,
        terminal_session_id: &str,
    ) -> Result<TerminalRelaySubscription, String> {
        let subscription_id = Uuid::new_v4().to_string();
        let events = {
            let mut inner = self.inner.lock().await;
            let limits = self.runtime_config().limits;
            let current_subscriber_count = inner
                .terminal_subscriptions
                .get(terminal_session_id)
                .map(HashSet::len)
                .unwrap_or(0);
            if current_subscriber_count == 0
                && self.platform_pressure_critical.load(Ordering::Relaxed)
            {
                return Err(
                    "Local Connector is temporarily pausing new terminal sessions while platform pressure is critical"
                        .to_string(),
                );
            }
            if current_subscriber_count == 0
                && inner.terminal_subscriptions.len() >= limits.terminal_max_active_sessions
            {
                return Err(format!(
                    "Local Connector terminal session capacity is exhausted at {} active sessions",
                    limits.terminal_max_active_sessions
                ));
            }
            if current_subscriber_count == 0
                && inner.terminal_subscriptions.len() >= limits.terminal_new_session_soft_limit
            {
                return Err(format!(
                    "Local Connector is temporarily pausing new terminal sessions at the soft pressure limit of {} active sessions",
                    limits.terminal_new_session_soft_limit
                ));
            }
            if current_subscriber_count >= limits.terminal_max_subscribers_per_session {
                return Err(format!(
                    "Local Connector terminal subscriber capacity is exhausted at {} subscribers for session {terminal_session_id}",
                    limits.terminal_max_subscribers_per_session
                ));
            }
            let events = inner
                .terminal_events
                .entry(terminal_session_id.to_string())
                .or_insert_with(|| broadcast::channel(limits.terminal_event_channel_capacity).0)
                .subscribe();
            inner
                .terminal_subscriptions
                .entry(terminal_session_id.to_string())
                .or_default()
                .insert(subscription_id.clone());
            events
        };
        if let Some(distributed) = self.distributed.as_ref() {
            if let Err(error) = distributed
                .coordinator
                .register_terminal_subscriber(terminal_session_id, distributed.instance_id.as_str())
                .await
            {
                self.remove_local_terminal_subscription(
                    terminal_session_id,
                    subscription_id.as_str(),
                )
                .await;
                return Err(error);
            }
        }
        Ok(TerminalRelaySubscription {
            id: subscription_id,
            events,
        })
    }

    pub async fn refresh_terminal_subscription(
        &self,
        terminal_session_id: &str,
        subscription_id: &str,
    ) -> Result<bool, String> {
        let active = {
            let inner = self.inner.lock().await;
            inner
                .terminal_subscriptions
                .get(terminal_session_id)
                .is_some_and(|subscriptions| subscriptions.contains(subscription_id))
        };
        if !active {
            return Ok(false);
        }
        if let Some(distributed) = self.distributed.as_ref() {
            distributed
                .coordinator
                .register_terminal_subscriber(terminal_session_id, distributed.instance_id.as_str())
                .await?;
        }
        Ok(true)
    }

    pub async fn drop_terminal_subscription(
        &self,
        terminal_session_id: &str,
        subscription_id: &str,
    ) -> Result<(), String> {
        let removed_last = self
            .remove_local_terminal_subscription(terminal_session_id, subscription_id)
            .await;
        let Some(distributed) = self.distributed.as_ref() else {
            return Ok(());
        };
        if !removed_last {
            return Ok(());
        }
        distributed
            .coordinator
            .unregister_terminal_subscriber(terminal_session_id, distributed.instance_id.as_str())
            .await?;
        let still_active = {
            let inner = self.inner.lock().await;
            inner
                .terminal_subscriptions
                .get(terminal_session_id)
                .is_some_and(|subscriptions| !subscriptions.is_empty())
        };
        if still_active {
            distributed
                .coordinator
                .register_terminal_subscriber(terminal_session_id, distributed.instance_id.as_str())
                .await?;
        }
        Ok(())
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
            return self.publish_terminal_event(event).await;
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
        if self.complete_response(response.clone()).await {
            return Ok(true);
        }
        self.route_remote_response(response).await
    }

    pub(crate) async fn handle_inter_instance_message(
        &self,
        message: InterInstanceRelayMessage,
    ) -> Result<(), String> {
        match message {
            InterInstanceRelayMessage::Dispatch {
                request,
                requester_instance_id,
            } => {
                if let Err(error) = self.send_to_local_session(&request).await {
                    let distributed = self
                        .distributed
                        .as_ref()
                        .ok_or_else(|| "distributed relay is not configured".to_string())?;
                    distributed
                        .coordinator
                        .publish_instance_message(
                            requester_instance_id.as_str(),
                            &InterInstanceRelayMessage::Response {
                                response: RelayResponse {
                                    request_id: request.request_id,
                                    status: 503,
                                    headers: BTreeMap::new(),
                                    body: serde_json::json!({ "error": error.message() }),
                                },
                            },
                        )
                        .await?;
                }
                Ok(())
            }
            InterInstanceRelayMessage::Send {
                request,
                requester_instance_id,
            } => {
                let delivery = self.send_to_local_session(&request).await;
                let (status, body) = match delivery {
                    Ok(()) => (202, serde_json::json!({ "delivered": true })),
                    Err(error) => (503, serde_json::json!({ "error": error.message() })),
                };
                let distributed = self
                    .distributed
                    .as_ref()
                    .ok_or_else(|| "distributed relay is not configured".to_string())?;
                distributed
                    .coordinator
                    .publish_instance_message(
                        requester_instance_id.as_str(),
                        &InterInstanceRelayMessage::Response {
                            response: RelayResponse {
                                request_id: request.request_id,
                                status,
                                headers: BTreeMap::new(),
                                body,
                            },
                        },
                    )
                    .await
            }
            InterInstanceRelayMessage::Response { response } => {
                let request_id = response.request_id.clone();
                self.complete_response(response).await;
                if let Some(distributed) = self.distributed.as_ref() {
                    distributed
                        .coordinator
                        .delete_relay_correlation(
                            request_id.as_str(),
                            distributed.instance_id.as_str(),
                        )
                        .await?;
                }
                Ok(())
            }
            InterInstanceRelayMessage::TerminalEvent { event } => {
                self.publish_local_terminal_event(event).await;
                Ok(())
            }
        }
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

    async fn route_remote_response(&self, response: RelayResponse) -> Result<bool, String> {
        let Some(distributed) = self.distributed.as_ref() else {
            return Ok(false);
        };
        let Some(correlation) = distributed
            .coordinator
            .relay_correlation(response.request_id.as_str())
            .await?
        else {
            return Ok(false);
        };
        if let Err(error) = distributed
            .coordinator
            .publish_instance_message(
                correlation.requester_instance_id.as_str(),
                &InterInstanceRelayMessage::Response {
                    response: response.clone(),
                },
            )
            .await
        {
            tracing::warn!(
                request_id = response.request_id.as_str(),
                requester_instance_id = correlation.requester_instance_id.as_str(),
                error = error.as_str(),
                "route Local Connector relay response to requester instance failed"
            );
            let _ = distributed
                .coordinator
                .delete_relay_correlation(
                    response.request_id.as_str(),
                    correlation.requester_instance_id.as_str(),
                )
                .await;
        }
        Ok(true)
    }

    async fn send_to_local_session(&self, request: &RelayRequest) -> Result<(), RelayError> {
        let Some(outbound) = self
            .local_session_outbound(request.device_id.as_str(), request.owner_user_id.as_str())
            .await
        else {
            return Err(RelayError::Offline);
        };
        let text = serde_json::to_string(request)
            .map_err(|error| RelayError::RequestEncode(error.to_string()))?;
        outbound.send(text).await.map_err(|_| RelayError::Offline)
    }

    async fn local_session_outbound(
        &self,
        device_id: &str,
        owner_user_id: &str,
    ) -> Option<mpsc::Sender<String>> {
        let inner = self.inner.lock().await;
        inner.sessions.get(device_id).and_then(|session| {
            (session.owner_user_id == owner_user_id).then(|| session.outbound.clone())
        })
    }

    async fn remote_instance_for_request(
        &self,
        request: &RelayRequest,
    ) -> Result<String, RelayError> {
        let distributed = self.distributed.as_ref().ok_or(RelayError::Offline)?;
        let presence = distributed
            .coordinator
            .device_presence(request.device_id.as_str())
            .await
            .map_err(RelayError::Coordination)?
            .ok_or(RelayError::Offline)?;
        if presence.owner_user_id != request.owner_user_id
            || presence.instance_id == distributed.instance_id
        {
            return Err(RelayError::Offline);
        }
        Ok(presence.instance_id)
    }

    async fn insert_pending_request(
        &self,
        request_id: &str,
        device_id: &str,
    ) -> Result<oneshot::Receiver<RelayResponse>, RelayError> {
        let runtime = self.runtime_config();
        let mut inner = self.inner.lock().await;
        if inner.pending.contains_key(request_id) {
            return Err(RelayError::DuplicateRequestId(request_id.to_string()));
        }
        let pending_count = inner
            .pending
            .values()
            .filter(|pending| pending.device_id == device_id)
            .count();
        if pending_count >= runtime.limits.max_pending_requests_per_device {
            return Err(RelayError::TooManyPendingRequests {
                device_id: device_id.to_string(),
                limit: runtime.limits.max_pending_requests_per_device,
            });
        }
        let (sender, receiver) = oneshot::channel();
        inner.pending.insert(
            request_id.to_string(),
            PendingRelayRequest {
                device_id: device_id.to_string(),
                sender,
            },
        );
        Ok(receiver)
    }

    async fn remove_pending(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending.remove(request_id);
    }

    async fn cleanup_request(&self, request_id: &str) {
        self.remove_pending(request_id).await;
        if let Some(distributed) = self.distributed.as_ref() {
            let _ = distributed
                .coordinator
                .delete_relay_correlation(request_id, distributed.instance_id.as_str())
                .await;
        }
    }

    async fn remove_local_terminal_subscription(
        &self,
        terminal_session_id: &str,
        subscription_id: &str,
    ) -> bool {
        let mut inner = self.inner.lock().await;
        let Some(subscriptions) = inner.terminal_subscriptions.get_mut(terminal_session_id) else {
            return false;
        };
        if !subscriptions.remove(subscription_id) || !subscriptions.is_empty() {
            return false;
        }
        inner.terminal_subscriptions.remove(terminal_session_id);
        inner.terminal_events.remove(terminal_session_id);
        true
    }

    async fn publish_terminal_event(&self, inbound: InboundTerminalEvent) -> Result<bool, String> {
        let original_message_type = inbound.message_type.clone();
        let terminal_session_id = inbound.terminal_session_id.clone();
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
        let event = self.normalize_terminal_event(TerminalRelayEvent {
            message_type: original_message_type,
            terminal_session_id: terminal_session_id.clone(),
            body,
        });
        let delivered_locally = self.publish_local_terminal_event(event.clone()).await;
        let Some(distributed) = self.distributed.as_ref() else {
            return Ok(delivered_locally);
        };
        let subscriber_instances = distributed
            .coordinator
            .terminal_subscriber_instances(terminal_session_id.as_str())
            .await?;
        for subscriber_instance in subscriber_instances {
            if subscriber_instance == distributed.instance_id {
                continue;
            }
            if let Err(error) = distributed
                .coordinator
                .publish_instance_message(
                    subscriber_instance.as_str(),
                    &InterInstanceRelayMessage::TerminalEvent {
                        event: event.clone(),
                    },
                )
                .await
            {
                tracing::warn!(
                    terminal_session_id = terminal_session_id.as_str(),
                    subscriber_instance = subscriber_instance.as_str(),
                    error = error.as_str(),
                    "route Local Connector terminal event to subscriber instance failed"
                );
            }
        }
        Ok(true)
    }

    async fn publish_local_terminal_event(&self, event: TerminalRelayEvent) -> bool {
        let sender = {
            let inner = self.inner.lock().await;
            inner
                .terminal_events
                .get(event.terminal_session_id.as_str())
                .cloned()
        };
        sender.is_some_and(|sender| sender.send(event).is_ok())
    }

    fn normalize_terminal_event(&self, event: TerminalRelayEvent) -> TerminalRelayEvent {
        let runtime = self.runtime_config();
        let within_budget = serde_json::to_vec(&event)
            .map(|bytes| bytes.len() <= runtime.limits.terminal_max_event_bytes)
            .unwrap_or(false);
        if within_budget {
            return event;
        }
        TerminalRelayEvent {
            message_type: "terminal_error".to_string(),
            terminal_session_id: event.terminal_session_id,
            body: serde_json::json!({
                "error": format!(
                    "terminal relay event exceeded {} bytes and was dropped",
                    runtime.limits.terminal_max_event_bytes
                ),
                "original_message_type": event.message_type,
            }),
        }
    }

    fn runtime_config(&self) -> RelayRuntimeConfig {
        self.runtime
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
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
            Self::Signing(err) => format!("sign Local Connector relay request failed: {err}"),
            Self::TooManyPendingRequests { device_id, limit } => format!(
                "too many Local Connector relay requests are pending for device {device_id} (limit: {limit})"
            ),
            Self::DuplicateRequestId(request_id) => {
                format!("duplicate Local Connector relay request id: {request_id}")
            }
            Self::Coordination(err) => {
                format!("Local Connector distributed relay coordination failed: {err}")
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
    use futures::StreamExt;

    use super::*;
    use crate::valkey_coordination::DevicePresence;

    fn relay_request(request_id: &str) -> RelayRequest {
        RelayRequest {
            message_type: "plugin_prepare_request".to_string(),
            request_id: request_id.to_string(),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            workspace_id: "workspace-1".to_string(),
            method: "POST".to_string(),
            path: "/plugins/prepare".to_string(),
            headers: BTreeMap::new(),
            body: serde_json::json!({"plugin_id":"plugin-browser"}),
            platform_signature: None,
            platform_signature_key_id: None,
            platform_signature_alg: None,
            platform_timestamp: None,
            platform_nonce: None,
        }
    }

    async fn spawn_test_instance_listener(
        coordinator: ValkeyCoordinator,
        instance_id: String,
        relay: ConnectorRelay,
    ) -> tokio::task::JoinHandle<()> {
        let mut pubsub = coordinator
            .subscribe_instance(instance_id.as_str())
            .await
            .expect("subscribe test relay instance");
        tokio::spawn(async move {
            let mut messages = pubsub.on_message();
            while let Some(message) = messages.next().await {
                let payload = message
                    .get_payload::<String>()
                    .expect("decode test relay message");
                let message = serde_json::from_str::<InterInstanceRelayMessage>(&payload)
                    .expect("parse test relay message");
                relay
                    .handle_inter_instance_message(message)
                    .await
                    .expect("handle test relay message");
            }
        })
    }

    #[test]
    fn inter_instance_dispatch_message_has_versioned_tagged_shape() {
        let message = InterInstanceRelayMessage::Dispatch {
            request: relay_request("request-1"),
            requester_instance_id: "local-connector-requester".to_string(),
        };

        let value = serde_json::to_value(&message).expect("serialize dispatch message");
        assert_eq!(value["type"], "dispatch");
        assert_eq!(value["requester_instance_id"], "local-connector-requester");
        assert_eq!(value["request"]["request_id"], "request-1");
        let decoded: InterInstanceRelayMessage =
            serde_json::from_value(value).expect("deserialize dispatch message");
        assert!(matches!(
            decoded,
            InterInstanceRelayMessage::Dispatch { .. }
        ));
    }

    #[test]
    fn inter_instance_send_message_keeps_delivery_ack_route() {
        let message = InterInstanceRelayMessage::Send {
            request: relay_request("request-2"),
            requester_instance_id: "local-connector-requester".to_string(),
        };

        let value = serde_json::to_value(&message).expect("serialize send message");
        assert_eq!(value["type"], "send");
        assert_eq!(value["requester_instance_id"], "local-connector-requester");
        assert_eq!(value["request"]["request_id"], "request-2");
    }

    #[test]
    fn inter_instance_response_supports_delivery_ack_shape() {
        let message = InterInstanceRelayMessage::Response {
            response: RelayResponse {
                request_id: "request-2".to_string(),
                status: 202,
                headers: BTreeMap::new(),
                body: serde_json::json!({"delivered": true}),
            },
        };

        let value = serde_json::to_value(&message).expect("serialize response message");
        assert_eq!(value["type"], "response");
        assert_eq!(value["response"]["request_id"], "request-2");
        assert_eq!(value["response"]["status"], 202);
        assert_eq!(value["response"]["body"]["delivered"], true);
    }

    #[test]
    fn inter_instance_terminal_event_contains_ephemeral_body_only() {
        let message = InterInstanceRelayMessage::TerminalEvent {
            event: TerminalRelayEvent {
                message_type: "terminal_output".to_string(),
                terminal_session_id: "terminal-1".to_string(),
                body: serde_json::json!({"data": "hello"}),
            },
        };

        let value = serde_json::to_value(&message).expect("serialize terminal event");
        assert_eq!(value["type"], "terminal_event");
        assert_eq!(value["event"]["terminal_session_id"], "terminal-1");
        assert_eq!(value["event"]["body"]["data"], "hello");
        assert!(value.get("queue_name").is_none());
    }

    #[tokio::test]
    #[ignore = "requires CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL"]
    async fn real_valkey_routes_commands_responses_and_terminal_events_across_instances() {
        let valkey_url = std::env::var("CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL")
            .expect("CHATOS_LOCAL_CONNECTOR_TEST_VALKEY_URL must be set");
        let test_id = Uuid::new_v4().to_string();
        let key_prefix = format!("test:local-connector:{test_id}");
        let instance_a = format!("local-connector-a-{test_id}");
        let instance_b = format!("local-connector-b-{test_id}");
        let coordinator_a = ValkeyCoordinator::connect(
            valkey_url.as_str(),
            key_prefix.as_str(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .await
        .expect("connect first test coordinator");
        let coordinator_b = ValkeyCoordinator::connect(
            valkey_url.as_str(),
            key_prefix.as_str(),
            Duration::from_secs(5),
            Duration::from_secs(5),
        )
        .await
        .expect("connect second test coordinator");
        let relay_a = ConnectorRelay::new_distributed(
            None,
            RelayRuntimeLimits::default(),
            instance_a.clone(),
            coordinator_a.clone(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let relay_b = ConnectorRelay::new_distributed(
            None,
            RelayRuntimeLimits::default(),
            instance_b.clone(),
            coordinator_b.clone(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let listener_a = spawn_test_instance_listener(
            coordinator_a.clone(),
            instance_a.clone(),
            relay_a.clone(),
        )
        .await;
        let listener_b = spawn_test_instance_listener(
            coordinator_b.clone(),
            instance_b.clone(),
            relay_b.clone(),
        )
        .await;

        let (device_outbound, mut device_inbound) = mpsc::channel(8);
        relay_b
            .register_session(
                "device-1".to_string(),
                "owner-1".to_string(),
                "session-new".to_string(),
                device_outbound,
            )
            .await;
        let stale_presence = DevicePresence {
            instance_id: instance_b.clone(),
            owner_user_id: "owner-1".to_string(),
            device_id: "device-1".to_string(),
            session_id: "session-old".to_string(),
        };
        let active_presence = DevicePresence {
            session_id: "session-new".to_string(),
            ..stale_presence.clone()
        };
        coordinator_b
            .register_device_presence(&stale_presence)
            .await
            .expect("register stale presence");
        coordinator_b
            .register_device_presence(&active_presence)
            .await
            .expect("replace active presence");
        assert!(!coordinator_b
            .unregister_device_presence(&stale_presence)
            .await
            .expect("reject stale presence cleanup"));
        assert_eq!(
            coordinator_a
                .device_presence("device-1")
                .await
                .expect("load active presence"),
            Some(active_presence.clone())
        );

        let dispatch = {
            let relay = relay_a.clone();
            tokio::spawn(async move {
                relay
                    .dispatch(relay_request("request-dispatch"), Duration::from_secs(2))
                    .await
            })
        };
        let outbound = device_inbound
            .recv()
            .await
            .expect("receive cross-instance dispatch");
        let outbound: RelayRequest =
            serde_json::from_str(&outbound).expect("parse cross-instance dispatch");
        assert_eq!(outbound.request_id, "request-dispatch");
        assert!(relay_b
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-dispatch","status":200,"body":{"adapter_session_id":"adapter-cross-instance"}}"#,
            )
            .await
            .expect("route cross-instance response"));
        let response = dispatch
            .await
            .expect("join cross-instance dispatch")
            .expect("complete cross-instance dispatch");
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["adapter_session_id"].as_str(),
            Some("adapter-cross-instance")
        );

        let send = {
            let relay = relay_a.clone();
            tokio::spawn(async move { relay.send(relay_request("request-send")).await })
        };
        let outbound = device_inbound
            .recv()
            .await
            .expect("receive cross-instance one-way request");
        let outbound: RelayRequest =
            serde_json::from_str(&outbound).expect("parse cross-instance one-way request");
        assert_eq!(outbound.request_id, "request-send");
        send.await
            .expect("join cross-instance one-way request")
            .expect("receive owner-instance delivery acknowledgement");

        let timed_out_dispatch = {
            let relay = relay_a.clone();
            tokio::spawn(async move {
                relay
                    .dispatch(relay_request("request-timeout"), Duration::from_millis(100))
                    .await
            })
        };
        let outbound = device_inbound
            .recv()
            .await
            .expect("receive request that will time out");
        let outbound: RelayRequest =
            serde_json::from_str(&outbound).expect("parse request that will time out");
        assert_eq!(outbound.request_id, "request-timeout");
        assert!(matches!(
            timed_out_dispatch
                .await
                .expect("join timed-out dispatch")
                .expect_err("missing client response must time out"),
            RelayError::Timeout
        ));
        assert_eq!(relay_a.stats().await.pending_relay_requests, 0);
        assert!(coordinator_a
            .relay_correlation("request-timeout")
            .await
            .expect("load timed-out correlation")
            .is_none());

        let terminal_subscription = relay_a
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("subscribe remote terminal session");
        let terminal_subscription_id = terminal_subscription.id;
        let mut terminal_events = terminal_subscription.events;
        assert!(relay_b
            .handle_inbound_text(
                r#"{"type":"terminal_output","terminal_session_id":"terminal-1","data":"cross-instance-output"}"#,
            )
            .await
            .expect("publish cross-instance terminal event"));
        let terminal_event = tokio::time::timeout(Duration::from_secs(2), terminal_events.recv())
            .await
            .expect("wait for cross-instance terminal event")
            .expect("receive cross-instance terminal event");
        assert_eq!(terminal_event.message_type, "terminal_output");
        assert_eq!(
            terminal_event.body["data"].as_str(),
            Some("cross-instance-output")
        );
        relay_a
            .drop_terminal_subscription("terminal-1", terminal_subscription_id.as_str())
            .await
            .expect("drop remote terminal subscription");

        let missing_subscriber_error = coordinator_a
            .publish_instance_message(
                format!("missing-{test_id}").as_str(),
                &InterInstanceRelayMessage::TerminalEvent {
                    event: TerminalRelayEvent {
                        message_type: "terminal_output".to_string(),
                        terminal_session_id: "terminal-missing".to_string(),
                        body: serde_json::json!({"data":"ignored"}),
                    },
                },
            )
            .await
            .expect_err("missing target instance must fail");
        assert!(missing_subscriber_error.contains("no active control subscriber"));

        listener_b.abort();
        let _ = listener_b.await;
        let failed_instance_error = relay_a
            .dispatch(
                relay_request("request-failed-instance"),
                Duration::from_secs(1),
            )
            .await
            .expect_err("failed owner instance must reject the relay request");
        assert!(matches!(
            failed_instance_error,
            RelayError::Coordination(ref error)
                if error.contains("no active control subscriber")
        ));
        assert_eq!(relay_a.stats().await.pending_relay_requests, 0);
        assert!(coordinator_a
            .relay_correlation("request-failed-instance")
            .await
            .expect("load failed-instance correlation")
            .is_none());

        assert!(coordinator_b
            .unregister_device_presence(&active_presence)
            .await
            .expect("remove active presence"));
        let expiring_coordinator = ValkeyCoordinator::connect(
            valkey_url.as_str(),
            key_prefix.as_str(),
            Duration::from_secs(1),
            Duration::from_secs(5),
        )
        .await
        .expect("connect expiring presence coordinator");
        let expiring_presence = DevicePresence {
            instance_id: instance_b,
            owner_user_id: "owner-1".to_string(),
            device_id: "device-expiring".to_string(),
            session_id: "session-expiring".to_string(),
        };
        expiring_coordinator
            .register_device_presence(&expiring_presence)
            .await
            .expect("register expiring presence");
        tokio::time::sleep(Duration::from_millis(1_500)).await;
        assert!(coordinator_a
            .device_presence("device-expiring")
            .await
            .expect("load expired presence")
            .is_none());
        listener_a.abort();
        let _ = listener_a.await;
    }

    #[tokio::test]
    async fn rejects_pending_requests_over_per_device_limit() {
        let relay = ConnectorRelay::new(
            None,
            RelayRuntimeLimits {
                max_pending_requests_per_device: 1,
                ..RelayRuntimeLimits::default()
            },
        );
        let (outbound, mut inbound) = mpsc::channel(8);
        relay
            .register_session(
                "device-1".to_string(),
                "owner-1".to_string(),
                "session-1".to_string(),
                outbound,
            )
            .await;

        let first_dispatch = {
            let relay = relay.clone();
            tokio::spawn(async move {
                relay
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
                            platform_signature: None,
                            platform_signature_key_id: None,
                            platform_signature_alg: None,
                            platform_timestamp: None,
                            platform_nonce: None,
                        },
                        Duration::from_secs(1),
                    )
                    .await
            })
        };
        let outbound = inbound.recv().await.expect("first outbound request");
        assert!(outbound.contains("request-1"));

        let second_error = relay
            .dispatch(
                RelayRequest {
                    message_type: "plugin_prepare_request".to_string(),
                    request_id: "request-2".to_string(),
                    owner_user_id: "owner-1".to_string(),
                    device_id: "device-1".to_string(),
                    workspace_id: "workspace-1".to_string(),
                    method: "POST".to_string(),
                    path: "/plugins/prepare".to_string(),
                    headers: BTreeMap::new(),
                    body: serde_json::json!({"plugin_id":"plugin-browser"}),
                    platform_signature: None,
                    platform_signature_key_id: None,
                    platform_signature_alg: None,
                    platform_timestamp: None,
                    platform_nonce: None,
                },
                Duration::from_millis(250),
            )
            .await
            .expect_err("second request should be rejected");
        assert!(matches!(
            second_error,
            RelayError::TooManyPendingRequests { limit: 1, .. }
        ));

        assert!(relay
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-1","status":200,"body":{"adapter_session_id":"adapter-1"}}"#,
            )
            .await
            .expect("complete first request"));
        first_dispatch
            .await
            .expect("first dispatch task")
            .expect("first dispatch response");
    }

    #[tokio::test]
    async fn oversized_terminal_event_is_rewritten_to_terminal_error() {
        let relay = ConnectorRelay::new(
            None,
            RelayRuntimeLimits {
                terminal_max_event_bytes: 256,
                ..RelayRuntimeLimits::default()
            },
        );
        let subscription = relay
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("subscribe terminal session");
        let subscription_id = subscription.id;
        let mut receiver = subscription.events;
        assert!(relay
            .handle_inbound_text(
                serde_json::json!({
                    "type": "terminal_output",
                    "terminal_session_id": "terminal-1",
                    "data": "x".repeat(2048),
                })
                .to_string()
                .as_str(),
            )
            .await
            .expect("publish oversized terminal event"));
        let event = receiver.recv().await.expect("terminal event");
        assert_eq!(event.message_type, "terminal_error");
        assert_eq!(
            event.body["original_message_type"].as_str(),
            Some("terminal_output")
        );
        relay
            .drop_terminal_subscription("terminal-1", subscription_id.as_str())
            .await
            .expect("drop terminal subscription");
        let stats = relay.stats().await;
        assert_eq!(stats.terminal_sessions, 0);
        assert_eq!(stats.terminal_ws_subscribers, 0);
    }

    #[tokio::test]
    async fn terminal_subscription_limits_bound_in_memory_state() {
        let relay = ConnectorRelay::new(
            None,
            RelayRuntimeLimits {
                terminal_max_active_sessions: 1,
                terminal_max_subscribers_per_session: 1,
                ..RelayRuntimeLimits::default()
            },
        );
        let first = relay
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("first terminal subscription");

        let subscriber_error = match relay.subscribe_terminal_session("terminal-1").await {
            Ok(_) => panic!("second subscriber must hit the per-session limit"),
            Err(error) => error,
        };
        assert!(subscriber_error.contains("subscriber capacity is exhausted"));
        let session_error = match relay.subscribe_terminal_session("terminal-2").await {
            Ok(_) => panic!("second terminal session must hit the instance limit"),
            Err(error) => error,
        };
        assert!(session_error.contains("session capacity is exhausted"));
        let stats = relay.stats().await;
        assert_eq!(stats.terminal_sessions, 1);
        assert_eq!(stats.terminal_ws_subscribers, 1);

        relay
            .drop_terminal_subscription("terminal-1", first.id.as_str())
            .await
            .expect("drop first terminal subscription");
        relay
            .subscribe_terminal_session("terminal-2")
            .await
            .expect("released capacity must be reusable");
    }

    #[tokio::test]
    async fn terminal_soft_pressure_pauses_only_new_sessions() {
        let relay = ConnectorRelay::new(
            None,
            RelayRuntimeLimits {
                terminal_max_active_sessions: 2,
                terminal_new_session_soft_limit: 1,
                terminal_max_subscribers_per_session: 2,
                ..RelayRuntimeLimits::default()
            },
        );
        let first = relay
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("first terminal session");
        let second_existing = relay
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("existing terminal session remains available under pressure");
        let error = match relay.subscribe_terminal_session("terminal-2").await {
            Ok(_) => panic!("new terminal session must pause at the soft limit"),
            Err(error) => error,
        };
        assert!(error.contains("soft pressure limit"));
        assert!(relay.stats().await.new_terminal_sessions_paused);

        relay
            .drop_terminal_subscription("terminal-1", first.id.as_str())
            .await
            .expect("drop first subscriber");
        relay
            .drop_terminal_subscription("terminal-1", second_existing.id.as_str())
            .await
            .expect("drop second subscriber");
        relay
            .subscribe_terminal_session("terminal-2")
            .await
            .expect("new terminal sessions resume after pressure clears");
    }

    #[tokio::test]
    async fn critical_platform_pressure_pauses_only_new_terminal_sessions() {
        let relay = ConnectorRelay::new(None, RelayRuntimeLimits::default());
        let first = relay
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("first terminal session");

        relay.set_platform_pressure_level(PlatformPressureLevel::Critical);
        let existing = relay
            .subscribe_terminal_session("terminal-1")
            .await
            .expect("existing terminal remains subscribable under critical pressure");
        let error = match relay.subscribe_terminal_session("terminal-2").await {
            Ok(_) => panic!("new terminal session must pause under critical pressure"),
            Err(error) => error,
        };
        assert!(error.contains("platform pressure is critical"));
        assert!(relay.new_terminal_sessions_paused().await);

        relay.set_platform_pressure_level(PlatformPressureLevel::Elevated);
        relay
            .subscribe_terminal_session("terminal-2")
            .await
            .expect("new terminal sessions resume below critical pressure");

        relay
            .drop_terminal_subscription("terminal-1", first.id.as_str())
            .await
            .expect("drop first subscriber");
        relay
            .drop_terminal_subscription("terminal-1", existing.id.as_str())
            .await
            .expect("drop existing subscriber");
    }

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
                        platform_signature: None,
                        platform_signature_key_id: None,
                        platform_signature_alg: None,
                        platform_timestamp: None,
                        platform_nonce: None,
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
                        platform_signature: None,
                        platform_signature_key_id: None,
                        platform_signature_alg: None,
                        platform_timestamp: None,
                        platform_nonce: None,
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
                            platform_signature: None,
                            platform_signature_key_id: None,
                            platform_signature_alg: None,
                            platform_timestamp: None,
                            platform_nonce: None,
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

    #[tokio::test]
    async fn runtime_limits_are_applied_after_hot_reload() {
        let relay = ConnectorRelay::default();
        let (outbound, mut inbound) = mpsc::channel(8);
        relay
            .register_session(
                "device-1".to_string(),
                "owner-1".to_string(),
                "session-1".to_string(),
                outbound,
            )
            .await;

        relay.update_runtime_config(
            None,
            RelayRuntimeLimits {
                max_pending_requests_per_device: 1,
                ..RelayRuntimeLimits::default()
            },
        );

        let first_dispatch = {
            let relay = relay.clone();
            tokio::spawn(async move {
                relay
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
                            platform_signature: None,
                            platform_signature_key_id: None,
                            platform_signature_alg: None,
                            platform_timestamp: None,
                            platform_nonce: None,
                        },
                        Duration::from_secs(1),
                    )
                    .await
            })
        };
        let outbound = inbound.recv().await.expect("first outbound request");
        assert!(outbound.contains("request-1"));

        let second_error = relay
            .dispatch(
                RelayRequest {
                    message_type: "plugin_prepare_request".to_string(),
                    request_id: "request-2".to_string(),
                    owner_user_id: "owner-1".to_string(),
                    device_id: "device-1".to_string(),
                    workspace_id: "workspace-1".to_string(),
                    method: "POST".to_string(),
                    path: "/plugins/prepare".to_string(),
                    headers: BTreeMap::new(),
                    body: serde_json::json!({"plugin_id":"plugin-browser"}),
                    platform_signature: None,
                    platform_signature_key_id: None,
                    platform_signature_alg: None,
                    platform_timestamp: None,
                    platform_nonce: None,
                },
                Duration::from_millis(250),
            )
            .await
            .expect_err("second request should be rejected");
        assert!(matches!(
            second_error,
            RelayError::TooManyPendingRequests { limit: 1, .. }
        ));

        assert!(relay
            .handle_inbound_text(
                r#"{"type":"plugin_prepare_response","request_id":"request-1","status":200,"body":{"adapter_session_id":"adapter-1"}}"#,
            )
            .await
            .expect("complete first request"));
        first_dispatch
            .await
            .expect("first dispatch task")
            .expect("first dispatch response");
    }
}
