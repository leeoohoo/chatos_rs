// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::Value;
use tracing::warn;

use crate::core::auth::AuthUser;
use crate::core::session_access::{is_owned_session, SessionAccessError};
use crate::models::memory_mapping_types::SyncMemoryProjectRequestDto;
use crate::models::project::{ProjectService, PUBLIC_PROJECT_ID};
use crate::models::session::Session;
use crate::services::chatos_memory_engine;
use crate::services::chatos_memory_mappings;
use crate::services::chatos_sessions;
use crate::services::realtime::publish_sessions_updated;

use super::session_scope::normalize_project_scope;

#[derive(Debug, Clone)]
pub struct CreateConversationSessionInput {
    pub actor_user_id: String,
    pub user_id: String,
    pub title: String,
    pub project_id: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct CompatCreateSessionInput {
    pub actor_user_id: String,
    pub user_id: String,
    pub title: Option<String>,
    pub project_id: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct SyncConversationSessionCompatInput {
    pub session_id: String,
    pub scope_user_id: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<Value>,
    pub status: Option<String>,
    pub existing_session: Option<Session>,
}

#[derive(Debug)]
pub enum CompatSyncSessionError {
    NotFound,
    Forbidden,
    Internal(String),
}

#[derive(Debug)]
pub enum CompatCreateSessionError {
    EmptyTitle,
    Internal(String),
}

pub async fn list_sessions(
    user_id: &str,
    project_id: Option<&str>,
    limit: Option<i64>,
    offset: i64,
    include_archived: bool,
    include_archiving: bool,
) -> Result<Vec<Session>, String> {
    chatos_sessions::list_sessions(
        Some(user_id),
        project_id,
        limit,
        offset,
        include_archived,
        include_archiving,
    )
    .await
}

pub async fn create_session(input: CreateConversationSessionInput) -> Result<Session, String> {
    let saved = chatos_sessions::create_session(
        input.user_id.clone(),
        input.title,
        input.project_id,
        input.metadata,
    )
    .await?;

    sync_session_memory_projections(&saved, &input.user_id).await;

    let project_scope = normalize_project_scope(saved.project_id.as_deref());
    publish_sessions_updated(
        input.actor_user_id.as_str(),
        "session_created",
        Some(saved.id.as_str()),
        Some(project_scope.as_str()),
        Some(saved.clone()),
    );

    Ok(saved)
}

pub async fn create_session_compat(
    input: CompatCreateSessionInput,
) -> Result<Session, CompatCreateSessionError> {
    let title =
        normalize_compat_session_title(input.title).ok_or(CompatCreateSessionError::EmptyTitle)?;
    create_session(CreateConversationSessionInput {
        actor_user_id: input.actor_user_id,
        user_id: input.user_id,
        title,
        project_id: input.project_id,
        metadata: input.metadata,
    })
    .await
    .map_err(CompatCreateSessionError::Internal)
}

pub async fn update_session(
    actor_user_id: &str,
    session_id: &str,
    title: Option<String>,
    metadata: Option<Value>,
) -> Result<Option<Session>, String> {
    let session = chatos_sessions::update_session(session_id, title, None, metadata).await?;
    if let Some(session) = session.as_ref() {
        let project_scope = normalize_project_scope(session.project_id.as_deref());
        publish_sessions_updated(
            actor_user_id,
            "session_updated",
            Some(session.id.as_str()),
            Some(project_scope.as_str()),
            Some(session.clone()),
        );
    }
    Ok(session)
}

pub async fn archive_session(actor_user_id: &str, session_id: &str) -> Result<bool, String> {
    let archived = chatos_sessions::delete_session(session_id).await?;
    if archived {
        publish_sessions_updated(
            actor_user_id,
            "session_deleted",
            Some(session_id),
            None,
            None,
        );
    }
    Ok(archived)
}

pub async fn get_session_by_id(session_id: &str) -> Result<Option<Session>, String> {
    chatos_sessions::get_session_by_id(session_id).await
}

pub async fn update_session_compat(
    session_id: &str,
    title: Option<String>,
    status: Option<String>,
    metadata: Option<Value>,
) -> Result<Option<Session>, String> {
    chatos_sessions::update_session(session_id, title, status, metadata).await
}

pub async fn sync_session_compat(
    input: SyncConversationSessionCompatInput,
) -> Result<Option<Session>, String> {
    let SyncConversationSessionCompatInput {
        session_id,
        scope_user_id,
        project_id,
        title,
        metadata,
        status,
        existing_session,
    } = input;

    if let Some(current) = existing_session {
        return chatos_memory_engine::update_chatos_session(
            session_id.as_str(),
            title.or(Some(current.title.clone())),
            status,
            metadata.or(current.metadata),
        )
        .await;
    }

    let title = title.unwrap_or_else(|| "Untitled".to_string());
    let mut created =
        chatos_memory_engine::create_chatos_session(scope_user_id, title, project_id, metadata)
            .await?;
    if created.id != session_id {
        created.id = session_id.clone();
        chatos_memory_engine::sync_chatos_session(&created).await?;
    }
    if let Some(status) = status {
        match chatos_memory_engine::update_chatos_session(
            session_id.as_str(),
            None,
            Some(status),
            None,
        )
        .await?
        {
            Some(updated) => Ok(Some(updated)),
            None => Ok(Some(created)),
        }
    } else {
        Ok(Some(created))
    }
}

pub async fn sync_session_compat_for_auth(
    auth: &AuthUser,
    input: SyncConversationSessionCompatInput,
) -> Result<Session, CompatSyncSessionError> {
    let existing_session = match input.existing_session {
        Some(session) => Some(session),
        None => match get_session_by_id(input.session_id.as_str()).await {
            Ok(session) => session,
            Err(err) => return Err(CompatSyncSessionError::Internal(err)),
        },
    };

    if let Some(current) = existing_session.as_ref() {
        if !is_owned_session(current, auth) {
            return Err(CompatSyncSessionError::Forbidden);
        }
    }

    sync_session_compat(SyncConversationSessionCompatInput {
        existing_session,
        ..input
    })
    .await
    .map_err(CompatSyncSessionError::Internal)?
    .ok_or(CompatSyncSessionError::NotFound)
}

pub fn map_compat_sync_session_error(err: CompatSyncSessionError) -> SessionAccessError {
    match err {
        CompatSyncSessionError::NotFound => SessionAccessError::NotFound,
        CompatSyncSessionError::Forbidden => SessionAccessError::Forbidden,
        CompatSyncSessionError::Internal(err) => SessionAccessError::Internal(err),
    }
}

pub fn normalize_compat_session_title(title: Option<String>) -> Option<String> {
    let normalized = title
        .unwrap_or_else(|| "Untitled".to_string())
        .trim()
        .to_string();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

async fn sync_session_memory_projections(session: &Session, user_id: &str) {
    let project_scope = normalize_project_scope(session.project_id.as_deref());

    if project_scope != PUBLIC_PROJECT_ID {
        if let Ok(Some(project)) = ProjectService::get_by_id(project_scope.as_str()).await {
            let same_owner = project
                .user_id
                .as_deref()
                .map(|owner| owner == user_id)
                .unwrap_or(true);
            if same_owner {
                if let Err(err) =
                    chatos_memory_mappings::sync_memory_project(&SyncMemoryProjectRequestDto {
                        user_id: Some(user_id.to_string()),
                        project_id: Some(project.id.clone()),
                        name: Some(project.name.clone()),
                        root_path: Some(project.root_path.clone()),
                        description: project.description.clone(),
                        status: Some("active".to_string()),
                        is_virtual: Some(false),
                    })
                    .await
                {
                    warn!(
                        project_id = project.id.as_str(),
                        error = err.as_str(),
                        "sync memory project failed while creating session"
                    );
                }
            }
        }
    } else if let Err(err) =
        chatos_memory_mappings::sync_memory_project(&SyncMemoryProjectRequestDto {
            user_id: Some(user_id.to_string()),
            project_id: Some(PUBLIC_PROJECT_ID.to_string()),
            name: Some("未指定项目".to_string()),
            root_path: None,
            description: None,
            status: Some("active".to_string()),
            is_virtual: Some(true),
        })
        .await
    {
        warn!(
            error = err.as_str(),
            "sync virtual memory project failed while creating session"
        );
    }
}
