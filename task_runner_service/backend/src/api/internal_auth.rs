// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use axum::http::{HeaderMap, StatusCode};

use crate::config::AppConfig;

pub(super) const TASK_RUNNER_TOKEN_AUDIENCE: &str = "task-runner";
pub(super) const CHATOS_MESSAGES_READ_SCOPE: &str = "chatos.messages.read";
pub(super) const EXECUTION_OPTIONS_READ_SCOPE: &str = "execution-options.read";
pub(super) const CHATOS_CALLER: &str = "chatos-backend";
pub(super) const PROJECT_SERVICE_CALLER: &str = "project-service";

#[derive(Debug)]
pub(super) struct InternalAuthError {
    pub status: StatusCode,
    pub message: String,
}

pub(super) fn require_task_runner_internal_request(
    config: &AppConfig,
    headers: &HeaderMap,
    allowed_callers: &[&str],
    required_scope: &str,
) -> Result<(), InternalAuthError> {
    let caller = header_text(headers, "x-task-runner-caller");
    let token = header_text(headers, "x-task-runner-internal-token");
    if let Some(caller) = caller {
        if !allowed_callers.contains(&caller) {
            return Err(forbidden(
                "caller service is not allowed for this task runner operation",
            ));
        }
        let expected = caller_secret(config, caller)
            .ok_or_else(|| unauthorized("task runner internal API is disabled for caller"))?;
        if let Some(token) = token {
            chatos_service_runtime::verify_internal_service_token(
                token,
                expected,
                caller,
                TASK_RUNNER_TOKEN_AUDIENCE,
                required_scope,
            )
            .map_err(|_| unauthorized("invalid task runner internal API token"))?;
            return Ok(());
        }
        if chatos_service_runtime::is_production_environment() {
            return Err(unauthorized(
                "signed task runner internal API token is required",
            ));
        }
        require_legacy_secret(headers, expected)?;
        return Ok(());
    }

    if token.is_some() {
        return Err(bad_request(
            "task runner caller is required for signed internal requests",
        ));
    }
    if chatos_service_runtime::is_production_environment() {
        return Err(unauthorized(
            "signed task runner internal API token is required",
        ));
    }
    let caller = allowed_callers
        .first()
        .copied()
        .ok_or_else(|| forbidden("no task runner internal caller is allowed"))?;
    let expected = caller_secret(config, caller)
        .ok_or_else(|| forbidden("task runner internal API secret is not configured"))?;
    require_legacy_secret(headers, expected)
}

fn caller_secret<'a>(config: &'a AppConfig, caller: &str) -> Option<&'a str> {
    let value = match caller {
        CHATOS_CALLER => config.chatos_internal_api_secret.as_deref(),
        PROJECT_SERVICE_CALLER => config.internal_api_secret.as_deref(),
        _ => None,
    };
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn require_legacy_secret(headers: &HeaderMap, expected: &str) -> Result<(), InternalAuthError> {
    let provided = header_text(headers, "x-task-runner-internal-secret")
        .or_else(|| header_text(headers, "x-project-service-sync-secret"))
        .ok_or_else(|| unauthorized("missing task runner internal api secret"))?;
    if !constant_time_eq(expected.as_bytes(), provided.as_bytes()) {
        return Err(unauthorized("invalid task runner internal api secret"));
    }
    Ok(())
}

fn header_text<'a>(headers: &'a HeaderMap, key: &'static str) -> Option<&'a str> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn constant_time_eq(expected: &[u8], actual: &[u8]) -> bool {
    let mut difference = expected.len() ^ actual.len();
    for (left, right) in expected.iter().zip(actual.iter()) {
        difference |= usize::from(left ^ right);
    }
    difference == 0
}

fn unauthorized(message: impl Into<String>) -> InternalAuthError {
    InternalAuthError {
        status: StatusCode::UNAUTHORIZED,
        message: message.into(),
    }
}

fn forbidden(message: impl Into<String>) -> InternalAuthError {
    InternalAuthError {
        status: StatusCode::FORBIDDEN,
        message: message.into(),
    }
}

fn bad_request(message: impl Into<String>) -> InternalAuthError {
    InternalAuthError {
        status: StatusCode::BAD_REQUEST,
        message: message.into(),
    }
}
