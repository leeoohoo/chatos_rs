use std::future::Future;

mod agent_ops;
#[allow(dead_code)]
mod auth;
mod contact_ops;
mod dto;
mod http;
mod mapping;
mod model_config_ops;
mod session_ops;
mod skill_ops;
mod task_execution_ops;
#[cfg(test)]
mod tests;

pub use dto::*;

tokio::task_local! {
    static MEMORY_SERVER_ACCESS_TOKEN: Option<String>;
}

tokio::task_local! {
    static MEMORY_SERVER_INTERNAL_SCOPE: bool;
}

pub use self::agent_ops::{
    ai_create_memory_agent, create_memory_agent, delete_memory_agent, get_memory_agent,
    get_memory_agent_runtime_context, list_memory_agents, update_memory_agent,
};
#[allow(unused_imports)]
pub use self::auth::{auth_login, auth_me};
pub use self::contact_ops::{
    create_memory_contact, delete_memory_contact, get_contact_builtin_mcp_grants,
    list_contact_agent_recalls, list_contact_project_memories,
    list_contact_project_memories_by_contact, list_contact_projects, list_memory_contacts,
    list_project_contacts, resolve_memory_contact, sync_memory_project, sync_project_agent_link,
    update_contact_builtin_mcp_grants,
};
pub use self::model_config_ops::get_memory_model_config;
pub use self::session_ops::{
    clear_summaries, compose_context, create_session, delete_message, delete_messages_by_session,
    delete_session, delete_summary, get_latest_turn_runtime_snapshot, get_message_by_id,
    get_session_by_id, get_summary_job_config, get_task_execution_rollup_job_config,
    get_task_execution_summary_job_config, get_turn_runtime_snapshot_by_turn, list_messages,
    list_sessions, list_summaries, run_task_execution_summary_once_for_scope,
    sync_turn_runtime_snapshot, update_session, upsert_message, upsert_summary_job_config, upsert_task_execution_rollup_job_config,
    upsert_task_execution_summary_job_config,
};
pub use self::skill_ops::{get_memory_skill, get_memory_skill_plugin};
pub use self::task_execution_ops::{
    compose_task_execution_context, list_task_execution_messages,
    upsert_task_execution_message, upsert_task_result_brief, TaskExecutionScopeBinding,
};

pub async fn with_access_token_scope<T, Fut>(access_token: Option<String>, future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    MEMORY_SERVER_ACCESS_TOKEN
        .scope(normalize_optional_token(access_token), future)
        .await
}

pub async fn with_internal_scope<T, Fut>(future: Fut) -> T
where
    Fut: Future<Output = T>,
{
    MEMORY_SERVER_INTERNAL_SCOPE.scope(true, future).await
}

pub fn spawn_with_current_access_token<Fut>(future: Fut) -> tokio::task::JoinHandle<Fut::Output>
where
    Fut: Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let access_token = current_access_token();
    tokio::spawn(async move { with_access_token_scope(access_token, future).await })
}

pub(crate) fn current_access_token() -> Option<String> {
    MEMORY_SERVER_ACCESS_TOKEN
        .try_with(|token| token.clone())
        .ok()
        .flatten()
        .and_then(|token| normalize_optional_token(Some(token)))
}

pub(crate) fn is_internal_scope() -> bool {
    MEMORY_SERVER_INTERNAL_SCOPE
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
