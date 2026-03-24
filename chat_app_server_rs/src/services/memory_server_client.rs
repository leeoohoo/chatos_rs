use std::future::Future;

mod agent_ops;
mod auth;
mod contact_ops;
mod dto;
mod http;
mod mapping;
mod session_ops;
mod skill_ops;
#[cfg(test)]
mod tests;

pub use dto::*;

tokio::task_local! {
    static MEMORY_SERVER_ACCESS_TOKEN: Option<String>;
}

pub use self::agent_ops::{
    ai_create_memory_agent, create_memory_agent, delete_memory_agent, get_memory_agent,
    get_memory_agent_runtime_context, list_memory_agents, update_memory_agent,
};
pub use self::auth::{auth_login, auth_me};
pub use self::contact_ops::{
    create_memory_contact, delete_memory_contact, list_contact_agent_recalls,
    list_contact_project_memories, list_contact_project_memories_by_contact, list_contact_projects,
    list_memory_contacts, list_project_contacts, sync_memory_project, sync_project_agent_link,
};
pub use self::session_ops::{
    clear_summaries, compose_context, create_session, delete_message, delete_messages_by_session,
    delete_session, delete_summary, get_message_by_id, get_session_by_id, get_summary_job_config,
    list_messages, list_sessions, list_summaries, update_session, upsert_message,
    upsert_summary_job_config,
};
pub use self::skill_ops::get_memory_skill;

pub async fn with_access_token_scope<T, Fut>(access_token: Option<String>, future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    MEMORY_SERVER_ACCESS_TOKEN
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
    MEMORY_SERVER_ACCESS_TOKEN
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
