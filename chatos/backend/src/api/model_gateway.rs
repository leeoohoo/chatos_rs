// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive};
use axum::response::Sse;
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_ai_runtime::request_payload::{
    build_chat_completions_request_payload, build_responses_request_payload,
    responses_input_token_count_payload,
};
use chatos_ai_runtime::{
    AiRequestHandler, AiRequestOptions, AiResponse, AiTransport, StreamCallbacks,
};
use chatos_local_agent_protocol::{
    ModelGatewayRequest, ModelGatewayStreamEnvelope, ModelGatewayStreamEvent, ModelGatewayTerminal,
    ModelGatewayTerminalSource, ModelGatewayTerminalStatus, ModelGatewayTokenCount, ModelProtocol,
    ModelRuntimeDescriptor, MAX_MODEL_GATEWAY_JSON_BYTES,
};
use futures::Stream;
use serde_json::{json, Value};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use crate::config::Config;
use crate::core::auth::AuthUser;
use crate::services::user_service_api_client::{self, UserServiceInternalModelRuntimeRecord};

type ApiError = (StatusCode, Json<Value>);
type ApiResult<T> = Result<Json<T>, ApiError>;

pub fn router() -> Router {
    Router::new()
        .route(
            "/api/model-gateway/descriptors/{model_config_id}",
            get(get_model_runtime_descriptor),
        )
        .route(
            "/api/model-gateway/input-tokens",
            post(count_model_input_tokens),
        )
        .route("/api/model-gateway/stream", post(stream_model_request))
}

async fn count_model_input_tokens(
    auth: AuthUser,
    Json(request): Json<ModelGatewayRequest>,
) -> ApiResult<ModelGatewayTokenCount> {
    request.validate().map_err(|error| {
        warn!(error = %error, "model_gateway.input_tokens.invalid_request");
        api_error(StatusCode::BAD_REQUEST, error.to_string())
    })?;
    let runtime =
        load_runtime_for_authenticated_user(&auth, request.model_config_id.as_str()).await?;
    if runtime.revision != request.model_config_revision {
        return Err(api_error(
            StatusCode::CONFLICT,
            "model configuration revision does not match the frozen run",
        ));
    }
    let descriptor = descriptor_from_runtime(&runtime)
        .map_err(|detail| api_error(StatusCode::UNPROCESSABLE_ENTITY, detail))?;
    request
        .validate_against(&descriptor)
        .map_err(|error| api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string()))?;
    validate_gateway_parameter_policy(&request, &runtime)
        .map_err(|detail| api_error(StatusCode::UNPROCESSABLE_ENTITY, detail))?;
    if !descriptor.supports_input_token_count || descriptor.protocol != ModelProtocol::Responses {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "model configuration does not support exact input token counting",
        ));
    }

    let payload = responses_input_token_count_payload(build_provider_payload(&request, &runtime));
    let input_tokens = AiRequestHandler::new()
        .count_responses_input_tokens(
            runtime.base_url.as_str(),
            runtime.api_key.as_str(),
            payload,
            None,
        )
        .await
        .map_err(|error| {
            warn!(model_config_id = request.model_config_id, error = %error, "model_gateway.input_tokens.provider_failed");
            api_error(StatusCode::BAD_GATEWAY, "provider input token count failed")
        })?
        .ok_or_else(|| {
            api_error(
                StatusCode::BAD_GATEWAY,
                "provider does not implement its configured input token count capability",
            )
        })?;
    let input_tokens = u64::try_from(input_tokens).map_err(|_| {
        api_error(
            StatusCode::BAD_GATEWAY,
            "provider input token count exceeds the supported range",
        )
    })?;
    let response = ModelGatewayTokenCount {
        request_id: request.request_id.clone(),
        model_config_id: request.model_config_id.clone(),
        model_config_revision: request.model_config_revision,
        input_tokens,
    };
    response
        .validate_against(&request)
        .map_err(|error| api_error(StatusCode::BAD_GATEWAY, error.to_string()))?;
    Ok(Json(response))
}

async fn get_model_runtime_descriptor(
    Path(model_config_id): Path<String>,
    auth: AuthUser,
) -> ApiResult<ModelRuntimeDescriptor> {
    let model_config_id = model_config_id.trim();
    if model_config_id.is_empty() {
        return Err(api_error(
            StatusCode::BAD_REQUEST,
            "model_config_id is required",
        ));
    }

    let runtime = load_runtime_for_authenticated_user(&auth, model_config_id).await?;

    let descriptor = descriptor_from_runtime(&runtime).map_err(|detail| {
        warn!(model_config_id, detail = %detail, "model_gateway.descriptor.invalid_config");
        api_error(StatusCode::UNPROCESSABLE_ENTITY, detail)
    })?;
    Ok(Json(descriptor))
}

async fn stream_model_request(
    auth: AuthUser,
    Json(request): Json<ModelGatewayRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    request.validate().map_err(|error| {
        warn!(error = %error, "model_gateway.stream.invalid_request");
        api_error(StatusCode::BAD_REQUEST, error.to_string())
    })?;

    let runtime =
        load_runtime_for_authenticated_user(&auth, request.model_config_id.as_str()).await?;
    if runtime.revision != request.model_config_revision {
        return Err(api_error(
            StatusCode::CONFLICT,
            "model configuration revision does not match the frozen run",
        ));
    }
    let descriptor = descriptor_from_runtime(&runtime)
        .map_err(|detail| api_error(StatusCode::UNPROCESSABLE_ENTITY, detail))?;
    request.validate_against(&descriptor).map_err(|error| {
        warn!(model_config_id = request.model_config_id, error = %error, "model_gateway.stream.request_descriptor_mismatch");
        api_error(StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
    })?;
    if !descriptor.supports_streaming {
        return Err(api_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "model configuration does not support streaming",
        ));
    }
    validate_gateway_parameter_policy(&request, &runtime)
        .map_err(|detail| api_error(StatusCode::UNPROCESSABLE_ENTITY, detail))?;
    let payload = build_provider_payload(&request, &runtime);

    let cancellation = CancellationToken::new();
    let (sender, receiver) = mpsc::unbounded_channel();
    let emitter = Arc::new(Mutex::new(GatewayEventEmitter::new(
        request.request_id.clone(),
        request.protocol,
        sender,
        cancellation.clone(),
    )));
    let callbacks = stream_callbacks(Arc::clone(&emitter));
    tokio::spawn(run_single_provider_request(
        request,
        runtime,
        payload,
        callbacks,
        emitter,
        cancellation.clone(),
    ));

    let stream = futures::stream::unfold(
        GatewayEventReceiver::new(receiver, cancellation),
        |mut state| async move {
            state.receiver.recv().await.map(|event| {
                (
                    Ok::<Event, Infallible>(Event::default().data(event.data)),
                    state,
                )
            })
        },
    );
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}

fn validate_gateway_parameter_policy(
    request: &ModelGatewayRequest,
    runtime: &UserServiceInternalModelRuntimeRecord,
) -> Result<(), &'static str> {
    if let Some(requested) = request.parameters.temperature {
        let Some(configured) = runtime.temperature else {
            return Err("temperature override is not enabled for this model configuration");
        };
        if requested.to_bits() != configured.to_bits() {
            return Err("temperature does not match the model configuration");
        }
    }
    if let Some(requested) = request.parameters.reasoning_effort.as_deref() {
        let Some(configured) = runtime.thinking_level.as_deref() else {
            return Err("reasoning override is not enabled for this model configuration");
        };
        if requested != configured {
            return Err("reasoning effort does not match the model configuration");
        }
    }
    Ok(())
}

struct GatewayEventReceiver {
    receiver: mpsc::UnboundedReceiver<GatewayWireEvent>,
    cancellation: CancellationToken,
}

impl GatewayEventReceiver {
    fn new(
        receiver: mpsc::UnboundedReceiver<GatewayWireEvent>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            receiver,
            cancellation,
        }
    }
}

struct GatewayWireEvent {
    data: String,
}

impl Drop for GatewayEventReceiver {
    fn drop(&mut self) {
        self.cancellation.cancel();
    }
}

fn build_provider_payload(
    request: &ModelGatewayRequest,
    runtime: &UserServiceInternalModelRuntimeRecord,
) -> Value {
    let temperature = request.parameters.temperature.or(runtime.temperature);
    let reasoning_effort = request
        .parameters
        .reasoning_effort
        .clone()
        .or_else(|| runtime.thinking_level.clone());
    let maximum_output_tokens = Some(i64::from(request.parameters.maximum_output_tokens));
    let provider = Some(runtime.provider.clone());
    match request.protocol {
        ModelProtocol::Responses => {
            let mut payload = build_responses_request_payload(
                request.input.clone(),
                runtime.model.clone(),
                request.instructions.clone(),
                None,
                None,
                Some(request.tools.clone()),
                None,
                temperature,
                maximum_output_tokens,
                provider,
                reasoning_effort,
                true,
                false,
                None,
            );
            payload["store"] = Value::Bool(false);
            if let Some(threshold) = request.parameters.native_compaction_threshold {
                payload["context_management"] = json!([{
                    "type": "compaction",
                    "compact_threshold": threshold,
                }]);
            }
            payload
        }
        ModelProtocol::ChatCompletions => build_chat_completions_request_payload(
            request.input.clone(),
            runtime.model.clone(),
            request.instructions.clone(),
            Some(request.tools.clone()),
            temperature,
            maximum_output_tokens,
            provider,
            reasoning_effort,
            true,
            None,
        ),
    }
}

struct GatewayEventEmitter {
    request_id: String,
    protocol: ModelProtocol,
    next_sequence: u64,
    accumulated_payload_bytes: usize,
    failure_reason: Option<&'static str>,
    terminal_sent: bool,
    sender: mpsc::UnboundedSender<GatewayWireEvent>,
    cancellation: CancellationToken,
}

impl GatewayEventEmitter {
    fn new(
        request_id: String,
        protocol: ModelProtocol,
        sender: mpsc::UnboundedSender<GatewayWireEvent>,
        cancellation: CancellationToken,
    ) -> Self {
        Self {
            request_id,
            protocol,
            next_sequence: 1,
            accumulated_payload_bytes: 0,
            failure_reason: None,
            terminal_sent: false,
            sender,
            cancellation,
        }
    }

    fn emit_payload(&mut self, event: ModelGatewayStreamEvent) -> bool {
        if self.terminal_sent || self.failure_reason.is_some() {
            return false;
        }
        let payload_bytes = match serde_json::to_vec(&event) {
            Ok(payload) => payload.len(),
            Err(_) => {
                self.fail("gateway event serialization failed");
                return false;
            }
        };
        let Some(next_bytes) = self.accumulated_payload_bytes.checked_add(payload_bytes) else {
            self.fail("gateway stream exceeded its payload limit");
            return false;
        };
        if next_bytes > MAX_MODEL_GATEWAY_JSON_BYTES {
            self.fail("gateway stream exceeded its payload limit");
            return false;
        }
        if !self.send(event) {
            return false;
        }
        self.accumulated_payload_bytes = next_bytes;
        true
    }

    fn emit_terminal(&mut self, terminal: ModelGatewayTerminal) -> bool {
        if self.terminal_sent {
            return false;
        }
        self.failure_reason = None;
        let sent = self.send(ModelGatewayStreamEvent::Terminal {
            terminal: Box::new(terminal),
        });
        self.terminal_sent = sent;
        sent
    }

    fn failure_reason(&self) -> Option<&'static str> {
        self.failure_reason
    }

    fn fail(&mut self, reason: &'static str) {
        if self.failure_reason.is_none() {
            self.failure_reason = Some(reason);
        }
        self.cancellation.cancel();
    }

    fn send(&mut self, event: ModelGatewayStreamEvent) -> bool {
        let envelope = ModelGatewayStreamEnvelope {
            request_id: self.request_id.clone(),
            sequence: self.next_sequence,
            protocol: self.protocol,
            event,
        };
        if envelope.validate().is_err() {
            self.fail("gateway produced an invalid stream event");
            return false;
        }
        let data = match serde_json::to_string(&envelope) {
            Ok(data) => data,
            Err(_) => {
                self.fail("gateway event serialization failed");
                return false;
            }
        };
        if self.sender.send(GatewayWireEvent { data }).is_err() {
            self.cancellation.cancel();
            return false;
        }
        let Some(next_sequence) = self.next_sequence.checked_add(1) else {
            self.fail("gateway stream sequence overflowed");
            return false;
        };
        self.next_sequence = next_sequence;
        true
    }
}

fn stream_callbacks(emitter: Arc<Mutex<GatewayEventEmitter>>) -> StreamCallbacks {
    let content_emitter = Arc::clone(&emitter);
    let reasoning_emitter = emitter;
    StreamCallbacks {
        on_chunk: Some(Arc::new(move |delta| {
            if delta.is_empty() {
                return;
            }
            lock_emitter(&content_emitter)
                .emit_payload(ModelGatewayStreamEvent::ContentDelta { delta });
        })),
        on_thinking: Some(Arc::new(move |delta| {
            if delta.is_empty() {
                return;
            }
            lock_emitter(&reasoning_emitter)
                .emit_payload(ModelGatewayStreamEvent::ReasoningDelta { delta });
        })),
    }
}

fn lock_emitter(
    emitter: &Arc<Mutex<GatewayEventEmitter>>,
) -> std::sync::MutexGuard<'_, GatewayEventEmitter> {
    emitter
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

async fn run_single_provider_request(
    request: ModelGatewayRequest,
    runtime: UserServiceInternalModelRuntimeRecord,
    payload: Value,
    callbacks: StreamCallbacks,
    emitter: Arc<Mutex<GatewayEventEmitter>>,
    cancellation: CancellationToken,
) {
    let transport = match request.protocol {
        ModelProtocol::Responses => AiTransport::Responses,
        ModelProtocol::ChatCompletions => AiTransport::ChatCompletions,
    };
    let result = AiRequestHandler::new()
        .send_prebuilt_payload_with_options(
            runtime.base_url.as_str(),
            runtime.api_key.as_str(),
            transport,
            payload,
            callbacks,
            Some(runtime.provider.clone()),
            runtime.thinking_level.clone(),
            None,
            AiRequestOptions {
                request_body_limit_bytes: Some(MAX_MODEL_GATEWAY_JSON_BYTES),
                abort_token: Some(cancellation),
                stream: true,
                ..AiRequestOptions::default()
            },
        )
        .await;

    let stream_failure = lock_emitter(&emitter).failure_reason();
    if let Some(reason) = stream_failure {
        emit_gateway_failure(&emitter, reason, None, None);
        return;
    }

    match result {
        Ok(response) => emit_provider_response(&emitter, request.protocol, response),
        Err(error) => {
            warn!(
                request_id = request.request_id,
                model_config_id = request.model_config_id,
                error = %error,
                "model_gateway.stream.provider_request_failed"
            );
            let provider_http_status = tagged_u16(&error, "http_status=")
                .or_else(|| provider_status_from_transport_error(&error));
            let provider_request_id = tagged_text(&error, "provider_request_id=");
            emit_gateway_failure(
                &emitter,
                "provider request failed before a valid terminal result",
                provider_http_status,
                provider_request_id,
            );
        }
    }
}

fn emit_provider_response(
    emitter: &Arc<Mutex<GatewayEventEmitter>>,
    protocol: ModelProtocol,
    response: AiResponse,
) {
    let provider_http_status = response.provider_http_status;
    let provider_request_id = response.provider_request_id.clone();
    let output_items = response.response_output_items.clone();
    for item in &output_items {
        if !lock_emitter(emitter)
            .emit_payload(ModelGatewayStreamEvent::OutputItem { item: item.clone() })
        {
            let reason = lock_emitter(emitter)
                .failure_reason()
                .unwrap_or("gateway could not emit provider output items");
            emit_gateway_failure(emitter, reason, provider_http_status, provider_request_id);
            return;
        }
    }

    match terminal_from_ai_response(protocol, response) {
        Ok(terminal) => {
            lock_emitter(emitter).emit_terminal(terminal);
        }
        Err(reason) => {
            warn!(reason, "model_gateway.stream.invalid_provider_terminal");
            emit_gateway_failure(emitter, reason, provider_http_status, provider_request_id);
        }
    }
}

fn terminal_from_ai_response(
    protocol: ModelProtocol,
    response: AiResponse,
) -> Result<ModelGatewayTerminal, &'static str> {
    let provider_http_status = response
        .provider_http_status
        .filter(|status| *status > 0)
        .ok_or("provider response is missing its HTTP status")?;
    let fields = match protocol {
        ModelProtocol::Responses => responses_terminal_fields(&response)?,
        ModelProtocol::ChatCompletions => chat_completions_terminal_fields(&response)?,
    };
    let response_id = normalize_identifier(response.response_id);
    let provider_request_id = normalize_identifier(response.provider_request_id);
    let terminal = ModelGatewayTerminal {
        status: fields.status,
        source: ModelGatewayTerminalSource::Provider,
        response_id,
        provider_request_id,
        terminal_event: fields.terminal_event,
        provider_http_status: Some(provider_http_status),
        usage: response.usage,
        output_items: response.response_output_items,
        incomplete_details: fields.incomplete_details,
        provider_error: fields.provider_error,
    };
    terminal
        .validate(protocol)
        .map_err(|_| "provider returned an invalid terminal result")?;
    Ok(terminal)
}

struct TerminalFields {
    status: ModelGatewayTerminalStatus,
    terminal_event: String,
    incomplete_details: Option<Value>,
    provider_error: Option<Value>,
}

fn responses_terminal_fields(response: &AiResponse) -> Result<TerminalFields, &'static str> {
    if !response.terminal_event_seen {
        return Err("Responses stream ended without a provider terminal event");
    }
    let event = response
        .terminal_event_type
        .as_deref()
        .ok_or("Responses stream omitted its terminal event type")?;
    let status = response
        .response_status
        .as_deref()
        .ok_or("Responses terminal omitted its response status")?;
    match (event, status) {
        ("response.completed", "completed") => Ok(TerminalFields {
            status: ModelGatewayTerminalStatus::Completed,
            terminal_event: event.to_string(),
            incomplete_details: None,
            provider_error: None,
        }),
        ("response.incomplete", "incomplete") => Ok(TerminalFields {
            status: ModelGatewayTerminalStatus::Incomplete,
            terminal_event: event.to_string(),
            incomplete_details: Some(
                response
                    .incomplete_details
                    .clone()
                    .ok_or("Responses incomplete terminal omitted incomplete_details")?,
            ),
            provider_error: None,
        }),
        ("response.failed", "failed") => Ok(TerminalFields {
            status: ModelGatewayTerminalStatus::Failed,
            terminal_event: event.to_string(),
            incomplete_details: None,
            provider_error: Some(
                response
                    .provider_error
                    .clone()
                    .ok_or("Responses failed terminal omitted provider_error")?,
            ),
        }),
        _ => Err("Responses terminal event and response status do not match"),
    }
}

fn chat_completions_terminal_fields(response: &AiResponse) -> Result<TerminalFields, &'static str> {
    if let Some(provider_error) = response.provider_error.clone() {
        return Ok(TerminalFields {
            status: ModelGatewayTerminalStatus::Failed,
            terminal_event: "chat.completion.failed".to_string(),
            incomplete_details: None,
            provider_error: Some(provider_error),
        });
    }
    let finish_reason = response
        .finish_reason
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or("Chat Completions stream ended without a finish reason")?;
    let terminal_event = format!("chat.completion.{finish_reason}");
    if matches!(finish_reason, "length" | "content_filter") {
        Ok(TerminalFields {
            status: ModelGatewayTerminalStatus::Incomplete,
            terminal_event,
            incomplete_details: Some(json!({ "reason": finish_reason })),
            provider_error: None,
        })
    } else {
        Ok(TerminalFields {
            status: ModelGatewayTerminalStatus::Completed,
            terminal_event,
            incomplete_details: None,
            provider_error: None,
        })
    }
}

fn emit_gateway_failure(
    emitter: &Arc<Mutex<GatewayEventEmitter>>,
    reason: &'static str,
    provider_http_status: Option<u16>,
    provider_request_id: Option<String>,
) {
    let terminal = ModelGatewayTerminal {
        status: ModelGatewayTerminalStatus::Failed,
        source: ModelGatewayTerminalSource::Gateway,
        response_id: None,
        provider_request_id: normalize_identifier(provider_request_id),
        terminal_event: "gateway.provider_request_failed".to_string(),
        provider_http_status,
        usage: None,
        output_items: Vec::new(),
        incomplete_details: None,
        provider_error: Some(json!({
            "type": "gateway_provider_request_failed",
            "message": reason,
        })),
    };
    lock_emitter(emitter).emit_terminal(terminal);
}

fn normalize_identifier(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn tagged_u16(error: &str, tag: &str) -> Option<u16> {
    tagged_text(error, tag)?.parse().ok()
}

fn tagged_text(error: &str, tag: &str) -> Option<String> {
    let value = error.split_once(tag)?.1;
    let value = value.split([']', ',', ' ']).next()?.trim();
    (!value.is_empty() && value != "unavailable").then(|| value.to_string())
}

fn provider_status_from_transport_error(error: &str) -> Option<u16> {
    let status = error.strip_prefix("status ")?.split_whitespace().next()?;
    status
        .split_once(' ')
        .map_or(status, |(_, code)| code)
        .trim_start_matches(|value: char| !value.is_ascii_digit())
        .parse()
        .ok()
}

async fn load_runtime_for_authenticated_user(
    auth: &AuthUser,
    model_config_id: &str,
) -> Result<UserServiceInternalModelRuntimeRecord, ApiError> {
    let config = Config::try_get().map_err(|error| {
        warn!(error = %error, "model_gateway.config_unavailable");
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "model gateway configuration is unavailable",
        )
    })?;
    let internal_secret = config
        .user_service_internal_api_secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "model gateway service identity is unavailable",
            )
        })?;
    let runtime = user_service_api_client::get_internal_model_runtime_config(
        &config.user_service_internal_http_client,
        config.user_service_internal_base_url.as_str(),
        internal_secret,
        auth.user_id.as_str(),
        model_config_id,
    )
    .await
    .map_err(|error| map_runtime_service_error(model_config_id, &error))?;
    validate_runtime_identity(&runtime, auth.user_id.as_str(), model_config_id).map_err(
        |detail| {
            warn!(
                model_config_id,
                detail, "model_gateway.runtime.identity_mismatch"
            );
            api_error(
                StatusCode::BAD_GATEWAY,
                "model runtime configuration identity is invalid",
            )
        },
    )?;
    Ok(runtime)
}

fn validate_runtime_identity(
    runtime: &UserServiceInternalModelRuntimeRecord,
    expected_user_id: &str,
    expected_model_config_id: &str,
) -> Result<(), &'static str> {
    if runtime.owner_user_id != expected_user_id {
        return Err("runtime owner does not match authenticated user");
    }
    if runtime.id != expected_model_config_id {
        return Err("runtime model config id does not match requested id");
    }
    Ok(())
}

fn descriptor_from_runtime(
    runtime: &UserServiceInternalModelRuntimeRecord,
) -> Result<ModelRuntimeDescriptor, &'static str> {
    let protocol = runtime
        .protocol
        .ok_or("model configuration is missing protocol")?;
    let context_strategy = runtime
        .context_strategy
        .ok_or("model configuration is missing context_strategy")?;
    let context_window_tokens = runtime
        .context_window_tokens
        .and_then(|value| u64::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("model configuration is missing a positive context_window_tokens")?;
    let maximum_output_tokens = runtime
        .max_output_tokens
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or("model configuration is missing a valid max_output_tokens")?;

    let descriptor = ModelRuntimeDescriptor {
        model_config_id: runtime.id.clone(),
        revision: runtime.revision,
        provider: runtime.provider.clone(),
        model: runtime.model.clone(),
        protocol,
        context_window_tokens,
        maximum_output_tokens,
        context_strategy,
        supports_streaming: runtime.supports_streaming,
        supports_native_compaction: runtime.supports_native_compaction,
        supports_input_token_count: runtime.supports_input_token_count,
    };
    descriptor
        .validate()
        .map_err(|_| "model configuration cannot produce a valid runtime descriptor")?;
    Ok(descriptor)
}

fn map_runtime_service_error(model_config_id: &str, error: &str) -> ApiError {
    let status = match user_service_api_client::response_status_from_error(error) {
        Some(400) => StatusCode::UNPROCESSABLE_ENTITY,
        Some(401) => StatusCode::UNAUTHORIZED,
        Some(403) => StatusCode::FORBIDDEN,
        Some(404) => StatusCode::NOT_FOUND,
        Some(409) => StatusCode::CONFLICT,
        _ => StatusCode::BAD_GATEWAY,
    };
    warn!(model_config_id, status = %status, error = %error, "model_gateway.descriptor.user_service_failed");
    let message = if status == StatusCode::NOT_FOUND {
        "model configuration was not found"
    } else if status == StatusCode::FORBIDDEN {
        "model configuration does not belong to the current user"
    } else if status == StatusCode::UNPROCESSABLE_ENTITY {
        "model configuration is not ready for local agent execution"
    } else {
        "model runtime configuration could not be loaded"
    };
    api_error(status, message)
}

fn api_error(status: StatusCode, message: impl Into<String>) -> ApiError {
    (status, Json(json!({ "error": message.into() })))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
    use axum::routing::post;
    use chatos_ai_runtime::AiResponse;
    use chatos_local_agent_protocol::{
        ContextStrategy, ModelGatewayParameters, ModelGatewayRequest, ModelGatewayStreamEvent,
        ModelGatewayTerminalSource, ModelGatewayTerminalStatus, ModelProtocol,
    };
    use serde_json::json;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use tower::ServiceExt;

    use super::{
        build_provider_payload, descriptor_from_runtime, router, run_single_provider_request,
        stream_callbacks, terminal_from_ai_response, validate_gateway_parameter_policy,
        validate_runtime_identity, GatewayEventEmitter, GatewayEventReceiver,
    };
    use crate::services::user_service_api_client::UserServiceInternalModelRuntimeRecord;

    fn complete_runtime() -> UserServiceInternalModelRuntimeRecord {
        UserServiceInternalModelRuntimeRecord {
            id: "model-1".to_string(),
            revision: 7,
            owner_user_id: "user-1".to_string(),
            name: "Primary".to_string(),
            provider: "openai".to_string(),
            protocol: Some(ModelProtocol::Responses),
            context_strategy: Some(ContextStrategy::ProviderNative),
            prompt_vendor: Some("gpt".to_string()),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "secret-key-must-not-leak".to_string(),
            model: "gpt-test".to_string(),
            thinking_level: Some("high".to_string()),
            temperature: Some(0.2),
            context_window_tokens: Some(400_000),
            max_output_tokens: Some(32_000),
            supports_images: true,
            supports_reasoning: true,
            supports_responses: true,
            supports_streaming: true,
            supports_native_compaction: true,
            supports_input_token_count: true,
        }
    }

    fn gateway_request(protocol: ModelProtocol) -> ModelGatewayRequest {
        ModelGatewayRequest {
            request_id: "request-1".to_string(),
            model_config_id: "model-1".to_string(),
            model_config_revision: 7,
            protocol,
            input: json!([{"role": "user", "content": "hello"}]),
            tools: vec![json!({
                "type": "function",
                "name": "lookup",
                "parameters": {"type": "object"}
            })],
            instructions: Some("Answer clearly".to_string()),
            parameters: ModelGatewayParameters {
                maximum_output_tokens: 4_096,
                reasoning_effort: Some("high".to_string()),
                temperature: Some(0.2),
                native_compaction_threshold: (protocol == ModelProtocol::Responses)
                    .then_some(200_000),
            },
        }
    }

    fn provider_response(protocol: ModelProtocol) -> AiResponse {
        AiResponse {
            content: "done".to_string(),
            reasoning: Some("thinking".to_string()),
            finish_reason: (protocol == ModelProtocol::ChatCompletions).then(|| "stop".to_string()),
            usage: Some(json!({"input_tokens": 10, "output_tokens": 2})),
            response_id: (protocol == ModelProtocol::Responses).then(|| "resp_1".to_string()),
            response_status: (protocol == ModelProtocol::Responses)
                .then(|| "completed".to_string()),
            terminal_event_type: (protocol == ModelProtocol::Responses)
                .then(|| "response.completed".to_string()),
            terminal_event_seen: protocol == ModelProtocol::Responses,
            provider_request_id: Some("req_1".to_string()),
            provider_http_status: Some(200),
            response_output_items: if protocol == ModelProtocol::Responses {
                vec![json!({"type": "message", "id": "msg_1"})]
            } else {
                Vec::new()
            },
            ..AiResponse::default()
        }
    }

    #[test]
    fn descriptor_contains_only_frozen_non_secret_runtime_data() {
        let descriptor = descriptor_from_runtime(&complete_runtime()).expect("descriptor");
        let value = serde_json::to_value(descriptor).expect("serialize descriptor");
        let serialized = value.to_string();
        assert_eq!(value["revision"], 7);
        assert_eq!(value["protocol"], "responses");
        assert_eq!(value["context_strategy"], "provider_native");
        assert!(!serialized.contains("secret-key-must-not-leak"));
        assert!(!serialized.contains("api.openai.com"));
    }

    #[test]
    fn descriptor_rejects_incomplete_model_metadata() {
        let mut runtime = complete_runtime();
        runtime.context_window_tokens = None;
        assert_eq!(
            descriptor_from_runtime(&runtime),
            Err("model configuration is missing a positive context_window_tokens")
        );
    }

    #[test]
    fn runtime_identity_must_match_authenticated_request() {
        let runtime = complete_runtime();
        assert!(validate_runtime_identity(&runtime, "user-1", "model-1").is_ok());
        assert!(validate_runtime_identity(&runtime, "other-user", "model-1").is_err());
        assert!(validate_runtime_identity(&runtime, "user-1", "other-model").is_err());
    }

    #[test]
    fn gateway_payload_uses_only_frozen_server_parameters() {
        let runtime = complete_runtime();
        let request = gateway_request(ModelProtocol::Responses);
        validate_gateway_parameter_policy(&request, &runtime).unwrap();
        let payload = build_provider_payload(&request, &runtime);

        assert_eq!(payload["model"], "gpt-test");
        assert_eq!(payload["store"], false);
        assert_eq!(payload["temperature"], 0.2);
        assert_eq!(payload["reasoning"]["effort"], "high");
        assert_eq!(
            payload["context_management"][0]["compact_threshold"],
            200_000
        );
        let serialized = payload.to_string();
        assert!(!serialized.contains("secret-key-must-not-leak"));
        assert!(!serialized.contains("api.openai.com"));

        let mut mismatched = request;
        mismatched.parameters.temperature = Some(0.3);
        assert!(validate_gateway_parameter_policy(&mismatched, &runtime).is_err());
        mismatched.parameters.temperature = Some(0.2);
        mismatched.parameters.reasoning_effort = Some("low".to_string());
        assert!(validate_gateway_parameter_policy(&mismatched, &runtime).is_err());
    }

    #[test]
    fn responses_terminals_preserve_all_official_terminal_states() {
        let completed = terminal_from_ai_response(
            ModelProtocol::Responses,
            provider_response(ModelProtocol::Responses),
        )
        .unwrap();
        assert_eq!(completed.status, ModelGatewayTerminalStatus::Completed);
        assert_eq!(completed.source, ModelGatewayTerminalSource::Provider);
        assert_eq!(completed.response_id.as_deref(), Some("resp_1"));
        assert!(completed.usage.is_some());
        assert_eq!(completed.output_items.len(), 1);

        let mut incomplete_response = provider_response(ModelProtocol::Responses);
        incomplete_response.response_status = Some("incomplete".to_string());
        incomplete_response.terminal_event_type = Some("response.incomplete".to_string());
        incomplete_response.incomplete_details = Some(json!({"reason": "max_output_tokens"}));
        let incomplete =
            terminal_from_ai_response(ModelProtocol::Responses, incomplete_response).unwrap();
        assert_eq!(incomplete.status, ModelGatewayTerminalStatus::Incomplete);
        assert!(incomplete.incomplete_details.is_some());

        let mut failed_response = provider_response(ModelProtocol::Responses);
        failed_response.response_status = Some("failed".to_string());
        failed_response.terminal_event_type = Some("response.failed".to_string());
        failed_response.provider_error = Some(json!({"code": "provider_failed"}));
        let failed = terminal_from_ai_response(ModelProtocol::Responses, failed_response).unwrap();
        assert_eq!(failed.status, ModelGatewayTerminalStatus::Failed);
        assert!(failed.provider_error.is_some());
    }

    #[test]
    fn provider_eof_or_created_event_is_never_a_success_terminal() {
        let mut response = provider_response(ModelProtocol::Responses);
        response.terminal_event_seen = false;
        response.terminal_event_type = None;
        assert!(terminal_from_ai_response(ModelProtocol::Responses, response).is_err());

        let mut created = provider_response(ModelProtocol::Responses);
        created.response_status = Some("in_progress".to_string());
        created.terminal_event_type = Some("response.created".to_string());
        assert!(terminal_from_ai_response(ModelProtocol::Responses, created).is_err());
    }

    #[test]
    fn chat_completions_finish_reasons_map_to_formal_terminals() {
        for reason in ["stop", "tool_calls"] {
            let mut response = provider_response(ModelProtocol::ChatCompletions);
            response.finish_reason = Some(reason.to_string());
            let terminal =
                terminal_from_ai_response(ModelProtocol::ChatCompletions, response).unwrap();
            assert_eq!(terminal.status, ModelGatewayTerminalStatus::Completed);
            assert_eq!(terminal.terminal_event, format!("chat.completion.{reason}"));
        }
        for reason in ["length", "content_filter"] {
            let mut response = provider_response(ModelProtocol::ChatCompletions);
            response.finish_reason = Some(reason.to_string());
            let terminal =
                terminal_from_ai_response(ModelProtocol::ChatCompletions, response).unwrap();
            assert_eq!(terminal.status, ModelGatewayTerminalStatus::Incomplete);
            assert!(terminal.incomplete_details.is_some());
        }

        let mut missing = provider_response(ModelProtocol::ChatCompletions);
        missing.finish_reason = None;
        assert!(terminal_from_ai_response(ModelProtocol::ChatCompletions, missing).is_err());
    }

    async fn completed_provider_stream(
        State(hits): State<Arc<AtomicUsize>>,
    ) -> (HeaderMap, String) {
        hits.fetch_add(1, Ordering::SeqCst);
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream"),
        );
        headers.insert("x-request-id", HeaderValue::from_static("provider-req-1"));
        let completed = json!({
            "type": "response.completed",
            "response": {
                "id": "resp_1",
                "status": "completed",
                "output": [{
                    "type": "message",
                    "id": "msg_1",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "done"}]
                }],
                "usage": {"input_tokens": 10, "output_tokens": 2}
            }
        });
        (headers, format!("data: {completed}\n\n"))
    }

    async fn start_provider(app: axum::Router) -> (String, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind provider");
        let address = listener.local_addr().expect("provider address");
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{address}"), server)
    }

    #[tokio::test]
    async fn gateway_executes_exactly_one_provider_request_and_emits_one_terminal() {
        let hits = Arc::new(AtomicUsize::new(0));
        let (base_url, server) = start_provider(
            axum::Router::new()
                .route("/responses", post(completed_provider_stream))
                .with_state(Arc::clone(&hits)),
        )
        .await;
        let mut runtime = complete_runtime();
        runtime.base_url = base_url.clone();
        let request = gateway_request(ModelProtocol::Responses);
        let payload = build_provider_payload(&request, &runtime);
        let cancellation = CancellationToken::new();
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let emitter = Arc::new(std::sync::Mutex::new(GatewayEventEmitter::new(
            request.request_id.clone(),
            request.protocol,
            sender,
            cancellation.clone(),
        )));

        run_single_provider_request(
            request,
            runtime,
            payload,
            stream_callbacks(Arc::clone(&emitter)),
            Arc::clone(&emitter),
            cancellation,
        )
        .await;
        server.abort();

        assert_eq!(hits.load(Ordering::SeqCst), 1);
        let mut terminal_count = 0;
        let mut expected_sequence = 1;
        while let Ok(event) = receiver.try_recv() {
            let serialized = event.data;
            assert!(!serialized.contains("secret-key-must-not-leak"));
            assert!(!serialized.contains(base_url.as_str()));
            let envelope: super::ModelGatewayStreamEnvelope =
                serde_json::from_str(&serialized).expect("valid gateway envelope");
            assert_eq!(envelope.sequence, expected_sequence);
            expected_sequence += 1;
            if matches!(envelope.event, ModelGatewayStreamEvent::Terminal { .. }) {
                terminal_count += 1;
            }
        }
        assert_eq!(terminal_count, 1);
        assert!(expected_sequence > 1);
    }

    #[test]
    fn dropping_the_client_stream_cancels_the_provider_request() {
        let cancellation = CancellationToken::new();
        let (_sender, receiver) = mpsc::unbounded_channel();
        let stream = GatewayEventReceiver::new(receiver, cancellation.clone());
        assert!(!cancellation.is_cancelled());
        drop(stream);
        assert!(cancellation.is_cancelled());
    }

    #[tokio::test]
    async fn descriptor_route_requires_authentication() {
        let response = router()
            .oneshot(
                Request::get("/api/model-gateway/descriptors/model-1")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
