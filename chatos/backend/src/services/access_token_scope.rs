// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::future::Future;

tokio::task_local! {
    static ACCESS_TOKEN_SCOPE: Option<String>;
    static TRUSTED_BACKGROUND_SERVICE_SCOPE: bool;
}

pub async fn with_access_token_scope<T, Fut>(access_token: Option<String>, future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    ACCESS_TOKEN_SCOPE
        .scope(normalize_optional_token(access_token), future)
        .await
}

pub fn spawn_with_current_access_token<Fut>(future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    spawn_with_access_token(current_access_token(), future)
}

pub fn spawn_with_access_token<Fut>(
    access_token: Option<String>,
    future: Fut,
) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let access_token = normalize_optional_token(access_token);
    tokio::spawn(async move {
        TRUSTED_BACKGROUND_SERVICE_SCOPE
            .scope(true, with_access_token_scope(access_token, future))
            .await
    })
}

fn current_access_token() -> Option<String> {
    ACCESS_TOKEN_SCOPE
        .try_with(|token| token.clone())
        .ok()
        .flatten()
        .and_then(|token| normalize_optional_token(Some(token)))
}

pub fn get_current_access_token() -> Option<String> {
    current_access_token()
}

/// Detached work is authorized by the request before it is spawned, but it can
/// outlive the user's access token. Internal persistence should therefore use
/// ChatOS' signed service identity while the original user token remains
/// available for user-scoped Project, MCP, and Local Connector calls.
pub fn prefer_internal_memory_service_auth() -> bool {
    TRUSTED_BACKGROUND_SERVICE_SCOPE
        .try_with(|enabled| *enabled)
        .unwrap_or(false)
}

fn normalize_optional_token(token: Option<String>) -> Option<String> {
    token.and_then(|value| {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn request_scope_keeps_user_memory_auth_by_default() {
        with_access_token_scope(Some(" user-token ".to_string()), async {
            assert_eq!(get_current_access_token().as_deref(), Some("user-token"));
            assert!(!prefer_internal_memory_service_auth());
        })
        .await;
    }

    #[tokio::test]
    async fn detached_scope_preserves_user_token_and_prefers_internal_memory_auth() {
        with_access_token_scope(Some(" user-token ".to_string()), async {
            let result = spawn_with_current_access_token(async {
                (
                    get_current_access_token(),
                    prefer_internal_memory_service_auth(),
                )
            })
            .await
            .expect("detached task");
            assert_eq!(result.0.as_deref(), Some("user-token"));
            assert!(result.1);
        })
        .await;
    }

    #[tokio::test]
    async fn detached_scope_accepts_an_explicit_request_token() {
        let result = spawn_with_access_token(Some(" explicit-token ".to_string()), async {
            (
                get_current_access_token(),
                prefer_internal_memory_service_auth(),
            )
        })
        .await
        .expect("detached task");
        assert_eq!(result.0.as_deref(), Some("explicit-token"));
        assert!(result.1);
    }
}
