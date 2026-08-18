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

mod terminal;
#[cfg(test)]
mod tests;

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

    pub async fn has_active_session(
        &self,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<bool, RelayError> {
        if self
            .local_session_outbound(device_id, owner_user_id)
            .await
            .is_some()
        {
            return Ok(true);
        }
        let Some(distributed) = self.distributed.as_ref() else {
            return Ok(false);
        };
        let Some(presence) = distributed
            .coordinator
            .device_presence(device_id)
            .await
            .map_err(RelayError::Coordination)?
        else {
            return Ok(false);
        };
        Ok(presence.owner_user_id == owner_user_id
            && presence.instance_id != distributed.instance_id)
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
