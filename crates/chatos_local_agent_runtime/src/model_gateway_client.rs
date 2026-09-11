// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chatos_local_agent_protocol::{
    ModelGatewayRequest, ModelGatewayStreamEnvelope, ModelGatewayStreamEvent,
    ModelRuntimeDescriptor, MAX_MODEL_GATEWAY_JSON_BYTES, MAX_MODEL_GATEWAY_STREAM_BYTES,
};
use chatos_service_runtime::{
    classify_http_request_error, consume_sse_json_stream_with_progress_timeout,
    http_body::{
        read_response_json_limited, read_response_preview_text_limited_or_message,
        ERROR_BODY_PREVIEW_LIMIT_BYTES,
    },
};
use futures_util::StreamExt;
use reqwest::{header, Client, Url};
use tokio_util::sync::CancellationToken;

use crate::{ModelGatewayOutput, ModelGatewayStreamAccumulator, ModelGatewayStreamError};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DEFAULT_PROGRESS_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Clone, Default)]
pub struct ModelGatewayCallbacks {
    pub on_content: Option<Arc<dyn Fn(String) + Send + Sync>>,
    pub on_reasoning: Option<Arc<dyn Fn(String) + Send + Sync>>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ModelGatewayClientError {
    #[error("invalid model gateway configuration: {0}")]
    InvalidConfiguration(String),
    #[error("model gateway request was cancelled")]
    Cancelled,
    #[error("model gateway transport failed ({kind})")]
    Transport { kind: &'static str },
    #[error("model gateway returned HTTP {status}: {detail}")]
    HttpStatus { status: u16, detail: String },
    #[error("model gateway returned an invalid content type")]
    InvalidContentType,
    #[error("model gateway descriptor response is invalid: {0}")]
    InvalidDescriptor(String),
    #[error("model gateway emitted malformed JSON")]
    MalformedStreamJson,
    #[error("model gateway SSE stream failed: {0}")]
    StreamTransport(String),
    #[error(transparent)]
    StreamContract(#[from] ModelGatewayStreamError),
}

#[async_trait]
pub trait ModelGatewayClient: Send + Sync {
    async fn descriptor(
        &self,
        access_token: &str,
        model_config_id: &str,
        cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError>;

    async fn stream(
        &self,
        access_token: &str,
        descriptor: &ModelRuntimeDescriptor,
        request: ModelGatewayRequest,
        callbacks: ModelGatewayCallbacks,
        cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError>;
}

#[derive(Clone)]
pub struct HttpModelGatewayClient {
    client: Client,
    base_url: Url,
    progress_timeout: Duration,
}

impl HttpModelGatewayClient {
    pub fn new(base_url: &str) -> Result<Self, ModelGatewayClientError> {
        Self::with_progress_timeout(base_url, DEFAULT_PROGRESS_TIMEOUT)
    }

    pub fn with_progress_timeout(
        base_url: &str,
        progress_timeout: Duration,
    ) -> Result<Self, ModelGatewayClientError> {
        if progress_timeout.is_zero() {
            return Err(ModelGatewayClientError::InvalidConfiguration(
                "progress timeout must be positive".to_string(),
            ));
        }
        let mut base_url = Url::parse(base_url.trim()).map_err(|_| {
            ModelGatewayClientError::InvalidConfiguration("base URL must be absolute".to_string())
        })?;
        if !matches!(base_url.scheme(), "http" | "https") {
            return Err(ModelGatewayClientError::InvalidConfiguration(
                "base URL must use HTTP or HTTPS".to_string(),
            ));
        }
        if base_url.query().is_some() || base_url.fragment().is_some() {
            return Err(ModelGatewayClientError::InvalidConfiguration(
                "base URL cannot contain a query or fragment".to_string(),
            ));
        }
        let normalized_path = base_url.path().trim_end_matches('/').to_string();
        base_url.set_path(normalized_path.as_str());
        let client = Client::builder()
            .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
            .read_timeout(progress_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                ModelGatewayClientError::InvalidConfiguration(
                    "HTTP client could not be built".to_string(),
                )
            })?;
        Ok(Self {
            client,
            base_url,
            progress_timeout,
        })
    }

    fn endpoint(&self, suffix: &str) -> Url {
        let mut endpoint = self.base_url.clone();
        let path = format!("{}/{}", endpoint.path().trim_end_matches('/'), suffix);
        endpoint.set_path(path.as_str());
        endpoint
    }

    fn descriptor_endpoint(&self, model_config_id: &str) -> Result<Url, ModelGatewayClientError> {
        let model_config_id = model_config_id.trim();
        if model_config_id.is_empty() {
            return Err(ModelGatewayClientError::InvalidConfiguration(
                "model config ID is required".to_string(),
            ));
        }
        let mut endpoint = self.endpoint("api/model-gateway/descriptors");
        endpoint
            .path_segments_mut()
            .map_err(|_| {
                ModelGatewayClientError::InvalidConfiguration(
                    "base URL cannot be a base".to_string(),
                )
            })?
            .push(model_config_id);
        Ok(endpoint)
    }

    fn bearer_token(access_token: &str) -> Result<&str, ModelGatewayClientError> {
        let access_token = access_token.trim();
        if access_token.is_empty() {
            return Err(ModelGatewayClientError::InvalidConfiguration(
                "access token is required".to_string(),
            ));
        }
        Ok(access_token)
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        cancellation: &CancellationToken,
    ) -> Result<reqwest::Response, ModelGatewayClientError> {
        tokio::select! {
            _ = cancellation.cancelled() => Err(ModelGatewayClientError::Cancelled),
            result = request.send() => result.map_err(|error| ModelGatewayClientError::Transport {
                kind: classify_http_request_error(&error).as_str(),
            }),
        }
    }

    async fn require_success(
        response: reqwest::Response,
        cancellation: &CancellationToken,
    ) -> Result<reqwest::Response, ModelGatewayClientError> {
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let detail = tokio::select! {
            _ = cancellation.cancelled() => return Err(ModelGatewayClientError::Cancelled),
            detail = read_response_preview_text_limited_or_message(
                response,
                ERROR_BODY_PREVIEW_LIMIT_BYTES,
            ) => detail,
        };
        Err(ModelGatewayClientError::HttpStatus { status, detail })
    }
}

#[async_trait]
impl ModelGatewayClient for HttpModelGatewayClient {
    async fn descriptor(
        &self,
        access_token: &str,
        model_config_id: &str,
        cancellation: CancellationToken,
    ) -> Result<ModelRuntimeDescriptor, ModelGatewayClientError> {
        let access_token = Self::bearer_token(access_token)?;
        let response = self
            .send(
                self.client
                    .get(self.descriptor_endpoint(model_config_id)?)
                    .bearer_auth(access_token)
                    .header(header::ACCEPT, "application/json"),
                &cancellation,
            )
            .await?;
        let response = Self::require_success(response, &cancellation).await?;
        let descriptor: ModelRuntimeDescriptor = tokio::select! {
            _ = cancellation.cancelled() => return Err(ModelGatewayClientError::Cancelled),
            result = read_response_json_limited(response, MAX_MODEL_GATEWAY_JSON_BYTES) => {
                result.map_err(ModelGatewayClientError::InvalidDescriptor)?
            },
        };
        descriptor
            .validate()
            .map_err(|error| ModelGatewayClientError::InvalidDescriptor(error.to_string()))?;
        if descriptor.model_config_id != model_config_id.trim() {
            return Err(ModelGatewayClientError::InvalidDescriptor(
                "descriptor model config ID does not match the request".to_string(),
            ));
        }
        Ok(descriptor)
    }

    async fn stream(
        &self,
        access_token: &str,
        descriptor: &ModelRuntimeDescriptor,
        request: ModelGatewayRequest,
        callbacks: ModelGatewayCallbacks,
        cancellation: CancellationToken,
    ) -> Result<ModelGatewayOutput, ModelGatewayClientError> {
        let access_token = Self::bearer_token(access_token)?;
        request
            .validate_against(descriptor)
            .map_err(|error| ModelGatewayClientError::InvalidConfiguration(error.to_string()))?;
        let response = self
            .send(
                self.client
                    .post(self.endpoint("api/model-gateway/stream"))
                    .bearer_auth(access_token)
                    .header(header::ACCEPT, "text/event-stream")
                    .json(&request),
                &cancellation,
            )
            .await?;
        let response = Self::require_success(response, &cancellation).await?;
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if !content_type.starts_with("text/event-stream") {
            return Err(ModelGatewayClientError::InvalidContentType);
        }

        let mut accumulator =
            ModelGatewayStreamAccumulator::new(request.request_id.clone(), request.protocol);
        let child_cancellation = cancellation.child_token();
        let parser_cancellation = child_cancellation.clone();
        let mut event_error = None;
        let mut received_bytes = 0usize;
        let stats = consume_sse_json_stream_with_progress_timeout(
            response.bytes_stream().map(move |chunk| {
                let chunk = chunk.map_err(|error| error.to_string())?;
                received_bytes = received_bytes.saturating_add(chunk.len());
                if received_bytes > MAX_MODEL_GATEWAY_STREAM_BYTES {
                    return Err("model gateway stream exceeded its byte limit".to_string());
                }
                Ok(chunk)
            }),
            Some(parser_cancellation),
            Some(self.progress_timeout),
            |value| {
                if event_error.is_some() {
                    return;
                }
                let envelope = match serde_json::from_value::<ModelGatewayStreamEnvelope>(value) {
                    Ok(envelope) => envelope,
                    Err(_) => {
                        event_error = Some(ModelGatewayClientError::MalformedStreamJson);
                        child_cancellation.cancel();
                        return;
                    }
                };
                let callback = match &envelope.event {
                    ModelGatewayStreamEvent::ContentDelta { delta } => callbacks
                        .on_content
                        .as_ref()
                        .map(|callback| (Arc::clone(callback), delta.clone())),
                    _ => None,
                };
                let reasoning_callback = match &envelope.event {
                    ModelGatewayStreamEvent::ReasoningDelta { delta } => callbacks
                        .on_reasoning
                        .as_ref()
                        .map(|callback| (Arc::clone(callback), delta.clone())),
                    _ => None,
                };
                if let Err(error) = accumulator.accept(envelope) {
                    event_error = Some(error.into());
                    child_cancellation.cancel();
                    return;
                }
                if let Some((callback, delta)) = callback {
                    callback(delta);
                }
                if let Some((callback, delta)) = reasoning_callback {
                    callback(delta);
                }
            },
        )
        .await;

        if let Some(error) = event_error {
            return Err(error);
        }
        if cancellation.is_cancelled() {
            return Err(ModelGatewayClientError::Cancelled);
        }
        let stats =
            stats.map_err(|error| ModelGatewayClientError::StreamTransport(error.message))?;
        if stats.malformed_event_count > 0 {
            return Err(ModelGatewayClientError::MalformedStreamJson);
        }
        accumulator.finish().map_err(Into::into)
    }
}
