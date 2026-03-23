use crate::db::Db;
use crate::models::Session;

use super::{normalize_optional_text, now_rfc3339};

mod read_ops;
mod write_ops;

pub use self::read_ops::{
    get_active_session_by_contact_project, get_session_by_id, list_active_user_ids, list_sessions,
    list_sessions_by_agent,
};
pub use self::write_ops::{
    archive_sessions_by_contact, create_session, delete_session, update_session,
    upsert_session_sync,
};

pub(super) fn collection(db: &Db) -> mongodb::Collection<Session> {
    db.collection::<Session>("sessions")
}
