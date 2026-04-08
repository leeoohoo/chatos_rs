use axum::{
    routing::{get, post},
    Router,
};

mod contracts;
mod helpers;
mod mutate_handlers;
mod query_handlers;
mod read_mode;
mod response;
mod roots;
mod search;

use self::mutate_handlers::{create_dir, create_file, delete_entry, move_entry};
use self::query_handlers::{download_entry, list_dirs, list_entries, read_file, search_entries};

pub fn router() -> Router {
    Router::new()
        .route("/api/fs/list", get(list_dirs))
        .route("/api/fs/entries", get(list_entries))
        .route("/api/fs/search", get(search_entries))
        .route("/api/fs/mkdir", post(create_dir))
        .route("/api/fs/touch", post(create_file))
        .route("/api/fs/delete", post(delete_entry))
        .route("/api/fs/move", post(move_entry))
        .route("/api/fs/download", get(download_entry))
        .route("/api/fs/read", get(read_file))
}
