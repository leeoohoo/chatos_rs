use std::future::Future;

tokio::task_local! {
    static ACCESS_TOKEN_SCOPE: Option<String>;
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
    let access_token = current_access_token();
    tokio::spawn(async move { with_access_token_scope(access_token, future).await })
}

fn current_access_token() -> Option<String> {
    ACCESS_TOKEN_SCOPE
        .try_with(|token| token.clone())
        .ok()
        .flatten()
        .and_then(|token| normalize_optional_token(Some(token)))
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
