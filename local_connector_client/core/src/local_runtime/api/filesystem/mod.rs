// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

mod download;
mod git_mutations;
mod mutations;
mod open;
mod queries;
mod search;

use axum::routing::{get, post};
use axum::Router;

use crate::LocalRuntime;

pub(super) fn router() -> Router<LocalRuntime> {
    Router::new()
        .route("/api/local/runtime/fs/entries", get(queries::list_entries))
        .route("/api/local/runtime/fs/read", get(queries::read_file))
        .route(
            "/api/local/runtime/fs/download",
            get(download::download_entry),
        )
        .route("/api/local/runtime/fs/search", get(search::search_entries))
        .route(
            "/api/local/runtime/fs/search-content",
            get(search::search_content),
        )
        .route(
            "/api/local/runtime/fs/mkdir",
            post(mutations::create_directory),
        )
        .route("/api/local/runtime/fs/touch", post(mutations::create_file))
        .route("/api/local/runtime/fs/write", post(mutations::write_file))
        .route(
            "/api/local/runtime/fs/delete",
            post(mutations::delete_entry),
        )
        .route("/api/local/runtime/fs/move", post(mutations::move_entry))
        .route(
            "/api/local/runtime/fs/gitignore",
            post(git_mutations::append_gitignore),
        )
        .route("/api/local/runtime/fs/open", post(open::open_path))
        .route(
            "/api/local/runtime/fs/discard-git-changes",
            post(git_mutations::discard_git_changes),
        )
}
