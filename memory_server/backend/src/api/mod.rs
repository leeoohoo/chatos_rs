use std::sync::Arc;

use axum::routing::{delete, get, patch, post, put};
use axum::{Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;
mod agents_ai_api;
mod agents_api;
mod auth_api;
mod auth_session_api;
mod auth_users_api;
mod configs_api;
mod configs_job_configs_api;
mod configs_models_api;
mod contacts_api;
mod contacts_context_api;
mod contacts_crud_api;
mod context_api;
mod jobs_api;
mod messages_api;
mod messages_summaries_api;
mod projects_api;
mod projects_base_api;
mod projects_links_api;
mod sessions_api;
mod shared;
mod skills_api;
mod skills_manage_api;
mod summaries_api;
use self::shared::{
    build_ai_client, build_auth_token, default_project_name, ensure_admin,
    ensure_agent_manage_access, ensure_agent_read_access, ensure_contact_access,
    ensure_contact_manage_access, ensure_session_access, normalize_optional_text,
    normalize_project_scope_id, pick_latest_timestamp, require_auth, resolve_scope_user_id,
    resolve_visible_user_ids,
};

pub type SharedState = Arc<AppState>;

pub fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/memory/v1/auth/login", post(auth_api::login))
        .route("/api/memory/v1/auth/me", get(auth_api::me))
        .route(
            "/api/memory/v1/auth/users",
            get(auth_api::list_users).post(auth_api::create_user),
        )
        .route(
            "/api/memory/v1/auth/users/:username",
            patch(auth_api::update_user).delete(auth_api::delete_user),
        )
        .route(
            "/api/memory/v1/sessions",
            post(sessions_api::create_session).get(sessions_api::list_sessions),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/sync",
            put(sessions_api::sync_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id",
            get(sessions_api::get_session)
                .patch(sessions_api::update_session)
                .delete(sessions_api::delete_session),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages",
            post(messages_summaries_api::create_message)
                .get(messages_summaries_api::list_messages)
                .delete(messages_summaries_api::clear_session_messages),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages/:message_id/sync",
            put(messages_summaries_api::sync_message),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/messages/batch",
            post(messages_summaries_api::batch_create_messages),
        )
        .route(
            "/api/memory/v1/contacts",
            get(contacts_api::list_contacts).post(contacts_api::create_contact),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id",
            delete(contacts_api::delete_contact),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/project-memories",
            get(contacts_api::list_contact_project_memories),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/project-memories/:project_id",
            get(contacts_api::list_contact_project_memories_by_project),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/projects",
            get(contacts_api::list_contact_projects),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/projects/:project_id/summaries",
            get(contacts_api::list_contact_project_summaries),
        )
        .route(
            "/api/memory/v1/contacts/:contact_id/agent-recalls",
            get(contacts_api::list_contact_agent_recalls),
        )
        .route("/api/memory/v1/projects", get(projects_api::list_projects))
        .route(
            "/api/memory/v1/projects/:project_id/contacts",
            get(projects_api::list_project_contacts),
        )
        .route(
            "/api/memory/v1/projects/sync",
            post(projects_api::sync_project),
        )
        .route(
            "/api/memory/v1/project-agent-links/sync",
            post(projects_api::sync_project_agent_link),
        )
        .route(
            "/api/memory/v1/agents",
            get(agents_api::list_agents).post(agents_api::create_agent),
        )
        .route("/api/memory/v1/skills", get(skills_api::list_skills))
        .route(
            "/api/memory/v1/skills/:skill_id",
            get(skills_api::get_skill),
        )
        .route(
            "/api/memory/v1/skills/plugins",
            get(skills_api::list_skill_plugins),
        )
        .route(
            "/api/memory/v1/skills/import-git",
            post(skills_manage_api::import_skills_from_git),
        )
        .route(
            "/api/memory/v1/skills/plugins/install",
            post(skills_manage_api::install_skill_plugins),
        )
        .route(
            "/api/memory/v1/agents/ai-create",
            post(agents_ai_api::ai_create_agent),
        )
        .route(
            "/api/memory/v1/agents/:agent_id/sessions",
            get(agents_api::list_agent_sessions),
        )
        .route(
            "/api/memory/v1/agents/:agent_id/runtime-context",
            get(agents_api::get_agent_runtime_context),
        )
        .route(
            "/api/memory/v1/agents/:agent_id",
            get(agents_api::get_agent)
                .patch(agents_api::update_agent)
                .delete(agents_api::delete_agent),
        )
        .route(
            "/api/memory/v1/messages/:message_id",
            get(messages_summaries_api::get_message).delete(messages_summaries_api::delete_message),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries",
            get(messages_summaries_api::list_summaries),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries/levels",
            get(messages_summaries_api::summary_levels),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries/graph",
            get(messages_summaries_api::summary_graph),
        )
        .route(
            "/api/memory/v1/sessions/:session_id/summaries/:summary_id",
            delete(messages_summaries_api::delete_summary),
        )
        .route(
            "/api/memory/v1/configs/models",
            get(configs_api::list_model_configs).post(configs_api::create_model_config),
        )
        .route(
            "/api/memory/v1/configs/models/:model_id",
            patch(configs_api::update_model_config).delete(configs_api::delete_model_config),
        )
        .route(
            "/api/memory/v1/configs/models/:model_id/test",
            post(configs_api::test_model_config),
        )
        .route(
            "/api/memory/v1/configs/summary-job",
            get(configs_api::get_summary_job_config).put(configs_api::put_summary_job_config),
        )
        .route(
            "/api/memory/v1/configs/summary-rollup-job",
            get(configs_api::get_summary_rollup_job_config)
                .put(configs_api::put_summary_rollup_job_config),
        )
        .route(
            "/api/memory/v1/configs/agent-memory-job",
            get(configs_api::get_agent_memory_job_config)
                .put(configs_api::put_agent_memory_job_config),
        )
        .route(
            "/api/memory/v1/jobs/summary/run-once",
            post(jobs_api::run_summary_once),
        )
        .route(
            "/api/memory/v1/jobs/summary-rollup/run-once",
            post(jobs_api::run_rollup_once),
        )
        .route(
            "/api/memory/v1/jobs/agent-memory/run-once",
            post(jobs_api::run_agent_memory_once),
        )
        .route("/api/memory/v1/jobs/runs", get(jobs_api::list_job_runs))
        .route("/api/memory/v1/jobs/stats", get(jobs_api::job_stats))
        .route(
            "/api/memory/v1/context/compose",
            post(context_api::compose_context),
        )
        .with_state(state)
}

async fn health() -> Json<Value> {
    Json(json!({"status": "ok", "service": "memory_server"}))
}
