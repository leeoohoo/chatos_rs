use std::collections::HashSet;
use std::sync::atomic::Ordering;

use serde_json::Value;
use tokio::sync::broadcast;
use uuid::Uuid;

use super::{
    default_body, ConnectorRelay, InboundRelayResponse, InboundTerminalEvent,
    InterInstanceRelayMessage, RelayResponse, TerminalRelayEvent, TerminalRelaySubscription,
};
use crate::valkey_coordination::RelaySessionIdentity;

impl ConnectorRelay {
    pub async fn subscribe_terminal_session_for(
        &self,
        terminal_session_id: &str,
        owner_user_id: &str,
        device_id: &str,
    ) -> Result<TerminalRelaySubscription, String> {
        let source = self
            .relay_session_identity(owner_user_id, device_id)
            .await
            .map_err(|error| error.message())?;
        self.subscribe_terminal_session_with_source(terminal_session_id, source)
            .await
    }

    #[cfg(test)]
    pub async fn subscribe_terminal_session(
        &self,
        terminal_session_id: &str,
    ) -> Result<TerminalRelaySubscription, String> {
        let source = {
            let inner = self.inner.lock().await;
            inner
                .sessions
                .iter()
                .next()
                .map(|(device_id, session)| session.relay_identity(device_id))
                .unwrap_or_else(|| RelaySessionIdentity {
                    owner_user_id: "owner-1".to_string(),
                    device_id: "device-1".to_string(),
                    session_id: "session-1".to_string(),
                })
        };
        self.subscribe_terminal_session_with_source(terminal_session_id, source)
            .await
    }

    async fn subscribe_terminal_session_with_source(
        &self,
        terminal_session_id: &str,
        source: RelaySessionIdentity,
    ) -> Result<TerminalRelaySubscription, String> {
        let subscription_id = Uuid::new_v4().to_string();
        let events = {
            let mut inner = self.inner.lock().await;
            if let Some(existing) = inner.terminal_sources.get(terminal_session_id) {
                if existing != &source {
                    return Err(
                        "Local Connector terminal session belongs to another connector session"
                            .to_string(),
                    );
                }
            }
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
            inner
                .terminal_sources
                .insert(terminal_session_id.to_string(), source.clone());
            events
        };
        if let Some(distributed) = self.distributed.as_ref() {
            let binding_registered = distributed
                .coordinator
                .register_terminal_session_binding(terminal_session_id, &source)
                .await?;
            if !binding_registered {
                self.remove_local_terminal_subscription(
                    terminal_session_id,
                    subscription_id.as_str(),
                )
                .await;
                return Err(
                    "Local Connector terminal session is already bound to another connector session"
                        .to_string(),
                );
            }
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
            let source = {
                let inner = self.inner.lock().await;
                inner.terminal_sources.get(terminal_session_id).cloned()
            }
            .ok_or_else(|| "Local Connector terminal session binding is missing".to_string())?;
            if !distributed
                .coordinator
                .register_terminal_session_binding(terminal_session_id, &source)
                .await?
            {
                return Err(
                    "Local Connector terminal session binding was replaced by another connector session"
                        .to_string(),
                );
            }
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

    pub async fn handle_inbound_text_from(
        &self,
        source: RelaySessionIdentity,
        text: &str,
    ) -> Result<bool, String> {
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
            return self.publish_terminal_event(event, &source).await;
        }
        if !matches!(
            message_type,
            "lease_response"
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
                | "workspace_directory_list_response"
                | "workspace_directory_create_response"
                | "workspace_filesystem_response"
                | "relay_response"
        ) {
            if message_type.ends_with("_response")
                && value.get("request_id").and_then(Value::as_str).is_some()
            {
                return Err(format!("unsupported relay response type `{message_type}`"));
            }
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
        if self
            .complete_response_from_source(response.clone(), &source)
            .await?
        {
            return Ok(true);
        }
        self.route_remote_response(response, &source).await
    }

    #[cfg(test)]
    pub async fn handle_inbound_text(&self, text: &str) -> Result<bool, String> {
        let value = serde_json::from_str::<Value>(text).ok();
        let terminal_session_id = value
            .as_ref()
            .and_then(|value| value.get("terminal_session_id"))
            .and_then(Value::as_str);
        let request_id = value
            .as_ref()
            .and_then(|value| value.get("request_id"))
            .and_then(Value::as_str);
        let source = {
            let inner = self.inner.lock().await;
            terminal_session_id
                .and_then(|id| inner.terminal_sources.get(id).cloned())
                .or_else(|| {
                    request_id
                        .and_then(|id| inner.pending.get(id).map(|pending| pending.source.clone()))
                })
                .or_else(|| {
                    inner
                        .sessions
                        .iter()
                        .next()
                        .map(|(device_id, session)| session.relay_identity(device_id))
                })
                .unwrap_or_else(|| RelaySessionIdentity {
                    owner_user_id: "owner-1".to_string(),
                    device_id: "device-1".to_string(),
                    session_id: "session-1".to_string(),
                })
        };
        self.handle_inbound_text_from(source, text).await
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
        inner.terminal_sources.remove(terminal_session_id);
        true
    }

    async fn publish_terminal_event(
        &self,
        inbound: InboundTerminalEvent,
        source: &RelaySessionIdentity,
    ) -> Result<bool, String> {
        let original_message_type = inbound.message_type.clone();
        let terminal_session_id = inbound.terminal_session_id.clone();
        let local_source = {
            let inner = self.inner.lock().await;
            inner
                .terminal_sources
                .get(terminal_session_id.as_str())
                .cloned()
        };
        if let Some(expected) = local_source.as_ref() {
            if expected != source {
                return Err(
                    "Local Connector terminal event source does not match the subscribed session"
                        .to_string(),
                );
            }
        }
        if let Some(distributed) = self.distributed.as_ref() {
            let expected = distributed
                .coordinator
                .terminal_session_binding(terminal_session_id.as_str())
                .await?
                .ok_or_else(|| {
                    "Local Connector distributed terminal session binding is missing".to_string()
                })?;
            if &expected != source {
                return Err(
                    "Local Connector terminal event source does not match the distributed session"
                        .to_string(),
                );
            }
        } else if local_source.is_none() {
            return Ok(false);
        }
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

    pub(super) async fn publish_local_terminal_event(&self, event: TerminalRelayEvent) -> bool {
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
}
