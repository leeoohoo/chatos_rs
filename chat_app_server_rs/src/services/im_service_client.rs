use std::future::Future;

mod auth;
mod contact_ops;
mod dto;
mod http;
mod conversation_ops;

pub use dto::*;

tokio::task_local! {
    static IM_SERVICE_ACCESS_TOKEN: Option<String>;
}

pub use self::auth::{auth_login, auth_me};
pub use self::contact_ops::{create_contact, get_contact, list_contacts};
pub use self::conversation_ops::{
    create_action_request_internal, create_conversation, create_conversation_message,
    create_conversation_message_internal,
    create_run_internal, get_action_request_internal, get_conversation, list_action_requests,
    list_conversation_messages, list_conversations, list_runs, mark_conversation_read,
    publish_internal_event, update_action_request_internal, update_conversation,
    update_run_internal,
};

pub async fn with_access_token_scope<T, Fut>(access_token: Option<String>, future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    IM_SERVICE_ACCESS_TOKEN
        .scope(normalize_optional_token(access_token), future)
        .await
}

pub(crate) fn current_access_token() -> Option<String> {
    IM_SERVICE_ACCESS_TOKEN
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
