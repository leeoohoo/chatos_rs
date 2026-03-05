use axum::http::StatusCode;
use axum::{
    extract::{Path, Query},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::core::auth::AuthUser;
use crate::services::notepad::{
    CreateNoteParams, ListNotesParams, NotepadService, SearchNotesParams, UpdateNoteParams,
};

#[derive(Debug, Deserialize)]
struct DeleteFolderQuery {
    folder: Option<String>,
    recursive: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ListNotesQuery {
    folder: Option<String>,
    recursive: Option<bool>,
    tags: Option<String>,
    #[serde(rename = "match")]
    match_mode: Option<String>,
    query: Option<String>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchNotesQuery {
    query: Option<String>,
    folder: Option<String>,
    recursive: Option<bool>,
    tags: Option<String>,
    #[serde(rename = "match")]
    match_mode: Option<String>,
    include_content: Option<bool>,
    limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct CreateFolderRequest {
    folder: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RenameFolderRequest {
    from: Option<String>,
    to: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CreateNoteRequest {
    folder: Option<String>,
    title: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct UpdateNoteRequest {
    title: Option<String>,
    content: Option<String>,
    folder: Option<String>,
    tags: Option<Vec<String>>,
}

pub fn router() -> Router {
    Router::new()
        .route("/api/notepad/init", get(init_notepad))
        .route(
            "/api/notepad/folders",
            get(list_folders)
                .post(create_folder)
                .patch(rename_folder)
                .delete(delete_folder),
        )
        .route("/api/notepad/notes", get(list_notes).post(create_note))
        .route(
            "/api/notepad/notes/:note_id",
            get(get_note).patch(update_note).delete(delete_note_by_id),
        )
        .route("/api/notepad/tags", get(list_tags))
        .route("/api/notepad/search", get(search_notes))
}

async fn init_notepad(auth: AuthUser) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    match service.init().await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn list_folders(auth: AuthUser) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    match service.list_folders().await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn create_folder(
    auth: AuthUser,
    Json(req): Json<CreateFolderRequest>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let folder = req.folder.unwrap_or_default();
    match service.create_folder(folder.as_str()).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn rename_folder(
    auth: AuthUser,
    Json(req): Json<RenameFolderRequest>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let from = req.from.unwrap_or_default();
    let to = req.to.unwrap_or_default();
    match service.rename_folder(from.as_str(), to.as_str()).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn delete_folder(
    auth: AuthUser,
    Query(query): Query<DeleteFolderQuery>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let folder = query.folder.unwrap_or_default();
    let recursive = query.recursive.unwrap_or(false);
    match service.delete_folder(folder.as_str(), recursive).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn list_notes(
    auth: AuthUser,
    Query(query): Query<ListNotesQuery>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let params = ListNotesParams {
        folder: query.folder.unwrap_or_default(),
        recursive: query.recursive.unwrap_or(true),
        tags: parse_tags_csv(query.tags.as_deref()),
        match_any: query
            .match_mode
            .as_deref()
            .unwrap_or("all")
            .eq_ignore_ascii_case("any"),
        query: query.query.unwrap_or_default(),
        limit: query.limit.unwrap_or(200),
    };
    match service.list_notes(params).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn create_note(
    auth: AuthUser,
    Json(req): Json<CreateNoteRequest>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let params = CreateNoteParams {
        folder: req.folder.unwrap_or_default(),
        title: req.title.unwrap_or_default(),
        content: req.content.unwrap_or_default(),
        tags: req.tags.unwrap_or_default(),
    };
    match service.create_note(params).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn get_note(auth: AuthUser, Path(note_id): Path<String>) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    match service.get_note(note_id.as_str()).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn update_note(
    auth: AuthUser,
    Path(note_id): Path<String>,
    Json(req): Json<UpdateNoteRequest>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let params = UpdateNoteParams {
        id: note_id,
        title: req.title,
        content: req.content,
        folder: req.folder,
        tags: req.tags,
    };
    match service.update_note(params).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn delete_note_by_id(
    auth: AuthUser,
    Path(note_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    match service.delete_note(note_id.as_str()).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn list_tags(auth: AuthUser) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    match service.list_tags().await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

async fn search_notes(
    auth: AuthUser,
    Query(query): Query<SearchNotesQuery>,
) -> (StatusCode, Json<Value>) {
    let service = match resolve_service(&auth) {
        Ok(service) => service,
        Err(err) => return err,
    };
    let params = SearchNotesParams {
        query: query.query.unwrap_or_default(),
        folder: query.folder.unwrap_or_default(),
        recursive: query.recursive.unwrap_or(true),
        tags: parse_tags_csv(query.tags.as_deref()),
        match_any: query
            .match_mode
            .as_deref()
            .unwrap_or("all")
            .eq_ignore_ascii_case("any"),
        include_content: query.include_content.unwrap_or(true),
        limit: query.limit.unwrap_or(50),
    };
    match service.search_notes(params).await {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(err) => bad_request(err),
    }
}

fn resolve_service(auth: &AuthUser) -> Result<NotepadService, (StatusCode, Json<Value>)> {
    NotepadService::new(auth.user_id.as_str()).map_err(bad_request)
}

fn parse_tags_csv(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn bad_request(error: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "ok": false,
            "error": error
        })),
    )
}
