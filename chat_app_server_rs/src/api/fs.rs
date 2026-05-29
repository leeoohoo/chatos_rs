use axum::{
    routing::{get, post},
    Router,
};

mod contracts;
mod helpers;
mod mutate_handlers;
mod policy;
mod query_handlers;
mod read_mode;
mod response;
mod roots;
mod search;

use self::mutate_handlers::{
    append_gitignore_entry, create_dir, create_file, delete_entry, discard_git_changes, move_entry,
    open_path_externally,
    write_file,
};
use self::query_handlers::{
    download_entry, list_dirs, list_entries, read_file, search_content, search_entries,
};

pub fn router() -> Router {
    Router::new()
        .route("/api/fs/list", get(list_dirs))
        .route("/api/fs/entries", get(list_entries))
        .route("/api/fs/search", get(search_entries))
        .route("/api/fs/search-content", get(search_content))
        .route("/api/fs/mkdir", post(create_dir))
        .route("/api/fs/touch", post(create_file))
        .route("/api/fs/delete", post(delete_entry))
        .route("/api/fs/move", post(move_entry))
        .route("/api/fs/write", post(write_file))
        .route("/api/fs/gitignore", post(append_gitignore_entry))
        .route("/api/fs/open", post(open_path_externally))
        .route("/api/fs/discard-git-changes", post(discard_git_changes))
        .route("/api/fs/download", get(download_entry))
        .route("/api/fs/read", get(read_file))
}
