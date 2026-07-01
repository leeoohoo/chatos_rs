// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ToolingNotepadQuery {
    user_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(in crate::api) struct ToolingNotepadNotesQuery {
    user_id: Option<String>,
    folder: Option<String>,
    tags: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
    match_any: Option<bool>,
    recursive: Option<bool>,
}

pub(in crate::api) async fn list_notepad_folders(
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .list_notepad_folders(query.user_id.as_deref())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

pub(in crate::api) async fn list_notepad_tags(
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .list_notepad_tags(query.user_id.as_deref())
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

pub(in crate::api) async fn list_notepad_notes(
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadNotesQuery>,
) -> Result<Json<Value>, ApiError> {
    let tags = query
        .tags
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    let response = state
        .tooling_state_service
        .list_notepad_notes(
            query.user_id.as_deref(),
            query.folder,
            tags,
            query.query,
            query.limit,
            query.match_any.unwrap_or(false),
            query.recursive.unwrap_or(true),
        )
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}

pub(in crate::api) async fn read_notepad_note(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<ToolingNotepadQuery>,
) -> Result<Json<Value>, ApiError> {
    let response = state
        .tooling_state_service
        .read_notepad_note(query.user_id.as_deref(), &id)
        .await
        .map_err(ApiError::bad_request)?;
    Ok(Json(response))
}
