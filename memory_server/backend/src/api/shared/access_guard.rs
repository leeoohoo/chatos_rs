use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::repositories::{
    agents as agents_repo, auth as auth_repo, contacts as contacts_repo, sessions,
};
use crate::state::AppState;

use super::auth_context::AuthIdentity;

pub(crate) async fn ensure_session_access(
    state: &AppState,
    auth: &AuthIdentity,
    session_id: &str,
) -> Result<(), (StatusCode, Json<Value>)> {
    match sessions::get_session_by_id(&state.pool, session_id).await {
        Ok(Some(session)) => {
            if auth.is_admin() || session.user_id == auth.user_id {
                Ok(())
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "session not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load session failed", "detail": err})),
        )),
    }
}

pub(crate) async fn ensure_contact_access(
    state: &AppState,
    auth: &AuthIdentity,
    contact_id: &str,
) -> Result<crate::models::Contact, (StatusCode, Json<Value>)> {
    match contacts_repo::get_contact_by_id(&state.pool, contact_id).await {
        Ok(Some(contact)) => {
            if auth.is_admin() || contact.user_id == auth.user_id {
                Ok(contact)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load contact failed", "detail": err})),
        )),
    }
}

pub(crate) async fn ensure_contact_manage_access(
    state: &AppState,
    auth: &AuthIdentity,
    contact_id: &str,
) -> Result<crate::models::Contact, (StatusCode, Json<Value>)> {
    match contacts_repo::get_contact_by_id(&state.pool, contact_id).await {
        Ok(Some(contact)) => {
            if contact.user_id == auth.user_id {
                Ok(contact)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "contact not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load contact failed", "detail": err})),
        )),
    }
}

pub(crate) async fn ensure_agent_read_access(
    state: &AppState,
    auth: &AuthIdentity,
    agent_id: &str,
) -> Result<crate::models::MemoryAgent, (StatusCode, Json<Value>)> {
    match agents_repo::get_agent_by_id(&state.pool, agent_id).await {
        Ok(Some(agent)) => {
            if auth.is_admin()
                || agent.user_id == auth.user_id
                || agent.user_id == auth_repo::ADMIN_USER_ID
            {
                Ok(agent)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load agent failed", "detail": err})),
        )),
    }
}

pub(crate) async fn ensure_agent_manage_access(
    state: &AppState,
    auth: &AuthIdentity,
    agent_id: &str,
) -> Result<crate::models::MemoryAgent, (StatusCode, Json<Value>)> {
    match agents_repo::get_agent_by_id(&state.pool, agent_id).await {
        Ok(Some(agent)) => {
            if agent.user_id == auth.user_id {
                Ok(agent)
            } else {
                Err((StatusCode::FORBIDDEN, Json(json!({"error": "forbidden"}))))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({"error": "agent not found"})),
        )),
        Err(err) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "load agent failed", "detail": err})),
        )),
    }
}
