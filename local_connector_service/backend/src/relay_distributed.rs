use super::*;

impl ConnectorRelay {
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
        }
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
        if !matches!(
            message_type,
            "companion_resources_response"
                | "companion_resolve_resource_response"
                | "companion_agent_workspace_response"
                | "companion_agent_conversation_response"
                | "companion_agent_messages_response"
                | "companion_agent_send_message_response"
                | "companion_agent_open_direct_response"
                | "companion_approvals_response"
                | "companion_resolve_approval_response"
                | "companion_error_response"
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
        let response = RelayResponse {
            request_id: inbound.request_id,
            status: inbound.status.unwrap_or(200),
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
        let request_id = serde_json::from_str::<Value>(text).ok().and_then(|value| {
            value
                .get("request_id")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        let source = {
            let inner = self.inner.lock().await;
            request_id
                .as_deref()
                .and_then(|id| inner.pending.get(id).map(|pending| pending.source.clone()))
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

    pub(super) async fn complete_response_from_source(
        &self,
        response: RelayResponse,
        source: &RelaySessionIdentity,
    ) -> Result<bool, String> {
        let sender = {
            let mut inner = self.inner.lock().await;
            let Some(pending) = inner.pending.get(response.request_id.as_str()) else {
                return Ok(false);
            };
            if &pending.source != source {
                return Err(
                    "Local Connector relay response source does not match the dispatched request"
                        .to_string(),
                );
            }
            inner
                .pending
                .remove(response.request_id.as_str())
                .map(|pending| pending.sender)
        };
        Ok(sender.is_some_and(|sender| sender.send(response).is_ok()))
    }

    pub(super) async fn route_remote_response(
        &self,
        response: RelayResponse,
        source: &RelaySessionIdentity,
    ) -> Result<bool, String> {
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
        if &correlation.source != source {
            return Err(
                "Local Connector relay response source does not match the distributed request"
                    .to_string(),
            );
        }
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
        let Some(session) = self
            .local_session(request.device_id.as_str(), request.owner_user_id.as_str())
            .await
        else {
            return Err(RelayError::Offline);
        };
        let text = serde_json::to_string(request)
            .map_err(|error| RelayError::RequestEncode(error.to_string()))?;
        session
            .outbound
            .send(text)
            .await
            .map_err(|_| RelayError::Offline)
    }

    pub(super) async fn local_session(
        &self,
        device_id: &str,
        owner_user_id: &str,
    ) -> Option<ActiveConnectorSession> {
        let inner = self.inner.lock().await;
        inner
            .sessions
            .get(device_id)
            .and_then(|session| (session.owner_user_id == owner_user_id).then(|| session.clone()))
    }

    pub(super) async fn remote_presence_for_request(
        &self,
        request: &RelayRequest,
    ) -> Result<DevicePresence, RelayError> {
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
        Ok(presence)
    }

    pub(super) async fn insert_pending_request(
        &self,
        request_id: &str,
        source: RelaySessionIdentity,
        class: PendingRelayClass,
        expires_at: Instant,
    ) -> Result<oneshot::Receiver<RelayResponse>, RelayError> {
        let runtime = self.runtime_config();
        let mut inner = self.inner.lock().await;
        if inner.pending.contains_key(request_id) {
            return Err(RelayError::DuplicateRequestId(request_id.to_string()));
        }
        let pending_count = inner
            .pending
            .values()
            .filter(|pending| pending.source.device_id == source.device_id)
            .count();
        if pending_count >= runtime.limits.max_pending_requests_per_device {
            return Err(RelayError::TooManyPendingRequests {
                device_id: source.device_id.clone(),
                limit: runtime.limits.max_pending_requests_per_device,
            });
        }
        let PendingRelayClass::Companion { client_session_id } = &class;
        {
            let companion_device_count = inner
                .pending
                .values()
                .filter(|pending| {
                    pending.source.device_id == source.device_id
                        && matches!(&pending.class, PendingRelayClass::Companion { .. })
                })
                .count();
            if companion_device_count >= MAX_PENDING_COMPANION_REQUESTS_PER_DEVICE {
                return Err(RelayError::TooManyPendingRequests {
                    device_id: source.device_id.clone(),
                    limit: MAX_PENDING_COMPANION_REQUESTS_PER_DEVICE,
                });
            }
            let companion_client_count = inner
                .pending
                .values()
                .filter(|pending| {
                    matches!(
                        &pending.class,
                        PendingRelayClass::Companion {
                            client_session_id: pending_client_session_id
                        } if pending_client_session_id == client_session_id
                    )
                })
                .count();
            if companion_client_count >= MAX_PENDING_COMPANION_REQUESTS_PER_CLIENT_SESSION {
                return Err(RelayError::TooManyPendingRequests {
                    device_id: source.device_id.clone(),
                    limit: MAX_PENDING_COMPANION_REQUESTS_PER_CLIENT_SESSION,
                });
            }
        }
        let (sender, receiver) = oneshot::channel();
        inner.pending.insert(
            request_id.to_string(),
            PendingRelayRequest {
                source,
                class,
                expires_at,
                sender,
            },
        );
        Ok(receiver)
    }

    pub(crate) fn start_pending_reaper(&self, interval: Duration) {
        let relay = self.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;
                let reaped = relay.reap_expired_pending().await;
                if reaped > 0 {
                    tracing::warn!(
                        reaped,
                        "reaped expired Local Connector pending relay requests"
                    );
                }
            }
        });
    }

    pub(super) async fn reap_expired_pending(&self) -> usize {
        let now = Instant::now();
        let expired_request_ids = {
            let inner = self.inner.lock().await;
            inner
                .pending
                .iter()
                .filter(|(_, pending)| pending.expires_at <= now)
                .map(|(request_id, _)| request_id.clone())
                .collect::<Vec<_>>()
        };
        for request_id in &expired_request_ids {
            self.cleanup_request(request_id.as_str()).await;
        }
        expired_request_ids.len()
    }

    pub(super) async fn remove_pending(&self, request_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.pending.remove(request_id);
    }

    pub(super) async fn cleanup_request(&self, request_id: &str) {
        self.remove_pending(request_id).await;
        if let Some(distributed) = self.distributed.as_ref() {
            let _ = distributed
                .coordinator
                .delete_relay_correlation(request_id, distributed.instance_id.as_str())
                .await;
        }
    }

    pub(super) fn runtime_config(&self) -> RelayRuntimeConfig {
        self.runtime
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}
