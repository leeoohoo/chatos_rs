// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;

use axum::body::Body;
use axum::extract::{Extension, Path, Query, Request, State};
use axum::http::header::{ETAG, IF_NONE_MATCH};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chatos_config_sdk::{
    CONFIG_CENTER_AUDIENCE, CONFIG_CENTER_CALLER_HEADER, CONFIG_CENTER_TOKEN_HEADER,
    CONFIG_INSTANCE_HEARTBEAT_SCOPE, CONFIG_SNAPSHOT_READ_SCOPE,
};
use chatos_service_runtime::{verify_internal_service_token, InternalServiceTokenClaims};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::auth;
use crate::models::{
    ConfigDraftRecord, CurrentUser, CustomDefinitionRequest, DraftUpdateRequest, HealthResponse,
    InstanceHeartbeatRequest, LoginRequest, PublishRequest, ServiceInstanceRecord,
};
use crate::state::AppState;

mod internal;
mod public;

pub use internal::build_internal_router;
pub use public::build_public_router;

fn result_json<T>(result: Result<T, String>) -> Response
where
    T: serde::Serialize,
{
    match result {
        Ok(value) => Json(value).into_response(),
        Err(err) => error(StatusCode::BAD_REQUEST, err),
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": message.into() }))).into_response()
}

#[allow(dead_code)]
fn _type_anchors(_draft: ConfigDraftRecord, _values: BTreeMap<String, Value>) {}
